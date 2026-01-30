//! Version command for the infs CLI.
//!
//! Displays version information for the infs toolchain.
//! In verbose mode, shows additional details including build date,
//! platform, and compiler version.

use anyhow::Result;
use clap::Args;

/// Arguments for the version command.
#[derive(Args)]
pub struct VersionArgs {
    /// Show detailed version information including build date, platform, and features.
    #[clap(short = 'v', long = "verbose", action = clap::ArgAction::SetTrue)]
    pub verbose: bool,
}

/// Executes the version command.
///
/// Prints the version string derived from the package version
/// defined in Cargo.toml at compile time. In verbose mode,
/// prints additional build and platform information.
#[allow(clippy::unnecessary_wraps)]
pub fn execute(args: &VersionArgs) -> Result<()> {
    if args.verbose {
        print_verbose_version();
    } else {
        println!("infs {}", env!("CARGO_PKG_VERSION"));
    }
    Ok(())
}

/// Prints detailed version information.
fn print_verbose_version() {
    println!("infs {}", env!("CARGO_PKG_VERSION"));
    println!();
    println!("Build Information:");
    println!("  Version:  {}", env!("CARGO_PKG_VERSION"));
    println!("  Commit:   {}", git_commit());
    println!("  Platform: {}", platform_string());
}

/// Returns the git commit hash from environment or a fallback.
fn git_commit() -> &'static str {
    option_env!("INFS_GIT_COMMIT").unwrap_or("unknown")
}

/// Returns a human-readable platform string.
fn platform_string() -> String {
    format!("{}-{}", std::env::consts::OS, std::env::consts::ARCH)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn execute_with_verbose_false_succeeds() {
        let args = VersionArgs { verbose: false };
        let result = execute(&args);
        assert!(result.is_ok());
    }

    #[test]
    fn execute_with_verbose_true_succeeds() {
        let args = VersionArgs { verbose: true };
        let result = execute(&args);
        assert!(result.is_ok());
    }

    #[test]
    fn platform_string_is_not_empty() {
        let platform = platform_string();
        assert!(!platform.is_empty());
        assert!(platform.contains('-'));
    }

    #[test]
    fn git_commit_returns_value() {
        let commit = git_commit();
        assert!(!commit.is_empty());
    }
}
