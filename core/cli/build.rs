//! Build script for infc CLI.
//!
//! Sets compile-time environment variables for version information.

use std::process::Command;

fn main() {
    // Set git commit hash
    let commit = get_git_commit();
    println!("cargo:rustc-env=INFC_GIT_COMMIT={commit}");

    // Emit rerun-if-changed paths so cargo re-runs this script whenever
    // HEAD moves. `.git` can be a file (worktrees, submodules) pointing at
    // a separate gitdir, so the literal `<workspace>/.git/HEAD` path is
    // unreliable. Ask git where HEAD actually lives, then also watch the
    // ref HEAD points at and packed-refs for branches that only exist
    // there. Any failure is silent — a missing .git must not break the
    // build.
    emit_git_rerun_paths();
}

/// Emits `cargo:rerun-if-changed` lines for the git HEAD file, the ref it
/// points at (when HEAD is a symbolic ref), and packed-refs. Uses `git
/// rev-parse --git-path` so worktrees and submodules resolve to the real
/// on-disk location rather than the `.git` pointer file.
fn emit_git_rerun_paths() {
    let Some(head_path) = git_path("HEAD") else {
        return;
    };
    println!("cargo:rerun-if-changed={head_path}");

    if let Ok(head_contents) = std::fs::read_to_string(&head_path)
        && let Some(ref_name) = head_contents.strip_prefix("ref: ").map(str::trim)
        && !ref_name.is_empty()
        && let Some(ref_path) = git_path(ref_name)
    {
        println!("cargo:rerun-if-changed={ref_path}");
    }

    if let Some(packed_refs) = git_path("packed-refs") {
        println!("cargo:rerun-if-changed={packed_refs}");
    }
}

/// Returns `git rev-parse --git-path <name>` as a trimmed string, or
/// `None` if git is unavailable, the command fails, or stdout is empty.
fn git_path(name: &str) -> Option<String> {
    let output = Command::new("git")
        .args(["rev-parse", "--git-path", name])
        .output()
        .ok()?;
    if !output.status.success() {
        return None;
    }
    let path = String::from_utf8_lossy(&output.stdout).trim().to_string();
    if path.is_empty() { None } else { Some(path) }
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
