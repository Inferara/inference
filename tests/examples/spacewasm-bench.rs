//! The SpaceWasm benchmark: what this compiler's output costs the flight
//! interpreter, measured on the programs the SpaceWasm test tier runs, and
//! compared with the history the repository keeps of it.
//!
//! ```text
//! cargo run -p inference-tests --example spacewasm-bench -- compare
//! cargo run -p inference-tests --example spacewasm-bench -- record
//! ```
//!
//! `compare` measures every program and warns about each figure that grew
//! past a threshold since the history's last measurement of it, and about
//! each program it could not measure, and exits 0 whatever it finds; CI runs
//! it on every pull request. `record` measures every program and appends the
//! snapshot to `tests/bench/spacewasm/history.jsonl`, or appends nothing and
//! fails when a program could not be measured, and is run by hand before a
//! release. What is measured, the history's keys and the comparison's rules
//! are set out in the benchmark module.
//!
//! An **example** for the reason the `spacewasm-embed` harness is one: the
//! runner and the interpreter are dev-dependencies of this package, which
//! only a test, an example or a bench sees. The benchmark lives in the
//! SpaceWasm tier's directory, beside the support module it loads through
//! and the F´ programs it shares with the tier, so this file is only the
//! binary root, and it declares no interpreter allocator: the runner defines
//! it once for every program that links it.

#[path = "../tests/spacewasm/bench.rs"]
mod bench;
#[path = "../tests/spacewasm/fprime_programs.rs"]
mod fprime_programs;
#[path = "../tests/spacewasm/support.rs"]
mod support;

fn main() -> std::process::ExitCode {
    bench::run(&std::env::args().skip(1).collect::<Vec<_>>())
}
