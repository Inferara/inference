//! Test-only helpers shared by the `infs` unit tests.
//!
//! # Small WebAssembly modules
//!
//! Several unit tests need a module that exercises one instruction, and `wat`
//! cannot assemble the custom `0xfc`-prefixed Inference opcodes, so those
//! modules are assembled byte-by-byte here. The builders are shared rather than
//! per-module because the same few shapes are what every artifact-level test is
//! written against — a body wrapped in a minimal module, that module with a
//! memory so a memory operator can be validated rather than merely parsed, that
//! module with one custom section, and a module that only imports — and a second
//! copy of a shape is a second thing to keep true.
//!
//! # The write-then-exec (`ETXTBSY`) race
//!
//! Several unit tests write a small shell script, mark it executable, and then
//! have the code under test spawn it. That sequence is racy in a multithreaded
//! process. Rust opens files with `O_CLOEXEC`, but `CLOEXEC` closes a descriptor
//! only *at* `exec`: a child forked by another thread between the writer's
//! `open` and its `close` inherits a copy of the still-open write descriptor and
//! holds it until it reaches its own `exec`. An `execve` of the script inside
//! that window fails with `ETXTBSY` ("text file busy"). This test binary spawns
//! processes throughout, so the window is occasionally hit.
//!
//! The condition is transient — every inherited copy is closed by the child's
//! own `exec`, which follows within milliseconds — so a bounded retry is the
//! remedy, the same one Cargo and rustc carry in their spawn paths.
//!
//! Two shapes are needed, because a retry can only fire on a failure it can see:
//!
//! - When the code under test *surfaces* the spawn error, wrap the call in
//!   [`retry_while_exec_busy`]. The operation under test is retried exactly as
//!   written, and any other error returns immediately, so a test asserting a
//!   real failure is unaffected.
//! - When the code under test *swallows* it — mapping a failed spawn to a
//!   fallback value, as the `--version` probes do — there is no error to key on.
//!   Call [`settle_executable`] right after writing the stub instead, which
//!   blocks until the file can be executed at all.
//!
//! # Directory trees
//!
//! The release-archive tests and the `infs self update` tests both hold a
//! directory on disk against the repository's `licenses/`, which CI copies
//! into every release archive. They share the listing of a tree
//! ([`tree_entries`]), the path of a `/`-separated member ([`member_path`])
//! and the location of that directory ([`repository_licenses`]).

use std::path::{Path, PathBuf};
#[cfg(unix)]
use std::process::{Command, Stdio};
use std::time::Duration;

#[cfg(unix)]
use anyhow::Context;
use anyhow::Result;

/// Wraps a raw code-section body (an operator stream) into a one-function
/// module and returns the finished bytes. `wat` cannot assemble the custom
/// `0xfc`-prefixed Inference opcodes, so bodies exercising them are built
/// byte-by-byte (recipe mirrored from `core/wasm-linker/src/safety.rs`).
/// `Function::new([])` emits the empty-locals byte, so `body` is the
/// instruction stream that follows it.
pub(crate) fn module_with_raw_body(body: &[u8]) -> Vec<u8> {
    use wasm_encoder::{CodeSection, Function, FunctionSection, Module, TypeSection};
    let mut module = Module::new();
    let mut types = TypeSection::new();
    types.ty().function([], []);
    module.section(&types);
    let mut funcs = FunctionSection::new();
    funcs.function(0);
    module.section(&funcs);
    let mut code = CodeSection::new();
    let mut f = Function::new([]);
    f.raw(body.iter().copied());
    code.function(&f);
    module.section(&code);
    module.finish()
}

