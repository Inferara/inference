# Compilation Targets

## Compilation Matrix

**`non_det_operations`** = { `spec`, `uzumaki`, `forall_block`, `assume_block`, `exists_block`, `unique_block` }

`compile` mode produces a `.wasm` binary (executable or library). `proof` mode produces a `.v` Rocq file (via `wasm_to_v`). Non-deterministic operations can only appear inside `spec` blocks. In compile mode, `spec` nodes are stripped (they have no runtime meaning). In proof mode, all code including `spec` blocks is emitted.

| Option | Mode | Profile | Has `non_det_operations` | Behavior |
|--------|------|---------|--------------------------|----------|
| 1 | `compile` | `debug`   | no  | Compile with the chosen `Target`, recording `OptLevel::O0` |
| 2 | `compile` | `release` | no  | Compile with the chosen `Target`, recording its default `OptLevel` |
| 3 | `compile` | `debug`   | yes | Exclude `spec` nodes from codegen, then compile as `Option 1` |
| 4 | `compile` | `release` | yes | Exclude `spec` nodes from codegen, then compile as `Option 2` |
| 5 | `proof`   | *(fixed)* | no  | Identical to `Option 2` — no spec code to preserve, output matches compile mode release |
| 6 | `proof`   | *(fixed)* | yes | Spec functions lowered to vanilla WASM, in source order. Execution functions byte-identical to `Option 2`'s. |

**`compile` mode**: Produces production binaries. Non-det `spec` nodes are stripped from codegen since they have no runtime meaning. The output can be the verification target — the artifact whose behavior is proven correct by Rocq proofs.

**`proof` mode**: Emits a single WASM module for `wasm_to_v` Rocq translation, preserving the specification code that compile mode strips. The non-deterministic constructs do not survive into the module: a `forall`-quantified (or plain) spec function becomes an `hassert` obligation and is omitted from the module record altogether, while an `exists`- or `unique`-quantified one is retained with a vanilla body — each scalar `@` becomes a hidden trailing choice parameter, and each `assume`/`assert` a trap-on-false filter. What proof mode does preserve is source order and shape: statements lower in the order written and the compiler applies no optimization pass to reshape them, so a retained body reads against its source and there is no optimization barrier to insert. Execution functions are byte-for-byte identical to what compile mode's release profile emits for the same source, so Rocq proofs cover the artifact that actually ships. If the source has no `non_det_operations`, proof mode output is identical to compile mode release output (`Option 5` = `Option 2`). The target is always `Wasm32`: every other target supports `compile` mode only, because the custom 0xfc instructions proof mode is made of are decodable by no runtime but our own tooling. Build profiles (`debug`/`release`) do not change proof mode's output — only the `OptLevel` value it records, which today changes no emitted byte either way (see Appendix A below).

The contract between the generated `.wasm` binary, the per-spec function index map carried alongside (or embedded as the `inference.spec_funcs` custom section), and the Rocq predicates the generated `.v` file depends on is documented in [`core/wasm-to-v/ROCQ_CONTRACT.md`](../../core/wasm-to-v/ROCQ_CONTRACT.md).

### Selecting a mode at the CLI

Pass `--mode {compile,proof}` to either CLI: `infs build path/to/file.inf --mode proof` or `infc path/to/file.inf --mode proof`. Equivalently, `infc -v` (emit Rocq) implies `--mode proof` unless `--mode compile` is also passed; mirror-rule: `--mode proof` implies `-v`. Without either flag, the default is compile mode.

For project-aware builds — `Inference.toml`, project discovery, and the `infs build`/`run` workflow that resolves a mode from the manifest — see [Projects and the infs Toolchain](projects-and-the-infs-toolchain.md).

