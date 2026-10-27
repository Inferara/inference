//! libFuzzer harness over the whole compile pipeline, on arbitrary UTF-8
//! source.
//!
//! The output of the compiler is a security boundary: a user deploys the Wasm
//! and trusts the proofs. A source that passes analysis is therefore built the
//! way `infc` builds it (`run` in `core/cli/src/main.rs`), with no external
//! modules: analysis for the target, code generation with the bounds proofs
//! that analysis produced, the link step, and the target's own post-processing.
//! The program is built once for each entry of [`BUILDS`].
//!
//! The contract checked on top of "never panics":
//!
//! - an executable module `infc` would write is WebAssembly 1.0, as judged by
//!   the unmodified upstream `wasmparser`. That validator knows nothing about
//!   this project, so its acceptance means the artifact is genuine Wasm.
//! - a proof-mode module validates under `inf-wasmparser`, the fork that knows
//!   the custom non-deterministic opcodes.
//!
//! An `Err` from any phase is a refusal `infc` would print and exit on, so it
//! ends that build and is not a crash. What this target cannot see is a module
//! that is valid but computes the wrong thing. The cross-compiler tests
//! (`tests/src/codegen/wasm/algo_iter.rs`) look for those.
//!
//! ## Running
//!
//! From the repository root:
//!
//! ```text
//! cargo +nightly fuzz run --fuzz-dir core/inference/fuzz compile
//! ```
//!
//! `core/inference/fuzz/README.md` shows how to start from the seed corpus.

#![no_main]

use inference::wasm_link::ResolvedExternals;
use inference::{AnalysisOptions, BoundsChecks, LinkOptions, TargetName, TypedContext};
use inference_wasm_codegen::BoundsChecks::{All, OmitProven};
use inference_wasm_codegen::CompilationMode::{Compile, Proof};
use inference_wasm_codegen::{CodegenOptions, CompilationMode, EmitFeatures, MemoryLayout, Target};
use libfuzzer_sys::fuzz_target;

/// The module name every build is given. `infc` uses the source file's stem;
/// this one is a valid Rocq identifier that the generated `.v` does not
/// reserve, so the proof build reaches the translation.
const MODULE_NAME: &str = "fuzz";

/// The builds each program is put through: a target, a mode, and a
/// bounds-check policy.
///
/// `stellar` and `spacewasm` are built in compile mode only, because `infc`
/// refuses a proof artifact for either. The `omit-proven` build is the only
/// one whose code generation reads the bounds proofs analysis hands it.
const BUILDS: [(TargetName, CompilationMode, BoundsChecks); 5] = [
    (TargetName::Wasm32, Compile, All),
    (TargetName::Wasm32, Compile, OmitProven),
    (TargetName::Wasm32, Proof, All),
    (TargetName::Stellar, Compile, All),
    (TargetName::SpaceWasm, Compile, All),
];

fuzz_target!(|data: &[u8]| {
    let Ok(source) = std::str::from_utf8(data) else {
        return;
    };
    inference::with_compiler_stack(|| {
        let Ok(arena) = inference::parse(source) else {
            return;
        };
        let Ok(typed_context) = inference::type_check(arena) else {
            return;
        };
        for build in BUILDS {
            build_one(&typed_context, build);
        }
    });
});

/// Builds `typed_context` the way `infc` would for one entry of [`BUILDS`],
/// and checks the module that build would write.
fn build_one(
    typed_context: &TypedContext,
    (target_name, mode, bounds_checks): (TargetName, CompilationMode, BoundsChecks),
) {
    let target = emission_target(target_name);
    let layout = MemoryLayout::default();
    let analysis_options = AnalysisOptions {
        layout,
        target: target_name,
        bounds_checks,
    };
    let Ok(analysis) = inference::analyze_with_options(typed_context, analysis_options) else {
        return;
    };
    let codegen_options = CodegenOptions {
        target,
        mode,
        opt_level: target.default_opt_level(),
        features: EmitFeatures::default(),
        layout,
        bounds_checks,
    };
    let Ok(output) = inference_wasm_codegen::codegen_with_proven_in_bounds(
        typed_context,
        MODULE_NAME,
        codegen_options,
        analysis.proven_in_bounds().accesses(),
    ) else {
        return;
    };
    let Ok(linked) = inference::link_resolved(
        output.wasm(),
        &ResolvedExternals::default(),
        layout.memory_import(),
        &LinkOptions::default(),
    ) else {
        return;
    };

    if mode == Proof {
        if let Err(e) = inf_wasmparser::validate(&linked.wasm) {
            panic!("a proof-mode module fails inf-wasmparser validation: {e}");
        }
        let _ = inference::wasm_to_v(
            MODULE_NAME,
            &linked.wasm,
            output.spec_func_indices_by_spec(),
            output.hspecs(),
        );
        return;
    }

    // Code generation's own output is held to WebAssembly 1.0 for every target,
    // before the target's post-processing: an invalid module here is a code
    // generation bug even when a later check would have refused to ship it.
    validate_wasm1(&linked.wasm, target);
    match target {
        Target::Stellar => {
            if let Ok(contract) = inference_stellar_abi::rewrite(
                &linked.wasm,
                output.export_signatures(),
                inference_stellar_abi::STELLAR_ENV_PROTOCOL,
            ) {
                validate_wasm1(&contract, target);
            }
        }
        Target::SpaceWasm => {
            let _ = inference_target_conformance::spacewasm::check(&linked.wasm);
        }
        _ => {}
    }
}

/// The code generation target `infc` builds for `name` (`emission_target` in
/// `core/cli/src/main.rs`).
fn emission_target(name: TargetName) -> Target {
    match name {
        TargetName::Wasm32 => Target::Wasm32,
        TargetName::Stellar => Target::Stellar,
        TargetName::SpaceWasm => Target::SpaceWasm,
    }
}

/// Panics unless `wasm` validates at the WebAssembly 1.0 feature level (the MVP
/// plus mutable globals) under the upstream `wasmparser`.
fn validate_wasm1(wasm: &[u8], target: Target) {
    let features = wasmparser::WasmFeatures::WASM1;
    if let Err(e) = wasmparser::Validator::new_with_features(features).validate_all(wasm) {
        panic!("a {target:?} compile-mode module is not valid WebAssembly 1.0: {e}");
    }
}
