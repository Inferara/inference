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
today the SpaceWasm test tier and the `spacewasm-embed` example built on it —
and it registers the F´ (F Prime) reference host functions of upstream's
reference embedder for a program that imports them.

```rust,ignore
use inference_spacewasm_runner::{EngineConfig, Fuel, HostLog, Outcome, Session, Value, fprime};

let mut session = Session::acquire();
let mut module =
    fprime::load(&mut session, &wasm, HostLog::stderr(), Fuel::Unbounded, EngineConfig::REFERENCE)?;
match module.invoke("add", &[Value::I32(2), Value::I32(40)])? {
    Outcome::Returned(value) => println!("{value:?}"),
    Outcome::Trapped(reason) => {
        let report = module.trap_report("add", reason);
        println!("{}", report.headline());
    }
    Outcome::OutOfFuel { budget } => println!("stopped after {budget} instructions"),
}
```

## What is here

- `load` decodes a module at the reference embedder's verifier bounds with no
  host module registered, instantiates it in an engine of its own and runs its
  start function under a `Fuel` budget; `load_with` takes the two bounds as
  const generics and a host set. Both return a `LoadedModule` that borrows the
  session it was loaded under, or a `LoadError`: the decoder's verdict, an
  interpreter that could not be allocated, or a start function that trapped,
  ran out of fuel, paused or could not be begun.
- `LoadedModule::invoke` calls an export under an instruction budget of its
  own; `LoadedModule::invoke_within_budget` calls it under what the start
  function left of the load's `Fuel`. A call that returns, traps or runs out of
  fuel is an `Outcome` — `OutOfFuel` carrying the budget that ran out, and a
  frame that does not fit the value stack a `StackOverflow` trap, since
  wasmtime reports its own exhausted stack as a trap too. A call that cannot be
  made is an `InvokeError`: no such export, an export that is not a function or
  is a host import exported again, arguments of the wrong count or type, or a
  host function that paused it. So is a call that finished without the result
  its function declares, which only an interpreter defect produces and which
  is reported rather than read as a call that returned nothing. An export that
  is not a function is named for what it is: the module's linear memory, its
  table or one of its globals. Whatever the call does, the engine is idle
  again afterwards.
- `exported_functions` and `exported_host_imports` list what a module exports,
  with each function's WebAssembly signature; `ir_stats` measures the IR it
  compiled to, computed as upstream's `spacewasm_std` computes its own figures.
- `host_module` and `host_set` build the `HostSet` `load_with` binds imports to.
- `fprime` holds the F´ reference hosts, the import check against them and
  `fprime::load`, the one way to run a module with them; see below.
- `coerce_arguments` reads decimal arguments written on a command line as the
  values a function's parameters take — an `i32` or an `i64` in its signed or
  its unsigned range, a value above the signed maximum taken as its unsigned
  bit pattern — and `render` writes a value back.
  `ExportedFunction::arity_clause` says what a function takes, the clause a
  refusal of the wrong count opens with: `` `main` takes 1 argument (i32) ``.
- `TrapReport`, `trap_phrase`, `trap_group` and `out_of_fuel` put a run's
  ending into words; see below.

Every way a load or a call can fail is a value, and none is a panic. That
guarantee assumes every host keeps the interpreter's contract: it returns a
value of the result type it declares, never re-enters the engine it is handed,
and never pauses an engine already holding a paused call. A host that breaks it
can panic inside `spacewasm`, where no runner can turn the failure into a value.

`EngineConfig` carries the two parts of the configuration an embedder chooses at
run time: the words of value stack and the IR pages the code builder may fill.
`EngineConfig::REFERENCE` is the reference embedder's: `REFERENCE_STACK_WORDS`,
1,024 words, and `REFERENCE_MAX_CODE_PAGES`, 256 pages. The SpaceWasm test tier
runs with 65,536 words of stack instead, because it runs the whole codegen
corpus rather than one program.

`Fuel` is the instruction budget of a whole run: `Fuel::Unbounded`, which runs
until the call returns or traps, or `Fuel::Limited(n)`. The interpreter counts
the instructions of its own compiled form of the module, and a call's closing
return is one of them, so a budget of exactly the instructions a call takes
finishes it. The interpreter says how a run ended and never how much of its
budget it spent, so under a limit a start function is run one instruction at a
time, which is the one way to leave the call made after it exactly the rest.
Every load builds its own engine holding exactly one module, compiled with
`memory.grow` refused and registered under the empty name, which the
interpreter exempts from its check against the names of the host modules
registered beside it.

## The F´ reference hosts

`spacewasm_std`, the reference embedder in the `spacewasm` repository, registers
six host functions, which `fprime::REFERENCE_HOSTS` holds in one table:

| Host | Signature | Inference declaration |
|---|---|---|
| `fprime_core.panic` | `(addr: i32, len: i32, line: i32)` | `external fn panic(text: [u8; N], len: i32, line: i32);` |
| `fprime_core.rsleep` | `(ticks: i64)` | `external fn rsleep(ticks: i64);` |
| `fprime_core.command` | `(opcode: i32, arg: i32) -> i32` | `external fn command(opcode: i32, arg: i32) -> i32;` |
| `fprime_core.message` | `(ptr: i32, len: i32)` | `external fn message(text: [u8; N], len: i32);` |
| `fprime_core.telemetry` | `(id: i32, time_ptr: i32, time_len: i32, value_ptr: i32, value_len: i32) -> i32` | `external fn telemetry(id: i32, mut time: [u8; 11], time_len: i32, value: [u8; N], value_len: i32) -> i32;` |
| `env.clock_ms` | `() -> i64` | `external fn clock_ms() -> i64;` |

