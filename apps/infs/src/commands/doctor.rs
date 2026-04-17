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
            println!("  [WARN] PATH conflict detected:");
            for line in format_doctor_conflict_warning(&conflicts) {
                if !line.is_empty() {
                    println!("         {line}");
                }
            }
        }
    }

    // Duplicate infc binaries on PATH. The single-line [WARN] above inside
    // check_infc keeps the VS Code regex contract; the expanded block here
    // mirrors the detect_path_conflicts rendering for human readers.
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
