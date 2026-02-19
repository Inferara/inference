//! Shell configuration module for automatic PATH setup.
//!
//! This module provides functionality to detect the user's shell and
//! automatically configure their shell profile to include the inference
//! toolchain bin directory in PATH.
//!
//! ## Supported Shells
//!
//! - Bash: `~/.bashrc` or `~/.bash_profile`
//! - Zsh: `~/.zshrc`
//! - Fish: `~/.config/fish/config.fish`
//!
//! ## Configuration Format
//!
//! For bash/zsh:
//! ```bash
//! # Inference toolchain
//! export PATH="$HOME/.inference/bin:$PATH"
//! ```
//!
//! For fish:
//! ```fish
//! # Inference toolchain
//! fish_add_path $HOME/.inference/bin
//! ```

use anyhow::{Context, Result};
use std::path::{Path, PathBuf};

/// Marker comment used to identify inference PATH configuration.
#[cfg(unix)]
const INFERENCE_MARKER: &str = "# Inference toolchain";

/// Represents supported shell types.
#[cfg(unix)]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Shell {
    Bash,
    Zsh,
    Fish,
}

#[cfg(unix)]
impl Shell {
    /// Detects the user's shell from the SHELL environment variable.
    ///
    /// Returns `None` if the shell cannot be determined or is not supported.
    #[must_use]
    pub fn detect() -> Option<Self> {
        let shell_path = std::env::var("SHELL").ok()?;
        Self::from_path(&shell_path)
    }

    /// Parses a shell from a path string (e.g., "/bin/bash").
    #[must_use]
    pub fn from_path(path: &str) -> Option<Self> {
        let shell_name = Path::new(path).file_name()?.to_str()?;
        match shell_name {
            "bash" => Some(Self::Bash),
            "zsh" => Some(Self::Zsh),
            "fish" => Some(Self::Fish),
            _ => None,
        }
    }

    /// Returns the profile files to check for this shell.
    ///
    /// For bash, returns both `.bashrc` and `.bash_profile`.
    /// For zsh, returns `.zshrc`.
    /// For fish, returns `config.fish`.
    #[must_use]
    pub fn profile_candidates(self, home_dir: &Path) -> Vec<PathBuf> {
        match self {
            Self::Bash => vec![home_dir.join(".bashrc"), home_dir.join(".bash_profile")],
            Self::Zsh => vec![home_dir.join(".zshrc")],
            Self::Fish => vec![home_dir.join(".config").join("fish").join("config.fish")],
        }
    }

    /// Generates the PATH configuration snippet for this shell.
    ///
    /// Properly escapes special characters in paths:
    /// - For Bash/Zsh: escapes `$`, backticks, `"`, and `\` within double quotes
    /// - For Fish: uses single quotes for paths containing special characters
    ///   (spaces, `$`, `\`, `'`, `*`, `?`, `(`, `)`, `[`, `]`, `{`, `}`)
    #[must_use]
    pub fn path_config(self, bin_path: &Path) -> String {
        match self {
            Self::Bash | Self::Zsh => {
                let escaped_path = bin_path
                    .display()
                    .to_string()
                    .replace('\\', "\\\\")
                    .replace('$', "\\$")
                    .replace('`', "\\`")
                    .replace('"', "\\\"");
                format!("\n{INFERENCE_MARKER}\nexport PATH=\"{escaped_path}:$PATH\"\n")
            }
            Self::Fish => {
                let path_str = bin_path.display().to_string();
                let needs_quotes = path_str.contains(' ')
                    || path_str.contains('$')
                    || path_str.contains('\\')
                    || path_str.contains('\'')
                    || path_str.contains('*')
                    || path_str.contains('?')
                    || path_str.contains('(')
                    || path_str.contains(')')
                    || path_str.contains('[')
                    || path_str.contains(']')
                    || path_str.contains('{')
                    || path_str.contains('}');
                let formatted_path = if needs_quotes {
                    format!("'{}'", path_str.replace('\'', "\\'"))
                } else {
                    path_str
                };
                format!("\n{INFERENCE_MARKER}\nfish_add_path {formatted_path}\n")
            }
        }
    }

    /// Generates the source command for this shell's profile.
    #[must_use]
    pub fn source_command(self, profile_path: &Path) -> String {
        let _ = self;
        format!("source {}", profile_path.display())
    }
}

