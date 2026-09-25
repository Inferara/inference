//! An in-process embedder for the `SpaceWasm` flight interpreter.
//!
//! `spacewasm` is a `no_std` WebAssembly 1.0 interpreter written for on-board
//! use. It decodes, validates and compiles a module to its own sixteen-bit IR
//! in one pass, then runs that IR against an instruction budget. Decoding *is*
//! validation *is* IR compilation — there is no validate-only entry point — so
//! [`load`] returning `Ok` is the whole statement "this artifact loads on the
//! target runtime", and a refusal carries the byte offset and the reason a
//! flight computer would have given. The library brings no command line and no
//! allocator: an embedder supplies both, and this crate is the embedder every
//! program in this workspace that runs a `SpaceWasm` artifact goes through.
//!
//! # The allocator singleton
//!
//! The interpreter allocates through two `#[no_mangle]` symbols,
//! `__spacewasm_alloc` and `__spacewasm_dealloc`, which its
//! `global_allocator!` macro defines around two `static mut` globals. A
//! program has to link exactly one definition of each. None is an undefined
//! symbol at link time; two is a duplicate-symbol error under
//! `codegen-units = 1`, which is this workspace's release profile, while the
//! Windows GNU linker, which this workspace runs with
//! `--allow-multiple-definition`, would take two without a word and bind
//! whichever it met first.
//!
//! So the macro is invoked once, here, in a private module, and nothing that
//! links this crate invokes it again: not a binary, not a test, not an example.
//! A test in this crate reads every Rust source in the workspace for a second
//! invocation and names the file it finds one in.
//!
//! The allocator behind the symbols is an unbounded passthrough to the Rust
//! global allocator. Upstream's own embedding, `spacewasm_std`, runs behind a
//! bounded page allocator of sixteen 8 KiB pages instead, because a flight
//! computer has that much and no more. That allocator gives a page back only
//! once everything on it has been freed, asserts that nothing new lands on a
//! page after anything on that page has been freed, and refuses any allocation
//! larger than a page; a host tool loading arbitrary modules can live with
//! neither the assertion nor the refusal. What the bounded configuration
//! costs a module is a measurement of its own, not a verdict about whether the
//! module loads.
//!
//! # The session
//!
//! Upstream documents the allocator symbols as single-threaded and
//! non-re-entrant and puts the synchronization on the embedder: reaching them
//! from two threads at once is undefined behaviour by that contract. A
//! [`Session`] is that synchronization — a process-wide lock — and it is the
//! only door: [`load`] and [`load_with`] take it mutably, [`host_module`] and
//! [`host_set`] take it shared, and a [`LoadedModule`] borrows the session it
//! was loaded under, so a module cannot outlive the lock its memory is freed
//! through. A session is neither `Send` nor `Sync`: it is released on the
//! thread that acquired it, and a `&Session`, the proof of the lock the host
//! builders take, cannot reach another thread while this one holds the lock.
//!
//! A host module or a host set must be moved into a load, or dropped, while
//! the session it was built under is still held, since dropping it frees
//! through the same allocator; it borrows nothing because [`load_with`] takes
//! that session mutably beside it.
//!
//! One route stays a convention rather than a signature: the interpreter's own
//! public constructors, `HostFunction::try_new` among them, allocate, and a
//! caller holding the `spacewasm` crate can call them without a session. A
//! caller that builds its own host functions does so after [`Session::acquire`]
//! and moves them straight into [`host_module`].
//!
//! # No panics on the embedder's path
//!
//! Every way a load or a call can fail is a value: a [`LoadError`] from
//! [`load`], an [`InvokeError`] from [`LoadedModule::invoke`], a
//! [`HostSetError`] from the host builders and an [`ArgumentError`] from
//! [`coerce_arguments`]. A trap and an exhausted instruction budget are not
//! errors at all but the two ways a call can end besides returning, so they
//! are [`Outcome`]s — including a call the engine refuses for lack of stack,
//! which wasmtime reports as a trap too.
//!
//! The guarantee assumes every host keeps the interpreter's contract: it
//! returns a value of the result type it declares, never re-enters the engine
//! it is handed, and never pauses an engine already holding a paused call. A
//! host that breaks it can panic inside `spacewasm`, where no runner can turn
//! the failure into a value.
//!
//! # The configuration
//!
//! The verifier's two stacks are const generics upstream, so an embedder's
//! bounds are fixed when it is compiled. [`load`] fixes them at the reference
//! embedder's values, re-exported here from `inference-target-conformance`
//! rather than restated, and [`load_with`] takes them as parameters. The rest
//! of the configuration — the words of value stack and the IR pages the code
//! builder may fill — is an [`EngineConfig`] the caller passes on each load.
//! Every load builds its own engine holding exactly one module, compiled
//! without `memory.grow` and registered under the empty name.

#![warn(clippy::pedantic)]

mod args;
mod errors;
mod hosts;
mod module;
mod session;

pub use args::{coerce_arguments, render, type_name};
pub use errors::{ArgumentError, HostSetError, InvokeError, LoadError};
pub use hosts::{HostSet, host_module, host_set};
pub use inference_target_conformance::spacewasm::{
    LIMITS_FROM, REFERENCE_MAX_CONTROL_FRAMES, REFERENCE_MAX_STACK_DEPTH,
};
pub use module::{
    EngineConfig, ExportedFunction, IrStats, LoadedModule, Outcome, REFERENCE_MAX_CODE_PAGES,
    ReexportedImport, ir_stats, load, load_with,
};
pub use session::Session;
// Exactly the upstream types this crate's public signatures and error fields name.
pub use spacewasm::{
    AllocError, HostFunction, HostGlobal, HostModule, HostNameError, MemoryError, ParseError,
    TrapReason, ValType, Value,
};
