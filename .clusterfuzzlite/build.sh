#!/bin/bash -eu
#
# Builds every cargo-fuzz target in the repository into $OUT, each with a seed
# corpus, for ClusterFuzzLite. It runs in the image .clusterfuzzlite/Dockerfile
# builds, from the repository root.
#
# Each fuzz crate is its own workspace, detached from the main one, so each is
# built from the crate it belongs to: `cargo fuzz` finds `fuzz/` there, and so
# does the `cargo` wrapper of OSS-Fuzz's base image, which turns the build into
# a plain `cargo build` for a coverage build. Every target in a crate's
# `fuzz_targets/` is copied out, so a new target needs no change to the build
# loop; it gets its seeds from a `seed_corpus` line below.

# Cargo.lock is not tracked, so every build resolves the dependencies afresh.
# Prefer the newest release each dependency has that the image's nightly can
# compile, so a crate that raises its minimum Rust version does not break the
# fuzz build before the image's toolchain catches up.
export CARGO_RESOLVER_INCOMPATIBLE_RUST_VERSIONS=fallback

for crate in core/inference core/wasm-linker tools/inf-wasmparser; do
  (
    cd "$crate"
    cargo fuzz build -O --debug-assertions
    for source in fuzz/fuzz_targets/*.rs; do
      target=$(basename "${source%.rs}")
      cp "fuzz/target/x86_64-unknown-linux-gnu/release/$target" "$OUT/"
    done
  )
done

# Zips every file under the given roots whose name matches the pattern into
# $OUT/<target>_seed_corpus.zip, which ClusterFuzzLite unpacks into a target's
# corpus while that corpus is empty. Each file is stored under its content hash:
# the corpora hold many files of one name in different directories, and two
# identical files are one seed.
seed_corpus() {
  local target=$1 pattern=$2
  shift 2
  local dir="$WORK/seeds/$target"
  mkdir -p "$dir"
  find "$@" -type f -name "$pattern" -print0 |
    while IFS= read -r -d '' file; do
      cp "$file" "$dir/$(sha1sum "$file" | cut -d' ' -f1)"
    done
  rm -f "$OUT/${target}_seed_corpus.zip"
  zip -q -j -r "$OUT/${target}_seed_corpus.zip" "$dir"
}

seed_corpus front_end '*.inf' tests/test_data
seed_corpus compile '*.inf' tests/test_data
seed_corpus validate '*.wasm' tests/test_data
seed_corpus link '*' core/wasm-linker/fuzz/seeds/link
seed_corpus link_memory '*' core/wasm-linker/fuzz/seeds/link_memory

# The source-level targets mutate with the language's tokens.
cp core/inference/fuzz/inf.dict "$OUT/front_end.dict"
cp core/inference/fuzz/inf.dict "$OUT/compile.dict"
