# Measured Val ABI

Every sequence below was assembled with `wasm-encoder`, uploaded to
`soroban-env-host` 28.0.2 with `testutils`, and invoked. Nothing here is derived
from reading the host's source: each line is what a passing test in
`wrappers.rs`, `envelope.rs` and `contracts.rs` executed. The byte columns are
pinned by
`wrappers::measured_sequences_encode_to_the_recorded_bytes`, so this file cannot
go stale without a test going red.

Measured on 2026-09-12, macOS aarch64, `soroban-env-host = "=28.0.2"`,
`wasm-encoder = "0.254.0"`, host ledger protocol 28.

**Headline: every sequence derived from the written ABI description is correct as
written.** All seven shapes round-trip through the real host, both trap forms behave
as predicted, and the declared protocol is accepted. Nine things that description
did not say are
recorded under [Corrections](#corrections-to-the-written-abi-description); the export
identity rules and the memory/data-segment rules are measured in their own
sections below, because the compiler's own gate is built from them.

## The wrapper shape

An exported contract method is `(param i64 …) (result i64)`. The wrapper
unwraps each argument, calls the inner function that holds the real code, and
wraps the result:

```wat
(func (export "f") (param i64) (result i64)
  <unwrap p0> ... <unwrap pn>
  call <inner>
  <wrap result>)
```

The guard blocks are empty-blocktype and consume only their own condition, so
operands pushed by earlier unwraps survive them. Measured: a two-argument
wrapper (`add`) works, so the second unwrap's guard does not disturb the first
unwrap's operand.

## UNWRAP — `u32` parameter at local `i`

```wat
local.get i
i64.const 255
i64.and
i64.const 4      ;; Tag::U32Val
i64.ne
if
  unreachable
end
local.get i
i64.const 32
i64.shr_u
i32.wrap_i64
```

```
20 00 42 ff 01 83 42 04 52 04 40 00 0b 20 00 42 20 88 a7
```

Matches the written description exactly. Round-tripped for `0`, `1`, `7`, `0x7fffffff`
and `u32::MAX`.

## UNWRAP — `i32` parameter at local `i`

Identical but for the tag byte: `i64.const 5` in place of `i64.const 4`.

```
20 00 42 ff 01 83 42 05 52 04 40 00 0b 20 00 42 20 88 a7
```

Matches the written description exactly. `shr_u 32` then `wrap` yields the raw 32-bit
pattern, which is already the two's-complement `i32`. Round-tripped for `0`,
`1`, `-1`, `i32::MAX` and `i32::MIN`; `I32Val(-1)` is `0xffffffff00000005` as
prescribed.

## UNWRAP — `bool` parameter at local `i`

```wat
local.get i
i64.const 255
i64.and
i64.const 1
i64.gt_u
if
  unreachable
end
local.get i
i64.const 255
i64.and
i32.wrap_i64
```

```
20 00 42 ff 01 83 42 01 56 04 40 00 0b 20 00 42 ff 01 83 a7
```

Matches the written description's non-hoisted variant (the `and` is computed twice
rather than kept in a scratch local). `False` and `True` are tags 0 and 1 with
an empty body, so once the tag is known to be in range it *is* the value.
Round-tripped for `true` and `false`.

### The guard checks the tag and nothing else

The sequence masks the low byte and refuses only a value above one, so a word
whose tag is `True` but whose body is **not** empty — `0x0000_0000_0000_0101`,
bit 8 set — passes the guard and yields `true`. The wrap side is strict about
exactly that shape: `wrappers::bool_wrap_without_normalization_is_refused_by_the_host`
measures the host refusing a *returned* `0x101` with `Error(Value,
UnexpectedType)`. The two halves of the ABI therefore disagree about the same
word, and `soroban-sdk` traps where this sequence accepts.

Measured, on a real compiled contract, in both directions:

