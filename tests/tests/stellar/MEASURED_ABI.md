# Measured Val ABI

Every sequence below was assembled with `wasm-encoder`, uploaded to
`soroban-env-host` 28.0.2 with `testutils`, and invoked. Nothing here is derived
from reading the host's source: each byte column and each measured verdict is
what a passing test in `wrappers.rs`, `envelope.rs`, `contracts.rs` and
`spec.rs` executed — except the [CLI measurement](#cli-measurement) chapter, a
manual run with the external `stellar` CLI, recorded with its date and tool
versions — and the passages that cite the tooling's source instead say so where
they do. The byte columns are pinned by
`wrappers::measured_sequences_encode_to_the_recorded_bytes`, and the two
contract-spec runs by
`spec::the_add_and_tick_entries_are_the_runs_the_measured_record_annotates`,
so neither can go stale without a test going red.

Measured on 2026-09-12, macOS aarch64, `soroban-env-host = "=28.0.2"`,
`wasm-encoder = "0.254.0"`, host ledger protocol 28. The contract spec and meta
sections were measured on 2026-09-24, with `soroban-spec = "=28.0.0"` added.

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

## The contract spec section

`contractspecv0` is the machine-readable description of a contract's methods:
each method's name, each parameter's name and type, and what it returns. It is
what lets `stellar contract invoke … -- add --a 2 --b 40` turn command-line
flags into typed `Val`s. The host never reads it — measured below — and the
tooling does.

**What reads it.** The `stellar` CLI 28.0.0 reads a spec through two readers.
This paragraph and its table are read from source, not executed: the CLI's
source at tag `v28.0.0` (commit `300aaf69ab100536678bdb641428b06f06b318ea`, the
commit the installed CLI reports), and the published crates it locks:

| Command | Reader | What it decodes |
|---|---|---|
| `stellar contract invoke` against the deployed contract: the spec the call's arguments are built from (`get_spec.rs`, `get_remote_contract_spec`) | `Spec::new` (`soroban-spec-tools` 28.0.0, `cmd/crates/soroban-spec-tools/src/contract.rs`) | all three sections — `contractenvmetav0`, `contractmetav0` and `contractspecv0` — each gathered, repeats concatenated, and decoded as a run of whole entries at a depth of at most 500; any one that does not decode fails the command |
| `stellar contract info interface --wasm <file>` | the same `Spec::new` | the same |
| `stellar contract bindings rust --wasm <file>` (`commands/contract/bindings/rust.rs`), and the Soroban SDK's `contractimport!` | `soroban_spec::read::from_wasm` (`soroban-spec`, `src/read.rs`), through `soroban_spec_rust::generate_from_file` and `generate_from_wasm_with_options` respectively (`soroban-spec-rust` 28.0.0-rc.1; `soroban-sdk-macros` 27.0.6, the SDK release read from this machine's registry — the CLI itself locks 28.0.0-rc.1) | the first `contractspecv0` section |

`stellar contract invoke` has no `--wasm` option in 28.0.0. The `wasm` field
of its command (`cmd/soroban-cli/src/commands/contract/invoke.rs`) is
`#[arg(skip)]`, commented "For testing only", and `stellar contract invoke
--help` on the installed CLI lists no such flag. So `invoke` reads a spec
through `Spec::new` alone. `from_wasm` is asserted here because it is the
reader client code is generated from (`stellar contract bindings rust`, the
SDK's `contractimport!`), not because `invoke` runs it.

Both readers decode with the `stellar-xdr` 28.0.0 readers the host is built
on. The CLI 28.0.0 locks `soroban-spec` 28.0.0-rc.1 rather than 28.0.0, and the
two releases' `src/read.rs` and `src/shaking.rs` are byte-identical (compared
on 2026-09-24), so the reader this tier depends on is the one the CLI runs.
This tier runs `from_wasm` itself — `soroban-spec` is its dev-dependency — and
mirrors `Spec::new` with the same readers and the same depth limit rather than
taking `soroban-spec-tools` as a second dependency.

