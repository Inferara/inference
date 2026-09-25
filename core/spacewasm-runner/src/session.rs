//! The process-wide lock the interpreter runs under, and the allocator it
//! protects.
//!
//! The one invocation of `spacewasm::global_allocator!` in any program that
//! links this crate is in the private `singleton` module below
//! `StdAllocator`. See the crate documentation for why there is exactly one
//! and why it is here.

use std::alloc::Layout;
use std::marker::PhantomData;
use std::ptr::NonNull;
use std::sync::{Mutex, MutexGuard, PoisonError};

use spacewasm::{AllocError, Allocator, WasmMemoryAllocator};

/// Exclusive use of the interpreter in this process, for as long as it is
/// held.
///
/// Holding one is what makes reaching the interpreter sound: its allocator is
/// a pair of `static mut` globals that upstream documents as single-threaded,
/// so every load, every call and every drop of a loaded module happens under
/// one.
///
/// The value is neither `Send` nor `Sync`. Not `Send`, so a session acquired on
/// one thread is released on it. Not `Sync`, because a `&Session` is the proof
/// of the lock the host builders take: a shared reference that could cross to
/// another thread would let that thread allocate through the interpreter while
/// the holder's thread does too.
///
/// ```compile_fail,E0277
/// fn assert_send<T: Send>() {}
/// assert_send::<inference_spacewasm_runner::Session>();
/// ```
///
/// ```compile_fail,E0277
/// fn assert_sync<T: Sync>() {}
/// assert_sync::<inference_spacewasm_runner::Session>();
/// ```
pub struct Session {
    /// Held for the session's lifetime and never read: it is the lock itself.
    _guard: MutexGuard<'static, ()>,
    /// Takes away the `Sync` a `MutexGuard<()>` has, and the `Send` it lacks
    /// already.
    _single_thread: PhantomData<*const ()>,
}

impl Session {
    /// Blocks until no other session is held in this process, then takes the
    /// interpreter.
    ///
    /// Acquire once and pass the session down: the lock is a plain
    /// non-reentrant [`Mutex`], so a second acquire on a thread already
    /// holding a session deadlocks it with no output at all.
    ///
    /// The lock never poisons. A thread that panics while holding a session
    /// has already stopped using the interpreter, and poisoning would turn
    /// every later acquire in the process into an error — in a test binary,
    /// one failing test would bury itself under a failure in every test that
    /// ran after it.
    #[must_use]
    pub fn acquire() -> Self {
        static SESSION: Mutex<()> = Mutex::new(());
        Self {
            _guard: SESSION.lock().unwrap_or_else(PoisonError::into_inner),
            _single_thread: PhantomData,
        }
    }
}

/// The interpreter's allocator, backed by the Rust global allocator.
///
/// Unbounded on purpose; the crate documentation says why this is not
/// upstream's bounded page allocator. One type serves both allocator traits, as
/// upstream's own test allocator does: [`Allocator`] for the interpreter's
/// structures, and [`WasmMemoryAllocator`] for a module's linear memory.
pub(crate) struct StdAllocator;

/// The pointer a zero-sized request is answered with.
///
/// `std::alloc::alloc` is undefined behaviour on a zero-sized layout, and both
/// allocator traits can be reached with one — a module declaring `(memory 0)`
/// asks for a linear memory of no bytes. Answering with a dangling but
/// correctly aligned pointer costs two branches and removes the question,
/// since a zero-sized allocation is never read or written.
fn dangling_for(layout: Layout) -> *mut u8 {
    std::ptr::without_provenance_mut(layout.align())
}

// SAFETY: every `Ok` is either a live `std::alloc` allocation made with the
// requested layout, or — for a zero-sized layout, which is never dereferenced —
// a correctly aligned dangling pointer. `dealloc` releases exactly what `alloc`
// returned, under the same layout, and skips the zero-sized case that owns
// nothing.
unsafe impl Allocator for StdAllocator {
    unsafe fn alloc(&self, layout: Layout) -> Result<*mut u8, AllocError> {
        if layout.size() == 0 {
            return Ok(dangling_for(layout));
        }
        let ptr = unsafe { std::alloc::alloc(layout) };
        if ptr.is_null() {
            Err(AllocError::AllocationFailed)
        } else {
            Ok(ptr)
        }
    }

