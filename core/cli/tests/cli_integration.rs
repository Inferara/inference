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

/// Pins the ABI version string to the literal value `--host-imports` became
/// requestable at. The `abi_version_flag_prints_and_exits` test above checks the
/// binary against the shared constant; this one additionally asserts the
/// concrete `1.8` so an accidental constant change is caught here too.
///
/// Uses an exact trimmed equality (not `contains`) so a near-miss such as
/// "11.8" or "1.80" — which would satisfy a substring match — cannot pass.
#[test]
fn abi_version_is_one_dot_eight() {
    let mut cmd = Command::new(assert_cmd::cargo::cargo_bin!("infc"));
    cmd.arg("--abi-version");
    let assert = cmd.assert().success();
    let output = assert.get_output();
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert_eq!(
        stdout.trim(),
        "1.8",
        "ABI version must be exactly 1.8, not merely contain it"
    );
}

/// A source with a compound copy, so the artifact is one whose bytes differ
/// between the WebAssembly 1.0 lowering and the bulk-memory instructions.
const COMPOUND_COPY_SOURCE: &str = "\
struct Point { x: i32; y: i32; }

pub fn main() -> i32 {
    let p: Point = Point { x: 1, y: 2 };
    let q: Point = p;
    return q.x;
}
";

/// The `0xFC 0x0A` / `0xFC 0x0B` prefixed opcodes of `memory.copy` and
/// `memory.fill`, spelled out so the assertion does not depend on a disassembler.
fn contains_bulk_memory_opcode(wasm: &[u8]) -> bool {
    wasm.windows(2)
        .any(|w| w[0] == 0xFC && (w[1] == 0x0A || w[1] == 0x0B))
}

fn compile_source_with(args: &[&str], source: &str) -> Vec<u8> {
    let temp = assert_fs::TempDir::new().unwrap();
    let dest = temp.child("prog.inf");
    std::fs::write(dest.path(), source).unwrap();

    let mut cmd = Command::new(assert_cmd::cargo::cargo_bin!("infc"));
    cmd.current_dir(temp.path()).arg(dest.path()).args(args);
    cmd.assert().success();

    std::fs::read(temp.child("out").child("prog.wasm").path())
        .expect("infc must have written out/prog.wasm")
}

/// `--wasm-features bulk-memory` reaches code generation: the artifact carries
/// bulk-memory opcodes that the same source compiles without by default.
#[test]
fn wasm_features_bulk_memory_reaches_codegen() {
    let default_build = compile_source_with(&[], COMPOUND_COPY_SOURCE);
    assert!(
        !contains_bulk_memory_opcode(&default_build),
        "the default build must stay within WebAssembly 1.0"
    );

    let bulk_build = compile_source_with(&["--wasm-features", "bulk-memory"], COMPOUND_COPY_SOURCE);
    assert!(
        contains_bulk_memory_opcode(&bulk_build),
        "--wasm-features bulk-memory must emit memory.copy/memory.fill"
    );
    assert_ne!(
        default_build, bulk_build,
        "the two instruction levels must produce different artifacts"
    );
}

/// The features apply identically in proof mode — the `.v` must describe the same
/// program as the `.wasm`, so nothing may gate them on the compilation mode.
#[test]
fn wasm_features_apply_in_proof_mode_too() {
    let bulk_proof = compile_source_with(
        &["--wasm-features", "bulk-memory", "--mode", "proof"],
        COMPOUND_COPY_SOURCE,
    );
    assert!(
        contains_bulk_memory_opcode(&bulk_proof),
        "a proof-mode build must honor the requested features"
    );
}

/// An unrecognized name fails the build before any phase runs, rather than being
/// ignored — a build that quietly emitted a different instruction level than
/// requested is the failure this rejects.
#[test]
fn unknown_wasm_feature_is_rejected_before_any_output() {
    let temp = assert_fs::TempDir::new().unwrap();
    let src = example_file("trivial.inf");
    let dest = temp.child("trivial.inf");
    std::fs::copy(&src, dest.path()).unwrap();

    let mut cmd = Command::new(assert_cmd::cargo::cargo_bin!("infc"));
    cmd.current_dir(temp.path())
        .arg(dest.path())
        .arg("--wasm-features")
        .arg("memory.fill");

    cmd.assert()
        .failure()
        .stderr(predicate::str::contains("is an instruction, not a feature"))
        .stderr(predicate::str::contains("write `bulk-memory`"));

    assert!(
        !temp.child("out").child("trivial.wasm").path().exists(),
        "a rejected feature request must leave no artifact"
    );
}

// Target selection ---

/// Runs `infc` with the given `--target` value against a real source file and
/// returns the rejection's stderr, having first checked that nothing was written.
///
/// A target the compiler does not build for must be refused before any phase
/// runs, so the failure is about the request rather than about a program the
/// user then has to re-read.
fn reject_target(entry: &str) -> String {
    let temp = assert_fs::TempDir::new().unwrap();
    let src = example_file("trivial.inf");
    let dest = temp.child("trivial.inf");
    std::fs::copy(&src, dest.path()).unwrap();

    let mut cmd = Command::new(assert_cmd::cargo::cargo_bin!("infc"));
    cmd.current_dir(temp.path())
        .arg(dest.path())
        .arg("--target")
        .arg(entry);

    let assert = cmd.assert().failure();
    let stderr = String::from_utf8_lossy(&assert.get_output().stderr).into_owned();

    assert!(
        !temp.child("out").child("trivial.wasm").path().exists(),
        "a rejected target must leave no artifact"
    );
    stderr
}

/// Naming the default target explicitly must produce the same bytes as naming
/// none. The flag selects a target; it is not itself an input to emission, and a
/// build that differed here would mean the plumbing had reached the emitter.
#[test]
fn naming_the_default_target_changes_no_byte() {
    let implicit = compile_source_with(&[], COMPOUND_COPY_SOURCE);
    let explicit = compile_source_with(&["--target", "wasm32"], COMPOUND_COPY_SOURCE);
    assert_eq!(
        implicit, explicit,
        "`--target wasm32` must be byte-identical to omitting the flag"
    );
}

/// A former spelling of a target that is now requestable earns the dedicated
/// sentence redirecting to the current one. A generic "unknown target" would be
/// misleading about a name that does exist in the code, and the old spelling is
/// guessable.
///
/// Driven from the slice rather than from a literal, so promoting a name out of
/// it cannot leave this test asserting about something that is no longer there.
#[test]
fn a_reserved_target_is_refused_with_the_spelling_that_replaced_it() {
    for reserved in inference_compiler_interface::RESERVED_TARGET_NAMES {
        let stderr = reject_target(reserved);
        assert!(
            stderr.contains("is the former name of the `stellar` target")
                && stderr.contains("write `stellar` instead"),
            "`{reserved}` must be refused with the redirect, got: {stderr}"
        );
    }
}

/// An unrecognized name lists what is accepted, rather than falling back to the
/// default: a build that quietly targeted something other than what was asked
/// for is the failure this rejects.
#[test]
fn an_unknown_target_is_refused_listing_the_supported_set() {
    let stderr = reject_target("nope");
    assert!(
        stderr.contains("unknown compilation target"),
        "the rejection must say what went wrong, got: {stderr}"
    );
    for target in inference_compiler_interface::TargetName::ALL {
        assert!(
            stderr.contains(&format!("`{}`", target.as_str())),
            "the rejection must list `{}`, got: {stderr}",
            target.as_str()
        );
    }
    // The loop above passes on a listing that renders the names in any order.
    // This is the literal a user copies from, spelled out rather than compared
    // against the renderer that produced it: an assertion reading
    // `supported_targets_listing()` would move with any reordering or requoting
    // of the vocabulary and could not report one.
    assert!(
        stderr.contains("`wasm32`, `stellar`, `spacewasm`"),
        "the rejection must render the listing in the vocabulary's order, got: {stderr}"
    );
    assert!(
        stderr.contains("`--target`"),
        "the rejection must name the surface, got: {stderr}"
    );
}

/// Whitespace is rejected, never trimmed — and the message says so, because a
/// space around the name is invisible in the echoed value and the user would
/// otherwise read their own correct spelling reported back as unknown.
#[test]
fn whitespace_around_a_target_name_is_refused_naming_the_cause() {
    let stderr = reject_target(" wasm32");
    assert!(
        stderr.contains("surrounding whitespace") && stderr.contains("write `wasm32`"),
        "the rejection must name the whitespace, got: {stderr}"
    );
}

// The Stellar target ---

/// A contract whose three methods cover every admissible shape at once: two
/// parameters and a value return, a `bool` in both positions, and a method that
/// returns nothing.
const STELLAR_CONTRACT_SOURCE: &str = "\
pub fn add(a: u32, b: u32) -> u32 {
    return a + b;
}

pub fn flip(v: bool) -> bool {
    if v { return false; }
    return true;
}

pub fn nothing() {
    return;
}
";

/// Compiles `source` at the Stellar target and returns the written module
/// alongside the run's stdout, so the artifact and the line describing it are
/// asserted against one build.
fn compile_stellar(source: &str) -> (Vec<u8>, String) {
    let temp = assert_fs::TempDir::new().unwrap();
    let dest = temp.child("prog.inf");
    std::fs::write(dest.path(), source).unwrap();

    let mut cmd = Command::new(assert_cmd::cargo::cargo_bin!("infc"));
    cmd.current_dir(temp.path())
        .arg(dest.path())
        .arg("--target")
        .arg("stellar");
    let assert = cmd.assert().success();
    let stdout = String::from_utf8_lossy(&assert.get_output().stdout).into_owned();

    let wasm = std::fs::read(temp.child("out").child("prog.wasm").path())
        .expect("infc must have written out/prog.wasm");
    (wasm, stdout)
}

/// Compiles `source` at the Stellar target expecting a refusal, and returns the
/// stderr having first checked that no artifact survives.
///
/// The absence check is the load-bearing half. `infc` holds every output in
/// memory until the whole pipeline has succeeded precisely so a late rejection —
/// and the Val-ABI rewrite is the latest one there is — cannot leave a module on
/// disk that no host would accept.
fn reject_stellar(source: &str) -> String {
    let temp = assert_fs::TempDir::new().unwrap();
    let dest = temp.child("prog.inf");
    std::fs::write(dest.path(), source).unwrap();

    let mut cmd = Command::new(assert_cmd::cargo::cargo_bin!("infc"));
    cmd.current_dir(temp.path())
        .arg(dest.path())
        .arg("--target")
        .arg("stellar");
    let assert = cmd.assert().failure();

    assert!(
        !temp.child("out").child("prog.wasm").path().exists(),
        "a refused Stellar build must leave no artifact"
    );
    String::from_utf8_lossy(&assert.get_output().stderr).into_owned()
}

/// The declaration line of the function `name` is exported as, in `wat`.
///
/// Resolved through the export entry rather than by guessing the function's
/// symbol, so the assertion is about what a host reaches when it calls `name`
/// and not about how the rewriter chose to label the body.
fn exported_function_declaration(wat: &str, name: &str) -> String {
    let export = format!("(export \"{name}\" (func ");
    let rest = wat
        .split_once(&export)
        .unwrap_or_else(|| panic!("`{name}` must be exported:\n{wat}"))
        .1;
    let symbol = rest
        .split_once(')')
        .unwrap_or_else(|| panic!("the export of `{name}` must name a function:\n{wat}"))
        .0;
    let declaration = format!("(func {symbol} ");
    wat.lines()
        .find(|line| line.trim_start().starts_with(&declaration))
        .unwrap_or_else(|| panic!("`{name}` is exported as {symbol}, which no function declares:\n{wat}"))
        .trim()
        .to_string()
}

/// The whole point of the target: every contract method is reached through the
/// host's calling convention.
///
/// Asserted through the rendered module rather than through raw bytes, because
/// the claim is about the exported functions' *types* — one 64-bit word per
/// declared parameter, and one back — including for the method whose source
/// returns nothing, since the host's void is a value like any other. A byte
/// assertion here would pin the rewriter's encoding, which is the rewriter
/// crate's own business.
#[test]
fn a_stellar_build_exports_every_method_in_the_value_abi() {
    let (wasm, _) = compile_stellar(STELLAR_CONTRACT_SOURCE);
    let wat = wat_of(&wasm);

    for (method, params) in [("add", 2), ("flip", 1), ("nothing", 0)] {
        let declaration = exported_function_declaration(&wat, method);
        let expected = if params == 0 {
            String::from("(result i64)")
        } else {
            format!("(param{}) (result i64)", " i64".repeat(params))
        };
        assert!(
            declaration.ends_with(&expected),
            "`{method}` must be exported as a function taking {params} value \
             word(s) and returning one — expected a declaration ending \
             `{expected}`, got `{declaration}`"
        );
    }

    let plain = wat_of(&compile_source_with(&[], STELLAR_CONTRACT_SOURCE));
    let plain_add = exported_function_declaration(&plain, "add");
    assert!(
        plain_add.ends_with("(result i32)") && !plain_add.contains("i64"),
        "the default build must export the source signature, untouched — \
         otherwise the assertions above are about code generation rather than \
         about the rewrite, got `{plain_add}`"
    );
}

/// A module with no `contractenvmetav0` section is refused at upload by every
/// Soroban host there has ever been, so its presence is not a detail of the
/// encoding — it is the difference between an artifact and a deployable one.
///
/// Searched for as bytes rather than in the rendered text: a custom section's
/// name survives disassembly, but pinning the *bytes* is what proves the section
/// reached the file rather than merely the printer.
#[test]
fn a_stellar_build_carries_the_environment_metadata_section() {
    let (wasm, _) = compile_stellar(STELLAR_CONTRACT_SOURCE);
    assert!(
        wasm.windows(17).any(|w| w == b"contractenvmetav0"),
        "the written module must carry the environment metadata section"
    );

    let plain = compile_source_with(&["--target", "wasm32"], STELLAR_CONTRACT_SOURCE);
    assert!(
        !plain.windows(17).any(|w| w == b"contractenvmetav0"),
        "the default target must not carry it — otherwise the check above is \
         about code generation rather than about the rewrite"
    );
}

/// The summary line is the only place a build says what it just made
/// deployable, and each field earns its place: a Soroban host reports a
/// wrong-arity call without naming the arity it expected, and reports a
/// wrong-typed argument as an undiscriminated trap, so the build log is where a
/// caller finds out what to send.
#[test]
fn a_stellar_build_summarizes_the_contract_it_wrote() {
    let (wasm, stdout) = compile_stellar(STELLAR_CONTRACT_SOURCE);
    let summary = stdout
        .lines()
        .find(|line| line.starts_with("Stellar contract:"))
        .unwrap_or_else(|| panic!("the build must summarize the contract, got:\n{stdout}"));

    for method in ["add/2", "flip/1", "nothing/0"] {
        assert!(
            summary.contains(method),
            "the summary must name `{method}`, got: {summary}"
        );
    }
    assert!(
        summary.contains("env protocol 20"),
        "the summary must declare the environment protocol, got: {summary}"
    );
    assert!(
        summary.contains(&format!("{} bytes", wasm.len())),
        "the summary must report the size of the module actually written \
         ({} bytes), got: {summary}",
        wasm.len()
    );
}

/// Every shape the target refuses, refused through the CLI with no artifact left
/// behind, each naming the thing the author has to change.
///
/// The library gate is covered in the `inference-tests` crate; what is exercised
/// here is that the refusal survives the CLI — that `infc` reports it, exits
/// non-zero, and leaves nothing on disk. The over-long name and the empty module
/// are included because they are refusals about the *module*, not about a
/// declaration, and a gate that only inspected parameter types would pass them.
#[test]
fn every_inadmissible_contract_shape_is_refused_with_no_artifact() {
    let cases: &[(&str, &str, &[&str])] = &[
        (
            "a 64-bit parameter",
            "pub fn f(a: u64) -> u32 { return 1; }\n",
            &["'f'", "parameter 1 'a'", "'u64'"],
        ),
        (
            "a narrow parameter",
            "pub fn f(a: u8) -> u32 { return 1; }\n",
            &["'f'", "parameter 1 'a'", "'u8'"],
        ),
        (
            "a struct parameter",
            "struct Point { x: i32; y: i32; }\n\
             pub fn f(p: Point) -> u32 { return 1; }\n",
            &["'f'", "parameter 1 'p'", "Point"],
        ),
        (
            "an array return",
            "pub fn f() -> [i32; 2] { return [1, 2]; }\n",
            &["'f'"],
        ),
        (
            "an over-long export name",
            "pub fn aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa() -> u32 { return 1; }\n",
            &["aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa", "32"],
        ),
        (
            "no exported function",
            "fn f() -> u32 { return 1; }\n",
            &["exports no function"],
        ),
    ];

    for (label, source, fragments) in cases {
        let stderr = reject_stellar(source);
        for fragment in *fragments {
            assert!(
                stderr.contains(fragment),
                "{label}: the refusal must name `{fragment}`, got: {stderr}"
            );
        }
    }
}

