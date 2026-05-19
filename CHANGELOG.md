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
  `.spec_func_indices()` with `.spec_func_indices_by_spec()` ([#21])
- `inference::wasm_to_v` / `inference_wasm_to_v_translator::wasm_parser::translate_bytes`:
  third parameter changed from `spec_func_indices: &[u32]` to
  `spec_funcs_by_spec: &FxHashMap<String, Vec<u32>>`. Callers must pass an
  `FxHashMap` (use `FxHashMap::default()` for the empty case). Same `_by_spec`
  rename rationale: symmetric with the `CodegenOutput` getter shape and avoids
  an extra transformation at the API boundary ([#21])
- Rocq output: `ValidModule` arity changed from 2 → 1 (no longer takes a specs
  list); the new `ValidSpec : module -> list N -> Prop` predicate carries the
  per-spec proof obligation. Downstream Rocq libraries must define `ValidSpec`
  and update existing `ValidModule` consumers. Theorem names also changed:
  `valid_<mod>` is now 1-arg, and per-spec theorems take the form
  `valid_<mod>__<SpecName>` (double underscore, with explicit collision
  rationale documented in `core/wasm-to-v/ROCQ_CONTRACT.md`) ([#17], [#21])
- Lower `assert(<bool>)` to a WASM trap-on-false (previously panicked codegen) ([#195])
  - Emits `<cond>; i32.eqz; if (empty); unreachable; end` — the smallest correct shape, and one that `wasm-to-v` already maps to `BI_unreachable` for proof-mode translation
  - Asserts are emitted in both `Compile` and `Proof` modes (Stmt-level, not Def-level); no `CompilationMode` branching
  - Soroban target accepts asserts — `Unreachable` is baseline WASM, not a 0xfc non-det opcode
  - New golden fixture `tests/test_data/codegen/wasm/base/assert/` exercises literal, variable, nested-in-if, loop+break, double-assert, bool param, unary `!`, `&&`, `||`, `==`, compound `(a > 0) && ((b < 10) || (c == 0))`, and bool-local scenarios, with wasmtime execution coverage that distinguishes pass paths from `Trap::UnreachableCodeReached` paths
- WASM custom section name for the per-spec function index map is now `inference.spec_funcs` (vendor-prefixed namespace). External tools previously looking for `metadata.code.inference.spec_funcs` must update. The latter was a misuse of the WebAssembly tool-conventions reserved namespace ([CodeMetadata.md](https://github.com/WebAssembly/tool-conventions/blob/main/CodeMetadata.md)) ([#16])
- `inference.spec_funcs` custom section payload now starts with a `varuint32` version byte (`1` for current format). Consumers should reject unsupported versions. This is a wire-format change — anyone parsing the section directly must update; the in-tree parser handles it transparently. ([#16])

### Language

- Add struct definition and parsing support ([#14])
- Add division operator (`/`) support ([#86])
- Add unary negation (`-`) and bitwise NOT (`~`) operators ([#86])
- Parse visibility modifiers (`pub`) for functions, structs, enums, constants, and type aliases ([#86])

### Compiler

- ast: Remove dead `OperatorKind::BitNot` variant — `~x` is always parsed as `UnaryOperatorKind::BitNot` in a `PrefixUnaryExpression`; the binary enum variant was never produced by the AST builder ([#142])
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

- `FunctionOrigin { TopLevel, SpecInner }` enum threaded through `visit_function_definition`. Spec-inner functions can no longer be WASM-exported even when `pub`, closing a latent footgun for the upcoming `export` keyword ([#19])
- Per-spec function-index map (`spec_func_indices_by_spec : FxHashMap<String, Vec<u32>>`) replaces the prior single union list. Internal `build_func_name_to_idx` keys spec-inner functions as `"<SpecName>.<fn>"` so two specs may share function names; WASM `name` section emission stays unmangled ([#21])
- Emit `inference.spec_funcs` WASM custom section in `proof` mode carrying the per-spec index map. Bare `.wasm` binaries are now self-describing; the Rocq translator can recover the map without an out-of-band `CodegenOutput`. The section name uses the vendor-prefixed `inference.*` namespace rather than the `metadata.code.*` namespace reserved by the WebAssembly tool-conventions repo. Section is omitted in `compile` mode so binaries stay byte-identical ([#16])
- `wasm-to-v` crate: new `errors.rs` with `WasmToVError` thiserror enum (`InvalidRocqIdentifier`, `RocqStdlibShadow`, `EmbeddedSpecMismatch`, `WasmParse`) and `InvalidIdentifierReason` sub-enum, closing the CLAUDE.md compliance gap that left this crate without an `errors.rs` ([#20])
- `wasm-to-v` crate: `validate_rocq_identifier` helper rejects Rocq-illegal module/spec names (non-alphabetic leading char, invalid chars, length > 255, stdlib shadow, reserved vernacular/Gallina keyword) before they reach `Definition <name>` emission. Called at the top of `translate_bytes` and again per spec name in `translate()` ([#20])
- `wasm-to-v` translator: per-spec Rocq emission. Each entry in `spec_funcs_by_spec` produces one `Definition <mod>__<SpecName>_specs : list N` and one `Theorem valid_<mod>__<SpecName> : ValidSpec <mod> <mod>__<SpecName>_specs.`. Empty per-spec lists render as `(@nil N)` so they type-check regardless of scope state at the consumer site ([#21], [#22])
- Switch from LLVM to direct WebAssembly emission via `wasm-encoder` ([#125])
  - Remove all LLVM dependencies: `inkwell`, `build.rs`, external binaries (`inf-llc`, `rust-lld`)
  - Rewrite `compiler.rs` to generate WASM binary directly in-process
  - Non-deterministic instructions emitted as custom opcodes via `Function::raw()` byte sequences
  - Custom opcodes in 0xfc prefix space: uzumaki (0x31/0x32), forall (0x3a), exists (0x3b), assume (0x3c), unique (0x3d)
  - Reactor model: all `pub` functions exported individually, no `_start` entry point
- Add compilation architecture with `CodegenOutput` boundary ([#97], [#125])
  - `codegen()` returns `CodegenOutput` (WASM bytes + metadata)
  - `CodegenOutput` carries WASM binary, target, mode, opt level, module name, and `has_main` flag
  - New `Target` (Wasm32/Soroban), `CompilationMode` (Compile/Proof), and `OptLevel` (O0–O3/Os/Oz) enums
- Add per-function optimization strategy for proof mode (Decision #32) ([#97])
  - Spec functions compiled unoptimized to preserve structural correspondence with source for Rocq translation
  - Execution functions use target's release optimization so proofs cover actual deployed code
  - `OptLevel` is currently metadata only; optimization passes planned for future
- Add validation guards in `codegen()`: reject proof mode with non-Wasm32 targets, reject Soroban with non-det operations ([#97])
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
- Empty per-spec lists now emit `(@nil N)` instead of `[]%N` so the generated `Definition` type-checks regardless of whether `Open Scope N_scope` is active at the consumer's `Require` site. Downstream proof scripts matching `[]%N` literally must update ([#21], [#22])
- Add LLVM-based WASM code generation using `inf-llc` ([#44])
- Add custom LLVM intrinsics for non-deterministic instructions ([#44])
- Implement `forall`, `exists`, `uzumaki`, `assume`, `unique` block codegen ([#44])
- Add `rust-lld` linker invocation for WASM linking ([#44])
- Add mutable globals support in WASM compilation ([#44])
- Add base WASM code generation from typed AST ([#29])

### Analysis

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

### AST

- Migrate AST arena from `FxHashMap<u32, AstNode>` + `Rc<T>` + `RefCell<T>` to typed `Arena<T>` via vendored la-arena ([#156])
  - Typed indices (`ExprId`, `StmtId`, `DefId`, `BlockId`, `TypeId`, `IdentId`) prevent cross-category ID misuse at compile time
  - `AstArena` struct with separate `Arena<T>` per node category and `Index` trait for `arena[id]` syntax
  - `NodeId` enum for type-erased cross-category references (used in type annotation storage)
  - `Send + Sync` with compile-time assertion — no `RefCell` or `Rc` in AST nodes
  - Cache-friendly `Vec<T>` storage replacing heap-scattered `Rc<T>`
  - Remove `AstNode` enum, `ast_node!`/`ast_enum!`/`ast_enums!` macros, `enums_impl.rs`, `parent_map`/`children_map`

### CLI

- `infc --mode proof` and `infs build --mode proof` flags enable Rocq translation output. By default both tools run in `compile` mode (existing behavior, stripped specs). `--mode proof` keeps spec functions and writes the `.v` proof artifact alongside the `.wasm`. ([#22])
- `infc` now surfaces `WasmToVError::RocqStdlibShadow` and `WasmToVError::InvalidRocqIdentifier` with the dedicated user-facing messages from the plan (no `--module-name` flag mentioned — that flag does not exist yet) ([#20])
- Simplify `infc` and `infs build` default behavior: running without phase flags now performs full compilation and writes `out/<name>.wasm` ([#138])
  - `infc example.inf` equivalent to `infc example.inf --codegen -o`
  - `infc example.inf -v` produces both `out/example.wasm` and `out/example.v`
  - Supplying `--parse`, `--analyze`, or `--codegen` still overrides the default
  - Matches conventional compiler UX (e.g. `gcc foo.c`)
- Add `BuildProfile` (Debug/Release) with `resolve_opt_level()` for target-aware optimization ([#97])
- Remove external toolchain dependencies: no `inf-llc`, `rust-lld`, or platform-specific library paths required ([#125])
- Defer WASM compilation until output files are actually needed (`-o` or `-v` flags) ([#97])
- Refactor CLI architecture with improved argument handling ([#28])

### Rocq Translation

- Rewrite WASM-to-V translator for WasmCertCoq theory syntax ([#23])
- Add function name propagation to V output ([#24])

### Documentation

- New `core/wasm-to-v/ROCQ_CONTRACT.md` documenting the external Rocq predicates the generator depends on (`ValidModule` 1-arg, new `ValidSpec`), the emitted proof-skeleton shape, and the spec-map precedence rules (explicit vs embedded) ([#17])
- Add compilation targets matrix documentation (`book/compilation_targets.md`) ([#97])
  - 6-option matrix: Compile/Proof x Debug/Release x with/without non-det operations
- Add `unreachable` emission rationale document (`book/unreachable-emission-in-codegen.md`) ([#144])
- Add arithmetic overflow in WASM codegen deep-dive (`book/arithmetic-overflow-in-wasm-codegen.md`) ([#146])
  - WASM wrapping semantics, trapping instructions, negation behavior
  - Comparison with Rust, C, Zig, Go, Java overflow handling
  - Formal verification implications for Rocq translation
  - Empirical comparison: Inference vs rustc release vs rustc debug vs Soroban

### Type Checker

- Spec blocks now open a real symbol-table scope via `enter_spec`, parallel to `enter_module`. Spec-inner functions, structs, enums, type aliases, and constants live in a dedicated scope keyed by spec name, so two specs may declare same-named members without colliding ([#18])
- `flatten_defs_with_spec_inner` removed. The three phases that used it (`register_types`, `collect_function_and_constant_definitions`, and the body-inference loop) recurse into `Def::Spec` inline, opening the spec scope around the inner work ([#18])
- `TypedContext::lookup_struct` and `lookup_enum` now search across **all** scopes (`lookup_struct_anywhere` / `lookup_enum_anywhere`) so post-type-check phases (analysis, codegen) can resolve spec-inner types they walk into. Internal scope-local lookups inside the type checker are unchanged ([#18])
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
- Add 28 codegen tests with three-tier verification architecture ([#97], [#125])
  - Byte comparison tests against committed `.wasm` reference files
  - `inf_wasmparser::validate()` validation on all generated output
  - 2 Wasmtime execution tests verifying runtime behavior
  - Validation tests for metadata, target/mode combinations, non-det opcode presence
- Add codegen test helpers ([#97], [#125])
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

### Fixed

- Fix FxHashMap non-deterministic iteration in `Arena` — `filter_nodes()` and `list_nodes_cmp()` now sort by node ID, ensuring reproducible WASM function emission order
- Fix Drop instruction emission for nested non-det blocks — `parent_blocks_stack.last()` (innermost block) is now used instead of `.first()` (outermost block)
- Fix `lower_literal` to emit type-correct WASM const instructions — number literals now consult `TypedContext` and emit `i32.const` or `i64.const` based on inferred type instead of always emitting `i32.const`
- Fix `wasm_to_v` public API signature — parameter changed from `&Vec<u8>` to idiomatic `&[u8]`

### Project Manifest

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
[#97]: https://github.com/Inferara/inference/issues/97
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
[#16]: https://github.com/Inferara/inference/issues/16
[#17]: https://github.com/Inferara/inference/issues/17
[#18]: https://github.com/Inferara/inference/issues/18
[#19]: https://github.com/Inferara/inference/issues/19
[#20]: https://github.com/Inferara/inference/issues/20
[#21]: https://github.com/Inferara/inference/issues/21
[#22]: https://github.com/Inferara/inference/issues/22
