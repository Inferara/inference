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
//! - `FAKE_WASM_OPT_OVERSIZED_PARAMS=1`: write a module that is valid
//!   WebAssembly 1.0 and outside a narrowed target's envelope — one function of
//!   256 i32 parameters, one word past what a SpaceWasm embedder's parameter
//!   field holds. This is the only way to reach the post-optimization
//!   conformance check from a test: the compiler cannot produce such a module,
//!   and an optimizer that produced one is exactly what the check is there for.
//! - `FAKE_WASM_OPT_DEEP_NESTING=1`: write a module that is conformant and past
//!   the reference embedder's control-frame budget — one function nesting 65
//!   empty blocks. Conformance and budget are different questions, and this is
//!   the role that separates them: the module loads under an embedder built
//!   with a larger const generic, so the build succeeds and owes a warning
//!   rather than a refusal.
//! - `FAKE_WASM_OPT_FUNCTION_IMPORT=1`: write a module whose only content is
//!   one function import, `env.f`. For any program that does not bind `env.f`
//!   itself, this is the role that reaches the refusal of optimized bytes
//!   importing a function the compiler's module did not.
//! - `FAKE_WASM_OPT_NO_IMPORTS=1`: write a module that imports nothing and
//!   exports a `main` returning 0 — what Binaryen makes of a program whose only
//!   host import is never called. It is the role that shows the build log
//!   reporting the removal, and `infs run` executing a program the compiler's
//!   module would have been refused for.
//! - `FAKE_WASM_OPT_VERIFICATION_CONSTRUCT=1`: write a valid module whose one
//!   function carries `i32.uzumaki`. The validator the optimized bytes are
//!   re-checked with decodes the verification opcodes, so this is the role that
//!   reaches the scan of the bytes about to land.
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

    if std::env::var("FAKE_WASM_OPT_OVERSIZED_PARAMS").as_deref() == Ok("1") {
        std::fs::write(&output, oversized_parameter_module())
            .expect("fake wasm-opt: failed to write the oversized-parameter module");
        return;
    }

    if std::env::var("FAKE_WASM_OPT_DEEP_NESTING").as_deref() == Ok("1") {
        std::fs::write(&output, deeply_nested_module())
            .expect("fake wasm-opt: failed to write the deeply nested module");
        return;
    }

    if std::env::var("FAKE_WASM_OPT_FUNCTION_IMPORT").as_deref() == Ok("1") {
        std::fs::write(&output, function_import_module())
            .expect("fake wasm-opt: failed to write the function-import module");
        return;
    }

    if std::env::var("FAKE_WASM_OPT_NO_IMPORTS").as_deref() == Ok("1") {
        std::fs::write(&output, import_free_main_module())
            .expect("fake wasm-opt: failed to write the import-free module");
        return;
    }

    if std::env::var("FAKE_WASM_OPT_VERIFICATION_CONSTRUCT").as_deref() == Ok("1") {
        std::fs::write(&output, verification_construct_module())
            .expect("fake wasm-opt: failed to write the verification-construct module");
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

/// A module whose single function declares 256 `i32` parameters.
///
/// Valid WebAssembly 1.0 — a stock validator admits up to a thousand — and one
/// word past the byte a SpaceWasm embedder keeps a function's parameter size in,
/// so it passes the re-validation step and is caught only by the target's own
/// conformance check.
///
/// Assembled by hand because this fixture takes no dependencies: it is compiled
/// with a bare `rustc` invocation by the test harness.
fn oversized_parameter_module() -> Vec<u8> {
    const PARAMS: u32 = 256;

    let mut functype = vec![0x60];
    leb128_u32(&mut functype, PARAMS);
    functype.extend(std::iter::repeat_n(0x7F, PARAMS as usize));
    functype.push(0x00);

    let mut types = vec![0x01];
    types.extend_from_slice(&functype);

    let mut module = vec![0x00, 0x61, 0x73, 0x6D, 0x01, 0x00, 0x00, 0x00];
    section(&mut module, 0x01, &types);
    section(&mut module, 0x03, &[0x01, 0x00]);
    section(&mut module, 0x0A, &[0x01, 0x02, 0x00, 0x0B]);
    module
}

/// A module whose single function nests 65 empty blocks.
///
/// Valid WebAssembly 1.0 and inside every fixed SpaceWasm limit, so it is
/// accepted — but its control-frame depth is 66 counting the function-body
/// frame, past the 64 the reference `spacewasm_std` embedding is built with.
/// That is the shape a budget warning is about, and it is distinct from the
/// oversized-parameter module next door, which is refused outright.
///
/// Assembled by hand for the same reason: this fixture takes no dependencies.
fn deeply_nested_module() -> Vec<u8> {
    const BLOCKS: usize = 65;

    let mut body = Vec::new();
    for _ in 0..BLOCKS {
        // `block` with an empty block type.
        body.push(0x02);
        body.push(0x40);
    }
    // One `end` per block, plus the one that closes the function body.
    body.extend(std::iter::repeat_n(0x0B, BLOCKS + 1));

    let mut entry = Vec::new();
    // Body size counts the locals-group count byte that follows it.
    leb128_u32(&mut entry, (body.len() + 1) as u32);
    entry.push(0x00);
    entry.extend_from_slice(&body);

    let mut code = vec![0x01];
    code.extend_from_slice(&entry);

    let mut module = vec![0x00, 0x61, 0x73, 0x6D, 0x01, 0x00, 0x00, 0x00];
    section(&mut module, 0x01, &[0x01, 0x60, 0x00, 0x00]);
    section(&mut module, 0x03, &[0x01, 0x00]);
    section(&mut module, 0x0A, &code);
    module
}

/// A module whose only content is one function import, `env.f`, of type
/// `[] -> []`.
///
/// Valid WebAssembly 1.0, so it passes the re-validation step and reaches the
/// comparison of its imports with those of the module it replaces.
///
/// Assembled by hand for the same reason: this fixture takes no dependencies.
fn function_import_module() -> Vec<u8> {
    let mut module = vec![0x00, 0x61, 0x73, 0x6D, 0x01, 0x00, 0x00, 0x00];
    section(&mut module, 0x01, &[0x01, 0x60, 0x00, 0x00]);
    // One import: module `env`, field `f`, a function of type index 0.
    section(
        &mut module,
        0x02,
        &[0x01, 0x03, b'e', b'n', b'v', 0x01, b'f', 0x00, 0x00],
    );
    module
}

/// A module that imports nothing and exports `main`, of type `[] -> [i32]`,
/// returning 0.
///
/// Valid WebAssembly 1.0 that wasmtime can invoke, so it passes the
/// re-validation step and `infs run` executes it.
///
/// Assembled by hand for the same reason: this fixture takes no dependencies.
fn import_free_main_module() -> Vec<u8> {
    let mut module = vec![0x00, 0x61, 0x73, 0x6D, 0x01, 0x00, 0x00, 0x00];
    section(&mut module, 0x01, &[0x01, 0x60, 0x00, 0x01, 0x7F]);
    section(&mut module, 0x03, &[0x01, 0x00]);
    // One export: `main`, the function of index 0.
    section(
        &mut module,
        0x07,
        &[0x01, 0x04, b'm', b'a', b'i', b'n', 0x00, 0x00],
    );
    // One body: no locals, `i32.const 0`, `end`.
    section(&mut module, 0x0A, &[0x01, 0x04, 0x00, 0x41, 0x00, 0x0B]);
    module
}

/// A module whose single function, of type `[] -> []`, evaluates
/// `i32.uzumaki` and drops it.
///
/// The workspace validator decodes the verification opcodes as ordinary
/// operators, so this passes the re-validation step, and only the scan for a
/// verification construct tells it apart from an executable module.
///
/// Assembled by hand for the same reason: this fixture takes no dependencies.
fn verification_construct_module() -> Vec<u8> {
    let mut module = vec![0x00, 0x61, 0x73, 0x6D, 0x01, 0x00, 0x00, 0x00];
    section(&mut module, 0x01, &[0x01, 0x60, 0x00, 0x00]);
    section(&mut module, 0x03, &[0x01, 0x00]);
    // One body: no locals, `i32.uzumaki`, `drop`, `end`.
    section(&mut module, 0x0A, &[0x01, 0x05, 0x00, 0xFC, 0x31, 0x1A, 0x0B]);
    module
}

/// Appends a section with its LEB128-encoded byte length.
fn section(module: &mut Vec<u8>, id: u8, contents: &[u8]) {
    module.push(id);
    leb128_u32(module, contents.len() as u32);
    module.extend_from_slice(contents);
}

/// Appends `value` in unsigned LEB128.
fn leb128_u32(out: &mut Vec<u8>, mut value: u32) {
    loop {
        let byte = (value & 0x7F) as u8;
        value >>= 7;
        if value == 0 {
            out.push(byte);
            return;
        }
        out.push(byte | 0x80);
    }
}
