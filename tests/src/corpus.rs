//! The public face of this crate's compile helpers.
//!
//! Everything the helpers do lives in `utils`, which is `pub(crate)`: it is
//! reachable from the unit tests compiled inside this library and invisible to
//! an integration binary under `tests/`, because an integration binary is a
//! separate crate that sees only what this one exports. The Stellar host tier
//! is such a binary — it needs a real host, so it cannot live in `src/` — and
//! this module is the seam that lets it compile Inference source at all.
//!
//! Nothing here re-implements a pipeline. Each compile function is one call into
//! the same `utils` helper the in-library tests use, so a fixture compiled
//! through this module and the same fixture compiled inside `src/` cannot
//! diverge. [`compile_for_stellar`] is the one addition: the steps `infc` runs
//! *after* code generation, which no in-library test needed before.
//!
//! The corpus walkers below are here for the same reason the compile helpers
//! are: more than one test module needs them, and one of them had already been
//! copied into a second module. They are the single answer to "which fixtures
//! are there", so two sweeps that each claim corpus breadth are talking about
//! the same set.
//!
//! Being `pub` in a library module, they compile on every `cargo build` of this
//! crate rather than only under `cfg(test)`. That costs nothing because
//! `lib.rs` carries `#![allow(dead_code)]` — without it, the functions no
//! integration binary happens to call would warn on every build.

use inference_wasm_codegen::{CodegenOutput, CompilationMode, Target};

/// The `test_data` directory of this crate.
#[must_use]
pub fn test_data_path() -> std::path::PathBuf {
    crate::utils::get_test_data_path()
}

/// Compiles `source` for `target` at that target's own default optimization
/// level, in compile mode, with the analysis pass running.
///
/// The optimization level is the target's rather than a fixed one because that
/// is what the two command lines really produce: `Wasm32` builds at `O3` and
/// `Stellar` at `Oz`, and a comparison between the targets is only about the
/// targets if each side is built the way it ships.
///
/// # Panics
///
/// Panics if `source` does not compile for `target`.
#[must_use]
pub fn wasm_for_target(source: &str, target: Target) -> Vec<u8> {
    crate::utils::wasm_codegen_with_target(source, target)
}

/// Runs code generation for `target` in compile mode, keeping the full output
/// so a caller can read the per-export descriptor beside the bytes.
///
/// # Errors
///
/// Returns the code generator's own error, including the Stellar
/// admissibility refusals, so a caller can assert on one.
pub fn codegen_for_target(source: &str, target: Target) -> anyhow::Result<CodegenOutput> {
    crate::utils::codegen_with_target_mode(source, target, CompilationMode::Compile)
}

/// Runs code generation for `target` in compile mode at that target's own
/// default optimization level, with the analysis pass **skipped**.
///
/// Every other helper here runs analysis, which is right for a caller compiling
/// one hand-written fixture. It is wrong for a caller sweeping the corpus: the
/// fixtures are written to exercise code generation, and a good number of them
/// exercise a construct an analysis rule legitimately rejects — a `forall` block
/// outside a `spec` (A042), a compound uzumaki (A027). Those still have to reach
/// code generation, or a sweep silently stops covering exactly the shapes it was
/// written to cover.
///
/// # Errors
///
/// Returns the code generator's own error, including every target gate, so a
/// caller can tell a refusal apart from a fixture that does not compile at all.
pub fn codegen_for_target_no_analysis(
    source: &str,
    target: Target,
) -> anyhow::Result<CodegenOutput> {
    crate::utils::codegen_with_target_mode_no_analysis(source, target, CompilationMode::Compile)
}

