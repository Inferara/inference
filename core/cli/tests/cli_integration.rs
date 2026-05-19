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
//! 1. **Input validation**: File existence
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

/// Verifies that the full compilation pipeline executes correctly with explicit flags.
///
/// **Test setup**: Copies test input to a temporary directory to avoid
/// contaminating the repository with `out/` directories during parallel test runs.
///
/// **Expected behavior**: All phases complete successfully, producing a WASM file.
#[test]
fn full_pipeline_with_codegen() {
    let temp = assert_fs::TempDir::new().unwrap();
    let src = example_file("trivial.inf");
    let dest = temp.child("trivial.inf");
    std::fs::copy(&src, dest.path()).unwrap();

    let mut cmd = Command::new(assert_cmd::cargo::cargo_bin!("infc"));
    cmd.current_dir(temp.path())
        .arg(dest.path())
        .arg("--parse")
        .arg("--codegen")
        .arg("-o");

    cmd.assert()
        .success()
        .stdout(predicate::str::contains("Parsed:"));

    assert!(temp.child("out").child("trivial.wasm").path().exists());
}

/// Verifies that `infc file.inf` (no flags) defaults to full compilation and
/// writes a WASM file, matching conventional compiler UX (e.g. `gcc foo.c`).
#[test]
fn no_flags_produces_wasm() {
    let temp = assert_fs::TempDir::new().unwrap();
    let src = example_file("trivial.inf");
    let dest = temp.child("trivial.inf");
    std::fs::copy(&src, dest.path()).unwrap();

    let mut cmd = Command::new(assert_cmd::cargo::cargo_bin!("infc"));
    cmd.current_dir(temp.path()).arg(dest.path());

    cmd.assert()
        .success()
        .stdout(predicate::str::contains("WASM generated"));

    assert!(temp.child("out").child("trivial.wasm").path().exists());
}

/// Verifies that `-v` alone (no explicit phase flag) implies full pipeline
/// and produces both a WASM file and a Rocq translation file.
#[test]
fn v_flag_alone_produces_wasm_and_v() {
    let temp = assert_fs::TempDir::new().unwrap();
    let src = example_file("trivial.inf");
    let dest = temp.child("trivial.inf");
    std::fs::copy(&src, dest.path()).unwrap();

    let mut cmd = Command::new(assert_cmd::cargo::cargo_bin!("infc"));
    cmd.current_dir(temp.path()).arg(dest.path()).arg("-v");

    cmd.assert()
        .success()
        .stdout(predicate::str::contains("WASM generated"))
        .stdout(predicate::str::contains("V generated"));

    assert!(temp.child("out").child("trivial.wasm").path().exists());
    assert!(temp.child("out").child("trivial.v").path().exists());
}

/// Verifies that `--mode proof` produces both `.wasm` and `.v` outputs.
///
/// Proof mode implies `-v` because the Rocq translation IS the proof-mode
/// deliverable; emitting only `.wasm` in proof mode would silently waste the
/// unoptimized spec preservation work.
#[test]
fn mode_proof_produces_v_alongside_wasm() {
    let temp = assert_fs::TempDir::new().unwrap();
    let src = example_file("trivial.inf");
    let dest = temp.child("trivial.inf");
    std::fs::copy(&src, dest.path()).unwrap();

    let mut cmd = Command::new(assert_cmd::cargo::cargo_bin!("infc"));
    cmd.current_dir(temp.path())
        .arg(dest.path())
        .arg("--mode")
        .arg("proof");

    cmd.assert()
        .success()
        .stdout(predicate::str::contains("WASM generated"))
        .stdout(predicate::str::contains("V generated"));

    assert!(temp.child("out").child("trivial.wasm").path().exists());
    assert!(temp.child("out").child("trivial.v").path().exists());
}

/// `-v` with no explicit `--mode` must auto-promote to proof mode so the
/// emitted `.v` contains per-spec definitions and theorems. Without this
/// implication, `compile` mode strips spec functions and the `.v` is a
/// near-useless empty-specs file (the original reported UX bug).
#[test]
fn dash_v_implies_proof_mode_produces_per_spec_v() {
    let temp = assert_fs::TempDir::new().unwrap();
    let src = example_file("with_spec.inf");
    let dest = temp.child("with_spec.inf");
    std::fs::copy(&src, dest.path()).unwrap();

    let mut cmd = Command::new(assert_cmd::cargo::cargo_bin!("infc"));
    cmd.current_dir(temp.path()).arg(dest.path()).arg("-v");

    cmd.assert()
        .success()
        .stdout(predicate::str::contains("WASM generated"))
        .stdout(predicate::str::contains("V generated"));

    let v_path = temp.child("out").child("with_spec.v");
    assert!(v_path.path().exists(), "expected out/with_spec.v");
    let v_contents = std::fs::read_to_string(v_path.path()).unwrap();
    // The Rocq module name comes from the WASM custom `name` section (currently
    // hardcoded to "output" by codegen). The CLI-passed `source_fname` only
    // names the output file. What matters here is that the per-spec
    // Definition + Theorem are present at all — the empty-specs bug had
    // ZERO such entries regardless of the module name prefix.
    assert!(
        v_contents.contains("__MySpec_specs"),
        "expected a per-spec Definition for MySpec in:\n{v_contents}"
    );
    assert!(
        v_contents.contains("valid_") && v_contents.contains("__MySpec"),
        "expected a per-spec Theorem for MySpec in:\n{v_contents}"
    );
}

