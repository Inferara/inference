//! TUI state management.
//!
//! This module defines the screen state machine and view-specific state
//! for the infs TUI application.

use crate::toolchain::paths::ToolchainMetadata;

pub use crate::toolchain::doctor::{DoctorCheck, DoctorCheckStatus};

/// Active screen in the TUI application.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Screen {
    /// Main menu screen with shortcuts.
    #[default]
    Main,
    /// Installed toolchains list view.
    Toolchains,
    /// Doctor check results view.
    Doctor,
    /// Progress view for downloads and operations.
    Progress,
    /// Version selection view for choosing a version to install.
    VersionSelect,
}

/// Message sent from installation task to TUI for progress updates.
///
/// These messages are sent via a channel from the background installation thread
/// to the main TUI event loop. The TUI polls the channel non-blocking and updates
/// the progress display accordingly.
#[derive(Debug, Clone)]
pub enum InstallProgress {
    /// A new phase of the installation has started.
    PhaseStarted {
        /// Description of the phase (e.g., "Fetching manifest", "Downloading").
        phase: String,
    },
    /// Download has started with a known total size.
    DownloadStarted {
        /// Total file size in bytes.
        total: u64,
    },
    /// Download progress update.
    DownloadProgress {
        /// Bytes downloaded so far.
        downloaded: u64,
        /// Current download speed in bytes per second.
        speed: u64,
    },
    /// A phase of the installation has completed.
    PhaseCompleted {
        /// Description of the completed phase.
        phase: String,
    },
    /// Installation completed successfully.
    Completed {
        /// The version that was installed.
        version: String,
    },
    /// Installation failed with an error.
    Failed {
        /// Error description.
        error: String,
    },
}

/// Information about an installed toolchain version.
#[derive(Debug, Clone)]
pub struct ToolchainInfo {
    /// Version string (e.g., "0.1.0").
    pub version: String,
    /// Whether this is the default toolchain.
    pub is_default: bool,
    /// Installation metadata (if available).
    pub metadata: Option<ToolchainMetadata>,
}

/// State for the toolchains view.
#[derive(Debug, Clone, Default)]
pub struct ToolchainsState {
    /// List of installed toolchains.
    pub toolchains: Vec<ToolchainInfo>,
    /// Currently selected index.
    pub selected: usize,
    /// Whether the data has been loaded.
    pub loaded: bool,
}

impl ToolchainsState {
    /// Creates a new empty toolchains state.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Moves selection up.
    pub fn select_previous(&mut self) {
        if !self.toolchains.is_empty() {
            self.selected = self.selected.saturating_sub(1);
        }
    }

    /// Moves selection down.
    pub fn select_next(&mut self) {
        if !self.toolchains.is_empty() {
            self.selected = (self.selected + 1).min(self.toolchains.len() - 1);
        }
    }
}

/// State for the doctor view.
#[derive(Debug, Clone, Default)]
pub struct DoctorState {
    /// List of check results.
    pub checks: Vec<DoctorCheck>,
    /// Currently selected index.
    pub selected: usize,
    /// Whether the data has been loaded.
    pub loaded: bool,
}

impl DoctorState {
    /// Creates a new empty doctor state.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Moves selection up.
    pub fn select_previous(&mut self) {
        if !self.checks.is_empty() {
            self.selected = self.selected.saturating_sub(1);
        }
    }

    /// Moves selection down.
    pub fn select_next(&mut self) {
        if !self.checks.is_empty() {
            self.selected = (self.selected + 1).min(self.checks.len() - 1);
        }
    }

    /// Returns the number of checks with Ok status.
    #[must_use]
    pub fn ok_count(&self) -> usize {
        self.checks
            .iter()
            .filter(|c| c.status == DoctorCheckStatus::Ok)
            .count()
    }

    /// Returns the number of checks with Warning status.
    #[must_use]
    pub fn warning_count(&self) -> usize {
        self.checks
            .iter()
            .filter(|c| c.status == DoctorCheckStatus::Warning)
            .count()
    }

    /// Returns the number of checks with Error status.
    #[must_use]
    pub fn error_count(&self) -> usize {
        self.checks
            .iter()
            .filter(|c| c.status == DoctorCheckStatus::Error)
            .count()
    }
}

/// Information about an available version for installation.
#[derive(Debug, Clone)]
pub struct VersionSelectInfo {
    /// Version string (e.g., "0.2.0").
    pub version: String,
    /// Whether this is a stable release.
    pub stable: bool,
    /// List of available platforms for this version.
    pub platforms: Vec<String>,
    /// Whether this version is available for the current platform.
    pub available_for_current: bool,
}

