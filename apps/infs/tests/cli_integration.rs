#![warn(clippy::pedantic)]

//! Integration tests for the infs unified CLI toolchain.
//!
//! These tests exercise the `infs` binary in a realistic environment by spawning
//! the compiled executable and validating its behavior through stdout, stderr,
//! and exit codes.
//!
//! ## Test Strategy
//!
//! The test suite verifies:
//!
//! ### Phase 1: Build Command
//!
//! 1. **Error handling**: File existence, required flags, no panics on error paths
//! 2. **Build command**: Parse, analyze, and codegen phases
//! 3. **Output generation**: WASM and Rocq file creation
//! 4. **Version and help**: CLI metadata display
//! 5. **Headless mode**: Display info without TUI
//! 6. **Compatibility**: Byte-identical output compared to `infc`
//!
//! ### Phase 2: Toolchain Management
//!
//! 7. **Install command**: Help display, error handling without network
//! 8. **Uninstall command**: Help display, nonexistent version handling
//! 9. **List command**: Success on empty state, appropriate messaging
//! 10. **Default command**: Help display, argument validation, error handling
//! 11. **Doctor command**: Health checks execution, output verification
//! 12. **Self update command**: Help display, subcommand validation, error handling
//!
//! ### Phase 3: Project Scaffolding
//!
//! 13. **New command**: Project creation, validation, directory structure
//! 14. **Init command**: In-place initialization, manifest generation
//!
//! ### Phase 4-5: Verify Command
//!
//! 15. **Verify command**: Help display, path validation, coqc availability check
//!
//! ### Phase 6: Run Command
//!
//! 16. **Run command**: Help display, path validation, wasmtime availability check
//!
//! ## Test Infrastructure
//!
//! - Uses `assert_cmd` for spawning and asserting on command execution
//! - Uses `assert_fs` for temporary filesystem operations
//! - Uses `predicates` for flexible output matching
//! - Test data located in `tests/test_data/` at workspace root
//!
//! ## Running Tests
//!
//! ```bash
//! cargo test -p infs
//! ```
//!
//! Tests run in parallel and use temporary directories to avoid interference.

use assert_cmd::prelude::*;
use assert_fs::prelude::*;
use predicates::prelude::*;
use std::process::Command;

/// Resolves the path to a test fixture file in the `tests/fixtures/` directory.
///
/// ## Path Resolution
///
/// ```text
/// env!("CARGO_MANIFEST_DIR")  // apps/infs/
///   .join("tests")
///   .join("fixtures")
///   .join(name)
/// ```
fn fixture_file(name: &str) -> std::path::PathBuf {
    std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("tests")
        .join("fixtures")
        .join(name)
}

/// Resolves the path to a test data file (alias for `fixture_file`).
fn example_file(name: &str) -> std::path::PathBuf {
    fixture_file(name)
}

/// Resolves the path to a codegen test data file (alias for `fixture_file`).
///
/// These files are simpler examples that successfully compile through all phases.
fn codegen_test_file(name: &str) -> std::path::PathBuf {
    fixture_file(name)
}

/// Returns a PATH that excludes wasmtime and coqc but preserves essential
/// system directories and runtime DLLs needed for the process to run.
///
/// On Windows, setting PATH="" prevents the process from finding essential DLLs
/// (like MinGW runtime when compiled with GNU toolchain), causing `STATUS_DLL_NOT_FOUND`.
/// This function uses `which` to find the exact directories containing the tools
/// and excludes only those, preserving all other paths.
///
/// On non-Windows platforms, we can safely use an empty PATH since there are no
/// DLL loading issues.
fn path_without_tools() -> String {
    // On non-Windows, empty PATH is safe and ensures tools aren't found
    #[cfg(not(windows))]
    {
        String::new()
    }

    // On Windows, we must preserve system directories and MinGW runtime DLLs
    #[cfg(windows)]
    {
        use std::path::PathBuf;

        let current_path = std::env::var("PATH").unwrap_or_default();

        // Find directories containing the tools we want to exclude
        let tool_dirs: Vec<PathBuf> = ["wasmtime", "coqc"]
            .iter()
            .filter_map(|tool| {
                which::which(tool)
                    .ok()
                    .and_then(|p| p.parent().map(|p| p.to_path_buf()))
            })
            .collect();

        current_path
            .split(';')
            .filter(|dir| {
                let dir_path = std::path::Path::new(dir);
                // Keep directories that don't contain any of the tools
                !tool_dirs.iter().any(|tool_dir| dir_path == tool_dir)
            })
            .collect::<Vec<_>>()
            .join(";")
    }
}

// =============================================================================
// Error Path Tests
// =============================================================================

/// Verifies that the build command fails gracefully when the input file doesn't exist.
///
/// **Expected behavior**: Exit with non-zero code and print "Path not found" to stderr.
#[test]
fn build_fails_when_file_missing() {
    let mut cmd = Command::new(assert_cmd::cargo::cargo_bin!("infs"));
    cmd.arg("build")
        .arg("this-file-does-not-exist.inf")
        .arg("--parse");

    cmd.assert().failure().stderr(
        predicate::str::contains("Path not found").or(predicate::str::contains("path not found")),
    );
}

/// Verifies that the build command requires at least one phase flag.
///
/// **Expected behavior**: Exit with non-zero code when no phase flags are provided,
/// with an error message explaining that at least one phase must be specified.
#[test]
fn build_fails_when_no_phase_selected() {
    let mut cmd = Command::new(assert_cmd::cargo::cargo_bin!("infs"));
    cmd.arg("build").arg(example_file("example.inf"));

    cmd.assert().failure().stderr(
        predicate::str::contains("At least one of --parse")
            .or(predicate::str::contains("at least one of --parse")),
    );
}

// =============================================================================
// Success Path Tests
// =============================================================================

/// Verifies that the parse phase can run successfully as a standalone operation.
///
/// **Expected behavior**: Exit with code 0 and print "Parsed: <filepath>" to stdout
/// when the source file is syntactically valid.
#[test]
fn build_parse_only_succeeds() {
    let Some(infc_path) = require_infc() else {
        return;
    };

    let mut cmd = Command::new(assert_cmd::cargo::cargo_bin!("infs"));
    cmd.env("INFC_PATH", &infc_path)
        .arg("build")
        .arg(example_file("example.inf"))
        .arg("--parse");

    cmd.assert()
        .success()
        .stdout(predicate::str::contains("Parsed:"));
}

/// Verifies that the analyze phase can run successfully.
///
/// **Note**: Uses `trivial.inf` which successfully passes type checking,
/// unlike `example.inf` which has type checker issues.
#[test]
fn build_analyze_succeeds() {
    let Some(infc_path) = require_infc() else {
        return;
    };

    let temp = assert_fs::TempDir::new().unwrap();
    let src = codegen_test_file("trivial.inf");
    let dest = temp.child("trivial.inf");
    std::fs::copy(&src, dest.path()).unwrap();

    let mut cmd = Command::new(assert_cmd::cargo::cargo_bin!("infs"));
    cmd.env("INFC_PATH", &infc_path)
        .current_dir(temp.path())
        .arg("build")
        .arg(dest.path())
        .arg("--analyze");

    cmd.assert()
        .success()
        .stdout(predicate::str::contains("Parsed:"))
        .stdout(predicate::str::contains("Analyzed:"));
}

/// Verifies that the codegen phase produces WASM output.
///
/// **Test setup**: Copies test input to a temporary directory to isolate output files.
///
/// **Expected behavior**: The compilation succeeds and produces a .wasm file.
#[test]
fn build_codegen_succeeds() {
    let Some(infc_path) = require_infc() else {
        return;
    };

    let temp = assert_fs::TempDir::new().unwrap();
    let src = codegen_test_file("trivial.inf");
    let dest = temp.child("trivial.inf");
    std::fs::copy(&src, dest.path()).unwrap();

    let mut cmd = Command::new(assert_cmd::cargo::cargo_bin!("infs"));
    cmd.env("INFC_PATH", &infc_path)
        .current_dir(temp.path())
        .arg("build")
        .arg(dest.path())
        .arg("--codegen")
        .arg("-o");

    cmd.assert()
        .success()
        .stdout(predicate::str::contains("WASM generated"));

    let wasm_output = temp.child("out").child("trivial.wasm");
    assert!(
        wasm_output.path().exists(),
        "Expected WASM file at: {:?}",
        wasm_output.path()
    );
}

