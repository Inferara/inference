# `inference-wasm-linker` fuzz targets

Coverage-guided fuzzing over the static-merge linker's public entry point,
[`inference_wasm_linker::link`]. Under the Issue #9 threat model the external
`.wasm` bytes handed to `link` are arbitrary / third-party / adversarial, so the
linker must never panic, hang, or out-of-memory on any input, and a successful
merge must always produce a structurally valid module. The second target,
`link_memory`, also checks what a successful link produces: the linked memory
and the warnings, which must not depend on import order.

This crate is **detached from the main Inference workspace** (it declares its own
`[workspace]` table) because `cargo-fuzz` and a nightly toolchain are not part of
the default build. `cargo build` / `cargo test` at the repo root never touch it.

## Targets

- **`link`** — splits each input into a main module plus a list of externals and
  feeds them to `link`. A panic/abort is a crash; an `Ok` whose merged bytes fail
  `inf_wasmparser::validate` is also a crash (a silently-invalid merged artifact
  is the worst-case outcome for the verification pipeline).
- **`link_memory`** — structured: decodes each input into a main module, with
  or without a memory, plus one to four providers, builds the modules, and links
  them in three import orders. Every order must produce the same linked memory,
  or the same error text, and the same warnings, and the outcome must agree
  with an oracle written from the documented memory rule (the linker README's
  "Linear-Memory Reconciliation", and the book's "The Shared Linear Memory").
  #388, where swapping two `external fn` declarations changed what
  `memory.grow` returned, is the bug it targets. The generator, the oracle and
  the checks live in `core/wasm-linker/fuzz/src/link_memory.rs`, which
  documents the input format and the properties.

## Running

From the repository root, with a nightly toolchain and `cargo-fuzz`:

```sh
cargo install cargo-fuzz
mkdir -p ~/.cache/inference-fuzz/link ~/.cache/inference-fuzz/link_memory
cargo +nightly fuzz run --fuzz-dir core/wasm-linker/fuzz link \
    ~/.cache/inference-fuzz/link core/wasm-linker/fuzz/seeds/link
cargo +nightly fuzz run --fuzz-dir core/wasm-linker/fuzz link_memory \
    ~/.cache/inference-fuzz/link_memory core/wasm-linker/fuzz/seeds/link_memory
```

The first directory is the corpus the fuzzer writes to; the committed seeds
after it are only read.

A `link_memory` crash prints the violated property, the spec, and each run's
import order and outcome. `cargo fuzz fmt` prints the spec of a saved crash
input in its `Debug` form, which the seed table's `LinkSpec` literals follow:

```sh
cargo +nightly fuzz fmt --fuzz-dir core/wasm-linker/fuzz link_memory <crash-file>
```

ClusterFuzzLite builds both targets with `.clusterfuzzlite/build.sh` and runs
them on pull requests (`.github/workflows/cflite_pr.yml`) and every six hours on
`main` (`cflite_batch.yml`), seeding `link` from `seeds/link/` and
`link_memory` from `seeds/link_memory/`.

## Seed corpus

`seeds/link/` holds a committed seed corpus of the audit reproductions — the
round-2 control-flow-join (C-1), param-nulling-arithmetic (C-2), call-laundering
(C-3), memory64 (C-4), deep-nesting (H-3), over-declared-locals (M-1), and
main-passive-data-segment (M-2b) cases, plus two positive controls that must
merge: a pure external, and a main carrying an active data segment (M-2), which
the merge once dropped and now re-emits. Each seed imports from the empty module
`""` so the target's first-external binding satisfies it and the seed reaches the
real closure / provenance / merge logic.

The seeds are reproducible and continuously verified by two tests in
`core/wasm-linker/tests/fuzz_seeds.rs`:

- `regenerate_fuzz_seeds` (`#[ignore]`d) rebuilds the corpus from source —
  `cargo test -p inference-wasm-linker --test fuzz_seeds regenerate -- --ignored`;
- `committed_fuzz_seeds_reach_link_cleanly` (runs on every `cargo test`) replays
  each committed seed through the `link` target's exact wire-format `split` and
  module rotation, asserting it never panics, never yields a silently-invalid
  `Ok`, and that each round-2 seed still reaches the specific guard it was built
  to exercise.

The same reproductions are also carried inline by the property test
`adversarial_corpus_never_panics_and_only_emits_valid_modules` in
`core/wasm-linker/tests/link.rs`, so the seam runs under stable `cargo test` even
without `cargo-fuzz`.

`seeds/link_memory/` holds one file per seed of the seed table in
`core/wasm-linker/tests/link_memory_seeds.rs`: the 57-byte `encode` of that
seed's `LinkSpec` literal. The table pins each seed's outcome by hand, from the
documented memory rule, and `every_seed_reaches_its_pinned_outcome` links every
seed through the `link_memory` target's checks and compares the result with its
pin. Two more tests in that file keep the files reproducible:

- `committed_link_memory_seeds_are_current` (runs on every `cargo test`)
  requires each seed's file to equal the `encode` of its spec and to decode
  back to it, and the directory to hold no other entry;
- `regenerate_link_memory_seeds` (`#[ignore]`d) rewrites the corpus from the
  table and removes any other file —
  `cargo test -p inference-wasm-linker --test link_memory_seeds regenerate -- --ignored`.

A new seed, such as a minimized crash, is added to the table as a `LinkSpec`
literal with its pinned outcome, and the corpus is then regenerated.

## Relationship to the regression suite

The fuzzer is the *generative* guard; the integration tests in
`core/wasm-linker/tests/link.rs` are the *deterministic* guard. Every confirmed
robustness-audit issue (round-1 C1–C4 / H1–H26 / L1–L2 and round-2 C-1–C-4 /
H-1–H-4 / M-1–M-2 / L-1) has a hand-written regression test asserting a clean
outcome; the fuzzer exists to surface the *next* such defect before it ships.