/// `--mode compile -v` is the one spelling that keeps compile mode and still asks
/// for a Rocq artifact, and at this target the two would describe different
/// modules: the translation reads the linked bytes, the rewrite happens after
/// them, and the `.v` would be about a module that is not the one on disk.
///
/// Refused before any phase runs, so neither artifact appears.
#[test]
fn a_proof_artifact_is_refused_at_the_stellar_target() {
    let temp = assert_fs::TempDir::new().unwrap();
    let dest = temp.child("prog.inf");
    std::fs::write(dest.path(), STELLAR_CONTRACT_SOURCE).unwrap();

    let mut cmd = Command::new(assert_cmd::cargo::cargo_bin!("infc"));
    cmd.current_dir(temp.path())
        .arg(dest.path())
        .arg("--target")
        .arg("stellar")
        .arg("--mode")
        .arg("compile")
        .arg("-v");

    let assert = cmd.assert().failure();
    let stderr = String::from_utf8_lossy(&assert.get_output().stderr).into_owned();
    assert!(
        stderr.contains("-v cannot be combined with --target stellar")
            && stderr.contains("pre-rewrite"),
        "the refusal must name the divergence, got: {stderr}"
    );
    assert!(
        !temp.child("out").child("prog.wasm").path().exists()
            && !temp.child("out").child("prog.v").path().exists(),
        "the refusal must leave neither artifact"
    );
}

/// Proof mode itself is refused at this target too, and by code generation
/// rather than by the check above: the target cannot decode the custom
/// non-deterministic instructions proof mode emits at all. Pinned here because
/// the two refusals are one user-facing rule — no Rocq artifact from a Stellar
/// build — reached by two different spellings, and a change that dropped either
/// would leave the other looking like full coverage.
///
/// Both spellings of proof mode are run, and the bare `-v` is the load-bearing
/// one. The refusal above is deliberately narrowed to compile mode *because*
/// `-v` alone normalizes to proof mode and lands here instead; if that
/// normalization ever changed, the narrow refusal would stop covering it in
/// silence and `infc file.inf -v --target stellar` would write a `.v`
/// describing the pre-rewrite module beside the contract. Nothing else in the
/// suite would notice.
#[test]
fn proof_mode_is_refused_at_the_stellar_target_in_both_spellings() {
    for spelling in [&["--mode", "proof"][..], &["-v"][..]] {
        let temp = assert_fs::TempDir::new().unwrap();
        let dest = temp.child("prog.inf");
        std::fs::write(dest.path(), STELLAR_CONTRACT_SOURCE).unwrap();

        let mut cmd = Command::new(assert_cmd::cargo::cargo_bin!("infc"));
        cmd.current_dir(temp.path())
            .arg(dest.path())
            .arg("--target")
            .arg("stellar")
            .args(spelling);

        let assert = cmd.assert().failure();
        let stderr = String::from_utf8_lossy(&assert.get_output().stderr).into_owned();
        assert!(
            stderr.contains("Proof mode requires the `wasm32` target")
                && stderr.contains("the `stellar` runtime rejects a module carrying them"),
            "{spelling:?}: the refusal must name the mode and the target, got: {stderr}"
        );
        assert!(
            !temp.child("out").child("prog.wasm").path().exists()
                && !temp.child("out").child("prog.v").path().exists(),
            "{spelling:?}: the refusal must leave neither artifact"
        );
    }
}

/// The static-merge linker renumbers functions, so the export descriptor's
/// indices are stale by the time the rewriter runs — which is why the rewriter
/// matches an export by *name*. That is load-bearing and nothing before this
/// exercises it: every other Stellar fixture links nothing, so no index moves.
///
/// The fixture is the committed foreign-toolchain artifact, an unmodified
/// `cargo build --target wasm32-unknown-unknown` output whose function
/// numbering owes nothing to this compiler.
#[test]
fn a_stellar_build_that_links_an_external_still_resolves_its_exports_by_name() {
    let wasmlib = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .unwrap()
        .parent()
        .unwrap()
        .join("tests")
        .join("test_data")
        .join("wasmlib");

    let temp = assert_fs::TempDir::new().unwrap();
    let dest = temp.child("prog.inf");
    std::fs::write(
        dest.path(),
        "external fn clamp_add(a: i32, b: i32) -> i32;\n\
         use { clamp_add } from rustlib;\n\n\
         pub fn saturating_add(a: i32, b: i32) -> i32 {\n\
         \x20   return clamp_add(a, b);\n\
         }\n",
    )
    .unwrap();

    let mut cmd = Command::new(assert_cmd::cargo::cargo_bin!("infc"));
    cmd.current_dir(temp.path())
        .arg(dest.path())
        .arg("-L")
        .arg(&wasmlib)
        .arg("--memory-pages")
        .arg("16")
        .arg("--target")
        .arg("stellar");
    let assert = cmd.assert().success();
    let stdout = String::from_utf8_lossy(&assert.get_output().stdout).into_owned();
    assert!(
        stdout.contains("Linked 1 external module(s)"),
        "the fixture must actually link, or the renumbering never happens:\n{stdout}"
    );

    let wasm = std::fs::read(temp.child("out").child("prog.wasm").path())
        .expect("infc must have written out/prog.wasm");
    let wat = wat_of(&wasm);
    assert!(
        exported_function_declaration(&wat, "saturating_add")
            .ends_with("(param i64 i64) (result i64)"),
        "the wrapper must have been attached to the right function after the \
         merge renumbered everything:\n{wat}"
    );
    assert!(
        wasm.windows(17).any(|w| w == b"contractenvmetav0"),
        "a linked build is a contract like any other"
    );
}

/// A foreign artifact whose only post-WebAssembly-1.0 content is one
/// sign-extension instruction, which is what a stock Rust
/// `wasm32-unknown-unknown` build emits by default. Everything else about it is
/// ordinary: it exports one function with a signature an `external fn` can
/// declare.
const SIGN_EXTENDING_EXTERNAL: &str = r#"(module
  (func (export "clamp_add") (param i32 i32) (result i32)
    local.get 0
    i32.extend8_s
    local.get 1
    i32.add))
"#;

/// A program binding that export.
const CALLS_SIGN_EXTENDING_EXTERNAL: &str = "\
external fn clamp_add(a: i32, b: i32) -> i32;
use { clamp_add } from rustlib;

pub fn saturating_add(a: i32, b: i32) -> i32 {
    return clamp_add(a, b);
}
";

/// Stages the sign-extending external under a `-L` directory beside a program
/// that binds it, returning the temp directory, the entry path and the search
/// directory.
fn sign_extending_project() -> (assert_fs::TempDir, std::path::PathBuf, std::path::PathBuf) {
    let temp = assert_fs::TempDir::new().unwrap();
    let entry = write_source(temp.path(), "prog.inf", CALLS_SIGN_EXTENDING_EXTERNAL);
    let lib = temp.child("lib");
    std::fs::create_dir_all(lib.path()).unwrap();
    let bytes = wat::parse_str(SIGN_EXTENDING_EXTERNAL).expect("the external fixture is valid WAT");
    std::fs::write(lib.child("rustlib.wasm").path(), bytes).unwrap();
    let dir = lib.path().to_path_buf();
    (temp, entry, dir)
}

/// A contract is WebAssembly 1.0 throughout, so an external outside it is
/// refused — by name, by path, and before the merge.
///
/// The linker accepts sign extension, so nothing before the Val-ABI rewrite
/// objects, and the rewrite sees one merged module: its refusal names a byte
/// offset into bytes no file holds, says nothing about which artifact is at
/// fault, and offers nothing to do about it. Every committed external fixture
/// happens to be within 1.0, so the whole suite passed over this.
///
/// The negative control is the same external at the default target, which links
/// it and writes the artifact — which is what makes the advertised remedy an
/// actual remedy rather than a sentence.
///
/// The opening names the target as `--target` spells it, and the SpaceWasm twin
/// below asserts its own: two targets pinning their own prefix is what makes
/// the name an interpolation rather than a literal one of them could keep while
/// the other drifted.
#[test]
fn a_foreign_module_outside_webassembly_one_is_refused_before_the_merge() {
    let (temp, entry, lib) = sign_extending_project();

    let mut cmd = Command::new(assert_cmd::cargo::cargo_bin!("infc"));
    cmd.current_dir(temp.path())
        .arg(&entry)
        .arg("-L")
        .arg(&lib)
        .arg("--target")
        .arg("stellar");
    let assert = cmd.assert().failure();
    let stderr = String::from_utf8_lossy(&assert.get_output().stderr).into_owned();
    let stdout = String::from_utf8_lossy(&assert.get_output().stdout).into_owned();

    let resolved = lib.join("rustlib.wasm");
    for fragment in [
        "The `stellar` target:",
        "`rustlib`",
        resolved.to_str().expect("temp paths are UTF-8"),
        "not a WebAssembly 1.0 module",
        "sign extension",
        "Rebuild `rustlib`",
        "--target wasm32",
    ] {
        assert!(
            stderr.contains(fragment),
            "the refusal must carry `{fragment}`, got: {stderr}"
        );
    }
    assert!(
        !stdout.contains("Linked"),
        "the refusal must come before the merge, or it cannot name a file: {stdout}"
    );
    assert!(
        !temp.child("out").child("prog.wasm").path().exists(),
        "a refused build must leave no artifact"
    );

    let (temp, entry, lib) = sign_extending_project();
    let mut cmd = Command::new(assert_cmd::cargo::cargo_bin!("infc"));
    cmd.current_dir(temp.path()).arg(&entry).arg("-L").arg(&lib);
    let assert = cmd.assert().success();
    assert!(
        String::from_utf8_lossy(&assert.get_output().stdout).contains("Linked 1 external module(s)"),
        "the default target must link the very same module, or the refusal above \
         is about a broken fixture rather than about the instruction set"
    );
    assert!(
        temp.child("out").child("prog.wasm").path().exists(),
        "the default target must write the artifact it linked"
    );
}

/// `--codegen` without `-o` writes nothing, so it has no contract to describe
/// and no size to report — and it still refuses a program that could not be a
/// contract.
///
/// Both halves are one test because silencing the summary and silencing the
/// checks are the same edit away from each other: a gate on the whole target
/// branch rather than on the one line would turn `--codegen --target stellar`
/// from a check into a no-op that reports success for a program no host would
/// accept.
///
/// What the second half observes is the source-level export gate, which runs
/// inside code generation. The Val-ABI rewrite is left unconditional too, but
/// nothing here can see that: every refusal the rewrite owns that a user can
/// reach from source is already refused by an earlier gate — the export shapes
/// by that same gate, a post-1.0 external before the merge, an external's own
/// unsatisfied import by the linker — so in a build that writes no file the
/// rewrite has no observable effect at all. It stays unconditional because the
/// day one of its refusals stops being shadowed is not the day to discover that
/// half the invocations skip it.
#[test]
fn a_phase_only_stellar_build_describes_no_artifact_and_still_refuses() {
    let temp = assert_fs::TempDir::new().unwrap();
    let entry = write_source(temp.path(), "prog.inf", STELLAR_CONTRACT_SOURCE);
    let mut cmd = Command::new(assert_cmd::cargo::cargo_bin!("infc"));
    cmd.current_dir(temp.path())
        .arg(&entry)
        .arg("--codegen")
        .arg("--target")
        .arg("stellar");
    let assert = cmd.assert().success();
    let stdout = String::from_utf8_lossy(&assert.get_output().stdout).into_owned();
    assert!(
        !stdout.contains("Stellar contract:"),
        "a build that writes no file must not report the size of one: {stdout}"
    );
    assert!(
        !temp.child("out").child("prog.wasm").path().exists(),
        "--codegen alone writes nothing"
    );

    let temp = assert_fs::TempDir::new().unwrap();
    let entry = write_source(
        temp.path(),
        "prog.inf",
        "pub fn f(a: u64) -> u32 { return 1; }\n",
    );
    let mut cmd = Command::new(assert_cmd::cargo::cargo_bin!("infc"));
    cmd.current_dir(temp.path())
        .arg(&entry)
        .arg("--codegen")
        .arg("--target")
        .arg("stellar");
    let assert = cmd.assert().failure();
    assert!(
        String::from_utf8_lossy(&assert.get_output().stderr).contains("parameter 1 'a'"),
        "a phase-only build must hold a contract to the same admissible set as a \
         writing one"
    );
}

/// The artifact `infc` wrote for [`STELLAR_CONTRACT_SOURCE`] at the default
/// target *before this target existed* — produced by the binary built from the
/// commit this work branched from, which had no `--target` flag at all.
///
/// Committed as bytes rather than pinned as a hash because `inference-cli` has
/// no hashing crate among its dev-dependencies, and adding one would stale the
/// tracked `ci/rocq-discharge.cargo-lock`, which aborts that CI lane before it
/// compiles anything. The bytes are 186 of them; a hash would save nothing and
/// tell a reader less.
///
/// To regenerate after a deliberate change to code generation or to the write
/// path, from a scratch directory, with `$SOURCE` holding
/// `STELLAR_CONTRACT_SOURCE` verbatim:
///
/// ```text
/// printf '%s' "$SOURCE" > prog.inf
/// infc prog.inf
/// cp out/prog.wasm core/cli/tests/artifacts/three_methods_default_target.wasm
/// ```
///
/// The file name matters: it reaches the module's name section.
const DEFAULT_TARGET_ARTIFACT: &[u8] =
    include_bytes!("artifacts/three_methods_default_target.wasm");

/// Selecting the Stellar target must change nothing about a *default* build.
///
/// The rewrite is a branch in the write path, and a branch is exactly the kind of
/// edit that can leak: the golden artifacts in the `inference-tests` crate cover
/// `codegen()` and never reach this code at all.
///
/// The pin is the committed pre-campaign artifact. Comparing two builds of the
/// *current* binary against each other cannot do this job: an edit that changed
/// what every default build writes would move both sides together and pass, and
/// so would one that changed only what a build naming no target writes. Only
/// bytes produced before the branch existed can tell either of those from a
/// build that is genuinely unchanged.
#[test]
fn the_stellar_branch_leaves_the_default_write_path_untouched() {
    let implicit = compile_source_with(&[], STELLAR_CONTRACT_SOURCE);
    assert_eq!(
        implicit.as_slice(),
        DEFAULT_TARGET_ARTIFACT,
        "a build naming no target must write the {} bytes it wrote before this \
         target existed, and wrote {} instead",
        DEFAULT_TARGET_ARTIFACT.len(),
        implicit.len()
    );

    let explicit = compile_source_with(&["--target", "wasm32"], STELLAR_CONTRACT_SOURCE);
    assert_eq!(
        implicit, explicit,
        "a default build must not depend on whether the default was named"
    );

    let (stellar, _) = compile_stellar(STELLAR_CONTRACT_SOURCE);
    assert_ne!(
        implicit, stellar,
        "the Stellar build must differ — otherwise the comparison above is \
         vacuous because the rewrite never ran"
    );
}

// The SpaceWasm target ---

/// A SpaceWasm build reports the two numbers an embedder must be built with,
/// and reports them whether or not it writes a file.
///
/// The summary is the whole user-facing half of the conformance check: the
/// refusal only fires on a module the target cannot load, while every
/// successful build has to hand a flight integrator the control-frame and
/// operand-stack budgets, which nothing else in the toolchain can tell them.
/// Both numbers carry their unit in the line, because the value count and the
/// word count are different quantities and sizing either const generic from the
/// wrong one is exactly the deploy-time failure this check exists to prevent.
///
/// The `--codegen` half is the deliberate asymmetry with the Stellar summary
/// next door, which is withheld from a build that writes nothing because it
/// reports a file's size. This line reports properties of the module, which a
/// phase-only build has just as much.
///
/// Fails if the summary is gated on the write, if it stops naming the function
/// that attains a maximum, if either unit is dropped, or if the number an
/// embedder is told to build with stops being the number that was measured.
#[test]
fn a_spacewasm_build_reports_the_budget_its_embedder_needs() {
    let temp = assert_fs::TempDir::new().unwrap();
    let entry = write_source(temp.path(), "prog.inf", COMPOUND_COPY_SOURCE);
    let mut cmd = Command::new(assert_cmd::cargo::cargo_bin!("infc"));
    cmd.current_dir(temp.path())
        .arg(&entry)
        .arg("--target")
        .arg("spacewasm");
    let assert = cmd.assert().success();
    let stdout = String::from_utf8_lossy(&assert.get_output().stdout).into_owned();
    let stderr = String::from_utf8_lossy(&assert.get_output().stderr).into_owned();
    assert!(
        !stderr.contains("warning: spacewasm"),
        "this module is far inside the reference budget, so a warning here would mean the \
         warnings fire unconditionally: {stderr}"
    );

    for fragment in [
        "spacewasm: conformant with WebAssembly 1.0",
        "deepest control nesting",
        "tallest operand stack",
        "values in",
        "stack words in",
        "MAX_CONTROL_FRAMES >=",
        "MAX_STACK_DEPTH >=",
        "spacewasm_std uses 64 and 256",
        "limits from spacewasm 0.7.1",
    ] {
        assert!(
            stdout.contains(fragment),
            "the summary must carry `{fragment}`, got: {stdout}"
        );
    }

    // Every fragment above is prose on one side or the other of an
    // interpolation, so a line that named no function and reported zeroes would
    // carry all nine. The two numbers and the three names are what the line is
    // for, so they are asserted as values.
    let summary = stdout
        .lines()
        .find(|line| line.starts_with("spacewasm: conformant"))
        .expect("the summary line is on stdout");
    assert!(
        !summary.contains("in ``"),
        "every maximum names the function that attains it: {summary}"
    );
    let number_after = |marker: &str| -> u32 {
        let tail = summary
            .split_once(marker)
            .unwrap_or_else(|| panic!("the summary must carry `{marker}`: {summary}"))
            .1;
        tail.trim_start()
            .split(|c: char| !c.is_ascii_digit())
            .next()
            .and_then(|digits| digits.parse().ok())
            .unwrap_or_else(|| panic!("`{marker}` must be followed by a number: {summary}"))
    };
    let depth = number_after("deepest control nesting");
    let values = number_after("tallest operand stack");
    assert!(
        depth >= 1 && values >= 1,
        "a module that defines a function has at least the body frame and at least one \
         live value: {summary}"
    );
    assert_eq!(
        (depth, values),
        (
            number_after("MAX_CONTROL_FRAMES >="),
            number_after("MAX_STACK_DEPTH >=")
        ),
        "the two numbers an embedder is told to build with are the two measured, with no \
         off-by-one to apply: {summary}"
    );

    let temp = assert_fs::TempDir::new().unwrap();
    let entry = write_source(temp.path(), "prog.inf", COMPOUND_COPY_SOURCE);
    let mut cmd = Command::new(assert_cmd::cargo::cargo_bin!("infc"));
    cmd.current_dir(temp.path())
        .arg(&entry)
        .arg("--codegen")
        .arg("--target")
        .arg("spacewasm");
    let assert = cmd.assert().success();
    assert!(
        String::from_utf8_lossy(&assert.get_output().stdout)
            .contains("spacewasm: conformant with WebAssembly 1.0"),
        "a build that writes no file still has a module to describe, and its budget is \
         what a reader of this line came for"
    );
    assert!(
        !temp.child("out").child("prog.wasm").path().exists(),
        "--codegen alone writes nothing"
    );
}

