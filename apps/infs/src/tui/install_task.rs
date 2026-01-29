//! Background installation task for TUI integration.
//!
//! This module provides an async installation function that reports progress
//! via a channel, allowing the TUI to display real-time progress without blocking.
//!
//! The installation runs on a separate thread with its own tokio runtime to avoid
//! blocking the main TUI event loop.

use std::sync::mpsc::Sender;

use anyhow::{Context, Result};

use super::state::InstallProgress;
use crate::toolchain::paths::ToolchainMetadata;
use crate::toolchain::{
    Platform, ProgressCallback, ProgressEvent, ToolchainPaths, download_file_with_callback,
    extract_archive, fetch_artifact, set_executable_permissions, verify_checksum,
};

/// Runs the toolchain installation asynchronously, sending progress updates to the TUI.
///
/// This function performs the same operations as the CLI install command but reports
/// progress via the provided channel instead of printing to stdout.
///
/// # Arguments
///
/// * `version` - Optional version to install. If `None`, installs the latest version.
/// * `tx` - Channel sender for progress updates.
///
/// # Process
///
/// 1. Detect the current platform
/// 2. Fetch the release manifest
/// 3. Find the artifact for the requested version and platform
/// 4. Download the archive with progress reporting
/// 5. Verify the SHA256 checksum
/// 6. Extract to the toolchains directory
/// 7. Set as default if it's the first installation
pub async fn run_installation(version: Option<String>, tx: Sender<InstallProgress>) {
    if let Err(e) = run_installation_inner(version, tx.clone()).await {
        let _ = tx.send(InstallProgress::Failed {
            error: e.to_string(),
        });
    }
}

