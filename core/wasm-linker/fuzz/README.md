# `inference-wasm-linker` fuzz targets

Coverage-guided fuzzing over the static-merge linker's public entry point,
[`inference_wasm_linker::link`]. Under the Issue #9 threat model the external
`.wasm` bytes handed to `link` are arbitrary / third-party / adversarial, so the
linker must never panic, hang, or out-of-memory on any input, and a successful
merge must always produce a structurally valid module.

This crate is **detached from the main Inference workspace** (it declares its own
`[workspace]` table) because `cargo-fuzz` and a nightly toolchain are not part of
the default build. `cargo build` / `cargo test` at the repo root never touch it.

## Targets

- **`link`** — splits each input into a main module plus a list of externals and
  feeds them to `link`. A panic/abort is a crash; an `Ok` whose merged bytes fail
  `inf_wasmparser::validate` is also a crash (a silently-invalid merged artifact
  is the worst-case outcome for the verification pipeline).

## Running

```sh
cargo install cargo-fuzz
cargo +nightly fuzz run link core/wasm-linker/fuzz/seeds/link
```

## Seed corpus

`seeds/link/` holds a committed seed corpus of the audit reproductions — the
round-2 control-flow-join (C-1), param-nulling-arithmetic (C-2), call-laundering
(C-3), memory64 (C-4), deep-nesting (H-3), over-declared-locals (M-1), and
main-data-segment (M-2) cases, plus a positive control that must merge. Each seed
imports from the empty module `""` so the target's first-external binding
satisfies it and the seed reaches the real closure / provenance / merge logic.

The seeds are reproducible and continuously verified by two tests in
`core/wasm-linker/tests/fuzz_seeds.rs`:

- `regenerate_fuzz_seeds` (`#[ignore]`d) rebuilds the corpus from source —
  `cargo test -p inference-wasm-linker --test fuzz_seeds regenerate -- --ignored`;
- `committed_fuzz_seeds_reach_link_cleanly` (runs on every `cargo test`) replays
  each committed seed through this target's exact wire-format `split` and module
  rotation, asserting it never panics, never yields a silently-invalid `Ok`, and
  that each round-2 seed still reaches the specific guard it was built to exercise.

The same reproductions are also carried inline by the property test
`adversarial_corpus_never_panics_and_only_emits_valid_modules` in
`core/wasm-linker/tests/link.rs`, so the seam runs under stable `cargo test` even
without `cargo-fuzz`.

## Relationship to the regression suite

The fuzzer is the *generative* guard; the integration tests in
`core/wasm-linker/tests/link.rs` are the *deterministic* guard. Every confirmed
robustness-audit issue (round-1 C1–C4 / H1–H26 / L1–L2 and round-2 C-1–C-4 /
H-1–H-4 / M-1–M-2 / L-1) has a hand-written regression test asserting a clean
outcome; the fuzzer exists to surface the *next* such defect before it ships.