/// Like [`module_with_raw_body`] but the module also declares a one-page
/// memory, so a body exercising memory operators can be *validated* rather
/// than merely parsed.
pub(crate) fn module_with_memory_and_raw_body(body: &[u8]) -> Vec<u8> {
    use wasm_encoder::{
        CodeSection, Function, FunctionSection, MemorySection, MemoryType, Module, TypeSection,
    };
    let mut module = Module::new();
    let mut types = TypeSection::new();
    types.ty().function([], []);
    module.section(&types);
    let mut funcs = FunctionSection::new();
    funcs.function(0);
    module.section(&funcs);
    let mut memories = MemorySection::new();
    memories.memory(MemoryType {
        minimum: 1,
        maximum: Some(1),
        memory64: false,
        shared: false,
        page_size_log2: None,
    });
    module.section(&memories);
    let mut code = CodeSection::new();
    let mut f = Function::new([]);
    f.raw(body.iter().copied());
    code.function(&f);
    module.section(&code);
    module.finish()
}

/// `i32.const 0` three times, `memory.fill 0`, `end` — a well-typed
/// bulk-memory body over the single shared memory.
pub(crate) const MEMORY_FILL_BODY: &[u8] =
    &[0x41, 0x00, 0x41, 0x00, 0x41, 0x00, 0xfc, 0x0b, 0x00, 0x0b];

/// `i32.const 0` three times, `memory.copy 0 0`, `end`.
pub(crate) const MEMORY_COPY_BODY: &[u8] = &[
    0x41, 0x00, 0x41, 0x00, 0x41, 0x00, 0xfc, 0x0a, 0x00, 0x00, 0x0b,
];

/// [`module_with_raw_body`] plus one custom section, for the guard-record
/// tests. `name` is taken as written so a test can build a module whose
/// section is *not* the guard record.
pub(crate) fn module_with_custom_section(body: &[u8], name: &str, payload: &[u8]) -> Vec<u8> {
    use wasm_encoder::{
        CodeSection, CustomSection, Function, FunctionSection, Module, TypeSection,
    };
    let mut module = Module::new();
    let mut types = TypeSection::new();
    types.ty().function([], []);
    module.section(&types);
    let mut funcs = FunctionSection::new();
    funcs.function(0);
    module.section(&funcs);
    let mut code = CodeSection::new();
    let mut f = Function::new([]);
    f.raw(body.iter().copied());
    code.function(&f);
    module.section(&code);
    module.section(&CustomSection {
        name: name.into(),
        data: payload.into(),
    });
    module.finish()
}

/// A module whose import section holds `imports` in the order given, and nothing
/// else but the one `() -> ()` function type a function import can name.
///
/// The import list is the whole of what the scan's import question reads, so the
/// module carries no function of its own: a body would add a code section the
/// question never looks at.
pub(crate) fn module_with_imports(imports: &[(&str, &str, wasm_encoder::EntityType)]) -> Vec<u8> {
    use wasm_encoder::{ImportSection, Module, TypeSection};
    let mut module = Module::new();
    let mut types = TypeSection::new();
    types.ty().function([], []);
    module.section(&types);
    let mut section = ImportSection::new();
    for &(module_name, field, ty) in imports {
        section.import(module_name, field, ty);
    }
    module.section(&section);
    module.finish()
}

/// A module importing each `(module, field, params, results)` as a function of
/// that signature, each under a type of its own, and holding nothing else.
///
/// The typed counterpart of [`module_with_imports`], for a question that reads
/// an import's signature as well as its names.
pub(crate) fn module_with_function_imports(
    imports: &[(&str, &str, &[wasm_encoder::ValType], &[wasm_encoder::ValType])],
) -> Vec<u8> {
    use wasm_encoder::{EntityType, ImportSection, Module, TypeSection};
    let mut module = Module::new();
    let mut types = TypeSection::new();
    let mut section = ImportSection::new();
    for (index, &(module_name, field, params, results)) in (0_u32..).zip(imports) {
        types
            .ty()
            .function(params.iter().copied(), results.iter().copied());
        section.import(module_name, field, EntityType::Function(index));
    }
    module.section(&types);
    module.section(&section);
    module.finish()
}

