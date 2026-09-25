//! Archive extraction utilities for the infs toolchain.
//!
//! This module provides functionality for extracting ZIP and tar.gz archives
//! used during toolchain and self-update installations.

use anyhow::{Context, Result};
use flate2::read::GzDecoder;
use std::path::{Path, PathBuf};
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

    std::fs::create_dir_all(dest_dir)
        .with_context(|| format!("Failed to create directory: {}", dest_dir.display()))?;

    let strip_prefix = find_common_root_folder(&mut archive);

    for i in 0..archive.len() {
        let mut entry = archive
            .by_index(i)
            .with_context(|| format!("Failed to read archive entry {i}"))?;

        let entry_path = entry
            .enclosed_name()
            .with_context(|| format!("Invalid entry path in archive: entry {i}"))?;

        // Security: defense-in-depth check (enclosed_name already filters these)
        // Reject paths with parent directory references or absolute paths
        if entry_path.is_absolute()
            || entry_path
                .components()
                .any(|c| matches!(c, std::path::Component::ParentDir))
        {
            anyhow::bail!(
                "Refusing to extract path with parent directory or absolute reference: {}",
                entry_path.display()
            );
        }

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
/// # Errors
///
/// Returns an error if:
/// - The archive format cannot be determined
/// - The archive cannot be opened or extracted
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
    let path_str = archive_path.to_string_lossy();
    if path_str.ends_with(".tar.gz") || path_str.ends_with(".tgz") {
        extract_tar_gz(archive_path, dest_dir)
    } else {
        extract_zip(archive_path, dest_dir)
    }
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
    std::fs::create_dir_all(dest_dir)
        .with_context(|| format!("Failed to create directory: {}", dest_dir.display()))?;

    let strip_prefix = find_common_root_folder_tar(archive_path)?;

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

        let entry_path = entry
            .path()
            .with_context(|| "Failed to get entry path")?
            .into_owned();

        // Security: reject paths with parent directory references or absolute paths
        // to prevent path traversal attacks (e.g., "../../../etc/passwd")
        if entry_path.is_absolute()
            || entry_path
                .components()
                .any(|c| matches!(c, std::path::Component::ParentDir))
        {
            anyhow::bail!(
                "Refusing to extract path with parent directory or absolute reference: {}",
                entry_path.display()
            );
        }

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

/// Finds a common root folder shared by all tar.gz archive entries.
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
    let mut has_nested_entries = false;

    for entry in archive
        .entries()
        .with_context(|| format!("Failed to read tar entries: {}", archive_path.display()))?
    {
        let entry = entry
            .with_context(|| format!("Failed to read tar entry: {}", archive_path.display()))?;

        let path = entry.path().with_context(|| "Failed to get entry path")?;

        // Track if we have any entries with nested paths (depth > 1)
        if path.components().count() > 1 {
            has_nested_entries = true;
        }

        let Some(first_component) = path.components().next() else {
            continue;
        };
        let root = PathBuf::from(first_component.as_os_str());

        match &common_root {
            None => common_root = Some(root),
            Some(existing) if existing != &root => return Ok(None),
            Some(_) => {}
        }
    }

    // Only strip common root if there are nested entries
    // (root is actually a containing folder, not just a flat file)
    if has_nested_entries {
        Ok(common_root)
    } else {
        Ok(None)
    }
}