/// Result of attempting to configure PATH in a shell profile.
#[derive(Debug)]
pub enum ConfigureResult {
    /// PATH was successfully added to the profile.
    Added {
        profile: PathBuf,
        source_command: String,
    },
    /// PATH configuration already exists in the profile.
    AlreadyConfigured { profile: PathBuf },
    /// No suitable profile file was found (Unix only, never returned on Windows).
    #[cfg(unix)]
    NoProfileFound,
    /// Shell could not be detected (Unix only, never returned on Windows).
    #[cfg(unix)]
    ShellNotDetected,
}

/// Attempts to configure PATH in the user's shell profile.
///
/// This function:
/// 1. Detects the user's shell
/// 2. Finds the appropriate profile file
/// 3. Checks if PATH is already configured
/// 4. Adds the PATH configuration if needed
///
/// On Windows, this function modifies the user's PATH environment variable
/// in the registry (`HKEY_CURRENT_USER\Environment\Path`).
///
/// # Arguments
///
/// * `bin_path` - The path to add to PATH (e.g., `~/.inference/bin`)
///
/// # Errors
///
/// Returns an error if file operations fail. Does not return an error
/// if the shell cannot be detected or no profile is found - these cases
/// return `ConfigureResult::ShellNotDetected` or `ConfigureResult::NoProfileFound`.
#[cfg(unix)]
pub fn configure_path(bin_path: &Path) -> Result<ConfigureResult> {
    let Some(shell) = Shell::detect() else {
        return Ok(ConfigureResult::ShellNotDetected);
    };

    let Some(home_dir) = dirs::home_dir() else {
        return Ok(ConfigureResult::NoProfileFound);
    };

    let candidates = shell.profile_candidates(&home_dir);

    let Some(profile_path) = find_existing_profile(&candidates) else {
        return Ok(ConfigureResult::NoProfileFound);
    };

    if is_path_configured(&profile_path)? {
        return Ok(ConfigureResult::AlreadyConfigured {
            profile: profile_path,
        });
    }

    let config = shell.path_config(bin_path);
    append_to_file(&profile_path, &config)?;

    let source_command = shell.source_command(&profile_path);
    Ok(ConfigureResult::Added {
        profile: profile_path,
        source_command,
    })
}

/// Attempts to configure PATH in the Windows registry.
///
/// This function:
/// 1. Reads the current PATH from `HKEY_CURRENT_USER\Environment\Path`
/// 2. Checks if the bin path is already present (case-insensitive)
/// 3. Appends the bin path if not present
/// 4. Writes the updated PATH back to the registry
///
/// # Arguments
///
/// * `bin_path` - The path to add to PATH (e.g., `%USERPROFILE%\.inference\bin`)
///
/// # Errors
///
/// Returns an error if registry operations fail.
#[cfg(windows)]
pub fn configure_path(bin_path: &Path) -> Result<ConfigureResult> {
    use winreg::RegKey;
    use winreg::enums::{HKEY_CURRENT_USER, KEY_READ, KEY_WRITE};

    let hkcu = RegKey::predef(HKEY_CURRENT_USER);
    let env = hkcu
        .open_subkey_with_flags("Environment", KEY_READ | KEY_WRITE)
        .context("Failed to open HKCU\\Environment registry key")?;

    let current_path: String = env.get_value("Path").unwrap_or_default();
    let bin_str = bin_path.to_string_lossy();

    let registry_path = PathBuf::from(r"Registry: HKCU\Environment\Path");

    if current_path
        .split(';')
        .any(|p| p.eq_ignore_ascii_case(&bin_str))
    {
        return Ok(ConfigureResult::AlreadyConfigured {
            profile: registry_path,
        });
    }

    let new_path = if current_path.is_empty() {
        bin_str.to_string()
    } else {
        format!("{current_path};{bin_str}")
    };

    env.set_value("Path", &new_path)
        .context("Failed to update PATH in registry")?;

    Ok(ConfigureResult::Added {
        profile: registry_path,
        source_command: "Restart your terminal or log out and back in".to_string(),
    })
}