/// Compiles `source` into an uploadable Soroban contract, by the same route
/// `infc --target stellar` takes: code generation at [`Target::Stellar`], the
/// link step, then the Val-ABI rewrite.
///
/// The link is present because the compiler runs it, not because these
/// fixtures need it. None of them declares an external, so it is the
/// documented byte-identical pass-through; keeping it here means the helper
/// stays the whole compiler path rather than the part that happened to matter
/// on the day it was written.
///
/// # Errors
///
/// Returns whichever step refused: the source-level admissibility gate inside
/// code generation, the linker, or the rewriter.
pub fn try_compile_for_stellar(source: &str) -> anyhow::Result<Vec<u8>> {
    let output = codegen_for_target(source, Target::Stellar)?;
    let linked = inference::link(output.wasm(), &[], None)?;
    let contract = inference_stellar_abi::rewrite(
        &linked,
        output.export_signatures(),
        inference_stellar_abi::STELLAR_ENV_PROTOCOL,
    )?;
    Ok(contract)
}

/// [`try_compile_for_stellar`] for a fixture that is expected to compile.
///
/// # Panics
///
/// Panics with the refusing step's message if `source` does not produce a
/// contract.
#[must_use]
pub fn compile_for_stellar(source: &str) -> Vec<u8> {
    try_compile_for_stellar(source)
        .unwrap_or_else(|e| panic!("this fixture must compile for the Stellar target: {e}"))
}

/// Root of the opt-in golden family: modules the compiler produced with a
/// post-MVP feature requested, so they are deliberately not Wasm 1.0.
///
/// Both partitions are defined by this one path — the default gates exclude
/// the subtree and the opt-in gates cover exactly it — so the two can only
/// disagree about which side a golden belongs to, never leave one ungated.
#[must_use]
pub fn bulk_memory_golden_root() -> std::path::PathBuf {
    test_data_path()
        .join("codegen")
        .join("wasm")
        .join("bulk_memory_golden")
}

/// Every golden `.wasm` under `tests/test_data/codegen`, both partitions,
/// sorted so failures name the same file across runs.
///
/// `out` directories are skipped: they are the gitignored landing place for
/// `infs build` run by hand against a fixture tree, so whatever they hold is
/// whichever compiler someone last pointed at it, not a golden this suite
/// maintains.
#[must_use]
pub fn all_golden_wasm_artifacts() -> Vec<std::path::PathBuf> {
    fn collect(dir: &std::path::Path, found: &mut Vec<std::path::PathBuf>) {
        let entries = std::fs::read_dir(dir)
            .unwrap_or_else(|e| panic!("failed to read {}: {e}", dir.display()));
        for entry in entries {
            let path = entry.expect("failed to read a directory entry").path();
            if path.is_dir() {
                if path.file_name().is_some_and(|name| name == "out") {
                    continue;
                }
                collect(&path, found);
            } else if path.extension().is_some_and(|ext| ext == "wasm") {
                found.push(path);
            }
        }
    }
    let mut found = Vec::new();
    collect(&test_data_path().join("codegen"), &mut found);
    found.sort();
    found
}

/// The goldens the WebAssembly 1.0 invariants apply to: everything except the
/// opt-in family.
#[must_use]
pub fn golden_wasm_artifacts() -> Vec<std::path::PathBuf> {
    let root = bulk_memory_golden_root();
    all_golden_wasm_artifacts()
        .into_iter()
        .filter(|path| !path.starts_with(&root))
        .collect()
}

/// The opt-in family, which the inverse gates apply to.
#[must_use]
pub fn bulk_memory_golden_artifacts() -> Vec<std::path::PathBuf> {
    let root = bulk_memory_golden_root();
    all_golden_wasm_artifacts()
        .into_iter()
        .filter(|path| path.starts_with(&root))
        .collect()
}

