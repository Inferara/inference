# Compilation Targets

## Compilation Matrix

**`non_det_operations`** = { `spec`, `uzumaki`, `forall_block`, `assume_block`, `exists_block`, `unique_block` }

`compile` mode produces a `.wasm` binary (executable or library). `proof` mode produces a `.v` Rocq file (via `wasm_to_v`). Non-deterministic operations can only appear inside `spec` blocks. In compile mode, `spec` nodes are stripped (they have no runtime meaning). In proof mode, all code including `spec` blocks is emitted.

| Option | Mode | Profile | Has `non_det_operations` | Behavior |
|--------|------|---------|--------------------------|----------|
| 1 | `compile` | `debug`   | no  | Compile with the chosen `Target` skipping optimizations |
| 2 | `compile` | `release` | no  | Compile with the chosen `Target` and its default optimization |
| 3 | `compile` | `debug`   | yes | Exclude `spec` nodes from codegen, then compile as `Option 1` |
| 4 | `compile` | `release` | yes | Exclude `spec` nodes from codegen, then compile as `Option 2` |
| 5 | `proof`   | *(fixed)* | no  | Identical to `Option 2` — no spec code to preserve, output matches compile mode release |
| 6 | `proof`   | *(fixed)* | yes | Spec functions: `optnone`+`noinline` (`-O0`). Execution functions: target's default release optimization (same as `Option 2`). All code emitted. |

**`compile` mode**: Produces production binaries. Debug/release profiles control optimization. Non-det `spec` nodes are stripped from codegen since they have no runtime meaning. The output can be the verification target — the artifact whose behavior is proven correct by Rocq proofs.

**`proof` mode**: Emits all code (including spec functions with non-det intrinsics) into a single WASM module for `wasm_to_v` Rocq translation. Only spec functions (those containing `non_det_operations`) receive `optnone`+`noinline` barriers to preserve 1:1 structural correspondence with the source code — this ensures Rocq readability. Execution functions are compiled at the target's default release optimization, identical to compile mode release, so that Rocq proofs cover the actual deployed code. If the source has no `non_det_operations`, proof mode output is identical to compile mode release output (`Option 5` = `Option 2`). The target is always `Wasm32` (custom intrinsics require strict MVP). Build profiles (`debug`/`release`) do not apply to proof mode — execution always uses release optimization, spec always uses `O0` + barriers.

The contract between the generated `.wasm` binary, the per-spec function index map carried alongside (or embedded as the `inference.spec_funcs` custom section), and the Rocq predicates the generated `.v` file depends on is documented in [`core/wasm-to-v/ROCQ_CONTRACT.md`](../../core/wasm-to-v/ROCQ_CONTRACT.md).

### Selecting a mode at the CLI

Pass `--mode {compile,proof}` to either CLI: `infs build path/to/file.inf --mode proof` or `infc path/to/file.inf --mode proof`. Equivalently, `infc -v` (emit Rocq) implies `--mode proof` unless `--mode compile` is also passed; mirror-rule: `--mode proof` implies `-v`. Without either flag, the default is compile mode.

For project-aware builds — `Inference.toml`, project discovery, and the `infs build`/`run` workflow that resolves a mode from the manifest — see [Projects and the infs Toolchain](projects-and-the-infs-toolchain.md).

| Property | Value | Rationale |
|----------|-------|-----------|
| Spec function optimization | `-O0` + `optnone` + `noinline` | 1:1 structural correspondence for Rocq translation |
| Execution function optimization | Target's default release (e.g., `-O3` for Wasm32) | Proofs must cover the actual deployed code, not a differently-compiled variant |
| Target | Wasm32 only | Custom 0xfc intrinsics required |
| Name section | Always emitted | Rocq identifiers require function/local names |
| DWARF | Never | Not useful for formal verification |
| wasm-opt | Never on spec functions | Would destroy structural correspondence of specs |
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

The external artifact remains as-is (potentially fully optimized). The `spec` code requires structural identity for Rocq readability. Execution code — whether from Inference source or external modules — is compiled at the target's default release optimization so that Rocq proofs cover the actual deployed artifact. Only spec functions receive `optnone`+`noinline` barriers; execution functions are optimized normally.

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
| Optimization (compile) | `-O3` |
| Optimization (proof, execution functions) | `-O3` (same as compile release) |
| Optimization (proof, spec functions) | `-O0` (unoptimized) |

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

| Optimization Level | Description |
|--------------------|-------------|
| `-O0` | No optimizations. Used for spec functions to preserve structural correspondence. |
| `-O1` | Some optimizations. Balanced compile time and code size. |
| `-O2` | Aggressive optimizations. Standard release. |
| `-O3` | Maximum optimizations. Default for Wasm32 target. |
| `-Os` | Optimize for size. Similar to `-O2` with additional size reductions. |
| `-Oz` | Optimize for minimum size. Default for Soroban target. |

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