    unsafe fn dealloc(&self, ptr: *mut u8, layout: Layout) {
        if layout.size() == 0 {
            return;
        }
        unsafe { std::alloc::dealloc(ptr, layout) }
    }
}

impl WasmMemoryAllocator for StdAllocator {
    fn allocate(&self, layout: Layout) -> Result<NonNull<u8>, AllocError> {
        if layout.size() == 0 {
            return NonNull::new(dangling_for(layout)).ok_or(AllocError::AllocationFailed);
        }
        // SAFETY: the layout is non-zero-sized, which is `std::alloc::alloc`'s
        // only requirement.
        NonNull::new(unsafe { std::alloc::alloc(layout) }).ok_or(AllocError::AllocationFailed)
    }

    fn reallocate(
        &self,
        ptr: NonNull<u8>,
        old_layout: Layout,
        layout: Layout,
    ) -> Result<NonNull<u8>, AllocError> {
        if old_layout.size() == 0 {
            return self.allocate(layout);
        }
        if layout.size() == 0 {
            self.deallocate(ptr, old_layout);
            return NonNull::new(dangling_for(layout)).ok_or(AllocError::AllocationFailed);
        }
        // SAFETY: `ptr` came from `allocate`/`reallocate` under `old_layout`,
        // both sizes are non-zero, and `realloc` leaves the original block
        // untouched when it returns null — which is the failure contract this
        // trait states.
        NonNull::new(unsafe { std::alloc::realloc(ptr.as_ptr(), old_layout, layout.size()) })
            .ok_or(AllocError::AllocationFailed)
    }

    fn deallocate(&self, ptr: NonNull<u8>, layout: Layout) {
        if layout.size() == 0 {
            return;
        }
        // SAFETY: `ptr` was returned by `allocate`/`reallocate` under `layout`.
        unsafe { std::alloc::dealloc(ptr.as_ptr(), layout) }
    }
}

/// The interpreter's allocator symbols, defined once for every program that
/// links this crate.
///
/// In a module of its own so the two `static mut` globals the macro declares
/// cannot collide with a name in this file.
mod singleton {
    spacewasm::global_allocator!(super::StdAllocator, super::StdAllocator);
}

#[cfg(test)]
mod tests {
    use std::path::{Path, PathBuf};

    use super::Session;

    /// The directories the scan does not enter: build output, version
    /// control, local tool state, and this crate's own sources, which hold
    /// the one invocation.
    fn skipped(root: &Path, dir: &Path) -> bool {
        let name = dir.file_name().and_then(|name| name.to_str());
        matches!(name, Some("target" | ".git" | ".claude"))
            || dir == root.join("core").join("spacewasm-runner").join("src")
    }

    /// Every `.rs` file under `dir`, skipping what [`skipped`] names and never
    /// following a symbolic link.
    fn rust_sources(root: &Path, dir: &Path, found: &mut Vec<PathBuf>) {
        let entries = std::fs::read_dir(dir)
            .unwrap_or_else(|e| panic!("cannot read {}: {e}", dir.display()));
        for entry in entries {
            let entry = entry.unwrap_or_else(|e| panic!("cannot read in {}: {e}", dir.display()));
            let path = entry.path();
            let kind = entry
                .file_type()
                .unwrap_or_else(|e| panic!("cannot stat {}: {e}", path.display()));
            if kind.is_dir() && !skipped(root, &path) {
                rust_sources(root, &path, found);
            } else if kind.is_file() && path.extension().is_some_and(|ext| ext == "rs") {
                found.push(path);
            }
        }
    }

    /// Whether `source` invokes the allocator macro outside a line comment.
    ///
    /// An invocation is the macro's name, `!`, and the opening delimiter of any
    /// of the three kinds a macro call takes, with whitespace — line breaks
    /// included — allowed between them, as the compiler allows it. A mention of
    /// the name that no delimiter follows is not one.
    fn invokes_the_macro(source: &str) -> bool {
        const NAME: &str = "global_allocator";
        let code = source
            .lines()
            .map(str::trim_start)
            .filter(|line| !line.starts_with("//"))
            .collect::<Vec<_>>()
            .join("\n");
        code.match_indices(NAME).any(|(at, _)| {
            code[at + NAME.len()..]
                .trim_start()
                .strip_prefix('!')
                .is_some_and(|rest| rest.trim_start().starts_with(['(', '{', '[']))
        })
    }

