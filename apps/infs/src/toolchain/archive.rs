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
/// stripped during extraction (e.g., `toolchain-0.2.0/bin/infc` becomes `bin/infc`).
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
/// stripped during extraction (e.g., `toolchain-0.2.0/bin/infc` becomes `bin/infc`).
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

/// Sets executable permissions on binary files within a toolchain directory (Unix only).
///
/// This function iterates over all files in the `bin` subdirectory of the given
/// directory and sets the executable permission bits (0o755) on each file.
///
/// On Windows, this function does nothing since executable permissions are not
/// managed the same way.
///
/// # Arguments
///
/// * `dir` - The toolchain directory containing a `bin` subdirectory.
///
/// # Errors
///
/// Returns an error if:
/// - The bin directory cannot be read
/// - File metadata cannot be retrieved
/// - Permissions cannot be set
///
/// # Example
///
/// ```ignore
/// use crate::toolchain::set_executable_permissions;
/// set_executable_permissions(Path::new("/path/to/toolchain"))?;
/// ```
#[cfg(unix)]
pub fn set_executable_permissions(dir: &Path) -> Result<()> {
    use std::os::unix::fs::PermissionsExt;

    let bin_dir = dir.join("bin");
    if bin_dir.exists() {
        let entries = std::fs::read_dir(&bin_dir)
            .with_context(|| format!("Failed to read bin directory: {}", bin_dir.display()))?;

        for entry in entries {
            let entry = entry.with_context(|| "Failed to read directory entry")?;
            let path = entry.path();
            if path.is_file() {
                let mut perms = std::fs::metadata(&path)
                    .with_context(|| format!("Failed to get metadata: {}", path.display()))?
                    .permissions();
                perms.set_mode(0o755);
                std::fs::set_permissions(&path, perms)
                    .with_context(|| format!("Failed to set permissions: {}", path.display()))?;
            }
        }
    }

    let infc_path = dir.join("infc");
    if infc_path.is_file() {
        let mut perms = std::fs::metadata(&infc_path)
            .with_context(|| format!("Failed to get metadata: {}", infc_path.display()))?
            .permissions();
        perms.set_mode(0o755);
        std::fs::set_permissions(&infc_path, perms)
            .with_context(|| format!("Failed to set permissions: {}", infc_path.display()))?;
    }

    Ok(())
}

