# Compilation Targets

## Compilation Matrix

**`non_det_operations`** = { `spec`, `uzumaki`, `forall_block`, `assume_block`, `exists_block`, `unique_block` }

| Option | Mode | Profile | Has `non_det_operations` | Behavior |
|--------|------|---------|--------------------------|----------|
| 1 | `compile` | `debug`   | no  | Compile with the chosen `Target` skipping optimizations |
| 2 | `compile` | `release` | no  | Compile with the chosen `Target` and its default optimization |
| 3 | `compile` | `debug`   | yes | Exclude `spec` nodes from codegen, then compile as `Option 1` |
| 4 | `compile` | `release` | yes | Exclude `spec` nodes from codegen, then compile as `Option 2` |
| 5 | `proof`   | `debug`   | no  | Compile all code without optimizations for formalization |
| 6 | `proof`   | `release` | no  | Same as `Option 2` |
| 7 | `proof`   | `debug`   | yes | Same as `Option 5` |
| 8 | `proof`   | `release` | yes | Compile executable code as with `Option 2` and `spec` was with `Option 5` |

**`compile` mode**: Produces optimized production binaries. Non-det `spec` nodes are stripped from codegen since they have no runtime meaning.

**`proof` mode**: Produces literal, unoptimized WASM that preserves 1:1 structural correspondence with the `spec` source code. This output feeds into the `wasm_to_v` Rocq translation for formal verification. All code (including non-det intrinsics) is emitted with `-O0` and per-function `optnone`+`noinline` barriers as defense-in-depth. The target is always `Wasm32` (custom intrinsics require strict MVP and `inf-llc`).

## Targets

Compilation target and optimization settings can be overridden in `Inference.toml`.

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
| Optimization (proof) | `-O0` |
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