    /// No Rust source in the workspace but this crate's invokes
    /// `spacewasm::global_allocator!`.
    ///
    /// A second invocation is a duplicate-symbol link error under
    /// `codegen-units = 1`, and on the Windows GNU leg, which links with
    /// `--allow-multiple-definition`, it is no error at all: the program binds
    /// whichever definition the linker met first. The scan is textual, so it
    /// finds an invocation in a file no build compiles as well — which is the
    /// point, since an example or a test that invokes it builds on a leg this
    /// one never runs. Fails naming every file that invokes the macro.
    #[test]
    #[cfg_attr(miri, ignore)]
    fn no_other_rust_source_invokes_the_allocator_macro() {
        let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("..").join("..");
        let root = root.canonicalize().expect("the workspace root resolves");
        let mut sources = Vec::new();
        rust_sources(&root, &root, &mut sources);
        assert!(
            sources.len() > 100,
            "the scan found {} Rust sources under {}, so it is not reading the workspace",
            sources.len(),
            root.display()
        );
        let offenders: Vec<String> = sources
            .iter()
            .filter(|path| {
                let source = std::fs::read_to_string(path)
                    .unwrap_or_else(|e| panic!("cannot read {}: {e}", path.display()));
                invokes_the_macro(&source)
            })
            .map(|path| path.strip_prefix(&root).unwrap_or(path).display().to_string())
            .collect();
        assert!(
            offenders.is_empty(),
            "`spacewasm::global_allocator!` is invoked once, in inference-spacewasm-runner, and \
             every program that links the runner takes its definition from there; these files \
             invoke it again: {offenders:?}"
        );
    }

    /// The scan reads an invocation on a code line in every form the compiler
    /// accepts, and not one in a comment or a mention of the name.
    ///
    /// Fails if the comment filter starts swallowing code, or if the match
    /// narrows to one delimiter or one spacing again — either would leave the
    /// workspace scan above green over an invocation the build still links.
    #[test]
    fn the_scan_tells_an_invocation_from_a_comment() {
        for invocation in [
            "spacewasm::global_allocator!(A, A);",
            "    spacewasm::global_allocator!(A, A);",
            "fn f() {}\nglobal_allocator!(A, A);\n",
            "spacewasm::global_allocator! {A, A}",
            "spacewasm::global_allocator!{A, A}",
            "spacewasm::global_allocator![A, A];",
            "spacewasm::global_allocator! (A, A);",
            "spacewasm::global_allocator !(A, A);",
            "spacewasm::global_allocator!\n    (A, A);",
            "spacewasm::global_allocator!\n// the two allocators\n{A, A}",
        ] {
            assert!(invokes_the_macro(invocation), "an invocation: {invocation:?}");
        }
        for mention in [
            "// spacewasm::global_allocator!(A, A);",
            "//! The `global_allocator!(A, A)` macro.",
            "/// global_allocator!(A, A)",
            "spacewasm::global_allocator!\n",
            "use spacewasm::global_allocator;",
            "let what = \"global_allocator! defines the symbols\";",
            "spacewasm::global_allocator!\n// (A, A)",
        ] {
            assert!(!invokes_the_macro(mention), "not an invocation: {mention:?}");
        }
    }

    /// A thread that panics while holding a session leaves the lock usable.
    ///
    /// Fails if the lock starts poisoning, which would turn the one real
    /// failure into a failure of every acquire after it.
    #[test]
    fn a_panic_while_holding_a_session_does_not_poison_it() {
        let holder = std::thread::spawn(|| {
            let _session = Session::acquire();
            panic!("a failing caller, holding the session");
        });
        assert!(holder.join().is_err(), "the holder thread panicked");
        let _session = Session::acquire();
    }
}