/// A module exporting each `(name, params, results)` as a function of that
/// signature whose body leaves a zero of each result type, and holding nothing
/// else.
pub(crate) fn module_exporting(
    functions: &[(&str, &[wasm_encoder::ValType], &[wasm_encoder::ValType])],
) -> Vec<u8> {
    use wasm_encoder::{
        CodeSection, ExportKind, ExportSection, Function, FunctionSection, Instruction, Module,
        TypeSection, ValType,
    };
    let mut module = Module::new();
    let mut types = TypeSection::new();
    let mut funcs = FunctionSection::new();
    let mut exports = ExportSection::new();
    let mut code = CodeSection::new();
    for (index, &(name, params, results)) in (0_u32..).zip(functions) {
        types
            .ty()
            .function(params.iter().copied(), results.iter().copied());
        funcs.function(index);
        exports.export(name, ExportKind::Func, index);
        let mut body = Function::new([]);
        for result in results {
            match result {
                ValType::I64 => body.instruction(&Instruction::I64Const(0)),
                ValType::F32 => body.instruction(&Instruction::F32Const(0.0_f32.into())),
                ValType::F64 => body.instruction(&Instruction::F64Const(0.0_f64.into())),
                _ => body.instruction(&Instruction::I32Const(0)),
            };
        }
        body.instruction(&Instruction::End);
        code.function(&body);
    }
    module.section(&types);
    module.section(&funcs);
    module.section(&exports);
    module.section(&code);
    module.finish()
}

/// One member of a directory tree: a directory, or a file and its bytes. The
/// name is the member's path from where the tree is listed, `/`-separated as
/// both archive formats spell a member.
pub(crate) enum TreeEntry {
    Dir(String),
    File(String, Vec<u8>),
}

/// The repository's `licenses/` directory, which CI copies into every release
/// archive beside the binaries.
pub(crate) fn repository_licenses() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("..")
        .join("..")
        .join("licenses")
}

/// `dir` as the member `name`, then everything under it: parents before their
/// children, and siblings in name order.
///
/// # Panics
///
/// Panics if a member cannot be read or its name is not UTF-8.
pub(crate) fn tree_entries(dir: &Path, name: &str) -> Vec<TreeEntry> {
    let mut entries = Vec::new();
    push_tree(&mut entries, dir, name);
    entries
}

/// Appends `dir` as the member `name`, then everything under it, to `entries`.
fn push_tree(entries: &mut Vec<TreeEntry>, dir: &Path, name: &str) {
    entries.push(TreeEntry::Dir(name.to_string()));
    let mut children: Vec<PathBuf> = std::fs::read_dir(dir)
        .unwrap_or_else(|e| panic!("{} must be readable: {e}", dir.display()))
        .map(|entry| entry.expect("Should read directory entry").path())
        .collect();
    children.sort();
    for child in children {
        let child_name = child
            .file_name()
            .and_then(|n| n.to_str())
            .expect("a member's name is UTF-8");
        let member = format!("{name}/{child_name}");
        if child.is_dir() {
            push_tree(entries, &child, &member);
        } else {
            let bytes = std::fs::read(&child)
                .unwrap_or_else(|e| panic!("{} must be readable: {e}", child.display()));
            entries.push(TreeEntry::File(member, bytes));
        }
    }
}

/// The path of the `/`-separated member `name` under `dir`.
pub(crate) fn member_path(dir: &Path, name: &str) -> PathBuf {
    let mut path = dir.to_path_buf();
    path.extend(name.split('/'));
    path
}

/// How many times [`retry_while_exec_busy`] runs its operation before giving up.
const EXEC_BUSY_ATTEMPTS: u32 = 50;

/// Pause between attempts. Together with [`EXEC_BUSY_ATTEMPTS`] this bounds the
/// wait at roughly a second — orders of magnitude longer than a fork/exec
/// window, yet short enough that a genuinely broken stub fails a test rather
/// than hanging it.
const EXEC_BUSY_DELAY: Duration = Duration::from_millis(20);