/// Verifies that the full pipeline with Rocq output works correctly.
///
/// **Expected behavior**: The compilation succeeds and produces both .wasm and .v files.
#[test]
fn build_full_pipeline_with_v_output() {
    let Some(infc_path) = require_infc() else {
        return;
    };

    let temp = assert_fs::TempDir::new().unwrap();
    let src = codegen_test_file("trivial.inf");
    let dest = temp.child("trivial.inf");
    std::fs::copy(&src, dest.path()).unwrap();

    let mut cmd = Command::new(assert_cmd::cargo::cargo_bin!("infs"));
    cmd.env("INFC_PATH", &infc_path)
        .current_dir(temp.path())
        .arg("build")
        .arg(dest.path())
        .arg("--codegen")
        .arg("-o")
        .arg("-v");

    cmd.assert()
        .success()
        .stdout(predicate::str::contains("WASM generated"))
        .stdout(predicate::str::contains("V generated at:"));

    let wasm_output = temp.child("out").child("trivial.wasm");
    let v_output = temp.child("out").child("trivial.v");
    assert!(
        wasm_output.path().exists(),
        "Expected WASM file at: {:?}",
        wasm_output.path()
    );
    assert!(
        v_output.path().exists(),
        "Expected V file at: {:?}",
        v_output.path()
    );
}

// =============================================================================
// Version and Help Tests
// =============================================================================

/// Verifies that the `version` subcommand displays the correct version information.
///
/// **Expected behavior**: Exit with code 0 and print the version string to stdout.
#[test]
fn version_command_shows_version() {
    let mut cmd = Command::new(assert_cmd::cargo::cargo_bin!("infs"));
    cmd.arg("version");

    cmd.assert()
        .success()
        .stdout(predicate::str::contains("infs"))
        .stdout(predicate::str::contains(env!("CARGO_PKG_VERSION")));
}

/// Verifies that the `--version` flag displays the correct version information.
///
/// **Expected behavior**: Exit with code 0 and print the version string to stdout.
#[test]
fn version_flag_shows_version() {
    let mut cmd = Command::new(assert_cmd::cargo::cargo_bin!("infs"));
    cmd.arg("--version");

    cmd.assert()
        .success()
        .stdout(predicate::str::contains(env!("CARGO_PKG_VERSION")));
}

/// Verifies that the `--help` flag displays usage information.
///
/// **Expected behavior**: Exit with code 0 and print help text including available commands.
#[test]
fn help_shows_available_commands() {
    let mut cmd = Command::new(assert_cmd::cargo::cargo_bin!("infs"));
    cmd.arg("--help");

    cmd.assert()
        .success()
        .stdout(predicate::str::contains("build"))
        .stdout(predicate::str::contains("version"))
        .stdout(predicate::str::contains("--headless"));
}

// =============================================================================
// Headless Mode Tests
// =============================================================================

/// Verifies that headless mode without a command shows informational output.
///
/// **Expected behavior**: Exit with code 0 and display guidance about available commands.
#[test]
fn headless_mode_without_command_shows_info() {
    let mut cmd = Command::new(assert_cmd::cargo::cargo_bin!("infs"));
    cmd.arg("--headless");

    cmd.assert()
        .success()
        .stdout(predicate::str::contains("infs"))
        .stdout(predicate::str::contains("--help").or(predicate::str::contains("build")));
}

/// Verifies that the TUI is skipped when `INFS_NO_TUI` environment variable is set.
///
/// **Test setup**: Sets `INFS_NO_TUI=1` environment variable and runs infs without subcommand.
/// This is the dedicated way to disable the interactive TUI in non-interactive environments.
///
/// **Expected behavior**: Exit with code 0 and display informational output (same as headless mode).
/// The TUI should NOT be launched because the headless detection recognizes the `INFS_NO_TUI` setting.
#[test]
fn tui_detects_infs_no_tui_environment() {
    let mut cmd = Command::new(assert_cmd::cargo::cargo_bin!("infs"));
    cmd.env("INFS_NO_TUI", "1");

    cmd.assert()
        .success()
        .stdout(predicate::str::contains("infs"))
        .stdout(predicate::str::contains("--help").or(predicate::str::contains("build")));
}

// =============================================================================
// Byte-Identical Output Tests
// =============================================================================

/// Resolves the path to the `infc` binary in the workspace target directory.
///
/// This function locates the `infc` binary built by cargo. Since `infc` is in
/// a different package (inference-cli), we cannot use the `cargo_bin!` macro
/// directly and must construct the path manually.
fn infc_binary() -> std::path::PathBuf {
    let workspace_root = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .unwrap()
        .parent()
        .unwrap()
        .to_path_buf();

    let target_dir = workspace_root.join("target").join("debug");

    #[cfg(target_os = "windows")]
    let binary_name = "infc.exe";
    #[cfg(not(target_os = "windows"))]
    let binary_name = "infc";

    target_dir.join(binary_name)
}

/// Helper to check if infc binary is available and skip test if not.
/// Returns the path to infc if available.
#[allow(clippy::unnecessary_debug_formatting)]
fn require_infc() -> Option<std::path::PathBuf> {
    let infc_path = infc_binary();
    if infc_path.exists() {
        Some(infc_path)
    } else {
        eprintln!(
            "Skipping test: infc binary not found at {infc_path:?}. \
             Build with `cargo build -p inference-cli` first."
        );
        None
    }
}

/// Verifies that `infs build` produces byte-identical WASM output as `infc`.
///
/// This test ensures backward compatibility and correctness by comparing
/// the output from both CLI tools when compiling the same source file.
#[test]
fn build_produces_identical_wasm_as_infc() {
    let Some(infc_path) = require_infc() else {
        return;
    };

    let temp_new = assert_fs::TempDir::new().unwrap();
    let temp_legacy = assert_fs::TempDir::new().unwrap();

    let src = codegen_test_file("trivial.inf");

    let dest_new = temp_new.child("trivial.inf");
    std::fs::copy(&src, dest_new.path()).unwrap();

    let dest_legacy = temp_legacy.child("trivial.inf");
    std::fs::copy(&src, dest_legacy.path()).unwrap();

    let mut cmd_new = Command::new(assert_cmd::cargo::cargo_bin!("infs"));
    cmd_new
        .env("INFC_PATH", &infc_path)
        .current_dir(temp_new.path())
        .arg("build")
        .arg(dest_new.path())
        .arg("--codegen")
        .arg("-o");

    cmd_new.assert().success();

    let mut cmd_legacy = Command::new(&infc_path);
    cmd_legacy
        .current_dir(temp_legacy.path())
        .arg(dest_legacy.path())
        .arg("--parse")
        .arg("--codegen")
        .arg("-o");

    cmd_legacy.assert().success();

    let wasm_new = temp_new.child("out").child("trivial.wasm");
    let wasm_legacy = temp_legacy.child("out").child("trivial.wasm");

    assert!(wasm_new.path().exists(), "infs did not produce WASM output");
    assert!(
        wasm_legacy.path().exists(),
        "infc did not produce WASM output"
    );

    let new_bytes = std::fs::read(wasm_new.path()).expect("Failed to read infs WASM");
    let legacy_bytes = std::fs::read(wasm_legacy.path()).expect("Failed to read infc WASM");

    assert_eq!(
        new_bytes, legacy_bytes,
        "WASM output from infs and infc should be byte-identical"
    );
}

// =============================================================================
// Phase 2: Toolchain Management Command Tests
// =============================================================================

// -----------------------------------------------------------------------------
// Install Command Tests
// -----------------------------------------------------------------------------

