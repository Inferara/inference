//! SpaceWasm tier: this compiler's artifacts decoded, instantiated and executed
//! in process by the real `spacewasm` flight interpreter — the runtime the
//! `spacewasm` target is named for.
//!
//! Four things live here. `conformance_oracle` puts a hand-written module on
//! both sides of every boundary the `spacewasm` target's checker models, in
//! front of that checker and in front of the real decoder, and requires the two
//! verdicts to be equal everywhere but one class — the references the
//! interpreter narrows without checking — where the rows state the two verdicts
//! separately because they differ on purpose, and run the loaded module to
//! record which definition it actually reaches. `decode_sweep` puts the
//! committed golden corpus in front of the decoder and requires the two
//! partitions to land on opposite sides of it: every WebAssembly 1.0 golden
//! that binds no import and carries no verification operator loads, each of
//! those two classes is refused for the reason it is excluded for rather than
//! skipped, and every golden built with
//! the bulk-memory feature opted in is refused with an unsupported-opcode
//! verdict rather than merely refused — which is what makes the target's
//! post-MVP refusal a measured property of the runtime instead of a claim about
//! it. `differential` stops asking whether a module loads and asks whether it
//! *computes the same thing*: every exported zero-parameter function of every
//! import-free single-file codegen fixture is run under both `wasmtime` and
//! SpaceWasm, on the same bytes, and the two engines must agree on the value
//! and on whether the call trapped. `host_imports` is where a program this
//! compiler built runs against hosts whose answers are chosen per row: it
//! binds its imports to hosts that record every call, and follows the values
//! they answer through the program and back out. The CLI matrix runs the same
//! program only against `--host` stubs that answer zero, the oracle registers
//! a host only so that a hand-written module can load, and the sweeps register
//! none.
//!
//! The interpreter is embedded through `inference-spacewasm-runner`, which
//! defines the interpreter's allocator symbols once for every program that
//! links it, so this binary defines none. The runner is a workspace member, so
//! `cargo build` compiles it and the interpreter with it; this binary is an
//! integration test of the package that takes the runner as a dev-dependency,
//! so neither `cargo build` nor `cargo clippy` without `--all-targets` compiles
//! the binary itself.

mod conformance_oracle;
mod decode_sweep;
mod differential;
mod host_imports;
mod support;