/// Regression guard: an explicit `--mode compile -v` must keep compile-mode
/// semantics — specs are stripped from the WASM and therefore absent from the
/// `.v`. Users who legitimately want V output from a spec-stripped WASM rely
/// on this escape hatch.
#[test]
fn explicit_mode_compile_plus_v_keeps_compile_semantics() {
    let temp = assert_fs::TempDir::new().unwrap();
    let src = example_file("with_spec.inf");
    let dest = temp.child("with_spec.inf");
    std::fs::copy(&src, dest.path()).unwrap();

    let mut cmd = Command::new(assert_cmd::cargo::cargo_bin!("infc"));
    cmd.current_dir(temp.path())
        .arg(dest.path())
        .arg("--mode")
        .arg("compile")
        .arg("-v");

    cmd.assert()
        .success()
        .stdout(predicate::str::contains("V generated"));

    let v_path = temp.child("out").child("with_spec.v");
    assert!(v_path.path().exists(), "expected out/with_spec.v");
    let v_contents = std::fs::read_to_string(v_path.path()).unwrap();
    assert!(
        !v_contents.contains("__MySpec_specs"),
        "explicit --mode compile must strip spec content from the .v; got:\n{v_contents}"
    );
}

/// Regression guard: without `--mode proof` and without `-v`, the default
/// (compile) mode must not emit a `.v` file. Existing behavior must be
/// preserved.
#[test]
fn mode_compile_default_does_not_emit_v_without_v_flag() {
    let temp = assert_fs::TempDir::new().unwrap();
    let src = example_file("trivial.inf");
    let dest = temp.child("trivial.inf");
    std::fs::copy(&src, dest.path()).unwrap();

    let mut cmd = Command::new(assert_cmd::cargo::cargo_bin!("infc"));
    cmd.current_dir(temp.path()).arg(dest.path());

    cmd.assert().success();

    assert!(temp.child("out").child("trivial.wasm").path().exists());
    assert!(
        !temp.child("out").child("trivial.v").path().exists(),
        "default compile mode without -v must not emit .v"
    );
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

/// Verifies that the compiler exits with failure and reports a parse error
/// when given a syntactically invalid source file.
///
/// **Expected behavior**: Exit with code 1 and print "Parse error" to stderr.
#[test]
fn fails_with_parse_error() {
    let mut cmd = Command::new(assert_cmd::cargo::cargo_bin!("infc"));
    cmd.arg(example_file("bad_syntax.inf")).arg("--parse");
    cmd.assert()
        .failure()
        .stderr(predicate::str::contains("Parse error"));
}

/// Verifies that `--commit-hash` prints the embedded git commit and exits 0
/// without requiring a source file argument.
///
/// **Expected behavior**: Exit with code 0, print a non-empty commit string
/// to stdout (either the short git hash or the `unknown` fallback).
#[test]
fn commit_hash_flag_prints_and_exits() {
    let mut cmd = Command::new(assert_cmd::cargo::cargo_bin!("infc"));
    cmd.arg("--commit-hash");
    let assert = cmd.assert().success();
    let output = assert.get_output();
    let stdout = String::from_utf8_lossy(&output.stdout);
    let hash = stdout.trim();
    assert!(!hash.is_empty(), "commit-hash stdout was empty");
    assert!(
        hash == "unknown" || hash.chars().all(|c| c.is_ascii_hexdigit()),
        "commit-hash stdout was not hex or 'unknown': {hash:?}"
    );
}

/// Verifies that `--abi-version` prints the compiler ABI version and exits 0
/// without requiring a source file argument.
///
/// **Expected behavior**: Exit with code 0, print `<major>.<minor>` to stdout
/// matching the constants exported by `inference-compiler-interface`.
#[test]
fn abi_version_flag_prints_and_exits() {
    let mut cmd = Command::new(assert_cmd::cargo::cargo_bin!("infc"));
    cmd.arg("--abi-version");
    let assert = cmd.assert().success();
    let output = assert.get_output();
    let stdout = String::from_utf8_lossy(&output.stdout);
    let version = stdout.trim();
    let expected = format!(
        "{}.{}",
        inference_compiler_interface::COMPILER_ABI_MAJOR,
        inference_compiler_interface::COMPILER_ABI_MINOR,
    );
    assert_eq!(version, expected);
}