/// Verifies that `infs install --help` displays the available options.
///
/// **Expected behavior**: Exit with code 0 and show version argument and usage.
#[test]
fn install_help_shows_options() {
    let mut cmd = Command::new(assert_cmd::cargo::cargo_bin!("infs"));
    cmd.arg("install").arg("--help");

    cmd.assert()
        .success()
        .stdout(predicate::str::contains("Install"))
        .stdout(predicate::str::contains("VERSION"));
}

/// Verifies that `infs install` shows a helpful error when network is unavailable.
///
/// **Test setup**: Uses an isolated `INFERENCE_HOME` directory to avoid affecting the system.
///
/// **Expected behavior**: Exit with non-zero code and print an error message
/// (not panic) when the manifest cannot be fetched.
#[test]
fn install_without_network_shows_error() {
    let temp = assert_fs::TempDir::new().unwrap();

    let mut cmd = Command::new(assert_cmd::cargo::cargo_bin!("infs"));
    cmd.env("INFERENCE_HOME", temp.path())
        .arg("install")
        .arg("0.0.0-nonexistent");

    cmd.assert()
        .failure()
        .stderr(predicate::str::contains("Error").or(predicate::str::contains("error")));
}

// -----------------------------------------------------------------------------
// Uninstall Command Tests
// -----------------------------------------------------------------------------

/// Verifies that `infs uninstall --help` displays the available options.
///
/// **Expected behavior**: Exit with code 0 and show version argument.
#[test]
fn uninstall_help_shows_options() {
    let mut cmd = Command::new(assert_cmd::cargo::cargo_bin!("infs"));
    cmd.arg("uninstall").arg("--help");

    cmd.assert()
        .success()
        .stdout(predicate::str::contains("Uninstall"))
        .stdout(predicate::str::contains("VERSION"));
}

/// Verifies that uninstalling a nonexistent version shows a helpful message.
///
/// **Test setup**: Uses an isolated `INFERENCE_HOME` directory with no toolchains installed.
///
/// **Expected behavior**: Exit with non-zero code and indicate the version is not installed.
#[test]
fn uninstall_nonexistent_shows_message() {
    let temp = assert_fs::TempDir::new().unwrap();

    let mut cmd = Command::new(assert_cmd::cargo::cargo_bin!("infs"));
    cmd.env("INFERENCE_HOME", temp.path())
        .arg("uninstall")
        .arg("0.0.0-nonexistent");

    cmd.assert()
        .failure()
        .stderr(predicate::str::contains("not installed"));
}

// -----------------------------------------------------------------------------
// List Command Tests
// -----------------------------------------------------------------------------

/// Verifies that `infs list` runs successfully even with no toolchains installed.
///
/// **Test setup**: Uses an isolated `INFERENCE_HOME` directory with no toolchains.
///
/// **Expected behavior**: Exit with code 0 (not a failure state).
#[test]
fn list_runs_successfully() {
    let temp = assert_fs::TempDir::new().unwrap();

    let mut cmd = Command::new(assert_cmd::cargo::cargo_bin!("infs"));
    cmd.env("INFERENCE_HOME", temp.path()).arg("list");

    cmd.assert().success();
}

/// Verifies that `infs list` shows appropriate message when no toolchains are installed.
///
/// **Test setup**: Uses an isolated `INFERENCE_HOME` directory with no toolchains.
///
/// **Expected behavior**: Exit with code 0 and display "No toolchains installed".
#[test]
fn list_shows_no_toolchains_message() {
    let temp = assert_fs::TempDir::new().unwrap();

    let mut cmd = Command::new(assert_cmd::cargo::cargo_bin!("infs"));
    cmd.env("INFERENCE_HOME", temp.path()).arg("list");

    cmd.assert()
        .success()
        .stdout(predicate::str::contains("No toolchains installed"));
}

// -----------------------------------------------------------------------------
// Versions Command Tests
// -----------------------------------------------------------------------------

/// Verifies that `infs versions --help` displays the available options.
///
/// **Expected behavior**: Exit with code 0 and show stable and json flags.
#[test]
fn versions_help_shows_options() {
    let mut cmd = Command::new(assert_cmd::cargo::cargo_bin!("infs"));
    cmd.arg("versions").arg("--help");

    cmd.assert()
        .success()
        .stdout(predicate::str::contains("List available"))
        .stdout(predicate::str::contains("--stable"))
        .stdout(predicate::str::contains("--json"));
}

/// Verifies that `infs versions` shows an error when no network is available.
///
/// **Test setup**: Uses a non-existent distribution server (`INFS_DIST_SERVER`) and
/// isolated `INFERENCE_HOME` to ensure no cached manifest is used.
///
/// **Expected behavior**: Exit with non-zero code and display a network error.
#[test]
fn versions_without_network_shows_error() {
    let temp = assert_fs::TempDir::new().unwrap();

    let mut cmd = Command::new(assert_cmd::cargo::cargo_bin!("infs"));
    cmd.env("INFS_DIST_SERVER", "http://localhost:1")
        .env("INFERENCE_HOME", temp.path())
        .arg("versions")
        .arg("--headless");

    cmd.assert().failure().stderr(
        predicate::str::contains("Failed")
            .or(predicate::str::contains("error"))
            .or(predicate::str::contains("Error")),
    );
}

/// Verifies that `infs versions --stable` flag is accepted.
///
/// **Test setup**: Uses a non-existent distribution server and isolated `INFERENCE_HOME`.
///
/// **Expected behavior**: The flag is parsed correctly (failure is from network, not flag parsing).
#[test]
fn versions_stable_flag_is_accepted() {
    let temp = assert_fs::TempDir::new().unwrap();

    let mut cmd = Command::new(assert_cmd::cargo::cargo_bin!("infs"));
    cmd.env("INFS_DIST_SERVER", "http://localhost:1")
        .env("INFERENCE_HOME", temp.path())
        .arg("versions")
        .arg("--stable")
        .arg("--headless");

    // Should fail due to network, not argument parsing
    cmd.assert().failure();
}

/// Verifies that `infs versions --json` flag is accepted.
///
/// **Test setup**: Uses a non-existent distribution server and isolated `INFERENCE_HOME`.
///
/// **Expected behavior**: The flag is parsed correctly (failure is from network, not flag parsing).
#[test]
fn versions_json_flag_is_accepted() {
    let temp = assert_fs::TempDir::new().unwrap();

    let mut cmd = Command::new(assert_cmd::cargo::cargo_bin!("infs"));
    cmd.env("INFS_DIST_SERVER", "http://localhost:1")
        .env("INFERENCE_HOME", temp.path())
        .arg("versions")
        .arg("--json")
        .arg("--headless");

    // Should fail due to network, not argument parsing
    cmd.assert().failure();
}

// -----------------------------------------------------------------------------
// Default Command Tests
// -----------------------------------------------------------------------------

/// Verifies that `infs default --help` displays the available options.
///
/// **Expected behavior**: Exit with code 0 and show version argument.
#[test]
fn default_help_shows_options() {
    let mut cmd = Command::new(assert_cmd::cargo::cargo_bin!("infs"));
    cmd.arg("default").arg("--help");

    cmd.assert()
        .success()
        .stdout(predicate::str::contains("Set the default"))
        .stdout(predicate::str::contains("VERSION"));
}

/// Verifies that `infs default` requires a version argument.
///
/// **Expected behavior**: Exit with non-zero code when no version is provided.
#[test]
fn default_requires_version_argument() {
    let mut cmd = Command::new(assert_cmd::cargo::cargo_bin!("infs"));
    cmd.arg("default");

    cmd.assert()
        .failure()
        .stderr(predicate::str::contains("VERSION").or(predicate::str::contains("required")));
}

/// Verifies that setting a nonexistent version as default shows a helpful error.
///
/// **Test setup**: Uses an isolated `INFERENCE_HOME` directory with no toolchains.
///
/// **Expected behavior**: Exit with non-zero code and indicate version is not installed
/// or does not exist (depending on whether the version exists in the release manifest).
#[test]
fn default_nonexistent_version_shows_error() {
    let temp = assert_fs::TempDir::new().unwrap();

    let mut cmd = Command::new(assert_cmd::cargo::cargo_bin!("infs"));
    cmd.env("INFERENCE_HOME", temp.path())
        .arg("default")
        .arg("0.0.0-nonexistent");

    cmd.assert().failure().stderr(
        predicate::str::contains("not installed").or(predicate::str::contains("does not exist")),
    );
}

