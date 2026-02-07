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

## Targets

Target parameters (triple, CPU, features, linker flags) are **locked per target variant** and cannot be overridden in `Inference.toml`. The only user-facing configuration is target selection and build profile (debug/release) for compile mode.

### Target::Wasm32 (default)

General-purpose WASM target using custom `inf-llc` with Inference intrinsic support. Used in both `compile` and `proof` modes.

| Setting | Value |
|---------|-------|
| LLVM triple | `wasm32-unknown-unknown` |
| CPU | `mvp` |
| LLVM features | (none) |
| inf-llc flags | `-mcpu=mvp -filetype=obj` |
| rust-lld flags | `-flavor wasm --no-entry [--export=main]` |
| Optimization (compile) | `-O3` |
| Optimization (proof, execution functions) | `-O3` (same as compile release) |
| Optimization (proof, spec functions) | `-O0` + `optnone` + `noinline` |
| Purpose | Verification and general WASM execution |

### Stellar Soroban

Produces Soroban-compatible WASM binaries matching the `wasm32v1-none` Rust target configuration.
The target's default optimization applies (`-Oz` for Soroban, `-O3` for Wasm32).

| Setting | Value | Source |
|---------|-------|--------|
| LLVM triple | `wasm32-unknown-unknown` | Same LLVM backend as wasm32v1-none |
| CPU | `mvp` | Pinned to WebAssembly 1.0 baseline |
| LLVM features | `+mutable-globals,+sign-ext,+bulk-memory` | Soroban VM accepts these three post-MVP features |
| inf-llc flags | `-mcpu=mvp -mattr=+mutable-globals,+sign-ext,+bulk-memory -filetype=obj` |
| rust-lld flags | `-flavor wasm --no-entry --export-dynamic --gc-sections -z stack-size=1048576 --stack-first` |
| Optimization | `OptLevel::Oz` → IR attributes `minsize`+`optsize` + `llc -O2` (size-aggressive, matching Soroban convention) |
| Max binary size | 64 KB (Soroban network limit) |
| Floats | Forbidden by Soroban VM — codegen must not emit float instructions |
| Purpose | Deploy to Stellar network as Soroban smart contracts |

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

**Soroban linker flags explained:**
- `--no-entry` — reactor model, no `_start` (same as Wasm32)
- `--export-dynamic` — export all symbols with default visibility (Soroban host discovers exports by name)
- `--gc-sections` — strip unreachable code (critical for 64KB limit)
- `-z stack-size=1048576` — 1MB stack (Soroban default)
- `--stack-first` — place stack before data in linear memory (Soroban convention)

# Appendix A: Optimization Levels

| Optimization Level | Description | LL Attribute  |
|--------------------|-------------| --------------|
| `-O0` | No optimizations. The "Gold Standard" for using debuggers like LLDB or GDB. | `optnone` + `noinline` |
| `-O1` | Some optimizations. Balanced compile time and code size. | None (default). |
| `-O2` | Aggressive optimizations. Standard Release. | None (default). |
| `-O3` | Maximum optimizations. May vectorize loops and inline aggressively. For High-Performance Computing. | None (default). |
| `-Os` | Optimize for size. Similar to `-O2` but with additional size-reducing optimizations. For Mobile / Embedded | `optsize` |
| `-Oz` | Optimize for minimum size. More aggressive size optimizations than `-Os`. For Size-Constrained Environments. | `minsize` + `optsize` |

# Appendix B: LLVM Features

| LLVM Feature | Stage | Description |
|--------------|-------| ------------|
| `-mcpu=mvp`               | Compilation | Target WebAssembly 1.0 baseline without post-MVP features. |
| `-mattr=+<feature>`       | Compilation | Machine Attributes. Enables `+` or disables `-` specific CPU features |
| `mattr: sign-ext`        | Compilation | Enable Sign-Extension Operators. Enables instructions that make converting small signed integers (like 8-bit or 16-bit int) to larger ones much faster. |
| `mattr: bulk-memory`     | Compilation | Enables instructions like `memory.copy` (like `memcpy`) and `memory.fill` (like `memset`). Without this, the compiler has to generate slow loops to copy data byte-by-byte. |
| `mattr: mutable-globals` | Compilation | Allows the Wasm module to import/export global variables that can be changed (mutated). This is often required for setting up the "Stack Pointer" for managing memory manually or linking dynamic libraries. |
| `mattr: multivalue`       | Compilation | Allows a Wasm function to return multiple values natively (e.g., returning two integers on the stack), and allows blocks/loops to have inputs. |
| `mattr: reference-types`  | Compilation | Allows Wasm to hold "opaque" references to host objects (like a JavaScript object or a DOM node) using the `externref` type. It is essential for Garbage Collection integration. |
| `mattr: tail-call`      | Compilation | Adds `return_call` instructions. If a function ends by calling another function, it reuses the current stack frame instead of creating a new one. |
| `mattr: extended-const`  | Compilation | Standard Wasm global variables can only be initialized with simple constants (e.g., `5`). This feature allows basic math in initializers, like `global x = 5 + 3`. |
| `mattr: simd128`         | Compilation | Single Instruction, Multiple Data. It allows the CPU to process 128 bits of data (e.g., four 32-bit integers) in a single clock cycle. |
| `-filetype=obj`           | Compilation | Output an Object File. |
| `-flavor wasm`            | Linking (lld) | Execute WebAssembly linking. |
| `--no-entry`              | Linking (lld) | Do not expect a `main` (`_start`) function. Useful for libraries or modules that will be invoked by a host environment. |
| `--export=main`           | Linking (lld) | Explicitly export the `main` function from the Wasm module. Necessary for standalone executables. |
| `--export-dynamic`        | Linking (lld) | Instead of picking specific functions to export, this blindly exports every global symbol. |
| `--gc-sections`           | Linking (lld) | This is Dead Code Elimination. |
| `-z stack-size=<size>`    | Linking (lld) | Sets the size of the stack for the Wasm module. |
| `--stack-first`           | Linking (lld) | Places the stack at the beginning of linear memory (before the Data/Heap). Usually, Wasm places static data (strings, globals) at the bottom (address `0`), and the stack starts after that, growing upwards (or downwards towards data). Stack-First Layout: Places the Stack at the very beginning of memory (starting near `0`) and the Data/Heap follows it. Since the stack grows downwards (towards address `0`), if the program overflows the stack, it hits address `0` and causes a Trap (crash) immediately. Without this, a stack overflow might silently overwrite static data (heap corruption). |