A row carries the host's names, its parameters with the names the reference
table gives them, its result, the declaration that binds it and its body, and
everything else is built from the rows: the host set, the import check and
every text that lists them.

Each host does what the reference embedder's does:

- `panic` reads `len` bytes of UTF-8 at `addr`, logs `PANIC {text}:{line}` and
  always stops the program with a trap.
- `rsleep` logs `RSLEEP {ticks}`.
- `command` logs `COMMAND {opcode} {arg}` and answers 0.
- `message` reads `len` bytes of UTF-8 at `ptr` and logs `MESSAGE {text}`.
- `telemetry` writes an eleven-byte F´ time of zero at `time_ptr` — a `u16`
  time base, a `u8` time context, `u32` seconds and `u32` microseconds — logs
  `TELEMETRY {id}` and answers 0. It reads nothing at `value_ptr`. The four
  fields are stored one after another, as the reference embedder stores them,
  so a time running past the end of memory is written up to the first field
  that does not fit before the program is stopped.
- `clock_ms` answers the milliseconds since the host set was built, when the
  module began loading, and logs nothing.

A buffer outside linear memory, and bytes that are not UTF-8, stop the program
with a trap rather than failing the host, and the host records why — which
host, the address, the length, the size of memory, the offset of the first
invalid byte, the `time_len` it was given — for the trap to be reported with.

Five things differ from the reference embedder, each on purpose, so the log is
in the style of the reference embedder's rather than the same bytes:

1. Values are logged as plain decimals, `RSLEEP 1` and `COMMAND 42 7`, where
   upstream prints Rust's debug form of an optional value. The `MESSAGE`,
   `TELEMETRY` and `PANIC` lines match upstream's byte for byte for a payload
   with no control characters (see 5).
2. The numbers a host addresses memory with — a guest address, and the length
   of a buffer a host reads — are read as unsigned 32-bit numbers, as
   WebAssembly addresses its memory; upstream sign-extends them, which differs
   for a memory of 32,768 pages or more. `telemetry` compares `time_len` as the
   signed `i32` its declaration gives it, so a negative one counts as short.
3. The lines go to a `HostLog`: standard error, streamed as each call makes it
   and never panicking on a closed stream, or a recording a test reads back.
   The log counts its lines, so a caller can say that host calls had already
   been made when a call trapped or ran out of fuel.
4. `telemetry` stops the program, before writing anything, when `time_len` is
   less than eleven. Upstream writes eleven bytes whatever `time_len` says, and
   since an array argument is passed as its address, those bytes land in the
   caller's frame, past the end of a shorter buffer.
5. A `MESSAGE` or `PANIC` payload has its control characters escaped — `\n`,
   `\r`, `\t` and `\0` as those two characters, every other C0 or C1 control
   and DEL as `\u{..}` — and printable text untouched, so a payload is one line:
   an embedded newline cannot forge a `PANIC` line, and zero padding shows as
   `hello\0\0\0`.

Every host is built with the interpreter's fallible constructors, answers a
value of the result type its row declares, and never re-enters or pauses the
engine.

`fprime::check_imports` reads a module's import section with stock `wasmparser`,
from the bytes alone, and compares every import to the table by its two names
and then by its signature. It lists every offender, sorted by module and then
field name: a function no host carries both names of, noting when a host
carries the field under the other module; a function a host carries at another
signature, with both signatures and the declaration that matches; and any
import that is not a function. A module whose sections it cannot read is left
to the interpreter, which refuses it whole.

`fprime::load` is the one way to run a module against the hosts. It runs the
import check first, so a module with an import the hosts do not provide is
refused before a byte of it is decoded; builds the host set under the session;
loads the module at the reference verifier bounds with the caller's `Fuel` and
`EngineConfig`; and returns a `HostedModule`, which calls exports under the
load's budget and holds the host's trap detail and the log. A module the
decoder refuses for want of memory — one verdict, `AllocError(OutOfMemory)`, for
too deep a control nesting, too tall an operand stack or too much IR for the
code pages — is measured again by the target's conformance check, which names
the limit it exceeds, the function and both numbers; inside both verifier
bounds it is too much IR. A module the conformance check accepts and the
decoder refuses for any other reason — a data segment outside linear memory or
at a negative offset among them — is reported as a gap in the check, except
for the verdicts the check leaves to others, which keep the decoder's plain
verdict: the host set's; an allocation that failed; a `memory.grow`, which the
code builder refuses because the load tells it to, as `spacewasm_std` tells its
own; and the three about the interpreter's compiled form of the module
(`LabelJumpTooLarge`, `PageFault`, `PossibleBackpatchCycle`), which the check
cannot reproduce without being the interpreter.

The texts the runner produces — the per-import lines of an import refusal, the
note explaining WebAssembly signatures, the reference table, a trap's first line
and its explanation, a host's detail, the over-limit facts, the argument
refusals, a function's arity clause, what an export that is not a function is,
and the core of the out-of-fuel sentence — state what the runner knows and
never name the program embedding it. Where a sentence has to, the caller passes
its name; everything a caller composes around them, such as the artifact's
path, its own flags and its own remedies, is the caller's.

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

`wasmparser` is the stock WebAssembly parser, the same one
`inference-target-conformance` reads with. The F´ import check reads a module's
import section with it before the interpreter decodes a byte, and answers about
any WebAssembly, not only what the interpreter accepts.

`thiserror` derives the error types in `src/errors.rs`.