// -----------------------------------------------------------------------------
// Doctor Command Tests
// -----------------------------------------------------------------------------

/// Verifies that `infs doctor` runs successfully even with no toolchains installed.
///
/// **Test setup**: Uses an isolated `INFERENCE_HOME` directory.
///
/// **Expected behavior**: Exit with code 0 (doctor reports issues but doesn't fail).
#[test]
fn doctor_runs_successfully() {
    let temp = assert_fs::TempDir::new().unwrap();

    let mut cmd = Command::new(assert_cmd::cargo::cargo_bin!("infs"));
    cmd.env("INFERENCE_HOME", temp.path()).arg("doctor");

    cmd.assert().success();
}

/// Verifies that `infs doctor` shows platform check in output.
///
/// **Test setup**: Uses an isolated `INFERENCE_HOME` directory.
///
/// **Expected behavior**: Output contains "Platform" check.
#[test]
fn doctor_shows_platform_check() {
    let temp = assert_fs::TempDir::new().unwrap();

    let mut cmd = Command::new(assert_cmd::cargo::cargo_bin!("infs"));
    cmd.env("INFERENCE_HOME", temp.path()).arg("doctor");

    cmd.assert()
        .success()
        .stdout(predicate::str::contains("Platform"));
}

/// Verifies that `infs doctor` shows multiple health checks.
///
/// **Test setup**: Uses an isolated `INFERENCE_HOME` directory.
///
/// **Expected behavior**: Output contains multiple check sections (Platform, Toolchain, etc.).
#[test]
fn doctor_shows_all_checks() {
    let temp = assert_fs::TempDir::new().unwrap();

    let mut cmd = Command::new(assert_cmd::cargo::cargo_bin!("infs"));
    cmd.env("INFERENCE_HOME", temp.path()).arg("doctor");

    cmd.assert()
        .success()
        .stdout(predicate::str::contains("Platform"))
        .stdout(predicate::str::contains("Toolchain directory"))
        .stdout(predicate::str::contains("Default toolchain"))
        .stdout(predicate::str::contains("inf-llc"))
        .stdout(predicate::str::contains("rust-lld"));
}

/// Verifies that `infs doctor` shows the checking message.
///
/// **Expected behavior**: Output contains the initial "Checking" message.
#[test]
fn doctor_shows_checking_message() {
    let temp = assert_fs::TempDir::new().unwrap();

    let mut cmd = Command::new(assert_cmd::cargo::cargo_bin!("infs"));
    cmd.env("INFERENCE_HOME", temp.path()).arg("doctor");

    cmd.assert()
        .success()
        .stdout(predicate::str::contains("Checking Inference toolchain"));
}

// -----------------------------------------------------------------------------
// Self Update Command Tests
// -----------------------------------------------------------------------------

/// Verifies that `infs self --help` displays the available subcommands.
///
/// **Expected behavior**: Exit with code 0 and show the update subcommand.
#[test]
fn self_help_shows_options() {
    let mut cmd = Command::new(assert_cmd::cargo::cargo_bin!("infs"));
    cmd.arg("self").arg("--help");

    cmd.assert()
        .success()
        .stdout(predicate::str::contains("update").or(predicate::str::contains("Update")));
}

/// Verifies that `infs self update --help` displays usage information.
///
/// **Expected behavior**: Exit with code 0 and show help text.
#[test]
fn self_update_help_shows_options() {
    let mut cmd = Command::new(assert_cmd::cargo::cargo_bin!("infs"));
    cmd.arg("self").arg("update").arg("--help");

    cmd.assert()
        .success()
        .stdout(predicate::str::contains("Update").or(predicate::str::contains("update")));
}

/// Verifies that `infs self update` shows a helpful error when network is unavailable.
///
/// **Test setup**: Uses an isolated `INFERENCE_HOME` directory and points to an invalid
/// distribution server via `INFS_DIST_SERVER` environment variable.
///
/// **Expected behavior**: Exit with non-zero code and print an error message
/// (not panic) when the manifest cannot be fetched.
#[test]
fn self_update_without_network_shows_error() {
    let temp = assert_fs::TempDir::new().unwrap();

    let mut cmd = Command::new(assert_cmd::cargo::cargo_bin!("infs"));
    cmd.env("INFERENCE_HOME", temp.path())
        .env("INFS_DIST_SERVER", "http://invalid-test-server.localhost")
        .arg("self")
        .arg("update");

    cmd.assert()
        .failure()
        .stderr(predicate::str::contains("Error").or(predicate::str::contains("error")));
}

/// Verifies that `INFS_DIST_SERVER` environment variable is used for manifest fetching.
///
/// **Test setup**: Sets `INFS_DIST_SERVER` to an invalid test server URL and runs install.
/// The cache TTL is set to 0 to force a network fetch.
///
/// **Expected behavior**: Exit with non-zero code and the error message should contain
/// the custom server URL, proving the environment variable was used.
#[test]
fn install_uses_custom_dist_server() {
    let temp = assert_fs::TempDir::new().unwrap();

    let mut cmd = Command::new(assert_cmd::cargo::cargo_bin!("infs"));
    cmd.env("INFERENCE_HOME", temp.path())
        .env("INFS_DIST_SERVER", "http://invalid-test-server.localhost")
        .arg("install");

    cmd.assert()
        .failure()
        .stderr(predicate::str::contains("invalid-test-server"));
}

/// Verifies that `infs self` without a subcommand shows an error.
///
/// **Expected behavior**: Exit with non-zero code when no subcommand is provided.
#[test]
fn self_requires_subcommand() {
    let mut cmd = Command::new(assert_cmd::cargo::cargo_bin!("infs"));
    cmd.arg("self");

    cmd.assert()
        .failure()
        .stderr(predicate::str::contains("subcommand").or(predicate::str::contains("required")));
}

// =============================================================================
// Phase 3: Project Scaffolding Command Tests
// =============================================================================

// -----------------------------------------------------------------------------
// New Command Tests
// -----------------------------------------------------------------------------

/// Verifies that `infs new --help` displays the available options.
///
/// **Expected behavior**: Exit with code 0 and show NAME argument, --no-git flag, and path option.
#[test]
fn new_help_shows_options() {
    let mut cmd = Command::new(assert_cmd::cargo::cargo_bin!("infs"));
    cmd.arg("new").arg("--help");

    cmd.assert()
        .success()
        .stdout(predicate::str::contains("NAME"))
        .stdout(predicate::str::contains("--no-git"))
        .stdout(predicate::str::contains("PATH").or(predicate::str::contains("path")));
}

/// Verifies that `infs new` requires a name argument.
///
/// **Expected behavior**: Exit with non-zero code when no name is provided.
#[test]
fn new_requires_name_argument() {
    let mut cmd = Command::new(assert_cmd::cargo::cargo_bin!("infs"));
    cmd.arg("new");

    cmd.assert()
        .failure()
        .stderr(predicate::str::contains("NAME").or(predicate::str::contains("required")));
}

/// Verifies that `infs new` creates the complete project structure.
///
/// **Test setup**: Uses a temporary directory and --no-git to avoid git dependency.
///
/// **Expected behavior**: Creates Inference.toml, src/main.inf, .gitignore, tests/, and proofs/.
#[test]
fn new_creates_project_structure() {
    let temp = assert_fs::TempDir::new().unwrap();

    let mut cmd = Command::new(assert_cmd::cargo::cargo_bin!("infs"));
    cmd.current_dir(temp.path())
        .arg("new")
        .arg("myproject")
        .arg("--no-git");

    cmd.assert()
        .success()
        .stdout(predicate::str::contains("Created project"));

    let project_dir = temp.child("myproject");
    assert!(
        project_dir.path().exists(),
        "Project directory should exist"
    );
    assert!(
        project_dir.child("Inference.toml").path().exists(),
        "Inference.toml should exist"
    );
    assert!(
        project_dir.child("src").child("main.inf").path().exists(),
        "src/main.inf should exist"
    );
    // With --no-git, .gitignore should NOT be created
    assert!(
        !project_dir.child(".gitignore").path().exists(),
        ".gitignore should NOT exist with --no-git"
    );
    assert!(
        project_dir.child("tests").path().exists(),
        "tests/ directory should exist"
    );
    assert!(
        project_dir.child("proofs").path().exists(),
        "proofs/ directory should exist"
    );
}