/// A module past the reference embedder's budget warns, names the functions to
/// blame, and still builds.
///
/// Conformance and budget are different verdicts and the build treats them
/// differently: the fixed limits are the decoder's and a module outside them is
/// refused, while `MAX_CONTROL_FRAMES` and `MAX_STACK_DEPTH` belong to whoever
/// embedded the interpreter, so a module above them is a perfectly good module
/// for a bigger embedder. Failing such a build would be this toolchain deciding
/// a deployment's const generics for it, so the warning is the whole mechanism
/// and the exit code has to stay 0.
///
/// The three-name ranking is what makes it actionable — flattening the deepest
/// function is often cheaper than raising a const generic, and which functions
/// they are decides that — so the shallow function must not be named and the
/// order must be worst-first.
///
/// Fails if the warning loop is dropped (nothing else asserts it fires), if a
/// warning starts failing the build, if the ranking stops being worst-first or
/// stops stopping at three, or if the axis that is inside its bound starts
/// warning too.
#[test]
fn a_spacewasm_build_past_the_reference_budget_warns_and_still_builds() {
    /// A function whose body nests `depth` `if` statements.
    fn nested(name: &str, depth: usize) -> String {
        let mut body = String::from("    r = r + 1;\n");
        for level in (0..depth).rev() {
            body = format!("    if r > {level} {{\n{body}    }}\n");
        }
        format!("fn {name}(x: u32) -> u32 {{\n    let mut r: u32 = x;\n{body}    return r;\n}}\n")
    }

    let source = format!(
        "{}{}{}{}",
        nested("deepest", 70),
        nested("middle", 66),
        nested("third", 64),
        "fn shallow(x: u32) -> u32 {\n    return x;\n}\n"
    );

    let temp = assert_fs::TempDir::new().unwrap();
    let entry = write_source(temp.path(), "prog.inf", &source);
    let mut cmd = Command::new(assert_cmd::cargo::cargo_bin!("infc"));
    cmd.current_dir(temp.path())
        .arg(&entry)
        .arg("--target")
        .arg("spacewasm");
    let assert = cmd.assert().success();
    let stdout = String::from_utf8_lossy(&assert.get_output().stdout).into_owned();
    let stderr = String::from_utf8_lossy(&assert.get_output().stderr).into_owned();

    assert!(
        temp.child("out").child("prog.wasm").path().exists(),
        "a module above a deployment's const generic is still a module: {stderr}"
    );

    let warning = stderr
        .lines()
        .find(|line| line.starts_with("warning: spacewasm: control nesting"))
        .unwrap_or_else(|| panic!("the over-budget build owes a warning, got: {stderr}"));
    assert!(
        warning.contains("exceeds spacewasm_std's MAX_CONTROL_FRAMES of 64"),
        "the warning must name the bound that was passed and whose it is: {warning}"
    );

    // The depth is codegen's to decide, so it is read out of the summary rather
    // than written here; what this pins is that the two lines agree and that
    // the warning fired because the measured number is over the reference.
    let summary = stdout
        .lines()
        .find(|line| line.starts_with("spacewasm: conformant"))
        .expect("a successful build still prints its summary");
    let depth = summary
        .split_once("MAX_CONTROL_FRAMES >= ")
        .expect("the summary names the control-frame budget")
        .1
        .split(|c: char| !c.is_ascii_digit())
        .next()
        .and_then(|digits| digits.parse::<u32>().ok())
        .expect("the budget is a number");
    assert!(depth > 64, "the fixture must be over the reference: {summary}");
    assert!(
        warning.contains(&format!("control nesting {depth} exceeds")),
        "the warning and the summary must report one measurement: {warning} / {summary}"
    );

    let ranking = warning
        .split_once("Deepest functions: ")
        .expect("the warning names the functions to blame")
        .1;
    let positions: Vec<usize> = ["`deepest`", "`middle`", "`third`"]
        .iter()
        .map(|name| {
            ranking
                .find(name)
                .unwrap_or_else(|| panic!("the ranking must name {name}: {ranking}"))
        })
        .collect();
    assert!(
        positions[0] < positions[1] && positions[1] < positions[2],
        "the ranking is worst-first: {ranking}"
    );
    assert!(
        !ranking.contains("`shallow`"),
        "the ranking stops at three, so the flattest function must not appear: {ranking}"
    );
    assert!(
        !stderr.contains("MAX_STACK_DEPTH of"),
        "the operand stack is far inside its bound, and one axis being over must not warn \
         about the other: {stderr}"
    );
}

/// A module outside the target's envelope is refused after linking, and the
/// build writes nothing.
///
/// The per-external gate below catches an instruction set; this catches a
/// *size*, which no external can be blamed for and which only the finished
/// module can be measured for. 256 parameter words is the one such shape a
/// source file can reach on its own — the decoder holds a function's parameter
/// size in a single byte — so it is what makes the post-link arm reachable at
/// all.
///
/// The refusal has to arrive before anything is on disk: an artifact a flight
/// computer cannot load is worse company for a build log than no artifact, and
/// a reader who saw one written would reasonably try to fly it.
///
/// Fails if the check moves after the write, if the refusal stops naming the
/// artifact it is about, or if it stops carrying the number and the remedy.
#[test]
fn a_module_outside_the_spacewasm_envelope_is_refused_after_linking() {
    let params = (0..256)
        .map(|index| format!("p{index}: u32"))
        .collect::<Vec<_>>()
        .join(", ");
    let source = format!("fn wide({params}) -> u32 {{\n    return p0;\n}}\n");

    let temp = assert_fs::TempDir::new().unwrap();
    let entry = write_source(temp.path(), "prog.inf", &source);
    let mut cmd = Command::new(assert_cmd::cargo::cargo_bin!("infc"));
    cmd.current_dir(temp.path())
        .arg(&entry)
        .arg("--target")
        .arg("spacewasm");
    let assert = cmd.assert().failure();
    let stderr = String::from_utf8_lossy(&assert.get_output().stderr).into_owned();

    for fragment in [
        "SpaceWasm conformance failed",
        "prog.wasm",
        "No file was written.",
        "function `wide` declares 256 parameter words",
        "at most 255",
        "collect them into a struct",
    ] {
        assert!(
            stderr.contains(fragment),
            "the refusal must carry `{fragment}`, got: {stderr}"
        );
    }
    assert!(
        !temp.child("out").child("prog.wasm").path().exists(),
        "a module the target cannot load must leave nothing behind"
    );
}

/// The same sign-extending external, refused at this target too, by the same
/// per-external gate and in a sentence that no longer talks about contracts.
///
/// The gate was written for one target and is asked through a predicate now, so
/// this is where the generalization is observed: the module is named, the file
/// it resolved to is named, the remedy is the same, and the refusal still
/// arrives *before* the merge — which is the only point at which a message can
/// name a file at all.
///
/// The negative control is the same external at the default target, which links
/// it and writes the artifact, so the advertised remedy is an actual remedy.
///
/// Fails if the gate goes back to naming one target, if the prose reverts to
/// contract wording, or if the refusal moves after the merge.
#[test]
fn a_foreign_module_outside_webassembly_one_is_refused_at_the_spacewasm_target() {
    let (temp, entry, lib) = sign_extending_project();

    let mut cmd = Command::new(assert_cmd::cargo::cargo_bin!("infc"));
    cmd.current_dir(temp.path())
        .arg(&entry)
        .arg("-L")
        .arg(&lib)
        .arg("--target")
        .arg("spacewasm");
    let assert = cmd.assert().failure();
    let stderr = String::from_utf8_lossy(&assert.get_output().stderr).into_owned();
    let stdout = String::from_utf8_lossy(&assert.get_output().stdout).into_owned();

    let resolved = lib.join("rustlib.wasm");
    for fragment in [
        "The `spacewasm` target:",
        "`rustlib`",
        resolved.to_str().expect("temp paths are UTF-8"),
        "not a WebAssembly 1.0 module",
        "sign extension",
        "Rebuild `rustlib`",
        "--target wasm32",
    ] {
        assert!(
            stderr.contains(fragment),
            "the refusal must carry `{fragment}`, got: {stderr}"
        );
    }
    assert!(
        !stderr.contains("contract"),
        "the sentence is shared with a target whose artifact is a contract, and this \
         target's is not: {stderr}"
    );
    assert!(
        !stdout.contains("Linked"),
        "the refusal must come before the merge, or it cannot name a file: {stdout}"
    );
    assert!(
        !temp.child("out").child("prog.wasm").path().exists(),
        "a refused build must leave no artifact"
    );

    let (temp, entry, lib) = sign_extending_project();
    let mut cmd = Command::new(assert_cmd::cargo::cargo_bin!("infc"));
    cmd.current_dir(temp.path()).arg(&entry).arg("-L").arg(&lib);
    let assert = cmd.assert().success();
    let stdout = String::from_utf8_lossy(&assert.get_output().stdout).into_owned();
    assert!(
        stdout.contains("Linked 1 external module(s)"),
        "the default target must link the very same module, or the refusal above is about \
         a broken fixture rather than about the instruction set"
    );
}

/// Compiles `source` with `args` and returns the Rocq translation written
/// beside the module.
///
/// A second output path with its own helper because the `.wasm` helper reads a
/// file `-v` builds do write and a `.v` they do not: asserting on the module
/// would compare the artifact both flags produce rather than the one the flag
/// under test asked for.
fn translate_source_with(args: &[&str], source: &str) -> String {
    let temp = assert_fs::TempDir::new().unwrap();
    let dest = temp.child("prog.inf");
    std::fs::write(dest.path(), source).unwrap();

    let mut cmd = Command::new(assert_cmd::cargo::cargo_bin!("infc"));
    cmd.current_dir(temp.path()).arg(dest.path()).args(args);
    cmd.assert().success();

    std::fs::read_to_string(temp.child("out").child("prog.v").path())
        .expect("infc must have written out/prog.v")
}

/// The target builds through the CLI and writes a module — and writes the
/// module a `wasm32` build writes, which is the whole of what selecting it does
/// to the artifact today.
///
/// The corpus-wide identity check lives in the `inference-tests` crate and
/// drives `codegen()`; this one drives the CLI, where the linking and the write
/// path are, and those are where a target *could* leak into the bytes.
///
/// Fails if `--target spacewasm` stops resolving, if the name reaches a write
/// path that branches on it, or if the flag reaches an emitter.
#[test]
fn the_spacewasm_target_builds_the_module_the_default_target_builds() {
    let spacewasm = compile_source_with(&["--target", "spacewasm"], COMPOUND_COPY_SOURCE);
    assert!(
        spacewasm.starts_with(b"\0asm"),
        "two empty or stub files compare equal, so the identity below is worth \
         asserting only over a real WebAssembly module"
    );
    assert_eq!(
        compile_source_with(&[], COMPOUND_COPY_SOURCE),
        spacewasm,
        "a SpaceWasm build must be byte-identical to the default build of the \
         same source"
    );
}

/// Proof mode is refused at this target, in both spellings, by code generation:
/// the interpreter's decoder does not define the custom `0xfc` instructions
/// proof mode emits.
///
/// Both spellings are run because `-v` alone normalizes to proof mode. That
/// normalization is what makes the `--mode compile -v` test below a statement
/// about compile mode rather than an accident, so the two tests have to be read
/// together.
///
/// Fails if `Target::SpaceWasm` starts supporting proof mode, or if either
/// spelling stops reaching the gate.
#[test]
fn proof_mode_is_refused_at_the_spacewasm_target_in_both_spellings() {
    for spelling in [&["--mode", "proof"][..], &["-v"][..]] {
        let temp = assert_fs::TempDir::new().unwrap();
        let dest = temp.child("prog.inf");
        std::fs::write(dest.path(), COMPOUND_COPY_SOURCE).unwrap();

        let mut cmd = Command::new(assert_cmd::cargo::cargo_bin!("infc"));
        cmd.current_dir(temp.path())
            .arg(dest.path())
            .arg("--target")
            .arg("spacewasm")
            .args(spelling);

        let assert = cmd.assert().failure();
        let stderr = String::from_utf8_lossy(&assert.get_output().stderr).into_owned();
        assert!(
            stderr.contains("Proof mode requires the `wasm32` target")
                && stderr.contains("the `spacewasm` runtime rejects a module carrying them"),
            "{spelling:?}: the refusal must name the mode and the target, got: {stderr}"
        );
        assert!(
            !temp.child("out").child("prog.wasm").path().exists()
                && !temp.child("out").child("prog.v").path().exists(),
            "{spelling:?}: the refusal must leave neither artifact"
        );
    }
}

/// A post-MVP instruction family is refused at this target, naming the entry to
/// drop rather than the proposal's opcodes: the interpreter has not implemented
/// bulk memory, so a module using it is refused here rather than on the vehicle.
///
/// Fails if `Target::SpaceWasm` starts permitting bulk memory, or if the
/// refusal stops naming the target in the spelling `--target` accepts.
#[test]
fn a_post_mvp_feature_is_refused_at_the_spacewasm_target() {
    let temp = assert_fs::TempDir::new().unwrap();
    let dest = temp.child("prog.inf");
    std::fs::write(dest.path(), COMPOUND_COPY_SOURCE).unwrap();

    let mut cmd = Command::new(assert_cmd::cargo::cargo_bin!("infc"));
    cmd.current_dir(temp.path())
        .arg(dest.path())
        .arg("--target")
        .arg("spacewasm")
        .arg("--wasm-features")
        .arg("bulk-memory");

    let assert = cmd.assert().failure();
    let stderr = String::from_utf8_lossy(&assert.get_output().stderr).into_owned();
    assert!(
        stderr.contains("The `spacewasm` target does not support the 'bulk-memory' \
                         WebAssembly feature"),
        "the refusal must name the target and the feature, got: {stderr}"
    );
    assert!(
        !temp.child("out").child("prog.wasm").path().exists(),
        "the refusal must leave no artifact"
    );
}

/// `--mode compile -v` **succeeds** at this target and writes the Rocq
/// translation a `wasm32` build of the same source writes, character for
/// character.
///
/// This is the one place this target's story is strictly better than the
/// Stellar one, and it is worth pinning precisely because nothing enforces it:
/// `proof_artifact_refusal` names one target, and SpaceWasm reaches the
/// translation by falling through that check. Falling through is the correct
/// outcome — the translation reads the bytes that ship, because there is no
/// rewrite for it to read them before — but "correct by falling through" is a
/// property a later edit can remove without noticing, and a widened refusal
/// would look like tidying.
///
/// Fails the moment that check is generalized from one target to "any
/// non-default target", and fails if a target ever reaches the emitter.
#[test]
fn a_compile_mode_translation_at_the_spacewasm_target_matches_the_default_target() {
    let spacewasm = translate_source_with(
        &["--target", "spacewasm", "--mode", "compile", "-v"],
        COMPOUND_COPY_SOURCE,
    );
    let wasm32 = translate_source_with(
        &["--target", "wasm32", "--mode", "compile", "-v"],
        COMPOUND_COPY_SOURCE,
    );
    assert!(
        spacewasm.contains("Definition"),
        "the build must write a Rocq module, not an empty or stub file: {spacewasm}"
    );
    assert_eq!(
        spacewasm, wasm32,
        "the two targets translate to the same Rocq module, because they are \
         the same WebAssembly module"
    );
}

// Memory layout flags ---

/// A program that allocates an array frame, so both places the layout is read
/// are emitted: the memory section exists only for a module that needs memory,
/// and `__stack_pointer` only accompanies it.
const FRAME_ALLOCATING_SOURCE: &str = "\
pub fn read_first() -> i32 {
    let arr: [i32; 4] = [1, 2, 3, 4];
    return arr[0];
}
";

/// Renders a module as WAT, for assertions about a section's shape rather than
/// its bytes.
fn wat_of(wasm: &[u8]) -> String {
    wasmprinter::print_bytes(wasm).expect("layout fixtures are printable WebAssembly 1.0")
}