/// Finds a common root folder shared by all archive entries.
///
/// Returns `Some(prefix)` if all entries start with the same folder name
/// AND there are nested entries (paths with more than one component).
/// Otherwise returns `None`.
///
/// This prevents flat files at the archive root from being incorrectly
/// treated as "common root folders" and stripped away.
fn find_common_root_folder<R: std::io::Read + std::io::Seek>(
    archive: &mut zip::ZipArchive<R>,
) -> Option<PathBuf> {
    if archive.is_empty() {
        return None;
    }

    let mut common_root: Option<PathBuf> = None;
    let mut has_nested_entries = false;

    for i in 0..archive.len() {
        let entry = archive.by_index(i).ok()?;
        let path = entry.enclosed_name()?;

        // Track if we have any entries with nested paths (depth > 1)
        if path.components().count() > 1 {
            has_nested_entries = true;
        }

        let first_component = path.components().next()?;
        let root = PathBuf::from(first_component.as_os_str());

        match &common_root {
            None => common_root = Some(root),
            Some(existing) if existing != &root => return None,
            Some(_) => {}
        }
    }

    // Only strip common root if there are nested entries
    // (root is actually a containing folder, not just a flat file)
    if has_nested_entries {
        common_root
    } else {
        None
    }
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

    /// One member of a release package as `.github/workflows/reusable-build.yml`
    /// stages it: a directory, or a file and its bytes. The name is the archive
    /// member name relative to the package root, `/`-separated as both archive
    /// formats spell it.
    enum PackageEntry {
        Dir(String),
        File(String, Vec<u8>),
    }

    /// The repository's `licenses/` directory, which CI copies into every
    /// release package beside the binaries.
    fn repository_licenses() -> PathBuf {
        Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("..")
            .join("..")
            .join("licenses")
    }

    /// What CI stages for a release package: each of `binaries` at the root,
    /// then `cp -R licenses` — the repository's notices, read from the
    /// checkout, so the fixture follows the directory as it grows.
    fn ci_package(binaries: &[&str]) -> Vec<PackageEntry> {
        let mut entries: Vec<PackageEntry> = binaries
            .iter()
            .map(|name| PackageEntry::File((*name).to_string(), format!("{name} binary").into()))
            .collect();
        push_tree(&mut entries, &repository_licenses(), "licenses");
        entries
    }

    /// Appends `dir` as the member `name` and then everything under it,
    /// parents before children and siblings in name order.
    fn push_tree(entries: &mut Vec<PackageEntry>, dir: &Path, name: &str) {
        entries.push(PackageEntry::Dir(name.to_string()));
        let mut children: Vec<PathBuf> = std::fs::read_dir(dir)
            .unwrap_or_else(|e| panic!("{} must be readable: {e}", dir.display()))
            .map(|entry| entry.expect("Should read directory entry").path())
            .collect();
        children.sort();
        for child in children {
            let child_name = child
                .file_name()
                .and_then(|n| n.to_str())
                .expect("a notice's name is UTF-8");
            let member = format!("{name}/{child_name}");
            if child.is_dir() {
                push_tree(entries, &child, &member);
            } else {
                let bytes = std::fs::read(&child)
                    .unwrap_or_else(|e| panic!("{} must be readable: {e}", child.display()));
                entries.push(PackageEntry::File(member, bytes));
            }
        }
    }

    /// Packs `entries` as `tar -czf ARCHIVE -C STAGING .` does on the Linux and
    /// macOS runners: a `./` member first and every other member under `./`,
    /// a directory as a member of its own, binaries executable and notices not.
    fn create_tar_gz_like_ci(archive_path: &Path, entries: &[PackageEntry]) {
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
                PackageEntry::Dir(name) => append_dir(&mut builder, &format!("./{name}/")),
                PackageEntry::File(name, bytes) => {
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
    fn create_zip_like_ci(archive_path: &Path, entries: &[PackageEntry]) {
        let file = std::fs::File::create(archive_path).expect("Should create file");
        let mut zip = zip::ZipWriter::new(file);
        let options = zip::write::SimpleFileOptions::default();
        for entry in entries {
            match entry {
                PackageEntry::Dir(name) => zip
                    .add_directory(format!("{name}/"), options)
                    .expect("Should add directory"),
                PackageEntry::File(name, bytes) => {
                    zip.start_file(name.as_str(), options)
                        .expect("Should start file");
                    zip.write_all(bytes).expect("Should write");
                }
            }
        }
        zip.finish().expect("Should finish");
    }

    /// The path of the archive member `name` under `dir`.
    fn member_path(dir: &Path, name: &str) -> PathBuf {
        name.split('/').fold(dir.to_path_buf(), |path, part| path.join(part))
    }

    /// Asserts that `dest` holds exactly what `entries` staged, where an install
    /// looks for it: each binary at the root and each notice under `licenses/`,
    /// byte for byte, and nothing else at the root — no `.` directory, no
    /// stripped or doubled `licenses` level.
    fn assert_extracted_as_staged(dest: &Path, entries: &[PackageEntry]) {
        for entry in entries {
            match entry {
                PackageEntry::Dir(name) => assert!(
                    member_path(dest, name).is_dir(),
                    "{name}/ must be extracted as a directory"
                ),
                PackageEntry::File(name, bytes) => assert_eq!(
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
                PackageEntry::Dir(name) | PackageEntry::File(name, _) => {
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
                PackageEntry::File(name, _) if name == "licenses/spacewasm/NOTICE"
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

    // Note: The `tar` crate itself prevents creating archives with `..` paths,
    // so we cannot easily test path traversal rejection. The security check in
    // extract_tar_gz provides defense-in-depth for archives from untrusted sources.
    // ZIP's `enclosed_name()` method also provides similar protection.

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
