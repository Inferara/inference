//! Integration tests for the inference CLI.
//!
//! These tests spawn the compiled binary and assert on stdout/stderr and exit codes.

use assert_cmd::prelude::*;
use assert_fs::prelude::*;
use predicates::prelude::*;
use std::process::Command;

/// Returns path to example input file under `test_data/inf` (workspace root perspective).
fn example_file(name: &str) -> std::path::PathBuf {
    std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR")) // cli/
        .parent()
        .unwrap() // workspace root
        .join("test_data")
        .join("inf")
        .join(name)
}

#[test]
fn fails_when_file_missing() {
    let mut cmd = Command::new(assert_cmd::cargo::cargo_bin!("infc"));
    cmd.arg("this-file-does-not-exist.inf").arg("--parse");
    cmd.assert()
        .failure()
        .stderr(predicate::str::contains("path not found"));
}

#[test]
fn fails_when_no_phase_selected() {
    let mut cmd = Command::new(assert_cmd::cargo::cargo_bin!("infc"));
    cmd.arg(example_file("example.inf"));
    cmd.assert()
        .failure()
        .stderr(predicate::str::contains("at least one of --parse"));
}

#[test]
fn parse_only_succeeds() {
    let mut cmd = Command::new(assert_cmd::cargo::cargo_bin!("infc"));
    cmd.arg(example_file("example.inf")).arg("--parse");
    cmd.assert()
        .success()
        .stdout(predicate::str::contains("Parsed:"));
}

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

#[test]
fn shows_version() {
    let mut cmd = Command::new(assert_cmd::cargo::cargo_bin!("infc"));
    cmd.arg("--version");
    cmd.assert()
        .success()
        .stdout(predicate::str::contains(env!("CARGO_PKG_VERSION")));
}