/// Verifies that `infs new` validates project names and rejects invalid ones.
///
/// **Test cases**:
/// - Names starting with numbers (e.g., "123invalid")
/// - Reserved keywords (e.g., "fn")
///
/// **Expected behavior**: Exit with non-zero code and display an error message.
#[test]
fn new_validates_project_name() {
    let temp = assert_fs::TempDir::new().unwrap();

    let mut cmd = Command::new(assert_cmd::cargo::cargo_bin!("infs"));
    cmd.current_dir(temp.path())
        .arg("new")
        .arg("123invalid")
        .arg("--no-git");

    cmd.assert()
        .failure()
        .stderr(predicate::str::contains("start with"));

    let mut cmd = Command::new(assert_cmd::cargo::cargo_bin!("infs"));
    cmd.current_dir(temp.path())
        .arg("new")
        .arg("fn")
        .arg("--no-git");

    cmd.assert()
        .failure()
        .stderr(predicate::str::contains("reserved"));
}

/// Verifies that `infs new` fails when the target directory already exists.
///
/// **Test setup**: Pre-creates a directory with the same name.
///
/// **Expected behavior**: Exit with non-zero code and indicate directory exists.
#[test]
fn new_fails_if_directory_exists() {
    let temp = assert_fs::TempDir::new().unwrap();
    let existing_dir = temp.child("existing_project");
    std::fs::create_dir_all(existing_dir.path()).unwrap();

    let mut cmd = Command::new(assert_cmd::cargo::cargo_bin!("infs"));
    cmd.current_dir(temp.path())
        .arg("new")
        .arg("existing_project")
        .arg("--no-git");

    cmd.assert()
        .failure()
        .stderr(predicate::str::contains("already exists"));
}

/// Verifies that `infs new` generates a valid Inference.toml manifest.
///
/// **Expected behavior**: The manifest contains the correct project name and version.
#[test]
fn new_generates_valid_manifest() {
    let temp = assert_fs::TempDir::new().unwrap();

    let mut cmd = Command::new(assert_cmd::cargo::cargo_bin!("infs"));
    cmd.current_dir(temp.path())
        .arg("new")
        .arg("test_manifest_project")
        .arg("--no-git");

    cmd.assert().success();

    let manifest_path = temp.child("test_manifest_project").child("Inference.toml");
    let manifest_content =
        std::fs::read_to_string(manifest_path.path()).expect("Failed to read Inference.toml");

    assert!(
        manifest_content.contains("name = \"test_manifest_project\""),
        "Manifest should contain project name"
    );
    assert!(
        manifest_content.contains("version = \"0.1.0\""),
        "Manifest should contain default version"
    );
}

/// Verifies that `infs new --no-git` skips git initialization.
///
/// **Expected behavior**: Project is created successfully without .git directory.
#[test]
fn new_with_no_git_flag() {
    let temp = assert_fs::TempDir::new().unwrap();

    let mut cmd = Command::new(assert_cmd::cargo::cargo_bin!("infs"));
    cmd.current_dir(temp.path())
        .arg("new")
        .arg("nogit_project")
        .arg("--no-git");

    cmd.assert().success();

    let project_dir = temp.child("nogit_project");
    assert!(
        project_dir.path().exists(),
        "Project directory should exist"
    );
    assert!(
        !project_dir.child(".git").path().exists(),
        ".git directory should not exist when --no-git is used"
    );
}

// -----------------------------------------------------------------------------
// Init Command Tests
// -----------------------------------------------------------------------------

/// Verifies that `infs init --help` displays the available options.
///
/// **Expected behavior**: Exit with code 0 and show the name option.
#[test]
fn init_help_shows_options() {
    let mut cmd = Command::new(assert_cmd::cargo::cargo_bin!("infs"));
    cmd.arg("init").arg("--help");

    cmd.assert()
        .success()
        .stdout(predicate::str::contains("NAME").or(predicate::str::contains("name")));
}

/// Verifies that `infs init` creates the manifest and source files.
///
/// **Test setup**: Uses a temporary directory.
///
/// **Expected behavior**: Creates Inference.toml and src/main.inf.
#[test]
fn init_creates_manifest() {
    let temp = assert_fs::TempDir::new().unwrap();

    let mut cmd = Command::new(assert_cmd::cargo::cargo_bin!("infs"));
    cmd.current_dir(temp.path())
        .arg("init")
        .arg("init_test_project");

    cmd.assert()
        .success()
        .stdout(predicate::str::contains("Initialized"));

    assert!(
        temp.child("Inference.toml").path().exists(),
        "Inference.toml should exist"
    );
    assert!(
        temp.child("src").child("main.inf").path().exists(),
        "src/main.inf should exist"
    );
}

/// Verifies that `infs init` uses a custom name when provided.
///
/// **Expected behavior**: The manifest contains the specified project name.
#[test]
fn init_uses_custom_name() {
    let temp = assert_fs::TempDir::new().unwrap();

    let mut cmd = Command::new(assert_cmd::cargo::cargo_bin!("infs"));
    cmd.current_dir(temp.path())
        .arg("init")
        .arg("custom_name_project");

    cmd.assert().success();

    let manifest_content = std::fs::read_to_string(temp.child("Inference.toml").path())
        .expect("Failed to read Inference.toml");

    assert!(
        manifest_content.contains("name = \"custom_name_project\""),
        "Manifest should contain the custom project name"
    );
}

/// Verifies that `infs init` fails when Inference.toml already exists.
///
/// **Test setup**: Pre-creates an Inference.toml file.
///
/// **Expected behavior**: Exit with non-zero code and indicate manifest exists.
#[test]
fn init_fails_if_manifest_exists() {
    let temp = assert_fs::TempDir::new().unwrap();
    std::fs::write(
        temp.child("Inference.toml").path(),
        "[package]\nname = \"existing\"",
    )
    .expect("Failed to create existing manifest");

    let mut cmd = Command::new(assert_cmd::cargo::cargo_bin!("infs"));
    cmd.current_dir(temp.path()).arg("init").arg("newproject");

    cmd.assert()
        .failure()
        .stderr(predicate::str::contains("already exists"));
}

/// Verifies that `infs init` validates custom project names.
///
/// **Expected behavior**: Exit with non-zero code for reserved keywords.
#[test]
fn init_validates_custom_name() {
    let temp = assert_fs::TempDir::new().unwrap();

    let mut cmd = Command::new(assert_cmd::cargo::cargo_bin!("infs"));
    cmd.current_dir(temp.path()).arg("init").arg("fn");

    cmd.assert()
        .failure()
        .stderr(predicate::str::contains("reserved"));
}

/// Verifies that `infs init` uses the directory name as default project name.
///
/// **Test setup**: Creates a directory with a specific name and runs init without arguments.
///
/// **Expected behavior**: The manifest contains the directory name as project name.
#[test]
fn init_uses_directory_name_as_default() {
    let temp = assert_fs::TempDir::new().unwrap();
    let project_dir = temp.child("my_default_name_project");
    std::fs::create_dir_all(project_dir.path()).expect("Failed to create project directory");

    let mut cmd = Command::new(assert_cmd::cargo::cargo_bin!("infs"));
    cmd.current_dir(project_dir.path()).arg("init");

    cmd.assert().success();

    let manifest_content = std::fs::read_to_string(project_dir.child("Inference.toml").path())
        .expect("Failed to read Inference.toml");

    assert!(
        manifest_content.contains("name = \"my_default_name_project\""),
        "Manifest should contain the directory name as project name"
    );
}