| Route | Word | Outcome |
|---|---|---|
| through the host, `negate(0x101)` | `True` with bit 8 set | **never reaches the contract**: `Error(Value, InvalidInput)`, and the host's `fn_call` event count does not move |
| the same word into `Host::vec_new_from_slice` on its own | same | `Error(Value, InvalidInput)` |
| the same word pushed onto an empty argument vector | same | `Error(Value, InvalidInput)` — the vector cannot be built |
| a clean `True` into `Host::vec_new_from_slice` | `0x1` | accepted, and `negate` answers `false` |
| the contract's own wrapper under `wasmtime`, no host | same | returns `False`, i.e. the unwrap read it as `true` |
| the same wrapper under `wasmtime` with tag 2 | `Void` | `UnreachableCodeReached` — the guard is live |

So the laxity is real in the instructions and **unreachable on both routes
measured**. An argument vector is built either from a slice or by pushing onto
an empty one, and both refuse the word before any contract is entered:

| Route | Refused by | Outcome |
|---|---|---|
| `Host::vec_new_from_slice` | its own per-element `check_val_integrity` | `Error(Value, InvalidInput)` |
| `Host::vec_new` then `Host::vec_push_back` | the `Env` boundary, which checks every `Val` argument of every host function before the implementation sees it | `Error(Value, InvalidInput)` |

The second route is the one that says something about the surface rather than
about the check: `vec_push_back` makes no `check_val_integrity` call of its own,
and the word is refused anyway, so there is no vector to call with and no third
call to make. Its *mechanism* is the one claim in this file read from the host's
source rather than executed — the pinned host calls `check_val_integrity`
explicitly in exactly three places, `vec_new_from_slice`,
`vec_new_from_linear_memory` and `map_new_from_slices`, and the blanket
`impl Env for T: VmCallerEnv` every host function is reached through runs the
same check over every `Val` argument it forwards. The refusal itself is
measured, and so is each attribution: the step is called on its own. The host's
`fn_call` event count does not move across either route, so nothing reached
dispatch.

**The sequence is therefore left as measured, and the asymmetry is a choice:**
the guard's job is to reject a tag outside `{0, 1}`, because the body bits of an
argument are already guaranteed clear by the layer that delivers it. Tightening
it to `i64.ne 0` against the whole word would cost two instructions per boolean
parameter to re-check something no caller can violate. What would change that is
a route into a contract that does not pass its arguments through a host
function; the pair of tests above is what would notice, since the second
measures the wrapper with the host taken out of the way. Both live in
`contracts::a_boolean_argument_with_a_nonzero_body_is_refused_on_both_routes_into_a_contract`
and
`contracts::the_boolean_unwrap_accepts_a_nonzero_body_when_the_host_is_not_in_the_way`.

## WRAP — `u32` return

```wat
i64.extend_i32_u
i64.const 32
i64.shl
i64.const 4
i64.or
```

```
ad 42 20 86 42 04 84
```

Matches the written description exactly.

## WRAP — `i32` return

Identical but for the tag byte: `i64.const 5`.

```
ad 42 20 86 42 05 84
```

Matches the written description exactly.

## WRAP — `bool` return

```wat
i32.const 0
i32.ne
i64.extend_i32_u
```

```
41 00 47 ad
```

Matches the written description exactly, **and the normalization is mandatory, not
defensive.** Measured both ways: an inner function returning `42` wrapped with
this sequence yields `True`, and the same module with the `i32.const 0; i32.ne`
removed is refused by the host. A rewriter that drops these two instructions
ships a contract that uploads and then fails at invoke time for every truthy
value that is not exactly 1.

Dropping the normalization goes wrong by **two distinct mechanisms**, and only
the second is the one the ABI rule is about. Both are measured, with the same
outer refusal:

| Inner `i32` | Word after `i64.extend_i32_u` | Why the host refuses it | Refusal |
|---|---|---|---|
| `42` | `0x000000000000002a` | low byte 42 is not a tag at all | `Error(Value, UnexpectedType)` |
| `257` | `0x0000000000000101` | low byte 1 *is* `Tag::True`, but `True` has an empty body and bit 8 is set | `Error(Value, UnexpectedType)` |

`True` being tag 1 with body 0 is the spec rule, so `257` is the fixture that
exercises it; `42` only shows that an arbitrary truthy value lands on no tag.
The two are asserted together in
`wrappers::bool_wrap_without_normalization_is_refused_by_the_host`, each with a
matching classification by this harness's own decoder (`Other { tag: 42 }` and
`Malformed { tag: 1, body: 1 }`) so neither fixture can silently stop measuring
its mechanism.

## WRAP — unit / void return

```wat
i64.const 2
```

```
42 02
```

Matches the written description exactly. The inner function has no result and the
wrapper still returns one `i64`.

## The metadata section — exact bytes

The one byte sequence the rewriter must reproduce verbatim and cannot derive
from the module it is rewriting. Pinned as bytes, not as a length, by
`envelope::the_reference_contract_ends_with_the_exact_metadata_section`: they
are the reference contract's trailing 32 bytes.

```
00                                ;; custom section id
1e                                ;; LEB128 payload size = 1 + 17 + 12 = 30
11                                ;; LEB128 name length = 17
63 6f 6e 74 72 61 63 74 65 6e 76 6d 65 74 61 76 30   ;; "contractenvmetav0"
00 00 00 00                       ;; SCEnvMetaKind = SC_ENV_META_KIND_INTERFACE_VERSION
00 00 00 14                       ;; protocol 20, big-endian
00 00 00 00                       ;; preRelease = 0
```

As one run:

```
00 1e 11 63 6f 6e 74 72 61 63 74 65 6e 76 6d 65 74 61 76 30 00 00 00 00 00 00 00 14 00 00 00 00
```

The section is last in the module and 32 bytes long in total. A length pin
alone admits two compensating errors — a name byte and a payload byte moving in
opposite directions — which is why the whole run is compared.

## Export identity

Every rule here is a *call-time* rule. The only export check the host runs at
upload is the argument-count check, so a module carrying an unusable export name
uploads cleanly and is broken later, in one of two distinct ways:

- the name is not a `Symbol` at all (over 32 bytes, or outside `[A-Za-z0-9_]`),
  so no caller can express it and the failure is in the host's `Symbol`
  constructor, before any contract is reached;
- the name is a valid `Symbol` the host refuses to dispatch to (the `__` prefix).

`support::symbol_for` exists to tell those two apart: it runs the same
conversion `call` runs internally, on its own.

| Export name | Upload | Nameable as a `Symbol` | Call |
|---|---|---|---|
| `aaaa…` (32 bytes) | accepted | yes | invokes, returns `Void` |
| `aaaa…` (33 bytes) | accepted | **no** — `Error(Value, InvalidInput)` | `Error(Value, InvalidInput)` |
| `__reserved` | accepted | yes | `Error(Context, InvalidAction)`, `"can't invoke a reserved function directly"` |
| `has-hyphen` | accepted | **no** — `Error(Value, InvalidInput)` | `Error(Value, InvalidInput)` |

32 bytes is measured on both sides: the 32-byte name converts and invokes, the
33-byte one does not convert. The `Symbol` conversion errors carry no debug
text — the rendering is `HostError: Error(Value, InvalidInput)` followed by
`DebugInfo not available`, because the failure is in a value conversion rather
than in a host operation with an event log.

### A non-Val export beside a wrapped one

```wat
(func (export "raw") (param i32) (result i32) local.get 0)
(func (export "f") (result i64) i64.const 2)
```

Measured: **uploads, and `f` invokes normally returning `Void`.** The raw export
is reachable by name and always fails, differently depending on the caller:

| Call | Error | Message |
|---|---|---|
| `raw()` | `(WasmVm, UnexpectedSize)` | `VM call failed: Func(MismatchingParameterLen)` |
| `raw(U32Val(7))` | `(WasmVm, UnexpectedType)` | `VM call failed: Func(MismatchingParameterType)` |

**Decision for the rewriter: it MAY leave a non-Val export in place — the host
neither refuses it nor lets it disturb a properly wrapped export — but it MUST
remove or rename the original export of any function whose name the wrapper
takes.** Two exports of one name is an *upload* refusal,
`Error(WasmVm, InvalidAction)` (measured; wasmi's validation text is not
surfaced, the event log repeats the code). Since the whole point of the wrapper
is to carry the contract method's own name, in practice every wrapped function's
original export is displaced by that rule. Exports the rewriter does not wrap
are the free case: leaving them costs nothing at upload, but they are callable
and always fail, so dropping them is the better default.

## Linear memory and data segments

Linear memory is **conditional**: the emitter declares it for a program that
uses a compound value and for no other, so most of the fixtures in
[Compiled contracts](#compiled-contracts) carry no memory section at all and
their export sections hold nothing but functions.
`array_local.inf` is the one that does — an exported signature inside the
scalar set over a body holding an array local — and the contract it compiles
to is 320 bytes carrying one memory of minimum 1 page and maximum 1 page, the
exports `pick` (function), `memory` (memory) and `__stack_pointer` (global).
Minimum equal to maximum is the emitter's shape and not this section's: the
hand-assembled modules here declare no maximum at all, which is why the
compiled fixture is what the export retarget over non-function entries is
measured on. `contracts::the_memory_fixture_deploys_with_linear_memory_and_its_two_exports`
asserts all of it, with `return_only` beside it as the control that declares no
memory.

**No data segment is emitted today.** The array local above is initialized by
stores rather than by a segment, and its compiled contract carries no data
section — asserted beside the memory it does carry, so the day that changes is
visible. Nor could one reach a contract by another path right now: the linker
every Stellar build runs refuses a main module that declares a data segment,
because it rebuilds the module section by section and would drop it. The
data-segment rows below therefore measure a host rule ahead of the emitter
reaching it, deliberately: it is the rule an emitter that started using data
segments would have to satisfy.

The module builder here takes an initial page count, an export name for the
memory, and one active data segment.

```wat
(memory 1)
(export "memory" (memory 0))
(data (i32.const 0) "\2a\00\00\00")
(func (export "f") (result i64)
  i32.const 0
  i32.load
  i64.extend_i32_u
  i64.const 32
  i64.shl
  i64.const 4
  i64.or)
```

| Module | Outcome |
|---|---|
| one page, exported as `memory`, 4-byte data segment at offset 0 | uploads; `f` returns `U32Val(42)`, so the segment reached the instantiated memory |
| the same module with the memory exported as `mem` | uploads; `f` still returns `U32Val(42)` |
| one page, 4-byte data segment at offset 65534 | **refused at upload**: `Error(WasmVm, IndexBounds)`, `"Memory(OutOfBoundsAccess)"` |

Two things follow.

**The `memory` export name is not enforced.** The host only ever *looks up* that
literal; a memory exported under any other name is a silent absence, not a
refusal, and a scalar-only contract never notices because nothing asks the host
to read its memory. The emitter cannot rely on being told it got the name wrong.
The emitter already exports `memory` (the sole `export("memory", …)` in
`core/wasm-codegen/src/compiler.rs`), so the rewriter's obligation is to leave
that export alone rather than to add it.

**A data segment past the declared initial memory is refused at upload**, not at
first touch, because the upload instantiates a throwaway VM and instantiation is
where an active data segment is written. The offset-0 module is the control: the
two differ only in the segment's offset.

## Compiled contracts

Everything above is measured on modules this file's own harness assembled. This
section is measured on modules the **compiler** produced: `contracts.rs` reads
`.inf` fixtures from `test_data/stellar`, runs code generation at the Stellar
target, the link and the Val-ABI rewrite — the three steps `infc --target
stellar` runs — uploads the result and invokes it.

Ten fixtures cover the M1 scalar set in every position: parameters with no
answer, an answer with no parameters, both together, neither, the widest method
the host will dispatch to (32 `u32` parameters), a boolean round trip, a boolean
computed from an unsigned argument, a negative integer round trip, `i32::MIN`
and `i32::MAX` passed through, `u32::MAX` passed through, and two methods in one
contract. Three more are about the module rather than about the scalar set: a
body whose array local forces linear memory and the two extra exports that come
with it, a private function sitting ahead of the exported ones so the exported
indices are neither zero-based nor contiguous, and a `main` beside another
method, `main` being the one export the emitter reaches through a branch of its
own. Thirteen fixtures and twenty-three invocations in all; every invocation
returns the value the source says it must.

Both counts are pinned by
`contracts::the_measured_record_states_the_counts_this_table_holds`, and the
fixture directory, the compiled export sections and the case table are held to
each other at file granularity by
`contracts::every_fixture_file_is_invoked_by_a_case` and at method granularity
by `contracts::every_exported_method_is_invoked_by_a_case` — the second because
the first cannot see a method added to a fixture that already has a case.
Measured: appending one exported method to `u32_methods.inf` and no case for it
leaves every other test in this binary green and turns exactly that one red.

**Transparency.** Each fixture is answered twice — once by the *default* build
under the in-process `wasmtime` tier the rest of this repository executes with,
once by the *Stellar* build under the Soroban host — and the two answers are
compared to each other. The comparison is about the *values*: the default side
has no tag to read, so it reads its raw `i32` result as the type the case table
declares, and only the Stellar side is answering with a type of its own. They
agree on every invocation. That is the assertion this tier exists for: code
generation reads no target, so the two artifacts hold
the same computation, and equality of their answers is what says the wrapper
adds marshalling and nothing else. It is also what makes "prove the default
build, deploy the Stellar one" a claim about the deployed artifact.

**Negatives, on compiled contracts.**

| Call | Error | Message |
|---|---|---|
| `identity(I32Val(7))` where a `u32` is declared | `(WasmVm, InvalidAction)` | `VM call trapped: UnreachableCodeReached` |
| `identity(Void)` | `(WasmVm, InvalidAction)` | `VM call trapped: UnreachableCodeReached` |
| `add(U32Val(2))` — one argument short | `(WasmVm, UnexpectedSize)` | `VM call failed: Func(MismatchingParameterLen)` |
| `add(U32Val(2), U32Val(40), U32Val(1))` — one too many | `(WasmVm, UnexpectedSize)` | `VM call failed: Func(MismatchingParameterLen)` |

Each is paired with its control in the same test — `identity(U32Val(7))` and
`add(U32Val(2), U32Val(40))` both answer correctly — because the host is the
code under test and cannot be neutralized, so a refusal on its own could as
easily be a broken fixture as a real rule. The trap carries no discrimination:
the wrong-tag report is byte-identical whichever argument and whichever tag were
wrong.

**Proof that this tier has teeth.** One expected value was corrupted
(`return_only::answer` from 42 to 43) and the suite went red at
`contracts::every_fixture_round_trips_through_the_soroban_host`; the change was
reverted immediately. The transparency test stayed green under that corruption,
which is correct and worth knowing: it compares the two builds' *values* to each
other rather than to the table, so a corrupted expected value cannot move it.
A corrupted expected *type* can, because that is the one thing the default side
reads from the table in order to read its untagged result at all. The two tests
fail for different reasons, which is why both exist.

## Corrections to the written ABI description

1. **A two-result export is refused at module parse, not by `check_max_args`.**
   The one-result rule is usually attributed to `check_max_args`
   checking params *and* results. It does — but against `MAX_VM_ARGS` (32), so
   it would let two results through. What actually refuses them is wasmi's
   `wasm_multi_value(false)`: `Error(WasmVm, InvalidAction)`, message
   `"func type returns multiple values but the multi-value feature is not
   enabled"`. The rule is the same; its source is not.

2. **The reference contract is 114 bytes, not ~108.** A single exported
   `add(i64, i64) -> i64` with two `u32` unwraps, an `i32.add`, a `u32` wrap and
   the `contractenvmetav0` section encodes to 114 bytes under
   `wasm-encoder 0.254.0`. Pinned by
   `envelope::the_smallest_real_contract_uploads_and_invokes`.

3. **Protocol 20 acceptance is confirmed on 28.0.2**, the version pinned here; an
   earlier measurement used 27.0.1, so acceptance on the pinned version was an
   assumption until this test. Protocol 99 is refused with
   `Error(WasmVm, InvalidInput)` and `"contract protocol number is newer than
   host"`.

4. **The missing-metadata refusal is `Error(WasmVm, InvalidInput)`**, matching
   the written description, and it happens at *upload*, confirming that
   `upload_contract_wasm` builds a throwaway VM rather than deferring the check.

5. **The export-identity rules bite at CALL time, not at upload.** The
   the written description lists them beside the module-shape rules, which reads as an
   upload-validation set. The only export check at upload is the argument count:
   a 33-byte name, a `__` prefix and a hyphen all upload cleanly. They then fail
   at two *different* stages — the first and third in the `Symbol` constructor
   (no caller can name them), the second in the host's dispatch (`"can't invoke
   a reserved function directly"`). See [Export identity](#export-identity).

6. **Nothing in the host forces the original export out of the way; the
   duplicate-export-name rule does.** A leftover `(i32) -> i32` export beside a
   wrapped one uploads and does not disturb the wrapped export. What is refused,
   at upload, is two exports sharing a name.

7. **The `memory` export name is not enforced.** The written description says memory
   "must be exported under exactly the name `memory`". Measured: a memory
   exported as `mem` uploads and the contract still invokes. The host only looks
   the literal up, so getting the name wrong is a silent absence that a
   scalar-only contract never notices — a stronger reason to pin the name than a
   refusal would have been.

8. **A data segment past the declared initial memory is refused at upload**, as
   `Error(WasmVm, IndexBounds)` / `"Memory(OutOfBoundsAccess)"`, because the
   upload instantiates a throwaway VM.

9. **This file previously described the bool-normalization refusal with the
   wrong mechanism.** The `42` fixture produces a word whose low byte is 42,
   which is an invalid *tag*, not a `True` with a dirty body. The spec rule —
   `True` is tag 1 with body 0 — is exercised by the `257` fixture added beside
   it. Both refusals are `Error(Value, UnexpectedType)`, which is why the
   substitution went unnoticed.

## Host error surface, as measured

| Situation | Error | Message |
|---|---|---|
| No `contractenvmetav0` | `(WasmVm, InvalidInput)` | `contract missing metadata section` |
| Declared protocol 99 | `(WasmVm, InvalidInput)` | `contract protocol number is newer than host` |
| Declared protocol 20 | accepted | — |
| Float in a body | `(WasmVm, InvalidAction)` | `floating-point instruction disallowed` |
| Two-result export | `(WasmVm, InvalidAction)` | `func type returns multiple values but the multi-value feature is not enabled` |
| Returned word with nonzero minor bits | `(Value, UnexpectedType)` | `contract call failed` |
| Boolean wrapped without normalizing, inner `42` (invalid tag) | `(Value, UnexpectedType)` | `contract call failed` |
| Boolean wrapped without normalizing, inner `257` (`True` with a dirty body) | `(Value, UnexpectedType)` | `contract call failed` |
| Argument with the wrong tag | `(WasmVm, InvalidAction)` | `VM call trapped: UnreachableCodeReached` |
| Boolean argument with a tag above 1 | `(WasmVm, InvalidAction)` | `VM call trapped: UnreachableCodeReached` |
| Boolean argument that is `True` with a nonzero body | `(Value, InvalidInput)` | — (refused building the argument vector, on either route, before dispatch) |
| Compiled contract carrying linear memory, `memory` and `__stack_pointer` exported beside the wrapped method | accepted, invokes | — |
| Compiled method called with too few or too many arguments | `(WasmVm, UnexpectedSize)` | `VM call failed: Func(MismatchingParameterLen)` |
| Exported mutable `i64` global | accepted | — |
| Export name of exactly 32 bytes | accepted, invokes | — |
| Export name of 33 bytes | uploads; not a `Symbol` | `(Value, InvalidInput)`, no debug text |
| Export name containing `-` | uploads; not a `Symbol` | `(Value, InvalidInput)`, no debug text |
| `__`-prefixed export, invoked | `(Context, InvalidAction)` | `can't invoke a reserved function directly` |
| `(i32) -> i32` export beside a wrapped one | uploads; wrapped export unaffected | — |
| Calling that raw export with no arguments | `(WasmVm, UnexpectedSize)` | `VM call failed: Func(MismatchingParameterLen)` |
| Calling that raw export with one `Val` | `(WasmVm, UnexpectedType)` | `VM call failed: Func(MismatchingParameterType)` |
| Two exports sharing a name | `(WasmVm, InvalidAction)` | — (the event log repeats the code) |
| One-page memory exported as `memory`, data segment at 0 | accepted, invokes | — |
| The same memory exported as `mem` | accepted, invokes | — |
| Data segment past the declared initial memory | `(WasmVm, IndexBounds)` | `Memory(OutOfBoundsAccess)` |

The tag-mismatch trap is exactly what `soroban-sdk` produces, and the host
reports it identically whether the offending tag is `Void`, `U32Val` where an
`I32Val` was wanted, or anything else: the trap carries no discrimination, so a
caller learns only that the call trapped.

## Proof that these tests have teeth

Two neutralization runs against the marshalling sequences, each reverted
immediately. Both were run when this file pinned 20 tests; the counts are the
ones observed then, not rescaled to the larger suite it pins now:

- Wrapping the payload at bit 8 instead of bit 32, and dropping the boolean
  normalization: **5 of 20 tests fail**, including both scalar round trips and
  the byte pin.
- Reading the payload from bit 8 instead of bit 32, and inverting the tag guard
  from `i64.ne` to `i64.eq`: **9 of 20 tests fail**, including both trap tests.

Two tests deliberately stayed green under the dropped normalization —
`bool_round_trips_in_both_positions` and `u32_parameter_feeds_a_bool_return` —
because their inner functions return exactly 0 or 1 and so never exercise it.
That is the reason `bool_wrap_normalizes_a_nonzero_that_is_not_one` exists as a
separate test with an inner function returning 42.

The envelope tests cannot be neutralized the same way — the code under test is
the host, not this crate — so each negative there is paired with a positive that
differs in exactly the one property being measured, and both are asserted:

| Negative | Its control |
|---|---|
| 33-byte export name is not a `Symbol` | the 32-byte name is, and invokes |
| `__reserved` is refused at call | it converts to a `Symbol` first, so the refusal is dispatch, not naming |
| a raw `(i32) -> i32` export is uncallable | the wrapped export in the same module invokes |
| two exports of one name are refused at upload | the same two functions under different names upload |
| a data segment at 65534 is refused | the same module with the segment at 0 uploads *and* the loaded value proves the segment was written |

The bool-normalization pair carries its own control of a different kind: each
fixture asserts how this harness's decoder classifies the word it produces, so a
fixture that stopped exercising its mechanism fails before it reaches the host.
