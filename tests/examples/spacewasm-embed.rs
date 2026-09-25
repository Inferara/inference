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
//! A module binding host imports loads once each import has a stub. For an F´
//! program importing the reference host set — `panic`, `rsleep`, `command`,
//! `message` and `telemetry` from `fprime_core` and `clock_ms` from `env`:
//!
//! ```text
//! cargo run -p inference-tests --example spacewasm-embed -- out/main.wasm \
//!     --host fprime_core.panic=iii --host fprime_core.rsleep=I \
//!     --host fprime_core.command=ii:i --host fprime_core.message=ii \
//!     --host fprime_core.telemetry=iiiii:i --host env.clock_ms=:I \
//!     --invoke report 3
//! ```
//!
//! Each stub answers zero and prints the calls it receives; what a stub is and
//! is not is set out in the shared support module.
//!
//! An **example** rather than a `src/bin` target, because the runner it loads
//! through, `inference-spacewasm-runner`, is a dev-dependency of this package
//! and only a test, an example or a bench sees one. The harness itself lives in
//! the SpaceWasm tier's shared support module, which this file includes by path
//! and which the CLI matrix drives in process, so this file is only the binary
//! root. It declares no interpreter allocator: the runner defines it once for
//! every program that links it.

#[path = "../tests/spacewasm/support.rs"]
mod support;

fn main() -> std::process::ExitCode {
    support::run(std::env::args().skip(1).collect())
}
