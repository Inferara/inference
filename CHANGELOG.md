# Changelog

All notable changes to the Inference compiler project.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

### Breaking

- `wasm-to-v` now fails closed on every construct the wasm-verifier proof contract does not cover, where it previously emitted terms the proof target cannot verify — or panicked. Each is refused as `WasmToVError::UnsupportedFeature` naming the construct: every floating-point instruction; every SIMD/vector instruction; **every conversion instruction, integer width conversions included** (the vendored stub declares no `cvtop` family at all, so `i32.wrap_i64` is as unrepresentable as `f32.convert_i32_s`); `f32`/`f64`/`v128` used as a value type in any position — function parameter, result, local, global, or block result type — so a module carrying an unused float *signature* stops translating even with no float instruction in any body; and the proposal families with no lowering (GC, exception handling both modern and legacy, stack switching, tail calls, 128-bit wide arithmetic, typed function references, `memory.discard`, and the segment-indexed table operations `table.init`/`elem.drop`/`table.copy`). This also deletes an ill-typed emission: the twelve float comparison arms wrapped the *integer* relop constructors inside the float wrapper (`BI_relop T_f32 (Relop_f ROI_eq)`, with `ROI_ge` left unapplied), which nothing caught because the `coqc` gate's corpus is Inference source and no Inference program lowers to float WASM. No Inference program reaches any of this surface — the language has no floating-point or vector types and its codegen emits no conversion — so the change is observable only for foreign bytes arriving through external linking (`infc -L` / `--wasm-dep` / `INFERENCE_WASM_LIB_PATH`) or the public `translate_bytes` API. Such inputs previously produced a `.v` that failed downstream — at the in-repo `coqc` gate as a type error ("The reference BI_cvtop was not found" against the vendored stub), or at the prover as an undischargeable obligation (the wasm-verifier program logic covers none of this surface, even where vanilla WasmCert-Coq can type the term); they now fail at `infc` with a message naming the construct and the position it occupies. Only one error surfaces per run, so a module with several unsupported constructs reports them one at a time. The integer and non-deterministic surface is byte-identical — every `.v` the Inference corpus produces is unchanged ([#284])
- `core/wasm-linker` retracts `i32.wrap_i64`, `i64.extend_i32_s`, and `i64.extend_i32_u` from its operator allow-list: an external `.wasm` using any of the three no longer links, failing with `integer width conversions (not supported by the Rocq translator)`. These were the last of the conversion block still allow-listed, kept on the stated premise that the Rocq translator had a lowering for them — but that lowering emitted `BI_cvtop`, so the premise held only at the Rust level and failed at `coqc`. The retraction is consistent with the file's earlier ones (saturating float-to-int, sign-extension, tail calls, segment-indexed table operations) and is required rather than optional: keeping the three would leave a `BI_cvtop` emission path alive. Note these are ordinary MVP instructions, so the feature gate cannot reject them — the allow-list is the only place they are refused ([#284])
- `inference_wasm_codegen::codegen()` now takes its configuration as one `CodegenOptions` value: `codegen(typed_context, target, mode, opt_level, module_name, features)` → `codegen(typed_context, module_name, CodegenOptions { target, mode, opt_level, features })`. Pure mechanical refactor — no behavior or emitted-byte change (every golden in both families is untouched); the recorded follow-up to the `EmitFeatures` parameter, landed before the parameter list grew again. `CodegenOptions` derives `Copy` and implements `Default` by hand (`opt_level` defaults to the *target's* default level, which a derived `Default` cannot express), and is the input mirror of the configuration `CodegenOutput` records. Library embedders migrate by wrapping their existing four arguments in the struct — or `CodegenOptions::default()` for the common Wasm32/Compile/default-opt/Wasm-1.0 case; the two-argument `inference::codegen` wrapper is unchanged ([#315])
- `Inference.toml` is now strictly verified: every struct table (the manifest root, `[package]`, `[build]`, `[build.wasm-opt]`, `[verification]`, and each `[wasm-dependencies]` entry's `{ path = … }` shape) rejects unknown keys with an error naming the expected fields, where they were previously ignored silently. A typo like `wasm_features` or a key placed under the wrong table now fails the build instead of yielding an artifact that quietly lacks the requested configuration; a `[package]` still carrying the long-removed `manifest_version`/`edition` keys now fails to load — delete them, no replacement is needed. Map-shaped tables whose keys are user data (`[dependencies]`, `[wasm-dependencies]` — their keys name dependencies) still accept arbitrary keys. Deliberate trade-off: an older `infs` reading a newer manifest's additive keys now errors rather than silently ignoring them — consistent with the compiler ABI gate, which already treats toolchain/manifest skew as a hard error rather than a silent downgrade ([#315])
- `inference_wasm_codegen::codegen()` gains a sixth parameter, `features: EmitFeatures` — the WebAssembly feature opt-ins the emitted module may use; `EmitFeatures::default()` keeps output inside WebAssembly 1.0. Library embedders of the five-argument form must pass it (migration: `EmitFeatures::default()`); the two-argument `inference::codegen` wrapper is unchanged and defaults it ([#315])
- Signed narrow division now traps on overflow: i8 `-128 / -1` and i16 `-32768 / -1` trap instead of silently wrapping to the minimum value. Division overflow now traps uniformly at every signed width: wasm itself traps i32/i64 `MIN / -1`, but narrow types divide in the promoted i32 width — where the true quotient +128/+32768 is representable, so no wasm trap fires — and the mandatory re-narrowing then silently sign-wrapped it back to MIN: a wrong answer with no failure signal. The compiler now guards the promoted quotient (`local.tee; i32.const 128|32768; i32.eq; if; unreachable; end`) before re-narrowing at every narrow signed division site, in both compile and proof modes, so proofs carry the same cannot-trap obligation at every width. `x / 0` and `x % 0` still trap natively at every width; `MIN % -1` remains `0` at every width (the mathematically correct remainder — intentionally not a trap); add/sub/mul and negation still wrap at every width. The narrow guard traps as `unreachable` while the full widths report wasm's native integer-overflow trap — the trap-or-not contract, not the trap code, is width-uniform. Precedents: Rust panics on signed division overflow in every profile (LLVM sdiv is UB on MIN/-1, so the check is unconditional — unlike the debug-only add/sub/mul checks), Zig safety-checks it, C makes it UB, wasm traps it at full width.
- Exported functions now trap on an out-of-range enum tag: every enum-typed parameter of an exported function gets a prologue guard (`local.get p; i32.const N; i32.ge_u; if; unreachable; end`) rejecting any host tag >= the variant count (negative tags arrive as huge unsigned values and are caught by the same compare). Previously an invalid tag (e.g. 99 for a 3-variant enum) flowed raw through the body as a phantom variant equal to no declared variant. The boundary rule extends the existing parameter canonicalization: a raw host i32 is normalized where a host convention already assigns every wire value a meaning (narrow ints take the low bits — the C ABI; bool takes truthiness), and rejected where it does not — a tag >= N names no variant under any convention, so any mapping (such as the `rem_u` used for provenance-free non-deterministic draws) would invent data. A parameter typed as a variantless enum now traps on every host call — the type is uninhabited, so no valid call exists; such exports previously returned normally. In-language callers always pass declaration-derived tags, so the guard never fires for them; enum returns need no guard (every in-language producer yields a valid tag, and entry validation now closes the only propagation source). Precedents: Rust (an out-of-range fieldless-enum discriminant is UB, so boundaries validate), wasm-bindgen (invalid enum values are rejected in glue), Zig (`@enumFromInt` out of range is safety-checked).
- Shift counts are now taken modulo the operand type's bit width for every integer type. WebAssembly masks shift counts modulo 32/64 in the promoted width, which for narrow types produced a non-monotonic cliff: `u8 x << 8` was `0` but `x << 32` was `x` (count masked to 0). Narrow-typed (`i8`/`u8`/`i16`/`u16`) shifts now mask the count (`& 7` / `& 15`) before the wasm shift, extending wasm's own mod-width semantics to the declared type: `u8 x << 8` now yields `x`, `x << 9` yields `x << 1`, and a negative dynamic count masks the same way. `i32`/`u32`/`i64`/`u64` shifts are byte-for-byte unchanged (their width equals the wasm mask width). Signed `>>` stays arithmetic and unsigned `>>` logical — only the count rule changes. Precedents: wasm/JS mask modulo width, Rust masks modulo width in release builds, Zig makes out-of-range counts unrepresentable, C leaves them undefined.
- New analysis rule **A045** (`FieldLessStructValue`, error) rejects *values* of a struct with no fields, while leaving the declaration legal. A field-less struct occupies zero bytes, so it has no value representation: there is no memory region to hold, copy, or reason about one of its values. The rule rejects such a type as a struct literal (in every expression position), as the declared type of a `let` or of a `const` at function or module scope, as a function/method/`external fn` parameter — `mut` and `_: E` included — as a return type, as a struct field, and as a `self`/`mut self` receiver declared on such a struct, looking through array nesting at any depth (`[E; 3]`, `[[E; 2]; 3]`). This fixes a compiler abort: `struct E { fn tag(self) -> i32 { return 7; } }` with any `E { }` literal previously killed `infc` on an internal assert from struct-literal lowering. It also rejects five shapes that previously compiled *by accident*, each lowered under a representation nobody chose — a field-less-struct parameter (`e = e` became a scalar copy of a pointer into a zero-byte region), an array of field-less structs, a field-less-struct-typed struct field, an `external fn` signature (an emitted import taking a pointer to nothing), and a `@` drawn at a field-less struct type inside a `spec`. Rejecting the *field* position is what closes the hole: a struct all of whose fields are zero-sized would itself be zero-sized, so forbidding a zero-sized field collapses the transitive case into the base case, and a program is reported one diagnostic per offending declaration rather than one per use (assignments and reads need no rule of their own, since each requires a binding, parameter, or field that is already rejected). Migration: give the struct at least one field if you need values of it, or keep it as a pure namespace and declare its functions without `self`. The method-namespace idiom is explicitly preserved — `struct E { fn helper() -> i32 { … } }` with `E::helper()` compiles unchanged, cross-file included — and a `self`-taking method on a field-less struct is rejected at its declaration because, once no value of the struct can exist, the method is uncallable by construction; the fix is the one keyword A010 already suggests. **A011 is unchanged** in predicate, severity, message, and tests: it warns about a struct with no fields *and no methods* — a declaration that declares nothing — which is a disjoint subject, and deliberately stays silent on the namespace idiom. Where both apply (a bare empty struct that is also given a value) both fire; there is no cross-rule suppression — a module-scope `const` is likewise reported in its own right rather than left to A032's blanket rejection of top-level `const`, so the closure does not quietly depend on that unimplemented-feature gate staying in place. Two documented non-scopes: generics, since a type parameter never resolves to a struct, so a generic signature (`fn id T'(x: T) -> T`) is outside the predicate — nothing is missed by that today, because the compiler does not monomorphize and codegen rejects a generic type outright, so there is no instantiation at a field-less struct to check; and local type aliases, which are non-transparent in Inference and so are a dead end rather than a route to a value ([#332])
- New analysis rule **A044** (`ShiftCountOutOfRange`, error) rejects a shift whose count is a literal (including parenthesized and negated literals) that is negative or greater than or equal to the operand type's bit width, e.g. `x << 32` or `x >> -1` on `i32` — such a shift never means what it says. Programs that previously compiled with such counts are now rejected. The rule reads the width from the shifted operand's type, and a literal count takes that operand's type (see contextual literal typing under Changed), so every width is genuinely reachable rather than merely covered in principle: `x << 64` on an `i64` and `x << 8` on a `u8` are rejected alongside `x << 32` on an `i32`. Const-declared counts (`const K: i32 = 33; x << K`) are not detected — the same statically-known-literal scope as the division-by-zero check and A022.
- `inference-parser`'s publicly re-exported `Input::new` now takes the source text alongside the tokens: `Input::new(&tokens)` → `Input::new(src, &tokens)`. A grammar rule needs a token's *spelling*, not only its kind, so the number-literal rule can quote back what the author actually wrote (see the teaching diagnostics under Changed); the trivia-free token view is where that lookup belongs, since it is the view a rule already holds. The two arguments must describe the same source or every span and spelling read through the view is wrong, so a debug assertion pins the pairing against the `Eof` sentinel's offset (`Lexer::run` places it at `src.len()`). Every in-repo caller is inside `core/parser` itself; an external embedder constructing an `Input` directly must pass the string it lexed ([#219])
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
- `inference::wasm_to_v` / `translate_bytes` gains a fourth parameter,
  `hspecs_by_spec: &inference_hassert::HSpecMap`, the per-spec `hassert`
  verification-obligation map. Pass `HSpecMap::default()` to source obligations
  entirely from the embedded `inference.hspecs` custom section (the normal
  post-link CLI path); pass a populated map to override or supplement it,
  mirroring the existing `spec_funcs_by_spec` explicit-vs-embedded precedence.
  `CodegenOutput` gains a matching `hspecs()` accessor (empty in compile mode)
- Rocq output targets wasm-verifier (a private Inferara repository; the
  vendored stub in `core/wasm-to-v/rocq-stub/` and
  `core/wasm-to-v/ROCQ_CONTRACT.md` are the in-repo statement of its interface)
  on vanilla WasmCert-Coq v2.2.0, replacing the WasmCert-Coq-Essence fork.
  `ValidModule : module -> Prop` is now truly 1-ary and always emitted (even
  for a module with zero specs); the new `ValidSpec : module -> list hassert
  -> Prop` predicate carries the per-spec proof obligation as a **`hassert`
  value** — a translated logical formula — rather than a WASM function-index
  list. This supersedes both the shape the translator actually emitted before
  this change (a 2-ary `ValidModule <mod> <specs>` with `specs : list N`, no
  separate `ValidSpec` at all) and the 1-ary-`ValidModule`-plus-`list N`-
  `ValidSpec` shape this file and `core/wasm-to-v/ROCQ_CONTRACT.md` previously
  *documented* under issue #21 but the translator never actually implemented.
  A `spec` function's WASM body is also no longer translated as instructions
  at all: it is omitted from the module record entirely (module-, export-,
  element-, start-, and `T_app`-index remap follows — see
  `core/wasm-to-v/ROCQ_CONTRACT.md`), and its logical content is instead
  derived, AST-side, from the specification body itself, `forall`-quantified
  (or plain) spec functions only this milestone. Downstream Rocq libraries
  must define the hassert-valued `ValidSpec` and update existing `ValidModule`
  consumers. Theorem names: `valid_<mod>` is 1-arg, per-spec theorems are
  `valid_<mod>__<SpecName>` (double underscore, with explicit collision
  rationale documented in `core/wasm-to-v/ROCQ_CONTRACT.md`) ([issue#17], [issue#21])
- New leaf crate `core/hassert` (`inference-hassert`): the `HAssert`/`HTerm`
  verification-obligation IR mirroring wasm-verifier's `term`/`hassert`
  inductives, smart constructors with `HA_true`-identity simplification
  (`and`/`imp`/`or`/`ex`/`nz`/`eqz`), and the codec for the new
  `inference.hspecs` custom WASM section (version 1, sorted symbol table,
  specs sorted by name, per-hassert `(fn_symbol, tree)` records, LEB128, a
  hardened decoder with bounds/UTF-8/trailing-byte/recursion-depth checks).
  Used by `wasm-codegen` (producer), `wasm-linker` (verbatim carrier), and
  `wasm-to-v` (consumer) so the wire format has one implementation instead of
  the per-crate duplication `inference.spec_funcs` has today
- New fatal proof-mode diagnostics `P001`–`P009` (`core/wasm-codegen/src/hassert/`):
  a specification function that cannot be translated to a `hassert`
  obligation — an `exists`/`unique`/`assume`-quantified body, a construct
  with no assertion encoding (`loop`, `break`, `unique`, `**`, memory access),
  reassignment, a non-scalar term/parameter/`@`, an untranslatable call, or a
  quantified spec *method* — now aborts code generation
  (`CodegenError::UntranslatableSpec`) instead of silently emitting a module
  whose specifications are unverifiable. Every diagnostic in a spec is
  collected before failing
- `wasm-to-v` rejects any non-deterministic instruction (`forall`/`exists`/
  `assume`/`unique`/uzumaki) reaching a *surviving* (executable, non-spec)
  function body as `WasmToVError::UnsupportedFeature`. With `spec` functions
  omitted from the module record and analysis rule A042 barring non-det
  syntax outside a `spec` declaration, this path is unreachable from
  Inference-compiled code; the rejection is defense-in-depth against a
  foreign or hand-crafted `.wasm`. Retires the `BI_unique` typecheck debt the
  vendored stub previously carried
- Lower `assert(<bool>)` to a WASM trap-on-false (previously panicked codegen) ([#195])
  - Emits `<cond>; i32.eqz; if (empty); unreachable; end` — the smallest correct shape, and one that `wasm-to-v` already maps to `BI_unreachable` for proof-mode translation
  - Asserts are emitted in both `Compile` and `Proof` modes (Stmt-level, not Def-level); no `CompilationMode` branching
  - Soroban target accepts asserts — `Unreachable` is baseline WASM, not a 0xfc non-det opcode
  - New golden fixture `tests/test_data/codegen/wasm/base/assert/` exercises literal, variable, nested-in-if, loop+break, double-assert, bool param, unary `!`, `&&`, `||`, `==`, compound `(a > 0) && ((b < 10) || (c == 0))`, and bool-local scenarios, with wasmtime execution coverage that distinguishes pass paths from `Trap::UnreachableCodeReached` paths
- WASM custom section name for the per-spec function index map is now `inference.spec_funcs` (vendor-prefixed namespace). External tools previously looking for `metadata.code.inference.spec_funcs` must update. The latter was a misuse of the WebAssembly tool-conventions reserved namespace ([CodeMetadata.md](https://github.com/WebAssembly/tool-conventions/blob/main/CodeMetadata.md)) ([issue#16])
- `inference.spec_funcs` custom section payload now starts with a `varuint32` version byte (`1` for current format). Consumers should reject unsupported versions. This is a wire-format change — anyone parsing the section directly must update; the in-tree parser handles it transparently. ([issue#16])
- New analysis rule **A042** (`NonDetOutsideSpec`, error) rejects non-deterministic constructs used outside a `spec` declaration. The non-deterministic block forms — inline `forall`/`exists`/`assume`/`unique` statement blocks and the function-body-modifier form (`fn f() forall { … }`) — are now valid only lexically inside a `spec`. Programs that previously compiled with such a block in a plain function or struct method are now rejected (one diagnostic per outermost offending block); move the specification logic into a function inside a `spec` declaration. The check is lexical, so it fires in both compile and proof modes.
- `&&` and `||` now short-circuit: the right operand is evaluated only when the left operand does not decide the result (`a && b` skips `b` when `a` is false; `a || b` skips `b` when `a` is true). Evaluation stays left-to-right and results stay canonical `0`/`1`; bitwise `&`/`|` are unchanged. Previously both operands were always evaluated and combined with `i32.and`/`i32.or`, which made the universal guard idiom trap at runtime: `x != 0 && 100 / x > 1` trapped at `x == 0`, and `i < len && arr[i] != 0` trapped at `i == len`. Such programs now return the guarded value instead of trapping. Observable changes: previously-trapping guard expressions now produce values; a right operand is no longer executed when the left decides the result, so its traps and side effects (e.g. mutating method calls) no longer occur in that case; the emitted code replaces one bitwise instruction with a valued `if (result i32)` block (+5 bytes per operator), trading the unconditional right-operand evaluation for a branch. Rationale: matches C/Rust/Zig expectations and makes guard idioms both executable and dischargeable as proof obligations. Fires identically in compile and proof modes.
  - Emitted shape: `<lhs>; if (result i32); <rhs>; else; i32.const 0; end` for `&&`; `<lhs>; if (result i32); i32.const 1; else; <rhs>; end` for `||`; left-associative chains lower flat (sequential valued ifs).
  - New golden fixture `tests/test_data/codegen/wasm/short_circuit/` with wasmtime coverage separating lazy-skip paths from still-evaluated paths by trap identity (`IntegerDivisionByZero` vs `UnreachableCodeReached`); proof-mode fixture `spec_short_circuit.inf` pins the first-ever valued `BI_if (BT_valtype (Some (T_num T_i32)))` in the `coqc` corpus gate.

### Changed

- Integer literals are contextually typed: a literal takes the type of the position it appears in, rather than always being `i32` ([#219])
  - **The rule.** An integer literal has no intrinsic type. An expected type arises at an annotated `let`/`const` initializer, an assignment right-hand side (for `p.x = 5` and `a[i] = 5`, the field's or element's type), a struct-literal field value, an element of an array literal expected to be `[T; N]`, a call argument (free, associated and method calls alike), and the operand of `return`. It descends unchanged through the type-transparent forms `( e )`, `-e` and `~e`, and — only when both operands are built entirely out of literals — into both operands of `+ - * / % & | ^ << >>`. Where exactly one operand of *any* binary operator is literal-built and the other has an integer type, the literal-built side takes its peer's type; that includes comparison, equality, and the shift count, which must match the shifted operand's width because code generation picks the shift opcode from the left operand alone. Nothing else propagates — array indices, the operands of a comparison or logical operator taken as a whole, receivers, and call results all keep the types they already have. A literal that receives no type is `i32`, exactly as before.
  - `a << 16`, `a + 65536`, `a < 65536`, `id(a, 65536)`, `return 65536` and `return -42` now compile against `i64`/`u64` without the `let one: i64 = 1;` preamble each used to need. `u64::MAX` (`18446744073709551615`) becomes expressible at every one of those positions: it fits no integer type but `u64`, so before this it could not be written as a call argument, a `return` operand, or an operand of any operator at all. Verification benefits directly — each such preamble was an extra WASM local plus a `local.set`/`local.get`, which became state in the Rocq translation; removing it makes proof terms smaller.
  - **This is not coercion**, and `new-value-copy-semantics`' "all type conversions must be explicit" stands unchanged. Contextual typing selects the type a literal *denotes*; it converts nothing, no expression that already has a type ever changes it, and no widening or narrowing instruction is emitted. Two typed values of different widths still never combine. Nor is there arbitrary-precision untyped-constant arithmetic as in Go and Zig: every node of a literal-built expression is stamped with the target type and evaluated at that width, so `let x: u8 = 200 + 100;` wraps like any other `u8` addition and WebAssembly and Rocq semantics stay identical with no separate constant-evaluation path. A literal that does not fit the type it receives is rejected by A022, never truncated. One boundary consequence at the signed minimum: spaced negation `let a: i8 = - 100;` now type-checks (the expected type descends through `-e`), but the range check sees the un-negated literal, so `- 128` is rejected (`128` exceeds `i8`) while glued `-128` is accepted — the minus is part of the number token there. Write the glued form for a signed minimum.
  - **No code generation changes.** Code generation was already fully type-directed — it reads each literal node's recorded type — so the WASM emitted for every previously-accepted program is byte-identical and the whole existing golden corpus is untouched. The mechanism is a check-mode parameter, `infer_expression_expecting(expr_id, expected, ctx)`, replacing five hand-duplicated "stamp the declared type onto the child before inferring it" sites that had drifted into five subtly different rules (one lacked the numeric guard, one seeded after its check, none recursed). `infer_expression` remains as the shim that passes `None`.
  - **Duplicate diagnostics collapse.** A literal whose target type is not numeric used to be reported twice — once eagerly before inference, once by the ordinary post-inference check — at both the variable-definition and the assignment site, including the array-typed-target variant. `let x: bool = 5;`, `let a: [i64; 2] = 5;` and `b = 5;` with `b: bool` now each emit exactly one mismatch, making them symmetric with the `const` path, which already emitted one.
  - **Two pre-existing bugs fall out of routing array elements through the same typed path.** `let a: [bool; 2] = [1, 2];` stamped `bool` onto the literals and crashed code generation with "Unsupported number literal type: Bool"; `let a: [Point; 2] = [1, 2];` stamped the struct type onto them, whereupon the array literal *matched* its own annotation and the program was accepted and compiled as if two integers were two `Point`s — a miscompile, not a crash. Both are now the same ordinary rejection, reported once — type mismatch in variable definition: expected `[Bool; 2]`, found `[i32; 2]`. Array-element typing also newly reaches assignment and `const` positions and nested initializers, so `a = [1, 2];` with `a: [i64; 2]`, `const A: [i64; 2] = [1, 2];` and `[[1, 2], [3, 4]]` for `[[i64; 2]; 2]` are accepted where they were rejected before.
  - **Proof obligations fail loudly rather than silently.** The `hassert` spec translator duplicated code generation's literal width dispatch but mapped a literal with no recorded type to `HConst::I32` and a failed parse to `0`. Both are now invariant panics with explanatory messages, matching the neighbouring code-generation convention (compiler-bug assertions, not `PCode` user diagnostics). A propagation gap would have put a constant into a proof obligation that the compiled program never computes, making the proof about a different program than the one that runs; it now aborts instead. This landed before the typing change, so it guarded every subsequent step.
  - **New parser teaching diagnostics** for the shapes that used to lex as a number glued to an identifier. `16i64` (and `16_i64`, `5i128`, `16usize`, `16u` — one uniform message for every suffix shape, so none is implied to be recognized) reports "integer literals do not take a type suffix — remove `i64`; an integer literal takes its type from where it is used…"; `1_000`, `0x1F`, `0b01`, `0o17` report "…Inference numbers are decimal digits only — no `_` separators and no `0x`/`0b`/`0o` prefixes". Each is exactly one diagnostic: the offending token is consumed into the literal node, which is what removes the old "expected Semi" + "expected an expression" cascade. This also eliminates a silent correctness trap — `1_000` previously parsed as the number `1` followed by an identifier `_000`, and `0x1F` as `0` followed by `x1F`. Literal suffixes are deliberately not implemented: contextual typing subsumes every case one would solve, and a suffix would be a second spelling of an existing capability and a one-way door on the number-token grammar. Two consequences remain visible only on the IDE's resilient parse path, where a rejected program is still lowered: `[i32; 1_0]` lowers to size `1` and `0x1F` to the value `"0"`. Neither is reachable through `infc`/`infs`, where a parse error aborts before lowering is consumed.
  - **Diagnostics name where a type came from.** A literal's type is now often written somewhere the literal is not, so A022 (`LiteralOutOfRange`) appends the position that supplied it — note: the literal is typed `u8` by the type expected in return statement — for annotations, arguments, returns, fields, elements, and the peer-operand case ("to match the other operand of `Shl`"). The provenance lives in a diagnostics-only side table on `TypedContext`; `node_types` remains the single source of truth for what a literal denotes and no backend may consult it. `BinaryOperandTypeMismatch` gains a note stating that Inference has no implicit widening and no cast operator, so the two types never combine and the fix is at one of the two declarations. Struct-literal field mismatches get their own `TypeMismatchContext::StructField`, reporting a mismatch in field `x` of struct `P` where they previously mislabelled themselves as being in a variable definition.
  - **Generic calls are unchanged and remain a known limitation.** `infer_type_params_from_args` observes `i32` for a bare literal argument and pushes `ConflictingTypeInference` before any expected type exists, so `id(a, 65536)` on a generic `id T'(x: T, y: T)` with `a: i64` is still rejected and `id(65536, a)` still binds `T = i32`. One adjacent shape did change: a literal at a *concrete* parameter of a generic function (`take T'(a: T, n: i64)` called as `take(x, 65536)`) is now typed by that parameter and passes the type checker, then fails in code generation with "unsupported type in WASM codegen: T" because monomorphization is unimplemented. The failure moved rather than went away; the underlying gap is generic code generation, not literal typing.
- Rebuild the type checker's scope tree as an index arena, making `TypedContext` `Send + Sync` ([#157])
  - `core/type-checker`'s `Scope` tree no longer uses `Arc<RefCell<Scope>>`/`Weak`; scopes live in a `Vec<Scope>` keyed by a `ScopeId(u32)` newtype, and parent/child links plus the scope maps are plain ids. This removes interior mutability entirely (rather than relocating it behind a lock), so `TypedContext` is now structurally `Send + Sync` — asserted at compile time in `typed_context.rs`, mirroring the `AstArena` assertion.
  - Behavior-preserving: scope-id allocation order, resolution order, and diagnostics are content-identical, and the WASM/Rocq golden corpus is byte-identical. The one ordering shift: diagnostics emitted while iterating all scopes (cross-scope import collisions) now come out in ascending scope-id order instead of hash-map order — deterministic where it was arbitrary. The scope id stays a bare `u32` at every public boundary (`StructInfo::definition_scope_id`, the `TypedContext` query surface), so no downstream crate is affected.
- Memoize `ide-db`'s per-entry analyses with Salsa (`salsa = "0.27"`, pinned to 0.27.x to match rust-analyzer) in place of the hand-rolled `FxHashMap`/generation-counter memo ([#157])
  - `RootDatabase` becomes a `#[salsa::db]` holding `Storage<Self>`; its `analyses` and `generation` fields are gone. A single tracked query (`analyze_entry`) runs the whole `FileAnalysis::compute` body, so a repeated request returns Salsa's memo. The `analyses`/`generation` state is replaced by a per-entry Salsa input plus lightweight bookkeeping.
  - The `Vfs` overlay stays outside Salsa storage: the import closure is read through the overlay-then-disk `VfsLoader`, the single resolution seam the compiler and IDE share, which must not be mediated by Salsa. The closure-aware invalidation, sticky per-document source roots, and never-opened FIFO cap remain in `RootDatabase` and force a recompute by bumping the entry's Salsa input (Salsa 0.27 has no per-memo eviction API; observable recompute-after-eviction is preserved).
  - Behavior-identical: the `FileAnalysis::generation` recompute probe is preserved by stamping from a monotonic counter read inside the tracked query body (a memo hit returns the prior stamp; a recompute mints a fresh one). All `ide-db`, `ide`, and LSP suites pass unmodified. No `salsa` type reaches `ide/ide` or `apps/lsp`.
- Make `ide/ide`'s feature-query surface (`Analysis`) take `&self` instead of `&mut self`, so one `AnalysisHost` can answer queries through a shared borrow ([#157])
  - `AnalysisHost` now wraps its `RootDatabase` in a `RefCell`; the write methods (`open`/`change`/`close_document`) stay `&mut self` and reach the database through `get_mut()` (a compile-time borrow, no runtime check), while `analysis()` and every query method take `&self` and open a `borrow_mut` scoped to that single call.
  - The database access underneath stays interior-mutable — a read still memoizes into the one host-owned `RootDatabase` in place — this is *not* a cloned-handle read model. A genuine cloned-`Storage` read handle is deferred to the cancellation work, because the read path still bumps a Salsa input on never-opened eviction and a write from one live handle blocks until every other handle drops, so cloned readers would not yet be freely concurrent.
  - Behavior-identical: no observable query, diagnostic, or invalidation change, and `RootDatabase` (and all of `ide-db`) is untouched. A new guard test asserts no `salsa` symbol reaches `apps/lsp`.
- Cancel in-flight LSP analyses: a `didChange` interrupts and supersedes older in-flight requests (answered `ContentModified`), `$/cancelRequest` is wired (`RequestCanceled`), and a cancelled analysis no longer discards the analysis cache the way a contained panic does ([#157])
  - `ide-db` gains `AnalysisCancelSource` (a monotonic write epoch plus the bound database handle's cancellation token) and an `is_cancellation` payload predicate, both re-exported through `inference-ide`. `RootDatabase`/`AnalysisHost` expose `bind_cancellation`, and the tracked analysis query polls at the fetch entry and at two `FileAnalysis::compute` stage boundaries, so a cancellation requested from another thread unwinds a long analysis instead of running it to completion. `RootDatabase` is asserted `Send` and `AnalysisCancelSource` `Send + Sync` at compile time.
  - `apps/lsp`'s resilient wrappers classify a caught unwind: a cancellation (a request superseded by a newer document write, told apart by the write epoch) is answered `ContentModified` and leaves the analysis host intact — a cancelled analysis leaves no memo behind — while a genuine panic still answers `InternalError` and rebuilds the host, and the rebuild rebinds the cancellation source to the fresh handle. No `salsa` symbol reaches `apps/lsp`; the classification uses the re-exported `is_cancellation`.
  - `apps/lsp`'s message loop becomes a router/worker split: an analysis worker thread owns the `ServerState` and the sole analysis handle and runs the loop one job at a time, while the main thread routes messages instantly, keeps incoming request-id bookkeeping, answers `$/cancelRequest` with `RequestCanceled` (-32800), and requests cancellation of the in-flight analysis before forwarding an adopted document write (or shutdown/exit). Dispatch stays strictly serial — no concurrent analyses. The transport pump is subsumed by the router; the unbounded job channel is where a typing burst accumulates for coalescing, unchanged in effect. A completion gate suppresses the worker's late response for a request the router already answered `RequestCanceled`.
  - Behavior-identical until a cancellation is actually fired: every existing `ide-db`, `ide`, and LSP test passes unmodified.
- Route `ide-db`'s analysis invalidation through Salsa dependency edges — per-file change stamps plus a conditional availability epoch registered by the query — with the write-path pass reduced to editor-facing staleness bookkeeping ([#157])
  - The `analyze_entry` query, once it knows the closure it just read, registers a `FileStamp` input edge for every closure file and an `AvailabilityEpoch` edge when an import went unresolved. The overlay mutators bump the matching input (`bump_file_stamp` on any `didOpen`/`didChange`/`didClose`, `bump_availability_epoch` on an open that makes content newly available), so a change the `Vfs` loader seam hides from Salsa now forces exactly the affected memos to recompute through Salsa's own dependency tracking rather than a per-entry revision bump. The epoch read stays conditional on the last compute having a missing import, reproducing the documented over-approximation and the `didClose`/`didOpen` overlay-presence semantics byte-for-byte.
  - The former closure-scan invalidation pass survives as a setter-free selectivity mirror (`note_stale_entries`): it clears each stale entry's cached analysis to `None` so the republish sweep, `is_analyzed`, the closure-donor search, and the never-opened cap keep their write-time view, but forces no recompute. A debug-only assertion in `analysis()` machine-checks that a memoized mirror hit is always a Salsa memo hit, catching any drift between the edges and the mirror. The one surviving revision bump backs the never-opened FIFO eviction (`evict_analysis`), which has no file event to stamp; it retires with the cap (#157).
  - Behavior-identical: every existing `ide-db`, `ide`, and LSP suite passes unmodified, including the 28 generation relational assertions and the #242/#243 regression families. New regression tests pin the event-only invalidation seam (a disk edit without an editor event stays invisible), the shared get-or-create stamp registry, the write-time mirror flip, and the conditional epoch's over-approximation and post-recovery drop. No `salsa` symbol reaches `apps/lsp`.
- Actually free `ide-db`'s evicted and closed entries' memoized analyses — an evicted-flag sentinel swap releases the superseded memo instead of leaving it resident until an unrelated recompute ([#157])
  - Closing a document, a never-opened cap eviction, and a change that stales a never-opened entry now set an `evicted` flag on the entry's Salsa input and queue a sentinel swap. The next `analysis` call recomputes the evicted entry to a roughly two-word `AnalysisResult::Evicted`, which pushes the superseded analysis onto Salsa's deleted list to be freed at the next revision boundary (a version-pinned 0.27.2 behavior, so at most one fat memo is ever pending); a requery un-evicts the entry with a single false-write that forces exactly one fresh recompute. The former per-entry revision bump is retired in favor of the flag.
  - Resident full analyses are now bounded by *open documents + `MAX_UNOPENED_ANALYSES` + a one-write-lagged transient* (a memo superseded or swapped since the last Salsa write), proven by a `Weak`-registry liveness probe and `WillExecute` execution-count assertions. A small per-path metadata residue stays session-permanent (Salsa 0.27 has no input removal). `MAX_UNOPENED_ANALYSES` is now `pub`; two `#[cfg(debug_assertions)] #[doc(hidden)]` probe seams were added; no release-build public API or wire-observable LSP change.
  - Salsa LRU, durability tiers, and result backdating were evaluated and rejected at closure granularity (no pinning or eviction callback in Salsa 0.27's LRU; the query output is `no_eq` with no dependents; the identical-text write-path skip is forbidden by the event-keyed invalidation contract) — re-audit under #280 when per-file parse granularity lands, and snapshot reads from a cloned `Storage` are deferred to #292. Behavior-identical: every existing `ide-db`, `ide`, and LSP suite passes unmodified, including the FIFO cap quartet, the generation relational assertions, the seam-invisibility contract, and the cancellation families ([#247])
- Serve read-only LSP feature requests (hover, definition, completion, documentSymbol, inlayHint) off the analysis worker on a two-thread read pool, so a slow analysis no longer blocks a fast interactive request behind it ([#292])
  - The worker mints a per-request snapshot (a cloned Salsa `Storage` handle sharing the overlay, generation counter, and stamp registry) for a request against a memoized entry — or a stale entry under a cached definitive source root — and hands it to a pool thread; the pool serves the query off the snapshot and posts the outcome back as an event. The snapshot (and its `Storage` clone) drops before any response is sent, and a concurrent write quiesces every live snapshot before it mutates the overlay (Salsa's setter waits for every outstanding handle to drop). Everything else — a first analysis of a path, an evicted entry, a tier-3 provisional stale entry, a superseded job — stays on the strictly serial worker path.
  - The worker stays the sole minter and the sole mutator of the entry/root/cap bookkeeping (`RootDatabase.worker`), so a snapshot cannot create an entry, evict, or touch the never-opened cap; the resident-memory bound is unchanged. A write superseding an in-flight pool read answers `ContentModified` (-32801); an equal-epoch worker-internal cancellation routes the request back for serial service under its original epoch; a pool compute panic is contained and rebuilds the host. The `apps/lsp` boundary still names no `salsa` symbol (the snapshot surface reaches it through `inference-ide` re-exports only), and every existing `ide-db`, `ide`, and LSP suite passes unmodified.
  - New: `RootDatabase::plan_concurrent_read`/`apply_concurrent_read`/`apply_unopened_read_bookkeeping` and the `ReadSnapshot`/`ReadServe`/`ConcurrentReadPlan` types in `ide-db`; the additive `AnalysisHost` snapshot surface (`AnalysisSnapshot`/`DocumentAnalysis`/`ReadPlan`/`SnapshotServe`) in `ide/ide`, leaving the `&self` `Analysis` query surface byte-identical; a `crossbeam-channel` dependency in `apps/lsp` (version-aligned with `lsp-server`) for the worker↔pool task/event channels ([#292])
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

- `wasm-to-v`'s operator translation is now `todo!()`-free: all 285 unimplemented
  arms in `translate_basic_operator` are gone, replaced by grouped rejection arms,
  so an operator with no lowering yields a diagnostic instead of aborting the
  process. This mattered most on the external linking path, where an abort
  bypasses every `?` and produces no diagnostic at all — strictly worse than the
  ill-typed emission the same change removes. Scope of the claim, stated
  precisely: the *operator match* is now total. Three `unwrap()` sites elsewhere
  in the file (`local.unwrap()`, `next_operator.as_ref().unwrap()`,
  `target.unwrap()`) are untouched, so this is not a claim that the translator
  cannot panic ([#284])
- `infc`'s `UnsupportedFeature` message no longer frames a modeled limit as
  unfinished work. It said the feature was "not yet supported" and had "not yet
  been wired through", which promises a future version that accepts the input;
  for a construct the proof model does not describe, no such version is coming.
  The message now says the construct falls outside the subset the WasmCert
  proof model describes, that this is a property of the model rather than unfinished work,
  and gives the actionable alternative: the module still compiles and runs, so
  dropping `-v` (and any explicit `--mode proof`) builds the `.wasm` without a
  proof artifact. Wording is cause-agnostic — the same arm serves `memory64`,
  atomics, deep nesting, and non-det-outside-spec — and no control flow or error
  variant changed ([#284])
- Every embedder now runs the compiler's recursive phases on an explicitly sized stack, so
  deep input no longer aborts the process where it previously did. Parsing, lowering, type
  checking, analysis and codegen each descend once per level of the input's syntactic
  nesting, and a stack overflow aborts rather than unwinding — no thread can catch one and
  turn it into a diagnostic — so the only mitigation is headroom that does not depend on
  which thread happens to be running. `inference_parser::MIN_COMPILE_STACK` (128 MiB) states
  the requirement, the new `inference::with_compiler_stack` runs a closure on a thread that
  meets it, and `infc`'s driver now runs inside that helper. Exit codes are unchanged —
  `process::exit` terminates the process identically from the scoped worker thread, and a
  panic is re-raised on the calling thread with its original payload, printed exactly once —
  and the only stderr difference is that a panic header now names the compile thread
  (`thread 'inference-compile' panicked at …`) rather than `main`, which also makes an
  overflow report which thread overflowed.
  Measured on macOS aarch64 in debug, the reported repro is fixed — an operator chain that
  aborted at 350 operands (300 was the last that compiled) now compiles at 5,262, and an
  `else if` chain that aborted at 900 arms now compiles past 2,000. Input deeper than the new
  ceilings still aborts; making rejection *deterministic* needs an explicit depth limit with
  a proper diagnostic, which is separate and still pending. This change is what makes such a
  limit reachable in every embedder rather than shadowed by whichever host stack runs out
  first ([#322])
- parser: `SyntaxNode`'s drop is now iterative. The derived drop glue descended once per tree
  level, so merely *discarding* a deeply nested CST overflowed the stack — the path any
  rejected over-deep input has to take. Detaching the children into a worklist and draining
  it holds the depth at one: a 500,000-level tree, constructed directly, is now discarded on
  a bare 2 MiB thread, where the recursive glue capped at roughly 8,200 levels. This makes a
  tree safe to *discard*, not safe to *build*: CST construction still recurses with tree
  height — through the grammar's own recursive descent, and through `Builder::leave`'s span
  resolution for shapes whose extremal children are nodes rather than tokens — and that side
  binds first for a parsed tree (measured on 8 MiB: construction caps at 14,994 levels, the
  old drop glue at 32,804). Bounding the
  construction side is separate, still-pending work; removing the drop-side consumer is what
  lets a rejected tree be freed at all, and what makes limit-depth trees testable on small
  stacks. The derived `Clone`, `PartialEq` and `Debug` on `SyntaxNode` and `SyntaxElement`
  remain recursive and must not be applied to a tree of unbounded height ([#322])
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

- Compound (struct/array) parameters that the callee provably never writes are now passed by reference: no frame slot, no copy-on-entry, and — when nothing else in the function needs memory — no frame, prologue, epilogue or `__stack_pointer` mutation at all. Every compound parameter used to be copied into the callee's frame on entry to enforce value semantics, which is dead work for a pure reader: a plain dot product `fn dot(a: Vec3, b: Vec3) -> i64` over a three-`i64`-field struct (the sum of the three field products) allocated a 48-byte frame, zero-filled it, copied both parameters in and rebound them — 53 of its 84 instructions were spent before the first instruction of the translated body. The same function now compiles frameless, to 27 instructions that read straight off `$a`/`$b`. A parameter keeps its copy in exactly two cases, which are the complete set of ways its region can be written in a language with no address-of, no reference type and no aliasing local binding: an assignment rooted at it (`p.x = 9`, `arr[0] = 9`, `p = @`, whole-binding reassignment — `Stmt::Assign` is the only write statement), or flowing to an `external fn` argument, whose foreign body shares the single linear memory and may store through the pointer it is handed. The gate needs no interprocedural analysis: every path from a caller's memory to a foreign store passes through some function whose parameter *is* a direct extern argument, and that function keeps its copy. Method receivers obey the same rule — a `mut self` that never assigns is now by reference too ([#220])
  - The decision is a body scan, not the `mut` marker. Keying on `mut` would have made removing an unnecessary `mut` a speedup, putting the annotation's cost in opposition to its purpose; the scan is also strictly more powerful, since it elides `mut` parameters that are never written. Neither lowering is observable from Inference source: a written parameter is copied into a region no other frame can name, so a write never lands where a by-reference parameter points, and `f(x, x)` behaves as before
  - Size, ray tracer, default Wasm 1.0 profile: 26 373 → 25 363 bytes pre-opt (−3.8 %), 19 359 → 18 491 shipped after `wasm-opt -Os` (−4.5 %). With `wasm-features = ["bulk-memory"]`: 15 496 → 14 976 pre-opt (−3.4 %), 9 541 → 9 171 shipped (−3.9 %). This is the mitigation [#315] forward-referenced for its own inline-lowering growth; it recovers 1 010 of the 10 877 pre-opt bytes that change cost, and applies at both feature levels because a frameless function emits neither the fill nor the copies. Golden corpus: 16 modules shrank and 4 grew, 29 989 → 28 454 bytes across the 20 that moved (−5.1 %); the four that grew (three `bulk_free` fixtures and `self_extern_escape/escape_with_param`) were deliberately amended to written parameters so they keep pinning the copy lowerings the elision would otherwise have removed
  - Semantics canary: the ray tracer renders bit-identically under both feature levels and both toolchains (`identity_sha256` unchanged at `ecee6669…`). Throughput on that workload is unchanged within noise, measured interleaved against the pre-change toolchain on a settled machine: +2.1 % 16-thread, +1.0 % single-thread, −1.7 % on the second scene, against an interleaved same-session noise floor of about 1 % and a cross-session spread of 2.6 % on byte-identical modules. The mixed signs are the expected shape — this workload's hot loops are scalar `i64` fixed-point arithmetic (its `fx` math lives in an ordinary source module flattened in by multi-file codegen; the program links no external and so never exercises the extern gate), and the frame traffic the elision does remove here — 5 of its 23 shipped functions go frameless, `__stack_pointer` writes 35 → 25 — is small beside the `i64` multiply/shift chains that dominate the render. The prologue zero-fill is untouched, so the whole-frame zero-init the Rocq model consumes still holds for free — a by-reference parameter contributes no frame bytes, so it falls outside that hypothesis rather than weakening it, and both `.v` goldens are byte-identical. A036's frame estimate deliberately still charges every compound parameter: the parity assertion is one-sided (`estimate >= real`), so elision only widens the margin, and teaching the analysis crate the same predicate would put a second implementation of it where no dependency edge can share the code
- An immutable `self` receiver forwarded to a compound-parameter `external fn` now copies the receiver into the callee's frame on entry, so the foreign body's writes land in the method's own copy and the caller's struct is unchanged. Previously the method handed the external a raw pointer into the caller's memory (linked modules share one linear memory, and an external may write through a caller-passed pointer), so an in-place-writing external observably mutated the caller's struct through an *immutable* receiver — a value-semantics violation for a program the type checker accepts. The gate is a type-blind body scan: a method whose body passes any `self`-rooted argument (`self`, `self.f`, `self.arr[i]`, parenthesized forms — in any statement or nested expression position, function-scoped `const` initializers included) to a bound `external fn` allocates the `self` frame slot and copies on entry, exactly as `mut self` always has; the copy is emitted iff the slot exists, so the two decisions cannot diverge. Methods that never forward `self` to an external stay frameless — zero existing golden artifacts changed. What is restored is **caller-side** value semantics: inside the method the external still writes through the method's own copy (the same behaviour a named compound parameter's entry copy provides), and a checked write-set contract on `external fn` parameters that would make foreign writes visible in source is deferred to [#333]. A036's frame estimate now charges every `self` receiver rather than only `mut self`, keeping it an upper bound on real frame sizes without re-deriving the escape condition in the analysis crate — with zero acceptance changes across the suite and the ray-tracer benchmark ([#329])
- Bulk-memory-free output by default: codegen no longer emits any bulk-memory instruction (`memory.fill` `0xFC 0x0B`, `memory.copy` `0xFC 0x0A`) unless a build opts in (see the `wasm-features` entry below), so every Compile-mode module is plain WebAssembly 1.0 (MVP + mutable-globals — the exported mutable `__stack_pointer`) and parses on MVP-only embedded interpreters (spacewasm, MVP-configured wasm3/WAMR); Proof-mode modules are Wasm 1.0 except the documented Inference non-det opcodes (`0xFC 0x31/0x32/0x3A–0x3D`). Bulk memory was the only post-MVP feature codegen emitted (no sign-extension, saturating-trunc, multi-value, or table/data-segment use), so its removal completes the profile ([#315])
  - Frame zero-fill lowers to `i64.store` zero stores: straight-line for frames ≤ 128 bytes (`BULK_UNROLL_LIMIT_BYTES`), a 16-bytes-per-iteration index loop above (frame sizes are 16-aligned, so the decomposition is exact). The stack-overflow "free trap" is preserved: both forms store at offset 0 first, and a wrapped stack pointer makes that first store trap out-of-bounds exactly where `memory.fill`'s up-front bounds check did — now defense-in-depth behind A035/A036, which reject overflow statically
  - Compound copies (array/struct params, sret returns, assignments, field writes) lower to forward 8-byte-chunk copies with a statically unrolled 4/2/1-byte tail, straight-line ≤ 128 bytes and an index loop above, alignment hint 0 (slots can be 1-aligned; hints are semantics-free). Forward order is safe because every emitted copy moves a whole value between equal-size regions that are identical or disjoint — the per-site argument is documented on the emit helpers. The ≤ 16-element per-element parameter-copy path is unchanged byte-for-byte
  - The three copy/loop scratch locals (`$dst`, `$src`, counter) are allocated lazily at first use and the local-declaration vector is now attached after the body is built (`Function::new([])` + `into_raw_body` splice), so a function that emits no copy or loop declares nothing — artifacts that carried no bulk op are byte-identical to before. The eager frame-pointer/bounds-check/narrow-div locals are untouched
  - Semantics canary: the ray-tracer benchmark renders bit-identically (`identity_sha256` unchanged vs the previous toolchain row) and `wasm-tools validate --features=-bulk-memory` accepts the output. Cost: inline lowering is larger than single bulk instructions — the ray tracer's module grew 15 496 → 26 373 bytes pre-opt (9 541 → 19 359 shipped after `wasm-opt -Os`) — and rendering throughput measured ~1–2 % lower in a same-conditions interleaved best-of-3 against the prior toolchain (single-thread showcase 624.5 → 613.9 ksps, single-thread final scene 267.9 → 262.6, 16-thread final 2724 → 2695). A shared-helper-function lowering could reclaim most of the size if it matters; frame-fill elision ([#220]) reduces how often the fill is emitted at all
  - Rocq translation is shape-compatible (the translator already handled `loop`/`br_if`/loads/stores; the in-repo `.v` goldens regenerate byte-identically), but downstream wasm-verifier proofs written against the old prologue shape (a single `BI_memory_fill`) now face straight-line stores or a loop and are tracked in that repository; the translator keeps its `BI_memory_fill`/`BI_memory_copy` arms for linked external modules
  - Statically linked *external* modules may still carry bulk-memory instructions (the linker's supported envelope is unchanged), so a linked artifact is Wasm 1.0 iff all its inputs are; a linker-side reject-or-lower policy is follow-up work
- Opt-in WebAssembly feature selection: `wasm-features = ["bulk-memory"]` under `[build]` in `Inference.toml` (forwarded as `infc --wasm-features bulk-memory`, which is also the direct/single-file escape hatch) lets a build targeting a full-featured runtime reclaim the single-instruction bulk-memory forms ([#315])
  - Feature names are kebab-case WebAssembly *proposal* names — the vocabulary every adjacent tool uses (`wasm-opt --enable-bulk-memory`, `rustc -C target-feature=+bulk-memory`, wasmparser feature bits). Instruction-level names are rejected with a teaching error (`memory.fill` "is an instruction, not a feature … write `bulk-memory`"): validators and runtimes enable whole proposals, and which bulk form appears at which site is a codegen decision. `mutable-globals` is likewise rejected as inherent — every module already exports the mutable `__stack_pointer`, which is why output validates at the WebAssembly 1.0 level rather than bare MVP
  - With `bulk-memory` enabled the compiler emits exactly what it emitted before the lowering existed — pinned by a standing golden family (`tests/test_data/codegen/wasm/bulk_memory/`, the 57 affected fixtures' pre-lowering artifacts vendored from history and reproduced byte-identically by the opt-in) plus inverse corpus gates: default goldens must contain no bulk operator, opt-in goldens must each contain one and validate at Wasm 1.0 + bulk-memory, and a total-cover check keeps every artifact under exactly one gate
  - The feature set applies identically in Compile and Proof mode, so the emitted `.v` always describes the same program as the shipped `.wasm`; flipping it invalidates existing proof artifacts, which is why the versioned manifest is the primary surface. Enabling it changes the prologue's proof-obligation shape from a store loop to `BI_memory_fill` — whether downstream proof libraries have lemmas for the builtin is unverified, so documentation stays neutral on which form proof users should prefer
  - The shared feature vocabulary lives in `core/compiler-interface` next to the ABI constants (minor bumped 1 → 2 for the new flag), so `infs` and `infc` cannot drift on names, diagnostics, or the version gate; the name→effect mapping in `infc` is an exhaustive match, so adding a vocabulary entry without wiring its codegen effect is a compile error. `Target::Soroban` rejects `bulk-memory` at the `codegen()` entry point (a library-level guard today — no CLI selects Soroban yet; its validator's acceptance is unverified, and reject-at-build beats fail-at-deploy)
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
- `wasm-to-v` translator: per-spec Rocq emission. Each spec with translated `hassert` obligations produces one `Definition <mod>__<SpecName>_hspec{k} : hassert` per obligation (source order) plus a gathering `Definition <mod>__<SpecName>_specs : list hassert`, and one `Theorem valid_<mod>__<SpecName> : ValidSpec <mod> <mod>__<SpecName>_specs.`; a spec with no free-function obligations (only methods, or an empty `spec { }`) renders `(@nil hassert)` so it type-checks regardless of scope state at the consumer site ([issue#21], [issue#22])
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
- A042 `NonDetOutsideSpec`: reject non-deterministic constructs — inline
  `forall`/`exists`/`assume`/`unique` statement blocks, the function-body-modifier
  form (`fn f() forall { … }`), and (transitively, via A006) `@` — used lexically
  outside a `spec { … }` declaration. Purely lexical (mode-independent), so it
  fires in both compile and proof modes; only the outermost offending block on
  each path is reported, since an inner non-det block nested inside an
  already-rejected outer one adds no new information

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
- Empty per-spec lists emit `(@nil hassert)` — not `[]%N`, and no longer `list N` at all — so the generated `Definition` type-checks regardless of whether a scope is active at the consumer's `Require` site. Downstream proof scripts matching `[]%N` or `(@nil N)` literally must update ([issue#21], [issue#22])
- Rewrite WASM-to-V translator for WasmCertCoq theory syntax ([#23])
- Add function name propagation to V output ([#24])

### Documentation

- New `core/wasm-to-v/ROCQ_CONTRACT.md` documenting the external Rocq predicates the generator depends on (`ValidModule` 1-arg, new `ValidSpec`), the emitted proof-skeleton shape, and the spec-map precedence rules (explicit vs embedded) ([issue#17])
- Rewrite `core/wasm-to-v/ROCQ_CONTRACT.md` for the wasm-verifier/vanilla-WasmCert target: the hassert-valued `ValidSpec` contract (superseding both the shape actually emitted before this change and the never-implemented shape this file previously documented), a full worked `.v` example, the spec-function omission and index-remap rule, `T_app` symbol resolution, both `inference.*` custom sections, the A042/`P0xx`/non-det-rejection language rules surfacing at `-v`, a migration section, and the source-to-`hassert` translation-scheme table. Rewrite `core/wasm-to-v/rocq-stub/README.md` for the two-namespace stub and `core/wasm-to-v/README.md`'s non-deterministic-instructions section for the new omit-and-derive translation
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

- Bump the test suite's `wasmtime` execution-harness dependency from 43.0.0 to 47.0.3, picking up the fix for [RUSTSEC-2026-0222](https://rustsec.org/advisories/RUSTSEC-2026-0222.html); wasmtime only executes compiled modules in tests, so no compiler output changes ([#335])
- Fix three golden-regeneration helpers that were stale against A042: `regenerate_const_in_forall_wasm`, `regenerate_struct_array_field_nondet_wasm`, and `regenerate_multidim_array_uzumaki_wasm` ran the analysis pass (`wasm_codegen`) while the golden tests they serve compile with `wasm_codegen_no_analysis`, so once A042 started rejecting non-det constructs outside `spec` the helpers crashed and those goldens could not be regenerated. Each helper now mirrors its test's pipeline. Eight sibling helpers with the same staleness (`if_nondet`, five `binops_*`, two `loops` non-det fixtures) are untouched — their goldens did not change here — and remain follow-up debt
- Add `tests/src/robustness/deep_syntax.rs`, a generated-source module covering deep and
  deeply chained input across seven shapes — flat operator chain, right-nested parentheses,
  prefix-unary nest, `else if` chain, block nest, nested array type, and a deep spec-function
  body. Its pipeline helpers all route through `inference::with_compiler_stack`, because
  cargo's test harness gives each test thread roughly 2 MiB and every one of these tests would
  otherwise overflow long before reaching the code under test. The two regressions pinned at
  issue scale are a 350-operand chain (the reported repro, asserted through analysis rather
  than merely parsing, since the type checker is the phase that aborted) and a 900-arm
  `else if` chain (the lowest known abort threshold), driven through code generation.
  Alongside them: `core/inference/tests/compiler_stack.rs` covers the helper's contract —
  return value, a closure borrowing its environment (the scoped-thread requirement the CLI
  call site depends on), a stack far larger than the caller's, and panic propagation with the
  original payload; `core/cli/tests/cli_integration.rs` proves both shapes exit 0 through the
  real binary, which distinguishes the fix from both the old abort and any diagnostic, since a
  stack-overflow abort is a signal kill rather than exit 1; and a parser unit test discards a
  500,000-level tree on a bare 2 MiB thread, deliberately not routed through the helper because
  the claim is that it needs no headroom ([#322])
- Fix a hot-spin in the LSP e2e test client's message waits ([#296])
  - `wait_for_response`/`wait_for_notification` looped over `recv_message`, which returns buffered messages before reading the wire. Once any non-matching message existed — most often a `publishDiagnostics` landing ahead of the awaited response — each iteration popped it from the buffer and pushed it straight back, so the loop never reached the wire again. The client burned 100% of a core indefinitely with no timeout, because `recv_timeout` was never called. This was misdiagnosed once as a server-side hang and forced a wire-direct `collect_responses` workaround in the cancellation tests
  - Both waits now scan the buffer once up front and then read straight off the wire, appending each non-matching message to the back of the buffer so arrival order survives for later waits. Each wait is bounded by a single deadline covering the whole wait rather than per read, and expiry panics naming what was awaited and how many messages it stepped over. A new `recv_from_wire` is the one place a wait turns a reader-thread item into a failure; the two drains keep interpreting the same items quietly, as collectors rather than waits. `recv_message`'s own semantics (buffer first, then the wire, one timeout per call) are unchanged — the `#294` shutdown-silence test depends on them
  - Seven harness self-tests pin the contract: stepping over buffered and over wire traffic on both the response and notification paths, retention order across the buffer/wire boundary, claiming an already-buffered target without a wire read, and giving up loudly when a target never comes. One of them runs the wait on its own thread under a watchdog, so a reintroduced spin fails red instead of hanging the suite. Determinism comes from seeding the buffer directly and from the server's own arrival-order contract, never from a sleep
- Add a `coqc` round-trip gate for proof-mode `wasm-to-v` output ([#231])
  - Every prior `wasm-to-v` test string-matched the emitted `.v` and never type-checked it, so a mis-aritied or renamed Rocq constructor (the [#230] `BI_forall`/`BI_exists` arity class) passed CI and failed only on the paid prover worker
  - New vendored signature stub `core/wasm-to-v/rocq-stub/` provides the logical library `Wasm` (`bytes`, `numerics`, `datatypes`, `verifier`) as signatures only — no semantics, no proofs — encoding each external declaration with the arity/shape the emitter writes, so a regression becomes a `coqc` type error
  - The stub declares only the operator surface reachable from Inference-generated wasm: integer families plus the non-deterministic instructions, with no floating-point (`f32`/`f64`, `T_f32`/`T_f64`, `VAL_float*`, `relop_f`/`binop_f`/`unop_f`), no `cvtop`, and no SIMD (`T_v128`) — Inference has no floats and its codegen emits no conversions, so an accidental emission of an unsupported operator fails the gate instead of silently type-checking; the translator's dead (and ill-typed) float arms that motivated [#284] are removed in the same release (see Breaking, above)
  - New gated test `tests/src/rocq_typecheck.rs` drives the in-process pipeline (parse → type-check → proof-mode codegen → `wasm_to_v`) over a corpus spanning the proof surface — inline and function-body-modifier `forall`/`exists`/`assume`, `unique`, `BI_call`, comparisons, `assert`, and `if`/`loop` control flow — then compiles each generated module against the stub; it rewrites the emitted `(* TODO *)` `Qed.` to `Admitted.` so it checks statements + definitions without requiring proofs to close
  - Two new corpus fixtures, `tests/test_data/inf/rocq_control_flow.inf` and `rocq_unique.inf`; existing spec fixtures are reused
  - The test is gated on `coqc` availability (`COQC` override, else `PATH`): it skips with a clear message when absent, and the new `.github/workflows/rocq-typecheck.yml` CI job installs Coq via apt so the gate is real on every PR. Wiring the full private WasmCert-Coq-Essence library into CI needs org secrets and remains a follow-up
- Replace the `rocq-stub` single `Wasm`-namespace stub with a two-namespace pair, `wasm/` (`Wasm.*`, vanilla WasmCert-Coq v2.2.0) and `wasm_verifier/` (`WasmVerifier.*`, the `hassert` assertion language and the `ValidModule`/`ValidSpec` predicates), retiring the WasmCert-Coq-Essence-fork target
  - `wasm/datatypes.v`'s `basic_instruction` inductive deliberately declares no `BI_forall`/`BI_exists`/`BI_assume`/`BI_unique`/`BI_uzumaki_num` constructors — their absence is itself the regression guard now that `spec` functions are omitted from the module record and non-det in a surviving body is a translate error
  - `tests/src/rocq_typecheck.rs` compiles both namespaces (`Wasm` before `WasmVerifier`, since the latter imports the former) and the corpus's required-construct needle set drops the `BI_*` non-det needles in favor of `ValidSpec`/`hassert`/`term_eq`/`Himpl`/`T_app`/`T_local`/`HA_ex`
  - New corpus fixture `tests/test_data/inf/rocq_spec_shapes.inf` exercises the `hassert` obligation shapes (an `assume` antecedent and an `if` guard as `Himpl`, a cross-call as `T_app`, a universal `@` as `T_local`, a `==` under `forall` as `term_eq`, a nested `exists` as `HA_ex`)
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

- `[build] wasm-features` in `Inference.toml`: opt into post-MVP WebAssembly proposals (initially `"bulk-memory"`) for the emitted artifact ([#315])
  - Validated at manifest load with teaching diagnostics: unknown names list the supported set; instruction spellings get a did-you-mean (`memory.fill` → `bulk-memory`); `mutable-globals` is rejected as inherent; duplicates and whitespace-padded entries are rejected, never trimmed. Validation, message wording, and the check order are shared with `infc --wasm-features` via `core/compiler-interface`, so the two front ends cannot diverge
  - Forwarded to `infc` only when the ABI handshake reports ≥ 1.2 (the `--out-dir` gating precedent); an older compiler is refused with remediation rather than silently handed — or silently denied — the request. Non-empty sets are echoed in build output (`wasm-features: bulk-memory`) so the resolved configuration is visible in logs
  - Honored in single-file mode too: `infs build src/main.inf` walks to the enclosing manifest exactly as `[wasm-dependencies]` resolution already does, so one project cannot silently emit modules at two different WebAssembly instruction levels depending on how the build was invoked
- `[build.wasm-opt]` no longer force-enables Binaryen's bulk-memory feature ([#315])
  - Codegen emits plain WebAssembly 1.0 by default, so `--enable-bulk-memory` is forwarded to `wasm-opt` only when the pre-optimization scan finds a bulk-memory operator in the artifact — which a statically merged external module or a `[build] wasm-features = ["bulk-memory"]` build can put there. The same scan verdict drives post-optimization re-validation: a bulk-free input is re-validated *without* bulk memory admitted, so a `wasm-opt` that introduced `memory.copy`/`memory.fill` fails the guard instead of shipping
  - The existing verification-construct pre-scan and the new bulk-memory probe are one artifact walk (`scan_artifact`), so the two facts cannot disagree about the same bytes
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

- A dynamic array index appearing **only** inside a function-scoped `const` initializer no longer aborts the compiler. `pub fn pick(i: u32) -> i32 { let arr: [i32; 4] = [1, 2, 3, 4]; const Q: i32 = arr[i]; return Q; }` panicked with `bounds-check scratch local must be reserved`; it now compiles and the index is guarded like any other. The scratch local the bounds-check guard needs is reserved from a scan of the function body, and that scan enumerated the statement positions holding expressions without listing `const` initializers — so a body whose only dynamic index sat there reserved nothing, and lowering then reached a guard site with no scratch to use. A `const` initializer lowers through the same path as a `let`, so it is now one of the positions every body-level expression scan looks at. Emitted bytes are unchanged for every program that already compiled ([#220])
- Exported functions now normalize narrow scalar parameters (`bool`, `i8`, `u8`, `i16`, `u16`) at entry. A WebAssembly host can pass any i32 bit pattern for a narrow parameter; previously the raw value flowed into the body (a host argument of `300` for a `u8` parameter compared as `300`, and a host `bool` of `2` behaved differently under `if b`, `b == true`, and `&&` pass-through). Each exported function (entry-file top-level `pub fn`, including `main`) now canonicalizes narrow parameters in its prologue: `u8`/`u16` take the argument's low bits, `i8`/`i16` sign-extend from the low bits (the C low-bits convention), and `bool` normalizes by truthiness — any nonzero host value is `true`, matching C hosts and the existing `if` lowering. In-language callers always pass canonical values and every normalization is a fixed point on them, so in-domain behavior is unchanged; non-exported functions are byte-identical. Enum-typed and compound parameters are not normalized: an enum tag has no bit-width truncation story, and tag-domain validation at the ABI boundary is deliberately left to follow-up work.
- The power operator `**` no longer crashes the compiler. `a ** b` was accepted by the parser and the type checker, then hit an unimplemented codegen path and aborted `infc` with a Rust panic (exit 101, backtrace). The type checker now rejects every use of `**` with ``the power operator `**` is not yet supported; compute the power with repeated multiplication in a loop`` (clean diagnostic, exit 1), in every expression position including operands whose types cannot be inferred. This also applies inside `spec` bodies, which previously compiled in plain compile mode only because specs are stripped from codegen there (and still panicked in proof mode `-v`).
- wasm-codegen: a narrow-typed scalar uzumaki (`@`) draw (`u8`, `i8`, `u16`, `i16`, `bool`, or a non-empty `enum`) now emits a domain-constraint sequence immediately after the draw, confining the drawn value to the declared type's value set instead of leaving it ranging over the full 32-bit draw. `u8`/`u16` get a bitmask (`and 0xFF`/`0xFFFF`); `i8`/`i16` get the sign-narrowing `shl`+`shr_s` shape that already normalizes sub-i32 arithmetic results; `bool` gets `and 1`; a non-empty `enum` gets `rem_u <variant count>` (tags are assigned by declaration position and are always contiguous from 0). A variantless enum is left unconstrained — it is uninhabited, and `rem_u 0` would trap. The same `bool`/`enum` constraint now also applies to compound (array/struct) uzumaki leaves before their store; narrow-int compound leaves needed no change, since the existing `store8`/`store16` truncation plus the sign/zero-extending typed load already realized their domain. `i32`/`u32`/`i64`/`u64` draws are no-ops (their value set already spans the full 32/64-bit width), and every other codegen path is byte-identical; only the `struct_nondet` golden was regenerated. The practical effect is on the Rocq side: a `forall`/`exists`/`unique` quantifier over a narrow-typed `@` now ranges over the declared type's domain instead of all 2^32 bit patterns ([#306])
- lsp: a diagnostic republish queued at `shutdown` is now **abandoned rather than flushed** — an observable behavior change. Previously the worker drained the deferred-republish queue on the shutdown path; the server now answers `shutdown` immediately and performs no republish drain, deferred bookkeeping, or routeback service until `exit`, so it sends no `publishDiagnostics` once shutting down (LSP 3.17 forbids notifications after `shutdown`, and a client that has shut down would never render them). When an open document was stale and pending as `shutdown` arrived, the old drain re-analyzed it under the cancellation the router fires ahead of `shutdown`: this delayed the shutdown response by seconds behind the doomed recompute and then published the stale document's diagnostics after `shutdown`. The server never actually hung — salsa resets its cancellation token on each unwind, so the drain's retry always completed; the "livelock" seen during earlier work was a client-side `wait_for_response` hot-spin triggered by that trailing publish landing ahead of the awaited shutdown response. Only notification-producing work is abandoned after `shutdown`, not responses: a pre-shutdown request that a pool read routed back and parked behind an in-flight sibling is still answered `ContentModified` (-32801) — at the shutdown flip and when a later routeback lands — rather than dropped with a dangling request id ([#294])
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
- compiler: the unreachable-file warning scan (`parse_project`) is now bounded and fails open. It recursively descended every directory under the source root; for a bare entry file whose parent is a home or filesystem-root directory that meant walking the whole disk on an otherwise-successful build, and `is_dir()` follows symlinks, so a symlink cycle in the tree could make the walk never terminate. The scan now stops after a fixed cap of directories (`MAX_SCAN_DIRECTORIES`), and a scan that gives up emits no unreachable-file warnings for that build — a partial file list cannot tell a genuine orphan from a file the scan never reached — while the parse itself completes exactly as before. Realistic projects sit far below the cap and are unaffected, and the resilient IDE walk never ran this scan, so interactive behavior is unchanged ([#288])
- lsp: a `didChange` for a document the client never opened is now dropped instead of silently adopted. Per LSP 3.17 a client sends `didChange` only between a document's `didOpen` and its `didClose`; `handlers::did_change` used to intern the path, install the change text as the overlay, track the URI, and publish diagnostics for it — enrolling a never-opened document in every future dependents-republish sweep. It now drops such a change (no interning, no tracking, no publish) and logs the URI to stderr, matching the URI layer's treat-unmappable-input-as-absent philosophy; a later proper `didOpen` starts tracking the document normally. The same rule now covers a change arriving after `didClose` (VS Code's preview-tab close race): it no longer silently resurrects tracking, and a still-open dependent is left untouched ([#275])

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

- The language server's `SERVER_STACK_SIZE` is now the larger of its historical 64 MiB
  (the rust-analyzer main-loop figure it mirrored) and `inference_parser::MIN_COMPILE_STACK`,
  with a compile-time assertion pinning the two together. All three spawn sites — router,
  read pool and analysis worker — reserve the larger amount. The deviation from the
  rust-analyzer number is deliberate: the server runs the same recursive phases the compiler
  does and owes them the same stack, and the reservation is address space with lazy commit,
  so the editor's resident memory still tracks the depth actually reached. Taking the larger
  of the two is what keeps them in step: were the server's figure allowed to sit below the
  front end's, a file at that limit would compile under the CLI while killing the editor
  process — a worse failure than the one the requirement exists to prevent. The compile-time
  assertion is a machine-checked restatement of that invariant rather than a gate; the `max`
  already makes it unfalsifiable ([#322])
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
[#288]: https://github.com/Inferara/inference/issues/288
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
[#292]: https://github.com/Inferara/inference/issues/292
[#294]: https://github.com/Inferara/inference/issues/294
[#157]: https://github.com/Inferara/inference/issues/157
[#256]: https://github.com/Inferara/inference/issues/256
[#254]: https://github.com/Inferara/inference/issues/254
[#275]: https://github.com/Inferara/inference/issues/275
[#306]: https://github.com/Inferara/inference/pull/306
[#296]: https://github.com/Inferara/inference/issues/296
[#219]: https://github.com/Inferara/inference/issues/219
[#220]: https://github.com/Inferara/inference/issues/220
[#315]: https://github.com/Inferara/inference/issues/315
[#322]: https://github.com/Inferara/inference/issues/322
[#329]: https://github.com/Inferara/inference/issues/329
[#332]: https://github.com/Inferara/inference/issues/332
[#333]: https://github.com/Inferara/inference/issues/333
[#335]: https://github.com/Inferara/inference/pull/335