/// State for the version selection view.
#[derive(Debug, Clone, Default)]
pub struct VersionSelectState {
    /// List of available versions.
    pub versions: Vec<VersionSelectInfo>,
    /// Currently selected index.
    pub selected: usize,
    /// Whether the data has been loaded.
    pub loaded: bool,
    /// Whether data is currently loading.
    pub loading: bool,
    /// Error message if loading failed.
    pub error: Option<String>,
    /// Current OS name for display.
    pub current_os: String,
}

impl VersionSelectState {
    /// Creates a new empty version select state.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Moves selection up.
    pub fn select_previous(&mut self) {
        if !self.versions.is_empty() {
            self.selected = self.selected.saturating_sub(1);
        }
    }

    /// Moves selection down.
    pub fn select_next(&mut self) {
        if !self.versions.is_empty() {
            self.selected = (self.selected + 1).min(self.versions.len() - 1);
        }
    }

    /// Returns the currently selected version info, if any.
    #[must_use]
    pub fn selected_version(&self) -> Option<&VersionSelectInfo> {
        self.versions.get(self.selected)
    }

    /// Returns whether the selected version is available for the current platform.
    #[must_use]
    pub fn can_install_selected(&self) -> bool {
        self.selected_version()
            .is_some_and(|v| v.available_for_current)
    }
}

/// Progress information for a single download or operation.
#[derive(Debug, Clone)]
pub struct ProgressItem {
    /// Description of what is being downloaded/processed.
    pub description: String,
    /// Current progress in bytes.
    pub current: u64,
    /// Total size in bytes (0 if unknown).
    pub total: u64,
    /// Whether this item is completed.
    pub completed: bool,
    /// Current download speed in bytes per second.
    pub speed_bytes_per_sec: Option<u64>,
    /// When the download started (for calculating speed).
    pub started_at: Option<std::time::Instant>,
}

impl ProgressItem {
    /// Creates a new progress item.
    #[must_use]
    pub fn new(description: impl Into<String>) -> Self {
        Self {
            description: description.into(),
            current: 0,
            total: 0,
            completed: false,
            speed_bytes_per_sec: None,
            started_at: None,
        }
    }

    /// Marks the start of the download operation.
    pub fn start(&mut self) {
        self.started_at = Some(std::time::Instant::now());
    }

    /// Updates the current progress with speed information.
    pub fn update_with_speed(&mut self, current: u64, speed: u64) {
        self.current = current;
        self.speed_bytes_per_sec = Some(speed);
        if self.total > 0 && self.current >= self.total {
            self.completed = true;
        }
    }

    /// Marks this item as completed.
    pub fn complete(&mut self) {
        self.completed = true;
        if self.total > 0 {
            self.current = self.total;
        }
    }

    /// Returns the progress as a percentage (0.0 to 1.0).
    #[must_use]
    pub fn percentage(&self) -> f64 {
        if self.total == 0 {
            if self.completed { 1.0 } else { 0.0 }
        } else {
            #[allow(clippy::cast_precision_loss)]
            let result = self.current as f64 / self.total as f64;
            result.clamp(0.0, 1.0)
        }
    }

    /// Formats the progress as a human-readable string.
    #[must_use]
    pub fn format_progress(&self) -> String {
        if self.total == 0 {
            if self.completed {
                "Done".to_string()
            } else {
                format_bytes(self.current)
            }
        } else {
            format!(
                "{} / {}",
                format_bytes(self.current),
                format_bytes(self.total)
            )
        }
    }

    /// Formats the download speed as a human-readable string.
    ///
    /// Returns an empty string if no speed information is available.
    #[must_use]
    pub fn format_speed(&self) -> String {
        match self.speed_bytes_per_sec {
            Some(speed) if speed > 0 => format!("{}/s", format_bytes(speed)),
            _ => String::new(),
        }
    }
}

/// Formats bytes as a human-readable string.
#[must_use]
fn format_bytes(bytes: u64) -> String {
    const KB: u64 = 1024;
    const MB: u64 = KB * 1024;
    const GB: u64 = MB * 1024;

    if bytes >= GB {
        #[allow(clippy::cast_precision_loss)]
        let gb = bytes as f64 / GB as f64;
        format!("{gb:.1} GB")
    } else if bytes >= MB {
        #[allow(clippy::cast_precision_loss)]
        let mb = bytes as f64 / MB as f64;
        format!("{mb:.1} MB")
    } else if bytes >= KB {
        #[allow(clippy::cast_precision_loss)]
        let kb = bytes as f64 / KB as f64;
        format!("{kb:.1} KB")
    } else {
        format!("{bytes} B")
    }
}

/// State for the progress view.
#[derive(Debug, Clone, Default)]
pub struct ProgressState {
    /// Current operation title.
    pub title: String,
    /// List of progress items.
    pub items: Vec<ProgressItem>,
    /// Overall status message.
    pub status: String,
    /// Whether the operation is complete.
    pub completed: bool,
    /// Error message if the operation failed.
    pub error: Option<String>,
}

