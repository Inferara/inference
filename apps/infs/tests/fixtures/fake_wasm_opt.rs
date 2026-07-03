//! Dependency-free fake `wasm-opt` used by the `infs` CLI integration tests.
//!
//! It stands in for the real Binaryen `wasm-opt` so the hermetic post-build
//! optimization tests run on machines without Binaryen installed. The harness
//! compiles this file once per test process with `rustc` and points `infs` at
//! the result through the `WASM_OPT_PATH` environment variable. Every role the
//! tests need is selected by environment variable so a single binary suffices:
//!
//! - `--version` among the arguments: print `FAKE_WASM_OPT_VERSION` (default
//!   `wasm-opt version 118 (fake)`) to stdout and exit 0. This branch runs
//!   before any logging, so the version probe `infs` performs is never recorded
//!   in the invocation log.
//! - `FAKE_WASM_OPT_LOG`: append this invocation's arguments (one per line,
//!   after a marker line) to the named file, letting a test assert the exact
//!   argument vector `infs` forwarded.
//! - `FAKE_WASM_OPT_EXIT`: when set to a nonzero integer, print `fake failure`
//!   to stderr and exit with that code (the optimizer-failed path).
//! - `FAKE_WASM_OPT_GARBAGE=1`: write non-wasm bytes to the `-o` target instead
//!   of a valid module (the re-validation-failed path).
//! - otherwise: copy the positional input file to the `-o` target byte-for-byte
//!   and exit 0 (the success path).
//!
//! It lives under `tests/fixtures/` (not `tests/`) so Cargo does not compile it
//! as an integration-test target; the harness builds it explicitly.

use std::io::Write;

/// Marker written before each logged invocation so a test can count invocations
/// and isolate a single invocation's argument lines.
const INVOCATION_MARKER: &str = "--- wasm-opt invocation ---";

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();

    if args.iter().any(|arg| arg == "--version") {
        let version = std::env::var("FAKE_WASM_OPT_VERSION")
            .unwrap_or_else(|_| String::from("wasm-opt version 118 (fake)"));
        println!("{version}");
        return;
    }

    if let Ok(log_path) = std::env::var("FAKE_WASM_OPT_LOG") {
        log_invocation(&log_path, &args);
    }

    if let Ok(raw) = std::env::var("FAKE_WASM_OPT_EXIT") {
        if let Ok(code) = raw.parse::<i32>() {
            if code != 0 {
                eprintln!("fake failure");
                std::process::exit(code);
            }
        }
    }

    let (input, output) = parse_io(&args);
    let output = output.expect("fake wasm-opt: no `-o <output>` argument was provided");

    if std::env::var("FAKE_WASM_OPT_GARBAGE").as_deref() == Ok("1") {
        std::fs::write(&output, b"not a valid wasm module")
            .expect("fake wasm-opt: failed to write garbage output");
        return;
    }

    let input = input.expect("fake wasm-opt: no positional input file was provided");
    let bytes = std::fs::read(&input).expect("fake wasm-opt: failed to read input");
    std::fs::write(&output, &bytes).expect("fake wasm-opt: failed to write output");
}

/// Appends one invocation (a marker line followed by each argument on its own
/// line) to the log file named by `FAKE_WASM_OPT_LOG`.
fn log_invocation(log_path: &str, args: &[String]) {
    let mut entry = String::from(INVOCATION_MARKER);
    entry.push('\n');
    for arg in args {
        entry.push_str(arg);
        entry.push('\n');
    }
    let mut file = std::fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(log_path)
        .expect("fake wasm-opt: failed to open FAKE_WASM_OPT_LOG");
    file.write_all(entry.as_bytes())
        .expect("fake wasm-opt: failed to append to FAKE_WASM_OPT_LOG");
}

/// Extracts the positional input (first non-flag argument) and the `-o` target
/// from a `wasm-opt`-style argument vector.
fn parse_io(args: &[String]) -> (Option<String>, Option<String>) {
    let mut input = None;
    let mut output = None;
    let mut i = 0;
    while i < args.len() {
        let arg = &args[i];
        if arg == "-o" {
            output = args.get(i + 1).cloned();
            i += 2;
            continue;
        }
        if !arg.starts_with('-') && input.is_none() {
            input = Some(arg.clone());
        }
        i += 1;
    }
    (input, output)
}
