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
| Floats | Impossible — no float instruction can be emitted, at any target | The language has no floating-point type: `SimpleTypeKind` (`core/ast`) admits `unit`, `bool` and the eight integer widths and nothing else |

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
Stellar contract: add/2, flip/1, nothing/0; env protocol 20; 359 bytes
```

#### What an exported function may declare

An exported function is a contract method, and a contract method is confined to
the scalars the convention encodes without a host object:

| Position | Admitted |
|----------|----------|
| Parameter | `u32`, `i32`, `bool` — at most 32 of them |
| Return | `u32`, `i32`, `bool`, or nothing |
| Name | 1–32 bytes of `[A-Za-z0-9_]`, not `__`-prefixed |

Everything else is refused at code generation, before any byte exists, naming the
function and the offending element. The refusals differ because the reasons do:

- **A 64-bit integer** (`u64`, `i64`): *"…the host's word is 64 bits wide and
  spends part of it on a tag, so no 64-bit value fits in one, and it travels as a
  host object built through host functions this toolchain does not bind yet —
  issue #464."*
- **A narrow integer** (`u8`, `i8`, `u16`, `i16`): *"…the host has no narrower
  word, so what an exported `u8` does with a caller-supplied 300 is a language
  question rather than a layout one, and it is not settled. Widen the declaration
  to `u32` or `i32`."* This one is **not** blocked on host objects and never will
  be.
- **A struct, array or enum**: *"A compound value crosses the contract boundary
  as a host object, which a contract has to build and read through host functions
  it imports; this toolchain binds none of those yet."*
- **A compound return** additionally explains the mechanism it cannot use: the
  caller would receive it through a hidden pointer into linear memory, and a
  contract method hands back one host word and has no pointer to give.
- **A name over 32 bytes**: *"…the host holds a method name of at most 32. A
  longer one is not expressible by any caller, so the method would upload and be
  unreachable — rename it."*
- **A module with no exported function**: refused outright — it would upload with
  no method to call.

`main` is a contract method like any other: it is exported under its own name and
held to the same rules. There is no entry point to a contract, so nothing
distinguishes it.

Only the *entry file's* top-level `pub fn` declarations are exported, so an
inadmissible type inside a private function, a method, or an imported file is not
the gate's business.

#### The metadata section and the declared protocol

Every contract carries a `contractenvmetav0` custom section declaring the
environment protocol it was built for. A module without one is refused at upload
by every Soroban host there has ever been, so the rewriter appends it
unconditionally, last in the module, as 32 fixed bytes.

The declared protocol is **20**, with pre-release 0. The rule a host applies is
that the declared protocol must not be *newer* than the ledger's, and that a
pre-release of 0 is required below it; 20 is the protocol Soroban arrived in, so
a contract declaring it is uploadable to every Soroban network that has ever
existed. Declaring today's protocol instead would make the artifact
un-uploadable to any network still behind it.

That constant is not permanent. It is the *minimum* the contract needs, and it
must become the maximum of the minimum protocols of the host functions the
contract imports once importing host functions is possible at all (issue #464).
Until then a contract imports nothing, and the floor is the whole answer.

#### What is not supported, and why

- **Host imports — anything stateful.** Storage, ledger access, events,
  authorization, cross-contract calls and the host-object constructors all arrive
  as imported host functions, and this toolchain binds none of them. Issue #464
  is where that work is tracked. Everything below follows from it.
- **Compound types and 64-bit integers at the contract boundary.** Both travel as
  host objects, which are built and read through those same imports.
- **No `contractspecv0` section.** A contract normally ships a machine-readable
  description of its methods' types, and `stellar contract invoke` uses it to
  parse command-line arguments into `Val`s. Without it that CLI cannot type your
  arguments, and a caller must encode them itself. This is scoped out on cost,
  not on impossibility: the `SCSpecTypeDef` discriminants are published in
  `Stellar-contract-spec.x`. Nobody should invent them; anybody may look them up.
- **Proof mode.** `--mode proof` fails the build, and so does `--mode compile -v`
  — see the next section for why the second one is refused rather than allowed.
- **`--wasm-features`.** `permits_bulk_memory()` is `false` for this target, so
  the one post-MVP family the compiler can emit is refused before a byte is
  emitted.
- **`[build.wasm-opt]`.** Declaring it alongside `target = "stellar"` is a
  load-time manifest error. Whether an external Binaryen preserves the wrappers
  and the metadata section depends on which version is installed, and nothing in
  the manifest pins one.
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
module with different exports, different bodies and no metadata section — not the
`.wasm` beside it on disk. `infc` refuses that pairing and tells you to build the
same source at `wasm32` instead.

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
(result i64)` wrapper per exported method, appended after them, and the metadata
section at the end.

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
underneath.

