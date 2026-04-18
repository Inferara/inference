//! Doctor command for the infs CLI.
//!
//! Verifies the installation health of the Inference toolchain and
//! reports any issues with suggested remediation steps.
//!
//! ## Usage
//!
//! ```bash
//! infs doctor
//! ```
//!
//! ## Checks Performed
//!
//! - Platform detection
//! - Toolchain directory existence
//! - Default toolchain configuration
//! - infc compiler binary presence
//!
//! ## Output Format (Public Contract)
//!
//! OUTPUT CONTRACT: check lines MUST match the regex
//!   `/^\s+\[(OK|WARN|FAIL)]\s+(.+?):\s+(.*)/`
//! Parsed by `editors/vscode/src/toolchain/doctor.ts`. Do not change the
//! line shape (leading whitespace, bracket status, colon-space, message)
//! without coordinating with the VS Code extension. A snapshot test at
//! `apps/infs/tests/cli_integration.rs::doctor_output_respects_vscode_check_line_contract`
//! enforces this invariant.

use anyhow::Result;

use crate::toolchain::ToolchainPaths;
use crate::toolchain::conflict::{
    detect_path_conflicts, enumerate_infc_on_path, format_doctor_conflict_warning,
    format_duplicate_path_warning,
};
use crate::toolchain::doctor::{DoctorCheckStatus, run_all_checks};

/// Executes the doctor command.
///
/// Runs all health checks and displays the results.
/// Returns an error when checks report failures so the caller gets a non-zero exit code.
#[allow(clippy::unnecessary_wraps, clippy::unused_async)]
pub async fn execute() -> Result<()> {
    println!("Checking Inference toolchain installation...");
    println!();

    let checks = run_all_checks();

    let mut has_errors = false;
    let mut has_warnings = false;

    for check in &checks {
        let prefix = check.prefix();
        println!("  {prefix} {}: {}", check.name, check.message);
        match check.status {
            DoctorCheckStatus::Ok => {}
            DoctorCheckStatus::Warning => has_warnings = true,
            DoctorCheckStatus::Error => has_errors = true,
        }
    }

    if let Ok(paths) = ToolchainPaths::new() {
        let conflicts = detect_path_conflicts(&paths.bin);
        if !conflicts.is_empty() {
            has_warnings = true;
            println!();
            // Print a regex-compliant [WARN] check line so the VS Code
            // extension keeps a structured entry for this warning, then
            // render the rest of the detail as plain indented continuation
            // (no leading `[`, so the regex filter skips them).
            let detail = format_doctor_conflict_warning(&conflicts);
            let summary = path_conflict_summary(&detail);
            println!("  [WARN] PATH conflict: {summary}");
            for line in detail {
                if !line.is_empty() {
                    println!("         {line}");
                }
            }
        }
    }

    // Duplicate infc binaries on PATH. The expanded block here mirrors the
    // detect_path_conflicts rendering for human readers. Header is plain
    // text (no `[WARN]` prefix) to stay outside the VS Code check-line
    // regex filter — duplicate-binary reporting is informational only.
    let on_path = enumerate_infc_on_path();
    if on_path.len() > 1 {
        has_warnings = true;
        println!();
        for line in format_duplicate_path_warning(&on_path) {
            println!("         {line}");
        }
    }

    println!();

    if has_errors {
        println!("Some checks failed. Run 'infs install' to install the toolchain.");
        anyhow::bail!("Doctor checks failed");
    } else if has_warnings {
        println!("Some warnings were found. The toolchain may work but could have issues.");
    } else {
        println!("All checks passed. The toolchain is ready to use.");
    }

    Ok(())
}

/// Reduces a multi-line `format_doctor_conflict_warning` block to a one-line
/// summary suitable for the `[WARN] PATH conflict: <summary>` check line.
///
/// The VS Code extension regex requires a non-empty message after the colon.
/// The first informational line ("'infc' resolves to ...") is the best fit:
/// it names the shadowing binary in a self-contained way. Falls back to a
/// generic phrase when `detail` is empty (should not happen in practice —
/// `format_doctor_conflict_warning` only returns an empty vec for empty
/// input, and the caller already guards on that).
fn path_conflict_summary(detail: &[String]) -> String {
    detail
        .iter()
        .find(|l| !l.trim().is_empty())
        .cloned()
        .unwrap_or_else(|| "managed toolchain shadowed by PATH".to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The VS Code extension parses check lines with this regex. The helper
    /// lives here so the test can verify the full `[WARN] PATH conflict: …`
    /// line round-trips through the regex.
    fn vscode_regex() -> regex::Regex {
        regex::Regex::new(r"^\s+\[(OK|WARN|FAIL)]\s+(.+?):\s+(.*)").unwrap()
    }

    #[test]
    fn conflict_summary_returns_first_nonempty_line() {
        let detail = vec![
            "'infc' resolves to /usr/local/bin/infc".to_string(),
            "  but managed version is at /home/u/.inference/bin/infc".to_string(),
        ];
        assert_eq!(path_conflict_summary(&detail), detail[0]);
    }

    #[test]
    fn conflict_summary_skips_leading_blank_lines() {
        let detail = vec![
            String::new(),
            "'infc' resolves to /usr/local/bin/infc".to_string(),
        ];
        assert_eq!(
            path_conflict_summary(&detail),
            "'infc' resolves to /usr/local/bin/infc"
        );
    }

    #[test]
    fn conflict_summary_falls_back_on_empty_input() {
        let detail: Vec<String> = vec![];
        assert!(!path_conflict_summary(&detail).is_empty());
    }

    #[test]
    fn path_conflict_header_line_matches_vscode_regex() {
        // Reconstruct the exact line emitted in execute() and assert it
        // passes the VS Code extension's check-line regex. Guards against
        // future edits that would silently break the extension.
        let detail = vec!["'infc' resolves to /usr/local/bin/infc".to_string()];
        let summary = path_conflict_summary(&detail);
        let line = format!("  [WARN] PATH conflict: {summary}");
        assert!(
            vscode_regex().is_match(&line),
            "header line violates VS Code contract: {line:?}"
        );
    }
}
