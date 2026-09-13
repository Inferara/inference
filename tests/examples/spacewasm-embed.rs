//! A minimal embedder for this compiler's SpaceWasm artifacts.
//!
//! `spacewasm` is a `no_std` flight interpreter with no command-line front end
//! of its own beyond upstream's demonstration binary, so a developer asking
//! "does the module I just built load, and what does it do when I call it?" has
//! nowhere to ask. This is that place: it loads an artifact, calls an export,
//! reports a trap or an exhausted fuel budget as its own exit code, and
//! measures the IR the module compiled to.
//!
//! ```text
//! cargo run -p inference-tests --example spacewasm-embed -- out/main.wasm --invoke main
//! ```
//!
//! An **example** rather than a `src/bin` target, because `spacewasm` is a
//! dev-dependency of this package and only a test, an example or a bench sees
//! one. The harness itself lives in the SpaceWasm tier's shared support module,
//! which this file includes by path and which the CLI matrix drives in process;
//! everything here is the binary root the interpreter's allocator singleton has
//! to be declared at.

#[path = "../tests/spacewasm/support.rs"]
mod support;

spacewasm::global_allocator!(support::StdAllocator, support::StdAllocator);

fn main() -> std::process::ExitCode {
    support::run(std::env::args().skip(1).collect())
}