/// Finds the first existing profile file from a list of candidates.
#[cfg(unix)]
fn find_existing_profile(candidates: &[PathBuf]) -> Option<PathBuf> {
    candidates.iter().find(|p| p.exists()).cloned()
}

/// Checks if the inference PATH configuration already exists in a file.
#[cfg(unix)]
fn is_path_configured(profile_path: &Path) -> Result<bool> {
    let content = std::fs::read_to_string(profile_path)
        .with_context(|| format!("Failed to read profile: {}", profile_path.display()))?;
    Ok(content.contains(INFERENCE_MARKER))
}

/// Appends content to a file.
#[cfg(unix)]
fn append_to_file(path: &Path, content: &str) -> Result<()> {
    use std::fs::OpenOptions;
    use std::io::Write;

    let mut file = OpenOptions::new()
        .create(true)
        .append(true)
        .open(path)
        .with_context(|| format!("Failed to open profile for writing: {}", path.display()))?;

    file.write_all(content.as_bytes())
        .with_context(|| format!("Failed to write to profile: {}", path.display()))?;

    Ok(())
}

/// Returns a human-readable message describing the configuration result.
#[must_use]
pub fn format_result_message(result: &ConfigureResult, bin_path: &Path) -> String {
    match result {
        ConfigureResult::Added {
            profile,
            source_command,
        } => {
            format!(
                "Added {} to PATH in {}\nRun '{}' to use the toolchain.",
                bin_path.display(),
                profile.display(),
                source_command
            )
        }
        ConfigureResult::AlreadyConfigured { profile } => {
            format!("PATH already configured in {}", profile.display())
        }
        #[cfg(unix)]
        ConfigureResult::NoProfileFound => {
            format!(
                "Could not find shell profile. To use the toolchain, add to your PATH:\n  {}",
                format_manual_path_instruction(bin_path)
            )
        }
        #[cfg(unix)]
        ConfigureResult::ShellNotDetected => {
            format!(
                "Could not detect shell. To use the toolchain, add to your PATH:\n  {}",
                format_manual_path_instruction(bin_path)
            )
        }
    }
}

/// Returns the manual PATH configuration instruction appropriate for the platform.
#[must_use]
#[cfg(unix)]
fn format_manual_path_instruction(bin_path: &Path) -> String {
    format!("export PATH=\"{}:$PATH\"", bin_path.display())
}

#[cfg(test)]
#[cfg(unix)]
mod tests {
    use super::*;
    use std::env;

    #[test]
    fn shell_from_path_bash() {
        assert_eq!(Shell::from_path("/bin/bash"), Some(Shell::Bash));
        assert_eq!(Shell::from_path("/usr/bin/bash"), Some(Shell::Bash));
    }

    #[test]
    fn shell_from_path_zsh() {
        assert_eq!(Shell::from_path("/bin/zsh"), Some(Shell::Zsh));
        assert_eq!(Shell::from_path("/usr/local/bin/zsh"), Some(Shell::Zsh));
    }

    #[test]
    fn shell_from_path_fish() {
        assert_eq!(Shell::from_path("/usr/bin/fish"), Some(Shell::Fish));
    }

    #[test]
    fn shell_from_path_unknown() {
        assert_eq!(Shell::from_path("/bin/sh"), None);
        assert_eq!(Shell::from_path("/bin/tcsh"), None);
        assert_eq!(Shell::from_path(""), None);
    }

    #[test]
    fn profile_candidates_bash() {
        let home = PathBuf::from("/home/user");
        let candidates = Shell::Bash.profile_candidates(&home);
        assert_eq!(candidates.len(), 2);
        assert_eq!(candidates[0], PathBuf::from("/home/user/.bashrc"));
        assert_eq!(candidates[1], PathBuf::from("/home/user/.bash_profile"));
    }

    #[test]
    fn profile_candidates_zsh() {
        let home = PathBuf::from("/home/user");
        let candidates = Shell::Zsh.profile_candidates(&home);
        assert_eq!(candidates.len(), 1);
        assert_eq!(candidates[0], PathBuf::from("/home/user/.zshrc"));
    }

    #[test]
    fn profile_candidates_fish() {
        let home = PathBuf::from("/home/user");
        let candidates = Shell::Fish.profile_candidates(&home);
        assert_eq!(candidates.len(), 1);
        assert_eq!(
            candidates[0],
            PathBuf::from("/home/user/.config/fish/config.fish")
        );
    }

