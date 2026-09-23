# inference-stellar-abi

Rewrites a compiled Inference module into an uploadable Soroban contract: the
Val-ABI layer of the Stellar compilation target.

## Overview

A Stellar contract does not use an ordinary WebAssembly calling convention.
Every method takes and returns 64-bit `Val` words — tagged unions whose low byte
names what the rest of the word holds — and every contract carries a
`contractenvmetav0` custom section declaring the environment protocol it was
built for. An Inference module has neither: it exports `add(i32, i32) -> i32`
and no metadata at all.

This crate is the layer between the two. Given the linked module and the
per-export source-type descriptor code generation recorded
(`inference_wasm_codegen::ExportSignature`), it returns the same module with one
`Val` wrapper appended per exported function, the export section retargeted onto
the wrappers, and three custom sections on the end: the contract spec, the
contract metadata, and the environment metadata last.

```rust,ignore
let contract = inference_stellar_abi::rewrite(
    &linked_bytes,
    output.export_signatures(),
    inference_stellar_abi::STELLAR_ENV_PROTOCOL,
)?;
```

## Why a rewriter and not an emitter

Nothing in code generation knows about Stellar. The bytes `codegen()` produces
for the Stellar target are the same bytes it produces for Wasm32, which is what
makes *prove the Wasm32 build, deploy the Stellar build* a theorem rather than a
hope: the marshalling layer is a post-link pass over an artifact that has already
been verified, and the wrappers it appends contain no arithmetic, no memory
access, and no control flow beyond one guard per argument.

## The rewrite

```text
1. Validate the input at the WebAssembly 1.0 feature set and parse it,
   recording every section's byte range
2. Match each exported function against the descriptor BY NAME
3. Check admissibility in one order (name, arity, parameter types, return,
   parameter names), the source-level gate's order too; the first refusal
   ends the pass and nothing is written
4. Synthesize one wrapper per exported function: a deduplicated
   (i64 x n) -> i64 type entry, a function entry, and a code body
5. Rebuild: every untouched section copied through by byte range; the type,
   function and code sections keep their entries and gain the new ones; the
   export section moves only the targets of wrapped methods
6. Append the wrappers AFTER every existing function, so no index moves
7. Extend the name section, when there is one, with one entry per wrapper
8. Append the contractspecv0 and contractmetav0 custom sections, then the
   contractenvmetav0 custom section LAST
9. Validate the result at WebAssembly 1.0 and fail closed
```

### By name, never by index

The static-merge linker renumbers functions when it folds an external module in,
so a function index recorded at code generation is stale by the time this pass
runs. The export section is what survives. The match is total in both
directions and fails closed on either mismatch: an exported function with no
descriptor would ship as a method that is reachable and always broken, and a
descriptor with no export would ship a contract silently missing a method.

### Appended, never renumbered

The module carries two records naming raw function indices that this crate does
not decode: `inference.checked`, the overflow-guard record, and the name
section's function names. Appending leaves the first byte-identical and lets the
second be extended. It is also the truthful arrangement: a wrapper holds no
guardable arithmetic, so its absence from the guard record is accurate rather
than merely convenient.

## The custom sections

Each of the three sections a contract carries has a different reader.

| Section | Read by | What it carries |
|---|---|---|
| `contractenvmetav0` | the host, at upload | the declared environment protocol; a contract without it is refused |
| `contractspecv0` | tooling, at invoke and bindings time | every method: its name, each parameter's name and type, and its return |
| `contractmetav0` | tooling; `stellar contract info meta` displays it | one entry, `infver`, the crate version the workspace declares (`CONTRACT_META_TOOLCHAIN_VERSION`) |

The environment metadata goes last, so every contract ends with the same 32
measured bytes it ended with before the other two sections existed. The host
imposes no order on custom sections, and it needs neither of the other two: a
contract without them uploads and invokes.

The meta section is not inert to tooling. `soroban-spec` 28.0.0
(`src/shaking.rs`) looks there for the Rust SDK's spec-shaking key,
`rssdk_spec_shaking`; a value of `"2"` says the data section marks every
user-defined type and event entry the contract uses, and lets tooling such as
the `stellar` CLI strip each such entry without a mark. Function entries are
always kept (`soroban_spec::shaking::filter`), so the key would strip nothing
this crate writes today. It is never written all the same. The marks are the
Rust SDK's own mechanism, which that file says is not part of the SEP-48
contract interface specification, and this crate writes none: the claim would
be false, and the day the spec describes a struct or an enum, those entries
would be stripped without a word.

`contractspecv0` is what lets `stellar contract invoke … -- add --a 2 --b 40`
turn plain command-line arguments into typed `Val` words. Its body is one
`SCSpecEntry` per exported method, in export order, each the `FunctionV0` arm
with an empty doc string. A method that returns nothing is described with an
empty outputs vector, not with `SC_SPEC_TYPE_VOID`: that is what `soroban-sdk`
writes and what the CLI expects. Parameter names are written exactly as the
source spells them, a leading underscore included.