/// Runs `op`, retrying while it fails with `ETXTBSY`.
///
/// Wrap the operation under test — not just the spawn — so the retry replays
/// exactly what raced. `Ok` and every non-`ETXTBSY` error pass straight through,
/// so a test that expects a real failure sees it on the first attempt.
///
/// # Errors
///
/// Returns the first non-`ETXTBSY` error, or the last `ETXTBSY` error once the
/// attempt bound is exhausted.
pub(crate) fn retry_while_exec_busy<T>(op: impl FnMut() -> Result<T>) -> Result<T> {
    retry_while_exec_busy_with(EXEC_BUSY_ATTEMPTS, EXEC_BUSY_DELAY, op)
}

/// The body of [`retry_while_exec_busy`] with the schedule supplied rather than
/// fixed, so the exhaustion path can be covered without paying the full wait.
fn retry_while_exec_busy_with<T>(
    attempts: u32,
    delay: Duration,
    mut op: impl FnMut() -> Result<T>,
) -> Result<T> {
    let mut attempt = 1;
    loop {
        match op() {
            Ok(value) => return Ok(value),
            Err(err) if attempt < attempts && is_exec_busy(&err) => {
                attempt += 1;
                std::thread::sleep(delay);
            }
            Err(err) => return Err(err),
        }
    }
}

/// Whether any error in `err`'s chain is an `ETXTBSY` I/O error.
///
/// The chain is walked rather than the head inspected because callers add
/// context to a failed spawn (`Failed to execute wasm-opt at …`), which buries
/// the [`std::io::Error`] carrying the errno.
fn is_exec_busy(err: &anyhow::Error) -> bool {
    err.chain()
        .filter_map(|cause| cause.downcast_ref::<std::io::Error>())
        .any(|io_err| io_err.kind() == std::io::ErrorKind::ExecutableFileBusy)
}