    #[test]
    fn path_config_bash() {
        let bin_path = PathBuf::from("/home/user/.inference/bin");
        let config = Shell::Bash.path_config(&bin_path);
        assert!(config.contains("# Inference toolchain"));
        assert!(config.contains("export PATH=\"/home/user/.inference/bin:$PATH\""));
    }

    #[test]
    fn path_config_zsh() {
        let bin_path = PathBuf::from("/home/user/.inference/bin");
        let config = Shell::Zsh.path_config(&bin_path);
        assert!(config.contains("# Inference toolchain"));
        assert!(config.contains("export PATH=\"/home/user/.inference/bin:$PATH\""));
    }

    #[test]
    fn path_config_fish() {
        let bin_path = PathBuf::from("/home/user/.inference/bin");
        let config = Shell::Fish.path_config(&bin_path);
        assert!(config.contains("# Inference toolchain"));
        assert!(config.contains("fish_add_path /home/user/.inference/bin"));
    }

    #[test]
    fn path_config_bash_escapes_special_chars() {
        let bin_path = PathBuf::from("/home/user/$HOME/`test`/\"quoted\"/bin");
        let config = Shell::Bash.path_config(&bin_path);
        assert!(config.contains("# Inference toolchain"));
        assert!(
            config.contains(r#"export PATH="/home/user/\$HOME/\`test\`/\"quoted\"/bin:$PATH""#)
        );
    }

    #[test]
    fn path_config_zsh_escapes_special_chars() {
        let bin_path = PathBuf::from("/home/user/$VAR/bin");
        let config = Shell::Zsh.path_config(&bin_path);
        assert!(config.contains(r#"export PATH="/home/user/\$VAR/bin:$PATH""#));
    }

    #[test]
    fn path_config_fish_quotes_path_with_spaces() {
        let bin_path = PathBuf::from("/home/user/My Documents/.inference/bin");
        let config = Shell::Fish.path_config(&bin_path);
        assert!(config.contains("# Inference toolchain"));
        assert!(config.contains("fish_add_path '/home/user/My Documents/.inference/bin'"));
    }

    #[test]
    fn path_config_fish_quotes_path_with_dollar() {
        let bin_path = PathBuf::from("/home/user/$HOME/bin");
        let config = Shell::Fish.path_config(&bin_path);
        assert!(config.contains("fish_add_path '/home/user/$HOME/bin'"));
    }

    #[test]
    fn path_config_fish_escapes_single_quotes() {
        let bin_path = PathBuf::from("/home/user/it's mine/bin");
        let config = Shell::Fish.path_config(&bin_path);
        assert!(config.contains(r"fish_add_path '/home/user/it\'s mine/bin'"));
    }

    #[test]
    fn source_command_bash() {
        let profile = PathBuf::from("/home/user/.bashrc");
        assert_eq!(
            Shell::Bash.source_command(&profile),
            "source /home/user/.bashrc"
        );
    }

    #[test]
    fn is_path_configured_detects_marker() {
        let temp_dir = env::temp_dir().join("infs_shell_test");
        std::fs::create_dir_all(&temp_dir).ok();

        let profile_path = temp_dir.join(".bashrc_test");

        std::fs::write(&profile_path, "# Some existing config\n").unwrap();
        assert!(!is_path_configured(&profile_path).unwrap());

        std::fs::write(
            &profile_path,
            "# Some existing config\n# Inference toolchain\nexport PATH=\"...\"",
        )
        .unwrap();
        assert!(is_path_configured(&profile_path).unwrap());

        std::fs::remove_file(&profile_path).ok();
    }

    #[test]
    fn append_to_file_creates_and_appends() {
        let temp_dir = env::temp_dir().join("infs_shell_test_append");
        std::fs::create_dir_all(&temp_dir).ok();

        let file_path = temp_dir.join("test_append");

        std::fs::write(&file_path, "initial content\n").unwrap();
        append_to_file(&file_path, "appended content\n").unwrap();

        let content = std::fs::read_to_string(&file_path).unwrap();
        assert!(content.contains("initial content"));
        assert!(content.contains("appended content"));

        std::fs::remove_file(&file_path).ok();
    }
}
