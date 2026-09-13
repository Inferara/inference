//! The public face of this crate's compile helpers.
//!
//! Everything the helpers do lives in `utils`, which is `pub(crate)`: it is
//! reachable from the unit tests compiled inside this library and invisible to
//! an integration binary under `tests/`, because an integration binary is a
//! separate crate that sees only what this one exports. The Stellar host tier
//! is such a binary — it needs a real host, so it cannot live in `src/` — and
//! this module is the seam that lets it compile Inference source at all.
//!
//! Nothing here re-implements a pipeline. Each function is one call into the
//! same `utils` helper the in-library tests use, so a fixture compiled through
//! this module and the same fixture compiled inside `src/` cannot diverge.
//! [`compile_for_stellar`] is the one addition: the steps `infc` runs *after*
//! code generation, which no in-library test needed before.

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