/// A layout requested on the command line must survive the whole pipeline and
/// reach the emitted module, in both of the numbers it carries.
///
/// The library API is covered by `a_configured_layout_reaches_the_emitted_module`
/// in the `inference-tests` crate. This is the flag half, and it is a separate
/// question: everything between `argv` and `CodegenOptions` — the clap fields,
/// the resolver, and the one assignment that puts the resolved layout on the
/// options — is exercised only from here. A `layout:` field left hard-coded to
/// the default would pass every library test.
///
/// The page count and the stack size are asserted together because they are read
/// independently — the memory section takes one, the stack-pointer global takes
/// the other — so pinning a single number would leave the other free to be
/// ignored. A layout whose stack is half its memory is what separates them: under
/// the default the two are numerically equal, and an emitter that confused one
/// for the other would still look correct.
///
/// The default-layout half is the control: the same program compiled twice
/// differs in exactly these two numbers, and differs in them only because the
/// flags asked it to.
#[test]
fn a_layout_requested_on_the_command_line_reaches_the_emitted_module() {
    let configured = wat_of(&compile_source_with(
        &["--memory-pages", "2", "--stack-size", "32768"],
        FRAME_ALLOCATING_SOURCE,
    ));
    assert!(
        configured.contains("(memory (;0;) 2 2)"),
        "the memory section must declare the requested 2 fixed pages:\n{configured}"
    );
    assert!(
        configured.contains("(global (;0;) (mut i32) i32.const 32768)"),
        "the stack pointer must start at the requested stack size:\n{configured}"
    );

    let default = wat_of(&compile_source_with(&[], FRAME_ALLOCATING_SOURCE));
    assert!(
        default.contains("(memory (;0;) 1 1)"),
        "the same source with no flags must declare one page:\n{default}"
    );
    assert!(
        default.contains("(global (;0;) (mut i32) i32.const 65536)"),
        "the same source with no flags must start the stack pointer at one page:\n{default}"
    );
}

/// Either flag alone reaches the module, with the other number left at its
/// default. Partial specification is the common case, and a resolver that
/// required both would be indistinguishable from one that ignored the missing
/// key if only the both-flags case were tested.
#[test]
fn either_memory_flag_alone_reaches_the_emitted_module() {
    let pages_only = wat_of(&compile_source_with(
        &["--memory-pages", "3"],
        FRAME_ALLOCATING_SOURCE,
    ));
    assert!(
        pages_only.contains("(memory (;0;) 3 3)"),
        "--memory-pages alone must size the memory:\n{pages_only}"
    );
    assert!(
        pages_only.contains("(global (;0;) (mut i32) i32.const 65536)"),
        "--memory-pages alone must leave the stack at its default:\n{pages_only}"
    );

    let stack_only = wat_of(&compile_source_with(
        &["--stack-size", "16384"],
        FRAME_ALLOCATING_SOURCE,
    ));
    assert!(
        stack_only.contains("(memory (;0;) 1 1)"),
        "--stack-size alone must leave the memory at its default:\n{stack_only}"
    );
    assert!(
        stack_only.contains("(global (;0;) (mut i32) i32.const 16384)"),
        "--stack-size alone must size the stack:\n{stack_only}"
    );
}

/// An unusable layout fails the build before any phase runs, rather than being
/// clamped or ignored — and the diagnostic names the flag spelling.
#[test]
fn an_unusable_layout_is_rejected_before_any_output() {
    let temp = assert_fs::TempDir::new().unwrap();
    let src = example_file("trivial.inf");
    let dest = temp.child("trivial.inf");
    std::fs::copy(&src, dest.path()).unwrap();

    let mut cmd = Command::new(assert_cmd::cargo::cargo_bin!("infc"));
    cmd.current_dir(temp.path())
        .arg(dest.path())
        .arg("--stack-size")
        .arg("131072");

    cmd.assert()
        .failure()
        .stderr(predicate::str::contains("`--stack-size`"))
        .stderr(predicate::str::contains(
            "does not fit in the linear memory",
        ));

    assert!(
        !temp.child("out").child("trivial.wasm").path().exists(),
        "a rejected layout must leave no artifact"
    );
}

/// `n` zero elements, as an Inference array literal.
fn zeros(n: usize) -> String {
    let mut literal = String::from("[");
    for i in 0..n {
        if i > 0 {
            literal.push_str(", ");
        }
        literal.push('0');
    }
    literal.push(']');
    literal
}

/// A three-deep call chain of ~4 KB frames: roughly 12 KB cumulative, which fits
/// the default 64 KB stack and does not fit an 8 KB one.
///
/// No single frame exceeds 8 KB, which is deliberate. Frame layout asserts on a
/// single frame outgrowing the stack, and that assert is a panic; sizing every
/// frame under the smaller stack keeps this test about A036's *cumulative* budget
/// rather than about which of two failure modes fires first.
fn stack_chain_source() -> String {
    let elements = zeros(1024);
    format!(
        "\
fn level_two() -> i32 {{
    let arr: [i32; 1024] = {elements};
    return arr[0];
}}

fn level_one() -> i32 {{
    let arr: [i32; 1024] = {elements};
    return arr[0] + level_two();
}}

pub fn main() -> i32 {{
    let arr: [i32; 1024] = {elements};
    return arr[0] + level_one();
}}
"
    )
}