/// Sets executable permissions (no-op on Windows).
#[cfg(windows)]
#[allow(clippy::unnecessary_wraps)]
pub fn set_executable_permissions(_dir: &Path) -> Result<()> {
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use flate2::Compression;
    use flate2::write::GzEncoder;
    use std::io::Write;
    use tar::Builder;

    /// Creates a temporary test directory with a unique name.
    fn temp_test_dir(name: &str) -> PathBuf {
        let dir =
            std::env::temp_dir().join(format!("infs_test_{}_{}", name, rand::random::<u64>()));
        std::fs::create_dir_all(&dir).expect("Should create temp dir");
        dir
    }

    /// Creates a tar.gz archive with files nested under a root folder.
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
                format!("{root_name}/bin/infc"),
                b"binary content".as_slice(),
            )
            .expect("Should append file");

        let mut header = tar::Header::new_gnu();
        header.set_size(15);
        header.set_mode(0o644);
        header.set_cksum();
        builder
            .append_data(
                &mut header,
                format!("{root_name}/lib/libLLVM.so"),
                b"library content".as_slice(),
            )
            .expect("Should append file");

        builder.finish().expect("Should finish");
    }

    /// Creates a tar.gz archive with files at the root level (no common folder).
    fn create_tar_gz_without_root(archive_path: &Path) {
        let file = std::fs::File::create(archive_path).expect("Should create file");
        let encoder = GzEncoder::new(file, Compression::default());
        let mut builder = Builder::new(encoder);

        let mut header = tar::Header::new_gnu();
        header.set_size(14);
        header.set_mode(0o755);
        header.set_cksum();
        builder
            .append_data(&mut header, "bin/infc", b"binary content".as_slice())
            .expect("Should append file");

        let mut header = tar::Header::new_gnu();
        header.set_size(15);
        header.set_mode(0o644);
        header.set_cksum();
        builder
            .append_data(&mut header, "lib/libLLVM.so", b"library content".as_slice())
            .expect("Should append file");

        builder.finish().expect("Should finish");
    }

    #[test]
    fn extract_tar_gz_strips_common_root_folder() {
        let temp_dir = temp_test_dir("tar_gz_strip");
        let archive_path = temp_dir.join("test.tar.gz");
        let dest_dir = temp_dir.join("output");

        create_tar_gz_with_root(&archive_path, "root-folder");

        extract_tar_gz(&archive_path, &dest_dir).expect("Should extract");

        assert!(dest_dir.join("bin").join("infc").exists());
        assert!(dest_dir.join("lib").join("libLLVM.so").exists());
        assert!(!dest_dir.join("root-folder").exists());

        let _ = std::fs::remove_dir_all(&temp_dir);
    }

    #[test]
    fn extract_tar_gz_preserves_structure_without_common_root() {
        let temp_dir = temp_test_dir("tar_gz_preserve");
        let archive_path = temp_dir.join("test.tar.gz");
        let dest_dir = temp_dir.join("output");

        create_tar_gz_without_root(&archive_path);

        extract_tar_gz(&archive_path, &dest_dir).expect("Should extract");

        assert!(dest_dir.join("bin").join("infc").exists());
        assert!(dest_dir.join("lib").join("libLLVM.so").exists());

        let _ = std::fs::remove_dir_all(&temp_dir);
    }

    #[test]
    fn extract_archive_selects_tar_gz_for_tar_gz_extension() {
        let temp_dir = temp_test_dir("archive_select_tar_gz");
        let archive_path = temp_dir.join("test.tar.gz");
        let dest_dir = temp_dir.join("output");

        create_tar_gz_without_root(&archive_path);

        extract_archive(&archive_path, &dest_dir).expect("Should extract");

        assert!(dest_dir.join("bin").join("infc").exists());
        assert!(dest_dir.join("lib").join("libLLVM.so").exists());

        let _ = std::fs::remove_dir_all(&temp_dir);
    }

    #[test]
    fn extract_archive_selects_tar_gz_for_tgz_extension() {
        let temp_dir = temp_test_dir("archive_select_tgz");
        let archive_path = temp_dir.join("test.tgz");
        let dest_dir = temp_dir.join("output");

        create_tar_gz_without_root(&archive_path);

        extract_archive(&archive_path, &dest_dir).expect("Should extract");

        assert!(dest_dir.join("bin").join("infc").exists());
        assert!(dest_dir.join("lib").join("libLLVM.so").exists());

        let _ = std::fs::remove_dir_all(&temp_dir);
    }

    #[test]
    fn extract_archive_selects_zip_for_zip_extension() {
        let temp_dir = temp_test_dir("archive_select_zip");
        let archive_path = temp_dir.join("test.zip");
        let dest_dir = temp_dir.join("output");

        {
            let file = std::fs::File::create(&archive_path).expect("Should create file");
            let mut zip = zip::ZipWriter::new(file);

            let options = zip::write::SimpleFileOptions::default();
            zip.start_file("bin/infc", options)
                .expect("Should start file");
            zip.write_all(b"binary content").expect("Should write");

            zip.start_file("lib/libLLVM.so", options)
                .expect("Should start file");
            zip.write_all(b"library content").expect("Should write");

            zip.finish().expect("Should finish");
        }

        extract_archive(&archive_path, &dest_dir).expect("Should extract");

        assert!(dest_dir.join("bin").join("infc").exists());
        assert!(dest_dir.join("lib").join("libLLVM.so").exists());

        let _ = std::fs::remove_dir_all(&temp_dir);
    }

    #[test]
    fn extract_zip_strips_common_root_folder() {
        let temp_dir = temp_test_dir("archive_strip");
        let archive_path = temp_dir.join("test.zip");
        let dest_dir = temp_dir.join("output");

        // Create a zip with a root folder
        {
            let file = std::fs::File::create(&archive_path).expect("Should create file");
            let mut zip = zip::ZipWriter::new(file);

            let options = zip::write::SimpleFileOptions::default();
            zip.start_file("root-folder/bin/infc", options)
                .expect("Should start file");
            zip.write_all(b"binary content").expect("Should write");

            zip.start_file("root-folder/lib/libLLVM.so", options)
                .expect("Should start file");
            zip.write_all(b"library content").expect("Should write");

            zip.finish().expect("Should finish");
        }

        extract_zip(&archive_path, &dest_dir).expect("Should extract");

        // Verify root folder was stripped
        assert!(dest_dir.join("bin").join("infc").exists());
        assert!(dest_dir.join("lib").join("libLLVM.so").exists());
        assert!(!dest_dir.join("root-folder").exists());

        // Cleanup
        let _ = std::fs::remove_dir_all(&temp_dir);
    }

    #[test]
    fn extract_zip_preserves_structure_without_common_root() {
        let temp_dir = temp_test_dir("archive_preserve");
        let archive_path = temp_dir.join("test.zip");
        let dest_dir = temp_dir.join("output");

        // Create a zip without a common root folder
        {
            let file = std::fs::File::create(&archive_path).expect("Should create file");
            let mut zip = zip::ZipWriter::new(file);

            let options = zip::write::SimpleFileOptions::default();
            zip.start_file("bin/infc", options)
                .expect("Should start file");
            zip.write_all(b"binary content").expect("Should write");

            zip.start_file("lib/libLLVM.so", options)
                .expect("Should start file");
            zip.write_all(b"library content").expect("Should write");

            zip.finish().expect("Should finish");
        }

        extract_zip(&archive_path, &dest_dir).expect("Should extract");

        // Verify structure is preserved
        assert!(dest_dir.join("bin").join("infc").exists());
        assert!(dest_dir.join("lib").join("libLLVM.so").exists());

        // Cleanup
        let _ = std::fs::remove_dir_all(&temp_dir);
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
        let temp_dir = temp_test_dir("tar_gz_flat_single");
        let archive_path = temp_dir.join("test.tar.gz");
        let dest_dir = temp_dir.join("output");

        // Archive contains just "infs" at root (like CI produces)
        create_tar_gz_flat_single_file(&archive_path, "infs");

        extract_tar_gz(&archive_path, &dest_dir).expect("Should extract");

        // File should be extracted as-is, not skipped
        assert!(dest_dir.join("infs").exists(), "infs should exist at root");

        let _ = std::fs::remove_dir_all(&temp_dir);
    }

    #[test]
    fn extract_tar_gz_flat_multiple_files_not_stripped() {
        let temp_dir = temp_test_dir("tar_gz_flat_multi");
        let archive_path = temp_dir.join("test.tar.gz");
        let dest_dir = temp_dir.join("output");

        create_tar_gz_flat_multiple_files(&archive_path);

        extract_tar_gz(&archive_path, &dest_dir).expect("Should extract");

        // Both files should exist at root
        assert!(dest_dir.join("infs").exists(), "infs should exist");
        assert!(
            dest_dir.join("README.md").exists(),
            "README.md should exist"
        );

        let _ = std::fs::remove_dir_all(&temp_dir);
    }

    #[test]
    fn extract_tar_gz_single_nested_file_stripped() {
        let temp_dir = temp_test_dir("tar_gz_nested_single");
        let archive_path = temp_dir.join("test.tar.gz");
        let dest_dir = temp_dir.join("output");

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

        let _ = std::fs::remove_dir_all(&temp_dir);
    }

    #[test]
    fn extract_zip_flat_single_file_not_stripped() {
        let temp_dir = temp_test_dir("zip_flat_single");
        let archive_path = temp_dir.join("test.zip");
        let dest_dir = temp_dir.join("output");

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

        let _ = std::fs::remove_dir_all(&temp_dir);
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
        let temp_dir = temp_test_dir("tar_gz_dot_prefix");
        let archive_path = temp_dir.join("test.tar.gz");
        let dest_dir = temp_dir.join("output");

        // Archive contains "./" and "./infs" (like CI produces with tar -C dir .)
        create_tar_gz_with_dot_prefix(&archive_path, "infs");

        extract_tar_gz(&archive_path, &dest_dir).expect("Should extract");

        // File should be extracted as "infs", not skipped
        assert!(
            dest_dir.join("infs").exists(),
            "infs should exist at root (dot prefix should be handled)"
        );

        let _ = std::fs::remove_dir_all(&temp_dir);
    }

    /// Creates a tar.gz archive mimicking CI infc toolchain structure.
    fn create_tar_gz_like_ci_infc_toolchain(archive_path: &Path) {
        let file = std::fs::File::create(archive_path).expect("Should create file");
        let encoder = GzEncoder::new(file, Compression::default());
        let mut builder = Builder::new(encoder);

        // Add ./ directory entry
        let mut header = tar::Header::new_gnu();
        header.set_entry_type(tar::EntryType::Directory);
        header.set_size(0);
        header.set_mode(0o755);
        header.set_cksum();
        builder
            .append_data(&mut header, "./", std::io::empty())
            .expect("Should append dir");

        // Add ./infc (compiler at root)
        let mut header = tar::Header::new_gnu();
        header.set_size(14);
        header.set_mode(0o755);
        header.set_cksum();
        builder
            .append_data(&mut header, "./infc", b"infc binary...".as_slice())
            .expect("Should append infc");

        // Add ./bin/ directory
        let mut header = tar::Header::new_gnu();
        header.set_entry_type(tar::EntryType::Directory);
        header.set_size(0);
        header.set_mode(0o755);
        header.set_cksum();
        builder
            .append_data(&mut header, "./bin/", std::io::empty())
            .expect("Should append bin dir");

        // Add ./bin/inf-llc
        let mut header = tar::Header::new_gnu();
        header.set_size(15);
        header.set_mode(0o755);
        header.set_cksum();
        builder
            .append_data(&mut header, "./bin/inf-llc", b"inf-llc binary.".as_slice())
            .expect("Should append inf-llc");

        // Add ./bin/rust-lld
        let mut header = tar::Header::new_gnu();
        header.set_size(16);
        header.set_mode(0o755);
        header.set_cksum();
        builder
            .append_data(
                &mut header,
                "./bin/rust-lld",
                b"rust-lld binary.".as_slice(),
            )
            .expect("Should append rust-lld");

        builder.finish().expect("Should finish");
    }

    #[test]
    fn extract_tar_gz_ci_infc_toolchain_structure() {
        let temp_dir = temp_test_dir("tar_gz_ci_infc");
        let archive_path = temp_dir.join("infc-linux-x64.tar.gz");
        let dest_dir = temp_dir.join("toolchain");

        // Create archive exactly like CI produces for infc toolchain
        create_tar_gz_like_ci_infc_toolchain(&archive_path);

        extract_tar_gz(&archive_path, &dest_dir).expect("Should extract");

        // Verify all binaries are in expected locations
        assert!(
            dest_dir.join("infc").exists(),
            "infc should exist at toolchain root"
        );
        assert!(
            dest_dir.join("bin").join("inf-llc").exists(),
            "inf-llc should exist in bin/"
        );
        assert!(
            dest_dir.join("bin").join("rust-lld").exists(),
            "rust-lld should exist in bin/"
        );

        // Verify ./ directory was NOT created (it should be stripped)
        assert!(
            !dest_dir.join(".").exists() || dest_dir.join(".") == dest_dir,
            "No literal '.' directory should be created"
        );

        let _ = std::fs::remove_dir_all(&temp_dir);
    }

    #[test]
    fn extract_zip_flat_multiple_files_not_stripped() {
        let temp_dir = temp_test_dir("zip_flat_multi");
        let archive_path = temp_dir.join("test.zip");
        let dest_dir = temp_dir.join("output");

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

        let _ = std::fs::remove_dir_all(&temp_dir);
    }

    // Note: The `tar` crate itself prevents creating archives with `..` paths,
    // so we cannot easily test path traversal rejection. The security check in
    // extract_tar_gz provides defense-in-depth for archives from untrusted sources.
    // ZIP's `enclosed_name()` method also provides similar protection.

    #[test]
    fn extract_tar_gz_empty_archive() {
        let temp_dir = temp_test_dir("tar_gz_empty");
        let archive_path = temp_dir.join("empty.tar.gz");
        let dest_dir = temp_dir.join("output");

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

        let _ = std::fs::remove_dir_all(&temp_dir);
    }

    #[test]
    fn extract_zip_empty_archive() {
        let temp_dir = temp_test_dir("zip_empty");
        let archive_path = temp_dir.join("empty.zip");
        let dest_dir = temp_dir.join("output");

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

        let _ = std::fs::remove_dir_all(&temp_dir);
    }

    #[cfg(unix)]
    mod unix_permissions {
        use super::*;
        use std::os::unix::fs::PermissionsExt;

        #[test]
        fn set_executable_permissions_sets_755_on_bin_files() {
            let temp_dir = temp_test_dir("exec_perm_bin");
            let bin_dir = temp_dir.join("bin");
            std::fs::create_dir_all(&bin_dir).expect("Should create bin dir");

            let file1 = bin_dir.join("infc");
            let file2 = bin_dir.join("inf-llc");
            std::fs::write(&file1, b"binary1").expect("Should write file1");
            std::fs::write(&file2, b"binary2").expect("Should write file2");

            // Set initial non-executable permissions
            std::fs::set_permissions(&file1, std::fs::Permissions::from_mode(0o644))
                .expect("Should set initial perms");
            std::fs::set_permissions(&file2, std::fs::Permissions::from_mode(0o644))
                .expect("Should set initial perms");

            set_executable_permissions(&temp_dir).expect("Should set permissions");

            let mode1 = std::fs::metadata(&file1)
                .expect("Should get metadata")
                .permissions()
                .mode();
            let mode2 = std::fs::metadata(&file2)
                .expect("Should get metadata")
                .permissions()
                .mode();

            assert_eq!(mode1 & 0o777, 0o755, "file1 should have 0o755 mode");
            assert_eq!(mode2 & 0o777, 0o755, "file2 should have 0o755 mode");

            let _ = std::fs::remove_dir_all(&temp_dir);
        }

        #[test]
        fn set_executable_permissions_handles_missing_bin_dir() {
            let temp_dir = temp_test_dir("exec_perm_no_bin");
            // Do not create bin/ subdirectory

            let result = set_executable_permissions(&temp_dir);

            assert!(result.is_ok(), "Should succeed without bin/ directory");

            let _ = std::fs::remove_dir_all(&temp_dir);
        }

        #[test]
        fn set_executable_permissions_sets_755_on_root_infc() {
            let temp_dir = temp_test_dir("exec_perm_root_infc");
            let infc_path = temp_dir.join("infc");
            std::fs::write(&infc_path, b"infc binary").expect("Should write infc");

            // Set initial non-executable permissions
            std::fs::set_permissions(&infc_path, std::fs::Permissions::from_mode(0o644))
                .expect("Should set initial perms");

            set_executable_permissions(&temp_dir).expect("Should set permissions");

            let mode = std::fs::metadata(&infc_path)
                .expect("Should get metadata")
                .permissions()
                .mode();

            assert_eq!(mode & 0o777, 0o755, "infc should have 0o755 mode");

            let _ = std::fs::remove_dir_all(&temp_dir);
        }
    }

    #[test]
    fn extract_zip_with_nested_tar_gz() {
        let temp_dir = temp_test_dir("zip_nested_tar_gz");
        let archive_path = temp_dir.join("outer.zip");
        let dest_dir = temp_dir.join("output");

        // Create inner tar.gz with actual files
        let inner_tar_gz_path = temp_dir.join("inner.tar.gz");
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
        assert!(
            dest_dir.join("bin").join("inf-llc").exists(),
            "inf-llc should exist"
        );
        // tar.gz should be cleaned up
        assert!(
            !dest_dir.join("infc-linux-x64.tar.gz").exists(),
            "tar.gz should be cleaned up"
        );

        let _ = std::fs::remove_dir_all(&temp_dir);
    }

    #[test]
    fn extract_zip_with_nested_tar_gz_and_sha256() {
        let temp_dir = temp_test_dir("zip_nested_sha256");
        let archive_path = temp_dir.join("outer.zip");
        let dest_dir = temp_dir.join("output");

        // Create inner tar.gz with actual files
        let inner_tar_gz_path = temp_dir.join("inner.tar.gz");
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
        assert!(
            dest_dir.join("bin").join("inf-llc").exists(),
            "inf-llc should exist"
        );
        // Both tar.gz and sha256 should be cleaned up
        assert!(
            !dest_dir.join("infc-linux-x64.tar.gz").exists(),
            "tar.gz should be cleaned up"
        );
        assert!(
            !dest_dir.join("infc-linux-x64.tar.gz.sha256").exists(),
            "sha256 should be cleaned up"
        );

        let _ = std::fs::remove_dir_all(&temp_dir);
    }

    #[test]
    fn extract_zip_with_mixed_content_not_nested() {
        let temp_dir = temp_test_dir("zip_mixed_content");
        let archive_path = temp_dir.join("outer.zip");
        let dest_dir = temp_dir.join("output");

        // Create inner tar.gz with actual files
        let inner_tar_gz_path = temp_dir.join("inner.tar.gz");
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

        let _ = std::fs::remove_dir_all(&temp_dir);
    }
}