/// Blocks until the freshly written executable at `path` can be executed, by
/// running it with `probe_args` until the spawn stops reporting `ETXTBSY`.
///
/// Pass an invocation the code under test makes anyway (a `--version` probe,
/// say) so the extra run has no effect the test can observe. The stub's exit
/// status and output are discarded — only reaching `exec` matters. On a platform
/// that enforces the check, a successful `exec` proves no process still holds a
/// write descriptor to the file, and nothing reopens it for writing afterwards,
/// so the spawns the test goes on to make are safe too.
///
/// # Panics
///
/// Panics if the stub cannot be spawned for any other reason, or if the busy
/// window has not cleared within the attempt bound.
#[cfg(unix)]
pub(crate) fn settle_executable(path: &Path, probe_args: &[&str]) {
    retry_while_exec_busy(|| {
        Command::new(path)
            .args(probe_args)
            .stdin(Stdio::null())
            .output()
            .with_context(|| format!("failed to execute the stub at {}", path.display()))?;
        Ok(())
    })
    .unwrap_or_else(|err| {
        panic!(
            "the stub at {} never became executable: {err:#}",
            path.display()
        )
    });
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::cell::Cell;

    /// A spawn failure shaped like the real one: an `ETXTBSY` [`std::io::Error`]
    /// buried under the context a caller attaches.
    fn busy_error() -> anyhow::Error {
        anyhow::Error::from(std::io::Error::from(std::io::ErrorKind::ExecutableFileBusy))
            .context("Failed to execute wasm-opt at /tmp/stub/wasm-opt")
    }

    #[test]
    fn a_busy_operation_is_retried_until_the_window_clears() {
        let calls = Cell::new(0_u32);
        let value = retry_while_exec_busy(|| {
            calls.set(calls.get() + 1);
            if calls.get() <= 2 {
                Err(busy_error())
            } else {
                Ok(7)
            }
        })
        .unwrap();

        assert_eq!(value, 7, "the first successful attempt must be returned");
        assert_eq!(calls.get(), 3, "both busy failures must have been retried");
    }

    #[test]
    fn a_non_busy_error_is_returned_on_the_first_attempt() {
        let calls = Cell::new(0_u32);
        let err = retry_while_exec_busy(|| {
            calls.set(calls.get() + 1);
            Err::<(), _>(anyhow::anyhow!("wasm-opt failed (exit code 1)"))
        })
        .unwrap_err();

        assert_eq!(
            calls.get(),
            1,
            "an error a test is asserting on must not be retried"
        );
        assert!(err.to_string().contains("wasm-opt failed"));
    }

    #[test]
    fn a_permanently_busy_operation_gives_up_with_the_last_error() {
        let calls = Cell::new(0_u32);
        // The real delay is elided: the bound is what is under test, and
        // sleeping through it would cost a second on every run of the suite.
        let err = retry_while_exec_busy_with(EXEC_BUSY_ATTEMPTS, Duration::ZERO, || {
            calls.set(calls.get() + 1);
            Err::<(), _>(busy_error().context(format!("attempt {}", calls.get())))
        })
        .unwrap_err();

        assert_eq!(
            calls.get(),
            EXEC_BUSY_ATTEMPTS,
            "the retry must stop at the attempt bound rather than spin"
        );
        assert!(
            format!("{err:#}").contains(&format!("attempt {EXEC_BUSY_ATTEMPTS}")),
            "the surfaced error must be the last attempt's, got: {err:#}"
        );
    }

    #[cfg(unix)]
    #[test]
    fn settle_executable_accepts_a_stub_that_exits_nonzero() {
        use std::os::unix::fs::PermissionsExt;

        let dir = assert_fs::TempDir::new().unwrap();
        let stub = dir.path().join("stub");
        std::fs::write(&stub, b"#!/bin/sh\nexit 3\n").unwrap();
        std::fs::set_permissions(&stub, std::fs::Permissions::from_mode(0o755)).unwrap();

        // Settling asks only that the `exec` succeed; what the stub then does
        // with its exit status belongs to the test that runs it for real.
        settle_executable(&stub, &["--version"]);
    }

    /// Exercises the retry against a genuine `ETXTBSY` rather than a synthetic
    /// error. The fork/exec window cannot be scheduled on demand, but the
    /// condition it opens can: a still-open write handle blocks `execve` the
    /// same way an inherited descriptor does, and closing it clears the block.
    ///
    /// Only Linux enforces that in practice — macOS execs a script its writer
    /// still holds open — so on a macOS host this self-skips and the real-race
    /// coverage comes from the ubuntu CI leg.
    #[cfg(unix)]
    #[test]
    fn a_real_etxtbsy_spawn_is_retried_until_the_writer_closes() {
        use std::io::Write;
        use std::os::unix::fs::PermissionsExt;

        let dir = assert_fs::TempDir::new().unwrap();
        let stub = dir.path().join("stub");
        let mut writer = std::fs::File::create(&stub).unwrap();
        writer.write_all(b"#!/bin/sh\nexit 0\n").unwrap();
        writer.flush().unwrap();
        std::fs::set_permissions(&stub, std::fs::Permissions::from_mode(0o755)).unwrap();

        let blocked = Command::new(&stub).stdin(Stdio::null()).output();
        match &blocked {
            Err(err) if err.kind() == std::io::ErrorKind::ExecutableFileBusy => {}
            other => {
                // Nothing to retry where the platform permits the exec anyway;
                // report rather than assert, so the suite stays portable.
                eprintln!(
                    "note: skipping, an open write handle does not block exec here: {other:?}"
                );
                return;
            }
        }

        let release = std::thread::spawn(move || {
            std::thread::sleep(Duration::from_millis(60));
            drop(writer);
        });

        settle_executable(&stub, &[]);
        release.join().unwrap();
    }
}