// -----------------------------------------------------------------------------
// File Permission and Error Handling Tests
// -----------------------------------------------------------------------------

/// Verifies that file permissions are handled correctly for created project files.
///
/// **Test setup**: Creates a project with git enabled and checks file permissions.
///
/// **Expected behavior**: All generated files should be readable.
#[test]
fn new_creates_files_with_correct_permissions() {
    let temp = assert_fs::TempDir::new().unwrap();

    let mut cmd = Command::new(assert_cmd::cargo::cargo_bin!("infs"));
    cmd.current_dir(temp.path())
        .arg("new")
        .arg("permission_test_project");
    // Note: not using --no-git so .gitignore will be created

    cmd.assert().success();

    let project_dir = temp.child("permission_test_project");

    // Verify all files are readable
    let manifest = project_dir.child("Inference.toml");
    assert!(
        std::fs::read_to_string(manifest.path()).is_ok(),
        "Inference.toml should be readable"
    );

    let main_inf = project_dir.child("src").child("main.inf");
    assert!(
        std::fs::read_to_string(main_inf.path()).is_ok(),
        "src/main.inf should be readable"
    );

    let gitignore = project_dir.child(".gitignore");
    assert!(
        std::fs::read_to_string(gitignore.path()).is_ok(),
        ".gitignore should be readable"
    );
}

/// Verifies that `infs new` handles permission denied errors gracefully.
///
/// **Test setup**: On Unix, creates a read-only directory where project creation should fail.
/// Uses an explicit path argument to create the project in the read-only directory.
///
/// **Expected behavior**: Exit with non-zero code and display a meaningful error message.
#[test]
#[cfg(unix)]
fn new_handles_permission_denied() {
    use std::os::unix::fs::PermissionsExt;

    let temp = assert_fs::TempDir::new().unwrap();
    let readonly_dir = temp.child("readonly_parent");
    std::fs::create_dir_all(readonly_dir.path()).expect("Failed to create directory");

    // Make the directory read-only (no write permission)
    let mut perms = std::fs::metadata(readonly_dir.path())
        .expect("Failed to get metadata")
        .permissions();
    perms.set_mode(0o555);
    std::fs::set_permissions(readonly_dir.path(), perms).expect("Failed to set permissions");

    // Run from temp directory but try to create project in the read-only subdirectory
    let mut cmd = Command::new(assert_cmd::cargo::cargo_bin!("infs"));
    cmd.current_dir(temp.path())
        .arg("new")
        .arg("should_fail_project")
        .arg(readonly_dir.path())
        .arg("--no-git");

    cmd.assert().failure().stderr(
        predicate::str::contains("Failed")
            .or(predicate::str::contains("Permission denied"))
            .or(predicate::str::contains("permission")),
    );

    // Restore permissions for cleanup
    let mut perms = std::fs::metadata(readonly_dir.path())
        .expect("Failed to get metadata")
        .permissions();
    perms.set_mode(0o755);
    std::fs::set_permissions(readonly_dir.path(), perms).expect("Failed to restore permissions");
}

/// Verifies that `infs init` handles permission denied errors gracefully.
///
/// **Test setup**: On Unix, we test that init properly reports errors when it cannot
/// write files. We do this by making the target directory read-only after creation.
///
/// **Expected behavior**: Exit with non-zero code and display a meaningful error message.
#[test]
#[cfg(unix)]
fn init_handles_permission_denied() {
    use std::os::unix::fs::PermissionsExt;

    let temp = assert_fs::TempDir::new().unwrap();
    let work_dir = temp.child("work_dir");
    std::fs::create_dir_all(work_dir.path()).expect("Failed to create directory");

    // Create a read-only subdirectory that we'll try to init
    let readonly_dir = work_dir.child("readonly_init_dir");
    std::fs::create_dir_all(readonly_dir.path()).expect("Failed to create directory");

    // Make the directory read-only (no write permission) - execute bit needed to cd into it
    let mut perms = std::fs::metadata(readonly_dir.path())
        .expect("Failed to get metadata")
        .permissions();
    perms.set_mode(0o555);
    std::fs::set_permissions(readonly_dir.path(), perms).expect("Failed to set permissions");

    // Run from work_dir but use -C or pass path to init in the readonly dir
    // Note: infs init takes name as positional arg, not path, so we need to cd into it
    // The issue is that cd into a read-only dir works, but writing fails
    let mut cmd = Command::new(assert_cmd::cargo::cargo_bin!("infs"));
    cmd.current_dir(readonly_dir.path())
        .arg("init")
        .arg("should_fail");

    cmd.assert().failure().stderr(
        predicate::str::contains("Failed")
            .or(predicate::str::contains("Permission denied"))
            .or(predicate::str::contains("permission")),
    );

    // Restore permissions for cleanup
    let mut perms = std::fs::metadata(readonly_dir.path())
        .expect("Failed to get metadata")
        .permissions();
    perms.set_mode(0o755);
    std::fs::set_permissions(readonly_dir.path(), perms).expect("Failed to restore permissions");
}

// -----------------------------------------------------------------------------
// Run Command Tests
// -----------------------------------------------------------------------------

/// Verifies that `infs run --help` displays the available options.
///
/// **Expected behavior**: Exit with code 0 and show path argument and usage.
#[test]
fn run_help_shows_options() {
    let mut cmd = Command::new(assert_cmd::cargo::cargo_bin!("infs"));
    cmd.arg("run").arg("--help");

    cmd.assert()
        .success()
        .stdout(predicate::str::contains("PATH").or(predicate::str::contains("path")))
        .stdout(predicate::str::contains("Run").or(predicate::str::contains("run")));
}

/// Verifies that `infs run` requires a path argument.
///
/// **Expected behavior**: Exit with non-zero code when no path is provided.
#[test]
fn run_requires_path_argument() {
    let mut cmd = Command::new(assert_cmd::cargo::cargo_bin!("infs"));
    cmd.arg("run");

    cmd.assert()
        .failure()
        .stderr(predicate::str::contains("PATH").or(predicate::str::contains("required")));
}

/// Verifies that `infs run` fails when source file doesn't exist.
///
/// **Expected behavior**: Exit with non-zero code and print "Path not found".
#[test]
fn run_fails_when_file_missing() {
    let mut cmd = Command::new(assert_cmd::cargo::cargo_bin!("infs"));
    cmd.arg("run").arg("this-file-does-not-exist.inf");

    cmd.assert().failure().stderr(
        predicate::str::contains("Path not found").or(predicate::str::contains("path not found")),
    );
}

/// Verifies that `infs run` shows a helpful error when wasmtime is not available.
///
/// **Test setup**: Uses PATH override to ensure wasmtime is not found.
///
/// **Expected behavior**: Exit with non-zero code and display installation instructions.
#[test]
fn run_shows_wasmtime_not_found_message() {
    let temp = assert_fs::TempDir::new().unwrap();
    let src = codegen_test_file("trivial.inf");
    let dest = temp.child("trivial.inf");
    std::fs::copy(&src, dest.path()).unwrap();

    let mut cmd = Command::new(assert_cmd::cargo::cargo_bin!("infs"));
    cmd.current_dir(temp.path())
        .env("PATH", path_without_tools())
        .arg("run")
        .arg(dest.path());

    cmd.assert()
        .failure()
        .stderr(predicate::str::contains("wasmtime not found"))
        .stderr(
            predicate::str::contains("wasmtime.dev")
                .or(predicate::str::contains("brew install wasmtime")),
        );
}

/// Verifies that `infs run` accepts trailing arguments for the WASM program.
///
/// **Expected behavior**: The help shows that arguments can be passed to the WASM program.
#[test]
fn run_accepts_trailing_args() {
    let mut cmd = Command::new(assert_cmd::cargo::cargo_bin!("infs"));
    cmd.arg("run").arg("--help");

    cmd.assert()
        .success()
        .stdout(predicate::str::contains("ARGS").or(predicate::str::contains("args")));
}

