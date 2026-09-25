# External Functions and WASM Linking

Inference programs can call functions from pre-compiled `.wasm` libraries using
two cooperating language constructs: `external fn` and `use … from`. The
compiler emits the calls as WebAssembly imports, and a separate link step
(provided by `inference-wasm-linker`) folds the external function bodies into the
output so the final `.wasm` and `.v` files are self-contained.

Self-contained holds for every `use … from` clause that names a *file*. A clause
whose first segment is `host` names none: the function is supplied by whatever
embedder instantiates the module, there is nothing for the link step to merge,
and the import survives into the artifact on purpose. Such a program's `.wasm`
is deliberately not self-contained — see [Host Imports](#host-imports).

## Declaring an External Function

Use `external fn` to declare a function whose body lives in another `.wasm`
module. The declaration looks like an ordinary function signature without a body:

```inference
external fn sum(a: i32, b: i32) -> i32;
```

A named parameter may be declared `mut`:

```inference
external fn store_at(mut ptr: i32, val: i32);
```

`mut` on a **linked** `external fn` parameter declares that the foreign body
may **store** through the address that parameter denotes — for a compound
parameter that is the pointer the caller hands over, for a scalar it is the
integer's own value. This is a claim about the linked library, not a request:
the linker derives, from the merged `.wasm` bytes, which parameters the body
actually stores through, and rejects the link when that set is not covered by
the declaration. An external whose body stores through an *undeclared*
parameter fails to link with `UndeclaredExternWrite`; a parameter declared
`mut` that the body never writes through links anyway, since `mut` here is a
permission the body is not obliged to exercise. The linker's side of this
check — including exactly what it does and does not prove about *where* a
declared store lands — is the subject of
[The WASM Linker](the-wasm-linker.md).

The word *linked* carries the whole of what makes this a check rather than a
claim: the derivation reads the merged body's bytes. A host import has no bytes
in this build, so `mut` on one is an assertion nothing tests — see
[`mut` on a host parameter](#mut-on-a-host-parameter-is-an-assertion-not-a-contract).

The set this check requires you to declare is looser than "the parameter
itself denotes the address" suggests. Attribution is affine: a store's
address is attributed to *every* parameter that may contribute a term to it,
not only the one playing the role of base pointer. The ordinary scaled-index
write `mem[ptr + (idx << 2)] = val` attributes to both `ptr` and `idx`, so
`external fn set_elem(ptr: i32, idx: i32, val: i32);` bound to a body that
writes this way must declare `set_elem(mut ptr: i32, mut idx: i32, val:
i32);` — `idx` included, even though `idx` never itself denotes a location
the body writes to; it only scales `ptr`'s. Narrowing the attribution to the
one operand that "looks like" a base pointer would need the linker to tell a
base from an offset, which an affine form alone does not carry; the safe
direction to err in is attributing too broadly, since that can only demand a
wider declaration than the informal reading of `mut` suggests, never let an
undeclared write through. This over-attribution is specific to a *store's*
own address computation: a *read*-only pointer, such as the source of a
`memory.copy(dest, src, size)`, contributes to no store's dependency and so
is never forced `mut` merely for being touched by the same instruction that
writes `dest`.

`mut` is required on every parameter a linked external's body stores through,
compound or scalar alike — there is no exemption for a pointer-shaped `i32`.
Passing a *compound* argument (a struct or an array) to a `mut` position
additionally requires the argument to be rooted at a `mut` binding, enforced
at the call site by analysis rule A047 (see
[Static Analysis](static-analysis.md)): exactly the same requirement an
ordinary assignment through that binding would carry, since a foreign store
through a `mut` parameter is otherwise the one write in the language invisible
at the call site. A *scalar* argument is not checked by A047 — it passes a
value, not a region, so there is no binding at the call site for the rule to
root — which keeps the check honest rather than complete: a foreign store
through an undeclared caller-supplied integer is still possible and is closed
only by a future containment analysis (issue #420).

Parameter names are optional in the declaration, for a parameter no merged
body writes through:

```inference
external fn sum(i32, i32) -> i32;
```

The unnamed form and the named form are equivalent **only when nothing is
written through the parameter**. Neither `ArgKind::Ignored` (`_: i32`) nor
`ArgKind::TypeOnly` (a bare type) carries a mutability field, and the grammar
gives neither a slot for `mut`, so a parameter a linked body stores through
cannot be declared in the unnamed form at all: `external fn sort_pair([i32;
2]);` bound to a library that writes through it has an empty declared write
set by construction and fails to link with `UndeclaredExternWrite`, whose
message says to name the parameter first. Give every parameter of a writing
external a name.

The type signature must match the exported function in the external module exactly.
If the types disagree, the validation step (`validate_extern`, run by the link
driver when it resolves each binding against the real `.wasm` bytes) reports a
`SignatureMismatch` error and no linked module is produced. A related
resolution-time check: two files that each declare and bind the same
`(module, field)` must agree on which parameters are `mut` — a disagreement is
rejected as `ConflictingWriteSet`, naming both files, since the linker checks
the merged body once and cannot honor two different declarations of it.

A host import reaches neither check. There is no `.wasm` file to resolve it
against, so `validate_extern` never runs on it and no `SignatureMismatch` is
possible: the declarations *are* the only description of the function. What
stands in their place is a consistency check among the declarations themselves
— two files that declare one `(module, field)` at different lowered signatures
are rejected as `ConflictingHostSignature`, and two that declare it with
different `mut` parameters as `ConflictingHostWriteSet`, each naming both files.
Neither compares a declaration to a body, because there is none to compare it
to.

## Binding an External Function to a Module

An `external fn` declaration is not tied to a particular module until a `use`
directive names the source:

```inference
use { sum } from arith;
```

The name after `from` is a **logical module reference**, not a file path. The
compiler resolves it at build time by searching:

1. The `[wasm-dependencies]` table in `Inference.toml` (highest priority).
2. Directories passed via `-L` / `--wasm-lib-dir` on the command line.
3. Directories listed in the `INFERENCE_WASM_LIB_PATH` environment variable
   (a `PATH`-style list, separated by `:` on Unix and `;` on Windows).

That is the resolution for a clause naming a *file*. A clause whose first
segment is `host` names none, and consults none of these three sources: nothing
is searched for and nothing on disk can satisfy it — see
[Host Imports](#host-imports).

A `::` separator is used for namespaced logical names:

```inference
use { sha256 } from crypto::digest;
```

This resolves to `crypto/digest.wasm` in one of the search directories (using the
platform's path separator at resolution time, so the source stays portable across
operating systems).

Multiple names from the same module are grouped in one `use` directive:

```inference
external fn sum(a: i32, b: i32) -> i32;
external fn neg(a: i32) -> i32;
use { sum, neg } from arith;
```

## Host Imports

A `use … from` clause whose first segment is `host` binds its names to functions
the **embedder** supplies at instantiation, not to functions this build links:

```inference
external fn telemetry(id: i32, mut time: [u8; 11], time_len: i32, value: [u8; 4], value_len: i32) -> i32;
use { telemetry } from host::fprime_core;
external fn clock_ms() -> i64;
use { clock_ms } from host::env;
```

`host::fprime_core` imports from the WASM module `fprime_core`, and
`host::env` from `env`; the `host` segment names the provider, not a path
component. The emitted entry is `(import "fprime_core" "telemetry" …)` — the
same two-level name an embedder registers a host function under — and `host`
appears nowhere in the artifact. Both belong to the F´ (F Prime) host set that
`spacewasm_std`, the reference embedder in the SpaceWasm repository, registers,
and are declared at its signatures: an array argument is passed as its
address, so `time` and `value` each arrive as one `i32`, and the length after
each says how many bytes the buffer holds.

### What `host` reserves, and what it does not

`host` is reserved **only as the first segment of a `from` clause**. Everything
else spelled `host` is untouched: a `src/host/` directory and the files under
it, the file import `use host::x;`, any variable, field, parameter, type or
function named `host`, and a linked module whose name merely contains the
segment, such as `use { f } from a::host;`.

The path grammar has a reserved handle of its own — `root`, which names the
entry file (see
[Module Hierarchy and Multi-File Compilation](module-hierarchy-and-multi-file-compilation.md))
— and a reader looking for the symmetry should know where it stops. Both
reserve one name in one position, but in different grammars: `root` is reserved
in `use root;`, which names files in *this project*, while `host` is reserved
after `from`, which names modules *outside* it. They differ in what becomes of a
file of that name, too. `root` **shadows** a literal `src/root.inf` — the name
resolves elsewhere, to the entry file — whereas a `from host::…` clause consults
no file at all, so a `host/io.wasm` on disk is not shadowed by anything; it is
simply never looked for.

Exactly one segment may follow `host`. `use { f } from host::a::b;` is refused,
because an import module string is flat: what an embedder registers is a name,
not a path, and WebAssembly has no nested module name for a runtime to take
apart. Joining the segments into `a::b` would emit a name the author never
wrote, so the compiler refuses the clause and says which one segment to keep.

### `mut` on a host parameter is an assertion, not a contract

[Declaring an External Function](#declaring-an-external-function) defines `mut`
as a claim the linker **checks**: it derives the body's write set from the
merged `.wasm` bytes and rejects a declaration that does not cover it. That
definition is about a linked external. A host import has no bytes in this build,
so nothing derives what the embedder's function stores through, and `mut` on a
host parameter is the author's assertion and stays one — there is no
`UndeclaredExternWrite` for a host import, because there is no body to read it
from. A reviewer weighing what `mut` proves about a mission-critical program
should read it as documentation at a host position and as a checked contract at
a linked one.

The call site is still held to it. A047 requires a compound argument passed to a
`mut` host parameter to be rooted at a `mut` binding, exactly as it does for a
linked external and exactly as an ordinary assignment through that binding
would. So the *caller* is constrained by the declaration; only the declaration
itself is unverified, and the write — when it happens — is performed by code
outside the artifact and outside anything the proof translation describes. A
proof build that admitted host imports would therefore have to carry a `mut`
host parameter as a frame condition on the call or refuse it, since nothing in
the build derives a write set to check the declaration against; as it is, a
proof build refuses any program that binds a host import (see [No contract
travels with a host import](#no-contract-travels-with-a-host-import)).

### A bound host import is emitted whether or not it is called

The import is emitted for the binding, not for a call. An `external fn` bound to
`host::…` and never called still appears in the import section of the artifact
`infc` writes, and the module still fails to instantiate until an embedder
offers a matching function. That is deliberate: the binding is where a program
declares what its environment must supply, so the artifact `infc` writes
carries every import the program declares, and what it requires of an embedder
can be read off the source.

An optimized build narrows that interface to the imports the program uses,
also by design. A project whose `Inference.toml` declares `[build.wasm-opt]`
has Binaryen rewrite `out/main.wasm` in place after `infc` exits, and at any
`level` other than `"0"`, `wasm-opt` removes unused module elements — an
imported function the program never calls among them (`-O0` runs no passes and
keeps it). The artifact that ships then declares only the host imports the
program calls, and the build log says so right after the optimizer's size line,
naming what went and what is left. This program calls `command` and
`telemetry` and binds `clock_ms` without calling it:

```inference
external fn telemetry(id: i32, mut time: [u8; 11], time_len: i32, value: [u8; 4], value_len: i32) -> i32;
external fn command(opcode: i32, arg: i32) -> i32;
use { telemetry, command } from host::fprime_core;
external fn clock_ms() -> i64;
use { clock_ms } from host::env;

pub fn main() -> i32 {
    let id: i32 = command(1, 0);
    let mut time: [u8; 11] = [0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0];
    let value: [u8; 4] = [21, 0, 0, 0];
    return telemetry(id, time, 11, value, 4);
}
```

Its manifest admits all three and turns the optimizer on:

```toml
[package]
name = "flight"
version = "0.1.0"
infc_version = "0.1.0"

[host-imports]
fprime_core = ["telemetry", "command"]
env = ["clock_ms"]

[build.wasm-opt]
level = "z"
```

and `infs build`, with Binaryen 130, prints:

```text
host-imports allowlist: env.clock_ms, fprime_core.command, fprime_core.telemetry
Parsed: src/main.inf
Analyzed: src/main.inf
Codegen complete
host imports: env.clock_ms, fprime_core.command, fprime_core.telemetry
WASM generated at: out/main.wasm
wasm-opt -Oz: main.wasm 300 -> 202 bytes
wasm-opt removed 1 host import the program never calls: env.clock_ms; the artifact now imports fprime_core.command, fprime_core.telemetry
```

So in an optimized build, editing a caller out can change what a deployment
must provide, and never silently: the last word on the imports in the log
describes the artifact that ships, and a build whose imports the optimizer left
alone prints no such line. The allowlist still bounds what ships, by
construction: the optimizer may only remove imports, and optimized bytes
importing a function the artifact `infc` wrote did not import are refused,
leaving that artifact in place, so every import that ships is one `infc`
admitted. `infs run` judges the final bytes, so a program whose host imports
are all uncalled runs under it once they are optimized away (see [Running a
program that binds host imports](#running-a-program-that-binds-host-imports)).

### What is refused, and by which layer

| Refused | Layer |
|---------|-------|
| `use … from host;` with no module after it, or more than one segment after it | type checker |
| Two declarations of one `(module, field)` at different signatures, or with different `mut` parameters | external resolution (`ConflictingHostSignature`, `ConflictingHostWriteSet`) |
| A program binding both host imports and linked modules | external resolution (`MixedHostAndLinked`) |
| An `Inference.toml [wasm-dependencies]` key whose first segment is `host` (`host`, `host::io`) | `infs`, when it resolves the table to forward it on any build or run route, before `infc` is asked to build; `infc` argument handling for `--wasm-dep host=…` from a direct caller |
| A malformed `[host-imports]` table — a key that is not one identifier, an empty array, a field that is not an identifier, a field listed twice | `infs` manifest validation, whenever the manifest is loaded |
| Any build that writes a `.v` (`--mode proof`, a bare `-v`, or `--mode compile -v`) that binds a host import | `infc`, before external resolution |
| A host import at a target that does not bind the host-call convention (`stellar`) | `infc`, before external resolution, in code generation's words; code generation itself for any other caller |
| A host import no embedder could register — a module or field name over 31 bytes, or more than nine parameters — at `--target spacewasm` | `infc` over the declarations, after external resolution and before the allowlist; the post-link conformance check over the bytes |
| A host import the build's allowlist does not admit | `infc`, after external resolution |
| Executing an artifact that imports a function — at `wasm32` any function; at `spacewasm` any function outside the F´ reference hosts or at another signature | `infs run`, after the build, from the artifact's import section |

The proof refusal is raised before external resolution deliberately. A program
that is both mixed and a proof build would otherwise hear the mixed-program
refusal first, and its first remedy is to bind every extern to `host::…` —
advice that walks the author into the proof refusal having rewritten every
clause for nothing. It asks for the build without the artifact rather than
naming a mode, because `-v` requests a `.v` on its own and `--mode compile -v`
is a spelling that already has the mode the advice would otherwise ask for. It
asks for every request to go — a build can pass both, and dropping one of two
changes nothing — and names the one a reader may not have typed: `infs build`
passes `--mode proof` itself for a project whose `Inference.toml` sets
`[build] mode = "proof"`, and there the setting is what to change.

The rest of the order follows the same rule: a refusal is raised before any
other whose remedy it would make void. The `stellar` refusal sits beside the
proof refusal for the same reason, and ahead of the allowlist too, which would
otherwise ask for an entry admitting an import the target refuses anyway. The
SpaceWasm registration caps come before the allowlist because an allowlist
entry has to spell the import's final name: asked the other way round, the
allowlist asks the reader to admit a name no embedder can register, and the
rename the caps then ask for leaves that entry matching nothing.

### The allowlist

`infc --host-imports=<list>` names the host functions a build may bind, as
comma-separated `module.field` pairs:

```bash
infc prog.inf --host-imports=fprime_core.telemetry,env.clock_ms
```

The flag has three states, and the `=` is required so that the empty one is
spellable at all:

| Spelling | Meaning |
|----------|---------|
| flag absent | no policy: every host import the program declares is allowed |
| `--host-imports=` | a declared, empty allowlist: **no** host import is allowed |
| `--host-imports=env.clock_ms,fprime_core.command` | exactly those pairs are allowed |

An import outside the allowlist is refused before any file is written, and every
unadmitted import is reported by one build rather than one per rebuild. The
findings are grouped one paragraph per import module, naming every unadmitted
field of it in one edit, because both the flag and the table hold one list per
module: two separate paragraphs about `env` would ask for a duplicate table key
and for two halves of one command-line list.

Entries are trimmed, so a list pasted out of the [inventory
line](#the-inventory-line) — which separates its pairs with `, ` — is read as
written, provided the paste is quoted:

```bash
infc prog.inf --host-imports="env.clock_ms, fprime_core.telemetry"
```

Unquoted, the shell splits the list at each space before `infc` sees it, and
the second pair arrives as an argument of its own, which `infc` refuses as
unexpected without mentioning the space. An entry that is empty once
trimmed is a stray or trailing comma and is named as one, since
`--host-imports=` is the deliberate spelling one character away from it.

The separator here is a dot and never `::`, and an entry that uses `::` is told
so. The clause being transcribed is `use { clock_ms } from host::env;`, and both
of its obvious transcriptions are wrong: `env::clock_ms` keeps the path
separator, and `host::env.clock_ms` keeps a segment that names the provider and
is never part of an import name. The entry is `env.clock_ms`, and the refusal
spells the entry its reader meant — for `fprime_core::telemetry` or
`host::fprime_core::telemetry` alike, `fprime_core.telemetry` — naming as the
fault whichever of the two mistakes the entry actually makes.

A project keeps its allowlist in `Inference.toml`, one array of field names per
import module, and `infs` fills the flag from it:

```toml
[host-imports]
fprime_core = ["telemetry", "command"]
env = ["clock_ms"]
```

The table has the flag's three states. Absent, no policy applies. Declared with
no keys under it, it admits no host function at all — the state
`--host-imports=` spells on a command line. With keys, it admits exactly the
fields it lists. Every route that compiles a project — `infs build`, `infs run`,
and both of their single-file forms — forwards it as *one* argument, since the
flag requires its `=`:
`--host-imports=env.clock_ms,fprime_core.command,fprime_core.telemetry`,
sorted by module and then field whatever order the manifest listed them in, or
exactly `--host-imports=` for the empty table. Each also echoes the policy
before `infc` runs:

```text
host-imports allowlist: env.clock_ms, fprime_core.command, fprime_core.telemetry
host-imports allowlist: empty (no host function admitted)
```

The echo is what shows a policy was applied; the [inventory
line](#the-inventory-line) `infc` prints next says which imports that policy
admitted. The forward is gated on compiler ABI 1.8, the minor `--host-imports`
landed at, and an older `infc` is refused rather than handed the build without
the flag: it would accept every host import the program binds, and the artifact
it wrote would be byte for byte the one the table exists to police.

The table is validated whenever the manifest is loaded, not only where it is
forwarded — a malformed allowlist is a defect in a policy, and a project
carrying one should not build anything. A key is one import module name:
`host::env` and `env::v2` are both refused, because the module string an
embedder registers is flat — it never contains `::` — and the `host::` prefix
of the clause names the provider and is not part of it, so the key for
`use { clock_ms } from host::env;` is `env`. Each value is a non-empty array of
field names, each an identifier and none listed twice. The empty array is
refused rather than read as "nothing from this module" because the flag has no
spelling for a module with no admitted field; keeping every key populated keeps
the table's modules and the flag's the same set, which is what lets an `infc`
refusal tell its reader whether to add a key or to add a name to the entry
already there.

### The inventory line

`infc` prints one line naming every pair it emits, sorted by module and then
field:

```text
host imports: env.clock_ms, fprime_core.telemetry
host imports (no allowlist): env.clock_ms, fprime_core.telemetry
```

The parenthetical is not decoration. It separates an import admitted by a
reviewed policy from one admitted by the absence of a policy, which two
otherwise identical build logs cannot be told apart by.

In a project whose `[build.wasm-opt]` step runs above level `"0"`, the set that
ships can be smaller, and a `wasm-opt removed …` line printed after this one
then names it (see [A bound host import is emitted whether or not it is
called](#a-bound-host-import-is-emitted-whether-or-not-it-is-called)). The
inventory line is also the only compile-time surface that reports the silent
reinterpretation described next, so it is worth reading even on a build that
succeeds.

### The breaking change

`host` was an ordinary module name before it was reserved, and two spellings
changed meaning:

- `use { f } from host;` no longer resolves `host.wasm`. It is a malformed host
  clause now, and is refused with the spelling to write instead.
- `use { f } from host::io;` used to resolve `host/io.wasm` and link it. It is
  a **host import** of the module `io` now: no file is read, the embedder
  becomes responsible for `f`, and the build still succeeds. No build
  diagnostic is issued — the inventory line is where the change shows up, and
  `infs run` refuses the program, naming `io.f`.

The remedy for either is to rename the module out of that first segment: the
`.wasm` file or directory, the `[wasm-dependencies]` key or `--wasm-dep` name,
and the `use … from` clause that binds it.

### No contract travels with a host import

Nothing in this build describes what a host function *does*. There is no `.wasm`
to validate the declared signature against, no write set to derive from a body,
and no specification an embedder is held to. The declaration is the only
description, and it is checked for internal consistency — two declarations of
one pair must agree — and for nothing else. That is why a proof build is
refused rather than qualified: a `.v` written over a module with an unmodelled
hole in it would claim more than the artifact supports. Admitting host imports
into a proof build later means giving the author a way to *state* the contract,
so a host import's declaration may come to carry more than a signature.

### Running a program that binds host imports

A program that binds a host import runs under the embedder that supplies it.
`infs run` supplies one set of host functions, and to a `spacewasm` build only:
the F´ (F Prime) reference hosts of `spacewasm_std`, the reference embedder in
the SpaceWasm repository, at the signatures that embedder registers them with
([Running a SpaceWasm build](compilation_targets.md#running-a-spacewasm-build)
lists them). A `spacewasm` build whose imports are all among them runs in
process under the SpaceWasm interpreter, each host logging its calls to stderr;
one that imports anything else, or a reference host at another signature, is
refused with nothing executed, naming each such import and the declaration that
would match.

A `wasm32` build runs under `wasmtime`, where `infs run` registers no host
function, and it does not let wasmtime stand in for an embedder either — not
even for the WASI functions the wasmtime CLI provides on its own, which a
program importing them could otherwise run against. Which host functions happen
to be at hand is a property of the runtime, not of the program. `infs run`
therefore builds such a program and then refuses to execute it, naming each
function the artifact imports — and, when every one is an F´ reference host at
its reference signature, the target that provides them:

```text
Error: `infs run` cannot execute this program at the `wasm32` target: out/main.wasm imports 3 functions that its embedder must supply.
  env.clock_ms
  fprime_core.command
  fprime_core.telemetry
A `wasm32` build runs under wasmtime, where `infs run` registers no host functions. All three are F Prime reference hosts, which `infs run` provides to a `spacewasm` build: set `target = "spacewasm"` under `[build]` in Inference.toml and run it again to execute the program under the SpaceWasm interpreter. Otherwise, run it from the embedder that supplies them. See the book's External Functions and WASM Linking chapter ("Running a program that binds host imports").
```

A project whose manifest sets `mode = "proof"` or a `wasm-features` list, which
a manifest naming the `spacewasm` target may not, is told to remove them as well
as to set the target. A file outside any project is told to run `infs init`
first, since it has no manifest to name a target in. A program with any other
import is told only that `infs run` provides host functions to a `spacewasm`
build alone, and only the F´ reference set at its reference signatures, and to
run it from the embedder that supplies them.

The refusal is decided from the artifact rather than from the manifest, because
a `[host-imports]` table is an allowlist and not a declaration: a project can
list functions it never binds, or bind them with no table at all. It is asked of
the bytes that would run, too — in a project build, the bytes `[build.wasm-opt]`
leaves behind, which may have lost an import nothing calls (see [A bound host
import is emitted whether or not it is
called](#a-bound-host-import-is-emitted-whether-or-not-it-is-called)).

An embedder registers each function under the two names its import carries,
before it instantiates the module. For a program declaring

```inference
external fn telemetry(id: i32, mut time: [u8; 11], time_len: i32, value: [u8; 4], value_len: i32) -> i32;
use { telemetry } from host::fprime_core;
external fn clock_ms() -> i64;
use { clock_ms } from host::env;
```

an embedder using wasmtime's Rust API registers the following. The two arrays
arrive as addresses into the calling module's linear memory, which every
module this compiler emits with a linear memory exports as `memory`, so the
host reaches them through the caller:

```rust,ignore
let mut linker = wasmtime::Linker::new(&engine);
linker.func_wrap(
    "fprime_core",
    "telemetry",
    |mut caller: wasmtime::Caller<'_, ()>,
     id: i32,
     time_ptr: i32,
     time_len: i32,
     value_ptr: i32,
     value_len: i32|
     -> i32 {
        /* write the time and read the value through `memory`, then forward
           them to the flight software's telemetry channel */
        0
    },
)?;
linker.func_wrap("env", "clock_ms", || -> i64 { /* read the mission clock */ 0 })?;
let instance = linker.instantiate(&mut store, &module)?;
```

An embedder of the SpaceWasm interpreter registers through `spacewasm`'s host
API instead: one `HostModule` per import module name, handed to the engine
before the artifact is decoded, because the decoder binds each import against
that set as it reads it. For `clock_ms`, with `spacewasm` 0.7.1:

```rust,ignore
use core::ops::ControlFlow;
use spacewasm::{Engine, HostFunction, HostModule, HostName, HostValList, Value};

let clock_ms = HostFunction::try_new(
    HostName::try_from_str("clock_ms")?,
    HostValList::try_new("")?,  // no parameters
    HostValList::try_new("I")?, // one i64 result
    |_engine, _args| {
        ControlFlow::Continue(Some(Value::I64(/* read the mission clock */ 0)))
    },
)?;
let env = HostModule {
    name: HostName::try_from_str("env")?,
    globals: spacewasm::Vec::zero(),
    functions: spacewasm::Vec::from_array([clock_ms])?,
    memory: spacewasm::Vec::zero(),
    table: spacewasm::Vec::zero(),
};
let hosts = spacewasm::Vec::from_array([env])?;
let engine = Engine::new(stack_words, max_modules, hosts)?;
```

Either way, the signature registered must be the one the `external fn`
declared, in WebAssembly's types — each array a single `i32` address, as
`telemetry` shows — since the artifact's import carries that type and the
runtime checks it when it binds the import: wasmtime at instantiation,
SpaceWasm while it decodes the module.

Without an embedder of your own, `infs run` on a `spacewasm` build is the
ready-made way to run a program whose imports are the F´ reference hosts. For
any other import, the repository's SpaceWasm harness is: each
`--host MODULE.FIELD=PARAMS[:RESULT]` builds a function like `clock_ms` above
that answers zero and logs each call it receives, and the harness runs the
program under the flight interpreter. See [Running a module under the embedder
harness](compilation_targets.md#running-a-module-under-the-embedder-harness).

## Calling an External Function

Once declared and bound, an external function is called exactly like a local one:

```inference
external fn sum(a: i32, b: i32) -> i32;
use { sum } from arith;

pub fn add_three(x: i32) -> i32 {
    return sum(x, 3);
}
```

The type-checker validates the call site (argument types, return type) using the
declared signature. If the call passes type checking, codegen emits `call 0` — the
import index — identically to how it would emit a call to a local function.

## What the Compiler Emits (Intermediate Form)

Before linking, the compiled module contains a WASM import section. The single-import
example above produces:

```wat
(module
  (type (;0;) (func (param i32 i32) (result i32)))
  (type (;1;) (func (param i32) (result i32)))
  (import "arith" "sum" (func (;0;) (type 0)))
  (func $add_three (;1;) (type 1) (param $x i32) (result i32)
    local.get $x
    i32.const 3
    call 0
    return
    unreachable)
  (export "add_three" (func 1)))
```

Imported functions occupy the lowest WASM function indices. The local `add_three`
is shifted to index 1 (after the one import at index 0). The call target `call 0`
is the import index, resolved statically during the pre-scan phase: the compiler
resolves the callee name to the `external fn` declaration in scope where the call
is written — its file, and the `spec` block enclosing it — and takes the import
that declaration reserved. Identity is the declaration, not the name, so two files
may each declare `scale` and bind it to a different module, and each file's calls
reach its own.

## The Link Step

`inference-wasm-linker` consumes the intermediate module and the resolved external
`.wasm` binaries, and produces a single self-contained module with the imports
satisfied and removed. The external function bodies are merged in and every index
reference is rewritten into the unified index space.

```text
main.wasm (with imports) ──┐
arith.wasm ────────────────┼──▶ inference-wasm-linker ──▶ unified.wasm
                           │                                     │
                                                          wasm-to-v
                                                                 ↓
                                                          unified.v
```

After linking:
- No `(import …)` referencing `arith` remains in the output.
- The bodies of `sum` (and any functions it calls transitively) are appended after
  `add_three` and called by index.
- The unified module passes validation and flows into `wasm-to-v` as an ordinary
  module whose merged functions translate to Rocq `Definition`s.

For the merge algorithm itself — transitive-closure computation, index re-encoding,
the Tier-B provenance proof, and the full link-error taxonomy — see
[The WASM Linker](the-wasm-linker.md).

## Memory-Merge Feasibility

Not all external functions can be merged. The linker classifies each closure:

| Tier | What the function touches | Merged? |
|------|--------------------------|---------|
| A | No memory, no global or table access, no data — pure arithmetic | Yes |
| B | Memory only through caller-supplied pointers (e.g., `sort(ptr, len)`) | Yes |
| C | Own static data, global access, or indirect-call tables | No — requires a relocatable build |

Tiers A and B turn on what the closure *uses*. A global the function never reads
or writes, and a table with no element segment that nothing names, do not force
Tier C — so the `__stack_pointer` global lld puts in every
`wasm32-unknown-unknown` artifact no longer rejects it on sight.

That is a necessary step toward linking stock toolchain output, not a sufficient
one. Such an artifact also declares a multi-page linear memory, and the merge
never relaxes the anchor module's declared bound, so against an Inference main —
which emits a fixed one-page `(memory 1 1)` — it now clears the tier gate and
fails at memory reconciliation instead. Configurable linear memory is a separate
change.

A Tier-C function produces a clear error at link time:

```text
error: external function `lookup` requires a relocatable build:
         defines or initializes its own static data segments
```

Build the library with a relocatable/position-independent toolchain to enable
Tier-C support in a future release.

## Current Restrictions

- External functions that themselves import their host environment (memory, globals)
  are rejected with a clear error: a static merge cannot reconstruct that environment.
- Analysis rule A024 (`ExternFunctionCall`) is scope-aware: a call to a *bound*
  external (one named by a `use { … } from <module>;` in scope) is allowed and
  flows through the codegen + link path. Only a call to an *unbound* bare
  `external fn` — one with no `use` binding — is rejected, since codegen emits no
  import for it and so cannot compile the call.
- Only one version of each logical module is resolved per build. Multi-version
  dependency resolution is deferred to a future manifest update.
- A `mut` scalar parameter's argument is not checked by analysis rule A047: a
  scalar carries no region for the rule to root a binding in, so a call
  passing a plain `i32` has nothing at the call site to reject. This is an
  accepted, documented gap, not an absence of risk — the shadow stack occupies
  `[0, stack_size)` of the same linear memory a scalar `i32` addresses, and
  under the default layout (one page, entirely stack) a caller's own frame
  sits just below address 65536, so `store_at(65528, 7)` overwrites that
  caller's own locals through a plain `i32.store`, admitted by the linker
  because Tier-B admission proves only that the address *derives from* a
  parameter, not where it lands — see
  [The WASM Linker](the-wasm-linker.md) for what that proof does and does not
  bound. Closing it is tracked in issue #420.

A program that binds host imports (see [Host Imports](#host-imports)) meets
restrictions of its own:

- It cannot bind linked modules as well (`MixedHostAndLinked`); see [What is
  refused, and by which layer](#what-is-refused-and-by-which-layer).
- It has no proof artifact: `infc` refuses every build of it that writes a `.v`;
  see [What is refused, and by which
  layer](#what-is-refused-and-by-which-layer).
- `mut` on a host parameter is not checked against what the embedder's function
  writes (the build holds no body to derive a write set from), though
  declarations of one pair must still agree and A047 still holds the call site
  to it; see [`mut` on a host
  parameter](#mut-on-a-host-parameter-is-an-assertion-not-a-contract).
- It cannot build at `stellar`, which refuses host imports; see [What is
  refused, and by which layer](#what-is-refused-and-by-which-layer).
- An import module or field name has to be an Inference identifier, and nothing
  maps a declaration onto any other import string: `wasi_snapshot_preview1` can
  be bound, while field names such as Soroban's `_` and `0` cannot be spelled.
- [No contract travels with a host
  import](#no-contract-travels-with-a-host-import): nothing in the build
  describes what the function does, and its declaration holds an embedder to
  nothing.
- `infs run` executes it only as a `spacewasm` build whose imports are all F´
  reference hosts at their reference signatures, and refuses every other
  artifact that still imports a function once the build has finished,
  including a project's `[build.wasm-opt]` step; see [Running a program that
  binds host imports](#running-a-program-that-binds-host-imports).
  A `[build.wasm-opt]` step that runs at any level but `"0"` leaves out a host
  import the program never calls, so an optimized program whose host imports
  are all uncalled runs; see [A bound host import is emitted whether or not it
  is called](#a-bound-host-import-is-emitted-whether-or-not-it-is-called).

## Example: Two Libraries, One Module

```inference
external fn sort(mut ptr: i32, len: i32);
external fn checksum(ptr: i32, len: i32) -> i32;
use { sort } from collections;
use { checksum } from crypto;

pub fn process(ptr: i32, len: i32) -> i32 {
    sort(ptr, len);
    return checksum(ptr, len);
}
```

The compiler emits two imports (indices 0 and 1), the local `process` at index 2.
The linker searches both `collections.wasm` and `crypto.wasm`, computes the closure
of each export, and merges the bodies into a single output module.

## Related Resources

- [The WASM Linker](the-wasm-linker.md) — the subsystem deep-dive: merge algorithm, feasibility tiers, the Tier-B provenance proof, and the link-error taxonomy
- [Projects and the infs Toolchain](projects-and-the-infs-toolchain.md) — declaring external `.wasm` modules in `Inference.toml` under `[wasm-dependencies]`
- [Compilation Targets: SpaceWasm host imports](compilation_targets.md#host-imports) — the registration caps `infc` asks of a host import's declaration at `--target spacewasm`, and the conformance check that asks them again of the bytes
- [Compilation Targets: Running a SpaceWasm build](compilation_targets.md#running-a-spacewasm-build) — `infs run` executing a `spacewasm` build under the SpaceWasm interpreter, with the F´ reference hosts and nothing else
- [Compilation Targets: the embedder harness](compilation_targets.md#running-a-module-under-the-embedder-harness) — running a host-import program under the SpaceWasm interpreter with `--host` stubs that log each call
- `core/wasm-linker/README.md` — the merge algorithm, tier classification, and entry point API
- `core/wasm-codegen/docs/function-calls-lowering.md` — three-stage index pre-scan and import section emission
- `core/type-checker` — `ExternOrigin`, `extern_origins()`, and the `A024 ExternFunctionCall` analysis rule
- [WebAssembly import section](https://webassembly.github.io/spec/core/binary/modules.html#import-section) — binary format reference
