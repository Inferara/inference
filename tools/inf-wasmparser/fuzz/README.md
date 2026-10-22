# `inf-wasmparser` fuzz target

Coverage-guided fuzzing over `inf-wasmparser`, the fork of `wasmparser` that
decodes Inference's custom `0xfc` opcodes. An external module linked with
`use … from` or `--wasm-dep` is a third-party binary, and this crate is what
reads it, so parsing and validating must never panic, hang or run out of memory
on any bytes.

This crate is **detached from the main Inference workspace** (it declares its own
`[workspace]` table), like `core/wasm-linker/fuzz`, because `cargo-fuzz` and a
nightly toolchain are not part of the default build. `cargo build` / `cargo test`
at the repo root never touch it, and `cargo package` leaves it out of the
published crate.

## Targets

- **`validate`**: validates each input at the fork's default feature set, at the
  WebAssembly 1.0 level the linker holds externals to, and with every feature
  enabled. Because validation stops at the first error, it also walks the input
  with the plain parser, decoding every function body's locals and operators and
  the function, local and global names of a `name` section. A panic, abort,
  timeout or out-of-memory is a crash.

## Running

From the repository root, with a nightly toolchain and `cargo-fuzz`:

```sh
cargo install cargo-fuzz
cargo +nightly fuzz run --fuzz-dir tools/inf-wasmparser/fuzz validate
```

To start from the compiler's `.wasm` goldens, pass a corpus directory the fuzzer
may write to, then `tests/test_data` as a read-only seed directory:

```sh
mkdir -p ~/.cache/inference-fuzz/validate
cargo +nightly fuzz run --fuzz-dir tools/inf-wasmparser/fuzz validate \
    ~/.cache/inference-fuzz/validate tests/test_data -- -max_len=8192
```

A crash is written to `tools/inf-wasmparser/fuzz/artifacts/validate/`; rerun one
input with `cargo +nightly fuzz run --fuzz-dir tools/inf-wasmparser/fuzz validate <file>`.

## Continuous fuzzing

ClusterFuzzLite builds this target with `.clusterfuzzlite/build.sh` and runs it
on pull requests (`.github/workflows/cflite_pr.yml`) and every six hours on
`main` (`cflite_batch.yml`). Its seed corpus is every `.wasm` file under
`tests/test_data`.
