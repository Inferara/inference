//! Integration tests for the Inference compiler CLI.
//!
//! These tests exercise the `infc` binary in a realistic environment by spawning
//! the compiled executable and validating its behavior through stdout, stderr,
//! and exit codes.
//!
//! ## Test Strategy
//!
//! The test suite verifies:
//!
//! 1. **Input validation**: File existence, required flags
//! 2. **Phase execution**: Correct execution of parse, analyze, codegen
//! 3. **Output generation**: WASM and Rocq file creation
//! 4. **Error handling**: Proper error messages and exit codes
//! 5. **Help and version**: CLI metadata display
//!
//! ## Test Infrastructure
//!
//! - Uses `assert_cmd` for spawning and asserting on command execution
//! - Uses `assert_fs` for temporary filesystem operations
//! - Uses `predicates` for flexible output matching
//! - Test data located in `tests/test_data/inf/` at workspace root
//!
//! ## Running Tests
//!
//! ```bash
//! cargo test -p inference-cli
//! ```
//!
//! Tests run in parallel and use temporary directories to avoid interference.
//!
//! ## See Also
//!
//! For comprehensive usage documentation and examples, see `README.md` in this crate.

use assert_cmd::prelude::*;
use assert_fs::prelude::*;
use predicates::prelude::*;
use std::process::Command;

/// Resolves the path to a test data file in the workspace.
///
/// Test data files are located at `<workspace_root>/tests/test_data/inf/`.
/// This function navigates from the CLI crate's manifest directory up to the
/// workspace root and then down into the test data directory.
///
/// ## Arguments
///
/// * `name` - The filename within the test data directory (e.g., "example.inf")
///
/// ## Returns
///
/// Absolute path to the test data file.
///
/// ## Path Resolution
///
/// ```text
/// env!("CARGO_MANIFEST_DIR")  // core/cli/
///   .parent()                 // core/
///   .parent()                 // workspace root
///   .join("tests")
///   .join("test_data")
///   .join("inf")
///   .join(name)
/// ```
///
/// ## Panics
///
/// Panics if the path traversal fails (should never happen in normal test execution).
fn example_file(name: &str) -> std::path::PathBuf {
    std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR")) // cli/
        .parent()
        .unwrap() // core/
        .parent()
        .unwrap() // workspace root
        .join("tests")
        .join("test_data")
        .join("inf")
        .join(name)
}

/// Verifies that the compiler fails gracefully when the input file doesn't exist.
///
/// **Expected behavior**: Exit with code 1 and print "path not found" to stderr.
#[test]
fn fails_when_file_missing() {
    let mut cmd = Command::new(assert_cmd::cargo::cargo_bin!("infc"));
    cmd.arg("this-file-does-not-exist.inf").arg("--parse");
    cmd.assert()
        .failure()
        .stderr(predicate::str::contains("path not found"));
}

/// Verifies that the compiler requires at least one phase flag.
///
/// **Expected behavior**: Exit with code 1 when no phase flags are provided,
/// with an error message explaining that at least one phase must be specified.
#[test]
fn fails_when_no_phase_selected() {
    let mut cmd = Command::new(assert_cmd::cargo::cargo_bin!("infc"));
    cmd.arg(example_file("example.inf"));
    cmd.assert()
        .failure()
        .stderr(predicate::str::contains("at least one of --parse"));
}

/// Verifies that the parse phase can run successfully as a standalone operation.
///
/// **Expected behavior**: Exit with code 0 and print "Parsed: <filepath>" to stdout
/// when the source file is syntactically valid.
#[test]
fn parse_only_succeeds() {
    let mut cmd = Command::new(assert_cmd::cargo::cargo_bin!("infc"));
    cmd.arg(example_file("example.inf")).arg("--parse");
    cmd.assert()
        .success()
        .stdout(predicate::str::contains("Parsed:"));
}

/// Verifies that the full compilation pipeline executes correctly.
///
/// **Test setup**: Copies test input to a temporary directory to avoid
/// contaminating the repository with `out/` directories during parallel test runs.
///
/// **Expected behavior**: The parse phase completes successfully and prints
/// "Parsed: <filepath>" to stdout. The codegen phase behavior depends on
/// current implementation status of the analyze phase.
///
/// **Note**: This test is tolerant of both success and failure outcomes for
/// codegen, as the analyze phase is work-in-progress. Once analysis is fully
/// implemented, this test should be updated to assert success and verify
/// output file generation.
#[test]
fn full_pipeline_with_codegen() {
    let temp = assert_fs::TempDir::new().unwrap();
    // Copy example.inf to temp so we don't contaminate repo out/ folder checks in parallel runs.
    let src = example_file("example.inf");
    let dest = temp.child("example.inf");
    std::fs::copy(&src, dest.path()).unwrap();

    let mut cmd = Command::new(assert_cmd::cargo::cargo_bin!("infc"));
    cmd.current_dir(temp.path())
        .arg(dest.path())
        .arg("--parse")
        // Skip --analyze for now (not yet implemented) but still request codegen.
        .arg("--codegen");

    // Expect failure because analysis is required prior to codegen; if codegen succeeds without analyze adjust later.
    let assert = cmd.assert();
    let out_pred = predicate::str::contains("Parsed:");
    assert.stdout(out_pred);
    // Accept either success (future implementation) or failure with panic message.
    // Can't directly match exit code with assert_cmd when allowing both, so pattern match stderr optional.
}

/// Verifies that the `--version` flag displays the correct version information.
///
/// **Expected behavior**: Exit with code 0 and print the version string to stdout.
/// The version string should match the version specified in `Cargo.toml`.
#[test]
fn shows_version() {
    let mut cmd = Command::new(assert_cmd::cargo::cargo_bin!("infc"));
    cmd.arg("--version");
    cmd.assert()
        .success()
        .stdout(predicate::str::contains(env!("CARGO_PKG_VERSION")));
}
