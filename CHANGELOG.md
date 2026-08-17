# Changelog

All notable changes to the Inference compiler project.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

### Breaking

- Proof mode now compiles `exists`- and `unique`-quantified spec functions end to end instead of rejecting them as **P001**. The body is *reachability-lowered*: compiled to vanilla WASM with one hidden trailing choice parameter per scalar `@` (name-section label `__choice{k}`; narrow-type domain normalization kept) and `assume`/`assert` as trap-on-false filters; the function is *retained* in the emitted `.v` module record — `forall`/plain spec functions stay omitted, and every executable reference to a retained function is rejected (it is the subject of a judgment, not a callable) — and its obligation is emitted as a `reachability_spec` record (`reach_func`/`reach_entry_arity`/`reach_visible_locs`/`reach_payload`) under a kind-selected theorem: `ValidExistsSpec` for `exists`, `ValidUniqueSpec` for `unique`, with the preamble import gaining ` Exists` only in reachability-bearing modules. The payload binds each `@` to its choice parameter's frame slot — no `HA_ex` binder, no `HA_has_type` guard: the judgment runs the retained body, so the frame supplies value and typing — while `_specs`/`ValidSpec` stays unconditional per spec name and forall-only output is byte-identical. The quantifier kind travels in `inference.hspecs` wire **v2** (per-entry kind byte plus reachability metadata; encode is always v2, decode rejects v1 — the section is proof-mode intermediate data, so recompile rather than migrate). **P001** narrows to standalone `assume` bodies, with a message explaining why an `assume` body states no property; new fatal diagnostics **P011** (any spec body calling an `exists`/`unique` spec function — the callee's compiled form carries hidden choice parameters no call site supplies) and **P012** (an anonymous `@` in a `unique` body — a choice nothing names cannot distinguish source-visible exit states; bind it first with `let c: i32 = @;`). Compound `@` in a reachability body keeps **P008** with a reachability-specific message; reachability bodies must be void and `return`-free (contract-forced: the downstream judgment reduces the retained body without an activation frame, so a `return` could never take a step), enforced by a codegen pre-scan even on analysis-skipping pipelines. `unique` compares source-visible exit states — entry parameters plus named `let x = @` choices, declared in `reach_visible_locs`; a `unique`-quantified *body* is a deliberate extension ahead of the language-spec amendment, while nested `unique` *blocks* stay **P002**. Reachability translation hard-depends on the WASM name section, and the obligations are dischargeable only for import-free modules (the always-link pipeline satisfies this). Also fixes the A002/A005 message article (`a 'exists' block` → `an 'exists' block`). Full contract in `core/wasm-to-v/ROCQ_CONTRACT.md` ([#354])
- New fatal proof-mode diagnostic **P010**: a spec function whose obligation collapses to the vacuous `HA_true` now aborts code generation instead of being emitted. Such an obligation is discharged by any proof without reading the program, so a green `Qed` over it carried zero verification content and no warning said so — `spec Caller { fn caller() -> i32 { return helper(); } }` emitted `Definition …_hspec1 : hassert := HA_true.` and the meaning the author wrote ("caller equals helper") was simply dropped. The check is the translated result, not the body shape: `HAssert::and`/`imp`/`or`/`ex` absorb `⊤`, so every vacuity path collapses to exactly `HA_true` and one equality catches them all — an empty or assert-free body, a body that only computes (`return`, pure `let`/`const`), a trailing `assume` (`Imp(p, ⊤) = ⊤`), and an `if` whose branches all vacuate, several of which look like they contribute. The message names what the body claimed and what to write instead. A spec function may still `return` — a body that also asserts is unaffected. Consequence: a helper that only computes can no longer live inside a `spec` block; move it to file scope, where a spec function still applies it as a `T_app`. A plain spec *method* keeps its helper exemption because it produces no obligation either way, so the same helper wrapped in a spec-inner struct still compiles — but a plain method that *states* a property — one carrying an `assert` at any depth, a non-deterministic block that asserts nothing excluded — now raises **P009** rather than being dropped with no output at all, which is the widening that closes the worst silent path in the area. Unaffected: compile mode, which has no obligations; every spec function that already stated a property, whose obligation is byte-identical. Migration: give the spec function an `assert`, or move it out of the `spec` block ([#356])
- Proof-mode obligations now encode a term-position `&&`/`||` as an `HA_ex`-bound witness pinned by a two-armed constraint — `Hor (nz l ∧ v = 1) (eqz l ∧ … ∧ v = r)` for `||`, dually for `&&` — instead of an eager `T_binop … BOI_and`/`BOI_or`. The eager term demanded that both operands denote, which neither the source language nor code generation does: `&&`/`||` short-circuit, and `lower_short_circuit_binary` emits a valued `if` precisely so a skipped right operand cannot trap. wasm-verifier's `term_denote` is strict in both `T_binop` operands, so `assert((x == 0 || 10 / x == 10 / x) == true);` — true for every `i32`, and returning normally under Wasmtime — emitted an obligation that is provably false at `x = 0`, where the division does not denote. Every term position was affected, not just a comparison operand: a pure `let` of a `bool`, a `const` initializer, an `if` condition, a call argument. The witness constraint mirrors the compiled control flow exactly, and a constraint belonging to a right operand is planted in the arm that evaluates it rather than hoisted to the statement's atom, so nesting a short-circuit inside another one does not re-introduce the eager demand one level up. This also removes a divergence the eager shape had independently of partiality: a `bool` slot is guarded only `HA_has_type … T_i32` and so may hold any `i32`, where the program computes `1 && 2` as `2` but the eager term computed `1 & 2` as `0`. Unaffected: `&&`/`||` in assertion position already lowered to `HA_and`/`Hor` faithfully and are byte-identical, as is every obligation containing no term-position `&&`/`||` — both committed `.v` goldens are unchanged. One accepted-program narrowing comes with it: a boolean chain used to live in the term tree's own depth budget, which restarts at each assertion atom, and now costs assertion-spine levels instead — two per operator written left-associatively (`a && b && c`), four when parenthesised to the right. A chain of roughly 250 operators used to compile; the limits are now about 100 left-associated and 63 right-nested, past which the existing `CodegenError::HspecTreeTooDeep` rejects the spec by name rather than truncating it. Downstream proofs over a previously emitted term-position `&&`/`||` must be restated against the witness shape ([#383])
- Argument labels at a call site are now checked instead of discarded: mixing named and positional arguments in one call fails as `MixedNamedAndPositionalArguments`, a label naming no parameter of the callee fails as `UnknownArgumentLabel`, and a label naming a parameter declared at another position fails as `ArgumentLabelOutOfOrder`. Previously the label was dropped after parsing — the type checker paired argument `i` with parameter `i` and never read the label, and code generation destructured it away — so all three compiled silently and bound positionally. Against `fn subtract(left: i32, right: i32)`, `subtract(left: 10, 3)` was accepted although the language requires that when any argument is named all of them must be named; `subtract(wrong: 10, nonexistent: 3)` was accepted with labels that name nothing; and `subtract(right: 3, left: 10)` returned `-7` where the labelled reading of the source says `7`, with the emitted Rocq `.v` encoding the same positional binding, so a proof discharged over it was a proof about a program the source does not name. Labels select nothing — they are checked for agreement with the declaration and are still lowered positionally, so a labelled call is rejected rather than reordered. Unaffected: an unlabelled call, and a fully labelled call whose labels match the parameters in declaration order, both compile to byte-identical WASM; a parameter that binds no name (`_: i32`, or a bare type as in `external fn sub(i32, i32)`) may still be passed positionally; struct literals bind by name and may still be written in any field order. Migration: spell each label as the parameter it names, in declaration order, or drop the labels ([#378])
- A struct method whose `self`/`mut self` receiver is declared anywhere but the first parameter now fails type checking as `SelfReferenceNotFirstParameter`, naming the method and pointing at the receiver to move. Previously this compiled silently: every instance call pushes the receiver ahead of the written arguments, but the callee's WASM parameters kept source declaration order, so a receiver declared later bound to whatever value the corresponding argument slot carried while a user argument bound to the receiver pointer instead — a type confusion no diagnostic ever caught. Depending on what collided, the mis-lowered method returned a silently wrong result, trapped out-of-bounds when the mis-bound integer was large enough to be read as an address, or — when the swapped values differed in width (e.g. an `i64` field) — emitted a module that fails WebAssembly validation while `infc` still exited 0; the emitted Rocq `.v` artifact faithfully encoded the same broken ABI, so a proof built over it would be a proof about the mis-lowered program. Unaffected: a leading `self`/`mut self` still compiles to byte-identical WASM; standalone functions and `external fn` already rejected a `self` parameter in any position; `fn f(self, self)` is reported as a duplicate parameter rather than a misplaced receiver. Migration: move `self` (or `mut self`) to the front of the parameter list ([#377])
- A parameter name declared more than once in one function-like declaration now fails type checking as `DuplicateParameterName`, naming the repeated parameter and the declaration it repeats in and pointing at the repeat. It replaces `RegistrationFailed { kind: Variable }`, which reported the collision as a scope-registration failure in the symbol table's own wording, and it covers declarations that reporting never reached: an `external fn` binds its parameters into no scope, so `external fn e(x: i32, x: i32) -> i32;` previously type-checked, generated a module and exited 0. A repeated `self` receiver participates under the name `self`. One repeat now yields exactly one diagnostic instead of one per pass, and each further repeat of the same name is reported at its own declaration. Code generation is unchanged: its two parameter-collision assertions stay as backstops and now name a diagnostic the frontend actually has. Unaffected: `_: T` and bare positional types bind no name and may still repeat; a body `let`/`const` that reuses a parameter name is a different mistake and keeps its own report. Migration: rename or remove the repeated parameter ([#377])
- Proof-mode obligations now guard every universal slot with an explicit `HA_has_type (T_local i) T_i32`/`T_i64` antecedent (fused into a following `assume`'s antecedent when present), and `!=` no longer conjoins per-side `HA_defined` — aligning emitted hspecs with wasm-verifier's strictified `ValidSpec`, under which the old unguarded shapes were undischargeable (and, pre-hardening, vacuously provable). Downstream proofs over previously emitted obligation trees must be restated against the guarded shapes; wasm-verifier's `PrimeExample.v`/`with_spec.v` already prove them ([#353])
- `wasm-to-v` now fails closed on every construct outside the wasm-verifier proof contract, refusing each as `WasmToVError::UnsupportedFeature` naming the construct: all floating-point and SIMD instructions, every conversion naming a float on either side, `f32`/`f64`/`v128` as a value type in any position, and the unlowered proposal families. (The integer-to-integer width conversions were also refused here at the time; [#363] restored them.) Only foreign bytes via external linking (`infc -L` / `--wasm-dep` / `INFERENCE_WASM_LIB_PATH`) or `translate_bytes` are affected — they now fail at `infc` instead of at `coqc` or the prover; every `.v` the Inference corpus produces is unchanged ([#284])
- `core/wasm-linker` admits the eight integer width-changing operators an external `.wasm` compiled by a real toolchain actually contains: the three integer-to-integer width conversions (`i32.wrap_i64`, `i64.extend_i32_s`, `i64.extend_i32_u`) and the five sign-extension operators (`i32.extend8_s`, `i32.extend16_s`, `i64.extend8_s`, `i64.extend16_s`, `i64.extend32_s`). **This supersedes the two [#284] entries above and below it**, which retracted these same operators and stated the opposite; they are listed under Breaking, and this reversal is recorded alongside them rather than as an unrelated addition. Admitting a previously-rejected module is otherwise additive for linker consumers — what makes this entry breaking in its own right is two contract surfaces that change value: the public `SUPPORTED_WASM_FEATURES` const gains `SIGN_EXTENSION`, so an embedder asserting on it must update; and the vendored proof stub's `basic_instruction` gains a constructor (`BI_cvtop`) while `unop` gains `Unop_extend`, so any downstream Rocq development that matches exhaustively over either inductive stops compiling until it handles the new arm. All eight had been retracted under [#284] on the premise that the Rocq translator had no lowering for them; the translator now lowers each, so the "an allow-listed operator is one the translator can render" contract holds at the Rocq level rather than only at the Rust one. The two halves were refused in different places and are lifted in lockstep: the width conversions are MVP instructions gated only by the allow-list, while sign-extension is a post-MVP proposal the validator refused before any body was scanned, so `WasmFeatures::SIGN_EXTENSION` joins `SUPPORTED_WASM_FEATURES`. `SATURATING_FLOAT_TO_INT` deliberately stays out — its operands are floats, the proof contract declares no float number type, and admitting it would recreate exactly the allow-listed-but-unlowerable divergence this gate exists to close. Unaffected: Inference codegen emits none of the eight (it narrows sub-`i32` values with shifts and masks), so no `.wasm` or `.v` this compiler produces changes; only an external `.wasm` that previously failed to link is affected, and it now links ([#363])
- `inference_wasm_codegen::codegen()` now takes its configuration as one value: `codegen(typed_context, target, mode, opt_level, module_name, features)` → `codegen(typed_context, module_name, CodegenOptions { target, mode, opt_level, features })`; no behavior or emitted-byte change. Migration: wrap the four arguments in the struct or use `CodegenOptions::default()`; the two-argument `inference::codegen` wrapper is unchanged ([#315])
- `Inference.toml` is now strictly verified: every struct table (root, `[package]`, `[build]`, `[build.wasm-opt]`, `[verification]`, `[wasm-dependencies]` entries) rejects unknown keys instead of ignoring them silently; map-shaped tables whose keys are user data still accept arbitrary keys. Migration: fix typos and misplaced keys, and delete the removed `manifest_version`/`edition` keys from `[package]` ([#315])
- `inference_wasm_codegen::codegen()` gains a sixth parameter `features: EmitFeatures`; `EmitFeatures::default()` keeps output inside WebAssembly 1.0. Migration: five-argument embedders pass `EmitFeatures::default()`; the two-argument `inference::codegen` wrapper defaults it ([#315])
- Signed narrow division now traps on overflow: i8 `-128 / -1` and i16 `-32768 / -1` trap (`unreachable`) instead of silently wrapping to MIN, making division-overflow trapping uniform across all signed widths; `MIN % -1` stays `0` and add/sub/mul still wrap
- Exported functions now trap on an out-of-range enum tag: every enum-typed parameter of an exported function gets a prologue guard rejecting any host tag >= the variant count (a variantless-enum parameter traps on every host call); in-language callers never fire the guard
- Shift counts are now taken modulo the operand type's bit width: narrow (`i8`/`u8`/`i16`/`u16`) shifts mask the count (`& 7` / `& 15`) before the wasm shift, so `u8 x << 8` yields `x`; `i32`/`u32`/`i64`/`u64` shifts are byte-for-byte unchanged
- New analysis rule **A046** (`SpacedNegativeLiteral`, error) requires a unary minus applied to a numeric literal to be written glued to the digits: `- 42` and every other separated spelling (extra spaces, a newline, a comment in the gap) are rejected, in every position. `- 128` was never a literal — it is a negation of the bare `128`, which A022 measured on its own, so the same value compiled or failed on a space (`- 100` was accepted at `i8`, `- 128` was not, and every signed minimum was unreachable in that spelling). A022 now skips exactly the literals A046 claims, so it can no longer report a magnitude the author did not write; nothing is silently accepted, since every skipped literal is rejected by A046. `- x`, `- g()`, `-(128)`, `~ 5` and binary subtraction are unaffected. Migration: close the gap — write `-42` ([#376])
- New analysis rule **A045** (`FieldLessStructValue`, error) rejects values of a field-less struct — literals, `let`/`const` types, parameters, return types, struct fields, `self` receivers, through array nesting — fixing a compiler abort on `E { }` struct-literal lowering; the declaration and the `E::helper()` namespace idiom stay legal, and A011 is unchanged. Migration: give the struct a field, or drop `self` and keep it as a pure namespace ([#332])
- New analysis rule **A044** (`ShiftCountOutOfRange`, error) rejects a literal shift count that is negative or >= the operand type's bit width (e.g. `x << 32` on `i32`); const-declared counts are out of scope, as for the division-by-zero check and A022
- `inference-parser`'s re-exported `Input::new` now takes the source text alongside the tokens: `Input::new(&tokens)` → `Input::new(src, &tokens)`, so grammar rules can read a token's spelling. Migration: an external embedder constructing an `Input` must pass the string it lexed ([#219])
- `inference_wasm_codegen::CodegenOutput::spec_func_indices: Vec<u32>` → `spec_func_indices_by_spec: FxHashMap<String, Vec<u32>>`; the accessor renames to `spec_func_indices_by_spec()`. Migration: replace `Vec::new()` with `FxHashMap::default()` and `.spec_func_indices()` with `.spec_func_indices_by_spec()` ([issue#21])
- `inference::wasm_to_v` / `translate_bytes`: third parameter changed from `spec_func_indices: &[u32]` to `spec_funcs_by_spec: &FxHashMap<String, Vec<u32>>`. Migration: pass an `FxHashMap` (`FxHashMap::default()` for the empty case) ([issue#21])
- `inference::wasm_to_v` / `translate_bytes` gains a fourth parameter `hspecs_by_spec: &inference_hassert::HSpecMap`: pass `HSpecMap::default()` to source obligations from the embedded `inference.hspecs` custom section, or a populated map to override it; `CodegenOutput` gains a matching `hspecs()` accessor (empty in compile mode)
- Rocq output targets wasm-verifier on vanilla WasmCert-Coq v2.2.0, replacing the WasmCert-Coq-Essence fork: `ValidModule` is 1-ary and always emitted, the new `ValidSpec : module -> list hassert -> Prop` carries each spec's obligation as a translated `hassert` formula, and `spec` function bodies are omitted from the module record with indices remapped. Migration: downstream Rocq libraries must define the hassert-valued `ValidSpec` and update `ValidModule` consumers; theorems are `valid_<mod>` and `valid_<mod>__<SpecName>` (see `core/wasm-to-v/ROCQ_CONTRACT.md`) ([issue#17], [issue#21])
- New leaf crate `core/hassert` (`inference-hassert`): the `HAssert`/`HTerm` verification-obligation IR, smart constructors with `HA_true`-identity simplification, and the single codec for the `inference.hspecs` custom section (version 1, LEB128, hardened decoder), shared by `wasm-codegen`, `wasm-linker`, and `wasm-to-v`
- New fatal proof-mode diagnostics `P001`–`P009` (`core/wasm-codegen/src/hassert/`): a spec function that cannot be translated to a `hassert` obligation now aborts code generation (`CodegenError::UntranslatableSpec`) instead of silently emitting an unverifiable module; every diagnostic in a spec is collected before failing
- `wasm-to-v` rejects any non-deterministic instruction (`forall`/`exists`/`assume`/`unique`/uzumaki) in a surviving executable function body as `WasmToVError::UnsupportedFeature` — unreachable from Inference-compiled code, defense-in-depth for foreign `.wasm`; retires the vendored stub's `BI_unique` typecheck debt
- Lower `assert(<bool>)` to a WASM trap-on-false (`<cond>; i32.eqz; if; unreachable; end`) in both compile and proof modes, replacing a codegen panic; new golden fixture `tests/test_data/codegen/wasm/base/assert/` with wasmtime trap-identity coverage ([#195])
- WASM custom section name for the per-spec function index map is now `inference.spec_funcs`; external tools reading `metadata.code.inference.spec_funcs` (a misuse of the tool-conventions reserved namespace) must update ([issue#16])
- `inference.spec_funcs` payload now starts with a `varuint32` version byte (`1`); anyone parsing the section directly must update and reject unsupported versions ([issue#16])
- New analysis rule **A042** (`NonDetOutsideSpec`, error) rejects non-deterministic blocks (`forall`/`exists`/`assume`/`unique`, inline or as function-body modifier) outside a `spec` declaration, in both compile and proof modes. Migration: move the specification logic into a function inside a `spec`
- `&&` and `||` now short-circuit: the right operand is evaluated only when the left does not decide the result, so guard idioms like `x != 0 && 100 / x > 1` no longer trap, and a skipped right operand's traps and side effects no longer occur; lowered as a valued `if (result i32)` block, with new golden fixture `tests/test_data/codegen/wasm/short_circuit/`

### Changed

- `core/wasm-linker` adopts an external's declared linear memory only when the external's closure actually uses memory. Previously every merged external's memory section was folded into the reconciliation unconditionally, so a module's declared page count became a fact about the merged output even when no merged body could observe it. The effect was twofold and both halves were wrong in the same way: against a memoryless main, a leaf `i32.add` over a 17-page `wasm32-unknown-unknown` artifact produced a merged module declaring 17 pages — recorded in the emitted `.wasm` and restated in the paired `.v` as the machine the proof is about, introduced by a pure function; and against the compiler's own `(memory 1 1)`, that same pure function was rejected outright with `IncompatibleMemory` over pages nothing would have touched. This **partly supersedes the [#363] tier entry above**, which stated that such an artifact clears the tier gate and then fails reconciliation: that remains true for a memory-*using* external and is no longer true for a pure one, which now links against a stock compiler main with main's single page kept as-is. Dropping the declaration is unobservable because `uses_memory` is closure-scoped, so when it is false no merged body contains a load, a store, `memory.copy`/`fill`/`init`, or `memory.size`/`memory.grow` — the last two being the ones worth naming, since they yield or extend a page count rather than addressing a byte and so read as unrelated to a memory's limits when they are in fact the operators that observe them most directly. Two guards deliberately stay outside the new gate: an unsupported memory *shape* (`memory64`, `shared`, a custom page size) is still rejected for every declared external memory, adopted or not, so the rejection stays absolute rather than becoming conditional on an effect flag; and a closure that uses memory when no module declares one is still rejected unchanged. Unaffected: the reconciler's refusal to relax a main module's pinned maximum is untouched, so a memory-using external over a multi-page module still fails against `(memory 1 1)` — configurable linear memory is a separate change ([#363])
- `core/wasm-linker` classifies an external's globals and tables by what its closure *uses*, not by what its module *declares*: the `!module.globals.is_empty()` and `!module.tables.is_empty()` disjuncts are gone from the Tier-C gate, leaving `effects.uses_globals` and `element_count > 0 || effects.uses_tables`. This is what makes a real `cargo build --target wasm32-unknown-unknown` artifact classifiable at all — every one carries an lld-synthesized `__stack_pointer` mutable global, and a `std` build also carries an empty `(table 1 1 funcref)` with no element segment and no `call_indirect`, pure linker boilerplate a leaf integer function never touches, which nonetheless rejected the whole link as `RequiresRelocatableBuild` over a declaration nothing read. It is **not sufficient on its own**: such an artifact declares a multi-page linear memory (16 pages is the usual lld default), and the merge's memory reconciliation never relaxes the anchor module's bound, so it now clears the tier gate and fails one step later with `IncompatibleMemory` ("the reconciled minimum (16 pages) exceeds the declared maximum (1 pages) of the memory it is merged into") against the compiler's fixed one-page `(memory 1 1)`. Configurable linear memory is a separate change. Dropping an admitted external's global or table from the merged output is sound because `ClosureEffects` is closure-scoped (accumulated over the bodies reachable from the root export), so a closure admitted with no global or table effect contains no operator naming either index space — which is what the relaxation depends on, since the rewrite pass remaps function and type indices only and the merge emits the main module's globals with no table section, so a surviving `global.get` would silently rebind to a main-module global and still pass post-merge validation. **Data segments deliberately stay declaration-gated**: an *active* segment writes linear memory at instantiation whether or not any instruction names it, so an unreferenced one still changes what the merged program observes (a *passive* segment would be inert on the same argument, but the parser retains only `data_count` and discards each segment's kind, so the two cannot be told apart). Still Tier C, unchanged: reading or writing a global, `call_indirect` and the other table operators, an element segment, a declared data segment, and `memory.init`/`data.drop`. The Tier-C reason strings change with the gate they describe, so each names what actually fired: "defines or accesses module globals" becomes "reads or writes module globals" (defining one is no longer a signal), and the single "uses a table / element segment (indirect calls)" splits into "performs an indirect call or otherwise names the table space" and "declares an element segment" — neither implies the other, and the shared string used to name a construct the module might not have. An embedder matching on the message text must update. Unaffected: Inference codegen emits no external modules, so no `.wasm` or `.v` this compiler produces changes; only an external `.wasm` that previously failed to link is affected ([#363])
- Integer literals are contextually typed: a literal takes the type of the position it appears in (annotated `let`/`const`, assignments, struct fields, array elements, call arguments, `return`, literal-built operands) instead of always `i32`; no coercion is introduced and emitted WASM for previously-accepted programs is byte-identical. Also collapses duplicate mismatch diagnostics, adds parser diagnostics for `16i64`/`1_000`/`0x1F` literal shapes, and A022 now names where a literal's type came from ([#219])
- Rebuild the type checker's scope tree as an index arena (`Vec<Scope>` keyed by `ScopeId(u32)`, no `Arc<RefCell>`), making `TypedContext` `Send + Sync`; behavior-preserving and golden-corpus byte-identical ([#157])
- Memoize `ide-db`'s per-entry analyses with Salsa 0.27 (a single tracked `analyze_entry` query) replacing the hand-rolled `FxHashMap`/generation-counter memo; the `Vfs` overlay stays outside Salsa storage and behavior is identical ([#157])
- `ide/ide`'s `Analysis` query surface now takes `&self` instead of `&mut self` (`AnalysisHost` wraps `RootDatabase` in a `RefCell`); write methods stay `&mut self` and behavior is identical ([#157])
- Cancel in-flight LSP analyses: a `didChange` supersedes older in-flight requests (`ContentModified`), `$/cancelRequest` answers `RequestCanceled`, and a cancelled analysis keeps the analysis cache; `apps/lsp` gains a router/worker split with strictly serial dispatch ([#157])
- Route `ide-db`'s analysis invalidation through Salsa dependency edges (per-file `FileStamp` inputs plus a conditional `AvailabilityEpoch`), reducing the write-path pass to editor-facing staleness bookkeeping; behavior-identical ([#157])
- Actually free evicted and closed entries' memoized analyses in `ide-db` via an evicted-flag sentinel swap, bounding resident analyses by open documents + `MAX_UNOPENED_ANALYSES` + a one-write-lagged transient ([#157], [#247])
- Serve read-only LSP feature requests (hover, definition, completion, documentSymbol, inlayHint) off the analysis worker on a two-thread read pool using per-request Salsa `Storage` snapshots; a superseding write answers `ContentModified` and quiesces live snapshots ([#292])
- Extract the shared project front end (import-closure walk, `FileLoader` seam, manifest source-root discovery) into new leaf crate `inference-project-model` (`core/project-model`); `core/inference` re-exports everything unchanged, and the IDE/LSP stack no longer links the WASM/Rocq backend ([#256])
- Drop the always-empty `ResilientProjectParse::warnings` field; the fail-fast `parse_project` keeps reporting `ProjectParse::warnings` ([#256])
- Document `RootDatabase`'s single-threaded, read-through-`&mut self` query model on `RootDatabase` and `ide/ide`'s `Analysis` ([#157], [#256])
- Declare `serde_json` in `[workspace.dependencies]` and inherit it in `apps/lsp`, `apps/infs`, and `tests` ([#256])
- codegen: the three function-body statement-descent passes now consult one block-classification helper (`nested_blocks`), so they can never disagree about which sub-blocks exist; emitted WASM is byte-identical ([#167])
- codegen: `Compiler::current_fn_name` mutable ambient state replaced by an explicit `fn_name: &str` parameter threaded through the statement walker to `lower_sret_return`; emitted WASM is byte-identical ([#172])

### Language

- File-based module hierarchy (Zig-style, no `mod` keyword): every `.inf` file under `src/` is an implicit namespace; `use a::b;` imports `src/a/b.inf`, `pub` gates cross-file visibility, `pub use` re-exports, and only the entry file's top-level `pub fn`s become WASM exports ([#63])
- `external fn` + `use { … } from <module>`: declare and call functions from external `.wasm` libraries via logical (platform-independent) module references; a separate link step (`inference-wasm-linker`) produces a single self-contained `.wasm` and `.v` ([#9])
- Add struct definition and parsing support ([#14])
- Add division operator (`/`) support ([#86])
- Add unary negation (`-`) and bitwise NOT (`~`) operators ([#86])
- Parse visibility modifiers (`pub`) for functions, structs, enums, constants, and type aliases ([#86])

### Compiler

- hassert: a `const` initializer inside an `exists` block no longer leaves its existential binder unwrapped — `const c: i32 = f(@);` aborted the compile in a debug build (`logical variable level 0 is not bound at depth 0`) and, in a release build, emitted a de Bruijn index naming no binder, which the codec and the printer both passed through into the `.v`. `const` now scopes its binders over the rest of the block exactly as a pure `let` does, an expression translated only for its diagnostics drops the binders it introduced instead of handing them to a later statement, and an out-of-scope level is now a hard failure rather than a `debug_assert!` compiled out of release ([#383])
- wasm-to-v: `translate_basic_operator` is now `todo!()`-free — all 285 unimplemented arms became grouped rejection arms, so an operator with no lowering yields a diagnostic instead of aborting the process ([#284])
- infc: the `UnsupportedFeature` message now explains the construct falls outside the subset the WasmCert proof model describes rather than being unfinished work, and points to building without `-v` ([#284])
- Compiler phases now run on an explicitly sized stack: `inference_parser::MIN_COMPILE_STACK` (128 MiB) states the requirement, new `inference::with_compiler_stack` provides it, and `infc` uses it, so deeply nested input no longer aborts ([#322])
- parser: `SyntaxNode`'s drop is now iterative, so discarding a deeply nested CST no longer overflows the stack; the derived `Clone`/`PartialEq`/`Debug` remain recursive ([#322])
- wasm-linker: new `core/wasm-linker` crate (`inference-wasm-linker`): `link(main_wasm, &[external_wasm])` statically merges satisfied imports' closures, rewrites index spaces, dedups function types, preserves the `name` section, and emits one unified binary ([#9])
- wasm-linker: reject external modules using floating-point (Inference has no float types and the Rocq translator models none), naming the exact opcode; sign-extension and saturating float-to-int are also removed from the supported set ([#9])
- wasm-linker: reject tail calls (`return_call`/`return_call_indirect`) and segment-indexed table ops (`table.init`/`elem.drop`/`table.copy`) with `UnsupportedConstruct` — the Rocq translator has no lowering for them ([#9])
- wasm-linker: main-module rebuild is fail-closed — start functions, non-function imports, table sections, and v128 value types are rejected with `UnsupportedConstruct` instead of silently dropped or mis-merged ([#9])
- wasm-linker: fix unsound Tier-B provenance rule — pointer subtraction preserves parameter-derivation only for `Param - Const`, closing a fabricated-absolute-address hole; also enforce the 256-level nesting cap and reject duplicate `inference.spec_funcs` sections, multi-memory mains, and trailing `spec_funcs` bytes ([#9])
- wasm-linker: merged external function names are module-prefixed in the name section (`mathlib.sum`, nameless fallback `mathlib.func_<idx>`), collision-free across modules; the Rocq translator sanitizes `.` to `_` ([#9])
- wasm-codegen: emit a WASM import section for `external fn` declarations — imports take the lowest function indices, local functions shift up by `N`, and extern calls lower to plain `call` ([#9])
- type-checker: `ExternOrigin { logical_module, export_field }` binds each `external fn` to its source module; `extern_origins()` on `SymbolTable` collects them for codegen ([#9])
- ast: Remove dead `OperatorKind::BitNot` variant — `~x` always parses as `UnaryOperatorKind::BitNot`; the binary variant was never produced by the AST builder ([#142])
- parser: Replace the `tree-sitter` + `tree-sitter-inference` front end with a resilient recursive-descent parser (new `inference-parser` crate, `core/parser`) producing byte-identical ASTs, collecting all syntax errors, and dropping the C toolchain requirement; `parse_external_module` moves to `inference::extern_prelude` ([#62])
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

- The linear memory layout is now a value on `CodegenOptions` rather than two constants: `MemoryLayout { pages, stack_size }` is the single source for the memory section (`minimum == maximum`, so the memory is fixed rather than growable), the `__stack_pointer` initializer, and the per-frame size assertion. `validate` rejects a layout that cannot describe a real memory, including one whose memory and stack together span more than the 32-bit address space — the headroom a wrapped stack pointer needs to land out of bounds and trap. No user-facing knob yet; the default (one page, all stack) leaves every golden artifact byte-identical ([#363])

- Compound (struct/array) parameters the callee provably never writes are now passed by reference — no frame slot, entry copy, or (when nothing else needs memory) frame at all; a copy remains only for written parameters or those passed to an `external fn`, recovering part of the size growth from [#315] ([#220])
- An immutable `self` receiver forwarded to a compound-parameter `external fn` now copies into the method's frame on entry, so foreign writes no longer mutate the caller's struct through an immutable receiver; a checked write-set contract on `external fn` parameters is deferred to [#333] ([#329])
- Bulk-memory-free output by default: codegen emits no `memory.fill`/`memory.copy` unless a build opts in, so Compile-mode modules are plain WebAssembly 1.0 (+ mutable-globals); frame zero-fill and compound copies lower to inline stores/loops, and artifacts that carried no bulk op stay byte-identical ([#315])
- Opt-in WebAssembly feature selection: `wasm-features = ["bulk-memory"]` under `[build]` in `Inference.toml` (or `infc --wasm-features bulk-memory`) restores the single-instruction bulk forms byte-identically; the shared vocabulary lives in `core/compiler-interface` (ABI minor 1 → 2) ([#315])
- Multi-file codegen: flatten the import-reachable file closure into one WASM module; the file-qualified `FnKey` (new `inference-fn-key` crate) keeps same-named functions distinct, structs lay out per defining file, only entry-file `pub fn`s export, and single-file output stays byte-identical ([#63])
- Fixed: multi-dimensional scalar array literal initialization (`[[i32; 3]; 2]`) no longer panics — new recursive `store_array_literal_elements` stores each scalar leaf at its computed offset; single-dimensional output is byte-identical
- Fixed: nested array-of-structs literal initialization (`[[Pt; 2]; 2]`) no longer panics — `store_array_literal_elements` gains a struct-leaf arm reusing the single-dimensional AoS machinery; single-dimensional AoS output is byte-identical
- Runtime bounds checks for dynamic array indices: `emit_index_offset` guards reads and writes with an unsigned length compare trapping via `unreachable` in all Compile-mode builds; Proof-mode proof obligations tracked as [#212] ([#164])
- `FunctionOrigin { TopLevel, SpecInner }` threaded through `visit_function_definition`; spec-inner functions can no longer be WASM-exported even when `pub` ([issue#19])
- Per-spec function-index map (`spec_func_indices_by_spec`) replaces the single union list; spec-inner functions key as `"<SpecName>.<fn>"` so two specs may share function names, `name` section stays unmangled ([issue#21])
- Emit `inference.spec_funcs` WASM custom section in `proof` mode carrying the per-spec index map, making bare `.wasm` binaries self-describing; omitted in `compile` mode so binaries stay byte-identical ([issue#16])
- wasm-to-v: new `errors.rs` with `WasmToVError` thiserror enum (`InvalidRocqIdentifier`, `RocqStdlibShadow`, `EmbeddedSpecMismatch`, `WasmParse`) and `InvalidIdentifierReason` sub-enum ([issue#20])
- wasm-to-v: `validate_rocq_identifier` rejects Rocq-illegal module/spec names (bad leading char, invalid chars, length > 255, stdlib shadow, reserved keyword) before `Definition <name>` emission ([issue#20])
- wasm-to-v: per-spec Rocq emission — one `Definition <mod>__<SpecName>_hspec{k}` per obligation plus a gathering `_specs` list and a `Theorem valid_<mod>__<SpecName> : ValidSpec …` per spec; obligation-free specs render `(@nil hassert)` ([issue#21], [issue#22])
- Switch from LLVM to direct WebAssembly emission via `wasm-encoder`: all LLVM dependencies removed (`inkwell`, `inf-llc`, `rust-lld`), non-det instructions emitted as custom `0xfc`-prefix opcodes, all `pub` functions exported (reactor model, no `_start`) ([#125])
- Add compilation architecture with `CodegenOutput` boundary: `codegen()` returns WASM bytes plus metadata, with new `Target` (Wasm32/Soroban), `CompilationMode` (Compile/Proof), and `OptLevel` (O0–O3/Os/Oz) enums ([issue#97], [#125])
- Add per-function optimization strategy for proof mode (Decision #32): spec functions compile unoptimized to preserve structural correspondence for Rocq, execution functions use release optimization; `OptLevel` is metadata only for now ([issue#97])
- Add validation guards in `codegen()`: reject proof mode with non-Wasm32 targets, reject Soroban with non-det operations ([issue#97])
- Upgrade shadowing detection from `debug_assert!` to `assert!` in `pre_scan_locals` — fires in release builds for parameter, constant, and variable name collisions in `locals_map`
- Add `Statement::Loop` body recursion to `pre_scan_locals()` — locals inside loop bodies are pre-registered before instruction emission
- Add loop and break statement lowering: `loop` emits `block`+`loop` with `br_if` exit and `br 0` back-edge, `break` emits `br <depth>`, and `LoopContext` tracks block depth across loop/if/non-det nesting ([#152])
- Replace silent `if let ArgumentType::Argument` skip with exhaustive `match` covering `SelfReference`, `IgnoreArgument`, and `Type` variants, each with an explicit `todo!()`
- Add fixed-size array support with linear memory allocation: `__stack_pointer` shadow stack with stack-first downward-growing layout, literal/index/uzumaki lowering, copy-on-entry value semantics, sret array returns, 16-byte-aligned zero-filled frames ([#148])
- Add struct type support with linear memory allocation: C-style field layout with natural alignment, literal and member-access lowering, copy-on-entry parameters, sret returns, `memory.copy` value-semantics copies, struct uzumaki ([pr#159])
- Add struct method codegen: methods compile as top-level WASM functions with mangled `TypeName.method_name` names, `self` as an i32 pointer (immutable `self` zero-copy, `mut self` copy-on-entry), instance and associated calls unified via `ResolvedCallee` ([pr#178])
- Add enum type codegen: unit variants lower as i32 constants with declaration-order zero-based tags, usable in all value positions, with `==`/`!=` comparisons and uzumaki support ([pr#187])
- Add nested compound type codegen (struct-in-struct, array-in-struct, struct-in-array): recursive `type_byte_size()`, `CompoundFieldLayout` sub-layout caching, pointer-chained access loading at the terminal scalar; one nesting level enforced by analysis rule A026 ([pr#185])
- Add per-element zero-store elision in array and struct literal codegen: stores of zero-valued elements are skipped during frame-local initialization — the prologue `memory.fill 0` already zeroed the frame ([#188])
- Eliminate dead trailing epilogue in non-void functions — each `return` emits its own epilogue and analysis rule A007 guarantees a return on every path, shrinking every non-void function with a frame ([#188])
- Add assignment statement lowering: `mut` support in AST and type-checker (`AssignToImmutable`), identifier and array-index targets, mutable function parameters, and literal type propagation in assignments ([#146])
- Add conditional statement lowering: `if`/`else` lowers to WASM structured control flow, `pre_scan_locals` recurses into both arms, and non-void functions get a trailing `unreachable` safety net ([#144])
- Add binary and unary expression lowering: all arithmetic, comparison, logical, bitwise, and shift operators for i32/i64 with sign-sensitive dispatch; `-x` via `0 - x`, `!x` via `i32.eqz`, `~x` via `x ^ -1`; `**` deferred ([#140])
- Add function parameter lowering and function call support: parameters map to WASM locals `0..n`, a pre-scan `func_name_to_idx` map enables forward references, and expression-statement calls `Drop` unused return values ([#136])
- Add local variable lowering (`let` bindings): `local.set`/`local.get` for literal, identifier, and uzumaki initializers across all numeric types and bool; `ConstantDefinition` now shares the `lower_literal` helper ([pr#135])
- Add LLVM-based WASM code generation using `inf-llc` ([#44])
- Add custom LLVM intrinsics for non-deterministic instructions ([#44])
- Implement `forall`, `exists`, `uzumaki`, `assume`, `unique` block codegen ([#44])
- Add `rust-lld` linker invocation for WASM linking ([#44])
- Add mutable globals support in WASM compilation ([#44])
- Add base WASM code generation from typed AST ([#29])

### Analysis

- A036's shadow-stack budget now travels in a new `AnalysisOptions`, threaded through `Rule::check` to every rule, replacing a `STACK_BUDGET_BYTES` constant that had to be hand-synced with codegen's stack size and could not be checked from either crate. `analyze` keeps its signature and means "default layout"; `analyze_with_options` is the entry point a configured build must use. A cross-crate test now asserts the two defaults agree ([#363])

- Whole-program call graph for the module hierarchy, keyed on the structured `FnKey` from `inference-fn-key`: A035 (recursion) and A036 (stack depth) now span files, catching cross-file recursion and >64 KB cross-file stack chains ([#63])
- Restore the duplicate-`FnKey` debug tripwire in `resolve_adjacency` dropped by the LSP server rewrite ([#239]); it now exempts parse-recovered keys (`is_parse_recovered`), and keep-first behavior is unchanged in every build ([#255])
- Add `core/analysis/` crate with rule-based static analysis between type checking and codegen: five rules (A001–A005), a `Rule` trait with `rule!` macro, a shared AST walker, and a three-severity model ([#156])
- Expand the analysis pass from 5 to 22 rules, migrating 13 checks from the type checker (now type-correctness only): new A006–A011 and A023–A024, migrated A012–A019 and A022
- Add 5 analysis rules for nested compound type constraints: A026 `NestedCompoundDepth`, A027 `UzumakiOnNestedStruct`, A028 `UzumakiOnStructInArray`, A029 `CompoundLiteralMemberAssign`, A031 `UnsupportedCompoundReturnExpr` ([pr#185])
- A033 `CombinedUnaryOperators`: reject adjacent prefix unary operators such as `--x`, `~~x`, `!!x`, and parenthesized variants like `-(~x)` (issues [#82], [#81]; PRs [#111], [#117])
- A035 `RecursionDetected`: reject all direct and mutual/indirect recursion (Power of 10, Rule 1) via a conservative whole-program call graph; recursive codegen fixtures rewritten to iterative form ([#205])
- A036 `StackDepthExceeded`: reject programs whose cumulative shadow-stack usage along a call chain exceeds the 64 KB budget, replacing the opaque runtime `memory.fill` trap with a compile-time error; a parity test keeps the estimate ≥ codegen's real frame size ([#166])
- A037 `ArrayIndexConstOutOfBounds`: reject a constant array index that is negative or `>= length`; dynamic indices remain runtime-guarded in Compile-mode builds ([#164])
- A038 `UzumakiOnCompoundField`: reject uzumaki (@) on a struct- or array-typed struct-literal field (e.g. `Outer { i: @ }`), which slipped past A027 and panicked proof-mode codegen ([#225])
- A039 `StructUzumakiAsArgument`: reject a struct-typed uzumaki (@) passed directly as a function argument (`f(@)`); the array case was already A014, the struct case panicked codegen ([#225])
- A040 `UzumakiOnCompoundArrayElement`: reject a struct- or array-typed uzumaki (@) element of an array literal (scalar `@` elements are supported); distinct from A028's whole-array `@` ([#225])
- A041 `DuplicateLocalName`: reject duplicate function-local names across disjoint sibling blocks with a two-location diagnostic instead of panicking in codegen ([#217])
- A042 `NonDetOutsideSpec`: reject non-deterministic constructs (`forall`/`exists`/`assume`/`unique` blocks, the function-body-modifier form, and via A006 `@`) used lexically outside a `spec { … }` declaration; only the outermost offending block on each path is reported

### AST

- Migrate the AST arena from `FxHashMap<u32, AstNode>` + `Rc<T>` + `RefCell<T>` to typed `Arena<T>` via vendored la-arena: typed indices (`ExprId`, `StmtId`, `DefId`, …) prevent cross-category ID misuse, storage is `Send + Sync` cache-friendly `Vec<T>` ([#156])

### CLI

- Add `infc --out-dir <path>` flag to redirect compilation artifacts (default `out/` unchanged); compiler ABI minor version bumped 0 → 1 to advertise the additive flag ([#223])
- `infc -v` (and `infs build -v`) now implies `--mode proof` when no explicit `--mode` is passed; pass `--mode compile -v` explicitly for the prior stripped-specs behavior ([issue#22])
- Add `infc --mode proof` and `infs build --mode proof` to enable Rocq translation output; default remains `compile` mode with stripped specs ([issue#22])
- `infc` now surfaces `WasmToVError::RocqStdlibShadow` and `WasmToVError::InvalidRocqIdentifier` with dedicated user-facing messages ([issue#20])
- Running `infc` or `infs build` without phase flags now performs full compilation and writes `out/<name>.wasm`, matching conventional compiler UX ([#138])
- Add `BuildProfile` (Debug/Release) with `resolve_opt_level()` for target-aware optimization ([issue#97])
- Remove external toolchain dependencies: no `inf-llc`, `rust-lld`, or platform-specific library paths required ([#125])
- Defer WASM compilation until output files are actually needed (`-o` or `-v` flags) ([issue#97])
- Refactor CLI architecture with improved argument handling ([#28])

### Rocq Translation

- `wasm-to-v` now lowers the eight integer width-changing operators instead of refusing them, and the vendored `coqc` stub grows the three declarations they need. The five sign-extension operators emit `BI_unop t (Unop_extend n)` — the proof contract classifies sign-extension as a **unop**, beside `clz`/`ctz`/`popcnt`, not as a conversion, so the WASM mnemonics group them misleadingly and that misgrouping is why they were retracted alongside the conversion block under [#284]. The three integer-to-integer conversions emit `BI_cvtop`: `BI_cvtop T_i32 CVO_wrap T_i64 None`, `BI_cvtop T_i64 CVO_extend T_i32 (Some SX_S)`, and the `SX_U` variant — the only three instances the backend's `cvtop_valid` admits. The stub gains `Unop_extend : N -> unop`, a `cvtop` inductive, and `BI_cvtop` appended to `basic_instruction`; `cvtop` declares only `CVO_wrap` and `CVO_extend`, because the backend's six remaining constructors each need a float number type the stub deliberately does not declare, and a stub narrower than the backend keeps an accidental emission an unbound-constructor error rather than a silently type-checking term. `Unop_extend`'s argument is the source width in **bits**, not bytes: the backend's `unop_type_agree` ignores the argument entirely while its `app_unop` divides by eight, so a byte count elaborates, satisfies the typing side condition, and denotes the constant zero for every input — a provable-but-false obligation no `coqc` gate can see, pinned instead by byte comparison against `Unop_extend 8%N`/`16%N`/`32%N` in `tests/src/rocq_typecheck.rs` and `core/wasm-to-v/src/lib.rs`. Still refused: every float instruction, every conversion naming a float (`trunc`, `trunc_sat`, `convert`, `demote`, `promote`, `reinterpret`), and the float/vector value types; the rejection message now says why (no float number type) rather than claiming the contract covers no conversion at all. Unaffected: Inference codegen emits none of the eight, so every committed `.v` golden and every corpus output is byte-identical, and the new terms reach the `.v` only from foreign or statically-linked `.wasm` — a hand-assembled module added to the `coqc` gate is what elaborates them ([#363])
- Specifications can now state aggregate, element-wise and alternating-quantifier properties directly, instead of being rejected outright. A compound `@`, a compound parameter, an array or struct literal, and a copy of one are translated to a shape-preserving tree of *scalar leaves*: one universal slot with its own `HA_has_type` guard per leaf (one `HA_ex` binder per leaf in an existential context), enumerated arrays row-major and struct fields in layout order, allocated parameters-first-in-declaration-order then each `@` in binding order. So `let a: [i32; 3] = @; assert(a[0] <= a[0]);` produces a real obligation where it was **P008**, `a[0]` and `p.x` resolve against that tree at translation time where they were **P002**, and aggregate `==`/`!=` in assertion position is the leafwise conjunction (or its De Morgan dual) — `==` compares values, and an aggregate's value is exactly its ordered leaves. The supported surface is deliberately equal to the executable aggregate `@` surface, since proof mode lowers spec bodies through the same unrolling: arrays of scalars at any rank, and structs whose fields are scalars or one-dimensional scalar arrays. Arrays of structs (**A028**) and structs with struct or multidimensional-array fields (**A027**) stay rejected on every specification path — `@`, parameters and literals alike — so neither surface is wider than the other. An index the translation cannot fold binds a witness pinned by the unsigned range bound `i <u N` **first**, then one implication per element, which is what makes bounded iteration expressible: a `forall` binding an array and an index, assuming a bound on every element and a range for the index, and asserting the bound at that index emits a real obligation that discharges against the verifier (`tests/test_data/inf/spec_bounded_iteration.inf`). Out of range that definition is unsatisfiable and the enclosing atom is refuted — `a[i]` denotes *the element at index `i`, which exists*, a definedness rule rather than a mirror of any runtime check, since proof mode emits no bounds check at all; the alternative, a guarded implication leaving an out-of-range read vacuously satisfied, is exactly what **P010** rejects elsewhere. Constant steps of a chain descend first, so `m[1][j]` splits over the selected row, while two non-constant steps in one chain are **P002**. A `forall` block inside an `exists`/`assume` block of a `forall`/plain spec function now emits a real universal binder — new IR node `HAssert::All`, printed as wasm-verifier's derived `Hall`, carried on new `inference.hspecs` assert tag `0x0B` (section version stays **2**: the change is additive, an older decoder fails loudly with `UnknownHassertTag(0x0B)`, the linker decodes hspecs only from the main module, and recompilation rather than migration is the compatibility story) and declared in the vendored `coqc` stub. A free slot there would have been quantified by `ValidSpec`'s outer universal, turning `∃k. ∀x. P` into `∀x. ∃k. P` silently — which is why **P007** is lifted only for that nesting and kept inside `exists`/`unique`-quantified bodies, where every `@` is a hidden choice parameter the judgment quantifies operationally. Two new fatal diagnostics come with the encoding: **P013**, a per-spec-function running budget of 64 quantified scalar leaves (each leaf costs a binder level and a guard level, and the levels accumulate across every introduction in the function, so a per-introduction cap would not bound the shared assertion-depth budget; checked from the declared type before any leaf is materialized), and **P014**, a constant-*folded* out-of-bounds index (`const K: i32 = 5; a[K]`, `a[1 + 4]`), stating the same fact **A037** states for a direct-literal index at the spellings A037's pattern cannot see. One usability trap is inherent to the definedness rule and is documented rather than diagnosed: the emitted range bound is unsigned, so a *signed* index needs both `0 <= i` and `i < N` before its element denotes at all — supplying only the upper bound compiles clean and yields an obligation that is false, surfacing later as an unprovable Rocq goal. The two bounds are necessary, not sufficient: they make the element exist, while a claim about its *value* still needs hypotheses about the element's value, since a compound `@` states only the typing of its leaves. A `u32` index needs no lower bound and yields a simpler obligation. Unaffected: every committed `.v` golden is byte-identical, as is every proof-mode `.wasm` (the pass is read-only over the typed AST), and compile mode has no obligations; no previously emitted obligation shape changes, so no downstream proof needs restating. Migration: none — programs that compiled still compile, and programs that were rejected may now translate ([#355])
- A data byte with no `byte_scope` notation now reaches the emitted `.v` as `(encode 18%Zst)` instead of `(encode 18%Z)`, and a module carrying a data segment claims that key in its preamble with a second conditional line — `Local Delimit Scope Z_scope with Zst.`, next to the `Open Scope byte_scope.` it already emitted. `%Zst` is a private delimiting key for the same standard `Z_scope` the literal always lived in (a Rocq scope may carry several keys), so the term denotes exactly what it denoted before. It is claimed because the `Z` key is not reliably `Z_scope`'s: mathcomp's algebra library delimits its own `int_scope` with it (`ssrint.v`, alongside a `Number Notation` on `int`), so in any file whose `Import`/`Export` chain applies that `Delimit` the argument of `(encode 18%Z)` re-reads as mathcomp's `int` and stops elaborating against `encode : Z -> byte`. Two files can be that file — the emitted `.v` itself, should the backend build behind its preamble ever re-export mathcomp algebra, and a consumer that imports it and restates emitted data bytes in its own text. Delimiting is last-writer-wins — whichever `Delimit` the chain applies last decides the key; an explicit `%Z` is read through the key regardless of which scopes are open, so `Open Scope Z_scope.` recovers nothing, and nothing a consumer writes downstream repairs a file that failed to compile on its own. Re-delimiting is the one measure that works, and taking a private key rather than taking `Z` back leaves mathcomp's own `%Z` intact for anything read alongside the module, while `Local` keeps the key from leaking to consumers that import the file. Unaffected: the 244 byte values that have a notation keep their `#78` spelling, and every module without a data segment is byte-identical, preamble included — Inference codegen emits no data segment, so no committed `.v` golden or corpus output moves and only foreign or statically-linked `.wasm` reaches the changed spelling. A downstream file that restates emitted data bytes in its own text must carry the same `Local Delimit` line ([#416])
- The emitted `Ma` memarg helper in every Rocq `.v` file now binds `ofs al` instead of `of al` (`Definition Ma ofs al := {|memarg_offset := ofs; memarg_align := al|}.`): `of` is an ordinary identifier in vanilla Rocq but a keyword under ssreflect, so a consumer importing mathcomp ahead of the emitted definitions hit a parse error on that preamble line before reaching anything the file states. `Ma` is always applied positionally (`Ma 0%N 2%N`), so no call site or downstream proof changes, and every emitted `.v` differs by exactly this one line ([#412])
- `wasm-to-v` no longer drops a table import's element type. `translate_module_import_desc`'s `TypeRef::Table` arm applied the contract's `MID_table : table_type -> module_import_desc` to a bare `limits` record instead of the two-field `table_type` (`{|tt_limits; tt_elem_type|}`) the constructor actually takes — the shape looked right because its neighbour `MID_mem` really does take a bare `limits`, since the contract's `memory_type` *is* `limits`. `coqc` rejects the result outright (`The term "{| lim_min := …; lim_max := … |}" has type "limits" while it is expected to have type "table_type"`), so any table import produced a `.v` that does not type-check. No fixture in the `coqc` gate had ever imported a table — Inference codegen does emit an import for every `extern fn`, but the static-merge linker (`inference-wasm-linker`) is fail-closed on imports (`LinkError::UnsatisfiedImport`), and `infc` runs that link, aborting on failure, before `-v` translation starts, so no import from the normal pipeline ever reaches the translator — so the arm was reachable only from a foreign or hand-assembled `.wasm` fed to `wasm_to_v` directly, and the defect went uncaught until the [#401] coverage work's `module_surface` fixture became the first gated module to import one. `MID_table` is now applied to `{|tt_limits := <limits>; tt_elem_type := <reference_type>|}`, pinned by both a `funcref` and an explicit-max `externref` table import in a new unit test (`an_imported_table_carries_its_element_type`, `core/wasm-to-v/src/lib.rs`) and by the `module_surface` gate fixture. Unaffected: every other import descriptor, and every committed corpus `.v` golden, since none of Inference codegen's own imports reach `wasm_to_v` — the fail-closed linker resolves them, or the build aborts, before translation runs ([#402])
- Element segments, data segments, and `br_table` now emit terms the proof backend accepts, and the vendored `coqc` stub now mirrors the part of that backend's interface a data segment needs. The backend is wasm-verifier, whose WASM datatypes come from its `coq-wasm` (>= 2.2.0) dependency; three emitted terms had no constructor there. An element segment's function indexes were written as an `ME_functions` element *mode* into `modelem_init`, the field that holds initializer expressions — an outright `coqc` error, so any module carrying a function-index element segment produced an unusable `.v`. The other two type-checked only because the vendored stub had drifted along with the emitter — the false green its own README warns about — declaring `ME_declared` where the dependency has `ME_declarative`, and a one-argument `BI_br_table` that silently dropped the default label, making the emitted instruction a different instruction from the module's; that one was a hard error only for a table with no explicit targets, which emitted a bare, partially-applied `BI_br_table`. Now `BI_br_table` carries the label vector *and* the default (`BI_br_table (0%N :: 1%N :: nil) 1%N`, and `BI_br_table nil 0%N` for `br_table 0`), function-index element items desugar to the `BI_ref_func` initializer expressions `modelem_init` is typed for, `ME_declarative` takes the drifted spelling's place, and a `ref.func` operand is renumbered past omitted spec functions like every other function index, so both element item forms reach the same index instead of only the shorthand one. A data byte keeps its `#78` spelling, which is the dependency's own `byte_scope` notation; the defect there was on the stub side, which declared no such notation and so rejected in-repo what the backend accepts. The stub now mirrors `byte_scope`, the exported `encode : Z -> byte` its notations abbreviate, and the 244 of 256 two-digit uppercase notations the dependency's hand-written block actually declares — it skips `#12` .. `#19` and `#1C` .. `#1F`, so those twelve now reach the `.v` as the `encode` application the notation would have abbreviated (`(encode 18%Z)`) rather than as syntax that parses nowhere. Mirroring all 256 instead would have made the gate accept a module the backend rejects, the same false green in the other direction. The stub deliberately omits the module-level `Open Scope byte_scope.` the dependency leaves open for importers, so a module carrying at least one data segment emits that line itself rather than depending on an `Import` chain to supply it, and the gate proves it does. Modules with no data segment emit no such line and are byte-identical. All of these constructs are unreachable from Inference codegen and arrive only from foreign or statically-linked `.wasm`, so no committed `.v` golden or corpus output moves; a handcrafted foreign module added to the `coqc` gate covers them ([#346])
- The emitted `.v` now spells every index immediate with an explicit `%N` scope. Six sites in the `wasm-to-v` translation wrote structurally identical operands bare while their siblings wrote them scoped: `BI_br`, the `BI_br_table` label list elements, both operands of `BI_call_indirect`, `BI_memory_init`, `BI_data_drop`, and the `ME_functions` element-segment function index list (`BI_br 0` / `BI_br_table (0 :: 1 :: nil)` / `BI_call_indirect 0 0` / `BI_memory_init 0` / `BI_data_drop 0` / `ME_functions 0::nil` become `BI_br 0%N` / `BI_br_table (0%N :: 1%N :: nil)` / `BI_call_indirect 0%N 0%N` / `BI_memory_init 0%N` / `BI_data_drop 0%N` / `ME_functions 0%N::nil`). The proof contract types all of these as `N`, and Rocq's numeral notation is type-directed, so the bare spellings elaborated correctly and `coqc` accepted them — this fixes no bug today. It removes a latent one: the bare sites were correct only by scope inference, so a future contract or notation change that made the `N` scope non-inferable would break exactly and only them, and at the prover rather than in-repo. Spelling-only; no semantic change. Emitted bytes change for any module containing `br`, `br_table`, `call_indirect`, `memory.init`, `data.drop`, or a function-index element segment — of the committed `.v` goldens only `tests/test_data/rocq/rocq_prime_example.v` is affected (one line, from its `loop`), regenerated in this change. Everything but `br` is unreachable from Inference codegen, which emits none of those constructs; they reach the translator only from foreign or statically-linked `.wasm`, so no Inference program's output changes beyond the `BI_br` spelling ([#344])
- Negative integer constants now reach the emitted `.v` parenthesized — `BI_const_num (Vi32 (-1))`, not `BI_const_num (Vi32 -1)`. Gallina's `-` is an infix operator, so the old spelling parsed as the subtraction `Vi32 - 1` and `coqc` rejected the whole module ("The term `Vi32` has type `Z -> value_num` while it is expected to have type `nat`"), making proof mode unusable for any program containing a negative constant anywhere — an offset, a threshold, an error code. Every signed width is affected, since `i8`/`i16`/`i32` all lower to `i32.const` and `i64` to `i64.const`; so is source that writes no minus sign at all, because a `u32`/`u64` literal above the signed maximum is stored as a negative constant of that width. The `hassert` obligation printer already parenthesized its constants, so obligation terms are unchanged, as is every non-negative constant — no existing `.v` golden moves ([#314])
- WASM module-name subsection now reflects the CLI-supplied input file stem instead of the hardcoded `"output"`. The Rocq translator reads this back, so the emitted `Definition <mod>__<Spec>_specs` and `Theorem valid_<mod>` identifiers now use the source filename. Multi-module workflows that previously collided on a single `output` identifier now produce distinct ones
- Empty per-spec lists emit `(@nil hassert)` — not `[]%N`, and no longer `list N` at all — so the generated `Definition` type-checks regardless of whether a scope is active at the consumer's `Require` site. Downstream proof scripts matching `[]%N` or `(@nil N)` literally must update ([issue#21], [issue#22])
- Rewrite WASM-to-V translator for WasmCertCoq theory syntax ([#23])
- Add function name propagation to V output ([#24])

### Documentation

- New `core/wasm-codegen/docs/specification-obligations.md` for readers of an emitted `.v` — usually someone whose proof did not close: the two kinds of obligation and the asymmetry between them, aggregates as ordered scalar leaves with the enumeration and allocation rules that fix every `T_local` index in a goal, one fully expanded three-leaf obligation, the definedness reading of `a[i]` beside the both-bounds requirement a signed index carries, quantifier alternation, the caps, and a table of every kept rejection with its reason. `core/wasm-to-v/ROCQ_CONTRACT.md` gains the matching contract rows (aggregate introduction, aggregate copy, constant and non-constant access, leafwise aggregate comparison) and its **P001**–**P014** registry is brought current ([#355])
- Proof-mode diagnostics reworded now that the aggregate encoding makes several of them inaccurate. **P004**'s tail said "only bool, integer, and enum values can" at every site, which read as a rule the language no longer has — the parameter position accepts `[i32; 3]` and rejects `[Point; 2]`, and the reason is the shape rather than aggregation — so all its sites now name the representable surface exactly, and an aggregate read *whole* where a term is required (an aggregate call argument, most often) gets its own wording instead, since the shared one would have rejected `[i32; 2]` while listing arrays of integers as nameable. **P003**'s "is not supported" read as a schedule for a decision that is permanent, and now states the rule: a specification names values, not storage. `loop` is lifted out of the shared no-encoding template — the constructs sharing it have nothing to be rewritten *as*, while a loop's purpose in a specification is exactly what quantifying an index and constraining it says directly — and its message names that idiom. **P007**, **P008** and **P004** in an `exists`/`unique` body now name the quantifier that makes the construct impossible, because the identical declaration translates in a `forall` body, and they point at the `forall`-bodied alternative. Every message naming a quantifier takes the article that word is spoken with, which also corrects **P011**'s "an `unique`-quantified" ([#355])
- New `core/wasm-to-v/ROCQ_CONTRACT.md` documenting the external Rocq predicates (`ValidModule` 1-arg, new `ValidSpec`), the emitted proof-skeleton shape, and spec-map precedence rules ([issue#17])
- Rewrite `core/wasm-to-v/ROCQ_CONTRACT.md` for the wasm-verifier/vanilla-WasmCert target (hassert-valued `ValidSpec`, worked `.v` example, migration section); also rewrite `rocq-stub/README.md` and the `core/wasm-to-v/README.md` non-det section
- Add compilation targets matrix documentation (`book/compilation_targets.md`): Compile/Proof x Debug/Release x with/without non-det operations ([issue#97])
- Add `unreachable` emission rationale document (`book/unreachable-emission-in-codegen.md`) ([#144])
- Add arithmetic overflow in WASM codegen deep-dive (`book/arithmetic-overflow-in-wasm-codegen.md`) ([#146])
- Add a "mathcomp consumers" note to `core/wasm-to-v/ROCQ_CONTRACT.md`: emitted `.v` spells `N` literals with the `%N` scope key, which mathcomp's `ssrnat` rebinds to `nat_scope`, so a consumer that imports mathcomp ahead of the emitted definitions must re-delimit with `Local Delimit Scope N_scope with N.` after its mathcomp imports (`Local` because a file-global `Delimit` leaks to files that import the consumer); `%num` cannot be emitted instead without breaking the mathcomp-free standalone contract and the repo's `coqc` gate ([#413])

### Type Checker

- Cross-file name resolution and file-based visibility for the module hierarchy: canonical file-qualified type identity, one `same_file` visibility chokepoint, per-call `CallTarget` records, and `CircularDefinition` detection for cross-file value cycles ([#63])
- Reject spec-inner functions whose bare name shadows a top-level function (`SpecFunctionShadowsTopLevel`); codegen and the type checker previously disagreed silently on which callee was invoked
- Reject same-named structs or enums across spec blocks at registration time (`RegistrationFailed`) instead of silently using the first-registered layout; functions remain mangleable across specs
- Spec blocks now open a real symbol-table scope via `enter_spec`, so two specs may declare same-named members without colliding ([issue#18])
- Remove `flatten_defs_with_spec_inner`; the three phases that used it recurse into `Def::Spec` inline, opening the spec scope around the inner work ([issue#18])
- `TypedContext::lookup_struct`/`lookup_enum` now search all scopes (`lookup_struct_anywhere`/`lookup_enum_anywhere`) so post-type-check phases can resolve spec-inner types ([issue#18])
- Add `resolve_custom_type()` to `SymbolTable`, resolving `TypeInfoKind::Custom(name)` to `Struct`/`Enum` at registration time and recursing into array element types ([#148])
- Add argument type validation at all function, method, and associated-function call sites ([#148])
- Add i64 array element type propagation from `[i64; N]` annotations to number literals in array initializers ([#148])
- Add array element and struct field assignment mutability checks (`arr[i] = value` and `p.x = 42` require `mut`), with `extract_root_variable_name` resolving the root identifier ([#148], [pr#159])
- Add `VariableShadowed` error: shadowing a name from an outer scope is a hard error, aligning with MISRA C Rule 5.3 and NASA Power of 10 ([pr#159])
- Add `ArrayReturnCallInExpressionPosition` error: sret calls permitted only in `let x = foo()` and `return foo()`, guarded at 6 sites ([#148])
- Add struct literal field validation: `MissingStructField`, `UnknownStructField`, `DuplicateStructField`, and field value type mismatches ([pr#159])
- Add `MethodNeverAccessesSelf` error: methods declaring `self` but never using it ([pr#159])
- Add `EmptyStruct` error: reject struct definitions with no fields or methods ([pr#159])
- Add `StructLiteralAsArgument` error: reject struct literals as direct function arguments ([pr#159])
- Add `CompoundLiteralInUnsupportedPosition` error: compound literals allowed only in variable declarations, assignments, returns, and struct field values ([pr#159])
- Extend `ArrayReturnCallInExpressionPosition` to also reject struct-returning calls in expression positions, including `MemberAccess` on sret calls ([pr#159])
- Add const initializer type validation: `const x: i32 = true;` now rejected ([pr#159])
- Add number-to-bool assignment rejection: `let x: bool = 0;` now rejected ([pr#159])
- Add ordering comparison validation: `true < false` now rejected; equality (`==`/`!=`) still allowed on all types ([pr#159])
- Fix duplicate `BinaryOperandTypeMismatch` error for mixed-type arithmetic ([pr#159])
- Remove dead code: `types_equal`, `is_compatible_with`, and `FuncInfo`'s `param_names` field
- Add `find_enclosing_variable_name()` to `TypedContext` for walking the AST parent chain to the enclosing variable
- Rename `ArrayReturnCallInExpressionPosition` to `CompoundReturnCallInExpressionPosition` to reflect struct coverage ([pr#178])
- Add `CompoundReturnCallInAssignment` error: `p = make_point()` rejected; use `let p = make_point()` instead ([pr#178])
- Add `MethodCallChainOnCompoundReturn` error: method call chains on compound-returning functions rejected — implicit temporaries cannot be named in formal proofs ([pr#178])
- Add `MethodMetadata` public struct and `TypedContext::lookup_method()` for cross-crate method metadata access ([pr#178])
- Migrate 13 codegen restriction checks from the type checker to the analysis pass; 11 `TypeCheckError` variants removed (46 remain, down from 50), `UzumakiInReassignment` and `ExternFunctionCall` are new
- Add 7 new `TypeCheckError` variants for validation hardening: `DuplicateStructFieldDefinition`, `RecursiveStructDefinition`, `InvalidAssignmentTarget`, `UninitializedVariable`, `ArrayLiteralSizeMismatch`, `DivisionByZero`, `DuplicateEnumVariant`
- Fix undeclared types in variable definitions now validated (previously missed in some positions)
- Fix case-insensitive type lookup removed — `I32` no longer resolves to `i32`; all type names are case-sensitive
- Fix `from_builtin_str` uses exact case-sensitive matching
- Fix external function parameter parsing in the AST builder (previously dropped parameters in some cases)
- Bump `tree-sitter-inference` grammar from 0.0.39 to 0.0.40 — fixes chained member access parsing
- Propagate `compound_literal_allowed` into nested struct literal fields and array literal elements, accepting `Outer { inner: Inner { x: 1 } }` and arrays inside struct fields ([pr#185])
- Add `find_enclosing_variable_name()` to `TypedContext` for analysis rule uzumaki struct name lookup ([pr#185])

### Testing

- Three new `coqc`-gate corpus fixtures cover the aggregate work end to end — `spec_aggregate_values.inf` (leaf encoding, enumeration order across ranks and struct field widths, literals, copies, leafwise comparison, existential leaves), `spec_bounded_iteration.inf` (the symbolic-index witness, a constant step descending before a symbolic one, a field step, and both the signed and unsigned index spellings) and `spec_quantifier_alternation.inf` (∃∀ nesting, with `"Hall "` added to `REQUIRED_CONSTRUCTS` as a needle the alternation fixture uniquely produces). Every obligation of all three was additionally proved `Qed` against real wasm-verifier before the goldens were committed, which is how the bounded-iteration fixture's first draft was caught: it emitted a *false* obligation — an index constrained only from above, refutable at `i = -1` — and was rewritten rather than shipped as the documented example of the encoding ([#355])
- The `coqc` gate now also audits, mechanically, that every constructor the vendored stub declares is elaborated by some module the gate compiles, closing what a hand-maintained needle list alone could not catch — the exact gap the [#230] `BI_forall` arity bug shipped through. New fixtures take obligation-printer operator coverage from 3 of 23 arms reachable at both `i32` and `i64` to full coverage, plus WASM module/instruction and `hassert`-printer surfaces no Inference source reaches. Seven stub declarations the emitter can never produce were deleted rather than exempted (see `rocq-stub/README.md`). This proves the stub is fully exercised, not that it still matches wasm-verifier's real `Assertions.v` ([#359]) ([#401])
- `infs` unit tests that write an executable stub and spawn it no longer fail intermittently with `ETXTBSY`; a shared `retry_while_exec_busy` helper retries spawns racing fork-inherited write descriptors ([#345])
- Tests outside `apps/infs` no longer race on fixed temporary paths — the class [#331] closed inside `apps/infs` and left standing elsewhere; every scratch path now comes from a `tempfile::TempDir`, fixing two live defects in codegen and cross-compiler tests ([#343])
- `infs` unit tests no longer collide on temporary directories across concurrent test processes; scratch dirs now come from `assert_fs::TempDir`, replacing constant paths and cross-process-colliding `fastrand` suffixes ([#331])
- `infs` unit tests no longer write the process-global `INFS_VERBOSE`; the predicate moved into a pure `verbose_from(Option<&OsStr>)` and `read_infs_metadata_with_verbosity`, removing an unsynchronized env read/write race ([#331])
- New `coqc`-gated fixture `tests/test_data/inf/spec_negative_consts.inf` carries negative constants through every constant-emitting position, and `negative_constants_are_parenthesized` scans the whole corpus for the unparenthesized spelling ([#314])
- Bump the test suite's `wasmtime` execution-harness dependency from 43.0.0 to 47.0.3, picking up the fix for [RUSTSEC-2026-0222](https://rustsec.org/advisories/RUSTSEC-2026-0222.html); no compiler output changes ([#335])
- Fix three golden-regeneration helpers stale against A042 (`regenerate_const_in_forall_wasm`, `regenerate_struct_array_field_nondet_wasm`, `regenerate_multidim_array_uzumaki_wasm`) to mirror their tests' `wasm_codegen_no_analysis` pipeline; eight sibling helpers remain follow-up debt
- Add `tests/src/robustness/deep_syntax.rs` covering deep input across seven shapes via `inference::with_compiler_stack`, pinning the 350-operand chain and 900-arm `else if` regressions, plus `compiler_stack.rs` contract tests and CLI exit-0 integration tests ([#322])
- Fix a hot-spin in the LSP e2e test client's `wait_for_response`/`wait_for_notification`: waits now scan the buffer once then read the wire under a single deadline; seven harness self-tests pin the contract ([#296])
- Add a `coqc` round-trip gate for proof-mode `wasm-to-v` output: vendored signature stub `core/wasm-to-v/rocq-stub/` (float-free, catching the [#230] arity class and the [#284] dead float arms), gated `tests/src/rocq_typecheck.rs`, and a `rocq-typecheck.yml` CI job ([#231])
- Replace the `rocq-stub` single `Wasm` namespace with a `wasm/` (vanilla WasmCert-Coq v2.2.0) plus `wasm_verifier/` (`hassert`, `ValidModule`/`ValidSpec`) pair; the absent `BI_forall`-family constructors are now the regression guard, with new fixture `rocq_spec_shapes.inf`
- Close the LSP/IDE test-coverage gaps from the PR #239 review: `ide-db` invalidation selectivity, `didClose`/`didChange` edge cases, percent-encoded URI round-trips, a deterministic republish barrier, and a pure `SerialQueue` extraction in the VS Code extension ([#254])
- Add 7 enum codegen test fixtures with four-tier verification (byte, WAT, validation, wasmtime execution) ([pr#187])
- Add 12 enum execution tests with Wasmtime assertions: variant tags, params, comparisons, reassignment, arrays, struct fields, const declarations, uzumaki ([pr#187])
- Add 7 type-checker tests for enum operator constraints: equality/inequality accepted, arithmetic/ordering/negation/boolean-context rejected ([pr#187])
- Rewrite all 85 AST builder tests in `tests/src/ast/helpers.rs` with deep structural verification; total test count up from ~1162 to 1917
- Expand analysis test coverage from 43 tests to match all 22 rules, adding tests for A006–A008 and the migrated A009–A019, A022–A024
- Add 43 analysis walker tests covering all 5 rules across free functions, struct methods, and spec functions, with all four nondet block types tested for A002 ([#156])
- Add 5 nested compound codegen test fixtures with four-tier verification: `nested_struct`, `struct_with_array`, `array_of_structs`, `nested_struct_with_array`, `multidim_array_uzumaki` ([pr#185])
- Add `struct_array_field_nondet` test fixture for struct uzumaki with array fields ([pr#185])
- Add 3 analysis test modules for nested compound rules: `rules_a026_a028.rs`, `rules_a029_a030.rs`, `rules_a031.rs` ([pr#185])
- Add type checker tests for nested compound literal propagation (`compound_literal_allowed`) ([pr#185])
- Add 9 method codegen test fixtures with four-tier verification (byte, WAT, validation, wasmtime execution) ([pr#178])
- Add negative codegen tests for unsupported features: `assert`, `**` operator, standalone `TypeMemberAccess`, recursive compound returns ([pr#178])
- Add validation tests for method mangling, immutable self zero-copy, and mutable self frame copy ([pr#178])
- Add 12 type checker tests for method chain rejection, compound-return in assignments, and member-access error cases ([pr#178])
- Update all AST, type-checker, and codegen tests for the typed arena API, migrating to structured traversal via typed IDs ([#156])
- Add 5 array test fixtures with 4-tier verification: `array_literal.inf`, `array_index.inf`, `array_assign.inf`, `array_params.inf`, `array_nondet.inf` ([#148])
- Add type-checker tests for array type validation: size/element-type mismatches, element assignment mutability, type equality, i64/u64 literal inference ([#148])
- Add 7 sret execution tests: literal return, variable return, chained forwarding, value semantics, sub-i32, i64, sret with params ([#148])
- Add 7 type-checker tests for `ArrayReturnCallInExpressionPosition` across let binding, return forwarding, standalone, argument, index access, and assignment positions ([#148])
- Add 10 inline execution tests for array element types: i8, u8, i16, u16, u32, i64, large array params (N > 16), mixed-type arrays, mutable parameters ([#148])
- Add runtime stack overflow trap test: two 32KB frames in 64KB stack verified to trap at runtime via Wasmtime ([#148])
- Add 6 struct codegen test fixtures with 4-tier verification: `struct_literal.inf`, `struct_access.inf`, `struct_assign.inf`, `struct_params.inf`, `struct_return.inf`, `struct_copy.inf` ([pr#159])
- Add ~30 type-checker tests for struct validation: mutability, shadowing, field validation, literal position and sret restrictions, and type-mismatch cases ([pr#159])
- Add 13 loop test fixtures with 4-tier verification, Wasmtime execution for all deterministic fixtures, and codegen coverage marks ([#152])
- Add execution test for `numeric_literals` verifying MIN/MAX boundary values for all 8 integer types via Wasmtime
- Add `arith_overflow` test module with 8 functions covering two's-complement wrapping arithmetic, multiplication overflow, and negation of MIN
- Add `expr_deep_nesting` test module with 5 functions verifying 8+ level expression nesting
- Add 4 algorithm integration test modules: `algo_bitwise`, `algo_converge`, `algo_i64_mixed`, `algo_recursive_math`
- Add 2 assignment test fixtures (`assign.inf`, `assign_nondet.inf`) with 10 Wasmtime execution assertions, plus AST `is_mut` parse tests and type-checker mutability tests ([#146])
- Add WAT golden file testing with `wasmprinter` (`assert_wat_equivalence()`, `regenerate_wat()`); non-det modules gracefully skipped ([#144])
- Add 3 conditional test fixtures (`if_else.inf`, `if_bool_exprs.inf`, `if_nondet.inf`) with 62 Wasmtime execution assertions ([#144])
- Flatten per-module test directory structure to avoid double-nesting via a `get_test_dir()` helper ([#144])
- Migrate codegen test data to per-test subdirectory layout: `tests/test_data/codegen/wasm/base/{name}/{name}.{inf,wasm}` ([pr#135])
- Add 28 codegen tests with three-tier verification: byte comparison against committed `.wasm` files, `inf_wasmparser::validate()`, and Wasmtime execution ([issue#97], [#125])
- Add codegen test helpers: the `codegen_output()` family, `wasm_codegen()` variants, and `assert_wasms_modules_equivalence()` ([issue#97], [#125])
- Expand `infs` test coverage from 282 to 429 tests (360 unit + 69 integration): TUI rendering, non-deterministic features, error handling; fixtures consolidated in `apps/infs/tests/fixtures/` ([#96])
- Move QA test suite to `apps/infs/docs/qa-test-suite.md` with 9 truly manual tests ([#96])
- tests: Consolidate builder tests by removing redundant `builder_extended.rs` module ([#50])
- tests: Add `builder_features.rs` module with feature-specific AST tests ([#50])
- tests: Add `primitive_type.rs` module with `SimpleTypeKind` tests ([#50])
- tests: Add utility assertions: `assert_single_binary_op`, `assert_function_signature`, etc. ([#50])

### infs CLI

- A build whose `infc` was chosen by adjacency now warns when that neighbour reports a different build commit than `infs` itself, naming both, so a stale compiler left in a build directory stops winning silently — adjacency asserts the two were built together, and nothing checked it. Only the sibling tier warns: for an `INFC_PATH`-pinned, PATH-installed, or managed `infc` a differing commit is the normal state. Cross-commit drift only; two binaries built from one commit with different working trees report the same hash ([#371])
- `infs doctor` now fails when the resolved `infc` exists but cannot be executed. Resolution selects a compiler by path, not by runnability, so an `infc` without an execute bit reported as a healthy `Resolved infc` line while every build was guaranteed to fail the moment it spawned. Note the observable change for anyone scripting it: `infs doctor` exits non-zero in that state where it previously exited zero. Executability is not a permission bit on Windows, so the check is inert there ([#371])
- `[build] wasm-features` in `Inference.toml`: opt into post-MVP WebAssembly proposals (initially `"bulk-memory"`), validated with did-you-mean diagnostics shared with `infc --wasm-features`, forwarded only to ABI ≥ 1.2 compilers, honored in single-file mode ([#315])
- `[build.wasm-opt]` no longer force-enables Binaryen's bulk-memory feature: `--enable-bulk-memory` is forwarded only when an artifact scan finds a bulk-memory operator, and bulk-free output is re-validated without bulk memory admitted ([#315])
- Fix `infs doctor` to verify `inference-lsp` at the `<INFERENCE_HOME>/bin` symlink where the editor actually resolves it, WARNing with `infs default <version>` as the repair when broken ([#253])
- Add opt-in post-build WASM optimization via Binaryen `wasm-opt` (`[build.wasm-opt]`): optimizes executable artifacts in place, skips proof/`-v` builds, requires Binaryen 116+, resolved via `WASM_OPT_PATH` then PATH; `--no-wasm-opt` skips one invocation
- Add `infs component add|list|remove` (rustup-style): `add wasm-opt` installs a pinned, sha256-verified Binaryen (`version_130`) atomically as a third resolution tier after `WASM_OPT_PATH` and PATH; adds `auto-install` manifest key and a doctor check
- Make `infs build` and `infs run` project-aware: with no path they discover `Inference.toml` by walking up and compile `<root>/src/main.inf` with its `use`-import closure (unreachable files warn); single-file forms unchanged ([#223], [#63])
- Add automatic PATH configuration on first install: shell profiles on Unix, `HKCU\Environment\Path` registry on Windows ([#96])
- Rename `INFS_HOME` → `INFERENCE_HOME` and `~/.infs` → `~/.inference` for consistency ([#96])
- Add `infc` symlink to installed toolchain ([#96])
- Improve `infs install` to auto-set the default toolchain when none is configured, recovering a manually removed default-toolchain file ([#96])
- Improve `infs doctor` to suggest `infs default <version>` when toolchains exist but no default is set, `infs install` otherwise ([#96])
- Fix `infs install` and `infs self update` to fall back to the latest pre-release version when no stable version exists in the manifest ([#96])
- Fix `infs install` failing on GitHub releases' nested archives by extracting the inner tar.gz after ZIP extraction ([#96])
- Fix `infs uninstall` leaving broken symlinks in `~/.inference/bin/`; broken links are now detected via `symlink_metadata()`, validated after uninstall, and repaired or removed ([#96])
- Change `infs doctor` to exit non-zero when checks fail so callers can detect failures ([#116])
- Remove manifest caching from `infs`; `fetch_manifest()` always fetches from network (the VS Code extension manages its own fetching lifecycle) ([#116])
- Remove LLVM toolchain management from `infs`: flat toolchain layout with `infc` at the root, no more `inf-llc`/`rust-lld`/`libLLVM`, simplified doctor checks, slimmed `InfsError`, `rand` replaced with `fastrand` ([#126])

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
- compiler: the multi-file front end parses each reachable file exactly once — the import walk lowers files directly into the shared arena, reordered via `AstArena::canonicalize_source_file_order`, instead of re-parsing after discovery ([#227])
- lsp: shed per-keystroke work — a forwarder thread lets `didChange` bursts coalesce, dependent republishes defer to idle, `LineIndex` handles are shared behind `Arc`, and the analysis cache is bounded ([#247])

### Fixed

- A spec-inner `struct`/`enum` whose name collides with one already registered elsewhere in the same file no longer loses its own declaration diagnostics to the collision: `reject_duplicate_spec_struct_or_enum` used to skip the rest of the `Def::Struct`/`Def::Enum` arm as soon as it reported the collision, so a repeated field (`DuplicateStructFieldDefinition`), a repeated variant (`DuplicateEnumVariant`), and a receiver declared anywhere but first (`SelfReferenceNotFirstParameter`) on the losing declaration went unreported no matter what else was wrong with it — the collision diagnostic was the only thing ever surfaced for that declaration. Registration (`register_struct`/`register_method`) stays skipped for the collided declaration, so no module is emitted either way; `RecursiveStructDefinition` on the collided type and the misleading cascade from `infer_def` resolving the receiver against the surviving top-level type of the same name are unaffected. The recovered diagnostics now surface beside the collision ([#377])
- Subtraction written without spaces now parses: `return value-1;` used to fail with `expected Semi`, because the lexer folded any `-` followed by a digit into one negative-literal token regardless of what preceded it, leaving `value` and `-1` adjacent with no operator between them. The fold is now gated on a one-token lookbehind, so it applies only in prefix position and `-` after anything an expression can end with (an identifier, a literal, `)`, `]`, `@`, …) is the subtraction operator. `a[n-1]`, `g(n-1)`, `1-2` and `v -1` are fixed by the same change. A closing brace is deliberately excluded from that set, so `if c { } -1;` keeps reading `-1` as a literal. No previously valid program changes meaning — every affected spelling was a parse error — and no emitted bytes change ([#375])
- infs: single-file `run` now forwards the enclosing manifest's `[wasm-dependencies]` and gains a `-L`/`--wasm-lib-dir` flag (project `run` forwards `-L` too), so a file or project binding `use { … } from <module>` runs instead of failing external resolution ([#367])
- infs: `infc` now resolves from the directory holding the running `infs`, whatever that directory is named, instead of requiring a literal `target/<debug|release>/` path and one of three enumerated build targets; a redirected `CARGO_TARGET_DIR`, a `--target-dir`, a custom cargo profile, or an unlisted target used to make the tier decline silently and run whatever `infc` was on `PATH` — the wrong compiler, looking like success. An installed `infs` now prefers an adjacent `infc` over a `PATH` hit, `INFS_VERBOSE` reports the fallthrough, and `infs doctor` labels the tier `sibling of infs` and no longer calls a plain managed install ambiguous ([#371])
- infs: `infs doctor`, PATH-conflict detection, and the managed `bin/` symlinks no longer consult the three-target allow-list merely to learn whether binaries end in `.exe`. On any other build target `doctor` reported `Cannot detect platform`, conflict detection and symlink validation returned empty results that read as "nothing wrong", and `infs default <version>` failed outright. `Platform` keeps its allow-list wherever the platform identity genuinely matters — downloads, manifest artifact selection, self-update ([#371])
- infs tests: the integration suite resolves `infc` from the running test binary's own directory instead of a hardcoded `target/debug`, so the end-to-end tests that spawn the real compiler no longer silently self-skip on the Windows and release CI legs; a missing `infc` or `wasmtime` now fails the run under `CI` instead of skipping ([#369])
- infs: project-mode `build` now forwards both the manifest's `[wasm-dependencies]` and the CLI's `-L`/`--wasm-lib-dir` to `infc`, and project-mode `run` forwards the manifest's, so a project binding `use { … } from <module>` links — and in proof mode emits its `.v` — without the `INFERENCE_WASM_LIB_PATH` workaround ([#361])
- wasm-to-v: `(*name*)` comments on `BI_local_get`/`BI_local_set`/`BI_local_tee` now resolve the local-name map by function index instead of type index, fixing misattributed names in linked or externally produced modules ([#336])
- A dynamic array index appearing only inside a function-scoped `const` initializer no longer aborts the compiler (`bounds-check scratch local must be reserved` panic); it now compiles and is guarded like any other index ([#220])
- Exported functions now normalize narrow scalar parameters (`bool`, `i8`, `u8`, `i16`, `u16`) at entry — low-bits/sign-extension per the C convention, `bool` by truthiness — so arbitrary host bit patterns cannot leak into the body
- The power operator `**` no longer crashes the compiler: the type checker rejects every use with a clean diagnostic suggesting repeated multiplication in a loop, including inside `spec` bodies
- wasm-codegen: a narrow-typed scalar uzumaki (`@`) draw (`u8`/`i8`/`u16`/`i16`/`bool`/non-empty `enum`) now emits a domain-constraint sequence after the draw, so Rocq quantifiers range over the declared type's domain instead of all 2^32 bit patterns ([#306])
- lsp: a diagnostic republish queued at `shutdown` is abandoned rather than flushed — no `publishDiagnostics` after `shutdown` per LSP 3.17 — while routed-back pre-shutdown requests are still answered `ContentModified` (-32801) ([#294])
- Constructing an array-of-struct value inside a struct field no longer panics in codegen; elements now store through the same per-element machinery used for top-level array-of-struct locals ([#224])
- Constructing a multi-dimensional array value inside a struct field (including arrays-of-structs) no longer panics in codegen; it delegates to the recursive leaf-store machinery shared with top-level locals ([#224])
- Fix FxHashMap non-deterministic iteration in `Arena` — `filter_nodes()` and `list_nodes_cmp()` now sort by node ID, ensuring reproducible WASM function emission order
- Fix Drop instruction emission for nested non-det blocks — `parent_blocks_stack.last()` (innermost block) is now used instead of `.first()` (outermost block)
- Fix `lower_literal` to emit type-correct WASM const instructions — number literals now consult `TypedContext` and emit `i32.const` or `i64.const` based on inferred type instead of always emitting `i32.const`
- Fix `wasm_to_v` public API signature — parameter changed from `&Vec<u8>` to idiomatic `&[u8]`
- ide: the resilient project walk (`inference::load_project_resilient`) no longer runs the unreachable-file warning scan, which walked the whole source tree on every keystroke to compute warnings the IDE discards; the compiler's `parse_project` keeps it ([#33])
- Extern-import diagnostics reported from an imported file now carry that file's module-path label instead of pointing at wrong positions in the entry file ([#33])
- tests: the `TempProject` helper's temp-directory names now append a process-wide `AtomicU64` sequence counter, so same-tag tests no longer collide under coarse clock resolution ([#270])
- lsp: `file:` URI-to-path mapping now normalizes (dot segments, case-insensitive scheme, RFC 8089 single-slash form) and rejects path-form UNC and bare/drive-relative Windows drive URIs, so one on-disk file interns under one spelling ([#248])
- ide: on case-insensitive filesystems (macOS/Windows) a mis-cased import path no longer bypasses the open-buffer overlay; the loader retries the overlay under the on-disk canonical spelling before reading stale disk text ([#248])
- compiler, ide: a leading UTF-8 BOM (U+FEFF) is now stripped in the shared source-ingestion seam, so BOM-prefixed files parse and compile instead of failing at the lexer with shifted line-0 positions ([#248])
- lsp: `LineIndex::new` and `Vfs` path interning now panic with a clear message at their `u32` width limits (4 GiB text, 2^32 paths) instead of silently wrapping ([#248])
- ide/lsp: completions no longer offer names that fail to compile when accepted: plain vs braced imports are respected, a new `<module>::` context offers public defs, cross-module private members are dropped, and comments/strings suppress completions ([#246])
- ide/lsp: goto-definition and hover cover five hit-testing gaps: an identifier's exclusive end, `use` path segments and braced items, function type parameters (`T'`), enum variant declarations, and function-local `const` references ([#244])
- ide/lsp: goto-definition and hover now agree with the type checker instead of a syntactic name-scan: free calls over same-named methods, braced-import-only bare values, `pub use` re-export chains, qualified type leaves, source-like `fn(…)` hover spelling, and `let`/`const` `full_range` ([#245])
- VS Code extension: switching or updating the toolchain now restarts a running language server, so diagnostics/hover/goto immediately reflect the new default toolchain instead of a stale `inference-lsp` process ([#250])
- VS Code extension: language-client lifecycle robustness — configuration changes decide start/stop/restart inside the serialized queue so the last setting always wins, and a hung `initialize` is bounded by a 30-second timeout with the process disposed and the failure surfaced ([#251])
- VS Code extension: contribute the standard `inference-lsp.trace.server` setting (`off`/`messages`/`verbose`) in `package.json` so the trace knob is discoverable ([#251])
- VS Code extension: the walkthrough's "Create a Project" step now instructs saving with the `.inf` extension, since language-server features are file-scheme-only ([#251])
- VS Code extension: on Windows the managed-location tier now probes `%APPDATA%\inference`, where `infs` actually installs; the shared `inferenceHome()` helper mirrors `ToolchainPaths::new()` in `apps/infs/src/toolchain/paths.rs` ([#252])
- lsp: an unwinding panic in the analysis stack no longer kills the server: a panicking request answers JSON-RPC `InternalError`, a panicking notification rebuilds the analysis host, and every other open document keeps working ([#241])
- lsp: LSP 3.17 conformance polish: post-`shutdown` requests answer `InvalidRequest`, `InitializeParams` validates during the handshake, `serverInfo` is reported, hover honors `contentFormat`, inlay-hint ranges clamp to EOF, and unmappable `didClose` publishes nothing ([#249])
- type-checker: a named constant used as an array size is now a `NonLiteralArraySize` diagnostic instead of a `todo!` panic aborting the compiler and IDE analysis; compile-time evaluation of array sizes remains future work (#79) ([#240])
- compiler: the unreachable-file warning scan (`parse_project`) is now bounded by `MAX_SCAN_DIRECTORIES` and fails open (no warnings) when it gives up, so a bare entry file near a filesystem root no longer walks the whole disk or loops on symlink cycles ([#288])
- lsp: a `didChange` for a never-opened (or already-closed) document is now dropped and logged instead of silently adopted into tracking and dependents-republish sweeps ([#275])

### Project Manifest

- Add optional `[build.wasm-opt]` sub-table to `Inference.toml`: `enabled` (default `true`), `level` (`"0"`–`"4"`, `"s"`, `"z"`; default `"3"`), `auto-install` (default `false`); `infs new`/`infs init` scaffold a commented-out block
- Consume `[build]` and `[verification]` in project-mode builds: new `[build] mode = "compile" | "proof"`, `[verification] output-dir` honored in proof builds only (validated relative-only, requires ABI ≥ 1.1); CLI flags override the manifest ([#223])
- Replace `manifest_version` with `infc_version` (semver String) in Inference.toml, auto-detected from `infc --version` on `infs new`/`infs init`, falling back to the `infs` version ([#96])

### Editor Support

- Add VS Code extension with syntax highlighting for Inference language ([#94])
- Add TextMate grammar with hierarchical scopes for non-deterministic keywords (`forall`, `exists`, `assume`, `unique`, `@`)
- Add language configuration with bracket matching, comment toggling, and code folding
- Publish extension to VS Code Marketplace: [inference-lang.inference](https://marketplace.visualstudio.com/items?itemName=inference-lang.inference)
- Add Configuration sidebar (TreeView) to VS Code extension with toolchain info, settings overview, doctor status, and copy/reveal actions, auto-refreshing on changes ([#116])
- Add automatic terminal PATH integration to VS Code extension: `infs` and `infc` work in integrated terminals immediately after install or update, persisting across sessions ([#116])
- Add toolchain management commands to VS Code extension: Install (SHA-256-verified), Update, Select Version, and Run Doctor ([#116])
- Add Getting Started walkthrough to VS Code extension: four-step guided setup (install toolchain, verify with doctor, create project, build) ([#116])
- Add status bar integration showing toolchain health at a glance ([#116])
- Update VS Code extension tests and QA docs after LLVM removal: flat toolchain layout, single `infc` doctor check replacing `inf-llc`/`rust-lld`/`libLLVM` ([#127])
- Add "Install Component (wasm-opt)" command to VS Code extension (`inference.installComponent`), also offered as an "Install wasm-opt" action on doctor toasts whenever a `wasm-opt` check warns or fails

### IDE / LSP

- The language server's `SERVER_STACK_SIZE` is now the larger of its historical 64 MiB and `inference_parser::MIN_COMPILE_STACK`, pinned by a compile-time assertion, so a file that compiles under the CLI cannot kill the editor process ([#322])
- Add `inference-lsp`, a Language Server Protocol server for Inference (`apps/lsp`): single-threaded `lsp-server` 0.8 stdio binary with diagnostics (rule codes `A001`–`A041`), hover, goto-definition, document symbols, completions, inlay hints, and an e2e suite over raw JSON-RPC ([#33])
- Add the `ide/` crate stack backing the LSP server: `ide/vfs` (path interning + overlay), `ide/base-db` (`LineIndex`, position PODs), `ide/ide-db` (`RootDatabase` with closure-aware invalidation), `ide/ide` (`AnalysisHost`/`Analysis` feature API) ([#33])
- Fix a permanently stale IDE analysis when an imported file exists but cannot be read: the resilient walk surfaces `ResilientProjectParse::read_failures` and folds them into invalidation, so making the file readable re-analyzes every open importer ([#242])
- Fix false missing-import diagnostics when a non-entry file of a multi-directory project is opened standalone: each open file's source root now resolves via the nearest `Inference.toml` manifest, then an analyzed entry's closure, then its own directory ([#243])
- Add structured type-check diagnostics: `check_with_diagnostics` returns a lossless `TypeCheckOutcome` whose `TypedContext` stays fully indexed even with errors; `build_typed_context` is re-expressed on top, so compiler and IDE share one checking implementation ([#33])
- Add a `FileLoader` seam (`exists`/`read`) to `core/inference` plus a resilient walk variant, `load_project_resilient`, that collects every problem instead of failing fast; `parse_project` is re-expressed on top and stays byte-identical for a clean project ([#33])
- Ship `inference-lsp` with the managed toolchain: bundled inside the existing `infc-<platform>` archives, symlinked into `$INFERENCE_HOME/bin` next to `infc`, with an appended `infs doctor` check ([#33])
- VS Code extension 0.0.5: built-in LSP client starts `inference-lsp` over stdio on activation (settings `inference.lsp.enabled`/`inference.lsp.path`, restart command), auto-starting after a toolchain install so first run needs no reload ([#33])

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
[#314]: https://github.com/Inferara/inference/issues/314
[#322]: https://github.com/Inferara/inference/issues/322
[#329]: https://github.com/Inferara/inference/issues/329
[#331]: https://github.com/Inferara/inference/issues/331
[#332]: https://github.com/Inferara/inference/issues/332
[#333]: https://github.com/Inferara/inference/issues/333
[#335]: https://github.com/Inferara/inference/pull/335
[#336]: https://github.com/Inferara/inference/issues/336
[#361]: https://github.com/Inferara/inference/issues/361
[#367]: https://github.com/Inferara/inference/issues/367
[#369]: https://github.com/Inferara/inference/issues/369
[#371]: https://github.com/Inferara/inference/issues/371
[#346]: https://github.com/Inferara/inference/issues/346
[#345]: https://github.com/Inferara/inference/issues/345
[#344]: https://github.com/Inferara/inference/issues/344
[#343]: https://github.com/Inferara/inference/issues/343
[#353]: https://github.com/Inferara/inference/issues/353
[#376]: https://github.com/Inferara/inference/issues/376
[#375]: https://github.com/Inferara/inference/issues/375
[#377]: https://github.com/Inferara/inference/issues/377
[#378]: https://github.com/Inferara/inference/issues/378
[#359]: https://github.com/Inferara/inference/issues/359
[#363]: https://github.com/Inferara/inference/issues/363
[#356]: https://github.com/Inferara/inference/issues/356
[#401]: https://github.com/Inferara/inference/issues/401
[#402]: https://github.com/Inferara/inference/issues/402
[#354]: https://github.com/Inferara/inference/issues/354
[#412]: https://github.com/Inferara/inference/issues/412
[#413]: https://github.com/Inferara/inference/issues/413
[#416]: https://github.com/Inferara/inference/issues/416
[#355]: https://github.com/Inferara/inference/issues/355
