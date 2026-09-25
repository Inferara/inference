# inference-spacewasm-runner

Loads and runs a WebAssembly module in process under the SpaceWasm flight
interpreter, and reports every way that can fail as a value.

`spacewasm` is a `no_std` WebAssembly 1.0 interpreter written for on-board use.
It decodes, validates and compiles a module to its own sixteen-bit IR in one
pass, then runs that IR against an instruction budget, so a module that loads
is a module the target runtime accepts, and a refusal carries the byte offset
and the reason a flight computer would have given. The library has no command
line and no allocator of its own: an embedder supplies both. This crate is that
embedder for every program in this workspace that runs a SpaceWasm artifact —
today the SpaceWasm test tier and the `spacewasm-embed` example built on it.

```rust,ignore
use inference_spacewasm_runner::{EngineConfig, Outcome, REFERENCE_MAX_CODE_PAGES, Session, Value};

let mut session = Session::acquire();
let config = EngineConfig { stack_words: 64 * 1024, max_code_pages: REFERENCE_MAX_CODE_PAGES };
let mut module = inference_spacewasm_runner::load(&mut session, &wasm, 1_000_000, config)?;
match module.invoke("add", &[Value::I32(2), Value::I32(40)], 1_000_000)? {
    Outcome::Returned(value) => println!("{value:?}"),
    Outcome::Trapped(reason) => println!("trapped: {reason:?}"),
    Outcome::OutOfFuel => println!("still running"),
}
```

## What is here

- `load` decodes a module at the reference embedder's verifier bounds with no
  host module registered, instantiates it in an engine of its own and runs its
  start function; `load_with` takes the two bounds as const generics and a host
  set. Both return a `LoadedModule` that borrows the session it was loaded
  under, or a `LoadError`: the decoder's verdict, an interpreter that could not
  be allocated, or a start function that trapped, ran out of fuel, paused or
  could not be begun.
- `LoadedModule::invoke` calls an export under an instruction budget. A call
  that returns, traps or runs out of fuel is an `Outcome` — a frame that does
  not fit the value stack included, as `StackOverflow`, since wasmtime reports
  its own exhausted stack as a trap too. A call that cannot be made is an
  `InvokeError`: no such export, an export that is not a function or is a host
  import exported again, arguments of the wrong count or type, or a host
  function that paused it. So is a call that finished without the result its
  function declares, which only an interpreter defect produces and which is
  reported rather than read as a call that returned nothing. Whatever the call
  does, the engine is idle again afterwards.
- `exported_functions` and `exported_host_imports` list what a module exports,
  with each function's WebAssembly signature; `ir_stats` measures the IR it
  compiled to, computed as upstream's `spacewasm_std` computes its own figures.
- `host_module` and `host_set` build the `HostSet` `load_with` binds imports to.
- `coerce_arguments` reads decimal arguments written on a command line as the
  values a function's parameters take; `render` writes a value back.

Every way a load or a call can fail is a value, and none is a panic. That
guarantee assumes every host keeps the interpreter's contract: it returns a
value of the result type it declares, never re-enters the engine it is handed,
and never pauses an engine already holding a paused call. A host that breaks it
can panic inside `spacewasm`, where no runner can turn the failure into a value.

`EngineConfig` carries the two parts of the configuration an embedder chooses at
run time: the words of value stack and the IR pages the code builder may fill.
`REFERENCE_MAX_CODE_PAGES` is the reference embedder's page budget. Every load
builds its own engine holding exactly one module, compiled with `memory.grow`
refused and registered under the empty name, which the interpreter exempts
from its check against the names of the host modules registered beside it.

## The allocator singleton

The interpreter allocates through two `#[no_mangle]` symbols,
`__spacewasm_alloc` and `__spacewasm_dealloc`, which its `global_allocator!`
macro defines around two `static mut` globals. A program has to link exactly
one definition of each: none is an undefined symbol, and two is a
duplicate-symbol error under the workspace's `codegen-units = 1` release
profile — or, on the Windows GNU leg, which links with
`--allow-multiple-definition`, no error at all and whichever definition the
linker met first.

So this crate invokes the macro once, in a private module, and nothing that
links it invokes it again. A test here reads every Rust source in the workspace
— outside `target/`, `.git/`, local tool state and this crate's own `src/` —
for an invocation outside a line comment, with any of the three delimiters and
any spacing the compiler accepts, and fails naming each file that has one.

The allocator behind the symbols passes every request through to the Rust
global allocator, with no bound. That is a deliberate departure from
`spacewasm_std`, upstream's own embedding, which runs behind a bounded page
allocator of sixteen 8 KiB pages because a flight computer has that much and no
more. That allocator gives a page back only once everything on it has been
freed, asserts that nothing new lands on a page after anything on that page has
been freed, and refuses any allocation larger than a page; a host tool loading
arbitrary modules can live with neither the assertion nor the refusal. A
zero-sized request is answered with a dangling, correctly aligned pointer rather
than passed through, since `std::alloc::alloc` is undefined behaviour on one.

## The session

Upstream documents the two allocator symbols as single-threaded and
non-re-entrant, and puts the synchronization on the embedder. `Session` is that
synchronization: a process-wide lock, acquired with `Session::acquire` and held
for as long as the value lives. `load` and `load_with` take it mutably,
`host_module` and `host_set` take it shared, and a `LoadedModule` borrows it, so
a module cannot outlive the lock its memory is freed under. The lock never
poisons, because a caller that panicked while holding it has already stopped
using the interpreter; and it is not re-entrant, so a second acquire on a thread
already holding a session deadlocks that thread. A `Session` is neither `Send`
nor `Sync`: it is released on the thread that acquired it, and a `&Session`,
the proof of the lock the host builders take, cannot reach another thread while
this one holds the lock.

A host module or a host set must be moved into a load, or dropped, while the
session it was built under is still held, since dropping it frees through the
same allocator; it borrows nothing because `load_with` takes that session
mutably beside it.

The interpreter's own public constructors, `HostFunction::try_new` among them,
allocate too, and a caller holding the `spacewasm` crate can reach them without
a session. That route is a convention rather than a signature: build host
functions after acquiring the session, and move them straight into
`host_module`.

## Dependencies

`spacewasm` is the interpreter, `=`-pinned at the workspace, with its
`strict-assertions` feature on unconditionally. Without that feature the
interpreter reads its own IR with `get_unchecked` and compiles its value-stack
bounds checks to nothing, so a malformed IR word or an out-of-range stack slot
would be undefined behaviour instead of a panic; upstream's own `spacewasm_std`
embedding turns it on as well.

`inference-target-conformance` is the source of the reference embedder's two
verifier bounds, `REFERENCE_MAX_CONTROL_FRAMES` and `REFERENCE_MAX_STACK_DEPTH`,
and of `LIMITS_FROM`, the release those limits were read from. This crate
re-exports them rather than restating them, so a load and a conformance report
cannot quote two different envelopes.

`thiserror` derives the error types in `src/errors.rs`.