**What is asserted, for every compiled fixture** (`spec.rs`; the fixtures are
the thirteen of [Compiled contracts](#compiled-contracts), read off the same
case table):

| Test | Asserts |
|---|---|
| `spec::every_fixture_spec_decodes_with_the_cli_reader_into_its_descriptor` | `soroban_spec::read::from_wasm`, the reader client code is generated from, returns the entries. Every entry is `FunctionV0`, and the names in export order, each input's name and type, the outputs and the empty doc strings equal the export descriptor the rewrite consumed, kept from the one code generation run the contract was written from rather than recomputed beside it. The source-type-to-spec-type map is the test's own match, not the crate's table |
| `spec::the_default_build_of_every_fixture_carries_no_spec_and_no_meta_section` | the control: the default build, and the Stellar target's code generation output before the link and the rewrite, are each `FromWasmError::NotFound` and carry no spec or metadata section |
| `spec::every_fixture_spec_section_is_stellar_xdrs_own_encoding_of_its_descriptor` | the oracle: the section is byte for byte what `stellar-xdr`'s own writer produces for the descriptor's methods, one entry after another in export order |
| `spec::every_contract_decodes_the_way_the_cli_reads_a_deployed_contract` | the `Spec::new` mirror: all three sections decode; the environment metadata is one interface-version entry for protocol 20 with a zero pre-release, both spelled as literals rather than read from the crate, the contract metadata one entry, the spec the descriptor's methods |
| `spec::the_widest_method_lists_every_input_and_a_unit_method_lists_no_output` | `max_arity::widest` lists 32 inputs, `p0` to `p31` in order; `zero_parameter::tick` and `params_only::record` list no output |
| `spec::the_add_and_tick_entries_are_the_runs_the_measured_record_annotates` | the two runs below |
| `spec::every_contract_ends_with_the_spec_then_the_meta_then_the_environment_metadata` | the section order below, and that every contract's last thirty-two bytes are the ones [the metadata section](#the-metadata-section--exact-bytes) pins |

The oracle is what makes the hand encoder in `core/stellar-abi` a measured
thing: the expected bytes are built from the descriptor with the real
`ScSpecEntry` types and written by `stellar-xdr`, a derivation that shares
nothing with the encoder it checks.

**The two name bounds.** Both gates refuse a method name over 32 bytes and a
parameter name over 30, the widths of `SCSymbol` and of `string name<30>` on
`SCSpecFunctionInputV0`. Every other statement of those widths — the
constants in `core/stellar-abi` and `core/wasm-codegen`, the
`stellar_abi_parity` rows, the gates' unit tests — is a hand-typed copy of the
XDR. The parity rows pin both gates to their own spelled-out numbers — a
parameter name of 30 bytes admitted and one of 31 refused, a method name of 32
admitted and one of 33 refused — but nothing there ties those numbers to the
XDR field widths. No fixture comes near either width: the longest names there
are `passthrough` and `index`. Three tests hold the copies against the
original, with an inline contract rather than a fixture, so the counts of
[Compiled contracts](#compiled-contracts), thirteen fixtures and twenty-three
invocations, stay as they are:

| Test | Asserts |
|---|---|
| `spec::the_name_bounds_the_gates_refuse_against_are_the_widths_of_the_spec_fields` | `MAX_INPUT_NAME_BYTES` bytes of name fit an `ScSpecFunctionInputV0`'s `name` field and one byte more is `LengthExceedsMax`; likewise `MAX_EXPORT_NAME_BYTES` and `ScSymbol`. No width is typed: the field's own type decides |
| `spec::a_contract_whose_names_are_at_both_bounds_decodes_into_the_methods_it_declares` | a contract declaring `add` and a method whose name and whose parameter's name are each at the bound compiles, `from_wasm` decodes it into exactly the two methods declared, and its section is `stellar-xdr`'s own encoding of them |
| `spec::one_byte_over_either_bound_is_refused_by_both_gates_and_by_the_reader` | one byte over either bound, the source gate and the rewriter both refuse, each for that rule; and that contract's section, with one name respelled a byte longer, is refused whole by the reader (`LengthExceedsMax`), the `add` entry before it included. Respelled a byte shorter, it decodes: the control |

Together they fix every gate's bound to the reader's: the at-bound contract
shows neither gate refuses under the width, the over-bound refusals show
neither admits over it, and the field-width probe ties the constants they are
written against to the XDR. The failure they exist for is quiet: a name one
byte too wide uploads and invokes, because the host never parses the section,
and then every reader of the section refuses it whole.

### `add(a: u32, b: u32) -> u32` — sixty bytes

`u32_methods`' `add`, the reference contract's method. Its section describes
`identity` first, in forty-eight bytes, and `add` follows at offset 48:

```
00 00 00 00                       ;; SCSpecEntryKind = SC_SPEC_ENTRY_FUNCTION_V0
00 00 00 00                       ;; doc = "" (length 0, no bytes)
00 00 00 03 61 64 64 00           ;; name = "add", one byte of padding
00 00 00 02                       ;; inputs: 2
00 00 00 00                       ;;   doc = ""
00 00 00 01 61 00 00 00           ;;   name = "a", three bytes of padding
00 00 00 04                       ;;   type = SC_SPEC_TYPE_U32
00 00 00 00                       ;;   doc = ""
00 00 00 01 62 00 00 00           ;;   name = "b"
00 00 00 04                       ;;   type = SC_SPEC_TYPE_U32
00 00 00 01                       ;; outputs: 1
00 00 00 04                       ;;   SC_SPEC_TYPE_U32
```

As one run:

```
00 00 00 00 00 00 00 00 00 00 00 03 61 64 64 00 00 00 00 02 00 00 00 00 00 00 00 01 61 00 00 00 00 00 00 04 00 00 00 00 00 00 00 01 62 00 00 00 00 00 00 04 00 00 00 01 00 00 00 04
```

### `tick()` — twenty-four bytes

`zero_parameter`'s one method, and so its whole section:

```
00 00 00 00                       ;; SC_SPEC_ENTRY_FUNCTION_V0
00 00 00 00                       ;; doc = ""
00 00 00 04 74 69 63 6b           ;; name = "tick", no padding
00 00 00 00                       ;; inputs: 0
00 00 00 00                       ;; outputs: 0
```

```
00 00 00 00 00 00 00 00 00 00 00 04 74 69 63 6b 00 00 00 00 00 00 00 00
```

Each run is measured three ways by the one test: it is what `stellar-xdr`
writes for the method, it is what the compiled contract carries at that
method's place in its section, and `soroban_spec::read::parse_raw` decodes it
back into that one method. The same two runs are pinned inside the crate too
(`core/stellar-abi/src/spec.rs`), against its own encoder; the pin here is the
independent one.

**A unit return is no outputs, not `Void` — a correction to the request that
specified this section.** That request asks for four type codes, `Void` among
them. A method that returns nothing is described by an *empty* outputs vector
— the final zero count of the `tick` run — and `SC_SPEC_TYPE_VOID` (`2`) is
emitted by nothing. That is what the Soroban SDK writes: its `derive_spec_fn.rs`
has `ReturnType::Default => vec![]` (read from source, not executed, in
`soroban-sdk-macros` 27.0.6 — the SDK release in this machine's registry;
the CLI itself locks 28.0.0-rc.1). And it is what `soroban-spec` decodes
back from `zero_parameter` and `params_only` here. Three codes are emitted:
`SC_SPEC_TYPE_BOOL = 1`, `SC_SPEC_TYPE_U32 = 4`, `SC_SPEC_TYPE_I32 = 5`.

**Provenance.** Read from source, not executed: the entry kind, the type codes
and the two name bounds (`SCSymbol<32>` for a method, `string name<30>` for an
input) are published in `Stellar-contract-spec.x` in the stellar-xdr
repository at revision `9c9c145953e80990d6ff1ae3a6a973a0ce6d0694` — the
revision the `stellar-xdr` 28.0.0 crate vendors (its `xdr-version` file) and
the one `stellar --version` prints for the installed CLI 28.0.0. The oracle
writes with that crate, so a code that disagreed with the revision would fail
the byte comparison rather than be copied into it.

**Section order.** The rewrite appends `contractspecv0`, then `contractmetav0`,
then `contractenvmetav0` last.
`spec::every_contract_ends_with_the_spec_then_the_meta_then_the_environment_metadata`
asserts that these are the last three custom sections of every compiled
contract, each exactly once, in that order, and that every contract ends with
the thirty-two bytes [the metadata section](#the-metadata-section--exact-bytes)
pins. The host imposes no order on custom sections, so the order is a choice,
and the environment metadata stays last so that every contract still ends with
those bytes — the tail every pin and every sentence about how a contract ends
was written against.

**The host ignores both tooling sections.** Measured in `envelope.rs`, on the
reference `add` contract with the sections attached before the environment
metadata, in the rewrite's order:

| `contractspecv0` body | `contractmetav0` body | The tooling's reader | Upload and `add(7, 9)` |
|---|---|---|---|
| the sixty-byte `add` run | the `infver` entry | one method, `add` | uploads, answers `16` |
| seven `0xff` bytes | the `infver` entry | `from_wasm` refuses: `FromWasmError::Parse` | uploads, answers `16` |
| the sixty-byte `add` run | an entry of kind 1 (only `SC_META_V0 = 0` exists) | the entry does not decode | uploads, answers `16` |
| seven `0xff` bytes | an entry of kind 1 | neither decodes | uploads, answers `16` |

Each malformed body is first shown to be refused by the tooling's reader, so a
row cannot pass by carrying a body that happens to decode. The control is the
reference contract carrying neither section, which uploads and invokes in
`envelope::the_smallest_real_contract_uploads_and_invokes`. The tests are
`envelope::a_contract_carrying_well_formed_spec_and_meta_sections_uploads_and_invokes`
and
`envelope::a_contract_whose_tooling_sections_no_reader_accepts_still_uploads_and_invokes`.

So the host never parses either section, and nothing at upload would notice a
wrong one. Their correctness is the toolchain's alone to check, and the checks
are the tests above.

## The contract meta section

`contractmetav0` carries a contract's own key-value metadata. This toolchain
writes exactly one entry: `SCMetaV0 { key: "infver", val }`, where `val` is
`inference_stellar_abi::CONTRACT_META_TOOLCHAIN_VERSION`, the workspace version
(`0.0.1` today). For that version the section body is twenty-eight bytes:

```
00 00 00 00                       ;; SCMetaKind = SC_META_V0
00 00 00 06 69 6e 66 76 65 72 00 00   ;; key = "infver", two bytes of padding
00 00 00 05 30 2e 30 2e 31 00 00 00   ;; val = "0.0.1", three bytes of padding
```

The value half moves with the workspace version, so this tier derives it rather
than pinning it: `spec::every_fixture_meta_section_is_one_toolchain_version_entry`
decodes every compiled fixture's section, requires exactly one entry with the
key `infver` and the crate's published constant as its value — never a
spelled-out version — and byte equality with `stellar-xdr`'s own encoding of
that entry. It also requires the constant to equal this test crate's own
version, which is the same workspace version. The layout for `0.0.1` itself is
pinned in the crate, `core/stellar-abi/src/spec.rs`
(`the_meta_entry_is_kind_key_and_value`). Provenance: `enum SCMetaKind` and
`struct SCMetaV0` in `Stellar-contract-meta.x`, at the revision above.

The host does not read it: the malformed-meta rows above upload and
invoke. Tooling does, and the next two sentences are read from source,
not executed. The CLI's `Spec::new` decodes every entry of it, so a meta
section it could not decode would fail `stellar contract invoke` and
`stellar contract info interface` for the whole contract — which is what
`spec::every_contract_decodes_the_way_the_cli_reads_a_deployed_contract` guards.
And `soroban-spec` 28.0.0 (`src/shaking.rs`, `spec_shaking_version_for_meta`)
looks in those entries for the key `rssdk_spec_shaking`, an absent key meaning
spec-shaking version 1, so this toolchain's single `infver` entry changes
nothing for it.

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

Ten fixtures cover the scalar set in every position: parameters with no
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

**Every fixture's spec decodes and matches.** The same thirteen contracts are
read back the way the tooling reads them, in `spec.rs`: each one's
`contractspecv0` section decodes with `soroban-spec` into exactly the methods
the export descriptor lists — the names in order, every parameter's name and
type, the outputs — and is byte for byte `stellar-xdr`'s own encoding of those
methods. See [The contract spec section](#the-contract-spec-section).

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

## CLI measurement

Everything above is measured in process. This chapter is measured with the tooling the spec
section exists for: `stellar` CLI 28.0.0 (`stellar-xdr` 28.0.0, XDR revision
`9c9c145953e80990d6ff1ae3a6a973a0ce6d0694` — the revision `core/stellar-abi/src/spec.rs` cites),
reading four compiled fixtures from disk and then driving them on a local `stellar/quickstart`
network in Docker. Measured at 16:09 UTC on 2026-09-23 (the small hours of 2026-09-24 in this machine's time zone) against the `infc` built from
this repository at `103f521`. The commands are reproducible with the CLI installed and Docker
running. Commands and outputs are quoted verbatim; a line reading `…` marks lines elided, and
home-directory paths are shortened. Only the CLI's own lines are quoted, not the exit codes the
recording script logged beside them, and trailing spaces are dropped. It confirms the reader table
of [The contract spec section](#the-contract-spec-section), which was read from source:
`info interface` decodes the sections from a file, `invoke` builds its commands and their typed
flags from the deployed contract's spec, and a contract without one offers `invoke` no command at
all.

### Before: a contract without the sections

The same CLI on a contract built at `main` `ba7c68b`, before the spec and meta sections were
emitted, fixture `pub fn f(_: u32, b: u32) -> u32 { return b; }`:

| Command | Outcome |
|---|---|
| `stellar contract info env-meta --wasm ign.wasm` | ` • Protocol: v20` — the environment metadata was already right |
| `stellar contract info meta --wasm ign.wasm` | `❌ error: no meta present in provided WASM file` |
| `stellar contract info interface --wasm ign.wasm` | **the CLI panics**: ``called `Option::unwrap()` on a `None` value`` at `soroban-cli-28.0.0/src/commands/contract/info/interface.rs:61:35` |
| deploy, then `stellar contract invoke --id $ID --source alice --network local -- f --b 5` | `error: unexpected argument 'f' found` — the CLI builds its subcommands from the spec, so a contract without one offers no command at all |

Uploading and deploying such a contract succeeds; only the tooling is blind to it.

### After: reading the sections from disk

```text
$ stellar contract info interface --wasm u32_methods.wasm
ℹ️ Loading contract spec from file...
#[soroban_sdk::contractargs(name = "Args")]
#[soroban_sdk::contractclient(name = "Client")]
pub trait Contract {
    fn identity(env: soroban_sdk::Env, x: u32) -> u32;
    fn add(env: soroban_sdk::Env, a: u32, b: u32) -> u32;
}

$ stellar contract info meta --wasm u32_methods.wasm
ℹ️ Loading contract spec from file...
Contract meta:
 • infver: 0.0.1

$ stellar contract info env-meta --wasm u32_methods.wasm
ℹ️ Loading contract spec from file...
Contract env-meta:
 • Protocol: v20
```

The interface is rendered in the Rust SDK's trait form; the parameter names are the source's, and
`tick()` renders as `fn tick(env: soroban_sdk::Env);` — no return, which is what the empty
outputs vector decodes to. `mixed.wasm` renders `fn choose(env: soroban_sdk::Env, a: u32, b: i32,
c: bool) -> i32;`, all three type codes.

### After: deploying and invoking with named arguments

```text
$ stellar keys generate alice --network local --fund --overwrite
❗️ Overwriting identity 'alice'
✅ Key saved with alias alice in "…/.config/stellar/identity/alice.toml"
✅ Account alice funded on "Standalone Network ; February 2017"

$ stellar contract deploy --wasm u32_methods.wasm --source alice --network local
ℹ️ Uploading contract WASM…
ℹ️ Simulating transaction…
ℹ️ Signing transaction: c6f9677ee6e5ac3bb1cf010840d0080df3748a9c9c43eca33fed7ab9346c68ac
🌎 Sending transaction…
✅ Transaction submitted successfully!
ℹ️ Deploying contract using wasm hash 453064c596e976b4a4752ef917c2ae2618f033f3d5cc8e96168210745d4e2700
ℹ️ Simulating transaction…
ℹ️ Signing transaction: b63ae3aec331f4482c60070960c120856e0c55fdd9449335ae405d999c87b17a
🌎 Sending transaction…
✅ Transaction submitted successfully!
✅ Deployed!
CCRE5RTDJCATV2J4HLBZTKS5SNP2Q2S7EEJ3A572FZX7TSM47CDHDEE3
```

Every line `deploy` printed but the last went to standard error; the contract id is the one line
on standard output, and `$ID` below stands for it. The wasm hash it reports is the SHA-256 of
`u32_methods.wasm`, so the bytes deployed are the file's.

```text
$ stellar contract invoke --id $ID --source alice --network local -- --help
Usage: stellar contract invoke --id CCRE5RTDJCATV2J4HLBZTKS5SNP2Q2S7EEJ3A572FZX7TSM47CDHDEE3 --source alice --network local -- [COMMAND]

Commands:
  identity
  add
  help      Print this message or the help of the given subcommand(s)

Options:
  -h, --help  Print help

$ stellar contract invoke --id $ID --source alice --network local -- add --help
Usage Notes:
Each arg has a corresponding --<arg_name>-file-path which is a path to a file containing the corresponding JSON argument.
Note: The only types which aren't JSON are Bytes and BytesN, which are raw bytes

Usage: add [OPTIONS]

Options:
      --b <u32>
          Example:\n  --b 1

      --a <u32>
          Example:\n  --a 1

  -h, --help
          Print help (see a summary with '-h')

$ stellar contract invoke --id $ID --source alice --network local -- add --a 2 --b 40
ℹ️ Simulation identified as read-only. Send by rerunning with `--send=yes`.
42
```

`Example:\n  --b 1` is printed as it appears here: a backslash and an `n`, not a line break.

| Invocation | Answer |
|---|---|
| `u32_methods`: `add --a 2 --b 40` | `42` |
| `u32_methods`: `identity --x 4294967295` | `4294967295` |
| `bool_round_trip`: `negate --b true` / `--b false` | `false` / `true` |
| `zero_parameter`: `tick` | an empty line, exit 0 — the void return |
| `mixed`: `choose --a 1 --b -5 --c true` / `--c false` | `-5` / `0` |
| [`keep`](#a-leading-underscore-is-kept): `keep --_x 5 --y 37` / `keep --x 5 --y 37` | `42` / `42` |

Every answer for the four fixtures is the one the in-process tier measures for the same fixture
and arguments; `keep` is not one of its fixtures, and its answer is `5 + 37`. The CLI simulates a
read-only call rather than sending a transaction; `--send=yes` would submit it.

### After: what the CLI refuses before any host call

| Invocation | CLI outcome |
|---|---|
| `add --a 2` | `❌ error: Missing required argument 'b' of type u32 (unsigned 32-bit integer)`, then, first of three under `Suggestions:`, `- Add the argument: --b <value>` |
| `add --a two --b 40` | `❌ error: Failed to parse argument 'a': expected ident at line 1 column 2`, then `Context: Expected type u32 (unsigned 32-bit integer), but received: 'two'` |
| `add --a -1 --b 40` | `error: unexpected argument '-1' found` — the argument parser reads `-1` as a flag for a `u32` parameter, while `--b -5` for the `i32` parameter of `choose` is accepted, so a negative literal is admitted exactly where the spec's type admits one |

None of these reaches the network: the spec is what lets the CLI type the arguments, and a
mistyped one is refused with the parameter's name and declared type instead of the
undiscriminated `UnreachableCodeReached` trap a raw `Val` of the wrong tag earns from the host.

### A leading underscore is kept

A parameter's name reaches the spec exactly as the source spells it, a leading underscore
included. Measured later the same day with the same CLI (interface read at 19:06 UTC, the deployed
contract driven at 19:07 UTC on a fresh container, 2026-09-23), on this fixture compiled by the
`infc` built from this repository at `2db9810`, whose code generation and rewrite are those of
`103f521`:

```inference
// A leading underscore is kept as written: the spec input is `_x`, so the CLI flag is `--_x`.
pub fn keep(_x: u32, y: u32) -> u32 {
    return _x + y;
}
```

Its interface, read from disk; then, the contract deployed as `u32_methods` was above and `$ID`
standing for its id, its options and two invocations:

```text
$ stellar contract info interface --wasm underscore.wasm
ℹ️ Loading contract spec from file...
#[soroban_sdk::contractargs(name = "Args")]
#[soroban_sdk::contractclient(name = "Client")]
pub trait Contract {
    fn keep(env: soroban_sdk::Env, _x: u32, y: u32) -> u32;
}

$ stellar contract invoke --id $ID --source alice --network local -- keep --help
…
Usage: keep [OPTIONS]

Options:
      --_x <u32>
          Example:\n  --_x 1

      --y <u32>
          Example:\n  --y 1

  -h, --help
          Print help (see a summary with '-h')

$ stellar contract invoke --id $ID --source alice --network local -- keep --_x 5 --y 37
ℹ️ Simulation identified as read-only. Send by rerunning with `--send=yes`.
42

$ stellar contract invoke --id $ID --source alice --network local -- keep --x 5 --y 37
ℹ️ Simulation identified as read-only. Send by rerunning with `--send=yes`.
42
```

The spec records the name as written, and the CLI takes `--x` as well as `--_x`: `--x` is an alias
`--help` does not list. For a method with one such name, keeping the author's spelling costs a
caller nothing.

### Parameter names and the CLI's flag aliases

`--x` reaching `_x` is not a rule about underscores. Read from the CLI's source at v28.0.0
(`build_custom_cmd` in `cmd/soroban-cli/src/commands/contract/arg_parsing.rs`), every input of a
method gets two flags: `--<name>`, the one `--help` lists, and an alias `--help` does not list, the
name converted by `heck` 0.5.0's `to_kebab_case`. That conversion splits a name into words at every
character that is not a letter or a digit, between a lowercase letter, or a digit following one,
and a capital (`toAddr` is `to` and `Addr`), and before the last capital of a run followed by a
lowercase letter (`HTTPServer` is `HTTP` and `Server`), so `Amount` stays one word; it lowercases
each word and joins the words with `-`: `_x`, `__x`, `x_` and `X` all have the alias `--x`,
`Amount` has `--amount`, and `to_addr`, `_to_addr` and `toAddr` all have the alias `--to-addr`.
The inputs are added to the command in the iteration order of a `HashMap`, which changes from one
run to the next, and `clap_builder` 4.6.0, which that source locks, resolves a flag to the first
argument declaring it. So when one input's name is another input's alias, the flag `--help` lists
for the first may reach the second instead, and which of the two it reaches changes from call to
call.

A method declaring both `x` and `_x` shows it. Measured at 19:43 UTC on 2026-09-23 with the same
CLI on a fresh container, on this fixture compiled by the `infc` built from this repository at
`2db9810` plus the uncommitted closing fixes; code generation and the rewrite are unchanged since
`103f521`:

```inference
// Both spellings in one method: the spec records `x` and `_x` as two inputs.
pub fn both(x: u32, _x: u32) -> u32 {
    return x * 10 + _x;
}
```

Its interface, read from disk; then, the contract deployed and `$ID` standing for its id, its
options and three invocations:

```text
$ stellar contract info interface --wasm collide.wasm
ℹ️ Loading contract spec from file...
#[soroban_sdk::contractargs(name = "Args")]
#[soroban_sdk::contractclient(name = "Client")]
pub trait Contract {
    fn both(env: soroban_sdk::Env, x: u32, _x: u32) -> u32;
}

$ stellar contract invoke --id $ID --source alice --network local -- both --help
…
Usage: both [OPTIONS]

Options:
      --x <u32>
          Example:\n  --x 1

      --_x <u32>
          Example:\n  --_x 1

  -h, --help
          Print help (see a summary with '-h')

$ stellar contract invoke --id $ID --source alice --network local -- both --x 4 --_x 2
error: the argument '--_x <u32>' cannot be used multiple times

Usage: both [OPTIONS]

For more information, try '--help'.

$ stellar contract invoke --id $ID --source alice --network local -- both --_x 2 --x 4
error: the argument '--_x <u32>' cannot be used multiple times

Usage: both [OPTIONS]

For more information, try '--help'.

$ stellar contract invoke --id $ID --source alice --network local -- both --x 4
❌ error: Missing required argument '_x' of type u32 (unsigned 32-bit integer)

Suggestions:
- Add the argument: --_x <value>
- Or use a file: --_x-file-path <path-to-json-file>
- Check the contract specification for required arguments
```

In the first two calls `--x` reached `_x`, so `--_x` repeated it, and in the third it reached `x`,
leaving `_x` missing (the CLI reports the first missing input in spec order). The spec describes
the method correctly, and none of the three calls invoked it.

Which pairs of names break a method, and which do not, was measured on five more methods, each
declaring two names that share an alias. Measured at 23:28 UTC on 2026-09-23 with the same CLI on
a fresh container, on a fixture compiled by the `infc` built from this repository at `ab8beb7`,
which refused none of these names, and deployed. The recording script's log is quoted from its
first line to its last invocation, with the container, funding and deployment lines between the
fixture and the first method elided and trailing spaces dropped. The script wrote `...` for
`stellar contract invoke --id $ID --source alice --network local` and kept only the option lines
of each method's `--help`; it then called each method eight times through the flags each
invocation line names, and summed the eight calls in that one line, quoting one failure's error:

```text
# CLI alias measurement, stellar-cli 28.0.0, 2026-09-23T23:28:06Z; fixture alias.inf compiled by the pre-rule infc built at ab8beb7
pub fn f1(_x: u32, __x: u32) -> u32 { return _x * 10 + __x; }
pub fn f2(x: u32, X: u32) -> u32 { return x * 10 + X; }
pub fn f3(to_addr: u32, _to_addr: u32) -> u32 { return to_addr * 10 + _to_addr; }
pub fn f4(to_addr: u32, toAddr: u32) -> u32 { return to_addr * 10 + toAddr; }
pub fn f5(x: u32, x_: u32) -> u32 { return x * 10 + x_; }

…

### f1
$ ... -- f1 --help (options only)
      --__x <u32>
      --_x <u32>

### f2
$ ... -- f2 --help (options only)
      --X <u32>
      --x <u32>

### f3
$ ... -- f3 --help (options only)
      --to_addr <u32>
      --_to_addr <u32>

### f4
$ ... -- f4 --help (options only)
      --to_addr <u32>
      --toAddr <u32>

### f5
$ ... -- f5 --help (options only)
      --x_ <u32>
      --x <u32>

### invocations (8 trials each)
  f1 long flags: f1 --_x 1 --__x 2 -> succeeded 8/8
  f1 alias+long: f1 --x 1 --__x 2 -> succeeded 4/8 ; a failure: error: the argument '--__x <u32>' cannot be used multiple times
  f2 long flags (x, X): f2 --x 1 --X 2 -> succeeded 2/8 ; a failure: error: the argument '--X <u32>' cannot be used multiple times
  f3 long flags: f3 --to_addr 1 --_to_addr 2 -> succeeded 8/8
  f3 alias+long: f3 --to-addr 1 --_to_addr 2 -> succeeded 4/8 ; a failure: error: the argument '--_to_addr <u32>' cannot be used multiple times
  f4 long flags: f4 --to_addr 1 --toAddr 2 -> succeeded 8/8
  f5 long flags (x, x_): f5 --x 1 --x_ 2 -> succeeded 3/8 ; a failure: error: the argument '--x_ <u32>' cannot be used multiple times
```

Called through the flags `--help` lists, each name as written, three of the five methods succeeded
in every call and two failed some of the calls. `f2` (`x` beside `X`) failed six calls of eight and
`f5` (`x` beside `x_`) five, the failure each line quotes a repeated option, as `both`'s were: in
each, one name is the other's alias exactly — `x` is the alias of `X` and of `x_` — so the flag
`--x`, listed for `x`, is claimed by both inputs. In `f1` (`_x` beside `__x`), `f3` (`to_addr`
beside `_to_addr`) and `f4` (`to_addr` beside `toAddr`) the two aliases agree, `--x` or
`--to-addr`, but neither name is the other's alias, so each input keeps a listed flag no other
input claims. Their shared alias, which `--help` does not list, is claimed by both inputs and is
no more reliable than `--x` beside `--X`: `f1` called as `--x 1 --__x 2`, and `f3` as
`--to-addr 1 --_to_addr 2`, each succeeded in four calls of eight.

The same was measured of the argument parser alone on 2026-09-24, with a replica of the arguments
`build_custom_cmd` declares for each input — the flag `--<name>` with the alias `--<name in kebab
case>`, beside its hidden `--<name>-file-path` companion, the inputs added in `HashMap` order —
compiled in release mode against the `clap` 4.6.1, `clap_builder` 4.6.0 and `heck` 0.5.0 the CLI's
lock file pins, and each call parsed 400 times. This measures `clap`, not the CLI, and involves no
contract. With `u32` inputs, `x` beside `_x`, `X`, `x_` or `_X_`, and `ab` beside `Ab`, failed
alike: the two flags together were refused as a repeated option in 178 to 189 of the 400 runs,
about half, and the flag that is also the other name's alias (`--x 1`, `--ab 1`) alone reached the
other parameter in 168 to 203 of them. That "about half" is the replica's figure; the CLI itself,
driven against a deployed contract above, refused both of the two calls that passed `--x` and
`--_x` together, and failed six calls of eight for `x` beside `X` and five of eight for `x` beside
`x_`. With `bool` inputs, whose flag takes no value and defaults to `false`, `--x` alone reached
the other parameter in 151 to 169 runs of 400 for `x` beside `_x`, `X` or `x_`, and parsing
succeeded with the other parameter `true` and `x` at its default, `false`. `_x` beside `__x` or
`x_`, `a_b` beside `aB`, and `x1` beside `x_1` took every flag to its own parameter in all 400
runs.

Both gates therefore refuse a method in which one parameter's name is another parameter's alias,
or two parameters' names are identical, and admit one whose aliases merely agree; a lone `_x`,
whose alias is then its own, is admitted too. The rule covers the flags `--help` lists; this
chapter measures nothing of an input's hidden `--<name>-file-path` companion or of `clap`'s
built-in `--help`, so a parameter named `help`, or `x` beside `x_file_path`, whose alias,
`--x-file-path`, is the long flag of `x`'s companion, is outside what it records. Both gates
compute the alias with one function, `inference_wasm_codegen::stellar_cli_flag_alias`, a
transcription of `heck`'s conversion, and find the pair with one more,
`inference_wasm_codegen::stellar_flag_collision`. The rule is pinned by
`a_name_that_is_another_parameters_flag_alias_is_refused`,
`names_whose_second_flags_are_no_other_parameters_name_are_admitted` and
`a_duplicate_parameter_name_is_refused_as_a_collision` in `core/stellar-abi/src/rewrite.rs`;
`every_name_that_is_another_parameters_flag_alias_is_refused` and the two tests of the same names
as the rewriter's last two in `core/wasm-codegen/src/lib.rs`;
`the_flag_alias_is_the_name_in_kebab_case` in `core/wasm-codegen/src/stellar_cli.rs`; and the rows
of `tests/tests/stellar_abi_parity.rs` that hold the two gates to one answer. `cli_flags.rs` in
this directory holds the transcription equal to `heck` 0.5.0 itself over a table of names and
every fixture's parameter names (`the_flag_alias_is_hecks_kebab_case_of_every_name`), and holds
both gates' verdict on each of the six methods above equal to what the CLI did with it
(`each_measured_method_is_refused_exactly_when_the_cli_failed_to_call_it`): the three whose calls
all succeeded are admitted, and `both`, `f2` and `f5` are refused.

### What the two sections cost

Measured on the four contracts above (each section's size field: its name and payload, not
counting the one-byte section id and the one-byte size, so a whole section is 2 bytes more: spec
41–125, meta 45, environment metadata 32):

| Contract | Total | `contractspecv0` | `contractmetav0` | `contractenvmetav0` |
|---|---|---|---|---|
| `zero_parameter` (`tick()`) | 228 | 39 | 43 | 30 |
| `bool_round_trip` (`negate(b)`) | 289 | 63 | 43 | 30 |
| `mixed` (`choose(a, b, c)`) | 377 | 95 | 43 | 30 |
| `u32_methods` (`identity(x)`, `add(a, b)`) | 479 | 123 | 43 | 30 |

The spec section's size field is 15 bytes for the section's name — a one-byte length and the
fourteen bytes of `contractspecv0` — plus one entry per method: 16 bytes plus the method name's
4-byte length and its bytes padded to a multiple of 4 (24 for `tick`), then 16 more per one-letter
parameter and 4 more for a returned scalar; `identity(x) -> u32` is 48 and `add(a, b) -> u32` is
60. The meta section's size field is 43 for the version string `0.0.1` (45 as a whole section).
Both are custom sections, so a host that does not look for them pays nothing but the upload bytes.

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
| Well-formed spec + meta sections | accepted, invokes | — |
| Arbitrary-bytes spec section | accepted, invokes | — |
| Malformed `contractmetav0` entry | accepted, invokes | — |
| Arbitrary-bytes spec section + malformed meta entry | accepted, invokes | — |

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

The spec and meta tests were neutralized on 2026-09-24, when this binary held
50 tests; each change was reverted immediately, and `core` was shown
byte-identical to its commit afterwards:

- Dropping the string padding from the crate's XDR encoder: **6 of 50 fail** —
  every `spec` test that reads a section's bytes. `soroban_spec::read::from_wasm`
  refuses twelve of the thirteen specs outright; `zero_parameter`'s `tick`
  needs no padding and still decodes. The section-order test reads only names
  and stays green, as it should.
- Flipping the crate's `SC_SPEC_TYPE_U32` from 4 to 5: **5 of 50 fail**, and the
  decoding test names exactly the nine fixtures that carry a `u32`. The meta
  and order tests stay green.
- Mapping `u32` to `I32` in the test's own type map instead: **4 of 50 fail**,
  naming the same nine fixtures — so the expectation is read from the
  descriptor, not from the section it is compared with.
- Renaming the `contractspecv0` section the harness's builder writes, or the
  `contractmetav0` one: **2 of 50 fail** each — both tooling-section envelope
  tests, at the check that the tooling reads the section the contract was
  given, before any host call.

The three name-bound tests were added later the same day, when the binary held
53 tests, and neutralized the same way, each change reverted and `core` again
shown byte-identical:

- Moving every parameter-name bound from 30 to 32 — the constant in
  `core/stellar-abi` and the one in `core/wasm-codegen` together, the slip
  those tests exist for: **3 of 53 fail**, all three. The at-bound contract
  compiles through both gates and `from_wasm` then refuses its spec with
  `LengthExceedsMax`, which is the failure a user would meet. One
  `stellar_abi_parity` test fails too, but only because its rows spell 30 and
  31; had they moved with the constants, these three would be the only red.
- Moving every method-name bound from 32 to 33 the same way: **3 of 53 fail**,
  the same three, with the same one `stellar_abi_parity` test for the same
  reason.
- Loosening the source gate's parameter-name bound alone, to 31: **1 of 53
  fails**, the over-bound test, whose source gate admits the 31-byte name (and
  two of `stellar_abi_parity`'s three).
- Loosening the rewriter's parameter-name check alone by one byte, its constant
  untouched: **1 of 53 fails**, the over-bound test (and one of
  `stellar_abi_parity`'s). The crate's own `assert!` in its entry builder
  panics first, in every build, meeting a name wider than the constant.
- Dropping the string padding again: **8 of 53 fail** — the six above and the
  two name-bound tests that read a section. The field-width test reads none and
  stays green.

The `cli_flags` tests were neutralized on 2026-09-24, when this binary held 58 tests. Each change
was one line, reverted by restoring that line, and the tree was shown byte-identical to its state
before:

- Making `stellar_cli_flag_alias` return the name unchanged, so that only identical names claim one
  flag: **3 of 58 fail** — the `heck` comparison, which lists every name it disagrees on, and both
  verdict tests, which find `both`, `f2` and `f5` admitted. The test of what the record quotes
  reads no alias and stays green. Outside this binary the same change turns red eight unit tests of
  the two gates, the source-level refusal test in `tests/src/codegen/wasm/stellar_gate.rs`, two of
  `stellar_abi_parity`'s four tests and the `infc` matrix of inadmissible contract shapes, while
  each gate's identical-name test and every admitted row stay green.
- Skipping the source gate's collision rule, or the rewriter's alone: **2 of 58 fail** either way,
  the two verdict tests. Each also turns `stellar_abi_parity`'s agreement test red, which is what
  holds the two gates to one rule.
