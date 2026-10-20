//! Archive extraction utilities for the infs toolchain.
//!
//! This module provides functionality for extracting ZIP and tar.gz archives
//! used during toolchain and self-update installations.
//!
//! An archive's checksum comes from the same release manifest as its URL, so
//! it proves the download is the one the server meant, not that the server is
//! honest. Extraction therefore writes only regular files and directories, at
//! paths inside the destination, and checks every member of an archive before
//! it writes any: an archive holding a link or a path that leaves the
//! destination is refused whole. A tar.gz wrapped in a ZIP is checked only once
//! the ZIP is written, so [`extract_archive`] removes a destination it created
//! when extraction fails, and a refused archive leaves nothing behind.

use anyhow::{Context, Result};
use flate2::read::GzDecoder;
use std::path::{Component, Path, PathBuf};
use tar::Archive;

/// Extracts a ZIP archive to the destination directory.
///
/// Creates the destination directory if it does not exist.
/// If all archive entries share a common root folder, it is automatically
/// stripped during extraction (e.g., `toolchain-0.2.0/infc` becomes `infc`).
///
/// # Errors
///
/// Returns an error if:
/// - The archive cannot be opened
/// - The archive is not a valid ZIP file
/// - A member is a symbolic link, or its path leaves the destination;
///   nothing is written then
/// - The archive wraps a tar.gz that [`extract_tar_gz`] refuses; the ZIP's
///   own members are written by then
/// - Directory or file creation fails
/// - File extraction fails
///
/// # Example
///
/// ```ignore
/// use crate::toolchain::extract_zip;
/// extract_zip(Path::new("archive.zip"), Path::new("output_dir"))?;
/// ```
pub fn extract_zip(archive_path: &Path, dest_dir: &Path) -> Result<()> {
    let file = std::fs::File::open(archive_path)
        .with_context(|| format!("Failed to open archive: {}", archive_path.display()))?;

    let mut archive = zip::ZipArchive::new(file)
        .with_context(|| format!("Failed to read ZIP archive: {}", archive_path.display()))?;

    let strip_prefix = find_common_root_folder(&mut archive)?;

    std::fs::create_dir_all(dest_dir)
        .with_context(|| format!("Failed to create directory: {}", dest_dir.display()))?;

    for i in 0..archive.len() {
        let mut entry = archive
            .by_index(i)
            .with_context(|| format!("Failed to read archive entry {i}"))?;

        let entry_path = zip_member_path(&entry, i)?;

        let relative_path = if let Some(ref prefix) = strip_prefix {
            match entry_path.strip_prefix(prefix) {
                Ok(p) if p.as_os_str().is_empty() => continue,
                Ok(p) => p.to_path_buf(),
                Err(_) => entry_path.clone(),
            }
        } else {
            entry_path.clone()
        };

        let output_path = dest_dir.join(&relative_path);

        if entry.is_dir() {
            std::fs::create_dir_all(&output_path).with_context(|| {
                format!("Failed to create directory: {}", output_path.display())
            })?;
        } else {
            if let Some(parent) = output_path.parent() {
                std::fs::create_dir_all(parent)
                    .with_context(|| format!("Failed to create directory: {}", parent.display()))?;
            }

            let mut outfile = std::fs::File::create(&output_path)
                .with_context(|| format!("Failed to create file: {}", output_path.display()))?;

            std::io::copy(&mut entry, &mut outfile)
                .with_context(|| format!("Failed to extract: {}", output_path.display()))?;
        }
    }

    // After extraction, check for nested tar.gz archive
    extract_nested_tar_gz_if_present(dest_dir)?;

    Ok(())
}

/// Extracts nested tar.gz archive if the extraction result is a single tar.gz file.
///
/// This handles GitHub releases that wrap tar.gz archives in ZIP files.
/// If `dest_dir` contains only a `.tar.gz` file (plus optional `.sha256`),
/// extracts the tar.gz and removes the archive files.
fn extract_nested_tar_gz_if_present(dest_dir: &Path) -> Result<()> {
    let entries: Vec<_> = std::fs::read_dir(dest_dir)
        .with_context(|| format!("Failed to read directory: {}", dest_dir.display()))?
        .filter_map(Result::ok)
        .collect();

    // Find tar.gz file(s)
    let tar_gz_files: Vec<_> = entries
        .iter()
        .filter(|e| {
            e.path()
                .file_name()
                .and_then(|n| n.to_str())
                .is_some_and(|n| n.ends_with(".tar.gz"))
        })
        .collect();

    // Only proceed if there's exactly one tar.gz file
    if tar_gz_files.len() != 1 {
        return Ok(());
    }

    let tar_gz_path = tar_gz_files[0].path();

    // Verify only tar.gz and optional sha256/metadata files exist (no other extracted content)
    let non_meta_files: Vec<_> = entries
        .iter()
        .filter(|e| {
            let name = e.file_name();
            let name_str = name.to_string_lossy();
            !name_str.ends_with(".sha256") && !name_str.starts_with('.')
        })
        .collect();

    if non_meta_files.len() != 1 {
        // Other files exist, not a nested archive scenario
        return Ok(());
    }

    // Extract the nested tar.gz
    extract_tar_gz(&tar_gz_path, dest_dir)?;

    // Clean up the archive files
    std::fs::remove_file(&tar_gz_path).ok();
    let sha256_path = tar_gz_path.with_extension("gz.sha256");
    if sha256_path.exists() {
        std::fs::remove_file(&sha256_path).ok();
    }

    Ok(())
}

/// Extracts an archive (ZIP or tar.gz) to the destination directory.
///
/// Automatically detects the archive format based on the file extension
/// and calls the appropriate extractor.
///
/// When extraction fails, a destination this call created is removed, so a
/// refused or broken archive leaves no half-written directory behind: an
/// install takes an existing toolchain directory to mean the version is
/// installed. A destination that existed before is left as it is.
///
/// # Errors
///
/// Returns an error if:
/// - The archive format cannot be determined
/// - The archive cannot be opened or extracted, or holds a member that is
///   unsafe to extract
/// - Directory or file creation fails
///
/// # Example
///
/// ```ignore
/// use crate::toolchain::extract_archive;
/// extract_archive(Path::new("archive.tar.gz"), Path::new("output_dir"))?;
/// extract_archive(Path::new("archive.zip"), Path::new("output_dir"))?;
/// ```
pub fn extract_archive(archive_path: &Path, dest_dir: &Path) -> Result<()> {
    let dest_existed = std::fs::symlink_metadata(dest_dir).is_ok();
    let path_str = archive_path.to_string_lossy();
    let result = if path_str.ends_with(".tar.gz") || path_str.ends_with(".tgz") {
        extract_tar_gz(archive_path, dest_dir)
    } else {
        extract_zip(archive_path, dest_dir)
    };
    if result.is_err() && !dest_existed {
        std::fs::remove_dir_all(dest_dir).ok();
    }
    result
}

/// Extracts a tar.gz archive to the destination directory.
///
/// Creates the destination directory if it does not exist.
/// If all archive entries share a common root folder, it is automatically
/// stripped during extraction (e.g., `toolchain-0.2.0/infc` becomes `infc`).
///
/// # Errors
///
/// Returns an error if:
/// - The archive cannot be opened
/// - The archive is not a valid tar.gz file
/// - A member is not a regular file or a directory, or its path leaves the
///   destination; nothing is written then
/// - Directory or file creation fails
/// - File extraction fails
///
/// # Example
///
/// ```ignore
/// use crate::toolchain::extract_tar_gz;
/// extract_tar_gz(Path::new("archive.tar.gz"), Path::new("output_dir"))?;
/// ```
pub fn extract_tar_gz(archive_path: &Path, dest_dir: &Path) -> Result<()> {
    let strip_prefix = find_common_root_folder_tar(archive_path)?;

    std::fs::create_dir_all(dest_dir)
        .with_context(|| format!("Failed to create directory: {}", dest_dir.display()))?;

    let file = std::fs::File::open(archive_path)
        .with_context(|| format!("Failed to open archive: {}", archive_path.display()))?;
    let decoder = GzDecoder::new(file);
    let mut archive = Archive::new(decoder);

    for entry in archive
        .entries()
        .with_context(|| format!("Failed to read tar entries: {}", archive_path.display()))?
    {
        let mut entry = entry
            .with_context(|| format!("Failed to read tar entry: {}", archive_path.display()))?;

        let Some(entry_path) = tar_member_path(&entry)? else {
            continue;
        };

        let relative_path = if let Some(ref prefix) = strip_prefix {
            match entry_path.strip_prefix(prefix) {
                Ok(p) if p.as_os_str().is_empty() => continue,
                Ok(p) => p.to_path_buf(),
                Err(_) => entry_path.clone(),
            }
        } else {
            entry_path.clone()
        };

        let output_path = dest_dir.join(&relative_path);

        if entry.header().entry_type().is_dir() {
            std::fs::create_dir_all(&output_path).with_context(|| {
                format!("Failed to create directory: {}", output_path.display())
            })?;
        } else {
            if let Some(parent) = output_path.parent() {
                std::fs::create_dir_all(parent)
                    .with_context(|| format!("Failed to create directory: {}", parent.display()))?;
            }

            entry
                .unpack(&output_path)
                .with_context(|| format!("Failed to extract: {}", output_path.display()))?;
        }
    }

    Ok(())
}