/// Inner implementation that returns a Result for easier error handling.
#[allow(clippy::too_many_lines)]
async fn run_installation_inner(
    version: Option<String>,
    tx: Sender<InstallProgress>,
) -> Result<()> {
    let _ = tx.send(InstallProgress::PhaseStarted {
        phase: String::from("Detecting platform"),
    });

    let platform = Platform::detect().context("Failed to detect platform")?;
    let paths = ToolchainPaths::new().context("Failed to initialize toolchain paths")?;

    paths
        .ensure_directories()
        .context("Failed to create toolchain directories")?;

    let _ = tx.send(InstallProgress::PhaseCompleted {
        phase: String::from("Detecting platform"),
    });

    let _ = tx.send(InstallProgress::PhaseStarted {
        phase: String::from("Downloading release manifest"),
    });

    let version_arg = version.as_deref();
    let (resolved_version, artifact) = fetch_artifact(version_arg, platform)
        .await
        .context("Failed to download release manifest")?;

    let _ = tx.send(InstallProgress::PhaseCompleted {
        phase: String::from("Downloading release manifest"),
    });

    if paths.is_version_installed(&resolved_version) {
        let _ = tx.send(InstallProgress::Completed {
            version: resolved_version,
        });
        return Ok(());
    }

    let _ = tx.send(InstallProgress::PhaseStarted {
        phase: format!("Downloading toolchain v{resolved_version}"),
    });

    let archive_filename = artifact.filename();
    let archive_path = paths.download_path(archive_filename);

    let tx_callback = tx.clone();
    let callback: ProgressCallback = std::sync::Arc::new(move |event| {
        match event {
            ProgressEvent::Started { total, .. } => {
                let _ = tx_callback.send(InstallProgress::DownloadStarted { total });
            }
            ProgressEvent::Progress { downloaded, speed } => {
                let _ = tx_callback.send(InstallProgress::DownloadProgress { downloaded, speed });
            }
            ProgressEvent::Completed | ProgressEvent::Failed { .. } => {
                // Handled at higher level
            }
        }
    });

    download_file_with_callback(&artifact.url, &archive_path, callback)
        .await
        .context("Failed to download toolchain archive")?;

    let _ = tx.send(InstallProgress::PhaseCompleted {
        phase: format!("Downloading toolchain v{resolved_version}"),
    });

    let _ = tx.send(InstallProgress::PhaseStarted {
        phase: String::from("Verifying checksum"),
    });

    verify_checksum(&archive_path, &artifact.sha256)
        .context("Checksum verification failed - download may be corrupted")?;

    let _ = tx.send(InstallProgress::PhaseCompleted {
        phase: String::from("Verifying checksum"),
    });

    let _ = tx.send(InstallProgress::PhaseStarted {
        phase: String::from("Extracting archive"),
    });

    let toolchain_dir = paths.toolchain_dir(&resolved_version);
    extract_archive(&archive_path, &toolchain_dir)
        .context("Failed to extract toolchain archive")?;

    set_executable_permissions(&toolchain_dir).context("Failed to set executable permissions")?;

    let metadata = ToolchainMetadata::now();
    paths
        .write_metadata(&resolved_version, &metadata)
        .context("Failed to write toolchain metadata")?;

    let _ = tx.send(InstallProgress::PhaseCompleted {
        phase: String::from("Extracting archive"),
    });

    let _ = tx.send(InstallProgress::PhaseStarted {
        phase: String::from("Configuring toolchain"),
    });

    let installed_versions = paths
        .list_installed_versions()
        .context("Failed to list installed versions")?;
    let is_first_install =
        installed_versions.len() == 1 && installed_versions[0] == resolved_version;
    let current_default = paths
        .get_default_version()
        .context("Failed to get default version")?;

    if is_first_install || current_default.is_none() {
        paths
            .set_default_version(&resolved_version)
            .context("Failed to set default version")?;
        paths
            .update_symlinks(&resolved_version)
            .context("Failed to update symlinks")?;
    }

    std::fs::remove_file(&archive_path).ok();

    let _ = tx.send(InstallProgress::PhaseCompleted {
        phase: String::from("Configuring toolchain"),
    });

    let _ = tx.send(InstallProgress::Completed {
        version: resolved_version,
    });

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::mpsc;

    #[test]
    fn install_progress_phase_started_contains_phase() {
        let progress = InstallProgress::PhaseStarted {
            phase: String::from("Testing"),
        };
        match progress {
            InstallProgress::PhaseStarted { phase } => {
                assert_eq!(phase, "Testing");
            }
            _ => panic!("Expected PhaseStarted variant"),
        }
    }

    #[test]
    fn install_progress_download_started_contains_total() {
        let progress = InstallProgress::DownloadStarted { total: 1024 };
        match progress {
            InstallProgress::DownloadStarted { total } => {
                assert_eq!(total, 1024);
            }
            _ => panic!("Expected DownloadStarted variant"),
        }
    }

    #[test]
    fn install_progress_download_progress_contains_data() {
        let progress = InstallProgress::DownloadProgress {
            downloaded: 512,
            speed: 1024,
        };
        match progress {
            InstallProgress::DownloadProgress { downloaded, speed } => {
                assert_eq!(downloaded, 512);
                assert_eq!(speed, 1024);
            }
            _ => panic!("Expected DownloadProgress variant"),
        }
    }

    #[test]
    fn install_progress_completed_contains_version() {
        let progress = InstallProgress::Completed {
            version: String::from("0.1.0"),
        };
        match progress {
            InstallProgress::Completed { version } => {
                assert_eq!(version, "0.1.0");
            }
            _ => panic!("Expected Completed variant"),
        }
    }

    #[test]
    fn install_progress_failed_contains_error() {
        let progress = InstallProgress::Failed {
            error: String::from("Network error"),
        };
        match progress {
            InstallProgress::Failed { error } => {
                assert_eq!(error, "Network error");
            }
            _ => panic!("Expected Failed variant"),
        }
    }

    #[test]
    fn install_progress_is_clone() {
        let progress = InstallProgress::DownloadProgress {
            downloaded: 100,
            speed: 50,
        };
        let cloned = progress.clone();
        match cloned {
            InstallProgress::DownloadProgress { downloaded, speed } => {
                assert_eq!(downloaded, 100);
                assert_eq!(speed, 50);
            }
            _ => panic!("Expected DownloadProgress variant"),
        }
    }

    #[test]
    fn install_progress_is_debug() {
        let progress = InstallProgress::PhaseStarted {
            phase: String::from("test"),
        };
        let debug_str = format!("{progress:?}");
        assert!(debug_str.contains("PhaseStarted"));
        assert!(debug_str.contains("test"));
    }

    #[test]
    fn channel_can_send_install_progress() {
        let (tx, rx) = mpsc::channel();

        tx.send(InstallProgress::PhaseStarted {
            phase: String::from("Test phase"),
        })
        .expect("Should send");

        let received = rx.recv().expect("Should receive");
        match received {
            InstallProgress::PhaseStarted { phase } => {
                assert_eq!(phase, "Test phase");
            }
            _ => panic!("Unexpected variant"),
        }
    }

    #[test]
    fn channel_try_recv_returns_empty_when_no_messages() {
        let (_tx, rx) = mpsc::channel::<InstallProgress>();
        assert!(rx.try_recv().is_err());
    }
}