Unlike the Stellar target, this one imposes no calling convention and adds
nothing to the module. There is no marshalling shell, no metadata section and no
rewrite: the artifact a SpaceWasm build writes is, byte for byte, the artifact a
`wasm32` build writes from the same source. What the target does is narrow what
a build may *contain* — the instruction set, and nothing else.

| Setting | Value | Source |
|---------|-------|--------|
| Target | `wasm32-unknown-unknown` | The only module shape code generation produces |
| Instruction set | WebAssembly 1.0, with no opt-in available | `Target::permits_bulk_memory()` is `false`, so the one post-MVP family the compiler can emit is unreachable here |
| WASM proposals the module uses | `mutable-globals` only — a module with linear memory exports its mutable `__stack_pointer` global — and no instruction outside WebAssembly 1.0 | See "What the interpreter decodes" below |
| Compilation mode | `compile` only | Proof mode emits the custom 0xfc non-deterministic instructions, which the interpreter's decoder does not define |
| Recorded `OptLevel` (compile) | `Os` under `release`, `O0` under `debug` — no optimization pass currently acts on either | `Os` is the target's `default_opt_level`: size is the scarce resource on a flight computer |
| Floats | Impossible — no float instruction can be emitted, at any target | The language has no floating-point type: `SimpleTypeKind` (`core/ast`) admits `unit`, `bool` and the eight integer widths and nothing else |
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
that exceeds one is refused on the device rather than at build time. **Nothing in
the compiler checks any of these today.** The conformance step that will is
landing in a later change; until it does, a module this target accepts is one
whose *instruction set* the interpreter covers, which is not yet the same
statement as one it will load.

| Limit | Value | Where it comes from |
|-------|-------|---------------------|
| Parameter words per function | 255 | An `i64` is two words, so 128 `i64` parameters already exceed it |
| Local words per function | 65,535 | The same accounting |
| Import module and field name | 32 bytes each | `Module::MAX_NAME_LENGTH` |
| Host-registered module and function name | 31 bytes each | `HOST_MODULE_NAME_CAP` / `HOST_FUNCTION_NAME_CAP` — the registration side is one byte tighter than the decode side, so 31 is the cap an import name has to meet to be bindable at all |
| Host function parameters | 9, and a single result | `MAX_HOST_FUNCTION_PARAMS`; more than one result is `MultiReturnNotAllowed` |
| Custom section name | 32 bytes | Compile mode emits at most `inference.checked` and `name`, both well inside it |
| Control-frame nesting | Embedder-configured; 64 in the `spacewasm_std` reference embedding | `MAX_CONTROL_FRAMES`, a const generic of `Module::new` — a *deployment's* number, not the interpreter's |
| Operand-stack depth | Embedder-configured; 256 in `spacewasm_std` | `MAX_STACK_DEPTH`, the same const generic |
| Linear memory | 4 GiB | The WebAssembly 32-bit address space |

The last two are the ones to read carefully: they belong to whoever embedded the
interpreter, not to the interpreter, so "does this module fit" is only answerable
against a particular flight configuration. `spacewasm_std`'s 64 and 256 are the
reference numbers a report can be read against, not a promise about the vehicle.

#### Proving the `wasm32` build and deploying the SpaceWasm one

Because code generation never reads the target, a SpaceWasm build and a `wasm32`
build of the same source are the same module. Not "the module the other was
derived from" — the same file. That is why the Rocq path is refused at this
target rather than qualified: there would be nothing for a second `.v` to
describe that the first does not.

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
appended wrappers and a metadata section, so its relationship to the proved
module is "rewritten from", not "equal to".

What the SpaceWasm build buys today is the envelope — the three refusals above,
applied to a build whose output is otherwise the default target's. The
conformance report against the decode-time limits is what will make it buy more,
and it is landing in a later change.

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
