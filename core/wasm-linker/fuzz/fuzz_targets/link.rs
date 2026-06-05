//! libFuzzer harness over [`inference_wasm_linker::link`].
//!
//! The static-merge linker consumes the codegen-produced "main" module plus one
//! or more **resolved external `.wasm` binaries**, which under the Issue #9
//! threat model are arbitrary / third-party / adversarial bytes. The robustness
//! contract is absolute: `link` must **never** panic, hang, or out-of-memory on
//! any input — every failure is a returned [`inference_wasm_linker::LinkError`].
//!
//! This target stresses that contract directly. Each fuzzer input is split into
//! a main module and a sequence of externals; the harness feeds them to `link`
//! and lets libFuzzer treat any panic / abort as a crash. It additionally
//! asserts the *soundness* half of the contract — when `link` returns `Ok`, the
//! merged bytes must pass the in-tree WASM validator, so a silently-invalid
//! merged artifact (the worst-case outcome for a verification toolchain) is also
//! a fuzzer crash rather than a persisted bad module.
//!
//! ## Running
//!
//! `cargo-fuzz` and a nightly toolchain are required and are intentionally *not*
//! part of the default workspace (this crate declares its own `[workspace]`):
//!
//! ```text
//! cargo install cargo-fuzz
//! cargo +nightly fuzz run link
//! ```
//!
//! ## Seed corpus
//!
//! A committed seed corpus of the audit reproductions (the round-2
//! control-flow-join / param-nulling / call-laundering / memory64 / deep-nesting
//! / over-declared-locals / main-data-segment cases, plus a positive control)
//! lives at `core/wasm-linker/fuzz/seeds/link/`. Start the fuzzer from it for
//! fast coverage:
//!
//! ```text
//! cargo +nightly fuzz run link core/wasm-linker/fuzz/seeds/link
//! ```
//!
//! Those seeds are reproducible: `regenerate_fuzz_seeds` in
//! `core/wasm-linker/tests/fuzz_seeds.rs` rebuilds them from source, and
//! `committed_fuzz_seeds_reach_link_cleanly` (run on every stable `cargo test`)
//! replays each through this target's exact `split` + module rotation, asserting
//! the same panic-free / `Ok ⇒ valid` invariant and that each round-2 seed still
//! reaches its intended rejection. The broader property test
//! `adversarial_corpus_never_panics_and_only_emits_valid_modules` in
//! `core/wasm-linker/tests/link.rs` carries the same reproductions inline, so the
//! seam is exercised even where `cargo-fuzz` is unavailable.

#![no_main]

use libfuzzer_sys::fuzz_target;

/// Splits the fuzzer-supplied bytes into a main module and a list of externals.
///
/// The wire format is deliberately simple so the structured-aware fuzzer can
/// reach the linker quickly: a leading byte `n` (clamped to `0..=4`) is the
/// external count, then `n` length-prefixed (`u16` little-endian) external
/// blobs, then the remainder as the main module. Truncated inputs degrade
/// gracefully — a short length prefix just yields the rest of the buffer.
fn split(data: &[u8]) -> (Vec<u8>, Vec<Vec<u8>>) {
    let Some((&count_byte, rest)) = data.split_first() else {
        return (Vec::new(), Vec::new());
    };
    let count = (count_byte % 5) as usize;
    let mut externals = Vec::with_capacity(count);
    let mut cursor = rest;
    for _ in 0..count {
        if cursor.len() < 2 {
            break;
        }
        let len = u16::from_le_bytes([cursor[0], cursor[1]]) as usize;
        cursor = &cursor[2..];
        let take = len.min(cursor.len());
        externals.push(cursor[..take].to_vec());
        cursor = &cursor[take..];
    }
    (cursor.to_vec(), externals)
}

fuzz_target!(|data: &[u8]| {
    let (main, externals) = split(data);

    // Match every external against each of a small set of plausible logical
    // module names. Codegen records imports as `(module, field)`, so resolution
    // keys on the module; exercising several names probes the binding path
    // (C4 / AmbiguousImport) rather than only the all-empty-name case.
    let module_names = ["", "mathlib", "crypto::sha256", "a"];
    let pairs: Vec<(&str, &[u8])> = externals
        .iter()
        .enumerate()
        .map(|(i, bytes)| (module_names[i % module_names.len()], bytes.as_slice()))
        .collect();

    match inference_wasm_linker::link(&main, &pairs) {
        // A returned error is the contractually-correct outcome for malformed or
        // unsupported input. Nothing more to check.
        Err(_) => {}
        // A successful merge must be a structurally valid module. A silently
        // invalid merged artifact is the worst-case failure for the verification
        // pipeline, so treat it as a fuzzer crash.
        Ok(merged) => {
            inf_wasmparser::validate(&merged)
                .expect("link returned Ok but the merged module fails WASM validation");
        }
    }
});
