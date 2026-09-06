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
/// `--adopt-external-specs` flag. The `abi_version_flag_prints_and_exits`
/// test above checks the binary against the shared constant; this one
/// additionally asserts the concrete `1.4` so an accidental constant change is
/// caught here too.
///
/// Uses an exact trimmed equality (not `contains`) so a near-miss such as
/// "11.4" or "1.40" — which would satisfy a substring match — cannot pass.
#[test]
fn abi_version_is_one_dot_four() {
    let mut cmd = Command::new(assert_cmd::cargo::cargo_bin!("infc"));
    cmd.arg("--abi-version");
    let assert = cmd.assert().success();
    let output = assert.get_output();
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert_eq!(
        stdout.trim(),
        "1.4",
        "ABI version must be exactly 1.4, not merely contain it"
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