impl ProgressState {
    /// Creates a new progress state with a title.
    #[must_use]
    pub fn new(title: impl Into<String>) -> Self {
        Self {
            title: title.into(),
            items: Vec::new(),
            status: String::new(),
            completed: false,
            error: None,
        }
    }

    /// Adds a progress item.
    pub fn add_item(&mut self, item: ProgressItem) {
        self.items.push(item);
    }

    /// Sets the status message.
    pub fn set_status(&mut self, status: impl Into<String>) {
        self.status = status.into();
    }

    /// Marks the operation as completed.
    pub fn complete(&mut self) {
        self.completed = true;
    }

    /// Sets an error message and marks as complete.
    pub fn set_error(&mut self, error: impl Into<String>) {
        self.error = Some(error.into());
        self.completed = true;
    }

    /// Returns the overall progress percentage (0.0 to 1.0).
    #[must_use]
    pub fn overall_percentage(&self) -> f64 {
        if self.items.is_empty() {
            if self.completed { 1.0 } else { 0.0 }
        } else {
            let sum: f64 = self.items.iter().map(ProgressItem::percentage).sum();
            #[allow(clippy::cast_precision_loss)]
            let avg = sum / self.items.len() as f64;
            avg
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn screen_default_is_main() {
        assert_eq!(Screen::default(), Screen::Main);
    }

    #[test]
    fn toolchains_state_select_previous_at_zero_stays_zero() {
        let mut state = ToolchainsState {
            toolchains: vec![ToolchainInfo {
                version: "0.1.0".to_string(),
                is_default: true,
                metadata: None,
            }],
            selected: 0,
            loaded: true,
        };
        state.select_previous();
        assert_eq!(state.selected, 0);
    }

    #[test]
    fn toolchains_state_select_next_respects_bounds() {
        let mut state = ToolchainsState {
            toolchains: vec![
                ToolchainInfo {
                    version: "0.1.0".to_string(),
                    is_default: true,
                    metadata: None,
                },
                ToolchainInfo {
                    version: "0.2.0".to_string(),
                    is_default: false,
                    metadata: None,
                },
            ],
            selected: 0,
            loaded: true,
        };
        state.select_next();
        assert_eq!(state.selected, 1);
        state.select_next();
        assert_eq!(state.selected, 1);
    }

    #[test]
    fn toolchains_state_empty_navigation_is_safe() {
        let mut state = ToolchainsState::new();
        state.select_previous();
        state.select_next();
        assert_eq!(state.selected, 0);
    }

    #[test]
    fn doctor_state_counts_are_correct() {
        let state = DoctorState {
            checks: vec![
                DoctorCheck::ok("check1", "ok"),
                DoctorCheck::ok("check2", "ok"),
                DoctorCheck::warning("check3", "warn"),
                DoctorCheck::error("check4", "err"),
            ],
            selected: 0,
            loaded: true,
        };
        assert_eq!(state.ok_count(), 2);
        assert_eq!(state.warning_count(), 1);
        assert_eq!(state.error_count(), 1);
    }

    #[test]
    fn doctor_state_select_respects_bounds() {
        let mut state = DoctorState {
            checks: vec![
                DoctorCheck::ok("check1", "ok"),
                DoctorCheck::ok("check2", "ok"),
            ],
            selected: 0,
            loaded: true,
        };
        state.select_next();
        assert_eq!(state.selected, 1);
        state.select_next();
        assert_eq!(state.selected, 1);
        state.select_previous();
        assert_eq!(state.selected, 0);
        state.select_previous();
        assert_eq!(state.selected, 0);
    }

    #[test]
    fn progress_item_percentage_no_total() {
        let item = ProgressItem::new("test");
        assert!((item.percentage() - 0.0).abs() < f64::EPSILON);

        let mut item = ProgressItem::new("test");
        item.complete();
        assert!((item.percentage() - 1.0).abs() < f64::EPSILON);
    }

    #[test]
    fn progress_item_format_bytes() {
        assert_eq!(format_bytes(500), "500 B");
        assert_eq!(format_bytes(1024), "1.0 KB");
        assert_eq!(format_bytes(1536), "1.5 KB");
        assert_eq!(format_bytes(1024 * 1024), "1.0 MB");
        assert_eq!(format_bytes(1024 * 1024 * 1024), "1.0 GB");
    }

    #[test]
    fn progress_state_complete() {
        let mut state = ProgressState::new("test");
        assert!(!state.completed);

        state.complete();
        assert!(state.completed);
        assert!((state.overall_percentage() - 1.0).abs() < f64::EPSILON);
    }

    #[test]
    fn progress_state_error() {
        let mut state = ProgressState::new("test");
        assert!(state.error.is_none());

        state.set_error("Something went wrong");
        assert!(state.completed);
        assert!(state.error.is_some());
        assert_eq!(state.error.as_deref(), Some("Something went wrong"));
    }

    #[test]
    fn progress_item_format_speed_none() {
        let item = ProgressItem::new("test");
        assert_eq!(item.format_speed(), "");
    }

    #[test]
    fn progress_item_format_speed_zero() {
        let mut item = ProgressItem::new("test");
        item.speed_bytes_per_sec = Some(0);
        assert_eq!(item.format_speed(), "");
    }

    #[test]
    fn progress_item_format_speed_bytes() {
        let mut item = ProgressItem::new("test");
        item.speed_bytes_per_sec = Some(500);
        assert_eq!(item.format_speed(), "500 B/s");
    }

    #[test]
    fn progress_item_format_speed_kilobytes() {
        let mut item = ProgressItem::new("test");
        item.speed_bytes_per_sec = Some(1024);
        assert_eq!(item.format_speed(), "1.0 KB/s");
    }

    #[test]
    fn progress_item_format_speed_megabytes() {
        let mut item = ProgressItem::new("test");
        item.speed_bytes_per_sec = Some(1024 * 1024);
        assert_eq!(item.format_speed(), "1.0 MB/s");

        item.speed_bytes_per_sec = Some(1024 * 1024 * 5);
        assert_eq!(item.format_speed(), "5.0 MB/s");
    }

    #[test]
    fn progress_item_start_sets_timestamp() {
        let mut item = ProgressItem::new("test");
        assert!(item.started_at.is_none());
        item.start();
        assert!(item.started_at.is_some());
    }

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
    fn install_progress_phase_completed_contains_phase() {
        let progress = InstallProgress::PhaseCompleted {
            phase: String::from("Download"),
        };
        match progress {
            InstallProgress::PhaseCompleted { phase } => {
                assert_eq!(phase, "Download");
            }
            _ => panic!("Expected PhaseCompleted variant"),
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
    fn version_select_state_new_is_default() {
        let state = VersionSelectState::new();
        assert!(state.versions.is_empty());
        assert_eq!(state.selected, 0);
        assert!(!state.loaded);
        assert!(!state.loading);
        assert!(state.error.is_none());
    }

    #[test]
    fn version_select_state_navigation() {
        let mut state = VersionSelectState {
            versions: vec![
                VersionSelectInfo {
                    version: "0.1.0".to_string(),
                    stable: true,
                    platforms: vec!["linux".to_string()],
                    available_for_current: true,
                },
                VersionSelectInfo {
                    version: "0.2.0".to_string(),
                    stable: true,
                    platforms: vec!["linux".to_string(), "macos".to_string()],
                    available_for_current: true,
                },
            ],
            selected: 0,
            loaded: true,
            loading: false,
            error: None,
            current_os: "linux".to_string(),
        };

        state.select_next();
        assert_eq!(state.selected, 1);

        state.select_next();
        assert_eq!(state.selected, 1); // Should not exceed bounds

        state.select_previous();
        assert_eq!(state.selected, 0);

        state.select_previous();
        assert_eq!(state.selected, 0); // Should not go below 0
    }

    #[test]
    fn version_select_state_selected_version() {
        let state = VersionSelectState {
            versions: vec![
                VersionSelectInfo {
                    version: "0.1.0".to_string(),
                    stable: true,
                    platforms: vec!["linux".to_string()],
                    available_for_current: true,
                },
                VersionSelectInfo {
                    version: "0.2.0".to_string(),
                    stable: false,
                    platforms: vec!["macos".to_string()],
                    available_for_current: false,
                },
            ],
            selected: 1,
            loaded: true,
            loading: false,
            error: None,
            current_os: "linux".to_string(),
        };

        let selected = state.selected_version().expect("Should have selected");
        assert_eq!(selected.version, "0.2.0");
        assert!(!selected.stable);
        assert!(!selected.available_for_current);
    }

    #[test]
    fn version_select_state_can_install_selected() {
        let mut state = VersionSelectState {
            versions: vec![
                VersionSelectInfo {
                    version: "0.1.0".to_string(),
                    stable: true,
                    platforms: vec!["linux".to_string()],
                    available_for_current: true,
                },
                VersionSelectInfo {
                    version: "0.2.0".to_string(),
                    stable: false,
                    platforms: vec!["macos".to_string()],
                    available_for_current: false,
                },
            ],
            selected: 0,
            loaded: true,
            loading: false,
            error: None,
            current_os: "linux".to_string(),
        };

        assert!(state.can_install_selected());

        state.selected = 1;
        assert!(!state.can_install_selected());
    }

    #[test]
    fn version_select_state_empty_navigation_is_safe() {
        let mut state = VersionSelectState::new();
        state.select_previous();
        state.select_next();
        assert_eq!(state.selected, 0);
    }
}
