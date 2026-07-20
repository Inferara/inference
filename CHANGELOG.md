# Changelog

All notable changes to the Inference compiler project.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

### Breaking

- `inference_wasm_codegen::CodegenOutput::spec_func_indices: Vec<u32>` →
  `spec_func_indices_by_spec: FxHashMap<String, Vec<u32>>`. The accessor renames
  to `spec_func_indices_by_spec()`. Library embedders of `core/inference` must
  update both the constructor argument and the getter call site. Migration:
  replace `Vec::new()` with `FxHashMap::default()` and
  `.spec_func_indices()` with `.spec_func_indices_by_spec()` ([issue#21])
- `inference::wasm_to_v` / `inference_wasm_to_v_translator::wasm_parser::translate_bytes`:
  third parameter changed from `spec_func_indices: &[u32]` to
  `spec_funcs_by_spec: &FxHashMap<String, Vec<u32>>`. Callers must pass an
  `FxHashMap` (use `FxHashMap::default()` for the empty case). Same `_by_spec`
  rename rationale: symmetric with the `CodegenOutput` getter shape and avoids
  an extra transformation at the API boundary ([issue#21])
- Rocq output: `ValidModule` arity changed from 2 → 1 (no longer takes a specs
  list); the new `ValidSpec : module -> list N -> Prop` predicate carries the
  per-spec proof obligation. Downstream Rocq libraries must define `ValidSpec`
  and update existing `ValidModule` consumers. Theorem names also changed:
  `valid_<mod>` is now 1-arg, and per-spec theorems take the form
  `valid_<mod>__<SpecName>` (double underscore, with explicit collision
  rationale documented in `core/wasm-to-v/ROCQ_CONTRACT.md`) ([issue#17], [issue#21])
- Lower `assert(<bool>)` to a WASM trap-on-false (previously panicked codegen) ([#195])
  - Emits `<cond>; i32.eqz; if (empty); unreachable; end` — the smallest correct shape, and one that `wasm-to-v` already maps to `BI_unreachable` for proof-mode translation
  - Asserts are emitted in both `Compile` and `Proof` modes (Stmt-level, not Def-level); no `CompilationMode` branching
  - Soroban target accepts asserts — `Unreachable` is baseline WASM, not a 0xfc non-det opcode
  - New golden fixture `tests/test_data/codegen/wasm/base/assert/` exercises literal, variable, nested-in-if, loop+break, double-assert, bool param, unary `!`, `&&`, `||`, `==`, compound `(a > 0) && ((b < 10) || (c == 0))`, and bool-local scenarios, with wasmtime execution coverage that distinguishes pass paths from `Trap::UnreachableCodeReached` paths
- WASM custom section name for the per-spec function index map is now `inference.spec_funcs` (vendor-prefixed namespace). External tools previously looking for `metadata.code.inference.spec_funcs` must update. The latter was a misuse of the WebAssembly tool-conventions reserved namespace ([CodeMetadata.md](https://github.com/WebAssembly/tool-conventions/blob/main/CodeMetadata.md)) ([issue#16])
- `inference.spec_funcs` custom section payload now starts with a `varuint32` version byte (`1` for current format). Consumers should reject unsupported versions. This is a wire-format change — anyone parsing the section directly must update; the in-tree parser handles it transparently. ([issue#16])

### Changed

- Extract the shared project front end into a new leaf crate `inference-project-model` (`core/project-model`) so the IDE/LSP stack no longer transitively links the WASM/Rocq backend ([#256])
  - The crate owns the import-closure walk and `FileLoader` seam (`parse_project`, `load_project_resilient`, `DiskLoader`, `ProjectParse`, `ResilientProjectParse`, …), `read_source_file`/`strip_utf8_bom`, the `InferenceError` project errors, and manifest source-root discovery (`manifest_source_root`). Its dependencies are leaf-safe (`inference-parser`, `inference-ast`, `toml`, `rustc-hash`) — no type-checker, codegen, or wasm crates.
  - `core/inference` re-exports every one of these items unchanged, so `infc`, `infs`, tools, and tests keep reaching them as `inference::…` with no call-site churn; compiler behavior is byte-identical.
  - `ide-db` now depends on `inference-project-model` instead of the full `inference` orchestration crate. `cargo tree -p inference-ide-db` (and `-p inference-ide`, `-p inference-lsp`) links none of `inference-wasm-codegen`, `inference-wasm-to-v-translator`, `inference-wasm-linker`, `inf-wasmparser`, or `wasm-encoder`.
- Drop the always-empty `ResilientProjectParse::warnings` field (the resilient IDE walk never scans for unreachable files); the fail-fast `parse_project` keeps reporting `ProjectParse::warnings` ([#256])
- Document `RootDatabase`'s single-threaded, read-through-`&mut self` query model on `RootDatabase` and `ide/ide`'s `Analysis`: memoizing on read forecloses cancellation and parallel reads until a Salsa-style rewrite ([#157]) ([#256])
- Declare `serde_json` in `[workspace.dependencies]` and inherit it in `apps/lsp`, `apps/infs`, and `tests` ([#256])

### Language

- File-based module hierarchy (Zig-style, no `mod` keyword) ([#63])
  - Every `.inf` file is an implicit namespace. A multi-file project lives under `src/`
    with `src/main.inf` as the entry point.
  - `use a::b;` imports `src/a/b.inf` and binds the name `b` in the importing file;
    members are accessed with `::` (`b::fn()`, `a::b::fn()`). `use a::b::{x, y};`
    imports specific `pub` items and makes them available bare. `use a::b::*;` is a
    hard parse error with a guiding message.
  - `pub fn`, `pub struct`, `pub enum`, `pub const`, and `pub type` are visible to
    importing files. Everything else is file-private by default. Struct fields have no
    per-field visibility — a field is accessible whenever its struct is accessible.
    `pub spec` is a parse error; specs take no visibility modifier.
  - `pub use a::b;` re-exports a namespace so importers of the current file can
    traverse through it (Rust-style explicit re-export). Plain `use` is private.
  - Only the entry file's top-level `pub fn`s become WASM exports; non-entry `pub` is
    intra-project visibility only.
  - File import cycles are allowed; only definition-value cycles (mutually referencing
    `const` or type-alias initialisers) are hard errors (`CircularDefinition`).
  - `infs build` and `infs build -v` compile the full import-reachable closure into one
    `.wasm` (and `.v`) artifact. Unreachable `src/**/*.inf` files produce a compiler
    warning; a missing imported file errors with a nearest-match suggestion.
  - Known limitations: `pub use … from M;` external re-export is inert (wrap externals
    in a `pub fn`); top-level `const` declarations do not reach codegen (A032 / #171);
    no import aliasing (`use a::b as c;`).
- `external fn` + `use { … } from <module>` — declare and call functions from external
  `.wasm` libraries using logical (platform-independent) module references. The compiler
  emits a WASM import section with one entry per bound extern; a separate link step
  (`inference-wasm-linker`) produces a single self-contained `.wasm` and `.v` with no
  dangling imports. Tier-A (pure) and Tier-B (caller-pointer memory) closures merge
  automatically; Tier-C (own static data/globals/tables) produces a clear error with a
  relocatable-build recommendation ([#9])
- Add struct definition and parsing support ([#14])
- Add division operator (`/`) support ([#86])
- Add unary negation (`-`) and bitwise NOT (`~`) operators ([#86])
- Parse visibility modifiers (`pub`) for functions, structs, enums, constants, and type aliases ([#86])

### Compiler

- wasm-linker: New `core/wasm-linker` crate (`inference-wasm-linker`) implementing the
  static-merge link pass. `link(main_wasm, &[external_wasm])` folds satisfied imports'
  transitive closures into the main module, rewrites all index-bearing operators into a
  unified index space, deduplicates function types, preserves the `name` custom section for
  Rocq translation, and emits the unified WASM binary ([#9])
- wasm-linker: External modules using **floating-point** (any `f32`/`f64` value type in a
  signature, local, or global, or any float instruction) are now rejected by the linker. The
  Inference language has no `f32`/`f64` types and the Rocq translator models none; floats were
  previously admitted at the feature gate via `WASM1` but are now excluded. The feature gate
  (`SUPPORTED_WASM_FEATURES`) is `GC_TYPES | MUTABLE_GLOBAL | BULK_MEMORY`; the safety
  allow-list provides a second, independent backstop that rejects every float opcode with a
  diagnostic naming the exact mnemonic (e.g. `floating-point instruction 'f32.add' is not
  supported by the static merge`). **Sign-extension** and **saturating float-to-int** are
  also removed from the supported set: the Rocq translator has no lowering for either, and
  Inference codegen emits neither ([#9])
- wasm-linker: **Tail calls** (`return_call`/`return_call_indirect`) and **segment-indexed
  table ops** (`table.init`/`elem.drop`/`table.copy`) are rejected by the safety allow-list
  (`UnsupportedConstruct`). The Rocq translator has no lowering for either; Inference codegen
  never emits them, so the rejection applies only to third-party externals ([#9])
- wasm-linker: The main-module rebuild is now fail-closed on constructs the merge cannot
  preserve: a main module that declares a **start function**, imports **non-function
  entities** (globals/memories/tables) from its environment, or declares a **table section**
  is rejected up front with `UnsupportedConstruct`. Previously the start section and
  non-function imports were silently dropped — the latter shifting the global index space so
  `global.get` could read the wrong global — and table-using mains failed after the merge
  with a misleading `InvalidMergedModule`. **v128** value types are likewise rejected in
  merged signatures, locals, and block types: the Inference language has no SIMD types and
  every SIMD operator is already rejected ([#9])
- wasm-linker: Fixed an unsound Tier-B provenance rule. Pointer subtraction classified
  `Param - NotParam` as still parameter-derived; because `NotParam` only means *not provably
  parameter-derived*, the subtrahend could itself be `p - C`, so `p - (p - C) == C` fabricated
  a fixed absolute address that the analysis accepted as caller-relative — letting a Tier-B
  external read or write host memory outside the caller's buffer. Subtraction now preserves
  parameter-derivation only when subtracting a provable constant (`Param - Const`), mirroring
  the existing `add` cancellation guard. The main-module rebuild also now enforces the same
  256-level control-flow nesting cap as the external scan and the Rocq translator, rejects a
  duplicate `inference.spec_funcs` section instead of silently keeping only the last, rejects
  a multi-memory main, and rejects trailing bytes in a `spec_funcs` payload ([#9])
- wasm-linker: Merged external function names in the output name section are now
  **module-prefixed** using a `module.field` dot convention. A closure root satisfying import
  `sum` from logical module `mathlib` is recorded as `mathlib.sum`; an inner callee the
  source named `helper` becomes `mathlib.helper`; a nameless callee receives a deterministic
  fallback `mathlib.func_<idx>`. The prefix is collision-free by construction (two externals
  bound under different logical modules can export the same field without colliding in the
  name section). The Rocq translator sanitizes `.` to `_`, so `mathlib.sum` translates to
  `Definition mathlib_sum` in the `.v` ([#9])
- wasm-codegen: Emit WASM import section for `external fn` declarations. The three-stage
  index pre-scan now runs `register_imports` before local functions, so every
  `Def::ExternFunction` bound via `use … from` is assigned a function import index (lowest
  indices, `0..N`), the local-function base is shifted to `N`, and extern calls lower to
  `call <import_idx>` identically to local calls. The import section is emitted between the
  Type and Function sections per the WASM binary format; it is omitted when there are no
  externs. Function type deduplication (`intern_type`) ensures imports with identical
  signatures share one type entry ([#9])
- type-checker: `ExternOrigin { logical_module, export_field }` binds each `external fn`
  declaration to its source module; `extern_origins()` on `SymbolTable` collects all bound
  externs for use by codegen ([#9])
- ast: Remove dead `OperatorKind::BitNot` variant — `~x` is always parsed as `UnaryOperatorKind::BitNot` in a `PrefixUnaryExpression`; the binary enum variant was never produced by the AST builder ([#142])
- parser: Replace the `tree-sitter` + `tree-sitter-inference` front end with a resilient recursive-descent parser in the new `inference-parser` crate (`core/parser`). The parser lexes, parses, and lowers directly into the same `inference_ast::arena::AstArena`, producing byte-identical ASTs for all previously valid inputs, so the type-checker, analysis, codegen, and wasm-to-v phases are unchanged. The `tree-sitter`/`tree-sitter-inference` dependencies are removed from the default build, eliminating the C toolchain requirement. Parsing is now resilient (collects every syntax error instead of aborting on the first) and never panics on malformed input. `parse_external_module` moves from `inference_ast::extern_prelude` to `inference::extern_prelude` so that `inference-ast` no longer depends on the parser ([#62])
- ast: Introduce `SimpleTypeKind` enum for primitive types, replacing string-based type matching ([#50])
- ast: Simplify Builder API to return `Arena` directly instead of using state machine pattern ([#50])
- ast: Add error collection in Builder with `collect_errors()` for better parse error reporting ([#50])
- ast: Add `@skip` macro annotation for enum variants without stable node IDs ([#50])
- type-checker: Add `type_kind_from_simple_type_kind()` for type-safe primitive type conversion ([#50])
- type-checker: Add type checking for unary negation (`-`) and bitwise NOT (`~`) operators ([#86])
- type-checker: Change expression inference to use immutable references ([#86])
- ast: Use atomic counter for deterministic node ID generation ([#86])
- type-checker: Add bidirectional type inference with scope-aware symbol table ([#54])
- type-checker: Implement import system with registration and resolution phases ([#54])
- type-checker: Add visibility handling for modules, structs, and enums ([#54])
- type-checker: Implement enum support with variant access validation ([#54])
- ast: Add `#[derive(Copy)]` to `Location` for efficient stack copies ([#69])
- ast: Replace `Vec<NodeRoute>` with `FxHashMap` for O(1) parent/children lookup ([#69])
- ast: Add `get_node_source()` and `find_source_file_for_node()` convenience API ([#69])
- ast: Implement arena-based AST with ID-based node references ([#25])
- ast: Add `NodeKind` support for AST node classification ([#25])

### Codegen

- Multi-file codegen: flatten the whole import-reachable file closure into one WASM module ([#63])
  - Codegen iterates every `SourceFileData` in the arena (it previously rejected more than one source file); single-file output stays byte-identical, enforced by the `single_via_project` golden
  - Function identity is the file-qualified `FnKey` from the new `inference-fn-key` leaf crate (shared with `analysis`), so same-named functions or methods in different files receive distinct WASM indices; spec names fold per file (`fold_spec_name`) for rendering while identity stays structural
  - Struct field layout resolves a struct's fields in the struct's *defining* file, so a same-named struct in another file lays out by its own definition rather than the access site
  - Only the entry file's top-level `pub fn`s are exported; non-entry `pub` functions, methods, and spec functions stay module-internal
- Fixed: multi-dimensional scalar array literal initialization (`let g: [[i32; 3]; 2] = [[1, 2, 3], [4, 5, 6]];`) no longer panics in codegen. Previously the scalar-element branch of `lower_array_literal` assumed scalar leaves and either hit `unreachable!("Invalid element size")` for inner sub-arrays whose byte size is not 1/2/4/8 (e.g. an inner `[i32; 3]` = 12 bytes) or hit `unreachable!("array literal in unsupported position")` when it tried to lower a nested `ArrayLiteral` directly. A new recursive helper `store_array_literal_elements` descends the declared array type and stores each scalar leaf at its computed offset (mirroring `emit_array_uzumaki_recursive`); non-literal array elements (`let g = [r, r];`) are copied with `memory.copy`. Single-dimensional scalar array output is byte-identical to before
- Fixed: nested array-of-structs literal initialization (`let g: [[Pt; 2]; 2] = [[Pt{..}, Pt{..}], [..]]`) no longer panics in codegen. Previously `store_array_literal_elements` recursed to a struct leaf and hit `todo!("Unsupported array element type for store")` (a `debug_assert` fired first in debug builds). Read, write, parameter passing, and indexing of nested AoS already worked; only literal construction was missing. The helper now has a struct-leaf arm that reuses the single-dimensional AoS machinery — `compute_struct_field_layout` once per leaf level, then `lower_struct_literal_fields` for `StructLiteral` elements or a full-struct `memory.copy` for non-literal elements (`let p = Pt{..}; let g = [[p, p], [p, p]];`). Enum leaves (`[[Color; 2]; 2]`) are scalar-sized and continue through the scalar leaf path. Single-dimensional AoS (`[Pt; 3]`) never enters this helper and is byte-identical to before
- Runtime array bounds checking for dynamic indices — the dynamic half of array bounds checking ([#164])
  - When the index is a runtime value, `emit_index_offset` emits a guard (`local.tee` the index into a scratch local, `i32.ge_u` against the length, `if (empty) unreachable end`) before the offset multiply, so an out-of-range `arr[i]` traps cleanly instead of silently reading/writing adjacent frame slots. The unsigned compare also traps negative indices (which arrive as a huge `u32`). Both the read and write paths share the one `emit_index_offset` choke point, so they are guarded identically
  - Emitted for **all Compile-mode builds** (Debug and Release, Wasm32 and Soroban): `codegen()` sets the `Compiler::emit_bounds_checks` flag whenever `mode == CompilationMode::Compile`, so the executed/deployed artifact is always checked. `OptLevel` no longer affects bounds checks. **Proof** mode is left unguarded pending the proof-obligation path ([#212]), which discharges dynamic bounds as Rocq obligations rather than runtime traps
  - The scratch i32 local is reserved per function **iff the body actually contains a dynamic array index** (`body_has_dynamic_array_index`), independent of frame presence: constant-index-only functions reserve no scratch and stay byte-identical to an unchecked build, while a dynamic index through an immutable-`self` method (`self.arr[idx]`) that needs no frame slot still gets its scratch. The `unreachable` trap reuses the `assert` idiom and maps to `BI_unreachable` in the Rocq translator, keeping guarded code translatable. New `wasm_codegen_emit_bounds_check` cov-mark. Constant indices are not guarded here — they are rejected statically by analysis rule A037
  - Treating dynamic bounds as discharged Rocq proof obligations (rather than runtime traps) is the Proof-mode path tracked as [#212]; the `emit_index_offset` choke point is the seam where it hooks in
- `FunctionOrigin { TopLevel, SpecInner }` enum threaded through `visit_function_definition`. Spec-inner functions can no longer be WASM-exported even when `pub`, closing a latent footgun for the upcoming `export` keyword ([issue#19])
- Per-spec function-index map (`spec_func_indices_by_spec : FxHashMap<String, Vec<u32>>`) replaces the prior single union list. Internal `build_func_name_to_idx` keys spec-inner functions as `"<SpecName>.<fn>"` so two specs may share function names; WASM `name` section emission stays unmangled ([issue#21])
- Emit `inference.spec_funcs` WASM custom section in `proof` mode carrying the per-spec index map. Bare `.wasm` binaries are now self-describing; the Rocq translator can recover the map without an out-of-band `CodegenOutput`. The section name uses the vendor-prefixed `inference.*` namespace rather than the `metadata.code.*` namespace reserved by the WebAssembly tool-conventions repo. Section is omitted in `compile` mode so binaries stay byte-identical ([issue#16])
- `wasm-to-v` crate: new `errors.rs` with `WasmToVError` thiserror enum (`InvalidRocqIdentifier`, `RocqStdlibShadow`, `EmbeddedSpecMismatch`, `WasmParse`) and `InvalidIdentifierReason` sub-enum, closing the CLAUDE.md compliance gap that left this crate without an `errors.rs` ([issue#20])
- `wasm-to-v` crate: `validate_rocq_identifier` helper rejects Rocq-illegal module/spec names (non-alphabetic leading char, invalid chars, length > 255, stdlib shadow, reserved vernacular/Gallina keyword) before they reach `Definition <name>` emission. Called at the top of `translate_bytes` and again per spec name in `translate()` ([issue#20])
- `wasm-to-v` translator: per-spec Rocq emission. Each entry in `spec_funcs_by_spec` produces one `Definition <mod>__<SpecName>_specs : list N` and one `Theorem valid_<mod>__<SpecName> : ValidSpec <mod> <mod>__<SpecName>_specs.`. Empty per-spec lists render as `(@nil N)` so they type-check regardless of scope state at the consumer site ([issue#21], [issue#22])
- Switch from LLVM to direct WebAssembly emission via `wasm-encoder` ([#125])
  - Remove all LLVM dependencies: `inkwell`, `build.rs`, external binaries (`inf-llc`, `rust-lld`)
  - Rewrite `compiler.rs` to generate WASM binary directly in-process
  - Non-deterministic instructions emitted as custom opcodes via `Function::raw()` byte sequences
  - Custom opcodes in 0xfc prefix space: uzumaki (0x31/0x32), forall (0x3a), exists (0x3b), assume (0x3c), unique (0x3d)
  - Reactor model: all `pub` functions exported individually, no `_start` entry point
- Add compilation architecture with `CodegenOutput` boundary ([issue#97], [#125])
  - `codegen()` returns `CodegenOutput` (WASM bytes + metadata)
  - `CodegenOutput` carries WASM binary, target, mode, opt level, module name, and `has_main` flag
  - New `Target` (Wasm32/Soroban), `CompilationMode` (Compile/Proof), and `OptLevel` (O0–O3/Os/Oz) enums
- Add per-function optimization strategy for proof mode (Decision #32) ([issue#97])
  - Spec functions compiled unoptimized to preserve structural correspondence with source for Rocq translation
  - Execution functions use target's release optimization so proofs cover actual deployed code
  - `OptLevel` is currently metadata only; optimization passes planned for future
- Add validation guards in `codegen()`: reject proof mode with non-Wasm32 targets, reject Soroban with non-det operations ([issue#97])
- Upgrade shadowing detection from `debug_assert!` to `assert!` in `pre_scan_locals` — fires in release builds for parameter, constant, and variable name collisions in `locals_map`
- Add `Statement::Loop` body recursion to `pre_scan_locals()` — locals inside loop bodies are pre-registered before instruction emission
- Add loop and break statement lowering to WebAssembly codegen ([#152])
  - Conditional loop (`loop COND { body }`) emits `block`+`loop` with `br_if` exit check and `br 0` back-edge
  - Infinite loop (`loop { body }`) emits `block`+`loop` with unconditional `br 0` back-edge
  - Break statement emits `br <depth>` targeting enclosing loop's exit `block`
  - `LoopContext` tracks `wasm_block_depth` across all structured blocks (loop, if, non-det) for correct `br` depth computation
  - Nested loops, loops inside non-det blocks, and break inside nested if-statements all compute correct depths
  - Per-function state refactoring: `func`, `locals_map`, `frame_layout`, `loop_ctx`, `parent_blocks_stack` moved to `Compiler` fields, reset per function in `visit_function_definition`
- Replace silent `if let ArgumentType::Argument` skip with exhaustive `match` covering `SelfReference`, `IgnoreArgument`, and `Type` variants, each with an explicit `todo!()`
- Add fixed-size array support with linear memory allocation ([#148])
  - Shadow stack with `__stack_pointer` mutable global, stack-first layout matching Rust/Zig convention
  - Stack-first: stack at address 0 grows downward, overflow traps via WASM OOB — no explicit guard needed
  - New `memory.rs` module: `PAGE_SIZE`, `STACK_SIZE`, `STACK_POINTER_INIT` constants, `FrameLayout`, `ArraySlot`, prologue/epilogue, param copy, load/store helpers
  - Array literal lowering: `let arr: [i32; 3] = [1, 2, 3];` stores elements in linear memory
  - Array index read: `arr[i]` loads elements via computed address (base + index * elem_size)
  - Array index write: `arr[i] = value;` stores elements via computed address
  - Array parameter copy-on-entry: value semantics — callee copies data into own frame, cannot mutate caller's array
  - Unrolled copy for small arrays (N <= 16), `memory.copy` for larger arrays
  - Element-wise uzumaki expansion: `let arr: [i32; 3] = @;` stores per-element `i32.uzumaki`
  - Zero-initialization of all array memory via `memory.fill` in function prologue
  - Conditional Memory/Global/Export sections — only emitted when functions use arrays
  - Sign-appropriate load/store for sub-i32 types (i8→load8_s, u8/bool→load8_u, etc.)
  - 16-byte frame alignment matching LLVM/Rust WASM convention
  - Per-type alignment padding: each array within a frame is aligned to its element type's natural alignment (1/2/4/8 bytes), matching LLVM/Rust/BasicCABI convention; padding bytes zeroed by prologue `memory.fill`
  - Constant-index folding: `arr[0]` emits no offset computation (load/store directly at base); `arr[N]` for constant N folds `N * elem_size` to a single compile-time `i32.const`; variable-index access uses runtime multiply
  - Array return types via sret (struct-return) calling convention matching Rust/Zig: hidden `$sret` parameter at index 0, void WASM return, caller allocates destination in its own frame
  - Three sret return expression cases: identifier (`return arr` → `memory.copy`), array literal (`return [1,2,3]` → element-wise stores), function call (`return inner()` → zero-copy sret forwarding)
  - Sub-i32 narrowing after arithmetic: signed types use shift-left/arithmetic-shift-right, unsigned types use AND mask; skipped for comparisons, Mod, Shr, bitwise ops
- Add struct type support with linear memory allocation ([pr#159])
  - Struct fields laid out in declaration order with C-style natural alignment padding
  - `compute_struct_field_layout()` computes per-field byte offsets and total struct size
  - `StructSlot` and `StructFieldSlot` types in memory.rs for frame layout tracking
  - Struct literal lowering: field-by-field stores into frame slot at computed offsets
  - Member access read: struct pointer + field offset + load instruction for field type
  - Member access write: struct pointer + field offset + store instruction, with cached layout lookup via `resolve_struct_field_offset()`
  - Struct parameter copy-on-entry via `memory.copy` — callee copies entire struct into own frame
  - Struct return via sret calling convention: hidden `$sret` param, void WASM return, field-by-field or `memory.copy` return
  - Struct-to-struct copy: `let q = p` emits `memory.copy` preserving value semantics
  - Struct reassignment: `p = q` uses `memory.copy` to destination frame slot (not pointer aliasing)
  - Struct literal reassignment: `p = Point { x: 3, y: 4 }` writes fields directly to existing frame slot
  - Uzumaki for all primitive types: bool, i8-u64 emit `i32.uzumaki` or `i64.uzumaki` as appropriate
  - Struct uzumaki (`let p: Point = @;`) now supported: `lower_struct_uzumaki` emits per-field uzumaki opcodes followed by stores (`wasm_codegen_emit_struct_uzumaki`)
- Add struct method codegen with instance methods, associated functions, and cross-calls ([pr#178])
  - Methods compiled as top-level WASM functions with mangled names (`TypeName.method_name`)
  - Two-phase traversal: register all function + method indices before compiling any bodies (enables forward references)
  - `self` parameter lowered as `ValType::I32` struct pointer at param index 0
  - Immutable `self` reads directly from caller pointer (zero-copy optimization); mutable `self` uses copy-on-entry
  - Instance method calls (`p.get_x()`) resolve receiver type, push struct pointer as implicit first argument
  - Associated function calls (`Point::new(1, 2)`) resolve mangled name without receiver
  - Methods returning compound types (structs, arrays) use sret calling convention
  - `ResolvedCallee` enum consolidates three callee patterns (Function, AssociatedFunction, InstanceMethod) across all call paths
  - `assert!` on mangled name collision: detects `TypeName.method_name` conflicts with top-level functions in release builds
- Add enum type codegen: unit enum variants lowered as i32 constants with zero-based tags ([pr#187])
  - Enum variant access (`Color::Red`) emits `i32.const <tag>` via `TypeMemberAccess` lowering
  - Enums work in all value positions: locals, parameters, return values, struct fields, arrays, const declarations
  - Equality (`==`) and inequality (`!=`) comparisons use native i32 instructions
  - Uzumaki support: `let c: Color = @;` emits `i32.uzumaki` in non-det blocks
  - Enum-typed struct fields stored as 4-byte i32 scalars with proper load/store/alignment
  - Arrays of enums (`[Color; N]`) use element_size=4 with standard array memory layout
  - `EnumInfo.variants` changed from `FxHashSet` to `Vec` for deterministic declaration-order tag assignment
  - `TypedContext::lookup_enum()` exposed for cross-crate enum metadata access
  - Analysis `has_compound_fields()` made enum-aware: enum-typed `Custom` fields treated as scalar
- Add nested compound type codegen: struct-in-struct, array-in-struct, struct-in-array ([pr#185])
  - Recursive `type_byte_size()` computes byte sizes for nested compound types via `TypedContext` struct lookup
  - `CompoundFieldLayout` enum (`Scalar`, `NestedStruct`, `NestedArray`) caches sub-layout on `StructFieldSlot` for efficient chained access
  - Pointer semantics for compound member/index access: compound fields push i32 pointer, load only at terminal scalar field
  - Struct-in-struct: nested struct literals, chained field access (`outer.inner.x`), field writes, parameter passing, sret return, copy
  - Array-in-struct: array field literals, index access through struct (`s.arr[i]`), field writes, parameter passing
  - Struct-in-array: struct element literals, field access through index (`arr[i].field`), element writes, sret return
  - Method support for nested types: `self.inner.x` and `self.arr[i]` via pointer chaining
  - Multidimensional array uzumaki: `[[i32; 3]; 2] = @` emits per-element uzumaki stores in non-det blocks
  - Struct uzumaki with array fields: `let s: HasArray = @;` emits per-element uzumaki for array-typed fields
  - `element_layout: Option<Vec<StructFieldSlot>>` on `ArraySlot` for cached struct-element array layouts
  - One level of compound nesting permitted (enforced by analysis rule A026)
- Add per-element zero-store elision in array and struct literal codegen ([#188])
  - Individual stores of zero-valued elements skipped during variable initialization — the prologue `memory.fill 0` already zeroed the frame
  - Per-element granularity: mixed arrays like `[0, 1, 0]` emit only the non-zero store
  - `is_syntactic_zero()` recognizes `0`, `-0`, `false`, parenthesized and negated zero forms
  - Applies to scalar arrays, struct fields, nested array-in-struct and struct-in-array fields
  - Correctly scoped to frame-local initialization only — sret return paths and assignment always emit all stores
  - `init_zero_elision` flag on `Compiler` gates elision to `VarDef` context; `skip_zero_stores` parameter threads through recursive helpers
- Eliminate dead trailing epilogue in non-void functions ([#188])
  - Remove unreachable `emit_stack_epilogue` before the trailing `unreachable` sentinel
  - Each `return` statement already emits its own epilogue; the trailing one was dead code
  - Precondition: analysis rule A007 guarantees all non-void functions return on every path
  - Reduces WASM binary size across all non-void functions with stack frames
- Add assignment statement lowering to WebAssembly codegen ([#146])
  - `mut` keyword support in AST: `is_mut: bool` field on `VariableDefinitionStatement`
  - Mutability enforcement in type-checker: `AssignToImmutable` error for assignment to non-`mut` variables
  - `lower_assign_statement()` emits `lower_expression(rhs)` + `LocalSet` for identifier targets
  - Mutable function parameters (`fn f(mut a: i32)`) supported
  - Number literal type propagation in assignments: `x = 42;` where `x: i64` correctly infers `42` as `i64`
  - Array index assignment targets (`arr[i] = value`) now supported via memory store instructions
- Add conditional statement lowering (`if`/`else`) to WebAssembly codegen ([#144])
  - `if`/`else` lowered to WASM structured control flow (`If`/`Else`/`End` with `BlockType::Empty`)
  - `pre_scan_locals` recurses into both if and else arms to declare locals upfront (WASM requirement)
  - Nested if statements supported via recursive descent
  - Emit `unreachable` instruction before function `end` for all non-void functions as defense-in-depth safety net (industry-standard pattern used by rustc, LLVM, GCC, Zig, Binaryen)
  - If-statements inside non-deterministic blocks (`forall`, `exists`, etc.) supported
- Add binary and unary expression lowering to WebAssembly codegen ([#140])
  - All arithmetic operators (`+`, `-`, `*`, `/`, `%`) for i32 and i64, signed and unsigned variants
  - All comparison operators (`==`, `!=`, `<`, `<=`, `>`, `>=`) with correct sign-sensitive dispatch
  - All logical operators (`&&`, `||`) lowered as bitwise `i32.and`/`i32.or` (bool operands guaranteed by type-checker)
  - All bitwise operators (`&`, `|`, `^`) and shift operators (`<<`, `>>`) for i32 and i64
  - Unary negation (`-x`) via `0 - x` idiom (no native WASM integer negate instruction)
  - Logical not (`!x`) via `i32.eqz`
  - Bitwise not (`~x`) via `x ^ -1` idiom (works for both i32 and i64)
  - Parenthesized expressions lowered transparently (no extra instructions emitted)
  - Variable definition initializers now accept any value-producing expression (not just literals/identifiers/uzumaki)
  - `Pow` operator (`**`) deferred — no native WASM instruction
- Add function parameter lowering and function call support to WebAssembly codegen ([#136])
  - Function parameters mapped to WASM local indices `0..n`; body locals start at `n`
  - Pre-scan builds `func_name_to_idx` map for forward reference support
  - `Expression::FunctionCall` lowered to `call` instruction with positional arguments
  - Void function calls in expression-statement position correctly omit `Drop`
  - Value-returning function calls in expression-statement position emit `Drop`
- Add local variable lowering (`let` bindings) to WebAssembly codegen ([pr#135])
  - Emit `local.set` / `local.get` for variable definitions with literal, identifier, and uzumaki initializers
  - Support all numeric types (i8, i16, i32, i64, u8, u16, u32, u64), bool, and uzumaki
  - Type-checker propagates declared type into numeric literal initializers for sub-i32 types
  - Refactor `ConstantDefinition` lowering to share `lower_literal` helper with `VariableDefinition` (~130 lines removed)
  - Remove dead `is_uzumaki: bool` field from `VariableDefinitionStatement` AST node
- Add LLVM-based WASM code generation using `inf-llc` ([#44])
- Add custom LLVM intrinsics for non-deterministic instructions ([#44])
- Implement `forall`, `exists`, `uzumaki`, `assume`, `unique` block codegen ([#44])
- Add `rust-lld` linker invocation for WASM linking ([#44])
- Add mutable globals support in WASM compilation ([#44])
- Add base WASM code generation from typed AST ([#29])

### Analysis

- Whole-program call graph for the module hierarchy, keyed on the shared `FnKey` ([#63])
  - A035 (recursion) and A036 (stack depth) span files: cross-file `::` / `root::` call edges are resolved and an imported struct's frame is sized from its defining file, so cross-file recursion and >64 KB cross-file stack chains are caught instead of compiling and overflowing at runtime
  - The call graph indexes the structured `FnKey` from `inference-fn-key`, never a flattened name, so same-named functions across files stay distinct nodes
- Restore the duplicate-`FnKey` tripwire in `resolve_adjacency`, now tolerant of parse-recovered keys ([#255])
  - The LSP server ([#239]) rewrote `resolve_adjacency` to keep-first on any duplicate `FnKey` in every build, silently dropping the previous `debug_assert!(false)` that guarded `FnKey` injectivity; a genuine duplicate means a recursive self-edge can resolve to the wrong same-keyed node and mask a cycle from A035/A036 (the #63 canonical-key bug class)
  - That removal was necessary because the resilient IDE path lowers every unparseable construct to an `<error>` placeholder function, so a broken parse legitimately yields two nodes under one key and the old assert aborted debug builds (and the LSP process) on it
  - The tripwire now fires in debug builds only when the duplicate key carries no parser recovery marker (`is_parse_recovered`); recovered keys are exempt and the keep-first behavior is unchanged in every build, so release builds and resilient parses still degrade deterministically
- Add `core/analysis/` crate with rule-based static analysis between type checking and codegen ([#156])
  - Five analysis rules: A001 break-outside-loop, A002 break-in-nondet, A003 return-in-loop, A004 infinite-loop-without-break, A005 return-in-nondet
  - `Rule` trait with `rule!` declarative macro for zero-boilerplate rule definitions
  - Shared AST walker (`walk_function_bodies`) with `loop_depth` and `nondet_depth` tracking
  - Three-severity model: `Error` (blocks compilation), `Warning`, `Info`
  - Diagnostic format: `<line>:<column>: <severity>[<rule_id>]: <message>`
  - Rules are zero-sized `Send + Sync` structs for future parallel execution
- Expand analysis pass from 5 to 22 rules; migrate 13 checks from the type checker
  - Type checker now enforces only type correctness; all other semantic checks live in analysis
  - New control-flow rules: A006 uzumaki-outside-nondet, A007 missing-return (branch-aware), A008 standalone-uzumaki
  - New lint warnings: A009 empty-enum, A010 method-never-accesses-self, A011 empty-struct
  - Migrated codegen restriction rules: A012 array-literal-as-argument, A013 struct-literal-as-argument, A014 array-uzumaki-as-argument, A015 compound-literal-in-unsupported-position, A016 compound-return-call-in-expression-position, A017 compound-return-call-in-assignment, A018 method-call-chain-on-compound-return, A019 array-index-64bit, A022 literal-out-of-range
  - New rules: A023 uzumaki-in-reassignment, A024 extern-function-call
  - `AssignToImmutable` and `VariableShadowed` remain in the type checker (require scope state)
- Add 5 analysis rules for nested compound type constraints ([pr#185])
  - A026 `NestedCompoundDepth`: reject struct field nesting deeper than one level (definition-site check)
  - A027 `UzumakiOnNestedStruct`: reject uzumaki on structs with compound fields
  - A028 `UzumakiOnStructInArray`: reject uzumaki on arrays of structs at any dimension depth
  - A029 `CompoundLiteralMemberAssign`: reject compound literal assignment directly to compound elements
  - A031 `UnsupportedCompoundReturnExpr`: reject complex return expressions in compound-returning functions
  - Walker helpers: `has_compound_fields()`, `array_nesting_depth()`, `is_compound_return_call()`
- A033 `CombinedUnaryOperators`: reject adjacent prefix unary operators such as `--x`, `~~x`, `-~x`, `!!x`, and parenthesized variants like `-(~x)` (issues [#82], [#81]; PRs [#111], [#117])
- A035 `RecursionDetected`: reject all direct and mutual/indirect recursion (Power of 10, Rule 1) so stack usage stays statically bounded ([#205])
  - Builds a whole-program call graph keyed by the canonical function name (matching the codegen `FnKey` scheme); call resolution is conservative, so edges are created only to existing nodes and the rule never produces a false positive
  - Reports each call cycle once via a white/gray/black DFS, naming the full cycle (e.g. `a -> b -> a`) and pointing the diagnostic at the call site that closes it
  - Migrated the recursive codegen fixtures to iterative form to comply with the new rule: rewrote `algo_bitwise` (`popcount`, `count_leading_zeros`), `algo_converge` (`slow_div`, `slow_mod`, `peasant_mul`, `is_prime`, `collatz_steps`, `collatz_max`), and `algo_i64_mixed` (`factorial_i64`, `fibonacci_i64`, `gcd_i64`) into conditional loops with `mut` accumulators and a single trailing return; removed the wholly recursive `algo_recursive_math` fixture (its functions already have iterative equivalents in `algo_iter`)
- A036 `StackDepthExceeded`: reject programs whose cumulative shadow-stack usage along a call chain exceeds the 64 KB stack budget, turning the previously opaque runtime `memory.fill` out-of-bounds trap into a precise compile-time error ([#166])
  - Reuses A035's whole-program call graph (now a DAG, since recursion is forbidden) and computes the maximum-weight root-to-leaf path, where each node's weight is a conservative upper bound on that function's compound (array/struct) frame size; scalar locals live in WASM locals and contribute nothing
  - The frame-size estimator computes each compound type's **exact** codegen size (mirroring `compute_struct_field_layout` field-by-field, including array-of-structs) and adds only a flat worst-case leading-padding margin once per frame slot, then rounds to the 16-byte boundary — so it remains a sound upper bound on codegen's `FrameLayout.total_size` (never accepts a program codegen would overflow) without falsely rejecting valid array-of-structs frames; `if`/`else` branches take the per-branch maximum, mirroring codegen's offset reuse
  - The longest-path DFS is cycle-safe (white/gray/black coloring); a recursive program is reported by A035 while A036 does not hang
  - Factored the shared call-graph construction into `core/analysis/src/call_graph.rs`, consumed by both A035 and A036
  - Diagnostic names the offending chain (e.g. `a -> b -> c`) and reports the computed byte total against the budget
  - The estimator's soundness (estimate ≥ codegen's real frame) is enforced cross-crate: `inference_analysis::estimate_frame_sizes()` and `CodegenOutput::frame_sizes()` expose per-function sizes (keyed by canonical name), and a parity test asserts estimate ≥ real over a corpus of struct, mixed-alignment, nested, array-of-struct, mutable-self, and if/else cases. A codegen test guards the ≤8-byte max-alignment invariant that `MAX_SLOT_PADDING` relies on
- A037 `ArrayIndexConstOutOfBounds`: reject a constant array index (`arr[c]`) that is negative or `>= length`, the static half of array bounds checking ([#164])
  - The array length is read from the array sub-expression's `Array(_, length)` type info, so the check is zero-runtime-cost and fires in every build profile and compilation mode; the literal index is parsed as `i128` so out-of-`i32` values are caught too
  - A negative literal such as `arr[-1]` lowers to a single `NumberLiteral` whose text keeps the leading `-`, so it is rejected here as well; the diagnostic names the offending index and the array length
  - Dynamic (non-literal) indices are out of scope for the static rule and are guarded at run time in all Compile-mode builds (see Codegen)
- A038 `UzumakiOnCompoundField`: reject uzumaki (@) on a struct- or array-typed
  struct-literal field (e.g. `Outer { i: @ }`); it previously slipped past A027 and
  panicked proof-mode codegen with "Struct/Array uzumaki ... has no enclosing
  variable name" ([#225])
- A039 `StructUzumakiAsArgument`: reject a struct-typed uzumaki (@) passed directly as a
  function argument (e.g. `f(@)` where the parameter is a struct); the array case was
  already A014, but the struct case slipped through and panicked codegen with
  "Struct uzumaki ... has no enclosing variable name". Sibling of #225 ([#225])
- A040 `UzumakiOnCompoundArrayElement`: reject a struct- or array-typed uzumaki (@)
  element of an array literal (e.g. `[Point { .. }, @]`); a scalar element `@` is now
  supported (the type checker threads the declared element type onto it), but a compound
  element has no enclosing variable name and panicked codegen. Distinct from A028
  (whole-array `@`), and also covers a nested-array element such as the outer `@` in
  `[@, [1, 2]]`. The array-element sibling of #225's struct-literal-field fix ([#225])
- A041 `DuplicateLocalName`: reject duplicate function-local names across disjoint
  sibling blocks (if/else arms, sequential ifs, non-det blocks) with a two-location
  diagnostic instead of panicking in codegen ([#217])

### AST

- Migrate AST arena from `FxHashMap<u32, AstNode>` + `Rc<T>` + `RefCell<T>` to typed `Arena<T>` via vendored la-arena ([#156])
  - Typed indices (`ExprId`, `StmtId`, `DefId`, `BlockId`, `TypeId`, `IdentId`) prevent cross-category ID misuse at compile time
  - `AstArena` struct with separate `Arena<T>` per node category and `Index` trait for `arena[id]` syntax
  - `NodeId` enum for type-erased cross-category references (used in type annotation storage)
  - `Send + Sync` with compile-time assertion — no `RefCell` or `Rc` in AST nodes
  - Cache-friendly `Vec<T>` storage replacing heap-scattered `Rc<T>`
  - Remove `AstNode` enum, `ast_node!`/`ast_enum!`/`ast_enums!` macros, `enums_impl.rs`, `parent_map`/`children_map`

### CLI

- Add `infc --out-dir <path>` flag to redirect compilation artifacts ([#223])
  - Default remains `out/` relative to the current working directory, preserving prior behavior
  - When supplied, both the `.wasm` and the `.v` (if requested) are written under the given directory
  - Pure output plumbing — `infc` gains no project awareness; `infs` uses it in project mode to honor `[verification] output-dir`
  - Compiler ABI minor version bumped 0 → 1 to advertise the additive flag; the `infs`↔`infc` handshake treats the bump as backward compatible (an older binary on either side simply never sends or sees the flag)
- `infc -v` (and `infs build -v`) now implies `--mode proof` when no explicit `--mode` is passed. Users wanting the prior behavior (V output from compile-mode WASM, stripped specs) can pass `--mode compile -v` explicitly. Closes a UX trap where `-v` alone produced a near-useless empty-specs `.v` file. ([issue#22])
- `infc --mode proof` and `infs build --mode proof` flags enable Rocq translation output. By default both tools run in `compile` mode (existing behavior, stripped specs). `--mode proof` keeps spec functions and writes the `.v` proof artifact alongside the `.wasm`. ([issue#22])
- `infc` now surfaces `WasmToVError::RocqStdlibShadow` and `WasmToVError::InvalidRocqIdentifier` with the dedicated user-facing messages from the plan (no `--module-name` flag mentioned — that flag does not exist yet) ([issue#20])
- Simplify `infc` and `infs build` default behavior: running without phase flags now performs full compilation and writes `out/<name>.wasm` ([#138])
  - `infc example.inf` equivalent to `infc example.inf --codegen -o`
  - `infc example.inf -v` produces both `out/example.wasm` and `out/example.v`
  - Supplying `--parse`, `--analyze`, or `--codegen` still overrides the default
  - Matches conventional compiler UX (e.g. `gcc foo.c`)
- Add `BuildProfile` (Debug/Release) with `resolve_opt_level()` for target-aware optimization ([issue#97])
- Remove external toolchain dependencies: no `inf-llc`, `rust-lld`, or platform-specific library paths required ([#125])
- Defer WASM compilation until output files are actually needed (`-o` or `-v` flags) ([issue#97])
- Refactor CLI architecture with improved argument handling ([#28])

### Rocq Translation

- WASM module-name subsection now reflects the CLI-supplied input file stem instead of the hardcoded `"output"`. The Rocq translator reads this back, so the emitted `Definition <mod>__<Spec>_specs` and `Theorem valid_<mod>` identifiers now use the source filename. Multi-module workflows that previously collided on a single `output` identifier now produce distinct ones
- Empty per-spec lists now emit `(@nil N)` instead of `[]%N` so the generated `Definition` type-checks regardless of whether `Open Scope N_scope` is active at the consumer's `Require` site. Downstream proof scripts matching `[]%N` literally must update ([issue#21], [issue#22])
- Rewrite WASM-to-V translator for WasmCertCoq theory syntax ([#23])
- Add function name propagation to V output ([#24])

### Documentation

- New `core/wasm-to-v/ROCQ_CONTRACT.md` documenting the external Rocq predicates the generator depends on (`ValidModule` 1-arg, new `ValidSpec`), the emitted proof-skeleton shape, and the spec-map precedence rules (explicit vs embedded) ([issue#17])
- Add compilation targets matrix documentation (`book/compilation_targets.md`) ([issue#97])
  - 6-option matrix: Compile/Proof x Debug/Release x with/without non-det operations
- Add `unreachable` emission rationale document (`book/unreachable-emission-in-codegen.md`) ([#144])
- Add arithmetic overflow in WASM codegen deep-dive (`book/arithmetic-overflow-in-wasm-codegen.md`) ([#146])
  - WASM wrapping semantics, trapping instructions, negation behavior
  - Comparison with Rust, C, Zig, Go, Java overflow handling
  - Formal verification implications for Rocq translation
  - Empirical comparison: Inference vs rustc release vs rustc debug vs Soroban

### Type Checker

- Cross-file name resolution and file-based visibility for the module hierarchy ([#63])
  - Each source file gets a nested file scope (`enter_file_scope`) keyed by its source-root-relative module path; structs/enums/consts are stored under canonical file-qualified keys (`canonical_key_for_scope`) so same-named types in different files never unify
  - Type/struct/enum identity carries the canonical defining-file key and all assignability comparisons use canonical equality, closing cross-file same-named-type confusion (a heap-OOB class of bug)
  - Visibility is enforced at one `same_file` chokepoint: a non-entry file cannot reach another file's items — even `pub` ones — by bare name, only through `use` + `::`; entry items are reachable only via the reserved `use root;` handle
  - Each resolved call records a `CallTarget { module_path, name, receiver_struct }` naming the callee's defining file, so codegen and analysis consume one authoritative identity instead of re-deriving it
  - Cross-file definition-*value* cycles (`const` / type-alias initialisers) are detected by a new `definition_graph` and reported as `CircularDefinition`; file import cycles remain legal
- Spec-inner functions whose bare name shadows a top-level function are now rejected (`SpecFunctionShadowsTopLevel`). Codegen's spec-aware call resolution and the type checker's nearest-binding rule disagreed silently on which callee was invoked from inside a spec; the rejection forces the user to rename one side
- Same-named structs or enums across spec blocks are now rejected at registration time (previously silently used the first-registered layout). Cross-spec mangling of struct/enum identity would require carrying spec context through every type access (field projection, sret layouts, method dispatch); rejecting at registration avoids that blast radius and surfaces a clear `RegistrationFailed` diagnostic. Functions remain mangleable across specs (`"<Spec>.<fn>"`) as before
- Spec blocks now open a real symbol-table scope via `enter_spec`, parallel to `enter_module`. Spec-inner functions, structs, enums, type aliases, and constants live in a dedicated scope keyed by spec name, so two specs may declare same-named members without colliding ([issue#18])
- `flatten_defs_with_spec_inner` removed. The three phases that used it (`register_types`, `collect_function_and_constant_definitions`, and the body-inference loop) recurse into `Def::Spec` inline, opening the spec scope around the inner work ([issue#18])
- `TypedContext::lookup_struct` and `lookup_enum` now search across **all** scopes (`lookup_struct_anywhere` / `lookup_enum_anywhere`) so post-type-check phases (analysis, codegen) can resolve spec-inner types they walk into. Internal scope-local lookups inside the type checker are unchanged ([issue#18])
- Add `resolve_custom_type()` to `SymbolTable` to fix `Custom` vs `Struct`/`Enum` type resolution mismatch ([#148])
  - Resolves `TypeInfoKind::Custom(name)` to `Struct(name)` or `Enum(name)` at function registration time
  - Recurses into array element types (handles `[MyStruct; 3]`)
  - Called at 9 sites throughout the type checker
- Add argument type validation at all function/method call sites ([#148])
  - Associated functions, instance methods, and free functions all validate argument types against parameter signatures
  - Uses plain `!=` (PartialEq) instead of compatibility shim
- Add i64 array element type propagation ([#148])
  - Propagates element type from `[i64; N]` annotation to number literals in array initializers
- Add array element assignment mutability check ([#148])
  - `arr[i] = value` requires `arr` to be declared `mut`
  - `extract_root_variable_name` resolves root identifier from nested index access and member access expressions ([pr#159])
  - Struct field assignment (`p.x = 42`) requires the struct variable to be declared `mut` ([pr#159])
- Add `VariableShadowed` error: variable declaration that shadows a name from an outer scope is a hard error ([pr#159])
  - Aligns with MISRA C Rule 5.3 and NASA Power of 10
  - `lookup_variable_in_parent_scopes()` added to symbol table to detect shadowing before registration
- Add `ArrayReturnCallInExpressionPosition` error: rejects array-returning function calls in unsupported positions ([#148])
  - Only `let x = foo()` and `return foo()` are permitted for sret calls
  - Standalone calls, argument positions, index access, and assignment RHS all rejected with clear diagnostic
  - Guards at 6 sites: Statement::Expression, ArrayIndexAccess, 3 argument validation loops, Statement::Assign
- Add struct literal field validation ([pr#159])
  - `MissingStructField`: reject struct literals missing required fields
  - `UnknownStructField`: reject struct literals with fields not in the struct definition
  - `DuplicateStructField`: reject struct literals with repeated field names
  - Field value type mismatch: reject `Point { x: true }` when `x: i32`
- Add `MethodNeverAccessesSelf` error: methods declaring `self` but never using it ([pr#159])
- Add `EmptyStruct` error: reject struct definitions with no fields or methods ([pr#159])
- Add `StructLiteralAsArgument` error: reject struct literals as direct function arguments ([pr#159])
- Add `CompoundLiteralInUnsupportedPosition` error: reject struct/array literals in arbitrary expression positions ([pr#159])
  - Compound literals allowed only in variable declarations, assignments, return statements, and struct field values
- Extend `ArrayReturnCallInExpressionPosition` to also reject struct-returning calls in expression positions ([pr#159])
  - Covers `MemberAccess` on sret-returning calls (e.g., `make_point().x`)
  - Error message updated from "array-returning" to "compound-returning"
- Add const initializer type validation: `const x: i32 = true;` now rejected ([pr#159])
- Add number-to-bool assignment rejection: `let x: bool = 0;` now rejected ([pr#159])
- Add ordering comparison validation: `true < false` now rejected; equality (`==`/`!=`) still allowed on all types ([pr#159])
- Fix duplicate `BinaryOperandTypeMismatch` error for mixed-type arithmetic ([pr#159])
- Remove dead code: `types_equal` function, `is_compatible_with` method, `param_names` field from `FuncInfo`
- Add `find_enclosing_variable_name()` to `TypedContext` for walking AST parent chain to enclosing variable
- Rename `ArrayReturnCallInExpressionPosition` to `CompoundReturnCallInExpressionPosition` to reflect struct coverage ([pr#178])
- Add `CompoundReturnCallInAssignment` error: rejects compound-returning function calls in assignment RHS ([pr#178])
  - `p = make_point()` rejected; use `let p = make_point()` instead
- Add `MethodCallChainOnCompoundReturn` error: rejects method call chains on compound-returning functions ([pr#178])
  - `p.translate(1, 2).get_x()` rejected; assign intermediate result to a variable first
  - Deliberate design choice: implicit temporaries cannot be named in formal proofs
- Add `MethodMetadata` public struct and `TypedContext::lookup_method()` for cross-crate method metadata access ([pr#178])
- Migrate 13 codegen restriction checks from type checker to analysis pass
  - Removed from `TypeCheckError`: `LiteralOutOfRange`, `ArrayLiteralAsArgument`, `StructLiteralAsArgument`, `ArrayUzumakiAsArgument`, `CompoundLiteralInUnsupportedPosition`, `CompoundReturnCallInExpressionPosition`, `CompoundReturnCallInAssignment`, `MethodCallChainOnCompoundReturn`, `ArrayIndex64Bit`, `EmptyStruct`, `MethodNeverAccessesSelf`; plus `UzumakiInReassignment` and `ExternFunctionCall` which are new
  - Type checker now produces 46 error variants (down from 50)
- Add 7 new `TypeCheckError` variants for validation hardening
  - `DuplicateStructFieldDefinition` — duplicate field names in a struct definition
  - `RecursiveStructDefinition` — field type creates an infinite-size cycle (direct, array, or alias)
  - `InvalidAssignmentTarget` — assignment LHS is not a valid lvalue
  - `UninitializedVariable` — variable declared without an initializer
  - `ArrayLiteralSizeMismatch` — array literal element count differs from declared size
  - `DivisionByZero` — literal zero in divisor position of `/` or `%`
  - `DuplicateEnumVariant` — duplicate variant names in an enum definition
- Fix undeclared types in variable definitions now validated (previously missed in some positions)
- Fix case-insensitive type lookup removed — `I32` no longer resolves to `i32`; all type names are case-sensitive
- Fix `from_builtin_str` uses exact case-sensitive matching
- Fix external function parameter parsing corrected in AST builder (previously dropped parameters in some cases)
- Bump `tree-sitter-inference` grammar from 0.0.39 to 0.0.40 — fixes chained member access parsing
- Add `compound_literal_allowed` propagation into nested struct literal fields and array literal elements ([pr#185])
  - `Outer { inner: Inner { x: 1 } }` correctly accepted in variable declarations
  - Array literals inside struct fields accepted: `HasArray { arr: [1, 2, 3] }`
- Add `find_enclosing_variable_name()` to `TypedContext` for analysis rule uzumaki struct name lookup ([pr#185])

### Testing

- Add a `coqc` round-trip gate for proof-mode `wasm-to-v` output ([#231])
  - Every prior `wasm-to-v` test string-matched the emitted `.v` and never type-checked it, so a mis-aritied or renamed Rocq constructor (the [#230] `BI_forall`/`BI_exists` arity class) passed CI and failed only on the paid prover worker
  - New vendored signature stub `core/wasm-to-v/rocq-stub/` provides the logical library `Wasm` (`bytes`, `numerics`, `datatypes`, `verifier`) as signatures only — no semantics, no proofs — encoding each external declaration with the arity/shape the emitter writes, so a regression becomes a `coqc` type error
  - The stub declares only the operator surface reachable from Inference-generated wasm: integer families plus the non-deterministic instructions, with no floating-point (`f32`/`f64`, `T_f32`/`T_f64`, `VAL_float*`, `relop_f`/`binop_f`/`unop_f`), no `cvtop`, and no SIMD (`T_v128`) — Inference has no floats and its codegen emits no conversions, so an accidental emission of an unsupported operator fails the gate instead of silently type-checking; the translator's dead (and ill-typed) float arms are tracked in [#284]
  - New gated test `tests/src/rocq_typecheck.rs` drives the in-process pipeline (parse → type-check → proof-mode codegen → `wasm_to_v`) over a corpus spanning the proof surface — inline and function-body-modifier `forall`/`exists`/`assume`, `unique`, `BI_call`, comparisons, `assert`, and `if`/`loop` control flow — then compiles each generated module against the stub; it rewrites the emitted `(* TODO *)` `Qed.` to `Admitted.` so it checks statements + definitions without requiring proofs to close
  - Two new corpus fixtures, `tests/test_data/inf/rocq_control_flow.inf` and `rocq_unique.inf`; existing spec fixtures are reused
  - The test is gated on `coqc` availability (`COQC` override, else `PATH`): it skips with a clear message when absent, and the new `.github/workflows/rocq-typecheck.yml` CI job installs Coq via apt so the gate is real on every PR. Wiring the full private WasmCert-Coq-Essence library into CI needs org secrets and remains a follow-up
- Close the LSP/IDE test-coverage gaps from the PR #239 review ([#254])
  - `ide-db` invalidation: selectivity is now pinned with several memoized analyses coexisting (a keystroke in one open buffer leaves unrelated buffers' analyses at their exact generation; editing a shared import recomputes every dependent but not an independent buffer), plus transitive-closure invalidation (edit `C` in `A→B→C` recomputes `A`), invalidation on editing a member of an import cycle, and `close_document`'s disk-fallback with divergent overlay/disk content (both the entry itself and a still-open dependent re-read the divergent disk text)
  - LSP e2e: requests after `didClose` (disk-backed doc answers from disk, never-on-disk doc answers null, server stays alive); `didChange` before `didOpen` pinned at the wire level (the handler silently starts tracking the never-opened document — documented as current behavior, not endorsed); a percent-encoded round-trip through a project directory containing a space and a non-ASCII character (didOpen target, publishDiagnostics echo, and cross-file goto target URIs all round-trip); an `inlayHint` bounded sub-document range that pins `params.range.end` clipping (the #249 clamp test only pinned the start side); and a position past the last line answering null for hover/definition/completion
  - `non_file_uri_is_ignored_without_crashing` now asserts the *absence* of a publish for the untitled URI, matching the query/fragment sibling test
  - `editing_an_imported_file_republishes_open_dependents` is made deterministic: the dependent's republish is awaited as a protocol barrier (`wait_for_publish`) rather than a fixed 500 ms wall-clock pre-drain, so a straggling clean republish under CI load can no longer be mistaken for the post-edit publish
  - VS Code extension: extract the client lifecycle promise queue into a pure `src/lsp/queue.ts` `SerialQueue` (no `vscode` import, mirroring `resolve.ts`/`timeout.ts`) and pin its invariants — submission-order serialization, atomic stop-then-start restart, and rejection isolation (a failed operation rejects to its own caller yet never wedges the queue); `client.ts` behavior is unchanged (a thin `enqueue` wrapper delegates to the instance)
- Add 7 enum codegen test fixtures with four-tier verification (byte, WAT, validation, wasmtime execution) ([pr#187])
  - `enum_variant`: basic variant access, variable declaration, return values
  - `enum_multi`: multiple enum definitions in one module
  - `enum_params`: enum as function parameter with branching on tags
  - `enum_compare`: equality and inequality comparisons
  - `enum_assign`: mutable reassignment, assignment from parameter
  - `enum_array`: arrays of enum values in linear memory
  - `enum_in_struct`: enum-typed struct fields
- Add 12 enum execution tests with Wasmtime assertions: variant tags, params, comparisons, reassignment, arrays, struct fields, const declarations, uzumaki ([pr#187])
- Add 7 type-checker tests for enum operator constraints: equality/inequality accepted, arithmetic/ordering/negation/boolean-context rejected ([pr#187])
- Rewrite all 85 AST builder tests in `tests/src/ast/helpers.rs` with deep structural verification
  - 50+ test helper functions for constructing and asserting on AST nodes
  - Tests now verify node positions, field values, and parent-child relationships
  - Total test count increased from ~1162 to 1917
- Expand analysis test coverage from 43 to match all 22 rules
  - Tests for all new control-flow rules: A006 (uzumaki placement), A007 (branch-aware missing return), A008 (standalone uzumaki)
  - Tests for migrated lint and codegen-restriction rules (A009–A019, A022–A024)
- Add 43 analysis walker tests covering all 5 rules across free functions, struct methods, and spec functions ([#156])
  - Negative tests for valid code, edge cases for nested loops, deeply nested nondet, overlapping rule triggers
  - All four nondet block types (forall, exists, assume, unique) tested for A002
- Add 5 nested compound codegen test fixtures with four-tier verification (byte, WAT, validation, wasmtime execution) ([pr#185])
  - `nested_struct`: struct-in-struct literal, chained access, write, param, return, copy, method
  - `struct_with_array`: array-in-struct literal, index access through struct, write, param, method
  - `array_of_structs`: struct-in-array literal, field access through index, element write, method
  - `nested_struct_with_array`: combined struct nesting with array fields
  - `multidim_array_uzumaki`: multidimensional array uzumaki in non-det block
- Add `struct_array_field_nondet` test fixture for struct uzumaki with array fields ([pr#185])
- Add 3 analysis test modules for nested compound rules ([pr#185])
  - `rules_a026_a028.rs`: nested depth, uzumaki on nested struct, uzumaki on struct-in-array (703 lines)
  - `rules_a029_a030.rs`: compound literal in compound assign, uzumaki on deep array (364 lines)
  - `rules_a031.rs`: unsupported compound return expression (234 lines)
- Add type checker tests for nested compound literal propagation (`compound_literal_allowed`) ([pr#185])
- Add 9 method codegen test fixtures with four-tier verification (byte, WAT, validation, wasmtime execution) ([pr#178])
  - `method_instance`, `method_assoc`, `method_self_mutate`, `method_return_struct`, `method_cross_call`, `method_multi_struct`, `method_i64_fields`, `method_three_fields`, `method_array_return`
- Add negative codegen tests for unsupported features: `assert`, `**` operator, standalone `TypeMemberAccess`, recursive compound returns ([pr#178])
- Add validation tests for method mangling, immutable self zero-copy, and mutable self frame copy ([pr#178])
- Add 12 type checker tests for method chain rejection, compound-return in assignments, and member-access error cases ([pr#178])
- Update all AST, type-checker, and codegen tests for typed arena API ([#156])
  - Migrate from `arena.filter_nodes(|node| matches!(node, AstNode::...))` to structured traversal via typed IDs
  - Update test utilities with `find_function_by_name()`, `collect_exprs_matching()`, `collect_all_stmts()`
- Add 5 array test fixtures with 4-tier verification (byte, WAT, validator, execution) ([#148])
  - `array_literal.inf`: i32/i64/bool/u8 array literals and empty array
  - `array_index.inf`: literal index, variable index, sum array, multiple element types
  - `array_assign.inf`: element assignment, swap, variable index, multiple types
  - `array_params.inf`: pass-by-value copy semantics, multiple arrays, large array copy
  - `array_nondet.inf`: arrays in forall/exists blocks, element-wise uzumaki
- Add type-checker tests for array type validation ([#148])
  - 6 tests for array size/element type mismatches at function call sites
  - 9 tests for array element assignment mutability checks
  - 6 type equality tests replacing old compatibility tests
  - i64/u64 array literal type inference tests
- Add 7 sret execution tests: literal return, variable return, chained forwarding, value semantics, sub-i32, i64, sret with params ([#148])
- Add 7 type-checker tests for `ArrayReturnCallInExpressionPosition`: let binding, return forwarding, standalone, argument, index access, assignment, non-array standalone ([#148])
- Add 10 inline execution tests for array element types: i8, u8, i16, u16, u32, i64, large array params (N > 16), mixed-type arrays, mutable parameters ([#148])
- Add runtime stack overflow trap test: two 32KB frames in 64KB stack verified to trap at runtime via Wasmtime ([#148])
- Add 6 struct codegen test fixtures with 4-tier verification (byte, WAT, validator, execution) ([pr#159])
  - `struct_literal.inf`: two-field, single-field, and mixed-type struct creation
  - `struct_access.inf`: field reads, arithmetic on fields, mixed-type alignment
  - `struct_assign.inf`: field writes, field swaps, bool field modification
  - `struct_params.inf`: copy-on-entry semantics, multiple struct params, mixed types
  - `struct_return.inf`: sret literal return, variable return, call forwarding
  - `struct_copy.inf`: value semantics, independent copies, mixed-type copy
- Add ~30 type-checker tests for struct validation ([pr#159])
  - Struct mutability: immutable/mutable variable and parameter field assignment
  - Variable shadowing: inner blocks, if/else, loops, const, parameters, sequential blocks
  - Struct field validation: missing, extra, duplicate fields, type mismatches
  - Compound literal position restrictions, sret call restrictions
  - Bool/number type mismatch, const initializer validation, ordering comparison rejection
- Add 13 loop test fixtures with 4-tier verification (byte, WAT, validator, execution) ([#152])
  - `simple_loop.inf`, `infinite_loop_break.inf`, `nested_loop.inf`, `loop_with_if.inf`, `loop_accumulator.inf`, `loop_break_early.inf`, `break_nested_if.inf`, `void_loop.inf`, `loop_zero_iters.inf`, `loop_with_array.inf`, `loop_in_nondet.inf`, `nondet_then_break.inf`, `loop_return_array.inf`
  - Execution tests via Wasmtime for all deterministic fixtures
  - Coverage marks: `wasm_codegen_emit_loop_statement`, `wasm_codegen_emit_loop_conditional`, `wasm_codegen_emit_loop_infinite`, `wasm_codegen_emit_break`
- Add execution test for `numeric_literals` verifying MIN/MAX boundary values for all 8 integer types (i8, i16, i32, i64, u8, u16, u32, u64) via Wasmtime
- Add `arith_overflow` test module with 8 functions covering two's-complement wrapping arithmetic: i32/i64/u32 overflow and underflow, multiplication overflow, and negation of MIN (8 Wasmtime execution assertions)
- Add `expr_deep_nesting` test module with 5 functions verifying 8+ level expression nesting: left-associative addition chain, mixed arithmetic in nested groups, boolean connectives over nested comparisons, function calls embedded in expressions, and chained unary negation (6 Wasmtime execution assertions)
- Add 4 algorithm integration test modules exercising assignments, conditionals, and expressions in realistic patterns:
  - `algo_bitwise`: bit manipulation (popcount, reverse bits, parity, hamming distance, power-of-2 check)
  - `algo_converge`: iterative convergence (integer sqrt, binary search, GCD, collatz steps)
  - `algo_i64_mixed`: i64 arithmetic (sum range, factorial, fibonacci, digit sum, geometric progression)
  - `algo_recursive_math`: recursive functions (factorial, fibonacci, GCD, power, sum-to-n)
- Add 2 assignment test fixtures with 10 Wasmtime execution assertions ([#146])
  - `assign.inf`: 10 functions covering simple i32/i64 assignment, expression RHS, parameter assignment, multiple reassignment, function call RHS, bool assignment, assignment inside conditional, mutable parameter assignment
  - `assign_nondet.inf`: assignment inside `forall` non-det block with uzumaki RHS
  - AST parse tests for `is_mut` flag on `VariableDefinitionStatement`
  - Type-checker tests for mutability enforcement (immutable, mutable, parameter mutability)
- Add WAT golden file testing with `wasmprinter` for human-readable codegen verification ([#144])
  - `assert_wat_equivalence()` compares generated WAT against committed `.wat` reference files
  - `regenerate_wat()` writes WAT alongside WASM during test data regeneration
  - Non-det modules gracefully skipped (custom opcodes unsupported by `wasmprinter`)
- Add 3 conditional test fixtures with 62 Wasmtime execution assertions ([#144])
  - `if_else.inf`: 6 functions covering if-only, if/else, locals in arms, nested if, void if
  - `if_bool_exprs.inf`: 16 functions across 7 groups (bool params, logical ops, De Morgan, range checks, bool locals)
  - `if_nondet.inf`: if-statement inside `forall` non-det block
- Flatten per-module test directory structure to avoid double-nesting ([#144])
  - `get_test_dir()` helper deduplicates module-name paths
- Migrate codegen test data to per-test subdirectory layout ([pr#135])
  - `tests/test_data/codegen/wasm/base/{name}/{name}.{inf,wasm}` replaces flat `base/{name}.{inf,wasm}`
  - `get_test_file_path` / `get_test_wasm_path` helpers updated to resolve through subdirectory
- Add 28 codegen tests with three-tier verification architecture ([issue#97], [#125])
  - Byte comparison tests against committed `.wasm` reference files
  - `inf_wasmparser::validate()` validation on all generated output
  - 2 Wasmtime execution tests verifying runtime behavior
  - Validation tests for metadata, target/mode combinations, non-det opcode presence
- Add codegen test helpers ([issue#97], [#125])
  - `codegen_output()`, `codegen_output_with_mode()`, `codegen_with_target_mode()`, `codegen_with_full_config()`
  - `wasm_codegen()`, `wasm_codegen_with_target()`, `assert_wasms_modules_equivalence()`
- Expand `infs` test coverage from 282 to 429 tests (360 unit + 69 integration) ([#96])
  - Add TUI rendering tests using TestBackend for main_view, doctor_view, toolchain_view
  - Add integration tests for non-deterministic features (forall, exists, assume, unique, oracle)
  - Add tests for error handling, environment variables, and edge cases
  - Consolidate test fixtures in `apps/infs/tests/fixtures/`
- Move QA test suite to `apps/infs/docs/qa-test-suite.md` with 9 truly manual tests ([#96])
- tests: Consolidate builder tests by removing redundant `builder_extended.rs` module ([#50])
- tests: Add `builder_features.rs` module with feature-specific AST tests ([#50])
- tests: Add `primitive_type.rs` module with `SimpleTypeKind` tests ([#50])
- tests: Add utility assertions: `assert_single_binary_op`, `assert_function_signature`, etc. ([#50])

### infs CLI

- Fix `infs doctor` to verify `inference-lsp` where the editor actually resolves it ([#253])
  - The VS Code extension resolves the language server only through `<INFERENCE_HOME>/bin/inference-lsp` (the managed symlink) and PATH; doctor previously checked the toolchain directory instead, so a toolchain that bundles the server but whose `bin/` link is missing or broken printed a misleading `[OK]` while the extension reported "not found". The check now verifies the symlink exists and resolves, WARNing with `infs default <version>` as the repair when it does not.
  - The "also on PATH" note no longer fires for infs's own managed `bin/` symlink (which the extension prepends to PATH before running doctor), so it reports only a genuinely separate copy.
  - The check is driven from `ToolchainPaths::OPTIONAL_MANAGED_BINARIES` rather than a hardcoded name, so a future optional managed binary gains doctor coverage automatically.
- Add opt-in post-build WASM optimization via Binaryen `wasm-opt`
  - After a successful project-mode `infs build`/`infs run`, when the manifest declares `[build.wasm-opt]`, the external `wasm-opt` binary optimizes `out/main.wasm` in place; absent the table, the pipeline is unchanged
  - Runs only for executable artifacts: proof-mode builds and any `-v` build are always skipped silently, since their WASM can carry non-deterministic opcodes (`forall`/`exists`/`assume`/`unique`/`@` uzumaki) that `wasm-opt` cannot parse
  - A compile-mode artifact that still contains a non-deterministic opcode is a hard error naming the construct and pointing at the fix (move it into a `spec` block, or disable optimization), rather than an opaque `wasm-opt` parse failure
  - `infs run` applies the same optimization as `infs build`, so it always executes exactly what a build would ship; single-file mode is unaffected
  - New `--no-wasm-opt` flag on `infs build` and `infs run` skips optimization for a single invocation regardless of the manifest
  - `wasm-opt` is resolved via the `WASM_OPT_PATH` environment variable, falling back to PATH, then an infs-managed Binaryen install (see below); if none resolves, the build fails with install hints led by `infs component add wasm-opt`
  - The resolved binary must be Binaryen 116 or newer; an older version is a hard error, while an unparseable `--version` output only warns and proceeds
  - `wasm-opt` strips the WASM names custom section, so stack traces from an optimized artifact lose function names
- Add infs-managed Binaryen provisioning for `wasm-opt` (`infs component`)
  - New `infs component add|list|remove <name>` command family (rustup-style) manages optional toolchain components, a tier distinct from the `infc` toolchain install; `wasm-opt` (Binaryen) is the only component today
  - `infs component add wasm-opt` downloads a pinned, sha256-verified Binaryen release (`version_130`) into `~/.inference/tools/binaryen/<version>/`; the checksum is verified before anything reaches the install directory, the install is idempotent (no network access when already installed) and atomic (staged under a per-process temp directory, published with a single rename), and a broken prior install is repaired rather than left stale
  - `infs component list` reports each component's install state and location; `infs component remove wasm-opt` deletes the managed install; `add` prints a note when `WASM_OPT_PATH` or a PATH `wasm-opt` would shadow the newly installed managed copy at build time
  - `wasm-opt` resolution gains a third precedence tier — `WASM_OPT_PATH` env → PATH → the managed install — completing a chain that previously hard-errored whenever the first two missed; set `INFS_VERBOSE=1` to trace which tier resolved the binary
  - New `[build.wasm-opt] auto-install` manifest key (default `false`): when `true` and `wasm-opt` resolves in no tier, `infs` downloads the pinned Binaryen at build time instead of erroring
  - The missing-`wasm-opt` install-hint error now leads with `infs component add wasm-opt`, ahead of the brew/apt/npm/releases hints, and mentions `auto-install = true` as the hands-off alternative
  - `infs doctor` gains an appended `wasm-opt` check: OK naming the resolved path, precedence tier, and Binaryen version (noting when a managed copy is shadowed by PATH); an unused `wasm-opt` reports OK as "not installed (optional)" rather than alarming projects that don't use `[build.wasm-opt]`; a broken managed install, a failing `--version` probe, or an invalid `WASM_OPT_PATH` each WARN with remediation
- Make `infs build` and `infs run` project-aware ([#223])
  - Invoked with no path, both commands discover the project's `Inference.toml` by walking up from the current directory (nearest ancestor wins; the start directory is canonicalized once for symlink stability), then compile `<root>/src/main.inf` with the compiler's working directory set to the project root so `out/` always lands at the root regardless of where the command was invoked
  - The existing single-file forms (`infs build path/to/file.inf`, `infs run path/to/file.inf`) are preserved unchanged
  - `infs new` / `infs init` "Next steps" hint updated from `infs build src/main.inf` to `infs build`
  - `src/**/*.inf` files reachable from `main.inf` through `use` imports are compiled into the single output artifact; files reachable from no import chain emit a warning (each named) and are excluded from the build ([#63])
  - Project-mode `infs run` always builds in compile mode and invokes `main`; a non-`main` `--entry-point` is rejected with guidance to use single-file mode (proof-mode WASM embeds non-deterministic opcodes wasmtime cannot execute)
  - Discovery and entry-point failures produce remediation-style errors (suggesting `infs new`, `infs init`, or an explicit file path)
- Add automatic PATH configuration on first install ([#96])
  - Unix: Modifies shell profile (`~/.bashrc`, `~/.zshrc`, `~/.config/fish/config.fish`)
  - Windows: Modifies user PATH in registry (`HKCU\Environment\Path`)
  - Users only need to restart their terminal after installation
- Rename environment variable and directory for consistency ([#96])
  - `INFS_HOME` → `INFERENCE_HOME`
  - `~/.infs` → `~/.inference`
- Add `infc` symlink to installed toolchain ([#96])
- Improve `infs install` to auto-set default toolchain when none is configured ([#96])
  - When installing an already-installed version without a default toolchain, `infs install` now automatically sets that version as default and updates symlinks
  - Provides graceful recovery if default toolchain file was manually removed
- Improve `infs doctor` recommendations for missing default toolchain ([#96])
  - When no default is set but toolchains exist, suggests `infs default <version>` instead of `infs install`
  - When no toolchains exist, suggests `infs install`
- Fix `infs install` and `infs self update` to fall back to latest pre-release version when no stable versions exist ([#96])
  - Previously failed with "No stable version found in manifest" error
  - Now uses latest stable version if available, otherwise falls back to latest version regardless of stability
- Fix `infs install` failing with nested archive structure from GitHub releases ([#96])
  - GitHub releases wrap tar.gz archives in ZIP files
  - Now automatically detects and extracts nested tar.gz after ZIP extraction
- Fix `infs uninstall` leaving broken symlinks when removing non-default toolchains ([#96])
  - Previously, `Path::exists()` returned false for broken symlinks, causing them to remain in `~/.inference/bin/`
  - Now uses `symlink_metadata().is_ok()` to correctly detect and remove both valid and broken symlinks
  - Added `validate_symlinks()` to check for broken symlinks after uninstallation
  - Added `repair_symlinks()` to automatically fix broken symlinks by updating them to the default version or removing them
- Change `infs doctor` to exit with non-zero status when checks fail ([#116])
  - Previously always exited 0; now returns non-zero so callers can detect failures
- Remove manifest caching from `infs` CLI ([#116])
  - `fetch_manifest()` now always fetches from network
  - Simplifies CLI code; VS Code extension manages its own fetching lifecycle
- Remove LLVM toolchain management from `infs` CLI ([#126])
  - Flatten toolchain layout: `infc` binary now at toolchain root (no more `bin/` subdirectory)
  - Remove `inf-llc`, `rust-lld`, and `libLLVM` binary management
  - Simplify doctor checks: single `infc` check replaces `inf-llc`, `rust-lld`, and `libLLVM` checks
  - Remove platform-specific `#[cfg(target_os = "linux")]` branching in `run_all_checks()`
  - Slim `InfsError` to single `ProcessExitCode` variant; all other errors use `anyhow::Result`
  - Replace `rand` dependency with lighter-weight `fastrand`
  - Remove dead code: unused error variants, `create_project_default()`, `available_versions()`, `selected_bg` theme field

### Build

- Add `infs` binaries to release artifacts for all platforms (Linux x64, Windows x64, macOS ARM64)
- Update release manifest to schema version 2 with separate `infc` and `infs` tool entries
- Add macOS Apple Silicon (M1/M2) support to build workflows ([#55])
- Add Codecov integration for test coverage reporting ([#57], [#58])
- Optimize local build time and refactor CI workflows ([#60])
- Add Windows development setup with cross-platform LLVM binaries
- Update libLLVM download URL to use consistent filename with `-nightly` suffix ([#56])
- Remove unused PATH configuration from `.cargo/config.toml` ([#56])
- Bump CI cache keys to invalidate stale binary caches ([#56])
- Fix LLVM environment variable reference in Windows installation guide ([#56])
- Add Linux development setup guide (`book/installation_linux.md`) ([#56])
- Add macOS development setup guide (`book/installation_macos.md`) ([#56])
- Add cross-platform dependency check script (`book/check_deps.sh`) ([#56])

### Tooling

- Remove `playground-server` tool (unused, superseded by external playground infrastructure) ([#56])
- Reorganize project structure: move crates to `core/` and `tools/` directories ([#43])
- Add `inf-wasmparser` crate (fork with non-det instruction support) ([#43])
- Add `inf-wat` crate for WAT parsing ([#43])
- Add `wat-fmt` crate for pretty-formatting WAT files ([pr#21])
- Improve error handling with `anyhow::Result` for AST parsing ([pr#22])

### Performance

- ast: 98% memory reduction in `Location` struct by removing unused source field ([#69])
- compiler: the multi-file project front end parses each reachable file exactly once — the import walk now lowers files directly into the shared arena and reorders them into canonical order afterward via the new `AstArena::canonicalize_source_file_order`; previously discovery parsed every file into a throwaway arena just to read its `use` directives and lowering re-parsed it ([#227])
- lsp: shed per-keystroke work in the single-threaded message loop and bound the analysis cache ([#247])
  - Coalesce a typing burst: a dedicated forwarder thread drains the transport's rendezvous receiver into an unbounded buffer, so a burst can accumulate where the coalescer can see it — `lsp-server`'s stdio/socket channel is zero-capacity (`bounded(0)`), so a backlog otherwise never lands in the channel the loop reads, only in the OS pipe, and an immediate `try_recv` always found it empty. With the buffer in place, when the head of the queue is a `didChange`, the available backlog is drained and consecutive changes to the *same* document collapse to their final text, so the closure pipeline runs a handful of times per burst instead of once per keystroke. A `didOpen`/`didClose` for that document or any request is a barrier the coalescer never reorders across, and no other message is dropped. The e2e suite asserts a 26-change burst over the real stdio binary publishes strictly fewer than 26 times (before the forwarder it published exactly once per change — coalescing never fired)
  - Defer dependent republishes: a notification publishes eagerly only for the changed document; every other open document it invalidated is queued and republished when the loop next goes idle, so an interactive request arriving right behind a keystroke is answered before the other documents recompute. The queue is drained before the loop blocks, a request against a queued document publishes it fresh immediately, and a shutdown flushes it. Each open dependent is thus republished once when the loop goes idle — not once per keystroke — so time-to-first-response for a request behind a keystroke no longer multiplies by the open-dependent count (it still grows with that count, but far more slowly than the eager per-keystroke path)
  - Share line indexes: `FileAnalysis` stores each closure file's `LineIndex` behind an `Arc`, and `Analysis::line_index`/`closure_line_index` return `Arc` handles, so a position query no longer copies the whole document's text (~66 KB / 2 heap allocations per request on a ~59 KB file → 0)
  - Bound the analysis cache: closing a document drops its overlay-derived analysis (recomputed from disk on demand), and analyses memoized for never-opened paths (feature requests on arbitrary URIs) are FIFO-capped at 8; open documents are never evicted

### Changed

- codegen: the WASM code generator's function-body passes now share one statement-descent helper. `pre_scan_locals` (local discovery), `collect_compound_slots` (frame-slot collection), and `body_has_dynamic_array_index` (bounds-check scratch reservation) previously each recursed into `Block`/`If`/`Loop` independently, kept in sync only by convention; a new block-bearing statement kind could be handled by one pass and silently missed by another, corrupting frame layout. Descent is now classified in one place (`nested_blocks`) that both the pure-enumeration walker (`walk_statements`) and the frame-slot pass consult, so the three passes can never disagree about which sub-blocks exist. Purely internal — emitted WASM is byte-identical (the full codegen golden suite passes unmodified) ([#167])
- codegen: the name of the function being compiled is no longer held as mutable ambient state on the compiler (`Compiler::current_fn_name`). It is now threaded explicitly as a `fn_name: &str` parameter from `visit_function_definition` through the statement-lowering walker (`lower_statement`, `lower_block`, `lower_if_statement`, `lower_loop_statement`) to its sole reader, the sret-return invariant panic in `lower_sret_return`. Removing the implicitly-shared field forecloses a class of stale-read hazards that would surface once method, incremental, or parallel function compilation is added. Purely internal — emitted WASM is byte-identical (the full codegen golden suite passes unmodified) ([#172])

### Fixed

- Constructing an array-of-struct value inside a struct field now lowers correctly. A struct literal whose field is an array of structs (e.g. `Grid { cells: [Point { … }, Point { … }] }`) previously panicked in codegen during element-wise store; it now stores each struct element through the same per-element machinery used for top-level array-of-struct locals. The read, write, parameter, and sret-return paths were already correct ([#224])
- Constructing a multi-dimensional array value inside a struct field now lowers correctly. A struct literal whose field is a nested array (e.g. `Grid { grid: [[1, 2, 3], [4, 5, 6]] }` for a `[[i32; 3]; 2]` field, including arrays-of-structs such as `[[Point; 2]; 2]`) previously panicked in codegen because the element-wise store loop could not handle array elements; it now delegates to the recursive leaf-store machinery shared with top-level multi-dimensional array locals. The read and write paths (e.g. `g.grid[i][j]`) were already correct ([#224])
- Fix FxHashMap non-deterministic iteration in `Arena` — `filter_nodes()` and `list_nodes_cmp()` now sort by node ID, ensuring reproducible WASM function emission order
- Fix Drop instruction emission for nested non-det blocks — `parent_blocks_stack.last()` (innermost block) is now used instead of `.first()` (outermost block)
- Fix `lower_literal` to emit type-correct WASM const instructions — number literals now consult `TypedContext` and emit `i32.const` or `i64.const` based on inferred type instead of always emitting `i32.const`
- Fix `wasm_to_v` public API signature — parameter changed from `&Vec<u8>` to idiomatic `&[u8]`
- ide: the resilient project walk (`inference::load_project_resilient`) no longer runs the unreachable-file warning scan at all — `ResilientProjectParse::warnings` is documented always-empty. The scan recursively walked and canonicalized every `.inf` under the source root on every keystroke (and, for a document at a volume root like `/main.inf`, the entire disk) to compute warnings the IDE discards. The fail-fast compiler path (`parse_project`) keeps the scan — it runs once per build, not once per keystroke — so compiler behavior is unchanged ([#33])
- Extern-import diagnostics (`use { … } from <module>;` binding errors such as an undeclared extern import or an ambiguous extern module) reported from an *imported* file now carry that file's module-path label instead of rendering as if they were in the entry file. Locations are per-file-local, so the missing label made these errors point at wrong positions in the entry file — visible in both the aggregated compiler message and the structured diagnostics the LSP consumes ([#33])
- tests: `core/inference` project tests no longer collide on their temp directory under parallel load. The `TempProject` test helper named directories `inference-project-<tag>-<pid>-<nanos>`; two tests sharing a tag in one process (e.g. the two `self-import` tests) could land in the same directory when the coarse system clock returned equal nanoseconds, making each see the other's `main.inf` and spuriously fail the "no duplicate `main` module" assertion. The suffix now appends a process-wide `AtomicU64` sequence counter, so same-tag directories are always distinct regardless of clock resolution ([#270])
- lsp: `file:` URI-to-path mapping now normalizes so one on-disk file interns under one spelling, closing a set of file-identity edge cases that keyed separate documents or reached outside the local disk ([#248])
  - Dot segments (`.` / `..`) are removed lexically after percent-decoding, so `file:///a/../b.inf` and `file:///b.inf` name one document instead of two (stale/duplicate analyses). Normalization is purely textual — a `..` crossing a symlink is resolved by name, not by following the link
  - Path-form UNC paths (empty authority with a `//` path, e.g. `file:////server/share/x.inf`) are rejected like a remote authority instead of decoding to `//server/…`, which is SMB network I/O on Windows
  - The scheme is matched case-insensitively per RFC 3986 (`File://`, `FILE://`), and the RFC 8089 single-slash form (`file:/path`) is accepted and normalized to the same path as the authority form
  - Bare and drive-relative drive URIs (`file:///C:` → `C:`, `file:///c:name` → `C:name`) are rejected on Windows — a drive prefix must be followed by `/` to name an absolute path; the drive-root `file:///C:/` and normal drive paths are unaffected, and on POSIX `/C:` remains a valid directory name
- ide: on a case-insensitive filesystem (macOS/Windows), a mis-cased import path (`use lib::Math;` reaching the on-disk `lib/math.inf`) no longer bypasses the open-buffer overlay. The overlay-then-disk loader now retries the overlay under the file's on-disk canonical spelling on a miss before reading disk, so live edits to an open buffer are honored instead of stale disk text that no `didChange` ever invalidated. The extra `canonicalize` stays off the hot path — it runs only on an overlay miss ([#248])
- compiler, ide: a leading UTF-8 BOM (U+FEFF) is now stripped when reading a source file from disk, in the single ingestion seam shared by the compiler's `DiskLoader` and the IDE's overlay-then-disk loader. Previously an unopened closure file carrying a BOM was analyzed one UTF-16 unit off on line 0 and produced a spurious lexer error at the file start (clients strip the BOM from opened buffers, so the two views disagreed). This changes compiler behavior for BOM-prefixed files: they now parse and compile instead of failing at the lexer ([#248])
- lsp: `LineIndex::new` and `Vfs` path interning now fail explicitly instead of silently wrapping at their `u32` width limits — a source text of 4 GiB or more (which would truncate line-start offsets and break position lookup) and interning more than `2^32` paths (which would alias two documents onto one `FileId`) now panic with a clear message. Neither bound is reachable in a real editing session ([#248])
- ide/lsp: completions no longer offer names that fail to compile when accepted ([#246])
  - A plain `use lib;` binds only the namespace, so its items are offered qualified (`lib::exported`, the label the LSP inserts verbatim) plus the bare namespace name — never bare `exported`, which the checker rejects as an undefined function. A braced `use lib::arith::{add};` binds only the braced names, so exactly those are offered bare (an item that names no public def in the target is dropped), not every public def of `arith`
  - New `<module>::` completion context: after a plain-import namespace qualifier, that module's public defs are offered by their bare name — the position where a bare member name is what compiles. An item import binds no namespace, so its module is not offered as a `::` qualifier, and a `::` position never falls back to the keyword/local list
  - Member completions after `.` on a struct defined in another module now drop private methods (the checker rejects `receiver.private_method()` across modules); a same-file receiver keeps its private methods, which are callable there
  - Completions are suppressed inside comments and string literals, decided by the lexer's token spans so quote boundaries are exact, rather than popping the general list into prose an editor auto-triggered on
- ide/lsp: goto-definition and hover cover five hit-testing gaps, so positions that previously returned nothing now resolve ([#244])
  - A caret at an identifier's exclusive end — where a double-click or a just-finished keystroke leaves it — now resolves the identifier. `hit_test` covers `start <= offset < end`, so the end position lands on the enclosing call or statement; goto, hover, and the completion locals now share one identifier-biased one-byte-back fallback (`inference_ide_db::enclosing_hit`) that still refuses to pull a caret past a `}` back into the closing definition
  - `use` directives are hit-testable: goto/hover on any path segment resolves to the module file it names (`lib`, then `lib::geom`), and on a braced item import resolves to that item's public definition in the target module. A `from`-clause external module reference names no source file, so it does not resolve
  - A declared function type parameter (`T'`) resolves to itself under goto/hover instead of falling to the whole function definition
  - An enum variant *declaration* name resolves to itself, like every other declaration name (goto/hover previously covered only function arguments and struct fields)
  - A function-local `const` reference now resolves to its declaration, respecting lexical scope: it is visible only after its declaration point and only within its own function/block, matching the type checker's statement-order registration (a const used before its declaration, or referenced from another function, does not resolve)
- ide/lsp: goto-definition and hover now agree with what the type checker resolved instead of contradicting it via a syntactic name-scan ([#245])
  - A free-function call over a same-named struct method resolves to the free function (goto) and shows its signature (hover). The checker records `receiver_struct=None` for a free call, so the by-name search now skips struct methods rather than landing on the method that precedes the free function in the pre-order flatten
  - A bare imported value resolves only through a braced import that names it (`use m::{MAX}`): a plain `use m;` binds only the namespace, so a bare `MAX` under it is a type error and no longer "resolves" to the first module that happens to export the name. When two imported modules both export a name, goto lands in the one the braced import actually selects
  - A constant imported through a `pub use` re-export chain (`use mid::{MAX}` where `mid` has `pub use lib::{MAX}`) now resolves to its defining file, the way calls already followed re-exports; the walk guards against re-export cycles
  - Hovering the leaf of a `::`-qualified type (`lib::T`) resolves through the qualifier into the defining file, matching goto, instead of showing a local same-named type's signature or degrading to the bare name
  - A function type renders as a source-like `fn(…) -> i32` spelling in hovers and inlays rather than the checker-internal `Function<2, i32>` carrier (parameter count plus return); the checker now builds the source-like carrier when constructing the type's `TypeInfo`. Written `fn(…)` parameter types are dropped by the parser (a pinned AST-parity quirk), so only the return type survives to the spelling
  - Goto on a local-binding use now reports the whole `let`/`const` statement as its `full_range` (with the name as `focus_range`), matching what landing on the declaration itself reports, instead of a `full_range` equal to just the ident
- VS Code extension: switching or updating the toolchain now restarts a running language server ("Select Toolchain Version", "Update Toolchain", and "Install Toolchain" all restart it on success), so diagnostics/hover/goto immediately reflect the new default toolchain. Previously these commands only ensured the server was started — a no-op while one was running — leaving the old toolchain's `inference-lsp` process serving stale results until a manual "Restart Language Server" or window reload. Restart is a strict superset of the old behavior: the stop phase no-ops when the server is not running ([#250])
- VS Code extension: language-client lifecycle robustness. A configuration change now decides start/stop/restart *inside* the serialized lifecycle queue, re-reading `inference.lsp.enabled` and the running state when the queued operation actually runs — previously the decision sampled state at event time, so disabling the LSP while a start was still in flight skipped the stop and left the server running against `enabled: false`; the last setting now always wins regardless of interleaving. A spawned server that never answers the `initialize` request no longer wedges the lifecycle queue forever: `start()` is bounded by a 30-second timeout (`withTimeout` helper in `src/utils/timeout.ts`), the hung process is disposed, and the failure is logged plus surfaced as a warning notification with a "Show Output" action ([#251])
- VS Code extension: the standard `inference-lsp.trace.server` protocol-trace setting (`off`/`messages`/`verbose`, window scope) is now contributed in `package.json`, so the vscode-languageclient trace knob is discoverable in the Settings UI and no longer flags as an unknown setting ([#251])
- VS Code extension: the getting-started walkthrough's "Create a Project" step now instructs saving the new file with the `.inf` extension — language-server features are file-scheme-only by design (the server's URI layer ignores untitled buffers), so the previous wording promised features an unsaved buffer cannot get ([#251])
- VS Code extension: on Windows the managed-location tier of binary resolution now probes `%APPDATA%\inference`, where `infs` actually installs the toolchain; previously the extension defaulted to `~/.inference` on every platform, so on default Windows setups the managed tier never matched and both `infs` detection and `inference-lsp` resolution only succeeded via PATH (failing entirely when the editor lacked the updated PATH). The shared `inferenceHome()` helper now mirrors `ToolchainPaths::new()` in `apps/infs/src/toolchain/paths.rs` — `INFERENCE_HOME` override first, `%APPDATA%\inference` on Windows, `~/.inference` elsewhere including macOS — and remains the single derivation used by LSP resolution, toolchain detection, install destination, doctor, and the terminal PATH prepend ([#252])
- lsp: an unwinding panic in the analysis stack (a `todo!`/`unwrap` in the type-checker or analysis passes, e.g. a named constant used as an array size) no longer kills the whole server session. The message loop now wraps each request and notification in a panic boundary (`std::panic::catch_unwind`): a panicking request is answered with a JSON-RPC `InternalError` carrying its original id, and a panicking notification publishes nothing and rebuilds the analysis host from the tracked open documents so later queries start from consistent state. Every other open document keeps working, and one bad file can no longer crash-loop the server into a permanent outage. Genuinely unrecoverable failures (stack overflow) still abort as before, and the panic message still goes only to stderr, never the stdout protocol channel ([#241])
- lsp: LSP 3.17 protocol-conformance polish across the initialize/shutdown lifecycle and a few request handlers ([#249])
  - A repeated `shutdown` (and any request received after `shutdown`) is now answered `InvalidRequest` instead of a second `null` success: the `shutting_down` guard arm precedes the `shutdown` arm in the message loop
  - `InitializeParams` is validated *during* the handshake (via `initialize_start`/`initialize_finish`) instead of after `Connection::initialize` completes it, so a wrongly-typed field (e.g. a fractional `processId`) fails the initialize *request* with an `InvalidParams` error rather than aborting the process post-handshake
  - The initialize result now carries `serverInfo` (the crate name and version, from `env!` metadata), which clients surface in logs and crash reports; `lsp-server` 0.8's `Connection::initialize` hard-codes a body without it
  - A mid-session `initialize` request is answered `InvalidRequest` ("the server is already initialized") instead of the misleading `MethodNotFound` "unsupported request: initialize"
  - Hover honors `textDocument.hover.contentFormat`: a client that does not list `markdown` now receives `PlainText` hover content (code fences dropped and inline-code backticks removed; a `*` inside a code example is preserved) instead of Markdown rendered literally
  - An inlay-hint request range whose end is past EOF now clamps to the file end (new `LineIndex::offset_clamped`, extending the existing character clamp to the line dimension) instead of disabling the clip entirely and returning hints outside the requested window
  - `didClose` for a URI this server cannot map to a file publishes nothing — no empty diagnostics set under the garbage URI and no dependents republish — mirroring `didOpen`, which already ignores such URIs
  - An oversized `Content-Length` (unbounded pre-allocation in `lsp-server` 0.8's `read_msg_text`) is documented in the crate's known-limitations note alongside the existing malformed-frame limitation; it is upstream framing owned by the reader thread with no clean stdio seam to bound without vendoring the transport
- type-checker: a named constant used as an array size (`let a: [i32; N] = …`) is now reported as a diagnostic instead of aborting the compiler and the IDE analysis with a `todo!` panic ([#240])
  - `extract_array_size_from_arena` is total again: a non-literal or out-of-range size collapses to a `0` sentinel rather than panicking, so building a `TypeInfo` never unwinds
  - `validate_array_size` raises the diagnostic — a named constant is `NonLiteralArraySize` ("array size must be an integer literal; named constant `N` is not yet supported…", located at the size identifier), a zero or out-of-range literal stays `InvalidArraySize`
  - Both the fail-fast (`build_typed_context`) and lossless (`check_with_diagnostics`) entry points surface it as an ordinary diagnostic; the size-`0` sentinel no longer cascades a spurious array-literal-size or variable/return type mismatch, so the reproduction reports exactly one error
  - The [#241] message-loop panic-boundary tests, which had used this exact panic as their trigger, now inject a deliberate panic through a debug-only server seam (`INFERENCE_LSP_TEST_PANIC_PATH_SUBSTR`, invisible in release builds) instead
  - Compile-time constant evaluation of array sizes remains future work (#79)

### Project Manifest

- Add optional `[build.wasm-opt]` sub-table to `Inference.toml`
  - `enabled` (bool, default `true`): table presence alone enables optimization; set `enabled = false` to keep the table while disabling the step
  - `level` (string, default `"3"`): forwarded to `wasm-opt` as `-O<level>`; one of `"0"`–`"4"`, `"s"`, `"z"`, validated on load with a clear error naming the offending value
  - `auto-install` (bool, default `false`): downloads a missing `wasm-opt` automatically at build time — the same pinned, checksum-verified Binaryen `infs component add wasm-opt` installs — instead of hard-erroring; recorded in the versioned manifest since `infs` has no interactive prompts
  - `infs new`/`infs init` scaffold a commented-out `[build.wasm-opt]` block after `[build]`, including an `# auto-install = true` line
- Consume `[build]` and `[verification]` configuration in project-mode builds ([#223])
  - New `[build] mode = "compile" | "proof"` field (default `"compile"`), validated on load; an invalid value is a clear error naming the field and allowed values
  - `[verification] output-dir` is honored only in effective-proof builds, where it redirects artifacts via `infc --out-dir`; in compile mode it is ignored so the default `proofs/` never relocates `out/main.wasm`
  - `output-dir` is validated relative-only: absolute paths, `..` traversal (even self-cancelling like `a/../b`), and Windows drive/UNC prefixes are rejected so artifacts cannot escape the project root
  - CLI flags override the manifest; `infs` forwards `--mode`/`-v` verbatim and never re-derives the `-v` ⇄ proof implication (that remains owned by `infc`)
  - `infs new`/`infs init` scaffold an explicit `[build] mode = "compile"` and ignore generated `proofs/*.wasm` and `proofs/*.v`
  - A non-default `output-dir` requires an `infc` advertising ABI ≥ 1.1; pairing one with an older compiler hard-errors with remediation rather than failing opaquely in the subprocess
- Replace `manifest_version` field with `infc_version` in Inference.toml ([#96])
  - `infc_version` is a String (semver format) that records the compiler version used to create the project
  - Automatically detected from `infc --version` when running `infs new` or `infs init`
  - Falls back to `infs` version if `infc` is not available
  - All Inference ecosystem crates share the same version number

### Editor Support

- Add VS Code extension with syntax highlighting for Inference language ([#94])
- Add TextMate grammar with hierarchical scopes for non-deterministic keywords (`forall`, `exists`, `assume`, `unique`, `@`)
- Add language configuration with bracket matching, comment toggling, and code folding
- Publish extension to VS Code Marketplace: [inference-lang.inference](https://marketplace.visualstudio.com/items?itemName=inference-lang.inference)
- Add Configuration sidebar (TreeView) to VS Code extension with toolchain info and settings overview ([#116])
  - Activity bar icon opens a Configuration view with Toolchain and Settings groups
  - Displays resolved infs path, version, INFERENCE_HOME, platform, and health status
  - Click settings items to open VS Code settings; click status to run doctor
  - Right-click path items for "Copy Value" and "Reveal in File Explorer"
  - Auto-refreshes on settings change, after install, and after doctor
- Add automatic terminal PATH integration to VS Code extension ([#116])
  - `infs` and `infc` are available in integrated terminals immediately after install or update
  - Existing open terminals show a relaunch indicator when PATH changes
  - PATH modification persists across VS Code sessions
- Add toolchain management commands to VS Code extension ([#116])
  - Install Toolchain: downloads, verifies (SHA-256), extracts, and runs `infs install`
  - Update Toolchain: checks for newer versions and applies updates
  - Select Version: switch between installed toolchain versions via QuickPick
  - Run Doctor: executes `infs doctor` and displays results in output channel
- Add Getting Started walkthrough to VS Code extension ([#116])
  - Four-step guided setup: install toolchain, verify with doctor, create project, build
- Add status bar integration showing toolchain health at a glance ([#116])
- Update VS Code extension tests and QA docs after LLVM removal ([#127])
  - Remove `inf-llc`, `rust-lld`, `libLLVM` references from e2e tests and doctor tests
  - Update fake `infs` shell script to use flat toolchain layout (`TOOLCHAIN_DIR/infc`, no `bin/` subdirectory)
  - Simplify `buildFakeInfcArchive()` to emit only `infc` binary
  - Update doctor check expectations from 6 to 5 checks (single `infc` check replaces `inf-llc`, `rust-lld`, `libLLVM`)
  - Change "missing lib directory triggers doctor warning" to "missing infc triggers doctor failure"
- Add "Install Component (wasm-opt)" command to VS Code extension (`inference.installComponent`)
  - Runs `infs component add <name>` with a progress notification; refreshes `infs doctor` on success; offers Show Output / Retry actions on failure
  - `infs doctor` notifications (error and warning toasts alike) gain an "Install wasm-opt" action button whenever a `wasm-opt` check reports a warning or failure, invoking the install command directly

### IDE / LSP

- Add `inference-lsp`, a Language Server Protocol server for Inference (`apps/lsp`) ([#33])
  - A synchronous, single-threaded `lsp-server` 0.8 stdio binary; single-threaded by design because `TypedContext` is `!Send`
  - Diagnostics: merged syntax, import, type-check, and analysis-rule findings (rule codes `A001`–`A041`), published on `didOpen`/`didChange`/`didClose`
  - Hover: type of the identifier/expression under the cursor, plus dedicated explanations of the non-deterministic keywords (`forall`, `exists`, `unique`, `assume`) and the uzumaki `@` operator, including their Rocq lowering (`BI_forall`/`BI_exists`/`BI_assume`/`BI_uzumaki_num`)
  - Goto-definition, including cross-file resolution into an imported module
  - Document symbols (hierarchical or flattened, negotiated from client capabilities), completions (context-aware: struct members only after `.`), and inlay hints annotating every non-det block and `@` binding
  - Full-text document sync (`TextDocumentSyncKind::Full`); UTF-16 position encoding only (the LSP default; no `positionEncoding` negotiation)
  - `file://` URI handling with percent-encoding and Windows drive-letter support
  - End-to-end test suite (`apps/lsp/tests/e2e.rs`) spawning the real binary over stdio and asserting on raw JSON-RPC across 27 test functions, grouped into twenty-one scenarios (handshake, diagnostics lifecycle, hover, goto, cross-file import, document symbols, completion, inlay hints, UTF-16 positions, robustness, shutdown/exit, stdout framing hygiene)
- Add the `ide/` crate stack backing the LSP server ([#33])
  - `ide/vfs`: `FileId` path interning plus an open-document content overlay; no file I/O, no path canonicalization
  - `ide/base-db`: `LineIndex` (byte offset ⇄ 0-based line / UTF-16 column) and the `TextRange`/`LineCol`/`FilePosition`/`FileRange` position PODs
  - `ide/ide-db`: `RootDatabase` with closure-aware analysis invalidation, analyzing every open file as its own project entry; `FileAnalysis` merges parse errors, structured type diagnostics, and analysis findings behind an overlay-then-disk `FileLoader` driving `core/inference`'s shared import-closure walk
  - `ide/ide`: the `AnalysisHost`/`Analysis` feature API — diagnostics, hover, goto-definition, document symbols, completions, and inlay hints, all returned as editor-terminology PODs with no compiler type crossing the boundary
- Fix a permanently stale IDE analysis when an imported file exists but cannot be read ([#242])
  - A reachable `use` target that exists on disk yet fails `read_to_string` (invalid UTF-8, a lock, a permission error) left no trace in the importing file's `FileAnalysis`: it was neither a loaded closure file nor a missing import, so no later `didOpen`/`didChange` of that file could ever evict the importing entry's symbol-less analysis
  - The resilient walk now surfaces read-failed paths (new `ResilientProjectParse::read_failures`), and `FileAnalysis` folds them into its invalidation closure, so making the file readable re-analyzes every open entry that imports it — the non-entry twin of the existing unreadable-entry recovery
  - The fail-fast compiler path (`parse_project`) is unchanged: it still aborts on the first read error
- Fix false missing-import diagnostics when a non-entry file of a multi-directory project is opened standalone ([#243])
  - Path-form imports resolve relative to a project's single source root, but `RootDatabase` analyzed each open file against its own directory, so opening `src/lib/a.inf` resolved its `use lib::b;` to the nonexistent `src/lib/lib/b.inf` — a false "file not found" squiggle (plus missing symbols) on a file the compiler accepts
  - Each open file's analysis source root is now resolved in three tiers: the nearest ancestor `Inference.toml` manifest's source root (`<manifest_dir>/src`, matching how `infs` compiles `src/main.inf`); failing that, the source root of an already-analyzed entry whose import closure contains the file; failing that, the file's own directory (the previous behavior)
  - New `inference::manifest_source_root` (module `inference::manifest`) performs the manifest walk-up, and `inference::load_project_resilient_with_root` resolves a closure against an explicit source root; invalidation is unchanged — a `didChange` in another directory of the same project still evicts and recomputes correctly under the new root
  - v1 limitation: there is no filesystem watch, so a manifest created or edited after a file was opened is not observed until that file's analysis is recomputed for another reason
- Add structured type-check diagnostics: `inference_type_checker::check_with_diagnostics` (re-exported as `inference::type_check_with_diagnostics`) ([#33])
  - Returns a `TypeCheckOutcome { typed_context, errors: Vec<TypeCheckDiagnostic> }` instead of aggregating errors into one `anyhow::Error` string
  - Lossless: the returned `TypedContext` is fully indexed (symbol table assigned, canonical-key indexes built) even when errors are present, so tooling can still query `lookup_struct`/`lookup_enum`/`call_target`/`get_node_typeinfo` for the parts of the program that did check
  - `TypeCheckerBuilder::build_typed_context` is re-expressed on top of this function, so the compiler and the IDE share exactly one checking implementation
- Add a `FileLoader` seam to `core/inference` (`exists`/`read`) so the import-closure walk can be driven by either a `DiskLoader` (the compiler) or an IDE-supplied overlay-then-disk loader, plus a resilient walk variant, `load_project_resilient`, that collects every problem (broken imports, per-file syntax errors) instead of failing fast on the first one ([#33])
  - `parse_project` is re-expressed on top of the same closure-walk logic and remains byte-identical for a clean project
- Ship `inference-lsp` with the managed toolchain ([#33])
  - Release packaging bundles the `inference-lsp` binary inside the existing `infc-<platform>` archives (no new archive names, no manifest-format change), so `infs install` places it in `toolchains/<version>/` automatically
  - `infs` symlinks `inference-lsp` into `$INFERENCE_HOME/bin` next to `infc` when the default toolchain contains it, cleans the stale symlink when switching to a toolchain that predates bundling, marks it executable on Unix, and includes it in PATH-shadowing conflict detection
  - `infs doctor` gains an appended `inference-lsp` check: `[OK]` with the resolved path when the default toolchain bundles it, `[WARN]` with an upgrade hint when the toolchain predates bundling — the server's absence is never a `[FAIL]` on its own, though the check still reports `[FAIL]` if platform detection, toolchain-path resolution, or the default-version read fails
- VS Code extension 0.0.5: built-in LSP client — installing the extension now brings up the language server out of the box ([#33])
  - Starts `inference-lsp` over stdio on activation (new `onLanguage:inference` activation event), resolving the binary via `inference.lsp.path` setting → `$INFERENCE_HOME/bin` → PATH, mirroring the `infs` detection order; silent (log-only) when the binary is absent
  - Auto-starts the server after a toolchain install/update completes, so the first-run flow (install extension → accept toolchain install) needs no reload
  - New settings `inference.lsp.enabled` / `inference.lsp.path` and command `Inference: Restart Language Server`; server traces go to a dedicated `Inference Language Server` output channel

---

## [0.0.1-alpha] - 2026-01-03

Initial tagged release.

### Language

- Support for non-deterministic blocks: `uzumaki`, `forall`, `exists`, `assume`, `unique`
- Function definitions with generic type parameters
- Module system with visibility modifiers
- Add `undef` syntax support ([#10])

### Compiler

- Tree-sitter-based parsing with error recovery
- Arena-based AST node storage
- Basic type inference

### Rocq Translation

- Add complete WASM module translation to Rocq (Coq) ([#11])
- Implement instruction translation: memory ops, control flow, numeric ops ([#11])
- Add element segment and data segment translation ([#11])
- Add function, table, global, and memory translation ([#11])

### CLI

- Add `infc` CLI binary with parsing diagnostics ([#12])
- Add V file output formatting ([#12])

### Build

- Add CI build workflow with cross-platform support ([#1])

---

[Unreleased]: https://github.com/Inferara/inference/compare/v0.0.1-alpha...HEAD
[0.0.1-alpha]: https://github.com/Inferara/inference/releases/tag/v0.0.1-alpha

[#1]: https://github.com/Inferara/inference/pull/1
[#10]: https://github.com/Inferara/inference/pull/10
[#11]: https://github.com/Inferara/inference/pull/11
[#12]: https://github.com/Inferara/inference/pull/12
[#14]: https://github.com/Inferara/inference/pull/14
[pr#21]: https://github.com/Inferara/inference/pull/21
[pr#22]: https://github.com/Inferara/inference/pull/22
[#23]: https://github.com/Inferara/inference/pull/23
[#24]: https://github.com/Inferara/inference/pull/24
[#25]: https://github.com/Inferara/inference/pull/25
[#28]: https://github.com/Inferara/inference/pull/28
[#29]: https://github.com/Inferara/inference/pull/29
[#43]: https://github.com/Inferara/inference/pull/43
[#44]: https://github.com/Inferara/inference/pull/44
[#50]: https://github.com/Inferara/inference/pull/50
[#54]: https://github.com/Inferara/inference/pull/54
[#55]: https://github.com/Inferara/inference/pull/55
[#56]: https://github.com/Inferara/inference/pull/56
[#57]: https://github.com/Inferara/inference/pull/57
[#58]: https://github.com/Inferara/inference/pull/58
[#60]: https://github.com/Inferara/inference/pull/60
[#69]: https://github.com/Inferara/inference/pull/69
[#86]: https://github.com/Inferara/inference/pull/86
[#94]: https://github.com/Inferara/inference/pull/94
[#96]: https://github.com/Inferara/inference/pull/96
[issue#97]: https://github.com/Inferara/inference/issues/97
[#116]: https://github.com/Inferara/inference/pull/116
[#125]: https://github.com/Inferara/inference/pull/125
[#126]: https://github.com/Inferara/inference/pull/126
[#127]: https://github.com/Inferara/inference/pull/127
[pr#135]: https://github.com/Inferara/inference/pull/135
[#136]: https://github.com/Inferara/inference/pull/136
[#138]: https://github.com/Inferara/inference/pull/138
[#140]: https://github.com/Inferara/inference/pull/140
[#142]: https://github.com/Inferara/inference/pull/142
[#144]: https://github.com/Inferara/inference/pull/144
[#146]: https://github.com/Inferara/inference/pull/146
[#148]: https://github.com/Inferara/inference/pull/148
[#152]: https://github.com/Inferara/inference/pull/152
[pr#159]: https://github.com/Inferara/inference/pull/159
[#156]: https://github.com/Inferara/inference/pull/156
[pr#185]: https://github.com/Inferara/inference/pull/185
[pr#178]: https://github.com/Inferara/inference/pull/178
[pr#187]: https://github.com/Inferara/inference/pull/187
[#188]: https://github.com/Inferara/inference/pull/188
[#195]: https://github.com/Inferara/inference/pull/195
[issue#16]: https://github.com/Inferara/inference/issues/16
[issue#17]: https://github.com/Inferara/inference/issues/17
[issue#18]: https://github.com/Inferara/inference/issues/18
[issue#19]: https://github.com/Inferara/inference/issues/19
[issue#20]: https://github.com/Inferara/inference/issues/20
[issue#21]: https://github.com/Inferara/inference/issues/21
[issue#22]: https://github.com/Inferara/inference/issues/22
[#81]: https://github.com/Inferara/inference/issues/81
[#82]: https://github.com/Inferara/inference/issues/82
[#111]: https://github.com/Inferara/inference/pull/111
[#117]: https://github.com/Inferara/inference/pull/117
[#205]: https://github.com/Inferara/inference/issues/205
[#166]: https://github.com/Inferara/inference/issues/166
[#164]: https://github.com/Inferara/inference/issues/164
[#212]: https://github.com/Inferara/inference/issues/212
[#63]: https://github.com/Inferara/inference/issues/63
[#223]: https://github.com/Inferara/inference/pull/223
[#224]: https://github.com/Inferara/inference/issues/224
[#225]: https://github.com/Inferara/inference/issues/225
[#227]: https://github.com/Inferara/inference/issues/227
[#217]: https://github.com/Inferara/inference/issues/217
[#33]: https://github.com/Inferara/inference/issues/33
[#230]: https://github.com/Inferara/inference/pull/230
[#231]: https://github.com/Inferara/inference/issues/231
[#284]: https://github.com/Inferara/inference/issues/284
[#167]: https://github.com/Inferara/inference/issues/167
[#172]: https://github.com/Inferara/inference/issues/172
[#270]: https://github.com/Inferara/inference/issues/270
[#248]: https://github.com/Inferara/inference/issues/248
[#246]: https://github.com/Inferara/inference/issues/246
[#244]: https://github.com/Inferara/inference/issues/244
[#245]: https://github.com/Inferara/inference/issues/245
[#242]: https://github.com/Inferara/inference/issues/242
[#243]: https://github.com/Inferara/inference/issues/243
[#239]: https://github.com/Inferara/inference/pull/239
[#255]: https://github.com/Inferara/inference/issues/255
[#246]: https://github.com/Inferara/inference/issues/246
[#242]: https://github.com/Inferara/inference/issues/242
[#250]: https://github.com/Inferara/inference/issues/250
[#251]: https://github.com/Inferara/inference/issues/251
[#252]: https://github.com/Inferara/inference/issues/252
[#241]: https://github.com/Inferara/inference/issues/241
[#240]: https://github.com/Inferara/inference/issues/240
[#249]: https://github.com/Inferara/inference/issues/249
[#247]: https://github.com/Inferara/inference/issues/247
[#157]: https://github.com/Inferara/inference/issues/157
[#256]: https://github.com/Inferara/inference/issues/256
[#254]: https://github.com/Inferara/inference/issues/254
