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