/// Every codegen fixture that compiles as a stand-alone file, paired with its
/// path for a failure message to name.
///
/// Multi-file fixtures keep their sources under a `src` directory and are only
/// meaningful as a tree, so they are excluded here; their merged output is
/// covered through [`golden_wasm_artifacts`].
///
/// The walk is `tests/test_data/codegen` and nothing else. The sibling
/// directories — `inf`, `panic_free`, `stellar` — are other suites' corpora and
/// are deliberately out of scope: a caller wanting one of them walks it itself
/// rather than widening this.
#[must_use]
pub fn single_file_corpus_sources() -> Vec<(String, String)> {
    fn collect(dir: &std::path::Path, found: &mut Vec<std::path::PathBuf>) {
        let entries = std::fs::read_dir(dir)
            .unwrap_or_else(|e| panic!("failed to read {}: {e}", dir.display()));
        for entry in entries {
            let path = entry.expect("failed to read a directory entry").path();
            if path.is_dir() {
                if path.file_name().is_some_and(|name| name == "src") {
                    continue;
                }
                collect(&path, found);
            } else if path.extension().is_some_and(|ext| ext == "inf") {
                found.push(path);
            }
        }
    }
    let mut paths = Vec::new();
    collect(&test_data_path().join("codegen"), &mut paths);
    paths.sort();
    paths
        .into_iter()
        .map(|path| {
            let source = std::fs::read_to_string(&path)
                .unwrap_or_else(|e| panic!("failed to read {}: {e}", path.display()));
            (path.display().to_string(), source)
        })
        .collect()
}

/// `path` as a committed list spells it: relative to `test_data`, and
/// `/`-separated so one list reads the same on every platform.
///
/// A sweep that names the fixtures it excludes has to spell them the way a
/// reviewer reads them, and two sweeps naming the same fixture differently is
/// how one list stops being comparable with the other.
///
/// # Panics
///
/// Panics if `path` is not under `test_data`.
#[must_use]
pub fn relative_to_test_data(path: &std::path::Path) -> String {
    path.strip_prefix(test_data_path())
        .unwrap_or_else(|_| panic!("{} is not under test_data", path.display()))
        .components()
        .map(|component| component.as_os_str().to_string_lossy().into_owned())
        .collect::<Vec<_>>()
        .join("/")
}

/// Whether `wasm` carries one of the compiler's custom verification operators.
///
/// Fixture names do not answer this — `nondet` and `array_nondet` are compiled
/// in compile mode with analysis skipped, and a proof-mode module need not be
/// named for it — so the classification reads the operators themselves.
///
/// It lives beside the walkers for the reason they do: more than one sweep
/// partitions the corpus on this question, and a second copy is how two sweeps'
/// notions of "carries a verification operator" drift apart.
///
/// Read with the fork rather than with the stock parser, because only the fork
/// can name these operators positively: to a stock parser they are an unknown
/// `0xfc` sub-opcode and a parse error indistinguishable from any other.
#[must_use]
pub fn carries_verification_operator(wasm: &[u8]) -> bool {
    use inf_wasmparser::{Operator, Parser, Payload};

    for payload in Parser::new(0).parse_all(wasm) {
        let Ok(Payload::CodeSectionEntry(body)) = payload else {
            continue;
        };
        let Ok(operators) = body.get_operators_reader() else {
            continue;
        };
        for operator in operators {
            let Ok(operator) = operator else { continue };
            if matches!(
                operator,
                Operator::Forall { .. }
                    | Operator::Exists { .. }
                    | Operator::Assume { .. }
                    | Operator::Unique { .. }
                    | Operator::I32Uzumaki { .. }
                    | Operator::I64Uzumaki { .. }
            ) {
                return true;
            }
        }
    }
    false
}

/// Whether `wasm` declares an import section.
///
/// The companion classification to [`carries_verification_operator`], and here
/// for the same reason: a sweep that runs a module against a runtime with no
/// host module registered has to know which artifacts have something to bind.
#[must_use]
pub fn has_import_section(wasm: &[u8]) -> bool {
    use inf_wasmparser::{Parser, Payload};

    Parser::new(0)
        .parse_all(wasm)
        .any(|payload| matches!(payload, Ok(Payload::ImportSection(_))))
}