Both spellings of a Rocq request are refused at `--target stellar`: `--mode proof` (and a bare `-v`, which implies it) because the target cannot decode the instructions proof mode emits, and `--mode compile -v` because the `.v` would describe the module *before* the value-ABI rewrite while the `.wasm` beside it is the module after. See [Stellar](#stellar) below.

At `--target spacewasm` only the first of those is refused. `--mode proof` and a bare `-v` fail for the same reason as at Stellar, but `--mode compile -v` **succeeds** and writes the `.v` a `wasm32` compile-mode build writes, character for character — there is no rewrite for the translation to run ahead of, because the two builds are the same module. See [SpaceWasm](#spacewasm) below.

| Property | Value | Rationale |
|----------|-------|-----------|
| Spec function lowering | Structural 1:1 from source | Rocq readability — no optimizer runs to disturb it |
| Execution function bytes | Byte-identical to compile mode release | Proofs must cover the actual deployed code, not a differently-compiled variant |
| Target | Wasm32 only | Custom 0xfc intrinsics required |
| Runtime guards | Emitted in both modes | A `+`, `-`, `*` or unary `-` that traps on overflow traps in the module a proof is written about and in the one that ships, as the bounds, narrow-division and enum-tag guards already did; nothing may gate an emitted instruction on the mode |
| Name section | Always emitted | Rocq identifiers require function/local names |
| DWARF | Never | Not useful for formal verification |
| wasm-opt | Never applied to proof-mode output | `[build.wasm-opt]` (`infs`'s opt-in Binaryen post-build step) explicitly skips proof/`-v` builds — Binaryen has no lowering for the non-det opcode family a spec function may carry |
| Code inclusion | All (spec + executable) | Spec code defines properties; execution code is the verification target |
| Host imports | None: `infc` refuses every build that writes a `.v` for a program binding `use { f } from host::<module>;`, at every target | The body is the embedder's, outside the artifact a proof is written about, and the translation has no way to state an assumption about it; see [Host Imports](external-functions-and-wasm-linking.md#host-imports) |
| No non_det output | Identical to compile mode release | Nothing to formalize structurally |
| Determinism | Bitwise reproducible | Same source must produce same `.v` file |

## Verification Scenario: External Module Linking

Inference verifies the **final artifact** — the deployed WASM module. This module can be:
1. Produced by `infc` from `.inf` source code (compile mode, spec stripped)
2. A WASM module built elsewhere (e.g., a Rust cryptographic library compiled to WASM)

In the linking scenario, a user:
1. Compiles their library to WASM (e.g., `my_crypto.wasm` from Rust)
2. Writes an `.inf` specification that imports external functions from the module
3. Writes `spec` blocks with assertions: `assert(my_crypto_function(input) == 0)`
4. `infc` in proof mode links the external module with the compiled spec into a unified WASM module
5. The unified module is translated to Rocq (`.v`) by `wasm_to_v`
6. The user writes Rocq proofs establishing properties about the external function's behavior

The external artifact remains as-is (however it was built, and by whatever compiler produced it). The `spec` code requires structural identity for Rocq readability, which the compiler gives it by lowering it 1:1 from source rather than by withholding an optimizer that does not otherwise run. Execution code — whether from Inference source or external modules — is exactly the bytes that will run, so Rocq proofs cover the actual deployed artifact.

The language-level constructs that import external functions (`external fn`, `use … from`) are documented in [External Functions and WASM Linking](external-functions-and-wasm-linking.md); the static merge that folds them into the verified artifact — feasibility tiers and the Tier-B provenance proof — in [The WASM Linker](the-wasm-linker.md).

## Targets

Target parameters are **locked per target variant** and cannot be overridden in `Inference.toml`. The only user-facing configuration is target selection and build profile (debug/release) for compile mode.

### Target::Wasm32 (default)

General-purpose WASM target with custom non-deterministic instruction support for specs. Used in both `compile` and `proof` modes. WebAssembly is generated directly via `wasm-encoder`.
Purpose: general WASM execution and verification of Inference code.

| Setting | Value |
|---------|-------|
| Target | `wasm32-unknown-unknown` |
| Instruction set | WebAssembly 1.0. `proof` mode may additionally emit the custom 0xfc non-deterministic instructions, which only this target permits, and `--wasm-features bulk-memory` opts into `memory.copy`/`memory.fill` |
| WASM proposals the module uses | `mutable-globals` only — a module with linear memory exports its mutable `__stack_pointer` global — unless a build opts into `bulk-memory` above |
| Recorded `OptLevel` (compile) | `O3` under `release`, `O0` under `debug` — no optimization pass currently acts on either |
| Proof mode output | Byte-identical to compile mode's, plus structurally 1:1 spec functions |

### Stellar

Produces a deployable Soroban smart contract for the Stellar network, selected
with `infc --target stellar` or `[build] target = "stellar"` in an
`Inference.toml`.

| Setting | Value | Source |
|---------|-------|--------|
| Target | `wasm32-unknown-unknown` | The only module shape code generation produces |
| Instruction set | WebAssembly 1.0, with no opt-in available | `Target::permits_bulk_memory()` is `false`, so the one post-MVP family the compiler can emit is unreachable here |
| WASM proposals the module uses | `mutable-globals` only — a module with linear memory exports its mutable `__stack_pointer` global — and no instruction outside WebAssembly 1.0 | See "What the compiler emits" below |
| Compilation mode | `compile` only | Proof mode emits the custom 0xfc non-deterministic instructions, which no Soroban host decodes |
| Recorded `OptLevel` (compile) | `Oz` under `release`, `O0` under `debug` — no optimization pass currently acts on either | `Oz` is the target's `default_opt_level`, matching the toolchain convention for the platform |
| Floats | Impossible — no float instruction can be emitted, at any target | The language has no floating-point type: `SimpleTypeKind` (`core/ast`) admits the unit type `()`, `bool` and the eight integer widths and nothing else |

#### The value ABI

A Soroban host does not call a contract method with its declared WebAssembly
parameters. Every method takes and returns `Val`: a 64-bit word whose low byte is
a tag naming what the rest of the word holds. `add(a: u32, b: u32) -> u32`
therefore reaches the host as `(i64, i64) -> i64`, not as `(i32, i32) -> i32`.

The compiler does not emit that convention. Code generation is target-blind — the
module it produces for Stellar is the module it produces for `wasm32`, byte for
byte — and the value ABI is applied afterwards, to the linked artifact, by a
rewriter. Per exported method the rewriter appends one wrapper function that
unwraps each argument, calls the original body, and wraps the result; the export
section is retargeted onto the wrappers, and every existing function keeps its
index and its bytes. That ordering is the whole point of the design: **the module
you prove is the module you deploy, minus a marshalling shell that contains no
arithmetic, no memory access and no control flow beyond one guard per argument.**

An argument whose tag is wrong traps. It is worth knowing what that looks like
from the caller's side: a wrong tag, a wrong argument type and an out-of-range
boolean all produce the byte-identical host report `WasmVm, InvalidAction` /
`"VM call trapped: UnreachableCodeReached"`. A caller cannot tell which argument
was wrong, or that the problem was a type at all. The build log is where that
information is — `infc` prints one summary line naming each method and its value
arity:

```text
Stellar contract: add/2, flip/1, nothing/0; env protocol 20; 558 bytes
```

That size includes the three contract sections the rewriter appends, not just
the wrappers.

#### What an exported function may declare

An exported function is a contract method, and a contract method is confined to
the scalars the convention encodes without a host object:

| Position | Admitted |
|----------|----------|
| Parameter | `u32`, `i32`, `bool` — at most 32 of them |
| Return | `u32`, `i32`, `bool`, or nothing |
| Name | 1–32 bytes of `[A-Za-z0-9_]`, not `__`-prefixed |
| Parameter name | written (not `_`), 1–30 bytes |

Everything else is refused at code generation, before any byte exists, naming the
function and the offending element. The refusals differ because the reasons do:

- **A 64-bit integer** (`u64`, `i64`): *"…the host's word is 64 bits wide and
  spends part of it on a tag, so no 64-bit value fits in one, and it travels as a
  host object built through host functions this target refuses to import, their
  call convention not being bound here yet — issue #324."*
- **A narrow integer** (`u8`, `i8`, `u16`, `i16`): *"…the host has no narrower
  word, so what an exported `u8` does with a caller-supplied 300 is a language
  question rather than a layout one, and it is not settled. Widen the declaration
  to `u32` or `i32`."* This one is **not** blocked on host objects and never will
  be.
- **A struct, array or enum**: *"A compound value crosses the contract boundary
  as a host object, which a contract has to build and read through host functions
  it imports; this target refuses a host binding, because the Soroban host-call
  convention is not bound here yet."*
- **A compound return** additionally explains the mechanism it cannot use: the
  caller would receive it through a hidden pointer into linear memory, and a
  contract method hands back one host word and has no pointer to give.
- **A name over 32 bytes**: *"…the host holds a method name of at most 32. A
  longer one is not expressible by any caller, so the method would upload and be
  unreachable — rename it."*
- **A module with no exported function**: refused outright — it would upload with
  no method to call.
- **A parameter written `_`**: *"…parameter `N` is unnamed ('_'). A contract
  method's parameters are named: `stellar contract invoke` passes each one as
  `--<name>`, and the contract's spec section records that name. Name the
  parameter."*
- **A parameter name over 30 bytes**: *"…parameter `N` has a name of `L` bytes,
  and a contract method's parameter name is at most 30: the contract's spec
  section records each name in a field that wide, and `stellar contract invoke`
  passes the parameter as `--<name>`. Shorten the name."* (`N` is the
  parameter's one-based position, and its name where it has one; `L` is the
  name's length in bytes.)

Within one exported function, both name rules run after its type and return
rules, so a type or return refusal on that function is reported before a name
refusal on it, and each name rule runs over every parameter before the next
begins. Functions are checked one at a time in export order, and the first
refusal ends the build.

`main` is a contract method like any other: it is exported under its own name and
held to the same rules. There is no entry point to a contract, so nothing
distinguishes it.

Only the *entry file's* top-level `pub fn` declarations are exported, so an
inadmissible type inside a private function, a method, or an imported file is not
the gate's business.

#### The custom sections

A contract carries three contract sections beyond the ones every build has —
the `name` section, and `inference.checked` where code generation wrote one —
and each is written for a different primary reader.

**`contractenvmetav0`.** Every contract carries this section, declaring the
environment protocol it was built for. A module without one is refused at
upload by every Soroban host there has ever been, so the rewriter appends it
unconditionally, last in the module, as 32 fixed bytes.

The declared protocol is **20**, with pre-release 0. The rule a host applies is
that the declared protocol must not be *newer* than the ledger's, and that a
pre-release of 0 is required below it; 20 is the protocol Soroban arrived in, so
a contract declaring it is uploadable to every Soroban network that has ever
existed. Declaring today's protocol instead would make the artifact
un-uploadable to any network still behind it.

That constant is not permanent. It is the *minimum* the contract needs, and it
must become the maximum of the minimum protocols of the host functions the
contract imports once host imports are admitted at this target (issue #324).
Until then this target refuses every host import, so a contract imports nothing
and the floor is the whole answer.

**`contractspecv0`.** The XDR description of every method the contract exports:
each method's name, each input's name and type, and what it returns. Read by
**tooling**, never by the host: it is what `stellar contract invoke` reads to
turn `--a 2 --b 40` into typed `Val` arguments, what `stellar contract info
interface` renders, and what `stellar contract bindings rust` reads to write a
client. A contract carrying no such section still uploads and is callable by the
host, and so is one carrying a malformed one — the host never looks for it; the
CLI, by contrast, offers such a contract no command. It is the tooling's
readers, named below — `Spec::new` and `soroban_spec::read::from_wasm` — that
refuse a malformed section. Its body is one `SCSpecEntry` per exported method,
in export order, each the `FunctionV0` arm with an empty doc string, the
method's name, one input per parameter — an empty doc string, the parameter's
name exactly as the source spells it (a leading underscore included), and its
type code — and the outputs. The type codes are `SC_SPEC_TYPE_BOOL = 1`,
`SC_SPEC_TYPE_U32 = 4` and `SC_SPEC_TYPE_I32 = 5`, published in
`Stellar-contract-spec.x` at the stellar-xdr revision
`9c9c145953e80990d6ff1ae3a6a973a0ce6d0694` (`core/stellar-abi/src/spec.rs`
records the provenance beside each constant). A method that returns nothing is
described by an **empty** outputs vector, not by a fourth code,
`SC_SPEC_TYPE_VOID = 2`: that is what `soroban-sdk` writes for a function with
no return, and what the CLI expects.

**`contractmetav0`.** One key-value entry about the contract itself:
`SCMetaV0 { key: "infver", val }`, where `val` is the crate version the
workspace declares (`CONTRACT_META_TOOLCHAIN_VERSION`; currently `0.0.1`) —
the toolchain that produced the contract, not a network protocol. `stellar
contract info meta` displays it. Tooling reads it too: `soroban-spec`'s spec
shaking (`src/shaking.rs`) looks there for the Rust SDK's own key,
`rssdk_spec_shaking`, and its absence means spec-shaking version 1, which is
what this toolchain's single-entry section amounts to. The host reads none of
this.

The rewriter appends the three in one order: `contractspecv0`, then
`contractmetav0`, then `contractenvmetav0` **last**, so a contract still ends
with the same 32 measured bytes it ended with before the other two sections
existed — the host imposes no order on custom sections, and this order is a
choice made to keep that tail true.

Because the host parses neither tooling section — measured in
`tests/tests/stellar/envelope.rs`: well-formed and arbitrary-byte spec and meta
sections all upload and invoke — their correctness is the toolchain's alone to
check, and the CLI is strict about it: `Spec::new` (`soroban-spec-tools`
28.0.0), the reader `stellar contract invoke` builds a call's arguments with
from the deployed contract, and `stellar contract info interface` runs on a
file, decodes all three sections and fails the command if any one does not
decode (read from the CLI's source at v28.0.0; the tier mirrors it in
`spec::every_contract_decodes_the_way_the_cli_reads_a_deployed_contract`). So a
malformed `contractmetav0` or `contractspecv0` breaks `invoke` and `info
interface` even though the host itself uploads and runs the contract without
ever parsing either one, while `soroban-spec`'s and `stellar-xdr`'s readers
refuse each of those bodies first.

#### Invoking a contract from the CLI

Measured against `stellar` CLI 28.0.0 on a local `stellar/quickstart` network,
2026-09-24; the full transcript is `tests/tests/stellar/MEASURED_ABI.md`'s "CLI
measurement" chapter.

Reading the sections from a compiled `u32_methods.wasm` renders the interface
in the SDK's own trait form:

```text
$ stellar contract info interface --wasm u32_methods.wasm
#[soroban_sdk::contractargs(name = "Args")]
#[soroban_sdk::contractclient(name = "Client")]
pub trait Contract {
    fn identity(env: soroban_sdk::Env, x: u32) -> u32;
    fn add(env: soroban_sdk::Env, a: u32, b: u32) -> u32;
}
```

Deploying it first funds a local key and returns a contract id:

```text
$ stellar keys generate alice --network local --fund --overwrite
✅ Account alice funded on "Standalone Network ; February 2017"
$ stellar contract deploy --wasm u32_methods.wasm --source alice --network local
✅ Deployed!            (id CCRE5RTDJCATV2J4HLBZTKS5SNP2Q2S7EEJ3A572FZX7TSM47CDHDEE3)
```

`$ID` below stands for the contract id `deploy` printed, and `alice` is the
funded local key `keys generate` created.

Deployed and invoked with named arguments, the CLI reports it is simulating a
read-only call and prints the answer:

```text
$ stellar contract invoke --id $ID --source alice --network local -- add --a 2 --b 40
ℹ️ Simulation identified as read-only. Send by rerunning with `--send=yes`.
42
```

A mistyped argument is refused by the CLI itself, before any host call, naming
the parameter and its declared type:

```text
$ stellar contract invoke --id $ID --source alice --network local -- add --a two --b 40
error: Failed to parse argument 'a': … Expected type u32 (unsigned 32-bit integer), but received: 'two'
```

Contrast that with [the value ABI](#the-value-abi) above: a raw `Val` of the
wrong tag earns the host's undiscriminated `WasmVm, InvalidAction` /
`"VM call trapped: UnreachableCodeReached"`, identically whichever argument and
whichever tag were wrong. The spec is what lets the CLI catch the mistake with
the parameter's own name and type instead.

Against a contract carrying neither section — what this toolchain produced
before it emitted them (#466) — the same CLI panics on `info interface` and
offers `invoke` no command at all.

#### What is not supported, and why

- **Host imports — anything stateful.** Storage, ledger access, events,
  authorization, cross-contract calls and the host-object constructors all arrive
  as imported host functions. The language spells such a binding
  `use { f } from host::<module>;`, and at this target `infc` refuses it before
  external resolution, and so ahead of the allowlist, while code generation
  refuses it again, in the same words, as the backstop for a caller that skips
  that step. The refusal names one host binding — the first by module and field
  name, not by position in the source — and says why: the Soroban host-call
  convention is not bound by this toolchain, each of those host functions takes
  and returns the host's 64-bit tagged word, and nothing here maps an
  `external fn` onto one of them. It offers two ways out — remove the host
  binding to build a contract, or build for `wasm32`, where a host import is
  supported. Issue #324 is where binding that convention is tracked. Everything
  below follows from it.
- **Compound types and 64-bit integers at the contract boundary.** Both travel as
  host objects, which are built and read through those same imports.
- **Proof mode.** `--mode proof` fails the build, and so does `--mode compile -v`
  — see the next section for why the second one is refused rather than allowed.
- **`--wasm-features`.** `permits_bulk_memory()` is `false` for this target, so
  the one post-MVP family the compiler can emit is refused before a byte is
  emitted.
- **`[build.wasm-opt]`.** Declaring it alongside `target = "stellar"` is a
  load-time manifest error. Whether an external Binaryen preserves the
  wrappers and `contractenvmetav0` depends on which version is installed, and
  nothing in the manifest pins one.
- **`infs run`.** Refused. `run` executes the artifact through the `wasmtime` CLI,
  which passes each argument as a decimal — so `5` arrives as a word whose low
  byte is read as tag 5 and whose payload is empty, decoding to a zero-valued
  integer rather than to five. That is a wrong answer reported as a success. A
  contract is invoked by a Soroban host and by nothing else.

#### Proving the `wasm32` build and deploying the Stellar one

Because code generation never reads the target, the module a Stellar build starts
from is the module a `wasm32` build finishes with. That is what makes "prove one,
deploy the other" a statement about bytes rather than a hope, and it is why the
Rocq path is refused here rather than qualified: `wasm_to_v` reads the linked,
**pre-rewrite** bytes, so a `.v` written during a Stellar build would describe a
module with different exports, different bodies and none of the custom sections
that make it a contract — not the `.wasm` beside it on disk. `infc` refuses
that pairing and tells you to build the same source at `wasm32` instead.

The procedure:

```bash
# The module to reason about, and its Rocq translation.
infc contract.inf --target wasm32 -v --out-dir proofs/

# The module to deploy. Same source, same emitter, plus the marshalling shell.
infc contract.inf --target stellar --out-dir out/
```

`proofs/contract.wasm` is, byte for byte, the module `out/contract.wasm` was
rewritten from. The claim is checked mechanically over a set of fixtures — each
side built at its own default optimization level, as the CLI really builds them:

```bash
cargo test -p inference-tests --lib \
  codegen::wasm::stellar_gate::stellar_gate_tests::a_stellar_build_is_byte_identical_to_the_default_build
```

To see the relationship in the artifacts themselves, disassemble both. Every
function of the `wasm32` module appears in the Stellar module at the same index
with the same body; what the Stellar module adds is one `(param i64 …)
(result i64)` wrapper per exported method, appended after them, and the three
custom sections at the end, the environment metadata last.

**What the host accepts** — the runtime's wasmi configuration. This is the envelope a
module is admitted against, not a description of what Inference puts in one:

| Feature | Host status | Rationale |
|---------|-------------|-----------|
| `mutable-globals` | Enabled | Stack pointer, commonly used by compilers |
| `sign-ext` | Enabled | Integer conversions, commonly emitted |
| `bulk-memory` | Enabled | memcpy/memset optimization |
| `floating-point` | **Banned** | Non-deterministic NaN bit patterns |
| `saturating-float-to-int` | Disabled | Float-related |
| `multi-value` | Disabled | Not needed |
| `reference-types` | Disabled | Security surface |
| `tail-call` | Disabled | Security surface |
| `extended-const` | Disabled | Security surface |
| `SIMD` | Disabled | Not needed |

**What the compiler emits** is narrower than that envelope on two of the three enabled rows:

- `mutable-globals` — used. The shadow-stack pointer is a mutable global and the module
  exports it as `__stack_pointer`, which is what the proposal permits. An exported
  mutable global is accepted at upload; this was measured, not assumed.
- `sign-ext` — never emitted, at any target. No `i32.extend8_s`/`i32.extend16_s` (or their
  i64 forms) appears anywhere in code generation; a narrow signed value is normalized with
  `i32.shl` followed by `i32.shr_s`.
- `bulk-memory` — refused at this target. `Target::permits_bulk_memory()` is `false` for it,
  so `--wasm-features bulk-memory` (or the manifest's `wasm-features`) fails the build before
  a byte is emitted, and region fills and copies take the load/store lowering instead.

**Module shape.** No lld-style link step runs and no linker flags exist to pass: every LLVM
dependency, `rust-lld` among them, was removed when the compiler moved to direct emission. (The
`.wasm`-into-`.wasm` static merge described in [The WASM Linker](the-wasm-linker.md) is a
different thing and takes no such flags either.) The two facts that decide the module's shape
are therefore properties of code generation:

- **Exports** are every top-level non-method `pub fn` in the *entry* file, under its own name,
  `main` included. A `pub fn` in an imported file is intra-project visibility and is not
  exported; methods and spec-inner functions are never exported. No dead-code-elimination pass
  runs, so a private, never-called function in a compiled file is still emitted.
- **Linear memory is stack-first**: the shadow stack occupies the low end and grows downward
  from its top toward 0, and anything above it is the data region. Its size is one 64 KiB page
  by default and is set through `[memory]` / `--stack-size`, not by a link-time flag.

### SpaceWasm

Produces a module for the SpaceWasm flight interpreter — NASA JPL's `no_std`
WebAssembly 1.0 interpreter for flight software, which decodes a module into its
own IR on a fixed allocation and executes it with no operating system
underneath. Selected with `infc --target spacewasm` or
`[build] target = "spacewasm"` in an `Inference.toml`.

Unlike the Stellar target, this one imposes no calling convention and adds
nothing to the module. There is no marshalling shell, none of the contract
sections a Stellar build appends, and no rewrite: the artifact a SpaceWasm
build writes is, byte for byte, the artifact a `wasm32` build writes from the
same source. What the target does is narrow what a build may *contain* — the
instruction set, and nothing else.

| Setting | Value | Source |
|---------|-------|--------|
| Target | `wasm32-unknown-unknown` | The only module shape code generation produces |
| Instruction set | WebAssembly 1.0, with no opt-in available | `Target::permits_bulk_memory()` is `false`, so the one post-MVP family the compiler can emit is unreachable here |
| WASM proposals the module uses | `mutable-globals` only — a module with linear memory exports its mutable `__stack_pointer` global — and no instruction outside WebAssembly 1.0 | See "What the interpreter decodes" below |
| Compilation mode | `compile` only | Proof mode emits the custom 0xfc non-deterministic instructions, which the interpreter's decoder does not define |
| Recorded `OptLevel` (compile) | `Os` under `release`, `O0` under `debug` — no optimization pass currently acts on either | `Os` is the target's `default_opt_level`: size is the scarce resource on a flight computer |
| Floats | Impossible — no float instruction can be emitted, at any target | The language has no floating-point type: `SimpleTypeKind` (`core/ast`) admits the unit type `()`, `bool` and the eight integer widths and nothing else |
| Emitted bytes | Identical to a `wasm32` compile-mode build of the same source | Nothing on the emission path reads a target; see the procedure below |

#### What the interpreter decodes

The instruction set is the WebAssembly 1.0 MVP plus `mutable-globals`. Several
further proposals are tracked upstream and not implemented, and only one of
them, `bulk-memory`, costs an Inference build anything at all:

| Proposal | Interpreter status | What it costs a build here |
|----------|--------------------|----------------------------|
| `mutable-globals` | Implemented, every version | Nothing — the exported `__stack_pointer` global needs it |
| `custom-page-sizes` | Implemented since 0.2.0 | Nothing — memory stays 64 KiB pages, `min == max` |
| `bulk-memory` | Planned, `nasa/spacewasm#54` | `--wasm-features bulk-memory` fails the build before a byte is emitted; region fills and copies take the load/store lowering instead |
| `sign-ext` | Planned, `nasa/spacewasm#55` | Nothing — no target ever emits `i32.extend8_s` or its relatives; a narrow signed value is normalized with `i32.shl` followed by `i32.shr_s` |
| `saturating-float-to-int` | Planned, `nasa/spacewasm#56` | Nothing — the language has no float type |
| SIMD, multi-value, multi-memory, reference types | Not implemented | Nothing — code generation emits no instruction from any of them |

#### Decode-time limits

Decoding a module is where the interpreter checks its own maxima, and a module
that exceeds one would be refused on the device rather than at build time. The
build asks the same questions first — see [Conformance](#conformance) below. The
fixed limits are refusals: a module this target accepts meets every one of them,
so its declarations fit the fields the decoder reads them into and not merely its
instruction set. That is a claim about this table rather than a proof of
decodability — `core/target-conformance`'s README classifies every refusal the
interpreter can raise, either as one of the rows here or with the reason it is
out of reach, and nothing is left unclassified. The two embedder-configured rows
cannot be refusals, because their values belong to a deployment rather than to
the interpreter; the build measures them instead and prints what they have to be.
One shape is refused here although the interpreter would load it, and it is the
only one: a module that *references* a function or a global it defines at
position 65,536 or beyond. The interpreter narrows those two index spaces to
sixteen bits without checking the narrowing, so it loads such a module and then
runs it against the definition 65,536 below — a `call` reaching the wrong
function, a `global.get` reading the wrong word. There is no load failure to
move to build time there, only a wrong execution to inherit, and for a flight
computer a build that fails is better than an artifact that flies and calls the
wrong function. `core/target-conformance`'s README carries the mechanism, the
five places a module can name a definition, and the measurements behind them;
the narrowing is reported upstream as
[nasa/spacewasm#201](https://github.com/nasa/spacewasm/issues/201).
This compiler emits one global and no build approaches either count, which is
why it is a sentence here rather than a row above.

| Limit | Value | Where it comes from |
|-------|-------|---------------------|
| Parameter words per function | 255 | An `i64` is two words, so 128 `i64` parameters already exceed it |
| Local words per function | 65,535 | The same accounting |
| Call frame per function | 65,535 words | Two words of header plus the locals plus the operand peak. It bites before the local-word limit does: a function with an empty operand stack may declare 65,533 local words and no more |
| Import module and field name | 32 bytes each | `Module::MAX_NAME_LENGTH` |
| Host-registered module and function name | 31 bytes each | `HOST_MODULE_NAME_CAP` / `HOST_FUNCTION_NAME_CAP` — the registration side is one byte tighter than the decode side, so 31 is the cap an import name has to meet to be bindable at all |
| Host function parameters | 9, and a single result | `MAX_HOST_FUNCTION_PARAMS`; more than one result is `MultiReturnNotAllowed` |
| Custom section name | 32 bytes | Compile mode emits at most `inference.checked` and `name`, both well inside it |
| A `call_indirect`'s type index, and a `br_table`'s target count | 65,535 | The interpreter compiles a module into a bytecode of its own, and holds both in one 16-bit immediate. Code generation emits neither instruction, so this is a bound on a linked module |
| Operands one branch discards | 255 words | Counted in words from the bottom of the live stack to the frame the branch leaves, so an `i64` held across it costs two. Leaving the *function* — `return`, or a branch to its own outermost label — is an early return and is not counted at all |
| Control-frame nesting | Embedder-configured; 64 in the `spacewasm_std` reference embedding | `MAX_CONTROL_FRAMES`, a const generic of `Module::new` — a *deployment's* number, not the interpreter's |
| Operand-stack depth | Embedder-configured; 256 in `spacewasm_std` | `MAX_STACK_DEPTH`, the same const generic |
| Linear memory | 4 GiB | The WebAssembly 32-bit address space |

The last two are the ones to read carefully: they belong to whoever embedded the
interpreter, not to the interpreter, so "does this module fit" is only answerable
against a particular flight configuration. `spacewasm_std`'s 64 and 256 are the
reference numbers a report can be read against, not a promise about the vehicle.

#### Foreign modules

A module bound through `[wasm-dependencies]` or `-L` is merged into the artifact,
so it is held to the same instruction set the artifact is. The build asks that of
each external *before* the merge, while each one is still its own file:

```
$ infc main.inf -L lib/ --target spacewasm
The `spacewasm` target: the external module `rustlib`, resolved to lib/rustlib.wasm,
is not a WebAssembly 1.0 module: sign extension operations support is not enabled (at
offset 0x1b). This target's artifact is WebAssembly 1.0 throughout and this module is merged
into it, so the artifact is refused as a whole rather than in part. Rebuild `rustlib`
for the WebAssembly 1.0 instruction set — a stock Rust `wasm32-unknown-unknown` build
emits sign-extension instructions by default — or build this program at --target
wasm32, which links the module as it is.
```

Asked after the merge instead, the same question could only answer with a byte
offset into bytes that came from no single file. The linker itself is
deliberately more permissive — it accepts sign extension, bulk memory and mutable
globals, which is what an ordinary foreign toolchain emits — so this refusal is
the target's, not the linker's, and the same external links without complaint at
`--target wasm32`. The conformance check below still asks the whole-artifact
question afterwards, as the backstop for anything that reaches the module by some
other route.

#### Host imports

This target admits the binding. A program that binds
`use { f } from host::<module>;` compiles here, and its artifact carries the
import for the embedder to satisfy — the module a `wasm32` build writes, since
nothing on the emission path reads the target. What the target adds is a check
against three registration caps. An embedder supplies an import by registering
a host function under the import's two names, and the registration API names a
host module and a host function through a 31-byte name type and builds a host
function of at most nine parameters and one result, so an import outside those
caps may decode, but it can never be bound. That API is the authority for all
three, and `infc` asks them of the program's *declarations* — after external
resolution, before the
[allowlist](external-functions-and-wasm-linking.md#the-allowlist) and before
any byte is emitted — so the refusal asks for an edit to the source rather than
to an import section in a module no file holds. For a module name of 32 bytes:

```inference
external fn telemetry(channel: i32, value: i32) -> i32;
use { telemetry } from host::fprime_core_telemetry_downlink_a;

pub fn report(channel: i32) -> i32 {
    return telemetry(channel, 7);
}
```

```text
$ infc main.inf --target spacewasm
Parsed: main.inf
Analyzed: main.inf
SpaceWasm conformance failed: the host imports this program declares cannot all be registered by a SpaceWasm embedder.
No file was written.

  import name too long: import module name `fprime_core_telemetry_downlink_a` on `fprime_core_telemetry_downlink_a`.`telemetry` is 32 bytes; SpaceWasm accepts at most 31.
    This is a registration limit, not a decode limit: the decoder reads a name of up to 32 bytes, but an embedder registers host modules and host functions through a 31-byte name type, so a 32-byte name decodes and can never be bound to a host. 31 is the cap that matters.
    Shorten the module name this extern is bound under.
```

The declaration-level check takes on trust that the declarations are the imports
the artifact will carry, so the post-link [conformance check](#conformance) asks
the same three caps of the bytes as the backstop; the two cannot disagree about
an import both see, because they run one rule. Why the caps are asked before the
allowlist rather than after it is set out in
[What is refused, and by which layer](external-functions-and-wasm-linking.md#what-is-refused-and-by-which-layer).
Of the three caps, the one-result cap is unreachable from Inference source,
since an `external fn` returns one value or none; it earns its findings on the
byte-level path.

Such a program has no proof path: `infc` refuses every build that writes a `.v`
for a program that binds a host import, at every target, so the procedure in
[Proving the `wasm32` build and deploying the SpaceWasm
one](#proving-the-wasm32-build-and-deploying-the-spacewasm-one) does not apply to
it. To run one, the embedder harness can register a stub per import — see the
`--host` transcript in
[Running a module under the embedder harness](#running-a-module-under-the-embedder-harness).

#### Conformance

Every SpaceWasm build checks the module it is about to write against every fixed
limit in the table above, and measures the two that an embedder chooses. The
check runs after linking and before any file is written, so a refusal leaves
nothing on disk:

```
$ infc main.inf --target spacewasm
Parsed: main.inf
Analyzed: main.inf
Codegen complete
spacewasm: conformant with WebAssembly 1.0; deepest control nesting 7 in `render_row`,
tallest operand stack 19 values in `mix` (peak 27 stack words in `mix`). Build the
embedder with MAX_CONTROL_FRAMES >= 7 and MAX_STACK_DEPTH >= 19 (spacewasm_std uses
64 and 256); limits from spacewasm 0.7.1.
WASM generated at: out/main.wasm
```

**What is failed.** Anything outside WebAssembly 1.0 plus mutable globals — which
for a linked build means a post-1.0 instruction in a foreign module, refused
earlier and by name, see [Foreign modules](#foreign-modules)
— and each decode-time and registration-time limit in the table: parameter words,
local words, call-frame words, a locals group, a `call_indirect` type index or
`br_table` width over the interpreter's 16-bit IR immediate, a branch discarding
more than 255 operand words, an import module or field name over 31 bytes, a host
function over nine parameters or one result, a custom section name over 32 bytes,
and a memory over the address space. One further refusal is the build being
deliberately stricter than the interpreter rather than agreeing with it: a
reference to a defined function or global at position 65,536 or beyond, which
the interpreter loads and then resolves to the wrong definition, as
[Decode-time limits](#decode-time-limits) above describes. Every refusal names
the two numbers, says which cap it met, and gives one thing to change. Six of
them describe module shapes this compiler cannot produce; those say so, and ask
you to rebuild the external module or report a compiler bug, because you did not
write the file the shape is in — the two IR-immediate ones and the truncation
one still name the edit that would shorten it, since for those there is one.

**What is reported.** Three numbers, and each carries its unit because two of
them sound alike and are not the same quantity:

- **Deepest control nesting**, in frames, counting the implicit function-body
  frame that both the interpreter and the compiler's checker push before reading
  an operator. Compare it directly against `MAX_CONTROL_FRAMES`; there is no
  off-by-one to apply.
- **Tallest operand stack**, in **values** — one entry per value whatever its
  width. This is the unit of `MAX_STACK_DEPTH`, so it is the number the second
  const generic must clear.
- **Peak stack words**, the same high-water mark weighted by width, where an
  `i64` counts two. This is *not* `MAX_STACK_DEPTH`'s unit: it is what the
  engine's value stack holds, and it is what `Engine::new`'s stack budget is
  sized from.

Sizing the verifier's const generic from the word figure over-allocates; sizing
the engine's stack from the value figure under-allocates and fails in flight.
That is why both are printed, each labelled, and why the line ends with the
release the limits were read from.

When a maximum exceeds the `spacewasm_std` reference configuration the build
still succeeds — a conformant module that needs a bigger const generic is not a
broken module — and prints a warning naming the three functions that reach
highest, so the choice between raising the generic and flattening a function is
made with the functions in front of you:

```
warning: spacewasm: control nesting 71 exceeds spacewasm_std's MAX_CONTROL_FRAMES of 64.
Deepest functions: `render_row` (71), `blend` (66), `mix` (65). Either raise the const
generic in your embedder or flatten the nesting in these functions.
```

**The `wasm-opt` re-check.** This target keeps `[build.wasm-opt]` — its artifact
is plain WebAssembly 1.0 with nothing layered on top, and size is the scarce
resource on a flight computer, which is why `-Os` is its recorded level. But the
optimizer is an external Binaryen that nothing here pins, and it reshapes control
flow and coalesces locals — both of them quantities the report above measures. So
`infs` runs the same check again on the optimized bytes, immediately before they
are moved into place, and a failure leaves the artifact the compiler wrote
exactly where it was:

```
$ infs build
...
SpaceWasm conformance failed: out/main.wasm is not a module a SpaceWasm embedder can load and run as written.
The original artifact is unchanged; try `--no-wasm-opt`, or a different Binaryen version.
```

The invariant is "the checked bytes are the bytes that ship", which is why the
re-check sits after every rewrite that step performs rather than beside the
re-validation next to it. A re-check that passes reprints the summary, so the
last budget in the log is the one describing the artifact on disk rather than the
one the compiler wrote before the optimizer saw it.

#### Running a module under the embedder harness

SpaceWasm is a library, not a program: it has no command line of its own, and a
finished artifact answers "does it load?" only once something embeds it. This
repository ships the smallest thing that does, as an example of the test crate.
Every transcript below is a real run, so the figures can be re-derived rather
than taken on trust, and all but the last two are against one artifact —
`out/main.wasm`, built by `infc main.inf --target spacewasm` from
`pub fn main() -> i32 { return 10; }`, whose `pub` is what puts `main` in the
export section:

```bash
cargo run -p inference-tests --example spacewasm-embed -- out/main.wasm --invoke main
main = 10
```

It loads the artifact under the reference embedder configuration, calls one
export, and prints the result as `NAME = value`, or `NAME = (unit)` for a
function that returns nothing. Arguments follow the export name and are decimal
integers coerced to the parameter types the artifact declares
(`--invoke add 2 40`). With neither `--invoke` nor `--stats` it reports what the
module exports and stops, which is the cheapest way to ask whether an artifact
loads at all; asking for a measurement makes the measurement the report.

Each way a run can end has an exit code of its own — a module that could not be
read, one the interpreter refused, a missing export, a trap, an exhausted fuel
budget — so a script can tell them apart without reading the message. A trap
prints the interpreter's own reason. Execution runs under an instruction budget
that `--fuel N` sets, so a program that does not terminate fails the run instead
of hanging it.

`--stats` reports what the module cost the interpreter — the IR pages it
compiled to, the sixteen-bit words written into them against the words those
pages hold, those words as bytes, the artifact's size, and the ratio of the two:

```bash
cargo run -p inference-tests --example spacewasm-embed -- out/main.wasm --stats
code pages: 1
IR words (16-bit): 4 / 256 (1.56%)
IR bytes: 8
wasm bytes: 62
IR bytes per wasm byte: 0.13
```

The page and word figures are computed the way upstream's own `spacewasm_std`
demonstration binary computes them, so they can be read beside its output. The
last line is deliberately not spelled the way upstream spells its own
*compilation ratio*: that one is a different quantity — the live bytes its
bounded page allocator holds, the engine and the guest memory as much as the IR
— and this harness cannot produce it, because it runs on an unbounded allocator
that keeps no statistics. A figure that is not upstream's should not travel
under upstream's name. Adding `--json` prints the measurement as a single line
of JSON instead, which is the hook for tracking these numbers over time; the
keys are documented where the harness is implemented, and the ratio is carried
there at full precision — the two decimals above are a courtesy to a reader,
not the figure.

A program binding `use { f } from host::<module>;` compiles at this target and
its artifact carries the import, but it loads only once something supplies that
import — and the harness can stand in for an embedder just far enough to run
it. `--host MODULE.FIELD=PARAMS[:RESULT]`, repeated once per import, registers
a stub under the import's two names, with the signature spelled in the
interpreter's own alphabet: one character per value type, `i` for `i32`, `I`
for `i64`, `f` for `f32` and `d` for `f64`, so `env.clock_ms=:I` takes nothing
and returns an `i64`. A stub answers zero of its result type, never traps and
never reads guest memory, and each call it receives is printed to stderr as
`host call: module.field(arg, …)`, ahead of the result of the invocation that
made it. What that shows is which host functions the program asked for, with
which arguments and in which order; what a real host would have answered is
your embedder's to decide. For the F´ program

```inference
external fn telemetry(channel: i32, value: i32) -> i32;
external fn command(opcode: i32) -> i32;
use { telemetry, command } from host::fprime_core;

external fn clock_ms() -> i64;
use { clock_ms } from host::env;

pub fn report(channel: i32) -> i64 {
    let ack: i32 = command(channel);
    let sent: i32 = telemetry(channel, ack);
    if sent == 0 {
        return 0;
    }
    return clock_ms();
}
```

built into `out/main.wasm` the same way:

```bash
cargo run -p inference-tests --example spacewasm-embed -- out/main.wasm \
    --host fprime_core.telemetry=ii:i --host fprime_core.command=i:i \
    --host env.clock_ms=:I --invoke report 1
host call: fprime_core.command(1)
host call: fprime_core.telemetry(1, 0)
report = 0
```

`telemetry`'s stub answered zero, so `report` returned zero without asking the
clock. A name longer than 31 bytes, more than nine parameters or more than one
result is a usage error that names the interpreter's own refusal and the limit
behind it, both read from the interpreter rather than restated by the harness.
Two counts are usage errors as well, because the interpreter does not check them
and would bind the excess imports to other stubs without a word: more than 256
module names, which its one-byte host-module reference cannot address, and more
than 65,536 stubs under one module name, whose positions its binder narrows to
sixteen bits.

A module whose imports have no stub is still a load failure. The interpreter's
verdict names no import, so the harness reads the module a second time and
lists each `module.field` beside the spec it needs:

```bash
cargo run -p inference-tests --example spacewasm-embed -- out/main.wasm --invoke report 1
error: out/main.wasm does not load under the SpaceWasm interpreter: FunctionImportNotFound at byte 58
  reading the Import section
  it imports these, and no `--host` stub was registered for any of them:
    fprime_core.telemetry: no stub; it needs `--host fprime_core.telemetry=ii:i`
    fprime_core.command: no stub; it needs `--host fprime_core.command=i:i`
    env.clock_ms: no stub; it needs `--host env.clock_ms=:I`
  register a stub for each with `--host MODULE.FIELD=PARAMS[:RESULT]`, or run the module from the embedder that supplies them
```

An import registered with a stub of another signature is listed beside that
stub and the half of its signature that differs. One no stub can supply — a
global, say — is listed with the reason. A spec naming no import of the module
is listed after the imports, which is where a misspelt name shows up.

#### Proving the `wasm32` build and deploying the SpaceWasm one

Because code generation never reads the target, a SpaceWasm build and a `wasm32`
build of the same source are the same module. Not "the module the other was
derived from" — the same file.

The procedure:

```bash
# The module to reason about, and its Rocq translation. `-v` implies proof mode.
infc main.inf --target wasm32 -v --out-dir proofs/

# The module to deploy, and the same source at the default target.
infc main.inf --target spacewasm --out-dir out/spacewasm/
infc main.inf --target wasm32    --out-dir out/wasm32/

# The two compile-mode modules are the same file.
cmp out/wasm32/main.wasm out/spacewasm/main.wasm    # fc /b on Windows
```

The `cmp` is the second of the two links below. It compares two *compile-mode*
builds, which is the comparison the target claim is about; `proofs/main.wasm` is
a proof-mode artifact and carries whatever the specifications in the source add
to it, so it is the first link — not this one — that ties it to
`out/wasm32/main.wasm`.

That `cmp` is a statement about *this* target and no other. The Stellar
procedure above deliberately does not have one: a Stellar artifact carries
appended wrappers and the three contract sections, so its relationship to the
proved module is "rewritten from", not "equal to".

The same difference shows up as a command line that works here and not there.
Proof mode is refused at both targets, for the same reason — it emits the custom
`0xfc` instructions and neither runtime decodes them — so `--mode proof` and a
bare `-v` fail either way. But `--mode compile -v`, the one spelling that keeps
compile mode and still asks for a translation, **succeeds** at `spacewasm` and
writes exactly the `.v` a `wasm32` compile-mode build writes:

```bash
infc main.inf --target spacewasm --mode compile -v --out-dir out/spacewasm/
infc main.inf --target wasm32    --mode compile -v --out-dir out/wasm32/
cmp out/wasm32/main.v out/spacewasm/main.v          # fc /b on Windows
```

At `stellar` that spelling is refused, because there the translation reads the
pre-rewrite bytes while the `.wasm` written beside it is the contract. Here
there is no rewrite for it to run ahead of. It is a decision rather than a
consequence of where the refusal happens to be written, and the CLI suite pins
it — a refusal widened from "the rewriting target" to "any non-default target"
would take this with it.

What the SpaceWasm build buys today is the envelope, applied to a build whose
output is otherwise the default target's. Named rather than counted, because
only two of the three are rows in the settings table above: a build for this
target refuses proof mode, refuses every post-MVP instruction family, and
refuses a non-deterministic construct — a `forall`, `exists`, `assume` or
`unique` block, or an `@` — anywhere in a function that ships.

The third is not a narrowing this target introduces. Analysis rules A042 and
A006 refuse non-determinism outside a `spec` in every build at every target,
with a source location, and every `infc` build runs both. What this target adds
is that code generation asks the question again, as a backstop for a caller that
reached it without running analysis; the default target does not ask it, because
there the custom `0xfc` instructions are ones Inference's own tooling decodes —
no general-purpose embedder does — which is what makes this a target-specific
question rather than a second copy of A042. The backstop reaches the same
programs the rules do: it descends every nested block, every statement and every
operand, so a `forall` in a loop body and an `@` under an operator are refused
here as surely as a `return @` is. What the rules have that it does not is the
source location. Either way the refusal is about executable code only: `compile`
mode strips `spec` bodies before either check looks at them, so a specification
written in those constructs costs a SpaceWasm build nothing.

The conformance report against the decode-time limits is what makes the envelope
buy more, and every build produces one: see [Conformance](#conformance) above
for what a build fails on, what it reports, and how to read the report against
an embedder's `MAX_CONTROL_FRAMES` and `MAX_STACK_DEPTH`.

**The two links.** For the deployed bytes to be the proved bytes, two identities
have to hold, and each has its own guard in the test suite:

1. A proof-mode module's execution functions are byte-identical to compile
   mode's. Codegen applies no optimization pass in either mode, so the only
   difference a proof build can carry is the spec functions compile mode drops.

   ```bash
   cargo test -p inference-tests --lib \
     codegen::wasm::checked_arith::checked_arith_tests::proof_and_compile_builds_of_a_guarded_source_are_byte_identical
   ```

2. A compile-mode SpaceWasm module is byte-identical to a compile-mode `wasm32`
   module. This is checked over every single-file fixture in the code-generation
   corpus, each side built at its own default optimization level as the two
   command lines really build them:

   ```bash
   cargo test -p inference-tests --lib \
     codegen::wasm::target_identity::target_identity_tests::every_target_emits_what_the_default_target_emits
   ```

Break either link and the `.v` describes a program that is not the one on the
vehicle. Both hold, so it is.

# Appendix A: Optimization Levels

`OptLevel` is a single per-build value recorded on the compiled output — not a
per-function-kind setting, and not something the compiler itself currently
acts on. No optimization pass runs during WASM emission in either mode: the
descriptions below are the levels' *intended* meaning for a future consumer
(a `wasm-opt` integration, say), not present-day behavior.

| Optimization Level | Intended meaning |
|--------------------|-------------|
| `-O0` | No optimizations. Recorded by `BuildProfile::Debug` in compile mode. |
| `-O1` | Some optimizations. Balanced compile time and code size. |
| `-O2` | Aggressive optimizations. Standard release. |
| `-O3` | Maximum optimizations. Default recorded level for the Wasm32 target. |
| `-Os` | Optimize for size. Similar to `-O2` with additional size reductions. Default recorded level for the SpaceWasm target. |
| `-Oz` | Optimize for minimum size. Default recorded level for the Stellar target. |

# Appendix B: WebAssembly Features

| Feature | Description |
|---------|-------------|
| `sign-ext` | Sign-Extension Operators. Makes converting small signed integers (8-bit, 16-bit) to larger ones faster. |
| `bulk-memory` | Bulk memory operations like `memory.copy` (memcpy) and `memory.fill` (memset). Without this, the compiler generates slow byte-by-byte loops. |
| `mutable-globals` | Allows importing/exporting mutable global variables. Often required for the stack pointer. |
| `multivalue` | Allows functions to return multiple values natively and blocks/loops to have inputs. |
| `reference-types` | Allows holding opaque references to host objects using `externref`. Essential for GC integration. |
| `tail-call` | Adds `return_call` instructions for tail call optimization. |
| `extended-const` | Allows basic math expressions in global initializers. |
| `simd128` | Single Instruction, Multiple Data. Processes 128 bits of data in a single operation. |