/// Checks every member of a tar.gz archive and finds the common root folder
/// they share.
///
/// Refuses the archive, before anything is extracted, if a member is unsafe to
/// extract (see [`tar_member_path`]); this is the pass that reads every member
/// ahead of the one that writes them.
///
/// Returns `Some(prefix)` if all entries start with the same folder name
/// AND there are nested entries (paths with more than one component).
/// Otherwise returns `None`.
///
/// This prevents flat files at the archive root from being incorrectly
/// treated as "common root folders" and stripped away.
fn find_common_root_folder_tar(archive_path: &Path) -> Result<Option<PathBuf>> {
    let file = std::fs::File::open(archive_path)
        .with_context(|| format!("Failed to open archive: {}", archive_path.display()))?;

    let decoder = GzDecoder::new(file);
    let mut archive = Archive::new(decoder);

    let mut common_root: Option<PathBuf> = None;
    let mut roots_differ = false;
    let mut has_nested_entries = false;

    for entry in archive
        .entries()
        .with_context(|| format!("Failed to read tar entries: {}", archive_path.display()))?
    {
        let entry = entry
            .with_context(|| format!("Failed to read tar entry: {}", archive_path.display()))?;

        let Some(path) = tar_member_path(&entry)? else {
            continue;
        };

        // Track if we have any entries with nested paths (depth > 1)
        if path.components().count() > 1 {
            has_nested_entries = true;
        }

        let Some(first_component) = path.components().next() else {
            continue;
        };
        let root = PathBuf::from(first_component.as_os_str());

        // Keep reading after the roots differ: every member must be checked.
        match &common_root {
            None => common_root = Some(root),
            Some(existing) if existing != &root => roots_differ = true,
            Some(_) => {}
        }
    }

    // Only strip common root if there are nested entries
    // (root is actually a containing folder, not just a flat file)
    if has_nested_entries && !roots_differ {
        Ok(common_root)
    } else {
        Ok(None)
    }
}

/// Checks every member of a ZIP archive and finds the common root folder they
/// share.
///
/// Refuses the archive, before anything is extracted, if a member is unsafe to
/// extract (see [`zip_member_path`]).
///
/// Returns `Some(prefix)` if all entries start with the same folder name
/// AND there are nested entries (paths with more than one component).
/// Otherwise returns `None`.
///
/// This prevents flat files at the archive root from being incorrectly
/// treated as "common root folders" and stripped away.
fn find_common_root_folder<R: std::io::Read + std::io::Seek>(
    archive: &mut zip::ZipArchive<R>,
) -> Result<Option<PathBuf>> {
    let mut common_root: Option<PathBuf> = None;
    let mut roots_differ = false;
    let mut has_nested_entries = false;

    for i in 0..archive.len() {
        let entry = archive
            .by_index(i)
            .with_context(|| format!("Failed to read archive entry {i}"))?;
        let path = zip_member_path(&entry, i)?;

        // Track if we have any entries with nested paths (depth > 1)
        if path.components().count() > 1 {
            has_nested_entries = true;
        }

        // Keep reading after the roots differ: every member must be checked.
        let Some(first_component) = path.components().next() else {
            roots_differ = true;
            continue;
        };
        let root = PathBuf::from(first_component.as_os_str());

        match &common_root {
            None => common_root = Some(root),
            Some(existing) if existing != &root => roots_differ = true,
            Some(_) => {}
        }
    }

    // Only strip common root if there are nested entries
    // (root is actually a containing folder, not just a flat file)
    if has_nested_entries && !roots_differ {
        Ok(common_root)
    } else {
        Ok(None)
    }
}

/// The path to extract a tar member at, or `None` for a header that carries
/// only metadata.
///
/// Only regular files and directories are extracted. A symbolic or a hard
/// link may name any file on the system: a later member written through it
/// lands outside the destination, and so does whatever follows it afterwards,
/// such as setting `infc` executable. A device node or a FIFO has no place in
/// a release archive. Each is refused, and so is a path that leaves the
/// destination (see [`ensure_member_path_stays_inside`]).
///
/// The `tar` crate folds a GNU long name or a pax extended header into the
/// member it describes, but yields a pax global header, and an extension
/// header in a pre-POSIX header, as a member of its own, and writes nothing
/// for it, so it is skipped.
fn tar_member_path<R: std::io::Read>(entry: &tar::Entry<'_, R>) -> Result<Option<PathBuf>> {
    let kind = entry.header().entry_type();
    if kind.is_pax_global_extensions()
        || kind.is_pax_local_extensions()
        || kind.is_gnu_longname()
        || kind.is_gnu_longlink()
    {
        return Ok(None);
    }

    let path = entry
        .path()
        .with_context(|| "Failed to get entry path")?
        .into_owned();

    if !kind.is_file() && !kind.is_dir() {
        anyhow::bail!(
            "Refusing to extract `{}`: it is {}, and an archive may hold only regular files \
             and directories",
            path.display(),
            describe_entry_type(kind)
        );
    }
    ensure_member_path_stays_inside(&path)?;

    Ok(Some(path))
}

/// Names a tar member type that is not extracted, for the refusal.
fn describe_entry_type(kind: tar::EntryType) -> String {
    match kind {
        tar::EntryType::Symlink => "a symbolic link".to_string(),
        tar::EntryType::Link => "a hard link".to_string(),
        tar::EntryType::Char => "a character device".to_string(),
        tar::EntryType::Block => "a block device".to_string(),
        tar::EntryType::Fifo => "a FIFO".to_string(),
        tar::EntryType::Continuous => "a contiguous file".to_string(),
        tar::EntryType::GNUSparse => "a sparse file".to_string(),
        other => format!("a member of type `{}`", other.as_byte().escape_ascii()),
    }
}

/// The path to extract member `index` of a ZIP archive at.
///
/// `enclosed_name` drops a leading root and resolves `..` within the name, but
/// its result still needs the check after it. On Windows a later component
/// such as `C:x` (`pkg/C:x`) replaces the path it is pushed onto, and before
/// zip 9 a name behind a verbatim prefix (`\\?\C:\../x`) came back as `../x`.
/// A symbolic link is refused, as in [`tar_member_path`].
/// This extractor writes every other member as a plain file, so a link would
/// otherwise become a file holding its target's path, which is never what the
/// archive meant.
fn zip_member_path<R: std::io::Read>(
    entry: &zip::read::ZipFile<'_, R>,
    index: usize,
) -> Result<PathBuf> {
    let path = entry
        .enclosed_name()
        .with_context(|| format!("Invalid entry path in archive: entry {index}"))?;

    if entry.is_symlink() {
        anyhow::bail!(
            "Refusing to extract `{}`: it is a symbolic link, and an archive may hold only \
             regular files and directories",
            path.display()
        );
    }
    ensure_member_path_stays_inside(&path)?;

    Ok(path)
}

/// Refuses a member path that could reach outside the directory it is joined
/// to: one with a `..` component, a root or a drive prefix. Only plain names
/// and `.` stay inside it.
///
/// `is_absolute` alone is not enough on Windows, where `\Windows\x` and `C:x`
/// are relative and yet `dest.join` of either leaves `dest`.
fn ensure_member_path_stays_inside(path: &Path) -> Result<()> {
    if path
        .components()
        .any(|c| !matches!(c, Component::Normal(_) | Component::CurDir))
    {
        anyhow::bail!(
            "Refusing to extract path with parent directory or absolute reference: {}",
            path.display()
        );
    }
    Ok(())
}

/// Sets executable permissions on the managed binaries within a toolchain
/// directory (Unix only).
///
/// Always targets the `infc` binary at the toolchain root, plus each optional
/// managed binary (e.g. the bundled `inference-lsp` server) when present.
/// Toolchains that predate the bundling lack the optional binaries, so their
/// absence is silently skipped.
///
/// On Windows, this function does nothing since executable permissions are not
/// managed the same way.
///
/// # Arguments
///
/// * `dir` - The toolchain directory containing the managed binaries at root.
///
/// # Errors
///
/// Returns an error if file metadata cannot be retrieved or permissions cannot be set.
#[cfg(unix)]
pub fn set_executable_permissions(dir: &Path) -> Result<()> {
    use std::os::unix::fs::PermissionsExt;

    let infc_path = dir.join("infc");
    if infc_path.is_file() {
        let mut perms = std::fs::metadata(&infc_path)
            .with_context(|| format!("Failed to get metadata: {}", infc_path.display()))?
            .permissions();
        perms.set_mode(0o755);
        std::fs::set_permissions(&infc_path, perms)
            .with_context(|| format!("Failed to set permissions: {}", infc_path.display()))?;
    }

    for optional in crate::toolchain::ToolchainPaths::OPTIONAL_MANAGED_BINARIES {
        let path = dir.join(optional);
        if path.is_file() {
            set_executable_file(&path)?;
        }
    }

    Ok(())
}

/// Sets executable permissions (no-op on Windows).
#[cfg(windows)]
#[allow(clippy::unnecessary_wraps)]
pub fn set_executable_permissions(_dir: &Path) -> Result<()> {
    Ok(())
}