// =============================================================================
// Conditional Tests: Full Workflow (Require External Tools)
// =============================================================================

/// Helper function to check if wasmtime is available in PATH.
fn is_wasmtime_available() -> bool {
    std::process::Command::new("wasmtime")
        .arg("--version")
        .output()
        .map(|o| o.status.success())
        .unwrap_or(false)
}

/// Verifies full `infs run` workflow with wasmtime.
///
/// **Prerequisites**: wasmtime must be installed and in PATH, and infc must be built.
///
/// **Test setup**: Compiles a trivial Inference program and runs it.
///
/// **Expected behavior**: Program compiles, runs with wasmtime, exits successfully.
#[test]
fn run_full_workflow_with_wasmtime() {
    if !is_wasmtime_available() {
        eprintln!("Skipping test: wasmtime not available");
        return;
    }

    let Some(infc_path) = require_infc() else {
        return;
    };

    let temp = assert_fs::TempDir::new().unwrap();
    let src = codegen_test_file("trivial.inf");
    let dest = temp.child("trivial.inf");
    std::fs::copy(&src, dest.path()).unwrap();

    let mut cmd = Command::new(assert_cmd::cargo::cargo_bin!("infs"));
    cmd.env("INFC_PATH", &infc_path)
        .current_dir(temp.path())
        .arg("run")
        .arg(dest.path())
        .arg("--entry-point")
        .arg("hello_world");

    cmd.assert().success();
}

/// Returns the path to the syntax_`error.inf` test file.
fn syntax_error_file() -> std::path::PathBuf {
    fixture_file("syntax_error.inf")
}

/// **Expected behavior**: Exit with non-zero code, meaningful error message, NO PANIC.
#[test]
fn run_fails_gracefully_on_syntax_error() {
    let syntax_error_file = syntax_error_file();

    let Some(infc_path) = require_infc() else {
        return;
    };

    let mut cmd = Command::new(assert_cmd::cargo::cargo_bin!("infs"));
    cmd.env("INFC_PATH", &infc_path)
        .arg("run")
        .arg(&syntax_error_file);

    cmd.assert().failure().stderr(
        predicate::str::contains("error")
            .or(predicate::str::contains("Error"))
            .or(predicate::str::contains("Syntax")),
    );
}

/// **Expected behavior**: Exit with non-zero code, meaningful error message, NO PANIC.
#[test]
fn build_fails_gracefully_on_syntax_error() {
    let syntax_error_file = syntax_error_file();

    let Some(infc_path) = require_infc() else {
        return;
    };

    let mut cmd = Command::new(assert_cmd::cargo::cargo_bin!("infs"));
    cmd.env("INFC_PATH", &infc_path)
        .arg("build")
        .arg(&syntax_error_file)
        .arg("--parse");

    cmd.assert().failure().stderr(
        predicate::str::contains("error")
            .or(predicate::str::contains("Error"))
            .or(predicate::str::contains("Syntax")),
    );
}

// =============================================================================
// Helper Functions for QA Test Files
// =============================================================================

/// Returns the path to `type_error.inf` test file.
#[allow(dead_code)]
fn type_error_file() -> std::path::PathBuf {
    example_file("type_error.inf")
}

/// Returns the path to `empty.inf` test file.
fn empty_file() -> std::path::PathBuf {
    example_file("empty.inf")
}

/// Returns the path to `uzumaki.inf` test file.
#[allow(dead_code)]
fn uzumaki_file() -> std::path::PathBuf {
    example_file("uzumaki.inf")
}

/// Returns the path to `forall_test.inf` test file.
#[allow(dead_code)]
fn forall_test_file() -> std::path::PathBuf {
    example_file("forall_test.inf")
}

/// Returns the path to `exists_test.inf` test file.
#[allow(dead_code)]
fn exists_test_file() -> std::path::PathBuf {
    example_file("exists_test.inf")
}

/// Returns the path to `assume_test.inf` test file.
#[allow(dead_code)]
fn assume_test_file() -> std::path::PathBuf {
    example_file("assume_test.inf")
}

/// Returns the path to `unique_test.inf` test file.
#[allow(dead_code)]
fn unique_test_file() -> std::path::PathBuf {
    example_file("unique_test.inf")
}

// =============================================================================
// QA Test Coverage: Migrated from docs/qa-test-suite.md
// =============================================================================

/// QA: TC-2.10 - Verify empty file is handled gracefully.
///
/// **Expected behavior**: Exit with code 0 or specific empty-file error, no crash/panic.
#[test]
fn build_handles_empty_file() {
    let Some(infc_path) = require_infc() else {
        return;
    };

    let mut cmd = Command::new(assert_cmd::cargo::cargo_bin!("infs"));
    cmd.env("INFC_PATH", &infc_path)
        .arg("build")
        .arg(empty_file())
        .arg("--parse");

    // Empty file should either succeed or fail gracefully (no panic)
    let output = cmd.output().expect("Failed to execute command");

    // Check that stderr doesn't contain panic messages
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        !stderr.contains("panic") && !stderr.contains("RUST_BACKTRACE"),
        "Empty file should not cause panic. Got: {stderr}"
    );
}

/// QA: TC-5.9c - Verify `infs init` does not overwrite existing .gitignore.
///
/// **Expected behavior**: Exit with code 0, .gitignore contains original content.
#[test]
fn init_preserves_existing_gitignore() {
    let temp = assert_fs::TempDir::new().unwrap();

    // Create .git directory to trigger git file creation
    std::fs::create_dir_all(temp.child(".git").path()).unwrap();

    // Create custom .gitignore with specific content
    let gitignore = temp.child(".gitignore");
    std::fs::write(gitignore.path(), "custom-ignore-pattern\n").unwrap();

    let mut cmd = Command::new(assert_cmd::cargo::cargo_bin!("infs"));
    cmd.current_dir(temp.path()).arg("init").arg("test_project");

    cmd.assert().success();

    // Verify .gitignore still contains original content
    let content = std::fs::read_to_string(gitignore.path()).expect("Failed to read .gitignore");

    assert!(
        content.contains("custom-ignore-pattern"),
        ".gitignore should preserve existing content. Got: {content}"
    );
}

/// QA: TC-10.5 - Verify graceful handling of invalid `INFC_PATH`.
///
/// **Expected behavior**: Exit with non-zero code, clear error message.
#[test]
fn build_with_invalid_infc_path_shows_error() {
    let mut cmd = Command::new(assert_cmd::cargo::cargo_bin!("infs"));
    cmd.env("INFC_PATH", "/nonexistent/path/to/infc")
        .arg("build")
        .arg(example_file("example.inf"))
        .arg("--parse");

    cmd.assert().failure().stderr(
        predicate::str::contains("not found")
            .or(predicate::str::contains("No such file"))
            .or(predicate::str::contains("does not exist"))
            .or(predicate::str::contains("compiler not found")),
    );
}

/// QA: TC-12.3 - Verify recovery from corrupted toolchain metadata.
///
/// **Expected behavior**: No crash, warning about corrupted metadata.
#[test]
fn list_handles_corrupted_metadata() {
    let temp = assert_fs::TempDir::new().unwrap();

    // Create toolchain directory structure with corrupted metadata
    let toolchain_dir = temp.child("toolchains").child("0.1.0");
    std::fs::create_dir_all(toolchain_dir.path()).unwrap();

    // Create a corrupted .metadata.json file
    std::fs::write(
        toolchain_dir.child(".metadata.json").path(),
        "{ invalid json content",
    )
    .unwrap();

    let mut cmd = Command::new(assert_cmd::cargo::cargo_bin!("infs"));
    cmd.env("INFERENCE_HOME", temp.path()).arg("list");

    // Should not panic, may show warning or skip corrupted entry
    let output = cmd.output().expect("Failed to execute command");
    let stderr = String::from_utf8_lossy(&output.stderr);

    assert!(
        !stderr.contains("panic") && !stderr.contains("RUST_BACKTRACE"),
        "Corrupted metadata should not cause panic. Got: {stderr}"
    );
}

