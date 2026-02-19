//! Build script for infs CLI.
//!
//! Sets compile-time environment variables for version information.

use std::process::Command;

fn main() {
    // Set git commit hash
    let commit = get_git_commit();
    println!("cargo:rustc-env=INFS_GIT_COMMIT={commit}");

    // Rerun if git HEAD changes (path relative to workspace root)
    if let Some(workspace_root) = get_workspace_root() {
        println!("cargo:rerun-if-changed={workspace_root}/.git/HEAD");
    }
}

/// Gets the workspace root directory.
fn get_workspace_root() -> Option<String> {
    let output = Command::new("git")
        .args(["rev-parse", "--show-toplevel"])
        .output()
        .ok()?;

    if output.status.success() {
        let path = String::from_utf8_lossy(&output.stdout).trim().to_string();
        if !path.is_empty() {
            return Some(path);
        }
    }
    None
}

/// Gets the short git commit hash.
fn get_git_commit() -> String {
    let output = Command::new("git")
        .args(["rev-parse", "--short", "HEAD"])
        .output();

    if let Ok(output) = output
        && output.status.success()
    {
        let hash = String::from_utf8_lossy(&output.stdout).trim().to_string();
        if !hash.is_empty() {
            return hash;
        }
    }

    "unknown".to_string()
}