Both tooling sections are XDR, hand-encoded in `src/spec.rs` the way
`src/meta.rs` encodes the environment metadata. Every type code there records
its source: `Stellar-contract-spec.x` or `Stellar-contract-meta.x` in the
stellar-xdr repository at revision `9c9c145953e80990d6ff1ae3a6a973a0ce6d0694`,
the revision the `stellar-xdr` 28.0.0 crate vendors and `soroban-env-host`
28.0.2 pins. The section names are not XDR; each records where the Soroban
tooling spells it (`soroban-spec` 28.0.0, `soroban-sdk` 27.0.6). The
`add(a: u32, b: u32) -> u32` entry is pinned as the 60 bytes derived by hand
from those definitions.

## The Val ABI

| Tag | Value | Body |
|---|---|---|
| `False` | 0 | none — the tag is the value |
| `True` | 1 | none — the tag is the value |
| `Void` | 2 | none |
| `U32Val` | 4 | the 32-bit payload in bits 32..64 |
| `I32Val` | 5 | the 32-bit payload in bits 32..64, two's complement |

Every instruction sequence this crate emits was executed against
`soroban-env-host` 28.0.2 and is recorded byte for byte in
`tests/tests/stellar/MEASURED_ABI.md`. The unit tests in `src/val.rs` pin the
same bytes, so the two records cannot drift apart silently.

Two of those measurements are load-bearing and easy to lose:

- **The boolean wrap normalizes before it tags.** A returned `bool` is only
  guaranteed nonzero, and `True` is tag 1 with an empty body, so extending an
  arbitrary truthy `i32` produces a word the host refuses at invoke time.
  Dropping `i32.const 0; i32.ne` ships a contract that uploads cleanly and then
  fails every call returning a truthy value other than exactly 1. A
  bool-identity fixture cannot see this; the golden here uses an inner value of
  42.
- **Two exports sharing a name is an upload refusal.** A wrapper carries its
  method's own name, so the original export of every wrapped function is
  displaced rather than joined.

## What it refuses

The admissible set is M1: `u32`, `i32` and `bool` parameters; those three or
nothing as a return. Every parameter is named, in at most 30 bytes, because a
caller reaches it by that name: `stellar contract invoke` passes each argument
as `--<name>`. Every refusal is a `StellarAbiError` variant naming the export
and the offending element:

| Refusal | Variant |
|---|---|
| no exported function | `NoExportedFunctions` |
| an export the descriptor is silent about | `UnknownExport` |
| a described export the module lacks | `MissingExport` |
| a descriptor that disagrees with the module's own signature | `DescriptorDisagreesWithModule` |
| an empty method name | `EmptyExportName` |
| a name over 32 bytes | `ExportNameTooLong` |
| a `__`-prefixed name | `ReservedExportName` |
| a name outside `[A-Za-z0-9_]` | `ExportNameNotASymbol` |
| more than 32 parameters | `TooManyParameters` |
| `i64`, `u64`, a narrow integer, a struct, an array or an enum parameter | `UnsupportedParameter` |
| the same as a return | `UnsupportedReturn` |
| a struct or array return, passed through a hidden pointer | `CompoundReturn` |
| a parameter written `_`, which the spec cannot record | `UnnamedParameter` |
| a parameter name over 30 bytes, the width of the spec's input-name field | `ParameterNameTooLong` |
| a surviving import | `ImportsUnsupported` |
| a start section | `StartSectionPresent` |
| a module already carrying `contractspecv0`, `contractmetav0` or `contractenvmetav0` | `AlreadyAContract` |
| more than one `name` custom section | `MultipleNameSections` |
| a declared protocol older than the one Soroban arrived in | `ProtocolPredatesSoroban` |
| bytes outside WebAssembly 1.0, before or after the rewrite | `InputNotWasm1`, `RewrittenNotWasm1` |
| a section that does not decode | `MalformedModule` |

`MalformedModule` is the one row with no reachable input: validation at the
WebAssembly 1.0 feature set runs before the parse, so anything that would fail
to decode has already been refused as `InputNotWasm1`. It exists so that no
decode step in this crate has to guess, and it is listed here so that its
absence from the table cannot be read as an omission.

Two of these are refusals of inputs nothing else rejects. WebAssembly permits
repeated custom sections and no validator complains about two `name` sections,
but this pass rebuilds exactly one function-name map, so carrying both would
duplicate one body and drop the other. And a `contractenvmetav0` section
declaring a protocol below 20 is perfectly well formed — it is refused by every
host that has ever run, at upload, long after the compiler said yes.

## Dependencies

`inference-target-conformance` owns the WebAssembly 1.0 question. `check_wasm1`
used to live here and moved there when a second target came to ask it: two
runtimes held to one envelope have to be held to one answer, and the two error
variants that quote it — `InputNotWasm1` and `RewrittenNotWasm1` — read exactly
as they did, because the message is the validator's own.

`wasmparser` here is the **stock** upstream crate, deliberately not the in-tree
`inf-wasmparser` fork: the fork decodes and validates the custom
non-deterministic opcodes with no feature gate, so it cannot testify about a
module's feature set. Default features are off, because the parsing this crate
does itself — reading the sections the rewrite appends to and rewrites — needs
nothing more.
