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
///
/// The fixture is copied into an isolated temp directory so the multi-file front
/// end's source-root scan sees exactly the one file under test and reports no
/// unreachable-sibling warnings.
#[test]
fn parse_only_succeeds() {
    let temp = assert_fs::TempDir::new().unwrap();
    let src = example_file("trivial.inf");
    let dest = temp.child("trivial.inf");
    std::fs::copy(&src, dest.path()).unwrap();

    let mut cmd = Command::new(assert_cmd::cargo::cargo_bin!("infc"));
    cmd.current_dir(temp.path()).arg(dest.path()).arg("--parse");
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

/// Pins the ABI version string to the literal value introduced for the
/// `--out-dir` flag. The `abi_version_flag_prints_and_exits` test above checks
/// the binary against the shared constant; this one additionally asserts the
/// concrete `1.1` so an accidental constant change is caught here too.
///
/// Uses an exact trimmed equality (not `contains`) so a near-miss such as
/// "11.1" or "1.10" — which would satisfy a substring match — cannot pass.
#[test]
fn abi_version_is_one_dot_one() {
    let mut cmd = Command::new(assert_cmd::cargo::cargo_bin!("infc"));
    cmd.arg("--abi-version");
    let assert = cmd.assert().success();
    let output = assert.get_output();
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert_eq!(
        stdout.trim(),
        "1.1",
        "ABI version must be exactly 1.1, not merely contain it"
    );
}

/// Verifies that `--out-dir <path>` redirects the `.wasm` artifact to the given
/// directory instead of the default `out/`.
#[test]
fn out_dir_redirects_wasm() {
    let temp = assert_fs::TempDir::new().unwrap();
    let src = example_file("trivial.inf");
    let dest = temp.child("trivial.inf");
    std::fs::copy(&src, dest.path()).unwrap();

    let mut cmd = Command::new(assert_cmd::cargo::cargo_bin!("infc"));
    cmd.current_dir(temp.path())
        .arg(dest.path())
        .arg("--out-dir")
        .arg("build");

    cmd.assert()
        .success()
        .stdout(predicate::str::contains("WASM generated"));

    assert!(
        temp.child("build").child("trivial.wasm").path().exists(),
        "expected build/trivial.wasm under --out-dir"
    );
    assert!(
        !temp.child("out").child("trivial.wasm").path().exists(),
        "--out-dir must not also write to the default out/ directory"
    );
}

/// Verifies that `--out-dir <path>` combined with `-v` redirects both the
/// `.wasm` and the `.v` to the given directory.
#[test]
fn out_dir_with_v_redirects_both_artifacts() {
    let temp = assert_fs::TempDir::new().unwrap();
    let src = example_file("trivial.inf");
    let dest = temp.child("trivial.inf");
    std::fs::copy(&src, dest.path()).unwrap();

    let mut cmd = Command::new(assert_cmd::cargo::cargo_bin!("infc"));
    cmd.current_dir(temp.path())
        .arg(dest.path())
        .arg("-v")
        .arg("--out-dir")
        .arg("build");

    cmd.assert()
        .success()
        .stdout(predicate::str::contains("WASM generated"))
        .stdout(predicate::str::contains("V generated"));

    assert!(
        temp.child("build").child("trivial.wasm").path().exists(),
        "expected build/trivial.wasm under --out-dir"
    );
    assert!(
        temp.child("build").child("trivial.v").path().exists(),
        "expected build/trivial.v under --out-dir"
    );
    assert!(
        !temp.child("out").path().exists(),
        "--out-dir must not create the default out/ directory"
    );
}

/// Regression guard: omitting `--out-dir` keeps the historical `out/` behavior.
#[test]
fn no_out_dir_keeps_default_out_directory() {
    let temp = assert_fs::TempDir::new().unwrap();
    let src = example_file("trivial.inf");
    let dest = temp.child("trivial.inf");
    std::fs::copy(&src, dest.path()).unwrap();

    let mut cmd = Command::new(assert_cmd::cargo::cargo_bin!("infc"));
    cmd.current_dir(temp.path()).arg(dest.path());

    cmd.assert()
        .success()
        .stdout(predicate::str::contains("WASM generated"));

    assert!(
        temp.child("out").child("trivial.wasm").path().exists(),
        "default output directory must remain out/ when --out-dir is omitted"
    );
}

/// Verifies that a multi-level `--out-dir` (e.g. `a/b/c`) is created at full
/// depth via a single `fs::create_dir_all`, with the artifact landing in the
/// leaf directory.
///
/// The path is assembled with `PathBuf` joins rather than a literal slash
/// string so the test is correct on every target platform (Linux, Windows,
/// macOS) and conforms to the repo rule against slash separators.
#[test]
fn out_dir_nested_path_is_created_at_full_depth() {
    let temp = assert_fs::TempDir::new().unwrap();
    let src = example_file("trivial.inf");
    let dest = temp.child("trivial.inf");
    std::fs::copy(&src, dest.path()).unwrap();

    let nested = std::path::PathBuf::from("a").join("b").join("c");

    let mut cmd = Command::new(assert_cmd::cargo::cargo_bin!("infc"));
    cmd.current_dir(temp.path())
        .arg(dest.path())
        .arg("--out-dir")
        .arg(&nested);

    cmd.assert()
        .success()
        .stdout(predicate::str::contains("WASM generated"));

    let leaf = temp.child("a").child("b").child("c");
    assert!(
        leaf.child("trivial.wasm").path().exists(),
        "expected a/b/c/trivial.wasm — nested out-dir must be created at full depth"
    );
    assert!(
        !temp.child("out").path().exists(),
        "nested --out-dir must not also create the default out/ directory"
    );
}

/// Verifies that an absolute `--out-dir` writes the artifact to that absolute
/// location, independent of the working directory, and does not create any
/// `out/` directory in the CWD.
///
/// A second `TempDir` provides the absolute destination so the test never
/// touches a real location outside the sandbox.
#[test]
fn out_dir_absolute_path_writes_there_and_no_cwd_out() {
    let cwd = assert_fs::TempDir::new().unwrap();
    let out = assert_fs::TempDir::new().unwrap();
    let src = example_file("trivial.inf");
    let dest = cwd.child("trivial.inf");
    std::fs::copy(&src, dest.path()).unwrap();

    let mut cmd = Command::new(assert_cmd::cargo::cargo_bin!("infc"));
    cmd.current_dir(cwd.path())
        .arg(dest.path())
        .arg("--out-dir")
        .arg(out.path());

    cmd.assert()
        .success()
        .stdout(predicate::str::contains("WASM generated"));

    assert!(
        out.child("trivial.wasm").path().exists(),
        "expected the artifact under the absolute --out-dir destination"
    );
    assert!(
        !cwd.child("out").path().exists(),
        "an absolute --out-dir must not create a default out/ in the CWD"
    );
}

/// Verifies that a `--out-dir` argument carrying a trailing path separator
/// (a common shell-completion artifact, e.g. `build/`) is tolerated: the
/// artifact still lands inside `build/`.
///
/// The trailing separator is appended with the platform's
/// `std::path::MAIN_SEPARATOR` so the literal-slash rule is respected and the
/// case is meaningful on Windows (`build\`) as well as Unix (`build/`).
#[test]
fn out_dir_trailing_separator_is_tolerated() {
    let temp = assert_fs::TempDir::new().unwrap();
    let src = example_file("trivial.inf");
    let dest = temp.child("trivial.inf");
    std::fs::copy(&src, dest.path()).unwrap();

    let with_trailing = format!("build{}", std::path::MAIN_SEPARATOR);

    let mut cmd = Command::new(assert_cmd::cargo::cargo_bin!("infc"));
    cmd.current_dir(temp.path())
        .arg(dest.path())
        .arg("--out-dir")
        .arg(&with_trailing);

    cmd.assert()
        .success()
        .stdout(predicate::str::contains("WASM generated"));

    assert!(
        temp.child("build").child("trivial.wasm").path().exists(),
        "a trailing path separator on --out-dir must still resolve to build/"
    );
}

/// Verifies that building into a pre-existing out-dir that already holds a
/// stale artifact of the same name succeeds and overwrites that artifact.
///
/// The directory and a sentinel file are created up front; after the build the
/// file must exist and its contents must no longer be the sentinel (i.e. it was
/// genuinely rewritten by codegen, not merely left in place).
#[test]
fn out_dir_overwrites_stale_artifact() {
    let temp = assert_fs::TempDir::new().unwrap();
    let src = example_file("trivial.inf");
    let dest = temp.child("trivial.inf");
    std::fs::copy(&src, dest.path()).unwrap();

    let build = temp.child("build");
    build.create_dir_all().unwrap();
    let stale = build.child("trivial.wasm");
    let sentinel = b"STALE NOT A WASM";
    std::fs::write(stale.path(), sentinel).unwrap();

    let mut cmd = Command::new(assert_cmd::cargo::cargo_bin!("infc"));
    cmd.current_dir(temp.path())
        .arg(dest.path())
        .arg("--out-dir")
        .arg("build");

    cmd.assert()
        .success()
        .stdout(predicate::str::contains("WASM generated"));

    assert!(
        stale.path().exists(),
        "the artifact must still exist after rebuilding into a populated out-dir"
    );
    let new_bytes = std::fs::read(stale.path()).unwrap();
    assert_ne!(
        new_bytes.as_slice(),
        sentinel.as_slice(),
        "the stale artifact must be overwritten by fresh codegen output"
    );
    assert_eq!(
        &new_bytes[..4],
        b"\0asm",
        "the overwritten file must be a real WASM module (magic bytes)"
    );
}

/// Verifies that `--out-dir out` (explicitly naming the historical default)
/// behaves identically to omitting the flag: the artifact lands in `out/`.
#[test]
fn out_dir_explicit_default_matches_default_behavior() {
    let temp = assert_fs::TempDir::new().unwrap();
    let src = example_file("trivial.inf");
    let dest = temp.child("trivial.inf");
    std::fs::copy(&src, dest.path()).unwrap();

    let mut cmd = Command::new(assert_cmd::cargo::cargo_bin!("infc"));
    cmd.current_dir(temp.path())
        .arg(dest.path())
        .arg("--out-dir")
        .arg("out");

    cmd.assert()
        .success()
        .stdout(predicate::str::contains("WASM generated"));

    assert!(
        temp.child("out").child("trivial.wasm").path().exists(),
        "--out-dir out must place the artifact in out/, same as the default"
    );
}

/// Verifies that `--out-dir .` writes artifacts directly into the current
/// working directory (the temp root) with no subdirectory.
#[test]
fn out_dir_dot_writes_into_cwd() {
    let temp = assert_fs::TempDir::new().unwrap();
    let src = example_file("trivial.inf");
    let dest = temp.child("trivial.inf");
    std::fs::copy(&src, dest.path()).unwrap();

    let mut cmd = Command::new(assert_cmd::cargo::cargo_bin!("infc"));
    cmd.current_dir(temp.path())
        .arg(dest.path())
        .arg("--out-dir")
        .arg(".");

    cmd.assert()
        .success()
        .stdout(predicate::str::contains("WASM generated"));

    assert!(
        temp.child("trivial.wasm").path().exists(),
        "--out-dir . must write the artifact directly into the CWD"
    );
    assert!(
        !temp.child("out").path().exists(),
        "--out-dir . must not also create an out/ directory"
    );
}

/// Verifies the collision error path: when a regular *file* already occupies
/// the requested out-dir name, directory creation fails and the build aborts.
///
/// **Expected behavior**: non-zero exit with "Failed to create output
/// directory" on stderr, and no `.wasm` artifact is produced. The pre-existing
/// path must remain a file (the build must not have clobbered it).
#[test]
fn out_dir_collides_with_existing_file_fails() {
    let temp = assert_fs::TempDir::new().unwrap();
    let src = example_file("trivial.inf");
    let dest = temp.child("trivial.inf");
    std::fs::copy(&src, dest.path()).unwrap();

    let blocker = temp.child("build");
    std::fs::write(blocker.path(), b"i am a file, not a directory").unwrap();

    let mut cmd = Command::new(assert_cmd::cargo::cargo_bin!("infc"));
    cmd.current_dir(temp.path())
        .arg(dest.path())
        .arg("--out-dir")
        .arg("build");

    cmd.assert()
        .failure()
        .stderr(predicate::str::contains("Failed to create output directory"));

    assert!(
        blocker.path().is_file(),
        "the colliding path must remain the original file"
    );
    assert!(
        !temp.child("build").child("trivial.wasm").path().exists(),
        "no artifact may be written when out-dir creation fails"
    );
}

/// Verifies that `--out-dir` together with `--mode proof` (which implies `-v`)
/// places both the `.wasm` and the `.v` under the requested directory.
#[test]
fn out_dir_with_mode_proof_redirects_both_artifacts() {
    let temp = assert_fs::TempDir::new().unwrap();
    let src = example_file("trivial.inf");
    let dest = temp.child("trivial.inf");
    std::fs::copy(&src, dest.path()).unwrap();

    let mut cmd = Command::new(assert_cmd::cargo::cargo_bin!("infc"));
    cmd.current_dir(temp.path())
        .arg(dest.path())
        .arg("--mode")
        .arg("proof")
        .arg("--out-dir")
        .arg("build");

    cmd.assert()
        .success()
        .stdout(predicate::str::contains("WASM generated"))
        .stdout(predicate::str::contains("V generated"));

    assert!(
        temp.child("build").child("trivial.wasm").path().exists(),
        "expected build/trivial.wasm under --out-dir in proof mode"
    );
    assert!(
        temp.child("build").child("trivial.v").path().exists(),
        "proof mode implies -v, so build/trivial.v must also be present"
    );
    assert!(
        !temp.child("out").path().exists(),
        "--out-dir must not create the default out/ directory"
    );
}

/// Verifies that `--out-dir` with an explicit `--mode compile -v` redirects
/// both artifacts under the directory (the compile-mode escape hatch for V).
#[test]
fn out_dir_with_mode_compile_v_redirects_both_artifacts() {
    let temp = assert_fs::TempDir::new().unwrap();
    let src = example_file("trivial.inf");
    let dest = temp.child("trivial.inf");
    std::fs::copy(&src, dest.path()).unwrap();

    let mut cmd = Command::new(assert_cmd::cargo::cargo_bin!("infc"));
    cmd.current_dir(temp.path())
        .arg(dest.path())
        .arg("--mode")
        .arg("compile")
        .arg("-v")
        .arg("--out-dir")
        .arg("build");

    cmd.assert()
        .success()
        .stdout(predicate::str::contains("WASM generated"))
        .stdout(predicate::str::contains("V generated"));

    assert!(
        temp.child("build").child("trivial.wasm").path().exists(),
        "expected build/trivial.wasm under --out-dir in compile -v mode"
    );
    assert!(
        temp.child("build").child("trivial.v").path().exists(),
        "explicit compile -v must still emit the .v under --out-dir"
    );
    assert!(
        !temp.child("out").path().exists(),
        "--out-dir must not create the default out/ directory"
    );
}

/// Verifies that `--out-dir` combined with `--parse` only succeeds without ever
/// creating the output directory: the parse phase writes no artifacts, so the
/// directory must remain absent afterward.
#[test]
fn out_dir_with_parse_only_creates_no_directory() {
    let temp = assert_fs::TempDir::new().unwrap();
    let src = example_file("trivial.inf");
    let dest = temp.child("trivial.inf");
    std::fs::copy(&src, dest.path()).unwrap();

    let mut cmd = Command::new(assert_cmd::cargo::cargo_bin!("infc"));
    cmd.current_dir(temp.path())
        .arg(dest.path())
        .arg("--parse")
        .arg("--out-dir")
        .arg("build");

    cmd.assert()
        .success()
        .stdout(predicate::str::contains("Parsed:"));

    assert!(
        !temp.child("build").path().exists(),
        "--parse writes no artifacts, so --out-dir must not be created"
    );
}

/// Verifies that `--out-dir` combined with `--analyze` only succeeds without
/// creating the output directory: analyze writes no artifacts.
#[test]
fn out_dir_with_analyze_only_creates_no_directory() {
    let temp = assert_fs::TempDir::new().unwrap();
    let src = example_file("trivial.inf");
    let dest = temp.child("trivial.inf");
    std::fs::copy(&src, dest.path()).unwrap();

    let mut cmd = Command::new(assert_cmd::cargo::cargo_bin!("infc"));
    cmd.current_dir(temp.path())
        .arg(dest.path())
        .arg("--analyze")
        .arg("--out-dir")
        .arg("build");

    cmd.assert()
        .success()
        .stdout(predicate::str::contains("Analyzed:"));

    assert!(
        !temp.child("build").path().exists(),
        "--analyze writes no artifacts, so --out-dir must not be created"
    );
}

/// Verifies that `--out-dir` combined with `--codegen` but neither `-o` nor
/// `-v` runs codegen yet writes no files: the directory is created lazily only
/// when an artifact is actually emitted, so it must not exist afterward.
#[test]
fn out_dir_with_codegen_no_output_flags_creates_no_directory() {
    let temp = assert_fs::TempDir::new().unwrap();
    let src = example_file("trivial.inf");
    let dest = temp.child("trivial.inf");
    std::fs::copy(&src, dest.path()).unwrap();

    let mut cmd = Command::new(assert_cmd::cargo::cargo_bin!("infc"));
    cmd.current_dir(temp.path())
        .arg(dest.path())
        .arg("--codegen")
        .arg("--out-dir")
        .arg("build");

    cmd.assert()
        .success()
        .stdout(predicate::str::contains("Codegen complete"));

    assert!(
        !temp.child("build").path().exists(),
        "--codegen without -o/-v writes nothing, so --out-dir must not be created"
    );
}

/// A `--codegen` dry run (neither `-o` nor `-v`) must NOT delete a pre-existing
/// artifact a prior build wrote. The stale-clearing step runs only when the run
/// will write at least one artifact; a dry run writes nothing, so an earlier
/// `out/trivial.wasm` survives. (Clearing it unconditionally on `--codegen` would
/// leave no artifact even though the dry run "succeeded".)
#[test]
fn codegen_dry_run_preserves_existing_artifact() {
    let temp = assert_fs::TempDir::new().unwrap();
    let src = example_file("trivial.inf");
    let dest = temp.child("trivial.inf");
    std::fs::copy(&src, dest.path()).unwrap();

    // A default build first writes out/trivial.wasm.
    let mut build = Command::new(assert_cmd::cargo::cargo_bin!("infc"));
    build.current_dir(temp.path()).arg(dest.path());
    build
        .assert()
        .success()
        .stdout(predicate::str::contains("WASM generated"));
    let artifact = temp.child("out").child("trivial.wasm");
    assert!(artifact.path().exists(), "default build must write the .wasm");

    // A `--codegen` dry run (no -o/-v) succeeds but writes nothing.
    let mut dry = Command::new(assert_cmd::cargo::cargo_bin!("infc"));
    dry.current_dir(temp.path()).arg(dest.path()).arg("--codegen");
    dry.assert()
        .success()
        .stdout(predicate::str::contains("Codegen complete"));

    assert!(
        artifact.path().exists(),
        "a --codegen dry run writes nothing, so the pre-existing out/trivial.wasm must survive"
    );
}

// Multi-file front end (issue #63).
//
// infc drives `parse_project` for the `--parse` phase, folding the
// import-reachable closure into one arena. These binary-level tests exercise
// the success path (`Parsed:` on stdout), the unreachable-file warning (stderr),
// and the missing-import error with a nearest-match suggestion (stderr, exit 1).
//
// They stay at `--parse` to isolate the front-end discovery behavior under test
// (success, warnings, and import errors). Multi-file codegen is fully wired —
// the import-reachable closure compiles to one artifact — so driving later phases
// would test codegen, not discovery; the dedicated codegen tests cover that.

/// Writes `source` to `<root>/<relative>` (a `/`-joined logical path), creating
/// parent directories, and returns the absolute path. The logical path is split
/// and re-joined with `PathBuf` so the literal-slash rule is honored on every
/// platform.
fn write_source(root: &std::path::Path, relative: &str, source: &str) -> std::path::PathBuf {
    let mut dest = root.to_path_buf();
    for segment in relative.split('/') {
        dest.push(segment);
    }
    if let Some(parent) = dest.parent() {
        std::fs::create_dir_all(parent).unwrap();
    }
    std::fs::write(&dest, source).unwrap();
    dest
}

/// A three-file project parses through `infc --parse`: the entry imports a file
/// which imports a nested file, all folded into one arena. Success prints
/// `Parsed:` and exits 0.
#[test]
fn parse_multi_file_project_succeeds() {
    let temp = assert_fs::TempDir::new().unwrap();
    let entry = write_source(temp.path(), "main.inf", "use math;\npub fn main() {}");
    write_source(temp.path(), "math.inf", "use lib::arith;\npub fn foo() {}");
    write_source(
        temp.path(),
        "lib/arith.inf",
        "pub fn add(a: i32, b: i32) -> i32 { return a + b; }",
    );

    let mut cmd = Command::new(assert_cmd::cargo::cargo_bin!("infc"));
    cmd.current_dir(temp.path()).arg(&entry).arg("--parse");

    cmd.assert()
        .success()
        .stdout(predicate::str::contains("Parsed:"));
}

/// An orphan `.inf` file under the source root that no import reaches produces a
/// warning on stderr while the parse still succeeds (exit 0).
#[test]
fn parse_multi_file_warns_on_unreachable_file() {
    let temp = assert_fs::TempDir::new().unwrap();
    let entry = write_source(temp.path(), "main.inf", "use used;\npub fn main() {}");
    write_source(temp.path(), "used.inf", "pub fn fu() {}");
    write_source(temp.path(), "orphan.inf", "pub fn fo() {}");

    let mut cmd = Command::new(assert_cmd::cargo::cargo_bin!("infc"));
    cmd.current_dir(temp.path()).arg(&entry).arg("--parse");

    cmd.assert()
        .success()
        .stdout(predicate::str::contains("Parsed:"))
        .stderr(
            predicate::str::contains("warning")
                .and(predicate::str::contains("orphan.inf"))
                .and(predicate::str::contains("not imported by any reachable file")),
        );
}

/// A `use` naming a file that does not exist aborts the parse with exit 1 and a
/// "Parse error" on stderr naming the missing import.
#[test]
fn parse_multi_file_missing_import_errors() {
    let temp = assert_fs::TempDir::new().unwrap();
    let entry = write_source(temp.path(), "main.inf", "use absent;\npub fn main() {}");

    let mut cmd = Command::new(assert_cmd::cargo::cargo_bin!("infc"));
    cmd.current_dir(temp.path()).arg(&entry).arg("--parse");

    cmd.assert()
        .failure()
        .stderr(predicate::str::contains("Parse error"))
        .stderr(predicate::str::contains("imported file not found"));
}

/// A missing import whose name is one edit away from an existing sibling yields
/// the "did you mean" suggestion in the error text.
#[test]
fn parse_multi_file_missing_import_suggests_near_match() {
    let temp = assert_fs::TempDir::new().unwrap();
    let entry = write_source(temp.path(), "main.inf", "use arith;\npub fn main() {}");
    // One edit away from the missing `arith.inf`.
    write_source(
        temp.path(),
        "arithh.inf",
        "pub fn add(a: i32, b: i32) -> i32 { return a + b; }",
    );

    let mut cmd = Command::new(assert_cmd::cargo::cargo_bin!("infc"));
    cmd.current_dir(temp.path()).arg(&entry).arg("--parse");

    cmd.assert()
        .failure()
        .stderr(predicate::str::contains("did you mean"))
        .stderr(predicate::str::contains("arithh"));
}

/// A syntax error inside an IMPORTED file is reported by name (its `::`-joined
/// module path), not as the entry, with exit 1.
#[test]
fn parse_multi_file_syntax_error_in_import_names_module() {
    let temp = assert_fs::TempDir::new().unwrap();
    let entry = write_source(temp.path(), "main.inf", "use lib::broken;\npub fn main() {}");
    write_source(temp.path(), "lib/broken.inf", "pub fn oops( { return 1; }");

    let mut cmd = Command::new(assert_cmd::cargo::cargo_bin!("infc"));
    cmd.current_dir(temp.path()).arg(&entry).arg("--parse");

    cmd.assert()
        .failure()
        .stderr(predicate::str::contains("Parse error"))
        .stderr(predicate::str::contains("lib::broken"));
}

/// A syntax error in the ENTRY file names its real path and must NOT use the
/// "imported file" wording — the entry is the file the user compiled, not an
/// import. (The imported-file channel keeps its own wording, asserted above.)
#[test]
fn parse_entry_syntax_error_names_real_path_not_imported() {
    let temp = assert_fs::TempDir::new().unwrap();
    let entry = write_source(temp.path(), "main.inf", "pub fn main() -> i32 { let x: i32 = ; return x; }");

    let mut cmd = Command::new(assert_cmd::cargo::cargo_bin!("infc"));
    cmd.current_dir(temp.path()).arg(&entry).arg("--parse");

    cmd.assert()
        .failure()
        .stderr(predicate::str::contains("Parse error"))
        .stderr(predicate::str::contains("main.inf"))
        .stderr(predicate::str::contains("imported file").not())
        .stderr(predicate::str::contains("<entry>").not());
}

/// A type error inside an IMPORTED file is reported by the file's `::`-joined
/// module path, so the user is not misdirected to the entry file. Source
/// locations are per-file-local, so a bare `line:col` would otherwise read as
/// the entry.
#[test]
fn type_check_error_in_import_names_module() {
    let temp = assert_fs::TempDir::new().unwrap();
    let entry = write_source(temp.path(), "main.inf", "use lib::geom;\npub fn main() -> i32 { return 0; }");
    write_source(
        temp.path(),
        "lib/geom.inf",
        "pub struct Point { x: i32; y: i32; }\npub fn bad() -> i32 { return Point { x: 1, y: 2 }; }",
    );

    let mut cmd = Command::new(assert_cmd::cargo::cargo_bin!("infc"));
    cmd.current_dir(temp.path()).arg(&entry).arg("--analyze");

    cmd.assert()
        .failure()
        .stderr(predicate::str::contains("Type checking failed"))
        .stderr(predicate::str::contains("lib::geom:"));
}

/// An analysis finding (A037) inside an IMPORTED file is reported by the file's
/// module path, matching the type-check and parse channels.
#[test]
fn analysis_finding_in_import_names_module() {
    let temp = assert_fs::TempDir::new().unwrap();
    let entry = write_source(
        temp.path(),
        "main.inf",
        "use lib::a;\npub fn main() -> i32 { return lib::a::oob(); }",
    );
    write_source(
        temp.path(),
        "lib/a.inf",
        "pub fn oob() -> i32 { let a: [i32; 3] = [1,2,3]; return a[5]; }",
    );

    let mut cmd = Command::new(assert_cmd::cargo::cargo_bin!("infc"));
    cmd.current_dir(temp.path()).arg(&entry).arg("--analyze");

    cmd.assert()
        .failure()
        .stderr(predicate::str::contains("[A037]"))
        .stderr(predicate::str::contains("lib::a:"));
}

/// Regression: a single-file type error stays a bare `line:col` (no file prefix),
/// so existing single-file diagnostics are unchanged.
#[test]
fn type_check_error_in_single_file_stays_bare() {
    let temp = assert_fs::TempDir::new().unwrap();
    let entry = write_source(temp.path(), "solo.inf", "pub fn main() -> i32 { return true; }");

    let mut cmd = Command::new(assert_cmd::cargo::cargo_bin!("infc"));
    cmd.current_dir(temp.path()).arg(&entry).arg("--analyze");

    cmd.assert()
        .failure()
        .stderr(predicate::str::contains("Type checking failed"))
        .stderr(predicate::str::contains("type mismatch"))
        .stderr(predicate::str::contains("<entry>").not());
}

/// Regression: a single-file input with no imports still parses through the
/// multi-file front end exactly as before, with no spurious warnings on stderr.
#[test]
fn parse_single_file_through_project_front_end_is_quiet() {
    let temp = assert_fs::TempDir::new().unwrap();
    let entry = write_source(temp.path(), "solo.inf", "pub fn main() -> i32 { return 0; }");

    let mut cmd = Command::new(assert_cmd::cargo::cargo_bin!("infc"));
    cmd.current_dir(temp.path()).arg(&entry).arg("--parse");

    cmd.assert()
        .success()
        .stdout(predicate::str::contains("Parsed:"))
        .stderr(predicate::str::is_empty());
}

/// A proof-mode spec whose file-qualified name would fabricate a reserved `__`
/// run (here `spec _S`, whose leading `_` lands next to the module-path join `_`)
/// is rejected during codegen — before any artifact is written — with an
/// educational, source-level message: it leads with the SOURCE spec and its file
/// (`spec '_S' in file 'lib::geo'`), shows the flattening so the user sees *why*
/// (`lib_geo__S`), and points at the rename. Crucially, no stale `out/main.wasm`
/// is left behind: the codegen failure precedes the WASM write.
#[test]
fn invalid_spec_name_rejected_early_leaves_no_stale_wasm() {
    let temp = assert_fs::TempDir::new().unwrap();
    let entry = write_source(temp.path(), "main.inf", "use lib::geo;\npub fn main() -> i32 { return 0; }");
    write_source(
        temp.path(),
        "lib/geo.inf",
        "spec _S { fn obligation() -> i32 { return 7; } }",
    );

    let mut cmd = Command::new(assert_cmd::cargo::cargo_bin!("infc"));
    cmd.current_dir(temp.path()).arg(&entry).arg("-v");

    cmd.assert()
        .failure()
        // Leads with the source spec and its file; shows the flattening so the
        // cause is visible; explains the readability rationale for rejecting.
        .stderr(predicate::str::contains("spec '_S' in file 'lib::geo'"))
        .stderr(predicate::str::contains("lib_geo__S"))
        .stderr(predicate::str::contains("reserved '__' separator"))
        .stderr(predicate::str::contains("appear verbatim in your generated .v"));

    assert!(
        !temp.child("out").child("main.wasm").path().exists(),
        "a rejected spec name must not leave a stale out/main.wasm behind"
    );
}

/// A proof-mode spec in a file whose stem ends in `_` (here `lib/x_.inf`) is
/// rejected with the same educational message, which blames the FILE stem (not
/// the spec) and gives the imperative rename `rename the file 'x_.inf' to
/// 'x.inf'`. No stale artifact remains.
#[test]
fn trailing_underscore_file_stem_spec_rejected_no_stale_wasm() {
    let temp = assert_fs::TempDir::new().unwrap();
    let entry = write_source(temp.path(), "main.inf", "use lib::x_;\npub fn main() -> i32 { return 0; }");
    write_source(
        temp.path(),
        "lib/x_.inf",
        "spec S { fn obligation() -> i32 { return 1; } }",
    );

    let mut cmd = Command::new(assert_cmd::cargo::cargo_bin!("infc"));
    cmd.current_dir(temp.path()).arg(&entry).arg("-v");

    cmd.assert()
        .failure()
        .stderr(predicate::str::contains("spec 'S' in file 'lib::x_'"))
        .stderr(predicate::str::contains("file stem 'x_'"))
        .stderr(predicate::str::contains("rename the file 'x_.inf' to 'x.inf'"));

    assert!(
        !temp.child("out").child("main.wasm").path().exists(),
        "a rejected file-stem spec name must not leave a stale out/main.wasm behind"
    );
    assert!(
        !temp.child("out").child("main.v").path().exists(),
        "a rejected file-stem spec name must not leave a stale out/main.v behind"
    );
}

/// Default `compile` mode (no `-v`) does not emit any Rocq name, so a file stem
/// ending in `_` that would be rejected in proof mode compiles cleanly here — the
/// rejection is scoped to where the name is actually emitted.
#[test]
fn trailing_underscore_file_stem_spec_compiles_in_default_mode() {
    let temp = assert_fs::TempDir::new().unwrap();
    let entry = write_source(temp.path(), "main.inf", "use lib::x_;\npub fn main() -> i32 { return 0; }");
    write_source(
        temp.path(),
        "lib/x_.inf",
        "spec S { fn obligation() -> i32 { return 1; } }",
    );

    let mut cmd = Command::new(assert_cmd::cargo::cargo_bin!("infc"));
    cmd.current_dir(temp.path()).arg(&entry);

    cmd.assert()
        .success()
        .stdout(predicate::str::contains("WASM generated"));

    assert!(temp.child("out").child("main.wasm").path().exists());
    assert!(
        !temp.child("out").child("main.v").path().exists(),
        "default mode must not emit a .v"
    );
}

/// The imported-file *trailing*-underscore spec (`spec Invariant_` in
/// `lib/geom.inf`). Its `_`-join `lib_geom_Invariant_` carries no `__` of its own
/// — the trailing `_` only abuts the translator's downstream `_specs` join — so an
/// earlier build let it through codegen and reported the internal joined name
/// (`lib_geom_Invariant_`) with no file label. It is now caught at the source
/// level: the message names the SOURCE spec (`Invariant_`) and its file
/// (`lib::geom`), gives the imperative rename, and (per the show-the-flattening
/// policy) still shows the generated name as the consequence. No stale artifact.
#[test]
fn imported_trailing_underscore_spec_rejected_with_source_level_message() {
    let temp = assert_fs::TempDir::new().unwrap();
    let entry = write_source(temp.path(), "main.inf", "use lib::geom;\npub fn main() -> i32 { return 0; }");
    write_source(
        temp.path(),
        "lib/geom.inf",
        "spec Invariant_ { fn obligation() -> i32 { return 7; } }",
    );

    let mut cmd = Command::new(assert_cmd::cargo::cargo_bin!("infc"));
    cmd.current_dir(temp.path()).arg(&entry).arg("-v");

    cmd.assert()
        .failure()
        // The source spec name and the file label both appear, leading the message.
        .stderr(predicate::str::contains("spec 'Invariant_'"))
        .stderr(predicate::str::contains("in file 'lib::geom'"))
        // The imperative fix names the source spec and the exact edit.
        .stderr(predicate::str::contains(
            "rename the spec 'Invariant_' to 'Invariant' (drop the trailing '_')",
        ))
        .stderr(predicate::str::contains("appear verbatim in your generated .v"));

    assert!(
        !temp.child("out").child("main.wasm").path().exists(),
        "a rejected imported-file spec name must not leave a stale out/main.wasm behind"
    );
    assert!(
        !temp.child("out").child("main.v").path().exists(),
        "a rejected imported-file spec name must not leave a stale out/main.v behind"
    );
}

/// The sibling imported-file offenses — a leading `_` (`_x`), an internal `__`
/// run (`a__b`), and a `__` run in the spec name (`S__T`) — all route through the
/// same source-level diagnostic, each naming the source spec and the file
/// `lib::geom`, never leading with the internal joined key. Run as one test over a
/// table so the uniform shape is asserted in one place.
#[test]
fn imported_spec_underscore_offenses_all_source_labeled() {
    // (spec name, the exact edit phrasing the imperative fix should carry)
    let cases = [
        ("_x", "drop the leading '_'"),
        ("a__b", "collapse the '__' run"),
        ("S__T", "collapse the '__' run"),
    ];
    for (spec_name, edit) in cases {
        let temp = assert_fs::TempDir::new().unwrap();
        let entry = write_source(
            temp.path(),
            "main.inf",
            "use lib::geom;\npub fn main() -> i32 { return 0; }",
        );
        write_source(
            temp.path(),
            "lib/geom.inf",
            &format!("spec {spec_name} {{ fn obligation() -> i32 {{ return 1; }} }}"),
        );

        let mut cmd = Command::new(assert_cmd::cargo::cargo_bin!("infc"));
        cmd.current_dir(temp.path()).arg(&entry).arg("-v");

        cmd.assert()
            .failure()
            .stderr(predicate::str::contains(format!("spec '{spec_name}'")))
            .stderr(predicate::str::contains("in file 'lib::geom'"))
            .stderr(predicate::str::contains(format!(
                "rename the spec '{spec_name}'"
            )))
            .stderr(predicate::str::contains(edit));

        assert!(
            !temp.child("out").child("main.wasm").path().exists(),
            "a rejected spec `{spec_name}` must not leave a stale out/main.wasm behind"
        );
    }
}

/// A clean imported-file spec name still compiles to both artifacts in proof mode:
/// the trailing-`_` rejection is scoped to the offending names, not all imported
/// specs.
#[test]
fn imported_clean_spec_name_produces_wasm_and_v() {
    let temp = assert_fs::TempDir::new().unwrap();
    let entry = write_source(temp.path(), "main.inf", "use lib::geom;\npub fn main() -> i32 { return 0; }");
    write_source(
        temp.path(),
        "lib/geom.inf",
        "spec Invariant { fn obligation() -> i32 { return 7; } }",
    );

    let mut cmd = Command::new(assert_cmd::cargo::cargo_bin!("infc"));
    cmd.current_dir(temp.path()).arg(&entry).arg("-v");

    cmd.assert()
        .success()
        .stdout(predicate::str::contains("WASM generated"))
        .stdout(predicate::str::contains("V generated"));

    assert!(temp.child("out").child("main.wasm").path().exists());
    assert!(temp.child("out").child("main.v").path().exists());
}

/// Companion to [`invalid_spec_name_rejected_early_leaves_no_stale_wasm`]: a
/// legal spec name in a non-entry file still compiles and produces both the WASM
/// and the Rocq `.v` artifact in proof mode.
#[test]
fn valid_non_entry_spec_name_produces_wasm_and_v() {
    let temp = assert_fs::TempDir::new().unwrap();
    let entry = write_source(temp.path(), "main.inf", "use lib::geo;\npub fn main() -> i32 { return 0; }");
    write_source(
        temp.path(),
        "lib/geo.inf",
        "spec S { fn obligation() -> i32 { return 7; } }",
    );

    let mut cmd = Command::new(assert_cmd::cargo::cargo_bin!("infc"));
    cmd.current_dir(temp.path()).arg(&entry).arg("-v");

    cmd.assert()
        .success()
        .stdout(predicate::str::contains("WASM generated"))
        .stdout(predicate::str::contains("V generated"));

    assert!(temp.child("out").child("main.wasm").path().exists());
    assert!(temp.child("out").child("main.v").path().exists());
}

/// The ENTRY file's spec is checked too: `spec Spec_` in the entry (empty module
/// path) leaves the entry's qualified spec name as bare `Spec_` — no `__` for
/// codegen's own join check to catch — but the translator joins it with the entry
/// stem `main` and the trailing `_specs` into the reserved `main__Spec__specs`.
/// Rejected in proof mode with the educational message, before any artifact write.
#[test]
fn entry_spec_trailing_underscore_rejected_no_stale_artifact() {
    let temp = assert_fs::TempDir::new().unwrap();
    let entry = write_source(
        temp.path(),
        "main.inf",
        "spec Spec_ { fn obligation() -> i32 { return 1; } }\npub fn main() -> i32 { return 0; }",
    );

    let mut cmd = Command::new(assert_cmd::cargo::cargo_bin!("infc"));
    cmd.current_dir(temp.path()).arg(&entry).arg("-v");

    cmd.assert()
        .failure()
        .stderr(predicate::str::contains("spec 'Spec_'"))
        .stderr(predicate::str::contains("main__Spec__specs"))
        .stderr(predicate::str::contains("'spec Spec_' -> 'spec Spec'"))
        .stderr(predicate::str::contains("appear verbatim in your .v"));

    assert!(
        !temp.child("out").child("main.wasm").path().exists(),
        "a rejected entry spec name must not leave a stale out/main.wasm behind"
    );
    assert!(
        !temp.child("out").child("main.v").path().exists(),
        "a rejected entry spec name must not leave a stale out/main.v behind"
    );
}

/// The ENTRY file's *stem* is checked too: an entry compiled as `app_.inf`
/// produces output module name `app_`, which joins with any spec via the `__`
/// separator into the reserved `app___Foo`. Rejected in proof mode, naming the
/// source file and the rename, before any artifact write.
#[test]
fn entry_filename_trailing_underscore_rejected_no_stale_artifact() {
    let temp = assert_fs::TempDir::new().unwrap();
    let entry = write_source(
        temp.path(),
        "app_.inf",
        "spec Foo { fn obligation() -> i32 { return 1; } }\npub fn main() -> i32 { return 0; }",
    );

    let mut cmd = Command::new(assert_cmd::cargo::cargo_bin!("infc"));
    cmd.current_dir(temp.path()).arg(&entry).arg("-v");

    cmd.assert()
        .failure()
        .stderr(predicate::str::contains("output module name 'app_'"))
        .stderr(predicate::str::contains("app___Foo"))
        .stderr(predicate::str::contains("'app_.inf' -> 'app.inf'"));

    assert!(
        !temp.child("out").child("app_.wasm").path().exists(),
        "a rejected entry stem must not leave a stale out/app_.wasm behind"
    );
    assert!(
        !temp.child("out").child("app_.v").path().exists(),
        "a rejected entry stem must not leave a stale out/app_.v behind"
    );
}

/// The entry-spec rejection is scoped to proof mode: in default `compile` mode no
/// Rocq name is emitted, so `spec Spec_` in the entry compiles cleanly to `.wasm`
/// with no `.v`.
#[test]
fn entry_spec_trailing_underscore_compiles_in_default_mode() {
    let temp = assert_fs::TempDir::new().unwrap();
    let entry = write_source(
        temp.path(),
        "main.inf",
        "spec Spec_ { fn obligation() -> i32 { return 1; } }\npub fn main() -> i32 { return 0; }",
    );

    let mut cmd = Command::new(assert_cmd::cargo::cargo_bin!("infc"));
    cmd.current_dir(temp.path()).arg(&entry);

    cmd.assert()
        .success()
        .stdout(predicate::str::contains("WASM generated"));

    assert!(temp.child("out").child("main.wasm").path().exists());
    assert!(
        !temp.child("out").child("main.v").path().exists(),
        "default mode must not emit a .v"
    );
}

// Stale-artifact safety: a rejected compile must never leave a runnable
// `out/<name>.wasm` (or `.v`) on disk for `wasmtime` to execute. After a good
// build, a later edit that fails any rejection channel (type check, analysis,
// codegen) must clear the previous artifact rather than leave it behind, in both
// single-file and multi-file modes.

/// Builds `entry` in the given temp dir and asserts the build succeeded and wrote
/// `out/<stem>.wasm`. Shared first half of every stale-artifact test.
fn build_ok_and_assert_wasm(temp: &assert_fs::TempDir, entry: &std::path::Path, stem: &str) {
    let mut cmd = Command::new(assert_cmd::cargo::cargo_bin!("infc"));
    cmd.current_dir(temp.path()).arg(entry);
    cmd.assert()
        .success()
        .stdout(predicate::str::contains("WASM generated"));
    assert!(
        temp.child("out").child(format!("{stem}.wasm")).path().exists(),
        "a successful build must write out/{stem}.wasm"
    );
}

/// Multi-file, type-check channel: a good build writes the WASM; editing an
/// imported file to call an undefined function makes the recompile fail type
/// checking, and the previously-written `out/main.wasm` must be gone.
#[test]
fn rejected_typecheck_clears_stale_multi_file_wasm() {
    let temp = assert_fs::TempDir::new().unwrap();
    let entry = write_source(
        temp.path(),
        "main.inf",
        "use lib::a;\npub fn main() -> i32 { return lib::a::seven(); }",
    );
    write_source(temp.path(), "lib/a.inf", "pub fn seven() -> i32 { return 7; }");

    build_ok_and_assert_wasm(&temp, &entry, "main");

    // Break the imported file: a call to an undefined function fails type check.
    write_source(temp.path(), "lib/a.inf", "pub fn seven() -> i32 { return nope(); }");

    let mut cmd = Command::new(assert_cmd::cargo::cargo_bin!("infc"));
    cmd.current_dir(temp.path()).arg(&entry);
    cmd.assert()
        .failure()
        .stderr(predicate::str::contains("Type checking failed"));

    assert!(
        !temp.child("out").child("main.wasm").path().exists(),
        "a type-check rejection must not leave a runnable stale out/main.wasm"
    );
}

/// Multi-file, analysis channel (A035 recursion): a good build writes the WASM;
/// rewriting an imported file into a self-recursive function makes the recompile
/// fail analysis, and the previously-written `out/main.wasm` must be gone.
#[test]
fn rejected_analysis_clears_stale_multi_file_wasm() {
    let temp = assert_fs::TempDir::new().unwrap();
    let entry = write_source(
        temp.path(),
        "main.inf",
        "use lib::r;\npub fn main() -> i32 { return lib::r::go(); }",
    );
    write_source(temp.path(), "lib/r.inf", "pub fn go() -> i32 { return 7; }");

    build_ok_and_assert_wasm(&temp, &entry, "main");

    // Recursion is forbidden (A035); the recompile fails the analysis channel.
    write_source(temp.path(), "lib/r.inf", "pub fn go() -> i32 { return go(); }");

    let mut cmd = Command::new(assert_cmd::cargo::cargo_bin!("infc"));
    cmd.current_dir(temp.path()).arg(&entry);
    cmd.assert()
        .failure()
        .stderr(predicate::str::contains("A035"));

    assert!(
        !temp.child("out").child("main.wasm").path().exists(),
        "an analysis rejection must not leave a runnable stale out/main.wasm"
    );
}

/// Single-file, type-check channel: the same guarantee holds without any imports.
#[test]
fn rejected_typecheck_clears_stale_single_file_wasm() {
    let temp = assert_fs::TempDir::new().unwrap();
    let entry = write_source(temp.path(), "prog.inf", "pub fn main() -> i32 { return 7; }");

    build_ok_and_assert_wasm(&temp, &entry, "prog");

    write_source(temp.path(), "prog.inf", "pub fn main() -> i32 { return nope(); }");

    let mut cmd = Command::new(assert_cmd::cargo::cargo_bin!("infc"));
    cmd.current_dir(temp.path()).arg(&entry);
    cmd.assert()
        .failure()
        .stderr(predicate::str::contains("Type checking failed"));

    assert!(
        !temp.child("out").child("prog.wasm").path().exists(),
        "a single-file type-check rejection must not leave a stale out/prog.wasm"
    );
}

/// Single-file, analysis channel: a recursive single-file program clears the
/// stale artifact too.
#[test]
fn rejected_analysis_clears_stale_single_file_wasm() {
    let temp = assert_fs::TempDir::new().unwrap();
    let entry = write_source(temp.path(), "prog.inf", "pub fn main() -> i32 { return 7; }");

    build_ok_and_assert_wasm(&temp, &entry, "prog");

    write_source(temp.path(), "prog.inf", "pub fn main() -> i32 { return main(); }");

    let mut cmd = Command::new(assert_cmd::cargo::cargo_bin!("infc"));
    cmd.current_dir(temp.path()).arg(&entry);
    cmd.assert()
        .failure()
        .stderr(predicate::str::contains("A035"));

    assert!(
        !temp.child("out").child("prog.wasm").path().exists(),
        "a single-file analysis rejection must not leave a stale out/prog.wasm"
    );
}

/// The stale-artifact guard must also clear the `.v` of a rejected proof-mode
/// build: with `-v`, both `out/main.wasm` and `out/main.v` of a previous good
/// build are removed when the recompile is rejected.
#[test]
fn rejected_build_clears_stale_v_with_dash_v() {
    let temp = assert_fs::TempDir::new().unwrap();
    let entry = write_source(
        temp.path(),
        "main.inf",
        "use lib::a;\npub fn main() -> i32 { return lib::a::seven(); }",
    );
    write_source(temp.path(), "lib/a.inf", "pub fn seven() -> i32 { return 7; }");

    let mut cmd = Command::new(assert_cmd::cargo::cargo_bin!("infc"));
    cmd.current_dir(temp.path()).arg(&entry).arg("-v");
    cmd.assert()
        .success()
        .stdout(predicate::str::contains("WASM generated"))
        .stdout(predicate::str::contains("V generated"));
    assert!(temp.child("out").child("main.wasm").path().exists());
    assert!(temp.child("out").child("main.v").path().exists());

    write_source(temp.path(), "lib/a.inf", "pub fn seven() -> i32 { return nope(); }");

    let mut cmd = Command::new(assert_cmd::cargo::cargo_bin!("infc"));
    cmd.current_dir(temp.path()).arg(&entry).arg("-v");
    cmd.assert().failure();

    assert!(
        !temp.child("out").child("main.wasm").path().exists(),
        "a rejected proof-mode build must not leave a stale out/main.wasm"
    );
    assert!(
        !temp.child("out").child("main.v").path().exists(),
        "a rejected proof-mode build must not leave a stale out/main.v"
    );
}

/// A parse-only or analyze-only run must NOT disturb a previous full build's
/// artifacts: the stale-artifact guard only clears outputs a codegen invocation
/// would itself write. Build fully, then run `--analyze` and confirm the WASM
/// from the earlier build is still on disk.
#[test]
fn analyze_only_does_not_clear_previous_build_artifact() {
    let temp = assert_fs::TempDir::new().unwrap();
    let entry = write_source(temp.path(), "prog.inf", "pub fn main() -> i32 { return 7; }");

    build_ok_and_assert_wasm(&temp, &entry, "prog");

    let mut cmd = Command::new(assert_cmd::cargo::cargo_bin!("infc"));
    cmd.current_dir(temp.path()).arg(&entry).arg("--analyze");
    cmd.assert().success();

    assert!(
        temp.child("out").child("prog.wasm").path().exists(),
        "an --analyze run must not clear a previous build's artifact"
    );
}

/// `--codegen -v` writes only the `.v` (no `.wasm`), so a previous build's
/// `out/main.wasm` is not the artifact this invocation rewrites. The stale guard
/// must still clear it: a `--codegen -v` rebuild rejected at `wasm_to_v` (here a
/// proof-name reservation) must leave NO runnable `out/main.wasm` behind from the
/// earlier good build. This is the specific gap the old `wants_wasm`-gated clear
/// missed.
#[test]
fn rejected_codegen_v_rebuild_clears_prior_wasm() {
    let temp = assert_fs::TempDir::new().unwrap();
    let entry = write_source(
        temp.path(),
        "main.inf",
        "spec Good { fn obligation() -> i32 { return 1; } }\npub fn main() -> i32 { return 7; }",
    );

    // A prior default build writes out/main.wasm.
    build_ok_and_assert_wasm(&temp, &entry, "main");

    // Introduce a proof-mode rejection and rebuild with --codegen -v (writes .v,
    // not .wasm — yet the prior .wasm must be cleared).
    write_source(
        temp.path(),
        "main.inf",
        "spec Bad_ { fn obligation() -> i32 { return 1; } }\npub fn main() -> i32 { return 7; }",
    );

    let mut cmd = Command::new(assert_cmd::cargo::cargo_bin!("infc"));
    cmd.current_dir(temp.path()).arg(&entry).arg("--codegen").arg("-v");
    cmd.assert().failure();

    assert!(
        !temp.child("out").child("main.wasm").path().exists(),
        "a rejected --codegen -v rebuild must clear the prior out/main.wasm"
    );
}

/// A plain `--codegen` rebuild (no `-o`, no `-v`) is a non-destructive dry run:
/// it writes no artifact, so it must not disturb a previous build's output — even
/// when the dry run is itself rejected. The stale-clear fires only when the run
/// will write at least one artifact (`generate_wasm_output || generate_v_output`);
/// a bare `--codegen` requests neither, so the earlier good build's `out/prog.wasm`
/// survives. Contrast the default and `--codegen -v` rejections below, which do
/// request output and so clear the now-stale prior artifact.
#[test]
fn rejected_codegen_only_rebuild_preserves_prior_wasm() {
    let temp = assert_fs::TempDir::new().unwrap();
    let entry = write_source(temp.path(), "prog.inf", "pub fn main() -> i32 { return 7; }");

    build_ok_and_assert_wasm(&temp, &entry, "prog");

    write_source(temp.path(), "prog.inf", "pub fn main() -> i32 { return nope(); }");

    let mut cmd = Command::new(assert_cmd::cargo::cargo_bin!("infc"));
    cmd.current_dir(temp.path()).arg(&entry).arg("--codegen");
    cmd.assert().failure();

    assert!(
        temp.child("out").child("prog.wasm").path().exists(),
        "a rejected --codegen-only dry run writes nothing, so the prior out/prog.wasm must survive"
    );
}

/// The default (no phase flags) rejected rebuild clears the prior `.wasm`: it
/// requests `.wasm` output, so the now-stale prior artifact must not be left
/// runnable. Pinned here alongside the `--codegen -v` rejection (which also
/// requests output and clears) and the bare `--codegen` dry run above (which
/// requests nothing and preserves), so all three rejection shapes are fixed
/// together.
#[test]
fn rejected_default_rebuild_clears_prior_wasm() {
    let temp = assert_fs::TempDir::new().unwrap();
    let entry = write_source(temp.path(), "prog.inf", "pub fn main() -> i32 { return 7; }");

    build_ok_and_assert_wasm(&temp, &entry, "prog");

    write_source(temp.path(), "prog.inf", "pub fn main() -> i32 { return nope(); }");

    let mut cmd = Command::new(assert_cmd::cargo::cargo_bin!("infc"));
    cmd.current_dir(temp.path()).arg(&entry);
    cmd.assert().failure();

    assert!(
        !temp.child("out").child("prog.wasm").path().exists(),
        "a rejected default rebuild must clear the prior out/prog.wasm"
    );
}

/// The success path is unchanged by the stale-clear behavior: a `--codegen -v`
/// success writes the `.v`, and a subsequent default success writes the `.wasm`.
#[test]
fn codegen_v_success_then_default_success_both_write_artifacts() {
    let temp = assert_fs::TempDir::new().unwrap();
    let entry = write_source(
        temp.path(),
        "main.inf",
        "spec Good { fn obligation() -> i32 { return 1; } }\npub fn main() -> i32 { return 7; }",
    );

    // --codegen -v writes the .v but not the .wasm.
    let mut cmd = Command::new(assert_cmd::cargo::cargo_bin!("infc"));
    cmd.current_dir(temp.path()).arg(&entry).arg("--codegen").arg("-v");
    cmd.assert()
        .success()
        .stdout(predicate::str::contains("V generated"));
    assert!(temp.child("out").child("main.v").path().exists());
    assert!(
        !temp.child("out").child("main.wasm").path().exists(),
        "--codegen -v must not write a .wasm"
    );

    // A following default build writes the .wasm (and clears the prior .v).
    build_ok_and_assert_wasm(&temp, &entry, "main");
}

// wasm_to_v rejection ordering: the Rocq translation runs before any artifact
// is written, so a `-v` build rejected by `wasm_to_v` (e.g. a spec named after a
// Rocq stdlib type, a keyword, or a `__`-containing name) leaves NO runnable
// `.wasm` behind. A `wasm_to_v` rejection at a non-zero exit must not be runnable.

/// A spec named `list` shadows the Rocq stdlib type and is rejected by
/// `wasm_to_v`. Because the translation runs before the WASM is written, no
/// runnable `out/main.wasm` (nor a `.v`) is left at the failing exit.
#[test]
fn wasm_to_v_stdlib_collision_leaves_no_wasm() {
    let temp = assert_fs::TempDir::new().unwrap();
    let entry = write_source(
        temp.path(),
        "main.inf",
        "spec list { fn ob() -> i32 { return 7; } }\npub fn main() -> i32 { return 7; }",
    );

    let mut cmd = Command::new(assert_cmd::cargo::cargo_bin!("infc"));
    cmd.current_dir(temp.path()).arg(&entry).arg("-v");
    cmd.assert()
        .failure()
        .stderr(predicate::str::contains("list"));

    assert!(
        !temp.child("out").child("main.wasm").path().exists(),
        "a wasm_to_v rejection must not leave a runnable out/main.wasm"
    );
    assert!(
        !temp.child("out").child("main.v").path().exists(),
        "a wasm_to_v rejection must not leave an out/main.v"
    );
}

/// A spec named after a Rocq keyword (`match`) is rejected by `wasm_to_v`; no
/// runnable WASM is left behind.
#[test]
fn wasm_to_v_keyword_spec_name_leaves_no_wasm() {
    let temp = assert_fs::TempDir::new().unwrap();
    let entry = write_source(
        temp.path(),
        "main.inf",
        "spec match { fn ob() -> i32 { return 7; } }\npub fn main() -> i32 { return 7; }",
    );

    let mut cmd = Command::new(assert_cmd::cargo::cargo_bin!("infc"));
    cmd.current_dir(temp.path()).arg(&entry).arg("-v");
    cmd.assert().failure();

    assert!(
        !temp.child("out").child("main.wasm").path().exists(),
        "a Rocq-keyword spec name rejection must not leave a runnable out/main.wasm"
    );
}

/// The same guard under `--out-dir`: a `wasm_to_v` rejection leaves no runnable
/// artifact in the requested directory either.
#[test]
fn wasm_to_v_rejection_leaves_no_wasm_under_out_dir() {
    let temp = assert_fs::TempDir::new().unwrap();
    let entry = write_source(
        temp.path(),
        "main.inf",
        "spec list { fn ob() -> i32 { return 7; } }\npub fn main() -> i32 { return 7; }",
    );

    let mut cmd = Command::new(assert_cmd::cargo::cargo_bin!("infc"));
    cmd.current_dir(temp.path())
        .arg(&entry)
        .arg("-v")
        .arg("--out-dir")
        .arg("build");
    cmd.assert().failure();

    assert!(
        !temp.child("build").child("main.wasm").path().exists(),
        "a wasm_to_v rejection under --out-dir must not leave a runnable build/main.wasm"
    );
    assert!(
        !temp.child("build").child("main.v").path().exists(),
        "a wasm_to_v rejection under --out-dir must not leave a build/main.v"
    );
}

/// A plain compile (no `-v`) after an earlier `-v` build must not leave a stale
/// `.v` describing the old program: the proof artifact is invalidated by the
/// since-changed source. The fresh `.wasm` is still written.
#[test]
fn plain_compile_clears_stale_v_from_prior_proof_build() {
    let temp = assert_fs::TempDir::new().unwrap();
    let entry = write_source(temp.path(), "main.inf", "pub fn main() -> i32 { return 1; }");

    // First, a proof build writes both artifacts.
    let mut cmd = Command::new(assert_cmd::cargo::cargo_bin!("infc"));
    cmd.current_dir(temp.path()).arg(&entry).arg("-v");
    cmd.assert().success();
    assert!(temp.child("out").child("main.v").path().exists());

    // Edit the program and rebuild WITHOUT -v.
    write_source(temp.path(), "main.inf", "pub fn main() -> i32 { return 42; }");
    let mut cmd = Command::new(assert_cmd::cargo::cargo_bin!("infc"));
    cmd.current_dir(temp.path()).arg(&entry);
    cmd.assert()
        .success()
        .stdout(predicate::str::contains("WASM generated"));

    assert!(
        temp.child("out").child("main.wasm").path().exists(),
        "a plain compile must still write out/main.wasm"
    );
    assert!(
        !temp.child("out").child("main.v").path().exists(),
        "a plain compile must not leave a stale out/main.v from a prior -v build"
    );
}

/// The success path is unchanged: a clean `-v` build writes both the `.wasm` and
/// the `.v`, and the `.wasm` is valid (the deferred-write ordering does not alter
/// a successful build).
#[test]
fn dash_v_success_still_writes_both_artifacts() {
    let temp = assert_fs::TempDir::new().unwrap();
    let entry = write_source(temp.path(), "main.inf", "pub fn main() -> i32 { return 7; }");

    let mut cmd = Command::new(assert_cmd::cargo::cargo_bin!("infc"));
    cmd.current_dir(temp.path()).arg(&entry).arg("-v");
    cmd.assert()
        .success()
        .stdout(predicate::str::contains("WASM generated"))
        .stdout(predicate::str::contains("V generated"));

    assert!(temp.child("out").child("main.wasm").path().exists());
    assert!(temp.child("out").child("main.v").path().exists());
}