/// QA: TC-13.1 - Verify compilation of uzumaki operator (@).
///
/// **Expected behavior**: Exit code 0, WASM binary generated.
#[test]
fn build_compiles_uzumaki_operator() {
    let Some(infc_path) = require_infc() else {
        return;
    };

    let temp = assert_fs::TempDir::new().unwrap();
    let src = codegen_test_file("nondet.inf");
    let dest = temp.child("nondet.inf");
    std::fs::copy(&src, dest.path()).unwrap();

    let mut cmd = Command::new(assert_cmd::cargo::cargo_bin!("infs"));
    cmd.env("INFC_PATH", &infc_path)
        .current_dir(temp.path())
        .arg("build")
        .arg(dest.path())
        .arg("--codegen")
        .arg("-o");

    cmd.assert()
        .success()
        .stdout(predicate::str::contains("WASM generated"));

    let wasm_output = temp.child("out").child("nondet.wasm");
    assert!(
        wasm_output.path().exists(),
        "Expected WASM file at: {:?}",
        wasm_output.path()
    );
}

/// QA: TC-13.6 - Verify compilation of file with all non-det features.
///
/// **Expected behavior**: Exit code 0, both WASM and Rocq outputs generated.
#[test]
fn build_compiles_all_nondet_features() {
    let Some(infc_path) = require_infc() else {
        return;
    };

    let temp = assert_fs::TempDir::new().unwrap();
    let src = codegen_test_file("nondet.inf");
    let dest = temp.child("nondet.inf");
    std::fs::copy(&src, dest.path()).unwrap();

    let mut cmd = Command::new(assert_cmd::cargo::cargo_bin!("infs"));
    cmd.env("INFC_PATH", &infc_path)
        .current_dir(temp.path())
        .arg("build")
        .arg(dest.path())
        .arg("--codegen")
        .arg("-o")
        .arg("-v");

    cmd.assert()
        .success()
        .stdout(predicate::str::contains("WASM generated"))
        .stdout(predicate::str::contains("V generated"));

    let wasm_output = temp.child("out").child("nondet.wasm");
    let v_output = temp.child("out").child("nondet.v");
    assert!(
        wasm_output.path().exists(),
        "Expected WASM file at: {:?}",
        wasm_output.path()
    );
    assert!(
        v_output.path().exists(),
        "Expected V file at: {:?}",
        v_output.path()
    );
}

/// QA: TC-2.11 - Verify phases execute in correct order regardless of flag order.
///
/// **Expected behavior**: Exit code 0, phases execute in order: parse -> analyze -> codegen.
#[test]
fn build_enforces_phase_order() {
    let Some(infc_path) = require_infc() else {
        return;
    };

    let temp = assert_fs::TempDir::new().unwrap();
    let src = codegen_test_file("trivial.inf");
    let dest = temp.child("trivial.inf");
    std::fs::copy(&src, dest.path()).unwrap();

    // Run with flags in reverse order: --codegen --parse -o
    let mut cmd = Command::new(assert_cmd::cargo::cargo_bin!("infs"));
    cmd.env("INFC_PATH", &infc_path)
        .current_dir(temp.path())
        .arg("build")
        .arg(dest.path())
        .arg("--codegen")
        .arg("--parse")
        .arg("-o");

    cmd.assert()
        .success()
        .stdout(predicate::str::contains("Parsed:"))
        .stdout(predicate::str::contains("WASM generated"));

    let wasm_output = temp.child("out").child("trivial.wasm");
    assert!(
        wasm_output.path().exists(),
        "Expected WASM file at: {:?}",
        wasm_output.path()
    );
}

/// QA: TC-1.6 - Verify graceful error on unknown subcommand.
///
/// **Expected behavior**: Exit code non-zero, error message indicates unknown subcommand.
#[test]
fn unknown_subcommand_shows_error() {
    let mut cmd = Command::new(assert_cmd::cargo::cargo_bin!("infs"));
    cmd.arg("unknown-command");

    cmd.assert().failure().stderr(
        predicate::str::contains("unrecognized")
            .or(predicate::str::contains("not found"))
            .or(predicate::str::contains("unknown")),
    );
}

/// QA: TC-1.9 - Verify --version flag and version subcommand produce consistent output.
///
/// **Expected behavior**: Both commands exit with code 0, both show same version.
#[test]
fn version_flag_and_subcommand_are_consistent() {
    let mut cmd_flag = Command::new(assert_cmd::cargo::cargo_bin!("infs"));
    cmd_flag.arg("--version");

    let mut cmd_subcmd = Command::new(assert_cmd::cargo::cargo_bin!("infs"));
    cmd_subcmd.arg("version");

    let flag_output = cmd_flag.output().expect("Failed to run --version");
    let subcmd_output = cmd_subcmd.output().expect("Failed to run version");

    assert!(flag_output.status.success(), "--version should succeed");
    assert!(
        subcmd_output.status.success(),
        "version subcommand should succeed"
    );

    let flag_stdout = String::from_utf8_lossy(&flag_output.stdout);
    let subcmd_stdout = String::from_utf8_lossy(&subcmd_output.stdout);

    // Both should contain the version number
    let version = env!("CARGO_PKG_VERSION");
    assert!(
        flag_stdout.contains(version),
        "--version should contain {version}"
    );
    assert!(
        subcmd_stdout.contains(version),
        "version subcommand should contain {version}"
    );
}

/// QA: TC-12.1 - Verify error when output directory not writable.
///
/// **Expected behavior**: Exit code non-zero, error indicates permission denied.
#[test]
#[cfg(unix)]
fn build_fails_on_readonly_output_directory() {
    use std::os::unix::fs::PermissionsExt;

    let Some(infc_path) = require_infc() else {
        return;
    };

    let temp = assert_fs::TempDir::new().unwrap();
    let src = codegen_test_file("trivial.inf");
    let dest = temp.child("trivial.inf");
    std::fs::copy(&src, dest.path()).unwrap();

    // Create read-only output directory
    let out_dir = temp.child("out");
    std::fs::create_dir_all(out_dir.path()).unwrap();
    let mut perms = std::fs::metadata(out_dir.path())
        .expect("Failed to get metadata")
        .permissions();
    perms.set_mode(0o555);
    std::fs::set_permissions(out_dir.path(), perms).expect("Failed to set permissions");

    let mut cmd = Command::new(assert_cmd::cargo::cargo_bin!("infs"));
    cmd.env("INFC_PATH", &infc_path)
        .current_dir(temp.path())
        .arg("build")
        .arg(dest.path())
        .arg("--codegen")
        .arg("-o");

    cmd.assert().failure().stderr(
        predicate::str::contains("permission")
            .or(predicate::str::contains("Permission"))
            .or(predicate::str::contains("denied"))
            .or(predicate::str::contains("Failed")),
    );

    // Restore permissions for cleanup
    let mut perms = std::fs::metadata(out_dir.path())
        .expect("Failed to get metadata")
        .permissions();
    perms.set_mode(0o755);
    std::fs::set_permissions(out_dir.path(), perms).expect("Failed to restore permissions");
}

/// QA: TC-11.4 - Verify correct path handling with subdirectories.
///
/// **Expected behavior**: Path resolved correctly, build succeeds.
#[test]
fn build_handles_nested_paths() {
    let Some(infc_path) = require_infc() else {
        return;
    };

    let temp = assert_fs::TempDir::new().unwrap();

    // Create nested directory structure
    let nested_dir = temp.child("subdir").child("nested");
    std::fs::create_dir_all(nested_dir.path()).unwrap();

    let src = codegen_test_file("trivial.inf");
    let dest = nested_dir.child("test.inf");
    std::fs::copy(&src, dest.path()).unwrap();

    let mut cmd = Command::new(assert_cmd::cargo::cargo_bin!("infs"));
    cmd.env("INFC_PATH", &infc_path)
        .current_dir(temp.path())
        .arg("build")
        .arg(dest.path())
        .arg("--parse");

    cmd.assert()
        .success()
        .stdout(predicate::str::contains("Parsed:"));
}
