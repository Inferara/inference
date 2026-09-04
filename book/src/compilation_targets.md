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

**`proof` mode**: Emits a single WASM module for `wasm_to_v` Rocq translation, preserving the specification code that compile mode strips. The non-deterministic constructs do not survive into the module: a `forall`-quantified (or plain) spec function becomes an `hassert` obligation and is omitted from the module record altogether, while an `exists`- or `unique`-quantified one is retained with a vanilla body — each scalar `@` becomes a hidden trailing choice parameter, and each `assume`/`assert` a trap-on-false filter. What proof mode does preserve is source order and shape: statements lower in the order written and the compiler applies no optimization pass to reshape them, so a retained body reads against its source and there is no optimization barrier to insert. Execution functions are byte-for-byte identical to what compile mode's release profile emits for the same source, so Rocq proofs cover the artifact that actually ships. If the source has no `non_det_operations`, proof mode output is identical to compile mode release output (`Option 5` = `Option 2`). The target is always `Wasm32`; the `Soroban` target supports `compile` mode only. Build profiles (`debug`/`release`) do not change proof mode's output — only the `OptLevel` value it records, which today changes no emitted byte either way (see Appendix A below).

The contract between the generated `.wasm` binary, the per-spec function index map carried alongside (or embedded as the `inference.spec_funcs` custom section), and the Rocq predicates the generated `.v` file depends on is documented in [`core/wasm-to-v/ROCQ_CONTRACT.md`](../../core/wasm-to-v/ROCQ_CONTRACT.md).

### Selecting a mode at the CLI

Pass `--mode {compile,proof}` to either CLI: `infs build path/to/file.inf --mode proof` or `infc path/to/file.inf --mode proof`. Equivalently, `infc -v` (emit Rocq) implies `--mode proof` unless `--mode compile` is also passed; mirror-rule: `--mode proof` implies `-v`. Without either flag, the default is compile mode.

For project-aware builds — `Inference.toml`, project discovery, and the `infs build`/`run` workflow that resolves a mode from the manifest — see [Projects and the infs Toolchain](projects-and-the-infs-toolchain.md).

| Property | Value | Rationale |
|----------|-------|-----------|
| Spec function lowering | Structural 1:1 from source | Rocq readability — no optimizer runs to disturb it |
| Execution function bytes | Byte-identical to compile mode release | Proofs must cover the actual deployed code, not a differently-compiled variant |
| Target | Wasm32 only | Custom 0xfc intrinsics required |
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
| WASM features | MVP baseline (no post-MVP features) |
| Recorded `OptLevel` (compile) | `O3` — no optimization pass currently acts on it |
| Proof mode output | Byte-identical to compile mode's, plus structurally 1:1 spec functions |

### Stellar Soroban

Produces Soroban-compatible WASM binaries matching the `wasm32v1-none` Rust target configuration.
The target's default optimization applies (`-Oz` for Soroban, `-O3` for Wasm32).
Purpose: deploy to Stellar network as Soroban smart contracts.

| Setting | Value | Source |
|---------|-------|--------|
| Target | `wasm32-unknown-unknown` | Same as `wasm32v1-none` |
| WASM baseline | MVP | Pinned to WebAssembly 1.0 baseline |
| WASM features | `+mutable-globals,+sign-ext,+bulk-memory` | Soroban VM accepts these three post-MVP features |
| Optimization | `Oz` (size-aggressive, matching Soroban convention) |
| Max binary size | 64 KB |
| Floats | Forbidden — codegen must not emit float instructions |

**Soroban VM WASM feature matrix** (from `rs-soroban-env` wasmi config):

| Feature | Status | Rationale |
|---------|--------|-----------|
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

**Soroban target flags**
- `--no-entry` — reactor model, no `_start` (same as Wasm32)
- `--export-dynamic` — export all symbols with default visibility (Soroban host discovers exports by name)
- `--gc-sections` — strip unreachable code (critical for 64KB limit)
- `-z stack-size=1048576` — 1MB stack (Soroban default)
- `--stack-first` — place stack before data in linear memory (Soroban convention)

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
| `-Os` | Optimize for size. Similar to `-O2` with additional size reductions. |
| `-Oz` | Optimize for minimum size. Default recorded level for the Soroban target. |

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
