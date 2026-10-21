# `inference` fuzz targets

Coverage-guided fuzzing over the compiler, driven through the library calls
`infc` and the language server make. A source file is untrusted input: the
compiler must handle any of it without panicking (CONTRIBUTING.md), and what it
emits is what users deploy.

This crate is **detached from the main Inference workspace** (it declares its own
`[workspace]` table), like `core/wasm-linker/fuzz`, because `cargo-fuzz` and a
nightly toolchain are not part of the default build. `cargo build` / `cargo test`
at the repo root never touch it.

## Targets

- **`front_end`**: parses arbitrary UTF-8 source, type-checks the arena the
  resilient parser built even when it has syntax errors, and runs every analysis
  rule on whatever the type checker recovered, rendering each diagnostic. This is
  the path the language server takes for every file an editor opens, and it
  reaches every phase `infc` runs before code generation. A panic, abort, stack
  overflow, timeout or out-of-memory is a crash.
- **`compile`**: compiles a source that passes analysis the way `infc` does, with
  no external modules, at five builds: `wasm32` in compile mode under both
  bounds-check policies, `wasm32` in proof mode, `stellar` and `spacewasm`.
  Besides never panicking, every executable module must validate as WebAssembly
  1.0 under the upstream `wasmparser`, and the proof-mode module must validate
  under `inf-wasmparser`; its Rocq translation runs too. A refusal from any phase
  ends that build and is not a crash.

Both run the phases on `inference::with_compiler_stack`, the stack `infc` gives
the compiler, so a deeply nested input does not overflow the fuzzer's own
smaller thread.

## Running

From the repository root, with a nightly toolchain and `cargo-fuzz`:

```sh
cargo install cargo-fuzz
cargo +nightly fuzz run --fuzz-dir core/inference/fuzz front_end
cargo +nightly fuzz run --fuzz-dir core/inference/fuzz compile
```

To start from the language corpus, pass a corpus directory the fuzzer may write
to, then `tests/test_data` as a read-only seed directory, and the token
dictionary:

```sh
mkdir -p ~/.cache/inference-fuzz/front_end
cargo +nightly fuzz run --fuzz-dir core/inference/fuzz front_end \
    ~/.cache/inference-fuzz/front_end tests/test_data \
    -- -dict=core/inference/fuzz/inf.dict -max_len=16384
```

`compile` takes the same arguments. A crash is written to
`core/inference/fuzz/artifacts/<target>/`; rerun one input with
`cargo +nightly fuzz run --fuzz-dir core/inference/fuzz <target> <file>`.

## Continuous fuzzing

ClusterFuzzLite builds these targets with `.clusterfuzzlite/build.sh` and runs
them on pull requests (`.github/workflows/cflite_pr.yml`) and every six hours on
`main` (`cflite_batch.yml`). Their seed corpus is every `.inf` file under
`tests/test_data`, and their dictionary is `inf.dict`.