/// Marks a single file as executable (`0o755`) on Unix; a no-op on Windows,
/// where executability is determined by extension rather than a permission bit.
///
/// Unlike [`set_executable_permissions`], which targets the `infc` binary at a
/// toolchain root by name, this operates on an arbitrary path — used when
/// staging individual managed-tool files whose names are not known in advance.
///
/// # Errors
///
/// Returns an error if file metadata cannot be read or permissions cannot be set.
#[cfg(unix)]
pub fn set_executable_file(path: &Path) -> Result<()> {
    use std::os::unix::fs::PermissionsExt;

    let mut perms = std::fs::metadata(path)
        .with_context(|| format!("Failed to get metadata: {}", path.display()))?
        .permissions();
    perms.set_mode(0o755);
    std::fs::set_permissions(path, perms)
        .with_context(|| format!("Failed to set permissions: {}", path.display()))?;
    Ok(())
}

/// Marks a file as executable (no-op on Windows).
#[cfg(windows)]
#[allow(clippy::unnecessary_wraps)]
pub fn set_executable_file(_path: &Path) -> Result<()> {
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::testing::{TreeEntry, member_path, repository_licenses, tree_entries};
    use flate2::Compression;
    use flate2::write::GzEncoder;
    use std::io::Write;
    use tar::Builder;

    /// Creates a temporary test directory.
    ///
    /// The returned `TempDir` owns the directory and deletes it on drop, so
    /// callers must keep it bound for the whole test. The name comes from the
    /// operating system's exclusive-create loop, so it is unique against both
    /// parallel test threads and concurrent test processes.
    fn temp_test_dir() -> assert_fs::TempDir {
        assert_fs::TempDir::new().expect("Should create temp dir")
    }

    /// Creates a tar.gz archive with a single file nested under a root folder.
    fn create_tar_gz_with_root(archive_path: &Path, root_name: &str) {
        let file = std::fs::File::create(archive_path).expect("Should create file");
        let encoder = GzEncoder::new(file, Compression::default());
        let mut builder = Builder::new(encoder);

        let mut header = tar::Header::new_gnu();
        header.set_size(14);
        header.set_mode(0o755);
        header.set_cksum();
        builder
            .append_data(
                &mut header,
                format!("{root_name}/infc"),
                b"binary content".as_slice(),
            )
            .expect("Should append file");

        builder.finish().expect("Should finish");
    }

    /// Creates a tar.gz archive with a single file at the root level (no common folder).
    fn create_tar_gz_without_root(archive_path: &Path) {
        let file = std::fs::File::create(archive_path).expect("Should create file");
        let encoder = GzEncoder::new(file, Compression::default());
        let mut builder = Builder::new(encoder);

        let mut header = tar::Header::new_gnu();
        header.set_size(14);
        header.set_mode(0o755);
        header.set_cksum();
        builder
            .append_data(&mut header, "infc", b"binary content".as_slice())
            .expect("Should append file");

        builder.finish().expect("Should finish");
    }

    #[test]
    fn extract_tar_gz_strips_common_root_folder() {
        let temp_dir = temp_test_dir();
        let archive_path = temp_dir.path().join("test.tar.gz");
        let dest_dir = temp_dir.path().join("output");

        create_tar_gz_with_root(&archive_path, "root-folder");

        extract_tar_gz(&archive_path, &dest_dir).expect("Should extract");

        assert!(dest_dir.join("infc").exists());
        assert!(!dest_dir.join("root-folder").exists());
    }

    #[test]
    fn extract_tar_gz_preserves_structure_without_common_root() {
        let temp_dir = temp_test_dir();
        let archive_path = temp_dir.path().join("test.tar.gz");
        let dest_dir = temp_dir.path().join("output");

        create_tar_gz_without_root(&archive_path);

        extract_tar_gz(&archive_path, &dest_dir).expect("Should extract");

        assert!(dest_dir.join("infc").exists());
    }

    #[test]
    fn extract_archive_selects_tar_gz_for_tar_gz_extension() {
        let temp_dir = temp_test_dir();
        let archive_path = temp_dir.path().join("test.tar.gz");
        let dest_dir = temp_dir.path().join("output");

        create_tar_gz_without_root(&archive_path);

        extract_archive(&archive_path, &dest_dir).expect("Should extract");

        assert!(dest_dir.join("infc").exists());
    }

    #[test]
    fn extract_archive_selects_tar_gz_for_tgz_extension() {
        let temp_dir = temp_test_dir();
        let archive_path = temp_dir.path().join("test.tgz");
        let dest_dir = temp_dir.path().join("output");

        create_tar_gz_without_root(&archive_path);

        extract_archive(&archive_path, &dest_dir).expect("Should extract");

        assert!(dest_dir.join("infc").exists());
    }

    #[test]
    fn extract_archive_selects_zip_for_zip_extension() {
        let temp_dir = temp_test_dir();
        let archive_path = temp_dir.path().join("test.zip");
        let dest_dir = temp_dir.path().join("output");

        {
            let file = std::fs::File::create(&archive_path).expect("Should create file");
            let mut zip = zip::ZipWriter::new(file);

            let options = zip::write::SimpleFileOptions::default();
            zip.start_file("infc", options)
                .expect("Should start file");
            zip.write_all(b"binary content").expect("Should write");

            zip.finish().expect("Should finish");
        }

        extract_archive(&archive_path, &dest_dir).expect("Should extract");

        assert!(dest_dir.join("infc").exists());
    }

    #[test]
    fn extract_zip_strips_common_root_folder() {
        let temp_dir = temp_test_dir();
        let archive_path = temp_dir.path().join("test.zip");
        let dest_dir = temp_dir.path().join("output");

        // Create a zip with a root folder
        {
            let file = std::fs::File::create(&archive_path).expect("Should create file");
            let mut zip = zip::ZipWriter::new(file);

            let options = zip::write::SimpleFileOptions::default();
            zip.start_file("root-folder/infc", options)
                .expect("Should start file");
            zip.write_all(b"binary content").expect("Should write");

            zip.finish().expect("Should finish");
        }

        extract_zip(&archive_path, &dest_dir).expect("Should extract");

        // Verify root folder was stripped
        assert!(dest_dir.join("infc").exists());
        assert!(!dest_dir.join("root-folder").exists());
    }

    #[test]
    fn extract_zip_preserves_structure_without_common_root() {
        let temp_dir = temp_test_dir();
        let archive_path = temp_dir.path().join("test.zip");
        let dest_dir = temp_dir.path().join("output");

        // Create a zip without a common root folder
        {
            let file = std::fs::File::create(&archive_path).expect("Should create file");
            let mut zip = zip::ZipWriter::new(file);

            let options = zip::write::SimpleFileOptions::default();
            zip.start_file("infc", options)
                .expect("Should start file");
            zip.write_all(b"binary content").expect("Should write");

            zip.finish().expect("Should finish");
        }

        extract_zip(&archive_path, &dest_dir).expect("Should extract");

        // Verify structure is preserved
        assert!(dest_dir.join("infc").exists());
    }

    /// Creates a flat tar.gz archive with a single file at root.
    fn create_tar_gz_flat_single_file(archive_path: &Path, filename: &str) {
        let file = std::fs::File::create(archive_path).expect("Should create file");
        let encoder = GzEncoder::new(file, Compression::default());
        let mut builder = Builder::new(encoder);

        let mut header = tar::Header::new_gnu();
        header.set_size(14);
        header.set_mode(0o755);
        header.set_cksum();
        builder
            .append_data(&mut header, filename, b"binary content".as_slice())
            .expect("Should append file");

        builder.finish().expect("Should finish");
    }

    /// Creates a flat tar.gz archive with multiple files at root.
    fn create_tar_gz_flat_multiple_files(archive_path: &Path) {
        let file = std::fs::File::create(archive_path).expect("Should create file");
        let encoder = GzEncoder::new(file, Compression::default());
        let mut builder = Builder::new(encoder);

        let mut header = tar::Header::new_gnu();
        header.set_size(14);
        header.set_mode(0o755);
        header.set_cksum();
        builder
            .append_data(&mut header, "infs", b"binary content".as_slice())
            .expect("Should append file");

        let mut header = tar::Header::new_gnu();
        header.set_size(11);
        header.set_mode(0o644);
        header.set_cksum();
        builder
            .append_data(&mut header, "README.md", b"readme text".as_slice())
            .expect("Should append file");

        builder.finish().expect("Should finish");
    }

    #[test]
    fn extract_tar_gz_flat_single_file_not_stripped() {
        let temp_dir = temp_test_dir();
        let archive_path = temp_dir.path().join("test.tar.gz");
        let dest_dir = temp_dir.path().join("output");

        // Archive contains just "infs" at root (like CI produces)
        create_tar_gz_flat_single_file(&archive_path, "infs");

        extract_tar_gz(&archive_path, &dest_dir).expect("Should extract");

        // File should be extracted as-is, not skipped
        assert!(dest_dir.join("infs").exists(), "infs should exist at root");
    }

    #[test]
    fn extract_tar_gz_flat_multiple_files_not_stripped() {
        let temp_dir = temp_test_dir();
        let archive_path = temp_dir.path().join("test.tar.gz");
        let dest_dir = temp_dir.path().join("output");

        create_tar_gz_flat_multiple_files(&archive_path);

        extract_tar_gz(&archive_path, &dest_dir).expect("Should extract");

        // Both files should exist at root
        assert!(dest_dir.join("infs").exists(), "infs should exist");
        assert!(
            dest_dir.join("README.md").exists(),
            "README.md should exist"
        );
    }

    #[test]
    fn extract_tar_gz_single_nested_file_stripped() {
        let temp_dir = temp_test_dir();
        let archive_path = temp_dir.path().join("test.tar.gz");
        let dest_dir = temp_dir.path().join("output");

        // Archive contains "root/infs" (nested under root folder)
        {
            let file = std::fs::File::create(&archive_path).expect("Should create file");
            let encoder = GzEncoder::new(file, Compression::default());
            let mut builder = Builder::new(encoder);

            let mut header = tar::Header::new_gnu();
            header.set_size(14);
            header.set_mode(0o755);
            header.set_cksum();
            builder
                .append_data(&mut header, "root/infs", b"binary content".as_slice())
                .expect("Should append file");

            builder.finish().expect("Should finish");
        }

        extract_tar_gz(&archive_path, &dest_dir).expect("Should extract");

        // Root should be stripped, file should be at dest_dir/infs
        assert!(
            dest_dir.join("infs").exists(),
            "infs should exist (root stripped)"
        );
        assert!(
            !dest_dir.join("root").exists(),
            "root folder should not exist"
        );
    }

    #[test]
    fn extract_zip_flat_single_file_not_stripped() {
        let temp_dir = temp_test_dir();
        let archive_path = temp_dir.path().join("test.zip");
        let dest_dir = temp_dir.path().join("output");

        // Create a zip with single file at root
        {
            let file = std::fs::File::create(&archive_path).expect("Should create file");
            let mut zip = zip::ZipWriter::new(file);

            let options = zip::write::SimpleFileOptions::default();
            zip.start_file("infs.exe", options)
                .expect("Should start file");
            zip.write_all(b"binary content").expect("Should write");

            zip.finish().expect("Should finish");
        }

        extract_zip(&archive_path, &dest_dir).expect("Should extract");

        // File should exist, not be skipped
        assert!(
            dest_dir.join("infs.exe").exists(),
            "infs.exe should exist at root"
        );
    }

    /// Creates a tar.gz archive with ./ prefix (like CI produces with -C dir .).
    fn create_tar_gz_with_dot_prefix(archive_path: &Path, filename: &str) {
        let file = std::fs::File::create(archive_path).expect("Should create file");
        let encoder = GzEncoder::new(file, Compression::default());
        let mut builder = Builder::new(encoder);

        // Add ./ directory entry (like tar -C dir . does)
        let mut header = tar::Header::new_gnu();
        header.set_entry_type(tar::EntryType::Directory);
        header.set_size(0);
        header.set_mode(0o755);
        header.set_cksum();
        builder
            .append_data(&mut header, "./", std::io::empty())
            .expect("Should append dir");

        // Add ./filename entry
        let mut header = tar::Header::new_gnu();
        header.set_size(14);
        header.set_mode(0o755);
        header.set_cksum();
        builder
            .append_data(
                &mut header,
                format!("./{filename}"),
                b"binary content".as_slice(),
            )
            .expect("Should append file");

        builder.finish().expect("Should finish");
    }

    #[test]
    fn extract_tar_gz_with_dot_prefix_not_stripped() {
        let temp_dir = temp_test_dir();
        let archive_path = temp_dir.path().join("test.tar.gz");
        let dest_dir = temp_dir.path().join("output");

        // Archive contains "./" and "./infs" (like CI produces with tar -C dir .)
        create_tar_gz_with_dot_prefix(&archive_path, "infs");

        extract_tar_gz(&archive_path, &dest_dir).expect("Should extract");

        // File should be extracted as "infs", not skipped
        assert!(
            dest_dir.join("infs").exists(),
            "infs should exist at root (dot prefix should be handled)"
        );
    }

    /// What CI stages for a release package: each of `binaries` at the root,
    /// then `cp -R licenses` — the repository's notices, read from the
    /// checkout, so the fixture follows the directory as it grows. A member's
    /// name is its archive member name relative to the package root.
    fn ci_package(binaries: &[&str]) -> Vec<TreeEntry> {
        let mut entries: Vec<TreeEntry> = binaries
            .iter()
            .map(|name| TreeEntry::File((*name).to_string(), format!("{name} binary").into()))
            .collect();
        entries.extend(tree_entries(&repository_licenses(), "licenses"));
        entries
    }

    /// Packs `entries` as `tar -czf ARCHIVE -C STAGING .` does on the Linux and
    /// macOS runners: a `./` member first and every other member under `./`,
    /// a directory as a member of its own, binaries executable and notices not.
    fn create_tar_gz_like_ci(archive_path: &Path, entries: &[TreeEntry]) {
        fn append_dir<W: Write>(builder: &mut Builder<W>, member: &str) {
            let mut header = tar::Header::new_gnu();
            header.set_entry_type(tar::EntryType::Directory);
            header.set_size(0);
            header.set_mode(0o755);
            header.set_cksum();
            builder
                .append_data(&mut header, member, std::io::empty())
                .expect("Should append dir");
        }

        let file = std::fs::File::create(archive_path).expect("Should create file");
        let encoder = GzEncoder::new(file, Compression::default());
        let mut builder = Builder::new(encoder);

        append_dir(&mut builder, "./");
        for entry in entries {
            match entry {
                TreeEntry::Dir(name) => append_dir(&mut builder, &format!("./{name}/")),
                TreeEntry::File(name, bytes) => {
                    let mut header = tar::Header::new_gnu();
                    header.set_size(u64::try_from(bytes.len()).expect("a member fits in a tar header"));
                    header.set_mode(if name.contains('/') { 0o644 } else { 0o755 });
                    header.set_cksum();
                    builder
                        .append_data(&mut header, format!("./{name}"), bytes.as_slice())
                        .expect("Should append file");
                }
            }
        }

        builder.finish().expect("Should finish");
    }

    /// Packs `entries` as `7z a -tzip ARCHIVE .\STAGING\*` does on the Windows
    /// runner: every member at its name under the staging directory, with no
    /// `./` and no root member, a directory as a member of its own.
    fn create_zip_like_ci(archive_path: &Path, entries: &[TreeEntry]) {
        let file = std::fs::File::create(archive_path).expect("Should create file");
        let mut zip = zip::ZipWriter::new(file);
        let options = zip::write::SimpleFileOptions::default();
        for entry in entries {
            match entry {
                TreeEntry::Dir(name) => zip
                    .add_directory(format!("{name}/"), options)
                    .expect("Should add directory"),
                TreeEntry::File(name, bytes) => {
                    zip.start_file(name.as_str(), options)
                        .expect("Should start file");
                    zip.write_all(bytes).expect("Should write");
                }
            }
        }
        zip.finish().expect("Should finish");
    }

    /// Asserts that `dest` holds exactly what `entries` staged, where an install
    /// looks for it: each binary at the root and each notice under `licenses/`,
    /// byte for byte, and nothing else at the root — no `.` directory, no
    /// stripped or doubled `licenses` level.
    fn assert_extracted_as_staged(dest: &Path, entries: &[TreeEntry]) {
        for entry in entries {
            match entry {
                TreeEntry::Dir(name) => assert!(
                    member_path(dest, name).is_dir(),
                    "{name}/ must be extracted as a directory"
                ),
                TreeEntry::File(name, bytes) => assert_eq!(
                    std::fs::read(member_path(dest, name))
                        .unwrap_or_else(|e| panic!("{name} must be extracted: {e}")),
                    *bytes,
                    "{name} must be extracted byte for byte"
                ),
            }
        }
        let mut root: Vec<String> = std::fs::read_dir(dest)
            .expect("Should read the destination")
            .map(|entry| {
                entry
                    .expect("Should read directory entry")
                    .file_name()
                    .to_string_lossy()
                    .into_owned()
            })
            .collect();
        root.sort();
        let mut staged: Vec<String> = entries
            .iter()
            .filter_map(|entry| match entry {
                TreeEntry::Dir(name) | TreeEntry::File(name, _) => {
                    (!name.contains('/')).then(|| name.clone())
                }
            })
            .collect();
        staged.sort();
        assert_eq!(root, staged, "the package root must be the destination's root");
    }

    /// The binaries of an `infc` release package: the compiler and every
    /// optional binary, at the names `infs install` looks for on `platform`.
    fn toolchain_binaries(platform: crate::toolchain::Platform) -> Vec<String> {
        std::iter::once(crate::toolchain::ToolchainPaths::MANAGED_BINARY)
            .chain(crate::toolchain::ToolchainPaths::OPTIONAL_MANAGED_BINARIES.iter().copied())
            .map(|name| format!("{name}{}", platform.executable_extension()))
            .collect()
    }

    /// Creates a tar.gz archive with CI's `infc` package layout: the compiler,
    /// the language server and the third-party notices.
    fn create_tar_gz_like_ci_infc_toolchain(archive_path: &Path) {
        let binaries = toolchain_binaries(crate::toolchain::Platform::LinuxX64);
        let binaries: Vec<&str> = binaries.iter().map(String::as_str).collect();
        create_tar_gz_like_ci(archive_path, &ci_package(&binaries));
    }

    /// An `infc` release archive packed as CI packs it on Linux and macOS
    /// extracts, through the format choice `infs install` makes, with the
    /// compiler and the language server at the toolchain root and the notices
    /// in `licenses/` beside them; setting the binaries executable then leaves
    /// the notices alone.
    ///
    /// Fails if the `./` root is kept as a directory, if a nested member is
    /// dropped or lands a level off, or if the notices stop surviving the
    /// round trip byte for byte.
    #[test]
    fn extract_tar_gz_ci_infc_toolchain_structure() {
        let temp_dir = temp_test_dir();
        let archive_path = temp_dir.path().join("infc-linux-x64.tar.gz");
        let dest_dir = temp_dir.path().join("toolchain");
        let binaries = toolchain_binaries(crate::toolchain::Platform::LinuxX64);
        let binaries: Vec<&str> = binaries.iter().map(String::as_str).collect();
        let entries = ci_package(&binaries);
        assert!(
            entries.iter().any(|entry| matches!(
                entry,
                TreeEntry::File(name, _) if name == "licenses/spacewasm/NOTICE"
            )),
            "the fixture must carry the nested notices it is about"
        );

        create_tar_gz_like_ci(&archive_path, &entries);
        extract_archive(&archive_path, &dest_dir).expect("Should extract");

        assert_extracted_as_staged(&dest_dir, &entries);
        set_executable_permissions(&dest_dir).expect("Should set permissions");
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let mode = |path: PathBuf| {
                std::fs::metadata(path)
                    .expect("Should get metadata")
                    .permissions()
                    .mode()
                    & 0o777
            };
            for binary in &binaries {
                assert_eq!(mode(dest_dir.join(binary)), 0o755, "{binary}");
            }
            assert_eq!(
                mode(member_path(&dest_dir, "licenses/spacewasm/NOTICE")),
                0o644,
                "a notice is not made executable"
            );
        }
    }

    /// An `infs` release archive packed as CI packs it on Linux and macOS
    /// extracts with `infs` at the root, where `infs self update` looks for
    /// it, and the notices in `licenses/` beside it.
    ///
    /// Fails if `licenses/` displaces the binary, or if a nested member is
    /// dropped or lands a level off.
    #[test]
    fn extract_tar_gz_ci_infs_package_structure() {
        let temp_dir = temp_test_dir();
        let archive_path = temp_dir.path().join("infs-macos-apple-silicon.tar.gz");
        let dest_dir = temp_dir.path().join("infs-temp");
        let entries = ci_package(&["infs"]);

        create_tar_gz_like_ci(&archive_path, &entries);
        extract_archive(&archive_path, &dest_dir).expect("Should extract");

        assert_extracted_as_staged(&dest_dir, &entries);
    }

    /// An `infc` release archive packed as CI packs it on Windows extracts,
    /// through the format choice `infs install` makes, with `infc.exe` and
    /// `inference-lsp.exe` at the toolchain root and the notices in
    /// `licenses/` beside them.
    ///
    /// The members have more than one root, so nothing may be stripped. Fails
    /// if a directory member is written as a file, if a nested member is
    /// dropped or lands a level off, or if a root is stripped.
    #[test]
    fn extract_zip_ci_infc_toolchain_structure() {
        let temp_dir = temp_test_dir();
        let archive_path = temp_dir.path().join("infc-windows-x64.zip");
        let dest_dir = temp_dir.path().join("toolchain");
        let binaries = toolchain_binaries(crate::toolchain::Platform::WindowsX64);
        let binaries: Vec<&str> = binaries.iter().map(String::as_str).collect();
        let entries = ci_package(&binaries);

        create_zip_like_ci(&archive_path, &entries);
        extract_archive(&archive_path, &dest_dir).expect("Should extract");

        assert_extracted_as_staged(&dest_dir, &entries);
    }

    /// An `infs` release archive packed as CI packs it on Windows extracts
    /// with `infs.exe` at the root, where `infs self update` looks for it, and
    /// the notices in `licenses/` beside it.
    ///
    /// Fails if a directory member is written as a file, if a nested member is
    /// dropped or lands a level off, or if a root is stripped.
    #[test]
    fn extract_zip_ci_infs_package_structure() {
        let temp_dir = temp_test_dir();
        let archive_path = temp_dir.path().join("infs-windows-x64.zip");
        let dest_dir = temp_dir.path().join("infs-temp");
        let entries = ci_package(&["infs.exe"]);

        create_zip_like_ci(&archive_path, &entries);
        extract_archive(&archive_path, &dest_dir).expect("Should extract");

        assert_extracted_as_staged(&dest_dir, &entries);
    }

    #[test]
    fn extract_zip_flat_multiple_files_not_stripped() {
        let temp_dir = temp_test_dir();
        let archive_path = temp_dir.path().join("test.zip");
        let dest_dir = temp_dir.path().join("output");

        // Create a zip with multiple files at root (no common folder)
        {
            let file = std::fs::File::create(&archive_path).expect("Should create file");
            let mut zip = zip::ZipWriter::new(file);

            let options = zip::write::SimpleFileOptions::default();
            zip.start_file("infs.exe", options)
                .expect("Should start file");
            zip.write_all(b"binary content").expect("Should write");

            zip.start_file("README.md", options)
                .expect("Should start file");
            zip.write_all(b"readme text").expect("Should write");

            zip.finish().expect("Should finish");
        }

        extract_zip(&archive_path, &dest_dir).expect("Should extract");

        // Both files should exist
        assert!(dest_dir.join("infs.exe").exists(), "infs.exe should exist");
        assert!(
            dest_dir.join("README.md").exists(),
            "README.md should exist"
        );
    }

    // Hostile archives
    //
    // Each test below extracts an archive a compromised or spoofed release
    // server could send into `dest/`, beside an `outside/` directory holding a
    // `secret` file, and requires the archive to be refused before anything is
    // written: no `dest/`, and `outside/` holding only the untouched secret.

    /// A temporary directory with an archive path, the destination an archive
    /// is extracted to, and `outside/` beside it, seeded with a `secret` file.
    struct HostileScene {
        temp: assert_fs::TempDir,
        archive: PathBuf,
        dest: PathBuf,
        outside: PathBuf,
    }

    const SECRET: &[u8] = b"private key";

    impl HostileScene {
        fn new(archive_name: &str) -> Self {
            let temp = temp_test_dir();
            let archive = temp.path().join(archive_name);
            let dest = temp.path().join("dest");
            let outside = temp.path().join("outside");
            std::fs::create_dir(&outside).expect("Should create outside/");
            std::fs::write(outside.join("secret"), SECRET).expect("Should write the secret");
            #[cfg(unix)]
            {
                use std::os::unix::fs::PermissionsExt;
                std::fs::set_permissions(
                    outside.join("secret"),
                    std::fs::Permissions::from_mode(0o600),
                )
                .expect("Should make the secret private");
            }
            Self {
                temp,
                archive,
                dest,
                outside,
            }
        }

        /// `outside/secret`, as a link target an archive can name.
        fn secret(&self) -> String {
            self.outside.join("secret").display().to_string()
        }

        /// `outside/`, as a link target an archive can name.
        fn outside(&self) -> String {
            self.outside.display().to_string()
        }

        /// Asserts that `result` refused the archive with a message holding
        /// each of `needles`, and that nothing was written: no destination,
        /// and `outside/` holding only the secret, with its bytes and its mode.
        /// Returns the message.
        fn assert_refused(&self, result: Result<()>, needles: &[&str]) -> String {
            let message = format!(
                "{:#}",
                result.expect_err("a hostile archive must be refused")
            );
            for needle in needles {
                assert!(
                    message.contains(needle),
                    "the refusal must mention `{needle}`: {message}"
                );
            }
            assert!(
                !self.dest.exists(),
                "a refused archive must not create the destination"
            );
            self.assert_outside_untouched();
            message
        }

        /// Asserts that `outside/` holds only the secret, with its bytes and
        /// its mode.
        fn assert_outside_untouched(&self) {
            let mut names: Vec<String> = std::fs::read_dir(&self.outside)
                .expect("Should read outside/")
                .map(|entry| {
                    entry
                        .expect("Should read directory entry")
                        .file_name()
                        .to_string_lossy()
                        .into_owned()
                })
                .collect();
            names.sort();
            assert_eq!(names, ["secret"], "nothing may be written into outside/");
            let secret = self.outside.join("secret");
            assert_eq!(
                std::fs::read(&secret).expect("Should read the secret"),
                SECRET,
                "the secret's bytes must not change"
            );
            #[cfg(unix)]
            {
                use std::os::unix::fs::PermissionsExt;
                let mode = std::fs::symlink_metadata(&secret)
                    .expect("Should stat the secret")
                    .permissions()
                    .mode();
                assert_eq!(mode & 0o777, 0o600, "the secret's mode must not change");
            }
            assert!(
                !self.temp.path().join("planted").exists(),
                "nothing may be written beside the destination"
            );
        }
    }

    /// A member of a hostile tar archive.
    enum HostileMember<'a> {
        /// A regular file holding the bytes.
        File(&'a str, &'a [u8]),
        /// A link of the type (symbolic or hard) to the target.
        Link(tar::EntryType, &'a str, &'a str),
        /// An empty member of another type, such as a device node or a FIFO.
        Special(tar::EntryType, &'a str),
        /// A regular file whose header names `harmless`, preceded by a GNU
        /// long-name header that names it the path.
        LongName(&'a str, &'a [u8]),
        /// A regular file whose header names `harmless`, preceded by a pax
        /// extended header whose `path` record names it the path.
        PaxPath(&'a str, &'a [u8]),
    }

    /// The tar header name field's length.
    const TAR_NAME_FIELD: usize = 100;

    /// Appends a GNU long-name (`L`) or long-link (`K`) header, or a pax
    /// extended header (`x`), carrying `data`, for the member after it.
    fn append_extension_header<W: Write>(builder: &mut Builder<W>, kind: u8, data: &[u8]) {
        let mut header = tar::Header::new_gnu();
        header.set_entry_type(tar::EntryType::new(kind));
        header.as_mut_bytes()[..13].copy_from_slice(b"././@LongLink");
        header.set_size(u64::try_from(data.len()).expect("a header fits"));
        header.set_mode(0o644);
        header.set_cksum();
        builder.append(&header, data).expect("Should append extension header");
    }

    /// A GNU long name or long link, as the bytes of its header: the name and
    /// a terminating NUL.
    fn gnu_long(name: &str) -> Vec<u8> {
        let mut bytes = name.as_bytes().to_vec();
        bytes.push(0);
        bytes
    }

    /// One pax record, `<length> <key>=<value>\n`, whose length counts itself.
    fn pax_record(key: &str, value: &str) -> Vec<u8> {
        let body = format!(" {key}={value}\n");
        let mut length = body.len();
        while format!("{length}{body}").len() != length {
            length += 1;
        }
        format!("{length}{body}").into_bytes()
    }

    /// Packs `members` into a tar.gz, writing each header's name and link
    /// target byte for byte: the `tar` crate's builder refuses a `..` or a
    /// rooted name, and an attacker's tool does not. A name or a link target
    /// too long for its header field goes in a GNU long-name or long-link
    /// header, so a long temporary directory still fits.
    fn create_hostile_tar_gz(archive_path: &Path, members: &[HostileMember<'_>]) {
        let file = std::fs::File::create(archive_path).expect("Should create file");
        let mut builder = Builder::new(GzEncoder::new(file, Compression::default()));
        for member in members {
            let mut header = tar::Header::new_gnu();
            let (name, data): (&str, &[u8]) = match *member {
                HostileMember::File(name, bytes) => {
                    header.set_entry_type(tar::EntryType::Regular);
                    (name, bytes)
                }
                HostileMember::Link(kind, name, target) => {
                    header.set_entry_type(kind);
                    if target.len() < TAR_NAME_FIELD {
                        header
                            .set_link_name_literal(target)
                            .expect("a short link target fits in a tar header");
                    } else {
                        append_extension_header(&mut builder, b'K', &gnu_long(target));
                    }
                    (name, b"".as_slice())
                }
                HostileMember::Special(kind, name) => {
                    header.set_entry_type(kind);
                    if kind == tar::EntryType::GNUSparse {
                        // An empty sparse map over an empty file, so that the
                        // `tar` crate reads the header and the check sees it.
                        header
                            .as_gnu_mut()
                            .expect("a GNU header")
                            .set_real_size(0);
                    }
                    (name, b"".as_slice())
                }
                HostileMember::LongName(name, bytes) => {
                    append_extension_header(&mut builder, b'L', &gnu_long(name));
                    ("harmless", bytes)
                }
                HostileMember::PaxPath(name, bytes) => {
                    append_extension_header(&mut builder, b'x', &pax_record("path", name));
                    ("harmless", bytes)
                }
            };
            let name = if name.len() < TAR_NAME_FIELD {
                name
            } else {
                append_extension_header(&mut builder, b'L', &gnu_long(name));
                "long"
            };
            header.as_mut_bytes()[..name.len()].copy_from_slice(name.as_bytes());
            header.set_size(u64::try_from(data.len()).expect("a member fits in a tar header"));
            header.set_mode(0o644);
            header.set_cksum();
            builder.append(&header, data).expect("Should append member");
        }
        builder
            .into_inner()
            .expect("Should finish tar")
            .finish()
            .expect("Should finish gzip");
    }

    /// A symbolic link followed by a member written through it is refused,
    /// whether the link names `outside/` by an absolute or a relative path,
    /// and whether or not the archive has a root folder to strip.
    ///
    /// Before the fix the link was created and the member landed in
    /// `outside/`.
    #[test]
    fn extract_tar_gz_refuses_a_member_written_through_a_symlink() {
        for (link, through, target) in [
            ("escape", "escape/planted", None),
            ("escape", "escape/planted", Some("../outside")),
            ("pkg/escape", "pkg/escape/planted", None),
            ("pkg/escape", "pkg/escape/planted", Some("../../outside")),
        ] {
            let scene = HostileScene::new("hostile.tar.gz");
            let outside = scene.outside();
            let target = target.unwrap_or(&outside);
            create_hostile_tar_gz(
                &scene.archive,
                &[
                    HostileMember::Link(tar::EntryType::Symlink, link, target),
                    HostileMember::File(through, b"planted"),
                ],
            );

            let result = extract_archive(&scene.archive, &scene.dest);

            scene.assert_refused(result, &[link, "symbolic link"]);
        }
    }

    /// A symbolic link with no member after it is refused too: left in place,
    /// it is followed by whatever reads the toolchain next, such as setting
    /// `infc` executable, which would make the secret world-readable.
    #[test]
    fn extract_tar_gz_refuses_a_lone_symlink() {
        let scene = HostileScene::new("hostile.tar.gz");
        let secret = scene.secret();
        create_hostile_tar_gz(
            &scene.archive,
            &[
                HostileMember::File("README.md", b"readme"),
                HostileMember::Link(tar::EntryType::Symlink, "infc", &secret),
            ],
        );

        let result = extract_archive(&scene.archive, &scene.dest);

        scene.assert_refused(result, &["infc", "symbolic link"]);
    }

    /// A hard link is refused, whether it names a file outside the archive or
    /// a member inside it. A hard link to the secret shares its inode, so
    /// setting `infc` executable would change the secret's mode.
    #[test]
    fn extract_tar_gz_refuses_a_hard_link() {
        let scene = HostileScene::new("hostile.tar.gz");
        let secret = scene.secret();
        create_hostile_tar_gz(
            &scene.archive,
            &[HostileMember::Link(tar::EntryType::Link, "infc", &secret)],
        );
        let result = extract_archive(&scene.archive, &scene.dest);
        scene.assert_refused(result, &["infc", "hard link"]);

        let scene = HostileScene::new("hostile.tar.gz");
        create_hostile_tar_gz(
            &scene.archive,
            &[
                HostileMember::File("infc", b"binary content"),
                HostileMember::Link(tar::EntryType::Link, "inference-lsp", "infc"),
            ],
        );
        let result = extract_archive(&scene.archive, &scene.dest);
        scene.assert_refused(result, &["inference-lsp", "hard link"]);
    }

    /// Every member type other than a regular file and a directory is
    /// refused and named, including one the `tar` crate does not know.
    #[test]
    fn extract_tar_gz_refuses_every_type_but_files_and_directories() {
        for (kind, described) in [
            (tar::EntryType::Char, "a character device"),
            (tar::EntryType::Block, "a block device"),
            (tar::EntryType::Fifo, "a FIFO"),
            (tar::EntryType::Continuous, "a contiguous file"),
            (tar::EntryType::GNUSparse, "a sparse file"),
            (tar::EntryType::new(b'Z'), "a member of type `Z`"),
        ] {
            let scene = HostileScene::new("hostile.tar.gz");
            create_hostile_tar_gz(
                &scene.archive,
                &[
                    HostileMember::File("infc", b"binary content"),
                    HostileMember::Special(kind, "device"),
                ],
            );

            let result = extract_archive(&scene.archive, &scene.dest);

            scene.assert_refused(result, &["`device`", described]);
        }
    }

    /// A member path with a `..` component or a root is refused, on every
    /// platform, together with the drive-relative and rooted forms Windows
    /// does not count as absolute.
    #[test]
    fn extract_tar_gz_refuses_member_paths_that_leave_the_destination() {
        // `None` stands for `outside/planted` spelled as an absolute path.
        #[allow(unused_mut)]
        let mut names = vec![
            Some("../planted"),
            Some("../outside/planted"),
            Some("pkg/../../planted"),
            Some("pkg/./../../outside/planted"),
            None,
        ];
        #[cfg(windows)]
        names.extend([Some(r"..\planted"), Some(r"\planted"), Some("C:planted")]);

        for name in names {
            let scene = HostileScene::new("hostile.tar.gz");
            let absolute = scene.outside.join("planted").display().to_string();
            let name = name.unwrap_or(&absolute);
            create_hostile_tar_gz(
                &scene.archive,
                &[
                    HostileMember::File("pkg/infc", b"binary content"),
                    HostileMember::File(name, b"planted"),
                ],
            );

            let result = extract_archive(&scene.archive, &scene.dest);

            scene.assert_refused(result, &["Refusing to extract path"]);
        }
    }

    /// The path checked is the one the member is extracted at: a `..` the
    /// header's own name field hides behind `harmless`, in a GNU long name or
    /// a pax `path` record, is refused as one in the field is.
    #[test]
    fn extract_tar_gz_refuses_a_path_from_a_long_name_or_a_pax_record() {
        for name in ["../planted", "../outside/planted", "pkg/../../planted"] {
            for member in [
                HostileMember::LongName(name, b"planted"),
                HostileMember::PaxPath(name, b"planted"),
            ] {
                let scene = HostileScene::new("hostile.tar.gz");
                create_hostile_tar_gz(
                    &scene.archive,
                    &[HostileMember::File("pkg/infc", b"binary content"), member],
                );

                let result = extract_archive(&scene.archive, &scene.dest);

                scene.assert_refused(result, &["Refusing to extract path", name]);
            }
        }
    }

    /// A pax global header, which `git archive` writes first, carries only
    /// metadata: it is skipped rather than refused or written as a file, and
    /// it no longer keeps the root folder the members share from being
    /// stripped.
    #[test]
    fn extract_tar_gz_skips_a_pax_global_header() {
        let temp_dir = temp_test_dir();
        let archive_path = temp_dir.path().join("test.tar.gz");
        let dest_dir = temp_dir.path().join("output");
        {
            let file = std::fs::File::create(&archive_path).expect("Should create file");
            let mut builder = Builder::new(GzEncoder::new(file, Compression::default()));
            let record = b"15 comment=abc\n";
            let mut header = tar::Header::new_ustar();
            header.set_entry_type(tar::EntryType::XGlobalHeader);
            header
                .set_path("pax_global_header")
                .expect("Should set path");
            header.set_size(u64::try_from(record.len()).expect("fits"));
            header.set_cksum();
            builder
                .append(&header, record.as_slice())
                .expect("Should append the global header");
            let mut header = tar::Header::new_ustar();
            header.set_size(14);
            header.set_mode(0o755);
            header.set_cksum();
            builder
                .append_data(&mut header, "pkg/infc", b"binary content".as_slice())
                .expect("Should append file");
            builder.finish().expect("Should finish");
        }

        extract_tar_gz(&archive_path, &dest_dir).expect("Should extract");

        assert_eq!(
            std::fs::read(dest_dir.join("infc")).expect("infc must be extracted at the root"),
            b"binary content"
        );
        assert!(!dest_dir.join("pkg").exists(), "the root folder must be stripped");
        assert!(
            !dest_dir.join("pax_global_header").exists(),
            "the global header must not be written as a file"
        );
    }

    /// A symbolic link in a ZIP archive is refused, whether a member is
    /// written through it or it stands alone. This extractor creates no
    /// links, so it used to write each as a file holding its target's path.
    #[test]
    fn extract_zip_refuses_a_symlink_member() {
        for through in [Some("escape/planted"), None] {
            let scene = HostileScene::new("hostile.zip");
            {
                let file = std::fs::File::create(&scene.archive).expect("Should create file");
                let mut zip = zip::ZipWriter::new(file);
                let options = zip::write::SimpleFileOptions::default();
                zip.add_symlink("escape", scene.outside(), options)
                    .expect("Should add symlink");
                if let Some(through) = through {
                    zip.start_file(through, options).expect("Should start file");
                    zip.write_all(b"planted").expect("Should write");
                }
                zip.finish().expect("Should finish");
            }

            let result = extract_archive(&scene.archive, &scene.dest);

            scene.assert_refused(result, &["escape", "symbolic link"]);
        }
    }

    /// A ZIP member whose name climbs out of the archive is refused before
    /// anything is written.
    #[test]
    fn extract_zip_refuses_member_paths_that_leave_the_destination() {
        for name in ["../planted", "../outside/planted", "pkg/../../planted"] {
            let scene = HostileScene::new("hostile.zip");
            {
                let file = std::fs::File::create(&scene.archive).expect("Should create file");
                let mut zip = zip::ZipWriter::new(file);
                let options = zip::write::SimpleFileOptions::default();
                zip.start_file("pkg/infc", options).expect("Should start file");
                zip.write_all(b"binary content").expect("Should write");
                zip.start_file(name, options).expect("Should start file");
                zip.write_all(b"planted").expect("Should write");
                zip.finish().expect("Should finish");
            }

            let result = extract_archive(&scene.archive, &scene.dest);

            scene.assert_refused(result, &["Invalid entry path"]);
        }
    }

    /// A ZIP member name `enclosed_name` turns into one that leaves the
    /// destination is refused by the check after it. Before zip 9, the `/`s
    /// behind a verbatim prefix were not separators, so `\\?\C:\../outside/x`
    /// came back as `../outside/x`; zip 9 refuses the name itself. On Windows,
    /// `pkg/C:planted` comes back as `C:planted`, which a join puts outside the
    /// destination.
    #[test]
    fn extract_zip_refuses_a_name_enclosed_name_lets_through() {
        #[allow(unused_mut)]
        let mut names = vec![r"\\?\C:\../outside/planted"];
        #[cfg(windows)]
        names.push("pkg/C:planted");

        for name in names {
            let scene = HostileScene::new("hostile.zip");
            {
                let file = std::fs::File::create(&scene.archive).expect("Should create file");
                let mut zip = zip::ZipWriter::new(file);
                let options = zip::write::SimpleFileOptions::default();
                zip.start_file("pkg/infc", options).expect("Should start file");
                zip.write_all(b"binary content").expect("Should write");
                zip.start_file(name, options).expect("Should start file");
                zip.write_all(b"planted").expect("Should write");
                zip.finish().expect("Should finish");
            }

            let result = extract_archive(&scene.archive, &scene.dest);

            let message = scene.assert_refused(result, &[]);
            assert!(
                message.contains("Refusing to extract path")
                    || message.contains("Invalid entry path"),
                "{name:?} must be refused as a path: {message}"
            );
        }
    }

    /// A tar.gz wrapped in a ZIP is extracted with links refused too: a
    /// symbolic link inside it, and a member written through it, never reach
    /// `outside/`. The inner archive is checked only after the ZIP has written
    /// it, so `extract_archive` removes the destination it created, leaving
    /// no partial toolchain behind.
    #[test]
    fn extract_zip_refuses_a_nested_tar_gz_holding_a_symlink() {
        let scene = HostileScene::new("hostile.zip");
        let inner = scene.temp.path().join("inner.tar.gz");
        create_hostile_tar_gz(
            &inner,
            &[
                HostileMember::Link(tar::EntryType::Symlink, "escape", &scene.outside()),
                HostileMember::File("escape/planted", b"planted"),
            ],
        );
        {
            let file = std::fs::File::create(&scene.archive).expect("Should create file");
            let mut zip = zip::ZipWriter::new(file);
            let options = zip::write::SimpleFileOptions::default();
            zip.start_file("infc-linux-x64.tar.gz", options)
                .expect("Should start file");
            zip.write_all(&std::fs::read(&inner).expect("Should read inner tar.gz"))
                .expect("Should write");
            zip.finish().expect("Should finish");
        }

        let result = extract_archive(&scene.archive, &scene.dest);

        scene.assert_refused(result, &["escape", "symbolic link"]);
    }

    /// A failed extraction removes only a destination it created: one that
    /// existed before keeps what it held.
    #[test]
    fn extract_archive_keeps_a_destination_that_existed_when_extraction_fails() {
        for (archive_name, is_zip) in [("hostile.tar.gz", false), ("hostile.zip", true)] {
            let scene = HostileScene::new(archive_name);
            if is_zip {
                let file = std::fs::File::create(&scene.archive).expect("Should create file");
                let mut zip = zip::ZipWriter::new(file);
                let options = zip::write::SimpleFileOptions::default();
                zip.add_symlink("escape", scene.outside(), options)
                    .expect("Should add symlink");
                zip.finish().expect("Should finish");
            } else {
                create_hostile_tar_gz(
                    &scene.archive,
                    &[HostileMember::Link(tar::EntryType::Symlink, "escape", &scene.outside())],
                );
            }
            std::fs::create_dir(&scene.dest).expect("Should create the destination");
            std::fs::write(scene.dest.join("kept"), b"kept").expect("Should write kept");

            extract_archive(&scene.archive, &scene.dest)
                .expect_err("the archive must be refused");

            assert_eq!(
                std::fs::read(scene.dest.join("kept"))
                    .expect("the destination must keep its files"),
                b"kept"
            );
            scene.assert_outside_untouched();
        }
    }

    #[test]
    fn extract_tar_gz_empty_archive() {
        let temp_dir = temp_test_dir();
        let archive_path = temp_dir.path().join("empty.tar.gz");
        let dest_dir = temp_dir.path().join("output");

        // Create an empty tar.gz archive (no entries)
        {
            let file = std::fs::File::create(&archive_path).expect("Should create file");
            let encoder = GzEncoder::new(file, Compression::default());
            let builder = Builder::new(encoder);
            builder.into_inner().expect("Should finish encoder");
        }

        // Extraction should succeed without errors
        extract_tar_gz(&archive_path, &dest_dir).expect("Should extract empty archive");

        // Destination directory should exist but be empty
        assert!(dest_dir.exists(), "Destination directory should exist");
        assert!(dest_dir.is_dir(), "Destination should be a directory");
        let entries: Vec<_> = std::fs::read_dir(&dest_dir)
            .expect("Should read dir")
            .collect();
        assert!(entries.is_empty(), "Destination directory should be empty");
    }

    #[test]
    fn extract_zip_empty_archive() {
        let temp_dir = temp_test_dir();
        let archive_path = temp_dir.path().join("empty.zip");
        let dest_dir = temp_dir.path().join("output");

        // Create an empty ZIP archive (no entries)
        {
            let file = std::fs::File::create(&archive_path).expect("Should create file");
            let zip = zip::ZipWriter::new(file);
            zip.finish().expect("Should finish");
        }

        // Extraction should succeed without errors
        extract_zip(&archive_path, &dest_dir).expect("Should extract empty archive");

        // Destination directory should exist but be empty
        assert!(dest_dir.exists(), "Destination directory should exist");
        assert!(dest_dir.is_dir(), "Destination should be a directory");
        let entries: Vec<_> = std::fs::read_dir(&dest_dir)
            .expect("Should read dir")
            .collect();
        assert!(entries.is_empty(), "Destination directory should be empty");
    }

    #[cfg(unix)]
    mod unix_permissions {
        use super::*;
        use std::os::unix::fs::PermissionsExt;

        #[test]
        fn set_executable_permissions_sets_755_on_root_infc() {
            let temp_dir = temp_test_dir();
            let infc_path = temp_dir.path().join("infc");
            std::fs::write(&infc_path, b"infc binary").expect("Should write infc");

            std::fs::set_permissions(&infc_path, std::fs::Permissions::from_mode(0o644))
                .expect("Should set initial perms");

            set_executable_permissions(temp_dir.path()).expect("Should set permissions");

            let mode = std::fs::metadata(&infc_path)
                .expect("Should get metadata")
                .permissions()
                .mode();

            assert_eq!(mode & 0o777, 0o755, "infc should have 0o755 mode");
        }

        #[test]
        fn set_executable_permissions_handles_missing_infc() {
            let temp_dir = temp_test_dir();

            let result = set_executable_permissions(temp_dir.path());

            assert!(result.is_ok(), "Should succeed without infc binary");
        }

        #[test]
        fn set_executable_permissions_sets_755_on_bundled_inference_lsp() {
            let temp_dir = temp_test_dir();
            let infc_path = temp_dir.path().join("infc");
            let lsp_path = temp_dir.path().join("inference-lsp");
            std::fs::write(&infc_path, b"infc binary").expect("Should write infc");
            std::fs::write(&lsp_path, b"lsp binary").expect("Should write inference-lsp");

            for path in [&infc_path, &lsp_path] {
                std::fs::set_permissions(path, std::fs::Permissions::from_mode(0o644))
                    .expect("Should set initial perms");
            }

            set_executable_permissions(temp_dir.path()).expect("Should set permissions");

            let lsp_mode = std::fs::metadata(&lsp_path)
                .expect("Should get metadata")
                .permissions()
                .mode();
            assert_eq!(lsp_mode & 0o777, 0o755, "inference-lsp should have 0o755 mode");
        }

        #[test]
        fn set_executable_permissions_skips_absent_inference_lsp() {
            let temp_dir = temp_test_dir();
            let infc_path = temp_dir.path().join("infc");
            std::fs::write(&infc_path, b"infc binary").expect("Should write infc");

            // A toolchain that predates the bundling has no inference-lsp; the
            // call must still succeed and leave the directory unchanged.
            set_executable_permissions(temp_dir.path())
                .expect("Should succeed without inference-lsp");
            assert!(
                !temp_dir.path().join("inference-lsp").exists(),
                "no inference-lsp file should be created"
            );
        }

        #[test]
        fn set_executable_file_sets_755_on_named_file() {
            let temp_dir = temp_test_dir();
            let file = temp_dir.path().join("wasm-opt");
            std::fs::write(&file, b"binary").expect("Should write file");

            std::fs::set_permissions(&file, std::fs::Permissions::from_mode(0o644))
                .expect("Should set initial perms");

            set_executable_file(&file).expect("Should set permissions");

            let mode = std::fs::metadata(&file)
                .expect("Should get metadata")
                .permissions()
                .mode();
            assert_eq!(mode & 0o777, 0o755, "file should have 0o755 mode");
        }

        #[test]
        fn set_executable_file_errors_on_missing_path() {
            let temp_dir = temp_test_dir();
            let missing = temp_dir.path().join("nope");

            assert!(
                set_executable_file(&missing).is_err(),
                "a missing path must surface as an error"
            );
        }
    }

    #[test]
    fn extract_zip_with_nested_tar_gz() {
        let temp_dir = temp_test_dir();
        let archive_path = temp_dir.path().join("outer.zip");
        let dest_dir = temp_dir.path().join("output");

        // Create inner tar.gz with actual files
        let inner_tar_gz_path = temp_dir.path().join("inner.tar.gz");
        create_tar_gz_like_ci_infc_toolchain(&inner_tar_gz_path);

        // Create outer zip containing only the tar.gz
        {
            let file = std::fs::File::create(&archive_path).expect("Should create file");
            let mut zip = zip::ZipWriter::new(file);
            let options = zip::write::SimpleFileOptions::default();

            zip.start_file("infc-linux-x64.tar.gz", options)
                .expect("Should start file");
            let tar_gz_content =
                std::fs::read(&inner_tar_gz_path).expect("Should read inner tar.gz");
            zip.write_all(&tar_gz_content).expect("Should write");

            zip.finish().expect("Should finish");
        }

        extract_zip(&archive_path, &dest_dir).expect("Should extract");

        // Verify nested archive was extracted
        assert!(dest_dir.join("infc").exists(), "infc should exist");
        // tar.gz should be cleaned up
        assert!(
            !dest_dir.join("infc-linux-x64.tar.gz").exists(),
            "tar.gz should be cleaned up"
        );
    }

    #[test]
    fn extract_zip_with_nested_tar_gz_and_sha256() {
        let temp_dir = temp_test_dir();
        let archive_path = temp_dir.path().join("outer.zip");
        let dest_dir = temp_dir.path().join("output");

        // Create inner tar.gz with actual files
        let inner_tar_gz_path = temp_dir.path().join("inner.tar.gz");
        create_tar_gz_like_ci_infc_toolchain(&inner_tar_gz_path);

        // Create outer zip containing tar.gz and sha256
        {
            let file = std::fs::File::create(&archive_path).expect("Should create file");
            let mut zip = zip::ZipWriter::new(file);
            let options = zip::write::SimpleFileOptions::default();

            zip.start_file("infc-linux-x64.tar.gz", options)
                .expect("Should start file");
            let tar_gz_content =
                std::fs::read(&inner_tar_gz_path).expect("Should read inner tar.gz");
            zip.write_all(&tar_gz_content).expect("Should write");

            zip.start_file("infc-linux-x64.tar.gz.sha256", options)
                .expect("Should start file");
            zip.write_all(b"abc123 infc-linux-x64.tar.gz")
                .expect("Should write");

            zip.finish().expect("Should finish");
        }

        extract_zip(&archive_path, &dest_dir).expect("Should extract");

        // Verify nested archive was extracted
        assert!(dest_dir.join("infc").exists(), "infc should exist");
        // Both tar.gz and sha256 should be cleaned up
        assert!(
            !dest_dir.join("infc-linux-x64.tar.gz").exists(),
            "tar.gz should be cleaned up"
        );
        assert!(
            !dest_dir.join("infc-linux-x64.tar.gz.sha256").exists(),
            "sha256 should be cleaned up"
        );
    }

    #[test]
    fn extract_zip_with_mixed_content_not_nested() {
        let temp_dir = temp_test_dir();
        let archive_path = temp_dir.path().join("outer.zip");
        let dest_dir = temp_dir.path().join("output");

        // Create inner tar.gz with actual files
        let inner_tar_gz_path = temp_dir.path().join("inner.tar.gz");
        create_tar_gz_like_ci_infc_toolchain(&inner_tar_gz_path);

        // Create zip with tar.gz PLUS other files - should NOT extract nested
        {
            let file = std::fs::File::create(&archive_path).expect("Should create file");
            let mut zip = zip::ZipWriter::new(file);
            let options = zip::write::SimpleFileOptions::default();

            zip.start_file("archive.tar.gz", options)
                .expect("Should start file");
            let tar_gz_content =
                std::fs::read(&inner_tar_gz_path).expect("Should read inner tar.gz");
            zip.write_all(&tar_gz_content).expect("Should write");

            zip.start_file("README.md", options)
                .expect("Should start file");
            zip.write_all(b"Some readme content").expect("Should write");

            zip.finish().expect("Should finish");
        }

        extract_zip(&archive_path, &dest_dir).expect("Should extract");

        // tar.gz should NOT be extracted (mixed content)
        assert!(
            dest_dir.join("archive.tar.gz").exists(),
            "tar.gz should still exist (not extracted)"
        );
        assert!(dest_dir.join("README.md").exists(), "README should exist");
        // Inner content should NOT exist
        assert!(
            !dest_dir.join("infc").exists(),
            "infc should NOT exist (not nested scenario)"
        );
    }
}
