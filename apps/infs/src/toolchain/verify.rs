//! Checksum verification for downloaded toolchain files.
//!
//! This module provides SHA256 checksum verification to ensure
//! downloaded files match their expected hashes.

use std::io::Read;
use std::path::Path;

use anyhow::{Context, Result, bail};
use sha2::{Digest, Sha256};

/// Verifies that a file matches the expected SHA256 checksum.
///
/// # Arguments
///
/// * `file_path` - Path to the file to verify
/// * `expected` - Expected SHA256 hash as a lowercase hex string
///
/// # Errors
///
/// Returns an error if:
/// - The file cannot be opened or read
/// - The computed checksum does not match the expected value
///
/// # Example
///
/// ```ignore
/// verify_checksum(Path::new("toolchain.zip"), "abc123...")?;
/// ```
pub fn verify_checksum(file_path: &Path, expected: &str) -> Result<()> {
    let computed = compute_sha256(file_path)?;

    if computed != expected.to_lowercase() {
        bail!(
            "Checksum verification failed for {}\n\
             \n\
             Expected: {expected}\n\
             Got:      {computed}\n\
             \n\
             The download may be corrupted or tampered with.\n\
             Please try downloading again.",
            file_path.display()
        );
    }

    Ok(())
}

/// Computes the SHA256 hash of a file.
///
/// # Arguments
///
/// * `file_path` - Path to the file to hash
///
/// # Returns
///
/// The SHA256 hash as a lowercase hex string.
///
/// # Errors
///
/// Returns an error if the file cannot be opened or read.
pub fn compute_sha256(file_path: &Path) -> Result<String> {
    let mut file = std::fs::File::open(file_path)
        .with_context(|| format!("Failed to open file for checksum: {}", file_path.display()))?;

    let mut hasher = Sha256::new();
    let mut buffer = [0u8; 8192];

    loop {
        let bytes_read = file.read(&mut buffer).with_context(|| {
            format!("Failed to read file for checksum: {}", file_path.display())
        })?;

        if bytes_read == 0 {
            break;
        }

        hasher.update(&buffer[..bytes_read]);
    }

    let hash = hasher.finalize();
    Ok(hex::encode(hash))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;

    #[test]
    fn compute_sha256_produces_correct_hash() {
        let temp_dir = std::env::temp_dir().join("infs_test_sha256");
        std::fs::create_dir_all(&temp_dir).expect("Should create temp dir");
        let test_file = temp_dir.join("test_file.txt");

        let mut file = std::fs::File::create(&test_file).expect("Should create test file");
        file.write_all(b"hello world\n")
            .expect("Should write test content");
        drop(file);

        let hash = compute_sha256(&test_file).expect("Should compute hash");

        assert_eq!(
            hash,
            "a948904f2f0f479b8f8197694b30184b0d2ed1c1cd2a1ec0fb85d299a192a447"
        );

        std::fs::remove_file(&test_file).ok();
    }

    #[test]
    fn verify_checksum_passes_for_matching_hash() {
        let temp_dir = std::env::temp_dir().join("infs_test_verify_pass");
        std::fs::create_dir_all(&temp_dir).expect("Should create temp dir");
        let test_file = temp_dir.join("test_file.txt");

        let mut file = std::fs::File::create(&test_file).expect("Should create test file");
        file.write_all(b"hello world\n")
            .expect("Should write test content");
        drop(file);

        let result = verify_checksum(
            &test_file,
            "a948904f2f0f479b8f8197694b30184b0d2ed1c1cd2a1ec0fb85d299a192a447",
        );
        assert!(result.is_ok());

        std::fs::remove_file(&test_file).ok();
    }

    #[test]
    fn verify_checksum_fails_for_mismatched_hash() {
        let temp_dir = std::env::temp_dir().join("infs_test_verify_fail");
        std::fs::create_dir_all(&temp_dir).expect("Should create temp dir");
        let test_file = temp_dir.join("test_file.txt");

        let mut file = std::fs::File::create(&test_file).expect("Should create test file");
        file.write_all(b"hello world\n")
            .expect("Should write test content");
        drop(file);

        let result = verify_checksum(&test_file, "wrong_hash_value");
        assert!(result.is_err());

        let error_msg = result.unwrap_err().to_string();
        assert!(error_msg.contains("Checksum verification failed"));
        assert!(error_msg.contains("wrong_hash_value"));

        std::fs::remove_file(&test_file).ok();
    }

    #[test]
    fn verify_checksum_handles_uppercase_expected() {
        let temp_dir = std::env::temp_dir().join("infs_test_verify_uppercase");
        std::fs::create_dir_all(&temp_dir).expect("Should create temp dir");
        let test_file = temp_dir.join("test_file.txt");

        let mut file = std::fs::File::create(&test_file).expect("Should create test file");
        file.write_all(b"hello world\n")
            .expect("Should write test content");
        drop(file);

        let result = verify_checksum(
            &test_file,
            "A948904F2F0F479B8F8197694B30184B0D2ED1C1CD2A1EC0FB85D299A192A447",
        );
        assert!(result.is_ok());

        std::fs::remove_file(&test_file).ok();
    }

    #[test]
    fn compute_sha256_fails_for_nonexistent_file() {
        let result = compute_sha256(Path::new("/nonexistent/file/path"));
        assert!(result.is_err());
    }
}
