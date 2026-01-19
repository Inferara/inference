#![warn(clippy::pedantic)]

//! Version command for the infs CLI.
//!
//! Displays the current version of the infs toolchain.

use anyhow::Result;

/// Executes the version command.
///
/// Prints the version string derived from the package version
/// defined in Cargo.toml at compile time.
#[allow(clippy::unnecessary_wraps)]
pub fn execute() -> Result<()> {
    println!("infs {}", env!("CARGO_PKG_VERSION"));
    Ok(())
}
