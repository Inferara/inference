//! SpaceWasm tier: this compiler's artifacts decoded, instantiated and executed
//! in process by the real `spacewasm` flight interpreter — the runtime the
//! `spacewasm` target is named for.
//!
//! Three things live here. `conformance_oracle` puts a hand-written module on
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
//! and on whether the call trapped.
//!
//! The interpreter's allocator is a pair of `no_mangle` symbols the library
//! resolves its internal allocations to, so every binary that links it declares
//! them itself. That is what the invocation below is; `support` deliberately
//! never invokes the macro, so including it elsewhere stays possible.
//!
//! The interpreter is a dev-dependency and this is an integration binary, so
//! neither `cargo build` nor `cargo clippy` without `--all-targets` compiles any
//! of it.

use support::StdAllocator;

spacewasm::global_allocator!(StdAllocator, StdAllocator);

mod conformance_oracle;
mod decode_sweep;
mod differential;
mod support;