/// A036 measures call chains against the stack this build emits, not against a
/// fixed default.
///
/// This is the test that says the analysis phase received the configured layout.
/// Wiring only code generation would leave the compiler emitting an 8 KB stack
/// while the rule cleared a 12 KB chain against 64 KB — it would accept, and ship,
/// a program that overflows its own stack. The default-budget half is the control:
/// the same source is fine, so the rejection is attributable to the flag.
///
/// The rejection is asserted as a *diagnostic*: the A036 message on stderr,
/// naming the smaller budget. A bare `.failure()` would also be satisfied by a
/// panic, which is the shape this failure takes if analysis and code generation
/// ever disagree about the stack — so the absence of a panic is asserted too.
#[test]
fn a036_measures_against_the_requested_stack_size() {
    let source = stack_chain_source();

    let temp = assert_fs::TempDir::new().unwrap();
    let dest = temp.child("prog.inf");
    std::fs::write(dest.path(), &source).unwrap();

    let mut cmd = Command::new(assert_cmd::cargo::cargo_bin!("infc"));
    cmd.current_dir(temp.path()).arg(dest.path());
    cmd.assert()
        .success()
        .stdout(predicate::str::contains("WASM generated"));
    assert!(
        temp.child("out").child("prog.wasm").path().exists(),
        "the chain fits the default stack, so the default build must produce an artifact"
    );

    let temp = assert_fs::TempDir::new().unwrap();
    let dest = temp.child("prog.inf");
    std::fs::write(dest.path(), &source).unwrap();

    let mut cmd = Command::new(assert_cmd::cargo::cargo_bin!("infc"));
    cmd.current_dir(temp.path())
        .arg(dest.path())
        .arg("--stack-size")
        .arg("8192");
    let assert = cmd.assert().failure();
    let stderr = String::from_utf8_lossy(&assert.get_output().stderr).into_owned();

    assert!(
        stderr.contains("maximum stack depth"),
        "the smaller stack must be reported by A036, got:\n{stderr}"
    );
    assert!(
        stderr.contains("8192-byte stack"),
        "the diagnostic must name the requested budget, not the default, got:\n{stderr}"
    );
    assert!(
        !stderr.contains("panicked"),
        "the smaller stack must produce a diagnostic, not a panic, got:\n{stderr}"
    );
    assert!(
        !temp.child("out").child("prog.wasm").path().exists(),
        "a rejected build must leave no artifact"
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

// Spec-name validation. Every spec below is scaffolding — these tests are about
// the NAME, the artifact writes, and the stale-artifact clearing, never about
// what the spec proves. The bodies are nonetheless written to state a real
// property (`assert(x == x)`) because a spec function that only computes has a
// vacuous obligation and is itself a hard codegen error in proof mode. A
// computing body would therefore reach the assertions below only when some other
// rejection happens to fire first, silently pinning these tests to diagnostic
// ordering instead of to the behavior they name.

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
        "spec _S { fn obligation(x: i32) { assert(x == x); } }",
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
        "spec S { fn obligation(x: i32) { assert(x == x); } }",
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
        "spec S { fn obligation(x: i32) { assert(x == x); } }",
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
        "spec Invariant_ { fn obligation(x: i32) { assert(x == x); } }",
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
            &format!("spec {spec_name} {{ fn obligation(x: i32) {{ assert(x == x); }} }}"),
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
        "spec Invariant { fn obligation(x: i32) { assert(x == x); } }",
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
        "spec S { fn obligation(x: i32) { assert(x == x); } }",
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
        "spec Spec_ { fn obligation(x: i32) { assert(x == x); } }\npub fn main() -> i32 { return 0; }",
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
        "spec Foo { fn obligation(x: i32) { assert(x == x); } }\npub fn main() -> i32 { return 0; }",
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
        "spec Spec_ { fn obligation(x: i32) { assert(x == x); } }\npub fn main() -> i32 { return 0; }",
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

/// An entry file whose stem is one of the helper definitions the emitted Rocq
/// preamble always occupies is rejected in proof mode, naming the file and the
/// rename. Emission never noticed the clash before: the `.v` was written with
/// exit 0, and the failure surfaced only when `coqc` reported `Me already
/// exists` and elaborated nothing in the file. The module name is the one
/// contestant with nowhere to move to — it is the `.v`'s identity and the
/// subject of its validity theorem — so the fix is a file rename.
#[test]
fn module_named_as_a_rocq_preamble_helper_rejected_no_stale_artifact() {
    let temp = assert_fs::TempDir::new().unwrap();
    let entry = write_source(temp.path(), "Me.inf", "pub fn main() -> i32 { return 0; }");

    let mut cmd = Command::new(assert_cmd::cargo::cargo_bin!("infc"));
    cmd.current_dir(temp.path()).arg(&entry).arg("-v");

    cmd.assert()
        .failure()
        .stderr(predicate::str::contains(
            "the output module name 'Me' is one of the helper definitions",
        ))
        .stderr(predicate::str::contains("'Me.inf' -> 'Me_module.inf'"))
        .stderr(predicate::str::contains("appear verbatim in your .v"));

    assert!(
        !temp.child("out").child("Me.wasm").path().exists(),
        "a rejected entry stem must not leave a stale out/Me.wasm behind"
    );
    assert!(
        !temp.child("out").child("Me.v").path().exists(),
        "a rejected entry stem must not leave a stale out/Me.v behind"
    );
}

/// The rejection is scoped to proof mode: default `compile` mode emits no Rocq
/// name at all, so the same `Me.inf` compiles cleanly to `.wasm` with no `.v`.
#[test]
fn module_named_as_a_rocq_preamble_helper_compiles_in_default_mode() {
    let temp = assert_fs::TempDir::new().unwrap();
    let entry = write_source(temp.path(), "Me.inf", "pub fn main() -> i32 { return 0; }");

    let mut cmd = Command::new(assert_cmd::cargo::cargo_bin!("infc"));
    cmd.current_dir(temp.path()).arg(&entry);

    cmd.assert()
        .success()
        .stdout(predicate::str::contains("WASM generated"));

    assert!(temp.child("out").child("Me.wasm").path().exists());
    assert!(
        !temp.child("out").child("Me.v").path().exists(),
        "default mode must not emit a .v"
    );
}

/// Asserts that the emitted Rocq file gives no top-level name to two
/// constructs. That is exactly what `coqc` refuses — `<name> already exists`,
/// reported for the whole file, so nothing in it elaborates, including the
/// definitions that were fine.
///
/// Asserted as the property rather than as the disambiguated spelling: which
/// suffix the translator picks is a cosmetic choice, and pinning it here would
/// turn a change to it into a false regression.
fn assert_v_has_no_duplicate_top_level_names(v_path: &std::path::Path) {
    let v = std::fs::read_to_string(v_path).unwrap();
    let mut seen = std::collections::HashSet::new();
    for line in v.lines() {
        let Some(rest) = line
            .strip_prefix("Definition ")
            .or_else(|| line.strip_prefix("Theorem "))
        else {
            continue;
        };
        let name = rest
            .split(|c: char| !(c.is_ascii_alphanumeric() || c == '_'))
            .next()
            .unwrap_or_default();
        assert!(
            seen.insert(name.to_string()),
            "`{name}` names two top-level definitions in {}, which coqc refuses:\n{v}",
            v_path.display(),
        );
    }
}

/// `main.inf` whose entry function is `main` — the standard shape in every
/// language — must build a proof. The emitted `.v` gives `main` to the module
/// record, so the function's own definition is disambiguated off it rather than
/// the program being rejected: `fn main` is the language entry point and is
/// special-cased by codegen's export rule, so renaming it is not a fix
/// available to the user.
#[test]
fn entry_function_named_as_the_module_record_produces_a_duplicate_free_v() {
    let temp = assert_fs::TempDir::new().unwrap();
    let entry = write_source(
        temp.path(),
        "main.inf",
        "pub fn main() -> i32 { return 0; }",
    );

    let mut cmd = Command::new(assert_cmd::cargo::cargo_bin!("infc"));
    cmd.current_dir(temp.path()).arg(&entry).arg("-v");

    cmd.assert()
        .success()
        .stdout(predicate::str::contains("WASM generated"))
        .stdout(predicate::str::contains("V generated"));

    assert!(temp.child("out").child("main.wasm").path().exists());
    let v = temp.child("out").child("main.v");
    assert!(v.path().exists(), "expected out/main.v");
    assert_v_has_no_duplicate_top_level_names(v.path());
}

/// The same disambiguation covers a function named after one of the helper
/// definitions the emitted preamble always occupies.
#[test]
fn function_named_as_a_rocq_preamble_helper_produces_a_duplicate_free_v() {
    let temp = assert_fs::TempDir::new().unwrap();
    let entry = write_source(
        temp.path(),
        "prog.inf",
        "fn Me(x: i32) -> i32 { return x; }\npub fn main() -> i32 { return Me(1); }",
    );

    let mut cmd = Command::new(assert_cmd::cargo::cargo_bin!("infc"));
    cmd.current_dir(temp.path()).arg(&entry).arg("-v");

    cmd.assert().success();

    let v = temp.child("out").child("prog.v");
    assert!(v.path().exists(), "expected out/prog.v");
    assert_v_has_no_duplicate_top_level_names(v.path());
}

/// And a function named after the module's validity theorem, the third
/// top-level name the module spends before it names any function.
#[test]
fn function_named_as_the_module_theorem_produces_a_duplicate_free_v() {
    let temp = assert_fs::TempDir::new().unwrap();
    let entry = write_source(
        temp.path(),
        "prog.inf",
        "fn valid_prog(x: i32) -> i32 { return x; }\npub fn main() -> i32 { return valid_prog(1); }",
    );

    let mut cmd = Command::new(assert_cmd::cargo::cargo_bin!("infc"));
    cmd.current_dir(temp.path()).arg(&entry).arg("-v");

    cmd.assert().success();

    let v = temp.child("out").child("prog.v");
    assert!(v.path().exists(), "expected out/prog.v");
    assert_v_has_no_duplicate_top_level_names(v.path());
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
        "spec Good { fn obligation(x: i32) { assert(x == x); } }\npub fn main() -> i32 { return 7; }",
    );

    // A prior default build writes out/main.wasm.
    build_ok_and_assert_wasm(&temp, &entry, "main");

    // Introduce a proof-mode rejection and rebuild with --codegen -v (writes .v,
    // not .wasm — yet the prior .wasm must be cleared).
    write_source(
        temp.path(),
        "main.inf",
        "spec Bad_ { fn obligation(x: i32) { assert(x == x); } }\npub fn main() -> i32 { return 7; }",
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
        "spec Good { fn obligation(x: i32) { assert(x == x); } }\npub fn main() -> i32 { return 7; }",
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
        "spec list { fn ob(x: i32) { assert(x == x); } }\npub fn main() -> i32 { return 7; }",
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
        "spec match { fn ob(x: i32) { assert(x == x); } }\npub fn main() -> i32 { return 7; }",
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
        "spec list { fn ob(x: i32) { assert(x == x); } }\npub fn main() -> i32 { return 7; }",
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

// Deeply nested and deeply chained input (issue #322).
//
// The compiler's phases recurse once per level of the input's syntactic nesting,
// and the platform's default main-thread stack is smaller than they need, so
// these programs used to end the process with `fatal runtime error: stack
// overflow`. That is a signal kill, not an exit status, so the assertion that
// separates the fixed behaviour from both the old abort and any diagnostic is
// `.success()` — a mere "not exit 1" would pass on an abort.
//
// The binary is the level that matters here: `fn main` is what reserves the
// stack, so only a real process proves the reservation is in place for a user's
// build rather than only for a test that opts into it.

/// `pub fn f(a: i64) -> i64 { return a + a + … + a; }` with `n` operands.
fn operand_chain_source(n: usize) -> String {
    let chain = std::iter::repeat_n("a", n).collect::<Vec<_>>().join(" + ");
    format!("pub fn f(a: i64) -> i64 {{ return {chain}; }}")
}

/// An `if` / `else if` chain of `k` arms closed by a final `else`.
fn else_if_chain_source(k: usize) -> String {
    let arms: String = (0..k)
        .map(|i| format!("if a == {i} {{ return {i}; }} else "))
        .collect();
    format!("pub fn f(a: i64) -> i64 {{ {arms}{{ return 0; }} }}")
}

/// The input reported in issue #322 — a 350-operand operator chain — compiles
/// through the binary. On the platform default stack the type checker aborted
/// here, while 300 operands survived.
#[test]
fn deep_operand_chain_compiles_through_the_binary() {
    let temp = assert_fs::TempDir::new().unwrap();
    let entry = write_source(temp.path(), "chain.inf", &operand_chain_source(350));

    let mut cmd = Command::new(assert_cmd::cargo::cargo_bin!("infc"));
    cmd.current_dir(temp.path()).arg(&entry);

    cmd.assert()
        .success()
        .stdout(predicate::str::contains("WASM generated"));

    assert!(temp.child("out").child("chain.wasm").path().exists());
}

/// A 900-arm `else if` chain compiles through the binary. This was the lowest
/// known abort threshold: 800 arms survived, 900 did not.
#[test]
fn deep_else_if_chain_compiles_through_the_binary() {
    let temp = assert_fs::TempDir::new().unwrap();
    let entry = write_source(temp.path(), "arms.inf", &else_if_chain_source(900));

    let mut cmd = Command::new(assert_cmd::cargo::cargo_bin!("infc"));
    cmd.current_dir(temp.path()).arg(&entry);

    cmd.assert()
        .success()
        .stdout(predicate::str::contains("WASM generated"));

    assert!(temp.child("out").child("arms.wasm").path().exists());
}

/// The acceptance bar for this work, driven end to end through the binary rather
/// than stopping at the type checker: 2,000 operands and 2,000 `else if` arms
/// compile. Both are an order of magnitude past the depths that used to abort, and
/// running them here is what makes the claim hold on every CI platform rather than
/// only on the machine the numbers were measured on.
#[test]
fn deep_input_at_the_acceptance_bar_compiles_through_the_binary() {
    let temp = assert_fs::TempDir::new().unwrap();

    let chain = write_source(temp.path(), "chain2k.inf", &operand_chain_source(2_000));
    let mut cmd = Command::new(assert_cmd::cargo::cargo_bin!("infc"));
    cmd.current_dir(temp.path()).arg(&chain);
    cmd.assert()
        .success()
        .stdout(predicate::str::contains("WASM generated"));
    assert!(temp.child("out").child("chain2k.wasm").path().exists());

    let arms = write_source(temp.path(), "arms2k.inf", &else_if_chain_source(2_000));
    let mut cmd = Command::new(assert_cmd::cargo::cargo_bin!("infc"));
    cmd.current_dir(temp.path()).arg(&arms);
    cmd.assert()
        .success()
        .stdout(predicate::str::contains("WASM generated"));
    assert!(temp.child("out").child("arms2k.wasm").path().exists());
}

// Exit statuses on the argument-handling paths.
//
// These are the paths most perturbed by running the driver on a worker thread:
// argument parsing now happens off the main thread, so clap's own `process::exit`
// and the missing-argument path both terminate the process from a thread that is
// not `main`. They are pinned by exact code rather than by `.failure()`, because
// `.failure()` cannot tell 1 from 2 — nor either of them from the signal kill a
// stack overflow produces.

/// No arguments: the driver reports the missing source file and exits 1.
#[test]
fn no_arguments_exits_one() {
    Command::new(assert_cmd::cargo::cargo_bin!("infc"))
        .assert()
        .code(1)
        .stderr(predicate::str::contains("source file argument required"));
}

/// An unknown flag is rejected by clap, which exits 2 — a distinct status from the
/// driver's own exit 1, and one that survives the move onto the worker thread.
#[test]
fn unknown_flag_exits_two() {
    Command::new(assert_cmd::cargo::cargo_bin!("infc"))
        .arg("--no-such-flag")
        .assert()
        .code(2);
}

/// A path that does not exist: exit 1, before any phase runs.
#[test]
fn missing_source_file_exits_one() {
    Command::new(assert_cmd::cargo::cargo_bin!("infc"))
        .arg("definitely-not-here.inf")
        .assert()
        .code(1)
        .stderr(predicate::str::contains("path not found"));
}

// Linker warnings on a build that merges an external.
//
// The merge admits an external at Tier B by proving every memory address it
// forms derives from a parameter of the call; it cannot prove the address stays
// inside the buffer that parameter points into. A single fixed page hid that gap
// — a reach past the caller's buffer left the memory and trapped — and a larger
// memory removes the backstop, so the merge says so.
//
// The linker's own suite pins when the warning is *raised*. What only a real
// process can show is that it is *delivered*: that the driver prints it, on
// stderr, without turning an advisory into a failed build. Deleting the print
// loop leaves every library assertion satisfied.

/// A `.wasm` whose one export reads linear memory at an address taken straight
/// from its parameter — the shape the merge admits at Tier B.
const TIER_B_LOAD_LIB: &str = r#"
(module
  (type (;0;) (func (param i32) (result i32)))
  (memory (;0;) 1)
  (func (;0;) (type 0) (param i32) (result i32)
    local.get 0
    i32.load)
  (export "load_at" (func 0)))
"#;

/// A program binding that export. The array is what makes the main module
/// declare a memory at all: with none, the merge would adopt the external's one
/// page and `--memory-pages` would have nothing to enlarge.
const CALLS_TIER_B_EXTERNAL: &str = "\
external fn load_at(p: i32) -> i32;
use { load_at } from memlib;

pub fn main() -> i32 {
    let scratch: [i32; 4] = [0, 0, 0, 0];
    return load_at(scratch[0]);
}
";

/// Stages the program and its external side by side, returning the temp
/// directory, the entry path, and the `--wasm-dep` value binding `memlib` to the
/// assembled `.wasm`.
fn tier_b_project() -> (assert_fs::TempDir, std::path::PathBuf, String) {
    let temp = assert_fs::TempDir::new().unwrap();
    let entry = write_source(temp.path(), "main.inf", CALLS_TIER_B_EXTERNAL);
    let lib = temp.child("memlib.wasm");
    let bytes = wat::parse_str(TIER_B_LOAD_LIB).expect("the external fixture is valid WAT");
    std::fs::write(lib.path(), bytes).unwrap();
    let dep = format!(
        "memlib={}",
        lib.path().to_str().expect("temp paths are UTF-8")
    );
    (temp, entry, dep)
}

/// A build whose merged external addresses memory through its parameter, into a
/// memory of more than one page, reports the gap on stderr and still succeeds.
///
/// A warning is not an error, so the exit status and the artifact are asserted
/// alongside the text: a driver that printed the message and then aborted would
/// satisfy a stderr-only assertion while breaking every such build.
///
/// `Linked 1 external module(s)` is asserted because it is what makes the rest
/// attributable. Without it, a fixture whose external silently failed to bind
/// would produce no warning for a reason that has nothing to do with the page
/// count, and the negative control below would pass for that reason too.
///
/// The text is matched on the claim, not the sentence: the two halves of the
/// distinction the warning exists to draw, the name the user knows the function
/// by, and the page count that made it worth saying.
#[test]
fn a_tier_b_external_merged_into_a_multi_page_memory_warns_on_stderr() {
    let (temp, entry, dep) = tier_b_project();

    let mut cmd = Command::new(assert_cmd::cargo::cargo_bin!("infc"));
    cmd.current_dir(temp.path())
        .arg(&entry)
        .arg("--memory-pages")
        .arg("2")
        .arg("--wasm-dep")
        .arg(&dep);

    let assert = cmd
        .assert()
        .success()
        .stdout(predicate::str::contains("Linked 1 external module(s)"))
        .stdout(predicate::str::contains("WASM generated"));

    let stderr = String::from_utf8_lossy(&assert.get_output().stderr).into_owned();
    assert!(
        stderr.contains("load_at"),
        "the warning must name the external the user bound, got:\n{stderr}"
    );
    assert!(
        stderr.contains("derives from a parameter"),
        "the warning must state what the merge does prove, got:\n{stderr}"
    );
    assert!(
        stderr.contains("stays within the buffer"),
        "the warning must state what the merge does not prove, got:\n{stderr}"
    );
    assert!(
        stderr.contains("2 pages"),
        "the warning must name the memory that removed the backstop, got:\n{stderr}"
    );

    assert!(
        temp.child("out").child("main.wasm").path().exists(),
        "a warning must not withhold the artifact"
    );
}

/// The same program and the same external, built without `--memory-pages`, merge
/// silently.
///
/// This is what makes the warning a condition rather than a constant: a driver
/// that printed the message unconditionally would pass the test above unchanged.
/// The emitted page count is asserted so the silence is attributable to the one
/// page and not to the external having dropped out of the build.
#[test]
fn the_same_external_in_a_single_page_build_merges_without_warning() {
    let (temp, entry, dep) = tier_b_project();

    let mut cmd = Command::new(assert_cmd::cargo::cargo_bin!("infc"));
    cmd.current_dir(temp.path())
        .arg(&entry)
        .arg("--wasm-dep")
        .arg(&dep);

    let assert = cmd
        .assert()
        .success()
        .stdout(predicate::str::contains("Linked 1 external module(s)"))
        .stdout(predicate::str::contains("WASM generated"));

    let stderr = String::from_utf8_lossy(&assert.get_output().stderr).into_owned();
    assert!(
        !stderr.contains("warning:"),
        "a single-page build must merge the same external silently, got:\n{stderr}"
    );
    assert!(
        !stderr.contains("derives from a parameter"),
        "the Tier-B claim must not be reported at one page, got:\n{stderr}"
    );

    let artifact = temp.child("out").child("main.wasm");
    let wat = wat_of(&std::fs::read(artifact.path()).unwrap());
    assert!(
        wat.contains("(memory (;0;) 1 1)"),
        "the silence must come from a one-page reconciled memory:\n{wat}"
    );
}

// Adoption of a linked library's own proof obligations.
//
// The linker's own suite pins when an obligation is carried, dropped, or
// refused. What only a real process can show is which policy the driver picks
// for a given command line, that the refusal happens before anything is
// written, and that the report and the adopted theorem reach the user through
// the two channels they actually read: stderr and the emitted `.v`.

/// A program with no external bindings, so a rejection of the flag is
/// attributable to the flag alone and to nothing about linking.
const NO_EXTERNALS_SOURCE: &str = "\
pub fn main() -> i32 {
    return 7;
}
";

/// Stages `spec_adopted_extern.inf` against a proof-mode build of the library
/// it binds, returning the temp directory, the entry path, and the
/// `--wasm-dep` value binding `mathlib` to the compiled library.
///
/// The library is compiled in proof mode because that is the only build that
/// carries the verification sections at issue: a compile-mode library ships
/// neither, so there would be nothing to report and nothing to adopt.
fn adopting_project() -> (assert_fs::TempDir, std::path::PathBuf, String) {
    let temp = assert_fs::TempDir::new().unwrap();
    Command::new(assert_cmd::cargo::cargo_bin!("infc"))
        .current_dir(temp.path())
        .arg(example_file("spec_adopted_extern_mathlib.inf"))
        .arg("--mode")
        .arg("proof")
        .assert()
        .success();
    let lib = temp.child("out").child("spec_adopted_extern_mathlib.wasm");
    assert!(
        lib.path().exists(),
        "the library fixture must build before anything links it"
    );
    let dep = format!(
        "mathlib={}",
        lib.path().to_str().expect("temp paths are UTF-8")
    );
    (temp, example_file("spec_adopted_extern.inf"), dep)
}

/// The `.v` the entry point writes under a build staged by [`adopting_project`].
fn adopted_v_text(temp: &assert_fs::TempDir) -> String {
    let v = temp.child("out").child("spec_adopted_extern.v");
    assert!(v.path().exists(), "a -v build must write the .v");
    std::fs::read_to_string(v.path()).expect("the emitted .v is UTF-8")
}

/// `--adopt-external-specs` on a build that resolves to compile mode is refused
/// before any phase runs, in all three ways such a build can be spelled.
///
/// The absence of `out/` is asserted alongside the exit code because the two
/// halves fail independently: a check placed after the artifact write would
/// still exit 1 while leaving a `.wasm` on disk that no verification section
/// describes.
#[test]
fn adopt_external_specs_requires_proof_mode() {
    for extra in [
        vec!["--mode", "compile"],
        vec![],
        vec!["--parse"],
    ] {
        let temp = assert_fs::TempDir::new().unwrap();
        let entry = write_source(temp.path(), "main.inf", NO_EXTERNALS_SOURCE);

        let mut cmd = Command::new(assert_cmd::cargo::cargo_bin!("infc"));
        cmd.current_dir(temp.path())
            .arg(&entry)
            .arg("--adopt-external-specs");
        for arg in &extra {
            cmd.arg(arg);
        }

        let assert = cmd.assert().code(1);
        let stderr = String::from_utf8_lossy(&assert.get_output().stderr).into_owned();
        assert!(
            stderr.contains("--adopt-external-specs requires proof mode"),
            "the refusal must name the flag and the requirement ({extra:?}), got:\n{stderr}"
        );
        assert!(
            stderr.contains("this build resolves to compile mode"),
            "the refusal must say what this build resolved to ({extra:?}), got:\n{stderr}"
        );
        assert!(
            stderr.contains("Pass -v (or --mode proof)"),
            "the refusal must carry its repair ({extra:?}), got:\n{stderr}"
        );
        assert!(
            !temp.child("out").path().exists(),
            "the refusal must precede every phase, so no artifact directory exists ({extra:?})"
        );
    }
}

/// The same flag on a build that resolves to proof mode is accepted, whether
/// the mode was reached through `-v` or named outright.
///
/// The `--parse` pairing is accepted deliberately: the rule is about the mode,
/// not about which phases run, and `normalize_args` already reports that no
/// `.v` will be written for a parse-only proof build.
#[test]
fn adopt_external_specs_is_accepted_in_proof_mode() {
    for extra in [vec!["-v"], vec!["--mode", "proof", "--parse"]] {
        let temp = assert_fs::TempDir::new().unwrap();
        let entry = write_source(temp.path(), "main.inf", NO_EXTERNALS_SOURCE);

        let mut cmd = Command::new(assert_cmd::cargo::cargo_bin!("infc"));
        cmd.current_dir(temp.path())
            .arg(&entry)
            .arg("--adopt-external-specs");
        for arg in &extra {
            cmd.arg(arg);
        }
        cmd.assert().success();
    }
}

/// A proof build that asks for adoption carries the library's own universal
/// obligation into its `.v`, under a key namespaced by the logical module.
///
/// The theorem is asserted, not merely the definition: a `_specs` list nothing
/// states a `ValidSpec` over would leave the obligation in the artifact and out
/// of the proof.
#[test]
fn a_proof_build_adopts_and_the_v_carries_the_theorem() {
    let (temp, entry, dep) = adopting_project();

    Command::new(assert_cmd::cargo::cargo_bin!("infc"))
        .current_dir(temp.path())
        .arg(&entry)
        .arg("-v")
        .arg("--adopt-external-specs")
        .arg("--wasm-dep")
        .arg(&dep)
        .assert()
        .success()
        .stdout(predicate::str::contains("Linked 1 external module(s)"));

    let v = adopted_v_text(&temp);
    assert!(
        v.contains("mathlib_ScaleSpec_specs"),
        "the adopted obligation must reach the .v under its namespaced key:\n{v}"
    );
    assert!(
        v.contains("ValidSpec"),
        "an adopted obligation must be stated as a theorem, not merely listed:\n{v}"
    );
}

/// The same inputs without the flag warn on stderr and carry nothing.
///
/// This is the partner that makes the test above attributable: a driver that
/// adopted unconditionally would satisfy it while ignoring the flag entirely.
#[test]
fn a_proof_build_without_the_flag_warns_on_stderr() {
    let (temp, entry, dep) = adopting_project();

    let assert = Command::new(assert_cmd::cargo::cargo_bin!("infc"))
        .current_dir(temp.path())
        .arg(&entry)
        .arg("-v")
        .arg("--wasm-dep")
        .arg(&dep)
        .assert()
        .success();

    let stderr = String::from_utf8_lossy(&assert.get_output().stderr).into_owned();
    assert!(
        stderr.contains("linked module `mathlib` ships proof obligations of its own"),
        "the report must name the library whose obligations were left behind, got:\n{stderr}"
    );
    assert!(
        stderr.contains("--adopt-external-specs"),
        "the report must carry the opt-in it is telling the reader about, got:\n{stderr}"
    );

    let v = adopted_v_text(&temp);
    assert!(
        !v.contains("mathlib_ScaleSpec"),
        "without the flag the library's obligation must not be in the .v:\n{v}"
    );
}

/// `--mode compile -v` writes a `.v` and is therefore owed the same report.
///
/// The policy follows the artifact, not the mode: this build's `.v` omits the
/// library's obligations exactly as a proof-mode one would, so keying the
/// report on the mode would silence it for the build that needs it most.
#[test]
fn a_compile_mode_v_build_still_warns() {
    let (temp, entry, dep) = adopting_project();

    let assert = Command::new(assert_cmd::cargo::cargo_bin!("infc"))
        .current_dir(temp.path())
        .arg(&entry)
        .arg("--mode")
        .arg("compile")
        .arg("-v")
        .arg("--wasm-dep")
        .arg(&dep)
        .assert()
        .success();

    let stderr = String::from_utf8_lossy(&assert.get_output().stderr).into_owned();
    assert!(
        stderr.contains("linked module `mathlib` ships proof obligations of its own"),
        "a compile-mode build that writes a .v is owed the report too, got:\n{stderr}"
    );
    assert!(
        temp.child("out").child("spec_adopted_extern.v").path().exists(),
        "the report is owed precisely because this build writes a .v"
    );
}

/// A build that writes no `.v` links the same library in silence.
///
/// Nothing would have consumed the obligations, so reporting them would put a
/// warning on every compile of every program that links a proof-mode library.
#[test]
fn a_compile_build_without_v_is_silent() {
    let (temp, entry, dep) = adopting_project();

    let assert = Command::new(assert_cmd::cargo::cargo_bin!("infc"))
        .current_dir(temp.path())
        .arg(&entry)
        .arg("--wasm-dep")
        .arg(&dep)
        .assert()
        .success()
        .stdout(predicate::str::contains("Linked 1 external module(s)"));

    let stderr = String::from_utf8_lossy(&assert.get_output().stderr).into_owned();
    assert!(
        !stderr.contains("ships proof obligations"),
        "a build that writes no .v must link the same library silently, got:\n{stderr}"
    );
}

/// `--help` documents the flag, so a user can find the opt-in the warning names.
///
/// The help text is whitespace-normalized before matching because clap rewraps
/// it to the terminal width, which would otherwise make the assertion depend on
/// where a line happened to break.
#[test]
fn help_names_the_adoption_flag() {
    let assert = Command::new(assert_cmd::cargo::cargo_bin!("infc"))
        .arg("--help")
        .assert()
        .success();
    let stdout = String::from_utf8_lossy(&assert.get_output().stdout).into_owned();
    let flowed = stdout.split_whitespace().collect::<Vec<_>>().join(" ");
    assert!(
        flowed.contains("--adopt-external-specs"),
        "the flag must appear in --help, got:\n{stdout}"
    );
    assert!(
        flowed.contains("Carry a linked library's own universal proof obligations"),
        "--help must say what the flag does, got:\n{stdout}"
    );
}

/// A construct with no lowering used to end a build as a process abort rather
/// than as a diagnostic, and both halves of the repair are visible only from
/// outside the compiler.
///
/// `return;` in a function that returns nothing is the shape that made the
/// point: the parser synthesizes a unit literal for the missing expression, and
/// the arm that received it had nothing to emit, so a program the front end had
/// just accepted exited with a stock Rust panic message and status 101. It now
/// builds, and the assertion below is that it builds *quietly* — a successful
/// exit is not enough on its own, because the failure this pins is a message on
/// stderr, not a status.
#[test]
fn a_void_return_builds_without_aborting() {
    let temp = assert_fs::TempDir::new().unwrap();
    let dest = temp.child("prog.inf");
    std::fs::write(dest.path(), "pub fn main() { return; }").unwrap();

    let mut cmd = Command::new(assert_cmd::cargo::cargo_bin!("infc"));
    cmd.current_dir(temp.path()).arg(dest.path());
    let assert = cmd
        .assert()
        .success()
        .stdout(predicate::str::contains("WASM generated"));
    let stderr = String::from_utf8_lossy(&assert.get_output().stderr).into_owned();

    assert!(
        !stderr.contains("panicked"),
        "an accepted program must compile without aborting, got:\n{stderr}"
    );
    assert!(
        temp.child("out").child("prog.wasm").path().exists(),
        "a successful build must leave an artifact"
    );
}

/// The other half: a construct that genuinely cannot be lowered is refused with
/// a rule diagnostic, not with an abort.
///
/// `string` is accepted as a type name by the type checker and has no
/// representation any later phase can produce, so it used to reach code
/// generation and die there. A bare `.failure()` would be satisfied by that
/// abort just as well as by the diagnostic, which is why the rule code and the
/// absence of a panic are both asserted, and why the artifact is checked for:
/// a refused build that still wrote a `.wasm` would be refusing after the fact.
#[test]
fn a_string_program_is_refused_with_a_diagnostic() {
    let temp = assert_fs::TempDir::new().unwrap();
    let dest = temp.child("prog.inf");
    std::fs::write(
        dest.path(),
        "pub fn main() -> i32 { let s: string = \"hi\"; return 1; }",
    )
    .unwrap();

    let mut cmd = Command::new(assert_cmd::cargo::cargo_bin!("infc"));
    cmd.current_dir(temp.path()).arg(dest.path());
    let assert = cmd.assert().failure();
    let stderr = String::from_utf8_lossy(&assert.get_output().stderr).into_owned();

    assert!(
        stderr.contains("error[A048]"),
        "the rejection must name the rule that owns it, got:\n{stderr}"
    );
    assert!(
        !stderr.contains("panicked"),
        "an unlowerable construct must produce a diagnostic, not an abort, got:\n{stderr}"
    );
    assert!(
        !temp.child("out").child("prog.wasm").path().exists(),
        "a rejected build must leave no artifact"
    );
}

/// A generic declaration is refused the same way, from outside the compiler.
///
/// It is the shape with the longest way to fall: a type parameter is accepted
/// by the parser and resolved by the type checker at each call site, and only
/// then reaches code generation with nothing standing behind it, where signature
/// lowering sees a type identifier it cannot tell from a misspelled type name.
/// Both the analysis rule and the code generation backstop behind it end the
/// build, and only the rule renders a bracketed code, so a bare `.failure()`
/// would be satisfied by either one; asserting `error[A051]` is what says the
/// refusal arrives from the phase that carries a caret, in time to stop the
/// artifact being written.
#[test]
fn a_generic_program_is_refused_with_a_diagnostic() {
    let temp = assert_fs::TempDir::new().unwrap();
    let dest = temp.child("prog.inf");
    std::fs::write(
        dest.path(),
        "fn id T'(x: T) -> T { return x; } pub fn main() -> i32 { return id(1); }",
    )
    .unwrap();

    let mut cmd = Command::new(assert_cmd::cargo::cargo_bin!("infc"));
    cmd.current_dir(temp.path()).arg(dest.path());
    let assert = cmd.assert().failure();
    let stderr = String::from_utf8_lossy(&assert.get_output().stderr).into_owned();

    assert!(
        stderr.contains("error[A051]"),
        "the rejection must name the rule that owns it, got:\n{stderr}"
    );
    assert!(
        !stderr.contains("panicked"),
        "an unlowerable construct must produce a diagnostic, not an abort, got:\n{stderr}"
    );
    assert!(
        !temp.child("out").child("prog.wasm").path().exists(),
        "a rejected build must leave no artifact"
    );
}

/// `--wasm-dep` refuses a module name whose first segment is `host`, in both the
/// bare and the `::`-qualified spelling.
///
/// The reservation is one rule, and this is the door that would otherwise skip
/// it. `--wasm-dep` binds a logical module name straight to a file and takes
/// precedence over every search directory, so a `host` entry would supply a
/// linked body under exactly the name a `use … from host::…;` clause reserves
/// for the embedder — the substitution the reservation exists to make
/// impossible, reached without the search path being consulted at all.
///
/// Refused at the flag rather than only where a manifest is read, because
/// `infs build` forwards one `--wasm-dep` per `[wasm-dependencies]` key: a rule
/// enforced only in the manifest reader holds for a project and not for a direct
/// `infc` invocation, which is a rule about the tool rather than the language.
#[test]
fn a_wasm_dep_under_the_reserved_host_segment_is_refused() {
    let temp = assert_fs::TempDir::new().unwrap();
    let entry = temp.child("prog.inf");
    std::fs::write(entry.path(), "pub fn main() -> i32 { return 1; }").unwrap();
    let lib = temp.child("x.wasm");
    std::fs::write(lib.path(), b"").unwrap();

    for name in ["host", "host::a"] {
        let dep = format!("{name}={}", lib.path().display());
        let assert = Command::new(assert_cmd::cargo::cargo_bin!("infc"))
            .current_dir(temp.path())
            .arg(entry.path())
            .arg("--wasm-dep")
            .arg(&dep)
            .assert()
            .failure();
        let stderr = String::from_utf8_lossy(&assert.get_output().stderr).into_owned();
        assert!(
            stderr.contains(&format!("invalid --wasm-dep `{dep}`")),
            "the refusal opens on the flag and the whole entry, as its two siblings do — an \
             `infs build` user whose `[wasm-dependencies]` key was forwarded here has to be able \
             to see where the name came from, got:\n{stderr}"
        );
        assert!(
            stderr.contains("reserved as the first segment")
                && stderr.contains("imports the embedder supplies"),
            "the refusal says what the segment is reserved for, got:\n{stderr}"
        );
        assert!(
            stderr.contains("If this entry was written for a host import, delete it"),
            "this is the only one of the three `host` reservations that can fire on a program \
             whose source is already correct — a working `use … from host::env;` beside a \
             `host::env` dependency entry — and for that author the entry is the redundant \
             half, got:\n{stderr}"
        );
        assert!(
            stderr.contains("Rename the module")
                && stderr.contains("only if you meant a linked `.wasm` module"),
            "the refusal carries the same remedy the two source-level reservation refusals end \
             with, and carries it conditionally: followed unconditionally it converts a working \
             host binding into a linked module, which is the provider substitution the \
             reservation exists to prevent, got:\n{stderr}"
        );
        assert!(
            stderr.contains("reserved in this position and no longer resolves to a file"),
            "the clause that marks this a migration rather than a rule the author broke, worded \
             as its two source-level siblings word it — the entry may be a `[wasm-dependencies]` \
             key forwarded here from a project that built yesterday, under a flag its author \
             never typed, got:\n{stderr}"
        );
    }

    // The rule is exact identity of the *first segment*: not a substring, so
    // `a::host` binds; not a prefix, so `hostlib` binds; not a case-insensitive
    // match, so `Host` binds. The CLI states the reservation as a raw string
    // comparison rather than through the type checker's segment identity, so
    // each of the three loosenings is one edit away and none of them would be
    // caught by refusing a single accepted control.
    for name in ["a::host", "hostlib", "Host"] {
        let assert = Command::new(assert_cmd::cargo::cargo_bin!("infc"))
            .current_dir(temp.path())
            .arg(entry.path())
            .arg("--wasm-dep")
            .arg(format!("{name}={}", lib.path().display()))
            .assert()
            .success();
        let stderr = String::from_utf8_lossy(&assert.get_output().stderr).into_owned();
        assert!(
            !stderr.contains("invalid --wasm-dep"),
            "`{name}` is an ordinary linked module and refusing it would instruct an author to \
             rename a module that was never reserved, got:\n{stderr}"
        );
    }
}

/// A program whose externs are host imports builds, and the artifact still
/// declares the import an embedder is asked to satisfy.
///
/// The end-to-end statement of the feature: no `-L`, no `--wasm-dep`, nothing on
/// disk for `env`, and a `.wasm` that carries `(import "env" "clock_ms" …)`
/// rather than a merged body. A build that quietly stripped the import would
/// pass every check the compiler makes and fail at the embedder, so the import's
/// survival is asserted on the bytes that were written.
#[test]
fn a_host_import_program_builds_and_keeps_its_import() {
    let temp = assert_fs::TempDir::new().unwrap();
    let entry = temp.child("clock.inf");
    std::fs::write(
        entry.path(),
        "external fn clock_ms() -> i64;\n\
         use { clock_ms } from host::env;\n\
         pub fn now() -> i64 { return clock_ms(); }\n",
    )
    .unwrap();

    Command::new(assert_cmd::cargo::cargo_bin!("infc"))
        .current_dir(temp.path())
        .arg(entry.path())
        .assert()
        .success();

    let artifact = temp.child("out").child("clock.wasm");
    assert!(artifact.path().exists(), "the build must write an artifact");
    let printed = wasmprinter::print_bytes(std::fs::read(artifact.path()).unwrap())
        .expect("the artifact is a decodable module");
    assert!(
        printed.contains("(import \"env\" \"clock_ms\""),
        "the import the author asked an embedder for must survive the link step:\n{printed}"
    );
}

// Host-import policy ---

/// The one-extern host program every test in this section builds on: no `-L`,
/// no `--wasm-dep`, and nothing on disk for `env`.
const HOST_CLOCK_SOURCE: &str = "\
external fn clock_ms() -> i64;
use { clock_ms } from host::env;

pub fn now() -> i64 {
    return clock_ms();
}
";

/// A program binding two host functions of one module, for the refusals that
/// have to report more than one finding.
const HOST_TWO_SOURCE: &str = "\
external fn clock_ms() -> i64;
external fn sleep_ms(n: i32);
use { clock_ms, sleep_ms } from host::env;

pub fn nap() -> i64 {
    sleep_ms(1);
    return clock_ms();
}
";

/// A program that binds a host import *and* a linked module no search directory
/// could resolve, which is what makes it a mixed program.
const HOST_AND_LINKED_SOURCE: &str = "\
external fn clock_ms() -> i64;
use { clock_ms } from host::env;
external fn add(a: i32, b: i32) -> i32;
use { add } from no_such_module;

pub fn sum() -> i32 {
    return add(1, 2);
}
";

/// Asserts a refused build left neither artifact in `out/`.
///
/// Both are checked on every refusal, not just the one the invocation asked
/// for: the artifacts are cleared before the phases run, so a rejection that
/// somehow wrote one would leave a runnable `.wasm` or a proof describing a
/// program the build refused.
fn assert_no_artifacts(root: &std::path::Path, stem: &str) {
    for extension in ["wasm", "v"] {
        let artifact = root.join("out").join(format!("{stem}.{extension}"));
        assert!(
            !artifact.exists(),
            "a refused build must write no .{extension}: {}",
            artifact.display()
        );
    }
}

/// Runs `infc` on `source` in a fresh directory and hands back the temp dir and
/// the finished assertion, so a caller can read both streams and then look at
/// what landed on disk.
fn run_host_build(source: &str, args: &[&str]) -> (assert_fs::TempDir, assert_cmd::assert::Assert) {
    let temp = assert_fs::TempDir::new().unwrap();
    let entry = write_source(temp.path(), "prog.inf", source);
    let mut cmd = Command::new(assert_cmd::cargo::cargo_bin!("infc"));
    cmd.current_dir(temp.path()).arg(&entry).args(args);
    let assert = cmd.assert();
    (temp, assert)
}

/// A host-only program builds at both targets that bind a host-call convention,
/// reports the import it will ask an embedder for, and ships it.
///
/// Both targets, because the inventory line and the declaration-level
/// conformance check are the two things a successful host-import build now
/// reports on, and only one of them is target-specific: a `spacewasm` build has
/// to print its own conformance summary as well, and a check that refused a
/// conformant host import would be caught here rather than by a user.
///
/// Fails if the inventory line stops naming the pair, if the qualifier is
/// dropped from an unpoliced build, if the conformance summary is lost behind
/// the declaration-level check, or if the import stops surviving into the
/// artifact.
#[test]
fn a_host_only_program_builds_at_every_target_that_binds_a_host() {
    for target in ["wasm32", "spacewasm"] {
        let (temp, assert) = run_host_build(HOST_CLOCK_SOURCE, &["--target", target]);
        let assert = assert.success();
        let stdout = String::from_utf8_lossy(&assert.get_output().stdout).into_owned();
        assert!(
            stdout.contains("host imports (no allowlist): env.clock_ms"),
            "a build given no allowlist must say so while naming the pair, at {target}:\n{stdout}"
        );
        if target == "spacewasm" {
            assert!(
                stdout.contains("spacewasm: conformant with WebAssembly 1.0"),
                "the conformance summary must still print for a host-import build:\n{stdout}"
            );
        }

        let artifact = temp.child("out").child("prog.wasm");
        assert!(
            artifact.path().exists(),
            "the {target} build must write an artifact"
        );
        let printed = wasmprinter::print_bytes(std::fs::read(artifact.path()).unwrap())
            .expect("the artifact is a decodable module");
        assert!(
            printed.contains("(import \"env\" \"clock_ms\""),
            "the import must survive into the {target} artifact:\n{printed}"
        );
    }
}

/// A host extern that is bound and never called is listed and shipped.
///
/// The import belongs to the binding, not to a call: it is part of what the
/// artifact requires of its environment, and an interface that appeared and
/// disappeared with the call graph would change what a deployment must provide
/// every time a caller was edited out. So both the inventory line and the
/// import section have to carry a function nothing in the program calls.
#[test]
fn a_bound_but_uncalled_host_extern_is_listed_and_shipped() {
    let (temp, assert) = run_host_build(
        "external fn telemetry(code: i32);\n\
         use { telemetry } from host::fprime_core;\n\
         \n\
         pub fn main() -> i32 {\n    \
             return 7;\n\
         }\n",
        &[],
    );
    let assert = assert.success();
    let stdout = String::from_utf8_lossy(&assert.get_output().stdout).into_owned();
    assert!(
        stdout.contains("host imports (no allowlist): fprime_core.telemetry"),
        "an uncalled binding is still an import this artifact asks for:\n{stdout}"
    );

    let printed =
        wasmprinter::print_bytes(std::fs::read(temp.child("out").child("prog.wasm").path()).unwrap())
            .expect("the artifact is a decodable module");
    assert!(
        printed.contains("(import \"fprime_core\" \"telemetry\""),
        "the import must ship whether or not the program calls it:\n{printed}"
    );
}

/// An allowlist that admits the program's imports builds, and the inventory line
/// drops the qualifier.
///
/// The qualifier is the whole of the difference between these two build logs —
/// the bytes are identical and the names are the same — so a run under a policy
/// must not carry it and a run without one must.
#[test]
fn an_admitted_host_import_builds_without_the_no_allowlist_qualifier() {
    let (_temp, assert) = run_host_build(HOST_CLOCK_SOURCE, &["--host-imports=env.clock_ms"]);
    let assert = assert.success();
    let stdout = String::from_utf8_lossy(&assert.get_output().stdout).into_owned();
    assert!(
        stdout.contains("host imports: env.clock_ms"),
        "an allowlisted build names the pairs it admitted:\n{stdout}"
    );
    assert!(
        !stdout.contains("(no allowlist)"),
        "a build given an allowlist must not be reported as unpoliced:\n{stdout}"
    );
}

/// `--host-imports=` is a policy that admits nothing, and it is refused in its
/// own words rather than as a list the import happens to be missing from.
///
/// The distinction is the point of the flag's third state: a reader told their
/// import "is not in the allowlist" while the allowlist is empty would go
/// looking for the entry that displaced it. The message says the allowlist is
/// present and empty, and names the manifest shape that spells the same policy.
///
/// Both halves of the remedy are asserted because the state has two readers. A
/// direct `infc` caller typed `--host-imports=` and has no manifest, so a
/// refusal whose only remedy was a TOML edit would offer them nothing they
/// could do; an `infs` reader never typed the flag, and is told the table with
/// no keys it was filled from and the key to write under it.
#[test]
fn an_empty_allowlist_forbids_every_host_import() {
    let (temp, assert) = run_host_build(HOST_CLOCK_SOURCE, &["--host-imports="]);
    let assert = assert.failure();
    let stderr = String::from_utf8_lossy(&assert.get_output().stderr).into_owned();
    for fragment in [
        "host import `env`.`clock_ms` is not in this build's host-import allowlist",
        "the allowlist is present and empty, which forbids every host import",
        "The allowlist (`--host-imports`) names every host function the program may bind",
        "Add `env.clock_ms` to `--host-imports` to admit it",
        "`infs` fills the flag from the `[host-imports]` table in Inference.toml, where \
         this empty policy is the table with no keys and the same edit is to add \
         `env = [\"clock_ms\"]` under it.",
        "drop every `host::env` binding of `clock_ms`.",
    ] {
        assert!(
            stderr.contains(fragment),
            "the empty-allowlist refusal must carry `{fragment}`:\n{stderr}"
        );
    }
    assert!(
        !stderr.contains("table exists") && !stderr.contains("today `infs`"),
        "the table exists, so the refusal must not say it will:\n{stderr}"
    );
    assert_no_artifacts(temp.path(), "prog");
}

/// An unadmitted host import is refused naming both the flag that carries the
/// allowlist and the manifest table `infs` fills it from, and **every**
/// unadmitted import is reported by one build.
///
/// One-at-a-time would make a program binding two unadmitted functions two
/// builds to correct. The `infs` half of the sentence is asserted because
/// `infc` has no manifest of its own: it names the flag as the mechanism and
/// the table as where a project build fills it from, and inverting that
/// would describe a file this invocation never read. The flag edit is asserted
/// in the flag's own `module.field` syntax, since `--host-imports` refuses the
/// TOML array spelling the same sentence hands an `infs` reader.
///
/// The two-import half asserts the *combined* edit, not just that both names
/// appear. Two fields of one module share one list in either mechanism, so two
/// separate edits would ask for a duplicate `env` key in a table and for two
/// halves of one list on a command line — and a reader applying them literally
/// would lose an import or write TOML that does not parse.
#[test]
fn every_unadmitted_host_import_is_reported_by_one_build() {
    let (temp, assert) = run_host_build(
        HOST_CLOCK_SOURCE,
        &["--host-imports=fprime_core.command,fprime_core.telemetry"],
    );
    let assert = assert.failure();
    let stderr = String::from_utf8_lossy(&assert.get_output().stderr).into_owned();
    for fragment in [
        "host import `env`.`clock_ms` is not in this build's host-import allowlist",
        "The allowlist (`--host-imports`) names every host function the program may bind",
        "this build was given `fprime_core.command, fprime_core.telemetry`",
        "Add `env.clock_ms` to `--host-imports` to admit it",
        "`infs` fills the flag from the `[host-imports]` table in Inference.toml, where \
         the same edit is to add `env = [\"clock_ms\"]`.",
        "drop every `host::env` binding of `clock_ms`.",
    ] {
        assert!(
            stderr.contains(fragment),
            "the refusal must carry `{fragment}`:\n{stderr}"
        );
    }
    assert!(
        !stderr.contains("table exists") && !stderr.contains("today `infs`"),
        "the table exists, so the refusal must not say it will:\n{stderr}"
    );
    assert_no_artifacts(temp.path(), "prog");

    let (temp, assert) = run_host_build(HOST_TWO_SOURCE, &["--host-imports=fprime_core.command"]);
    let assert = assert.failure();
    let stderr = String::from_utf8_lossy(&assert.get_output().stderr).into_owned();
    for fragment in [
        "host imports `env`.`clock_ms`, `env`.`sleep_ms` are not in this build's",
        "Add `env.clock_ms,env.sleep_ms` to `--host-imports` to admit them",
        "where the same edit is to add `env = [\"clock_ms\", \"sleep_ms\"]`",
        "drop every `host::env` binding of `clock_ms` and `sleep_ms`.",
    ] {
        assert!(
            stderr.contains(fragment),
            "both unadmitted imports must be one finding asking for one edit: `{fragment}` \
             missing from\n{stderr}"
        );
    }
    assert!(
        !stderr.contains("add `env = [\"clock_ms\"]`"),
        "a second block asking for the same key would be a duplicate TOML key:\n{stderr}"
    );
    assert_no_artifacts(temp.path(), "prog");
}

/// An entry that is not a `module.field` pair is refused before any phase runs,
/// with the whole entry quoted and the form spelled out.
///
/// Quoting the entry rather than the half that parsed is what makes a typo
/// findable on a long comma-separated list, and the example in the message is
/// the flag's own spelling, `=` included.
///
/// The ordering is under test, not just the wording: `infc` announces `Parsed:`
/// on stdout the moment the front end succeeds, so a build that reached this
/// refusal without printing it read the policy first. A source that parses and
/// type-checks cleanly is fed deliberately — a mistake about the build has to
/// preempt a program that has nothing wrong with it, which is the whole reason
/// the flag is read beside the other pre-phase resolutions.
///
/// A blank entry is the second half, and it is not this message: quoting an
/// empty entry back would name neither the stray comma that produced it nor the
/// `--host-imports=` spelling it is one character away from.
///
/// An entry written `host::env.clock_ms` is the third, and it is the row that
/// has to be run end to end rather than over the parser alone: it is not
/// malformed under the `module.field` rule at all — it reads as a module named
/// `host::env` — so under that rule alone it would reach the build, resolve,
/// and the program's own `env`.`clock_ms` would be refused two phases later as
/// missing from an allowlist that listed only a module named `host::env`.
/// Asserting here that nothing is parsed is what pins the earlier refusal, and
/// asserting the corrected spelling is what makes it one the reader can act
/// on: each transcription is told the fault it actually carries and the entry
/// it meant.
#[test]
fn a_malformed_host_import_entry_is_refused() {
    let (temp, assert) = run_host_build(HOST_CLOCK_SOURCE, &["--host-imports=env"]);
    let assert = assert.failure();
    let stderr = String::from_utf8_lossy(&assert.get_output().stderr).into_owned();
    for fragment in [
        "invalid `--host-imports` entry `env`",
        "each entry is a host import in `module.field` form",
        "--host-imports=env.clock_ms,fprime_core.command",
    ] {
        assert!(
            stderr.contains(fragment),
            "the refusal must carry `{fragment}`:\n{stderr}"
        );
    }
    let stdout = String::from_utf8_lossy(&assert.get_output().stdout).into_owned();
    assert!(
        !stdout.contains("Parsed:"),
        "a malformed allowlist entry must be refused before any phase runs:\n{stdout}"
    );
    assert_no_artifacts(temp.path(), "prog");

    for (transcribed, fault) in [
        (
            "--host-imports=env::clock_ms",
            "entry `env::clock_ms`: the separator here is a dot, not `::`.",
        ),
        (
            "--host-imports=host::env.clock_ms",
            "entry `host::env.clock_ms`: the `host::` prefix is not part of an import name.",
        ),
    ] {
        let (temp, assert) = run_host_build(HOST_CLOCK_SOURCE, &[transcribed]);
        let assert = assert.failure();
        let stderr = String::from_utf8_lossy(&assert.get_output().stderr).into_owned();
        assert!(
            stderr.contains(fault) && stderr.contains("Write it `env.clock_ms`."),
            "{transcribed} must be told its own fault and the entry it meant:\n{stderr}"
        );
        assert!(
            !stderr.contains("host-import allowlist"),
            "{transcribed} must not reach the allowlist and read as a broken \
             mechanism:\n{stderr}"
        );
        let stdout = String::from_utf8_lossy(&assert.get_output().stdout).into_owned();
        assert!(
            !stdout.contains("Parsed:"),
            "{transcribed} must be refused before any phase runs:\n{stdout}"
        );
        assert_no_artifacts(temp.path(), "prog");
    }

    let (temp, assert) = run_host_build(HOST_CLOCK_SOURCE, &["--host-imports=env.clock_ms,"]);
    let assert = assert.failure();
    let stderr = String::from_utf8_lossy(&assert.get_output().stderr).into_owned();
    for fragment in [
        "the list has an empty entry, which is a stray or trailing comma",
        "`--host-imports=` on its own is the empty allowlist",
    ] {
        assert!(
            stderr.contains(fragment),
            "a trailing comma must be named rather than quoted back as nothing: `{fragment}` \
             missing from\n{stderr}"
        );
    }
    assert_no_artifacts(temp.path(), "prog");
}

/// Every spelling that writes a `.v` is refused for a program binding a host
/// import, including the one that keeps compile mode.
///
/// `--mode compile -v` is the spelling a gate on the mode alone would miss, and
/// it is exactly the one that would have shipped a proof quietly assuming the
/// import away. Both spellings are asserted against the same wording, because
/// two refusals for one fact would be two wordings to keep in step.
///
/// The remedy is asserted to name the artifact rather than a mode. `-v` sets
/// the `.v` request on its own, so "build with `--mode compile`" clears exactly
/// one of the three spellings and sends `--mode compile -v` round the same
/// refusal having changed nothing. The row passing both requests is why the
/// remedy asks for every one of them to go: told to drop one of two, its reader
/// would be refused again.
///
/// The analyze-only row is the other half of the predicate. `--analyze --mode
/// proof` requests a `.v` and writes none — `normalize_args` says so in a
/// warning on the same run — so a gate on the request alone would refuse a
/// build in one breath for the mode it had just called irrelevant.
#[test]
fn a_proof_build_that_binds_a_host_import_is_refused_in_both_spellings() {
    for spelling in [
        vec!["--mode", "proof"],
        vec!["--mode", "compile", "-v"],
        vec!["--mode", "proof", "-v"],
    ] {
        let (temp, assert) = run_host_build(HOST_CLOCK_SOURCE, &spelling);
        let assert = assert.failure();
        let stderr = String::from_utf8_lossy(&assert.get_output().stderr).into_owned();
        for fragment in [
            "Host imports are not yet modeled in the proof translation",
            "`env`.`clock_ms` is satisfied by the embedder",
            "the translation has no way to state an assumption about it",
            "Build this program without a proof artifact, or remove the host import from the \
             program.",
            "`-v` and `--mode proof` each request a `.v` on their own, so drop every one of them \
             this build passed;",
        ] {
            assert!(
                stderr.contains(fragment),
                "{spelling:?} must be refused with `{fragment}`:\n{stderr}"
            );
        }
        assert!(
            !stderr.contains("Build this program with `--mode compile`"),
            "{spelling:?} must not be told to pass a flag it already passed:\n{stderr}"
        );
        assert_no_artifacts(temp.path(), "prog");
    }

    for spelling in [vec!["--analyze", "--mode", "proof"], vec!["--analyze", "-v"]] {
        let (temp, assert) = run_host_build(HOST_CLOCK_SOURCE, &spelling);
        let assert = assert.success();
        let stderr = String::from_utf8_lossy(&assert.get_output().stderr).into_owned();
        assert!(
            !stderr.contains("Host imports are not yet modeled"),
            "{spelling:?} writes no .v, so it is owed no proof refusal:\n{stderr}"
        );
        assert_no_artifacts(temp.path(), "prog");
    }
}

/// Writes a program that binds one host import from two files, and returns the
/// entry file.
///
/// `main.inf` and `lib/clock.inf` both declare and bind `env`.`clock_ms`, and
/// only `lib/clock.inf` binds `env`.`sleep_ms`. So `clock_ms` is one import an
/// embedder registers once behind two `use` clauses, and a message that names
/// `sleep_ms` at all is proof the second file's declarations arrived.
fn write_two_file_host_program(root: &std::path::Path) -> std::path::PathBuf {
    let entry = write_source(
        root,
        "main.inf",
        "use lib::clock;\n\
         \n\
         external fn clock_ms() -> i64;\n\
         use { clock_ms } from host::env;\n\
         \n\
         pub fn main() -> i64 {\n    \
             return clock_ms();\n\
         }\n",
    );
    write_source(
        root,
        "lib/clock.inf",
        "external fn clock_ms() -> i64;\n\
         external fn sleep_ms(n: i32);\n\
         use { clock_ms, sleep_ms } from host::env;\n\
         \n\
         pub fn now() -> i64 {\n    \
             sleep_ms(1);\n    \
             return clock_ms();\n\
         }\n",
    );
    entry
}

/// One `(module, field)` declared and bound in two files is one host import,
/// and the refusal that reads the declarations names it once.
///
/// A declaration's provenance is keyed on the declaration, so the same function
/// reached from two files arrives as two origins. An embedder registers it once
/// and the linker resolves it to one import, so a message that named it twice
/// would report one function as two findings and then disagree with itself
/// about the plural. The proof refusal is where this shows, because it is the
/// one message read off the declarations rather than off what resolution
/// produced — and every other host-import test here is single-file, so nothing
/// else reaches the two-origin case at all.
///
/// The imported file binds a host function of its own, and the refusal is
/// asserted to name it. Without that the test asserts its own premise nowhere:
/// `clock_ms` is declared identically in both files, so a `lib/clock.inf` whose
/// declarations never reached the typed context at all would leave one origin,
/// one name, and every assertion green — with the deduplication under test
/// never exercised and freely removable. `sleep_ms` appears in exactly one of
/// the two files, so naming it is the same run proving the second file arrived.
/// The singular agreement the wording also decides is covered by the unit test
/// `host_import_proof_refusal_agrees_with_its_subject`, which is where it can
/// be asserted against both arities.
#[test]
fn one_host_import_reached_from_two_files_is_named_once() {
    let temp = assert_fs::TempDir::new().unwrap();
    let entry = write_two_file_host_program(temp.path());
    let mut cmd = Command::new(assert_cmd::cargo::cargo_bin!("infc"));
    cmd.current_dir(temp.path())
        .arg(&entry)
        .args(["--mode", "proof"]);
    let assert = cmd.assert().failure();
    let stderr = String::from_utf8_lossy(&assert.get_output().stderr).into_owned();
    assert_eq!(
        stderr.matches("`env`.`clock_ms`").count(),
        1,
        "two declarations of one import are one name in a message:\n{stderr}"
    );
    assert!(
        stderr.contains(
            "`env`.`clock_ms`, `env`.`sleep_ms` are satisfied by the embedder, so their \
             behavior"
        ),
        "the imported file's own binding must reach the typed context, or the two-origin \
         case is never exercised:\n{stderr}"
    );
    assert_no_artifacts(temp.path(), "main");
}

/// An allowlist refusal for an import two files bind names it once and asks for
/// every binding of it, not for "the" one.
///
/// The refusal is read off what resolution produced, which is deduplicated, so
/// it sees one `env`.`clock_ms` where the program holds a clause in each of two
/// files. Told to drop "the" binding, a reader edits one file and is refused
/// again by the other, so the source-level remedy quantifies over binding sites
/// — and this is the one allowlist row with two sites for that to be right
/// about. Every other allowlist test here builds a single file, where "the"
/// and "every" read the same.
///
/// The fully admitted run is the control that keeps the premise honest: it
/// names `sleep_ms`, which only `lib/clock.inf` binds, so the second file's
/// clause really reached resolution; and it names `clock_ms` once, so the two
/// clauses really are one import rather than two findings the refusal happens
/// to phrase alike. Both runs share one directory, so the refused run's empty
/// `out/` also shows it cleared the artifact the control left there.
#[test]
fn an_allowlist_refusal_asks_for_every_binding_of_an_import_two_files_bind() {
    let temp = assert_fs::TempDir::new().unwrap();
    let entry = write_two_file_host_program(temp.path());

    let control = Command::new(assert_cmd::cargo::cargo_bin!("infc"))
        .current_dir(temp.path())
        .arg(&entry)
        .arg("--host-imports=env.clock_ms,env.sleep_ms")
        .assert()
        .success();
    let stdout = String::from_utf8_lossy(&control.get_output().stdout).into_owned();
    assert!(
        stdout.contains("host imports: env.clock_ms, env.sleep_ms\n"),
        "both files' bindings must reach resolution as one import each:\n{stdout}"
    );

    let refused = Command::new(assert_cmd::cargo::cargo_bin!("infc"))
        .current_dir(temp.path())
        .arg(&entry)
        .arg("--host-imports=env.sleep_ms")
        .assert()
        .failure();
    let stderr = String::from_utf8_lossy(&refused.get_output().stderr).into_owned();
    assert_eq!(
        stderr.matches("`env`.`clock_ms`").count(),
        1,
        "one import bound in two files is one finding:\n{stderr}"
    );
    for fragment in [
        "host import `env`.`clock_ms` is not in this build's host-import allowlist",
        "this build was given `env.sleep_ms`",
        "Add `env.clock_ms` to `--host-imports` to admit it, or drop every `host::env` \
         binding of `clock_ms`.",
        "where the same edit is to add `\"clock_ms\"` to the `env` entry",
    ] {
        assert!(
            stderr.contains(fragment),
            "the refusal must carry `{fragment}`:\n{stderr}"
        );
    }
    assert!(
        !stderr.contains("the `host::env` binding"),
        "a name two files bind must not be told it has one binding:\n{stderr}"
    );
    assert!(
        !stderr.contains("`env`.`sleep_ms`"),
        "an admitted import must not be reported:\n{stderr}"
    );
    assert_no_artifacts(temp.path(), "main");
}

/// The proof refusal is raised before external resolution, so a program that is
/// both mixed and a proof build hears the refusal that governs its build rather
/// than the one whose remedy walks it into this one.
///
/// The compile-mode run is the control, and it is what keeps this test from
/// being vacuous: the same source really does fail at resolution, with the
/// mixed-program wording whose first remedy is to bind every extern to
/// `host::…`. Following that from a proof build means rewriting every `use`
/// clause and arriving here anyway.
#[test]
fn the_proof_refusal_preempts_external_resolution() {
    let (temp, assert) = run_host_build(HOST_AND_LINKED_SOURCE, &[]);
    let assert = assert.failure();
    let control = String::from_utf8_lossy(&assert.get_output().stderr).into_owned();
    assert!(
        control.contains("External module resolution failed")
            && control.contains("host imports and statically linked modules cannot be combined"),
        "the control run must reach resolution and be refused as mixed:\n{control}"
    );
    assert_no_artifacts(temp.path(), "prog");

    for spelling in [vec!["--mode", "proof"], vec!["-v"]] {
        let (temp, assert) = run_host_build(HOST_AND_LINKED_SOURCE, &spelling);
        let assert = assert.failure();
        let stderr = String::from_utf8_lossy(&assert.get_output().stderr).into_owned();
        assert!(
            stderr.contains("Host imports are not yet modeled in the proof translation"),
            "{spelling:?} must hear the proof refusal:\n{stderr}"
        );
        assert!(
            !stderr.contains("External module resolution failed"),
            "{spelling:?} must not reach external resolution:\n{stderr}"
        );
        assert!(
            !stderr.contains("cannot be combined"),
            "{spelling:?} must not be told to bind every extern to `host::…`:\n{stderr}"
        );
        assert_no_artifacts(temp.path(), "prog");
    }
}

/// A host import no SpaceWasm embedder could register is refused at that target,
/// while the same program builds at a target whose embedder has no such limit.
///
/// Both caps are exercised from source: a 32-byte import module name, one past
/// the 31-byte name type an embedder registers through, and a ten-parameter
/// `external fn`, one past the fixed argument list a host call is dispatched
/// with. The `wasm32` control is what makes this a *target* rule rather than a
/// rule about declarations — without it, a refusal that fired everywhere would
/// pass.
///
/// The authority sentence is asserted because it is what separates this from a
/// decode limit: the module the caps refuse decodes perfectly well and simply
/// cannot be bound to a host.
///
/// Both rows assert the declaration-level header and the absence of the
/// artifact-level one, because the two checks share a body and find the same
/// thing: without that a call site deleted or mis-wired would simply be caught
/// by the post-link backstop and the test would stay green. The header is not
/// the only difference, though. On the name row the artifact-level rendering
/// also offers the linked-module alternative, which a finding read off a
/// declaration cannot use, so that row asserts the remedy ends at the source
/// edit — a second, independent sign that the declaration-level call site is
/// the one that refused. The name row also pins which half of the import was
/// measured — the module and the field are adjacent `&str` fields that a swap
/// at the call site would silently exchange, and a length of 32 is reported
/// either way.
#[test]
fn a_host_import_no_spacewasm_embedder_could_register_is_refused() {
    let over_long = "m".repeat(32);
    let long_name = format!(
        "external fn ping();\n\
         use {{ ping }} from host::{over_long};\n\
         \n\
         pub fn main() -> i32 {{\n    \
             ping();\n    \
             return 0;\n\
         }}\n"
    );

    let (temp, assert) = run_host_build(&long_name, &["--target", "spacewasm"]);
    let assert = assert.failure();
    let stderr = String::from_utf8_lossy(&assert.get_output().stderr).into_owned();
    let measured_half = format!(
        "import module name `{over_long}` on `{over_long}`.`ping` is 32 bytes; SpaceWasm \
         accepts at most 31"
    );
    for fragment in [
        "SpaceWasm conformance failed: the host imports this program declares cannot all be \
         registered by a SpaceWasm embedder.",
        "No file was written.",
        measured_half.as_str(),
        "This is a registration limit, not a decode limit",
    ] {
        assert!(
            stderr.contains(fragment),
            "the refusal must carry `{fragment}`:\n{stderr}"
        );
    }
    assert!(
        !stderr.contains("is not a module a SpaceWasm embedder can load"),
        "the name cap must be caught over the declarations, not by the post-link \
         backstop:\n{stderr}"
    );
    assert!(
        stderr.contains("Shorten the module name this extern is bound under.\n"),
        "a finding read off a declaration ends at the source edit:\n{stderr}"
    );
    assert!(
        !stderr.contains("If the import came from a linked module"),
        "a finding read off a declaration must not be offered a provenance it cannot \
         have:\n{stderr}"
    );
    assert_no_artifacts(temp.path(), "prog");

    let (_temp, assert) = run_host_build(&long_name, &["--target", "wasm32"]);
    let assert = assert.success();
    let stdout = String::from_utf8_lossy(&assert.get_output().stdout).into_owned();
    assert!(
        stdout.contains(&format!("host imports (no allowlist): {over_long}.ping")),
        "the same program must build where no registration cap applies:\n{stdout}"
    );

    let wide = "external fn wide(a: i32, b: i32, c: i32, d: i32, e: i32, f: i32, g: i32, \
                h: i32, i: i32, j: i32);\n\
                use { wide } from host::env;\n\
                \n\
                pub fn main() -> i32 {\n    \
                    wide(1, 2, 3, 4, 5, 6, 7, 8, 9, 10);\n    \
                    return 0;\n\
                }\n";
    let (temp, assert) = run_host_build(wide, &["--target", "spacewasm"]);
    let assert = assert.failure();
    let stderr = String::from_utf8_lossy(&assert.get_output().stderr).into_owned();
    for fragment in [
        "SpaceWasm conformance failed: the host imports this program declares cannot all be \
         registered by a SpaceWasm embedder.",
        "import `env`.`wide` declares 10 parameters; SpaceWasm accepts at most 9",
        "This is a registration limit, not a decode limit",
    ] {
        assert!(
            stderr.contains(fragment),
            "the arity refusal must carry `{fragment}`:\n{stderr}"
        );
    }
    assert!(
        !stderr.contains("is not a module a SpaceWasm embedder can load"),
        "the arity cap must be caught over the declarations, not by the post-link \
         backstop:\n{stderr}"
    );
    assert_no_artifacts(temp.path(), "prog");

    let (_temp, assert) = run_host_build(wide, &["--target", "wasm32"]);
    let assert = assert.success();
    let stdout = String::from_utf8_lossy(&assert.get_output().stdout).into_owned();
    assert!(
        stdout.contains("host imports (no allowlist): env.wide"),
        "the arity cap is the target's, not the declaration's, so the same program must \
         build where no registration cap applies:\n{stdout}"
    );
}

/// A host import sitting exactly on both SpaceWasm registration caps builds.
///
/// The refusing rows above pin the measured numbers and so catch an off-by-one,
/// but every one of them measures a quantity the wrong wiring would inflate,
/// and both return unit. That leaves the accepting edge untested and one wiring
/// mistake invisible: a call site handing `check_host_imports` the parameter
/// count *plus* the result count still reports `declares 10 parameters` for the
/// ten-parameter refusal, and still passes the one-result `env.clock_ms` used
/// everywhere else, while wrongly refusing every conformant nine-parameter host
/// import that returns a value. This fixture is that import — the cap in
/// parameters, the cap in module-name bytes, and a result on top, so no count
/// can absorb another and stay under.
///
/// Both numbers are built from the caps themselves, so an embedder that widens
/// one moves the fixture with it rather than leaving a row that tests an
/// interior point.
#[test]
fn a_host_import_exactly_at_both_spacewasm_caps_builds() {
    use inference_target_conformance::spacewasm::{
        MAX_HOST_FUNCTION_PARAMS, MAX_IMPORT_NAME_BYTES,
    };

    let at_cap = "m".repeat(MAX_IMPORT_NAME_BYTES);
    let params = (0..MAX_HOST_FUNCTION_PARAMS)
        .map(|index| format!("p{index}: i32"))
        .collect::<Vec<_>>()
        .join(", ");
    let arguments = (0..MAX_HOST_FUNCTION_PARAMS)
        .map(|index| index.to_string())
        .collect::<Vec<_>>()
        .join(", ");
    let source = format!(
        "external fn widest({params}) -> i32;\n\
         use {{ widest }} from host::{at_cap};\n\
         \n\
         pub fn main() -> i32 {{\n    \
             return widest({arguments});\n\
         }}\n"
    );

    let (temp, assert) = run_host_build(&source, &["--target", "spacewasm"]);
    let assert = assert.success();
    let stdout = String::from_utf8_lossy(&assert.get_output().stdout).into_owned();
    assert!(
        stdout.contains(&format!("host imports (no allowlist): {at_cap}.widest")),
        "an import on both caps is conformant and must build:\n{stdout}"
    );
    assert!(
        stdout.contains("spacewasm: conformant with WebAssembly 1.0"),
        "the post-link check must accept it too, or only the declaration-level one is \
         measured:\n{stdout}"
    );
    assert!(
        temp.child("out").child("prog.wasm").path().exists(),
        "the accepted build must write its artifact"
    );
}

/// At `--target spacewasm` the registration caps are asked before the
/// allowlist, so the allowlist is never asked to admit a name no embedder can
/// register.
///
/// An allowlist entry has to spell the import's final name. Asked first, the
/// allowlist tells the reader to add an over-long module name to
/// `--host-imports`; the caps then refuse that same name and ask for a rename,
/// which leaves the new entry matching nothing and the renamed import refused
/// by the allowlist again — three builds, and a reviewed policy admitting a
/// name no embedder could ever register.
///
/// Two allowlists that do not admit the import are run: the empty one and one
/// naming something else. The `wasm32` run of each is the control that keeps
/// this from being vacuous: with no cap to fail, the same program under the
/// same allowlist is refused by the allowlist, so it really is reachable here
/// and it is the order that keeps it silent at SpaceWasm.
#[test]
fn the_registration_caps_are_asked_before_the_allowlist() {
    use inference_target_conformance::spacewasm::MAX_IMPORT_NAME_BYTES;

    let over_long = "m".repeat(MAX_IMPORT_NAME_BYTES + 1);
    let source = format!(
        "external fn ping();\n\
         use {{ ping }} from host::{over_long};\n\
         \n\
         pub fn main() -> i32 {{\n    \
             ping();\n    \
             return 0;\n\
         }}\n"
    );

    for allowlist in ["--host-imports=", "--host-imports=env.clock_ms"] {
        let (temp, assert) = run_host_build(&source, &["--target", "spacewasm", allowlist]);
        let assert = assert.failure();
        let stderr = String::from_utf8_lossy(&assert.get_output().stderr).into_owned();
        assert!(
            stderr.contains(
                "SpaceWasm conformance failed: the host imports this program declares cannot \
                 all be registered by a SpaceWasm embedder."
            ),
            "the registration cap must be what refuses, under {allowlist}:\n{stderr}"
        );
        assert!(
            !stderr.contains("not in this build's host-import allowlist"),
            "the allowlist must not be asked to admit a name no embedder can register, under \
             {allowlist}:\n{stderr}"
        );
        assert_no_artifacts(temp.path(), "prog");

        let (temp, assert) = run_host_build(&source, &["--target", "wasm32", allowlist]);
        let assert = assert.failure();
        let control = String::from_utf8_lossy(&assert.get_output().stderr).into_owned();
        assert!(
            control.contains(&format!(
                "host import `{over_long}`.`ping` is not in this build's host-import allowlist"
            )),
            "where no cap applies, {allowlist} must refuse the same import:\n{control}"
        );
        assert_no_artifacts(temp.path(), "prog");
    }
}

/// At a target that binds no host, the target's refusal of a host import is
/// heard before external resolution and before the allowlist, whose remedies
/// it would make void.
///
/// Three rows. A plain host program is refused in code generation's words but
/// by `infc`, ahead of code generation: the `Error:` prefix is `infc`'s, where
/// the backstop inside code generation prints `Codegen failed:`. Under an
/// empty allowlist, the other order tells the reader to admit the import, and
/// the build that admits it is refused by the target anyway, leaving a policy
/// entry nothing can use. A mixed program would otherwise hear resolution's
/// refusal first, whose remedy — bind every extern to the host — walks its
/// reader into this refusal having rewritten every clause.
///
/// The `wasm32` runs are the controls: the same allowlist and the same mixed
/// program really are refused by the allowlist and by resolution where the
/// target binds a host, so the absence of those wordings at `stellar` is the
/// order at work and not a refusal that never had a chance to fire.
#[test]
fn a_host_import_at_a_target_that_binds_no_host_is_refused_first() {
    let stellar_refusal = "Error: The `stellar` target does not support host imports. \
                           `external fn clock_ms` is bound to `host::env`";

    let (temp, assert) = run_host_build(HOST_CLOCK_SOURCE, &["--target", "stellar"]);
    let assert = assert.failure();
    let stderr = String::from_utf8_lossy(&assert.get_output().stderr).into_owned();
    assert!(
        stderr.contains(stellar_refusal) && !stderr.contains("Codegen failed"),
        "the target refusal must be raised by `infc` before code generation:\n{stderr}"
    );
    assert_no_artifacts(temp.path(), "prog");

    let (temp, assert) =
        run_host_build(HOST_CLOCK_SOURCE, &["--target", "stellar", "--host-imports="]);
    let assert = assert.failure();
    let stderr = String::from_utf8_lossy(&assert.get_output().stderr).into_owned();
    assert!(
        stderr.contains(stellar_refusal),
        "the target refusal must win over the allowlist:\n{stderr}"
    );
    assert!(
        !stderr.contains("host-import allowlist"),
        "a reader must not be told to admit an import the target refuses:\n{stderr}"
    );
    assert_no_artifacts(temp.path(), "prog");

    let (temp, assert) =
        run_host_build(HOST_CLOCK_SOURCE, &["--target", "wasm32", "--host-imports="]);
    let assert = assert.failure();
    let control = String::from_utf8_lossy(&assert.get_output().stderr).into_owned();
    assert!(
        control.contains("host import `env`.`clock_ms` is not in this build's host-import \
                          allowlist"),
        "the control must reach the allowlist:\n{control}"
    );
    assert_no_artifacts(temp.path(), "prog");

    let (temp, assert) = run_host_build(HOST_AND_LINKED_SOURCE, &["--target", "stellar"]);
    let assert = assert.failure();
    let stderr = String::from_utf8_lossy(&assert.get_output().stderr).into_owned();
    assert!(
        stderr.contains(stellar_refusal),
        "the target refusal must win over resolution:\n{stderr}"
    );
    assert!(
        !stderr.contains("External module resolution failed")
            && !stderr.contains("cannot be combined"),
        "a mixed program at `stellar` must not be told to bind every extern to the \
         host:\n{stderr}"
    );
    assert_no_artifacts(temp.path(), "prog");

    let (temp, assert) = run_host_build(HOST_AND_LINKED_SOURCE, &["--target", "wasm32"]);
    let assert = assert.failure();
    let control = String::from_utf8_lossy(&assert.get_output().stderr).into_owned();
    assert!(
        control.contains("host imports and statically linked modules cannot be combined"),
        "the control must reach resolution and be refused as mixed:\n{control}"
    );
    assert_no_artifacts(temp.path(), "prog");
}

/// `--help` documents `--host-imports` in its reader's terms: the three states
/// and where `infs build` fills the flag from.
///
/// clap prints the field's doc comment as the help text, so a maintainer note
/// left in it ships to every `infc --help`; the Rust type and the clap
/// attribute behind the flag's shape are asserted absent for that reason. The
/// forwarding sentence is pinned because it is the one a toolchain change moves:
/// it said the flag was not forwarded until the `[host-imports]` table landed in
/// `infs`, and the retired wording is asserted absent so it cannot come back.
#[test]
fn help_documents_the_host_import_allowlist() {
    let assert = Command::new(assert_cmd::cargo::cargo_bin!("infc"))
        .arg("--help")
        .assert()
        .success();
    let stdout = String::from_utf8_lossy(&assert.get_output().stdout).into_owned();
    let flowed = stdout.split_whitespace().collect::<Vec<_>>().join(" ");
    for fragment in [
        "--host-imports=<LIST>",
        "`--host-imports=` with nothing after it is a declared, empty allowlist that admits none",
        "`infs build` and `infs run` forward one entry per field of each `Inference.toml \
         [host-imports]` module, and a declared-but-empty table as `--host-imports=`; direct \
         `infc` callers may pass the list by hand.",
    ] {
        assert!(
            flowed.contains(fragment),
            "--help must carry `{fragment}`, got:\n{stdout}"
        );
    }
    assert!(
        !flowed.contains("forward this flag yet"),
        "the table exists, so --help must not say the flag goes unforwarded:\n{stdout}"
    );
    for internal in ["Option<Vec<String>>", "require_equals"] {
        assert!(
            !flowed.contains(internal),
            "a maintainer note must not ship as help text: `{internal}` in\n{stdout}"
        );
    }
}

/// A program that binds no host import prints no host-import line at all.
///
/// The inventory line is the surface a reviewer scans a build log for, so it
/// must not appear — with or without its qualifier — on the builds that have
/// nothing to report. Asserted under an allowlist as well, since a policy given
/// to a program with no host imports still names no imports.
#[test]
fn a_program_with_no_host_import_prints_no_inventory_line() {
    for args in [vec![], vec!["--host-imports=env.clock_ms"]] {
        let (_temp, assert) = run_host_build(COMPOUND_COPY_SOURCE, &args);
        let assert = assert.success();
        let stdout = String::from_utf8_lossy(&assert.get_output().stdout).into_owned();
        assert!(
            !stdout.contains("host imports"),
            "a program binding no host import must print no inventory line, with {args:?}:\n{stdout}"
        );
    }
}
