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

/// Resolves the path to a test data file in the `tests/test_data/inf/` directory.
///
/// This function navigates from the infs crate's manifest directory up to the
/// workspace root and then down into the test data directory.
///
/// ## Path Resolution
///
/// ```text
/// env!("CARGO_MANIFEST_DIR")  // apps/infs/
///   .parent()                 // apps/
///   .parent()                 // workspace root
///   .join("tests")
///   .join("test_data")
///   .join("inf")
///   .join(name)
/// ```
fn example_file(name: &str) -> std::path::PathBuf {
    std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .unwrap()
        .parent()
        .unwrap()
        .join("tests")
        .join("test_data")
        .join("inf")
        .join(name)
}

/// Resolves the path to a codegen test data file in `tests/test_data/codegen/wasm/base/`.
///
/// These files are simpler examples that successfully compile through all phases.
fn codegen_test_file(name: &str) -> std::path::PathBuf {
    std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .unwrap()
        .parent()
        .unwrap()
        .join("tests")
        .join("test_data")
        .join("codegen")
        .join("wasm")
        .join("base")
        .join(name)
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
    let mut cmd = Command::new(assert_cmd::cargo::cargo_bin!("infs"));
    cmd.arg("build")
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
    let temp = assert_fs::TempDir::new().unwrap();
    let src = codegen_test_file("trivial.inf");
    let dest = temp.child("trivial.inf");
    std::fs::copy(&src, dest.path()).unwrap();

    let mut cmd = Command::new(assert_cmd::cargo::cargo_bin!("infs"));
    cmd.current_dir(temp.path())
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
    let temp = assert_fs::TempDir::new().unwrap();
    let src = codegen_test_file("trivial.inf");
    let dest = temp.child("trivial.inf");
    std::fs::copy(&src, dest.path()).unwrap();

    let mut cmd = Command::new(assert_cmd::cargo::cargo_bin!("infs"));
    cmd.current_dir(temp.path())
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
    let temp = assert_fs::TempDir::new().unwrap();
    let src = codegen_test_file("trivial.inf");
    let dest = temp.child("trivial.inf");
    std::fs::copy(&src, dest.path()).unwrap();

    let mut cmd = Command::new(assert_cmd::cargo::cargo_bin!("infs"));
    cmd.current_dir(temp.path())
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

/// Verifies that `infs build` produces byte-identical WASM output as `infc`.
///
/// This test ensures backward compatibility and correctness by comparing
/// the output from both CLI tools when compiling the same source file.
#[test]
fn build_produces_identical_wasm_as_infc() {
    let infc_path = infc_binary();
    if !infc_path.exists() {
        eprintln!(
            "Skipping byte-identical test: infc binary not found at {infc_path:?}. \
             Build with `cargo build -p inference-cli` first."
        );
        return;
    }

    let temp_new = assert_fs::TempDir::new().unwrap();
    let temp_legacy = assert_fs::TempDir::new().unwrap();

    let src = codegen_test_file("trivial.inf");

    let dest_new = temp_new.child("trivial.inf");
    std::fs::copy(&src, dest_new.path()).unwrap();

    let dest_legacy = temp_legacy.child("trivial.inf");
    std::fs::copy(&src, dest_legacy.path()).unwrap();

    let mut cmd_new = Command::new(assert_cmd::cargo::cargo_bin!("infs"));
    cmd_new
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

    assert!(
        wasm_new.path().exists(),
        "infs did not produce WASM output"
    );
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
/// **Test setup**: Uses an isolated `INFS_HOME` directory to avoid affecting the system.
///
/// **Expected behavior**: Exit with non-zero code and print an error message
/// (not panic) when the manifest cannot be fetched.
#[test]
fn install_without_network_shows_error() {
    let temp = assert_fs::TempDir::new().unwrap();

    let mut cmd = Command::new(assert_cmd::cargo::cargo_bin!("infs"));
    cmd.env("INFS_HOME", temp.path())
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
/// **Test setup**: Uses an isolated `INFS_HOME` directory with no toolchains installed.
///
/// **Expected behavior**: Exit with non-zero code and indicate the version is not installed.
#[test]
fn uninstall_nonexistent_shows_message() {
    let temp = assert_fs::TempDir::new().unwrap();

    let mut cmd = Command::new(assert_cmd::cargo::cargo_bin!("infs"));
    cmd.env("INFS_HOME", temp.path())
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
/// **Test setup**: Uses an isolated `INFS_HOME` directory with no toolchains.
///
/// **Expected behavior**: Exit with code 0 (not a failure state).
#[test]
fn list_runs_successfully() {
    let temp = assert_fs::TempDir::new().unwrap();

    let mut cmd = Command::new(assert_cmd::cargo::cargo_bin!("infs"));
    cmd.env("INFS_HOME", temp.path()).arg("list");

    cmd.assert().success();
}

/// Verifies that `infs list` shows appropriate message when no toolchains are installed.
///
/// **Test setup**: Uses an isolated `INFS_HOME` directory with no toolchains.
///
/// **Expected behavior**: Exit with code 0 and display "No toolchains installed".
#[test]
fn list_shows_no_toolchains_message() {
    let temp = assert_fs::TempDir::new().unwrap();

    let mut cmd = Command::new(assert_cmd::cargo::cargo_bin!("infs"));
    cmd.env("INFS_HOME", temp.path()).arg("list");

    cmd.assert()
        .success()
        .stdout(predicate::str::contains("No toolchains installed"));
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
/// **Test setup**: Uses an isolated `INFS_HOME` directory with no toolchains.
///
/// **Expected behavior**: Exit with non-zero code and indicate version is not installed.
#[test]
fn default_nonexistent_version_shows_error() {
    let temp = assert_fs::TempDir::new().unwrap();

    let mut cmd = Command::new(assert_cmd::cargo::cargo_bin!("infs"));
    cmd.env("INFS_HOME", temp.path())
        .arg("default")
        .arg("0.0.0-nonexistent");

    cmd.assert()
        .failure()
        .stderr(predicate::str::contains("not installed"));
}

// -----------------------------------------------------------------------------
// Doctor Command Tests
// -----------------------------------------------------------------------------

/// Verifies that `infs doctor` runs successfully even with no toolchains installed.
///
/// **Test setup**: Uses an isolated `INFS_HOME` directory.
///
/// **Expected behavior**: Exit with code 0 (doctor reports issues but doesn't fail).
#[test]
fn doctor_runs_successfully() {
    let temp = assert_fs::TempDir::new().unwrap();

    let mut cmd = Command::new(assert_cmd::cargo::cargo_bin!("infs"));
    cmd.env("INFS_HOME", temp.path()).arg("doctor");

    cmd.assert().success();
}

/// Verifies that `infs doctor` shows platform check in output.
///
/// **Test setup**: Uses an isolated `INFS_HOME` directory.
///
/// **Expected behavior**: Output contains "Platform" check.
#[test]
fn doctor_shows_platform_check() {
    let temp = assert_fs::TempDir::new().unwrap();

    let mut cmd = Command::new(assert_cmd::cargo::cargo_bin!("infs"));
    cmd.env("INFS_HOME", temp.path()).arg("doctor");

    cmd.assert()
        .success()
        .stdout(predicate::str::contains("Platform"));
}

/// Verifies that `infs doctor` shows multiple health checks.
///
/// **Test setup**: Uses an isolated `INFS_HOME` directory.
///
/// **Expected behavior**: Output contains multiple check sections (Platform, Toolchain, etc.).
#[test]
fn doctor_shows_all_checks() {
    let temp = assert_fs::TempDir::new().unwrap();

    let mut cmd = Command::new(assert_cmd::cargo::cargo_bin!("infs"));
    cmd.env("INFS_HOME", temp.path()).arg("doctor");

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
    cmd.env("INFS_HOME", temp.path()).arg("doctor");

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
/// **Test setup**: Uses an isolated `INFS_HOME` directory.
///
/// **Expected behavior**: Exit with non-zero code and print an error message
/// (not panic) when the manifest cannot be fetched.
#[test]
fn self_update_without_network_shows_error() {
    let temp = assert_fs::TempDir::new().unwrap();

    let mut cmd = Command::new(assert_cmd::cargo::cargo_bin!("infs"));
    cmd.env("INFS_HOME", temp.path())
        .arg("self")
        .arg("update");

    cmd.assert()
        .failure()
        .stderr(predicate::str::contains("Error").or(predicate::str::contains("error")));
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
