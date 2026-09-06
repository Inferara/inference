# inference-analysis

Static analysis pass for the Inference compiler. Runs after type checking, before code generation, and validates semantic invariants beyond what the type system expresses.

The type checker is responsible only for type correctness — blocking errors that would prevent further analysis. Everything else (control flow validation, lint warnings, codegen restrictions) lives here.

## Pipeline Position

```
parse -> type_check -> analyze -> codegen -> wasm_to_v
```

The `analyze()` function is the entry point. It accepts a `&TypedContext` produced by the type checker and returns either an `AnalysisResult` (success, possibly with warnings) or `AnalysisErrors` (one or more hard errors, all collected before returning).

## Architecture

```
analyze()
    |
    +-- rules::all_rules()  (static slice of &dyn Rule)
         |
         +-- Rule::check(&TypedContext) -> Vec<AnalysisDiagnostic>
                  |
                  +-- walker::walk_function_bodies()  (shared traversal)
                  |        visits every Stmt in every function body
                  |        tracks loop_depth, nondet_depth, nondet_block_kind
                  |
                  +-- or custom traversal  (A004 only)
```

Each rule is a zero-sized struct implementing the `Rule` trait. Rules are stateless and `Send + Sync`, which keeps the door open for parallel execution in the future. The `rule!` macro generates the struct and trait implementation from a compact attribute syntax, eliminating boilerplate.

Errors, warnings, and informational findings are partitioned by severity. The `analyze()` function returns `Err(AnalysisErrors)` only when at least one `Error`-severity finding exists; `Warning` and `Info` findings are always returned via the success path or bundled inside `AnalysisErrors`.

## Module Organization

| Module | Description |
|--------|-------------|
| `lib.rs` | `analyze()` entry point; partitions findings by severity |
| `rule` | `Rule` trait and `rule!` / `__severity!` macros |
| `errors` | `AnalysisDiagnostic`, `AnalysisErrors`, `AnalysisResult`, `Severity` |
| `walker` | `walk_function_bodies()`, `for_each_function_body()`, `WalkContext` |
| `rules` | `all_rules()` registry and one sub-module per rule |
| `rules::position` | the shared position phrases the value-rejecting rules name in their messages |

## Rules

### Control Flow (errors)

| ID | Struct | Severity | What it checks |
|----|--------|----------|----------------|
| A001 | `BreakOutsideLoop` | error | `break` must be inside a loop body |
| A002 | `BreakInsideNonDetBlock` | error | `break` must not be inside a `forall`/`exists`/`assume`/`unique` block |
| A003 | `ReturnInsideLoop` | error | `return` must not appear inside a loop body |
| A004 | `InfiniteLoopWithoutBreak` | error | `loop { }` without a condition must contain a reachable `break` |
| A005 | `ReturnInsideNonDetBlock` | error | `return` must not appear inside a non-deterministic block |
| A006 | `UzumakiOutsideNonDetBlock` | error | uzumaki (`@`) must not appear outside a `forall`/`exists`/`assume`/`unique` block |
| A007 | `MissingReturn` | error | non-void function must have a `return` statement on every branch (branch-aware) |
| A008 | `StandaloneUzumaki` | error | uzumaki expression that is not assigned to a variable serves no purpose |

A003 and A005 require a single exit point per function to simplify formal verification. A002 prevents `break` from prematurely terminating path exploration in non-det blocks.

### Lint Warnings

| ID | Struct | Severity | What it checks |
|----|--------|----------|----------------|
| A009 | `EmptyEnumDefinition` | warning | enum with no variants is likely an oversight |
| A010 | `MethodNeverAccessesSelf` | warning | method declares `self` but never reads or writes a field through it |
| A011 | `EmptyStructDefinition` | warning | struct with no fields and no methods |

A011 keys on no fields *and* no methods, and stays that way deliberately: a field-less struct that declares methods is the supported method-namespace idiom, so warning on it would flag the exact pattern the language points people at. A045 governs *values* of a field-less struct; A011 governs a declaration that declares nothing at all. The two subjects are disjoint, and where they overlap (a bare empty struct that is also given a value) both fire.

### Variable Initialization (errors)

| ID | Struct | Severity | What it checks |
|----|--------|----------|----------------|
| A025 | `UninitializedVariable` | error | variable declared without an initializer |

### Codegen Restrictions (errors)

These rules cover constructs that are valid in the type system but cannot yet be lowered by the code generator. They live here rather than in the type checker because they are implementation limits, not type errors.

| ID | Struct | Severity | What it checks |
|----|--------|----------|----------------|
| A012 | `CompoundLiteralAsArgument` | error | compound literal (array or struct) passed directly as a function argument |
| A013 | *(merged into A012)* | — | *(array and struct literal arguments are one rule)* |
| A014 | `ArrayUzumakiAsArgument` | error | array uzumaki (`@`) passed directly as a function argument |
| A015 | `CompoundLiteralPosition` | error | compound literal (array or struct) in an unsupported expression position |
| A016 | `CompoundReturnCallPosition` | error | compound-returning function call in a general expression position |
| A017 | `CompoundReturnCallAssignment` | error | compound-returning function call on the RHS of an assignment statement |
| A018 | `MethodCallChainCompound` | error | method call chained on the result of a compound-returning function |
| A019 | `ArrayIndex64Bit` | error | 64-bit integer used as an array index |
| A022 | `LiteralOutOfRange` | error | numeric literal is outside the valid range for the type its position gave it |
| A023 | `UzumakiInReassignment` | error | uzumaki (`@`) used in a variable reassignment (only `let` initializers are supported) |
| A024 | `ExternFunctionCall` | error | call to an **unbound** `external fn` — one no `use … from` clause binds to a source module |
| A026 | `NestedCompoundDepth` | error | struct field is itself a nested compound type beyond one level of nesting |
| A027 | `UzumakiOnNestedStruct` | error | uzumaki (`@`) assigned to a struct whose fields include another struct or an array of structs |
| A028 | `UzumakiOnStructInArray` | error | uzumaki (`@`) assigned to an array whose element type is a struct |
| A029 | `CompoundLiteralMemberAssign` | error | compound literal (struct or array) used directly as the RHS of a member-access or array-index assignment |
| A030 | *(removed)* | — | *(uzumaki on scalar arrays now supported at any depth)* |
| A031 | `UnsupportedCompoundReturnExpr` | error | return expression in a compound-returning function is not a supported form (identifier, literal, call, or field/element access) |

A022 validates a literal against the type the *type checker recorded* for it, and an integer literal takes that type from the position it appears in — an annotation, a call argument, a `return`, a struct field, an array element, or the operand it is compared or combined with. The type is therefore often written somewhere the literal is not, so the diagnostic names the position that supplied it:

```
literal `300` is out of range for type `u8` (valid range: 0..=255)
note: the literal is typed `u8` by the type expected in return statement
```

The note is present only when a position gave the literal its type; a literal left at the `i32` default has nothing to name and the message is the bare range line. The provenance comes from `TypedContext::literal_type_source`, a diagnostics-only side table — the recorded node type stays the single source of truth for what a literal denotes.

A literal is measured exactly as written, un-negated, so A022 hands the separated-sign spelling over to A046 and skips every literal `walker::separated_negated_literal` identifies. Without that handoff, `- 128` at `i8` would report "literal `128` is out of range" — true of `128`, false of the `-128` the author meant. The handoff accepts nothing: every literal A022 stops measuring is one A046 rejects, so `- 300` at `i8` is still an error, and once the spelling is fixed to `-300` the literal carries its sign and is measured as the negative number it is. Parenthesized negation (`-(128)`) is not part of the handoff.

### Recursion (errors)

| ID | Struct | Severity | What it checks |
|----|--------|----------|----------------|
| A035 | `RecursionDetected` | error | direct or indirect (mutual) recursion is forbidden so stack usage stays statically bounded (Power of 10, Rule 1) |

A035 builds a whole-program call graph keyed by `FnKey` (from `inference-fn-key`, the shared canonical function identity used by both this crate and `wasm-codegen`) and reports each call cycle once, pointing at the call site that closes the cycle.

### Stack Depth (errors)

| ID | Struct | Severity | What it checks |
|----|--------|----------|----------------|
| A036 | `StackDepthExceeded` | error | cumulative shadow-stack usage along a call chain must not exceed the configured stack budget (64 KB by default) |

A036 reuses A035's whole-program call graph (a DAG, since recursion is forbidden) and computes the maximum-weight root-to-leaf path, where each node's weight is a conservative upper bound on that function's compound (array/struct) frame size. Scalar locals live in WASM locals and contribute nothing. The estimate over-approximates codegen's real frame layout by construction, so the rule never accepts a program codegen would overflow. The shared graph construction lives in `src/call_graph.rs`.

### Array Bounds (errors)

| ID | Struct | Severity | What it checks |
|----|--------|----------|----------------|
| A037 | `ArrayIndexConstOutOfBounds` | error | a constant array index (`arr[c]`) is negative or `>= length` |

A037 is the static half of array bounds checking. When the index is a constant integer literal, the array length is known at compile time from the array sub-expression's `Array(_, length)` type info, so an out-of-range access is rejected with zero runtime cost in every build profile and compilation mode. A negative literal such as `arr[-1]` lowers to a single `NumberLiteral` whose text keeps the leading `-`, so it is caught here as well. Dynamic (non-literal) indices are out of A037's scope — they are guarded at run time by `core/wasm-codegen`, in every build and every compilation mode (see that crate's docs); the two mechanisms together close the bounds-safety hole.

### Uzumaki in unsupported positions (errors)

| ID | Struct | Severity | What it checks |
|----|--------|----------|----------------|
| A038 | `UzumakiOnCompoundField` | error | uzumaki (`@`) used as the value of a struct- or array-typed field in a struct literal (e.g. `Outer { inner: @ }`); only scalar and enum fields may use `@` |
| A039 | `StructUzumakiAsArgument` | error | struct-typed uzumaki (`@`) passed directly as a function argument; assign to a local variable first (struct sibling of A014, which covers array uzumaki) |
| A040 | `UzumakiOnCompoundArrayElement` | error | uzumaki (`@`) used as a struct- or array-typed element of an array literal (e.g. `[p, @]` where `p` is a struct); only scalar and enum elements may use `@` |

A compound (struct or array) `@` is lowered by writing into a *named* frame slot, which only a `let`/`const` binding supplies, so `@` of such a type is rejected wherever no slot exists. These complement A014 (array `@` as a function argument) and A027/A028 (whole-binding compound `@`). A scalar or enum `@` is unaffected: the type checker threads it its declared field/element type, so it lowers to a single uzumaki opcode. Array-literal arguments are handled separately by A012, so these rules do not extend to that position.

### Duplicate Local Names (errors)

| ID | Struct | Severity | What it checks |
|----|--------|----------|----------------|
| A041 | `DuplicateLocalName` | error | a function-local name (`let`/`const`) declared more than once per function body; well-typed across disjoint sibling blocks but collides in the flat WebAssembly local namespace — rename or hoist a single declaration |

A041 rejects a function-local name introduced more than once in a single function body, even when the two declarations sit in disjoint sibling blocks (`if`/`else` arms, sequential `if`s, `loop` bodies, or non-deterministic blocks) and are individually well-typed — the type checker's scope-based shadowing check never flags them, since sibling scopes never coexist. The conflict surfaces one phase later: `core/wasm-codegen` flattens every body local into a single name-keyed WebAssembly local namespace, so two sibling declarations of the same name would otherwise collide. This is a simplicity and auditability rule, not a proof-soundness requirement — the Rocq translation addresses locals by numeric index, so the flat namespace exists only to preserve a 1:1 mapping between source name, WASM local, and proof index for humans reading traces and proofs. The diagnostic cites both declaration sites and suggests renaming one of them or hoisting a single declaration above the blocks.

### Non-Deterministic Constructs Outside `spec` (errors)

| ID | Struct | Severity | What it checks |
|----|--------|----------|----------------|
| A042 | `NonDetOutsideSpec` | error | a non-deterministic block (`forall`/`exists`/`assume`/`unique`), inline or as a function-body modifier, used lexically outside a `spec` declaration |

A042 enforces that the non-deterministic block forms — inline `forall`/`exists`/`assume`/`unique` statement blocks and the function-body-modifier form (`fn f() forall { … }`) — appear only lexically inside a `spec` declaration, where they describe formal specifications rather than executable code. A block in a top-level function, a top-level struct method, or nested inside either is rejected. The check is purely lexical (it never inspects types), so it is independent of the compilation mode and runs in both compile and proof modes. Only the outermost non-det block on each path is reported: a `forall { exists { … } }` outside a spec yields one diagnostic, not two. Uzumaki (`@`) outside a spec is covered transitively — `@` already requires an enclosing non-det block (A006), and A042 rejects that block — so no separate `@` check lives in this rule.

### Shift Count Out of Range (errors)

| ID | Struct | Severity | What it checks |
|----|--------|----------|----------------|
| A044 | `ShiftCountOutOfRange` | error | a shift (`<<`/`>>`) whose count is a literal that is negative or `>=` the operand type's bit width |

A044 rejects a shift whose count operand is a statically-known literal outside `0..width` for the operand type — `x << 32` or `x >> -1` on an `i32`. It complements the runtime rule that a shift count is taken modulo the operand type's bit width: a literal that lands outside the valid range is a program error, not a value to fold silently. Parenthesized and negated literals are resolved (`x << (33)`, `x >> -1`); dynamic counts and const-declared counts (`const K: i32 = 33; x << K`) are not detected, the same statically-known-literal scope as A022 and the division-by-zero check. The width is read from the operand type, so every integer width is covered in practice as well as in principle: a literal count takes the type of the operand being shifted, which makes `x << 64` on an `i64` and `x << 8` on a `u8` reachable, and where both operands are literals the type expected of the whole shift fixes the width (`let x: i64 = 1 << 64;` is rejected, `1 << 40` is not). Unparseable or out-of-`i128`-range literals are left to A022 to avoid double-reporting.

### Field-less Struct Values (errors)

| ID | Struct | Severity | What it checks |
|----|--------|----------|----------------|
| A045 | `FieldLessStructValue` | error | a field-less struct used as a value (literal, binding, parameter, return, field, `self` receiver) |

A045 rejects *values* of a struct with no fields. Such a struct occupies zero bytes, so there is no memory region to hold, copy, or reason about one of its values: codegen's frame layout allocates a struct slot only when the size is greater than zero, while struct-literal lowering unconditionally requires one, and a binding or parameter that survives is lowered as a pointer into nothing. The rule covers the struct literal in every expression position, the declared type of a `let` or of a `const` at function or module scope, function/method/`external fn` parameters (including `_: E`) and return types, struct fields, and a `self`/`mut self` receiver declared on such a struct — looking through array nesting at any depth, since an array is zero-sized exactly when its element type is.

Rejecting a field-less struct as the type of a *field* is what closes the hole: a struct all of whose fields are zero-sized would itself be zero-sized, so forbidding a zero-sized field collapses that composition into the base case. In an accepted program a struct is therefore zero-sized if and only if it has no fields, which lets the predicate be `fields.is_empty()` plus array recursion — no transitive size computation, no visited set, no cycle handling. With every value-introducing position rejected, assignments, reads, and method calls on such values need no checks of their own (each requires a binding, parameter, or field that is already rejected), so a program reports one diagnostic per offending declaration rather than one per use. A module-scope `const` is checked here in its own right rather than left to A032, which rejects *every* top-level `const` as not yet implemented: A032 is a gate on an unimplemented feature, and a closure resting on it would go silently incomplete the day that feature lands. Both fire on such a declaration; there is no cross-rule suppression.

*Declaring* a field-less struct stays legal. A field-less struct with associated functions is the supported method-namespace idiom (`E::helper()`) and compiles unchanged; the `self` receiver is rejected because once no value of the struct can exist the method is uncallable by construction, and the fix — dropping `self` — produces exactly that idiom. `external fn` signatures are checked for their ABI surface rather than for the closure (A024 rejects a call to an *unbound* extern outright, and a bound extern's declared parameter and return types are themselves in scope above, so no field-less value flows through either). Two documented non-scopes: generics, since a type parameter never resolves to a struct, so a generic signature (`fn id T'(x: T) -> T`) is outside the predicate — nothing is missed by that today, because the compiler does not monomorphize and codegen rejects a generic type outright, so there is no instantiation at a field-less struct to check; and local type aliases, which are non-transparent in Inference and so are a dead end rather than a route to a value.

### Spaced Negative Literals (errors)

| ID | Struct | Severity | What it checks |
|----|--------|----------|----------------|
| A046 | `SpacedNegativeLiteral` | error | a unary minus applied to a numeric literal is written apart from the digits (`- 128`) instead of glued (`-128`) |

A046 requires that a unary minus applied to a numeric literal be written against the digits. `-128` is one token: the lexer folds the sign into the digits, and the literal is ranged and lowered as the negative number it spells. `- 128` is not a literal at all — it is a `Neg` over the bare literal `128`, which every later rule measures on its own. That is what made the same value compile or fail on a space: at `i8`, `- 100` was accepted (`100` fits) while `- 128` was rejected as "literal `128` is out of range", a diagnostic about a value the author never wrote and a limit `-128` does not exceed. Every signed minimum was unreachable in that spelling and only in that spelling. Rather than teach the range check to look through a negation, the rule removes the second spelling, leaving one canonical way to write a negative literal — the same readability argument A033 makes for combined unary operators.

The predicate lives in `walker::separated_negated_literal` and is shared with A022, which skips exactly the literals A046 claims (see above). Separation is measured on offsets, not on source text: a `PrefixUnary` node starts at its operator, so the glued spelling is the only one whose digits begin at `offset_start + 1`; a space, several, a newline, and a line comment are all the same offence. A literal whose own text carries a sign is excluded, because that is the grammar's eager lexing of `--42` / `- -42` and belongs to A033 — advising the glued form there would recommend a spelling A033 rejects.

Negating anything that is not a literal stays legal (`- x`, `- g()`): there is no token to glue the sign to and so no second spelling to choose between. Binary subtraction (`a - 1`, `a-1`) has a left operand, is never a `PrefixUnary`, and is never seen. Two documented non-scopes: `~ 5` and `! x`, since only `-` is folded into a literal by the lexer and so only `-` has a whitespace-dependent alternative to remove; and `-(128)`, whose operand is a parenthesized expression rather than a literal — it cannot be closed up into a token, A022's reading of it is unchanged, and peeling the parentheses would demand a rewrite the syntax does not offer.

### External Write Through an Immutable Argument (errors)

| ID | Struct | Severity | What it checks |
|----|--------|----------|----------------|
| A047 | `ExternMutArgument` | error | a compound argument at a `mut` `external fn` parameter whose root binding is not declared `mut` |

A047 requires that a struct or array argument landing on a `mut` `external fn` parameter be rooted at a `mut` binding. A linked external shares the caller's single linear memory, so a compound argument is not copied across the call at all: the caller hands over a raw pointer into its own frame, and the foreign body reads and writes the caller's bytes directly. `mut` on the declaration is the statement that it may store through that address, and the linker checks the claim against the merged body. That makes this the one place a write to a binding is invisible in Inference source — the store lives in a `.wasm` the type checker never reads — so the call site has to carry the statement instead, exactly as it would for an assignment written out in full.

Four conditions decide the report: the callee resolves through `ExternIndex` to an `external fn` declaration; that declaration's parameter at the argument's position is `mut`; the parameter's declared type passes a *region*, meaning an array at any depth or a name that resolves to a struct; and the argument is not rooted at a `mut` binding. Correspondence is positional — `call_args[i]` is declaration parameter `i` whether or not the call labels its arguments, since nothing in the pipeline reorders by label — and a call whose arity does not match the declaration is a type error reported before analysis runs.

An enum is out of scope because it lowers to a bare `i32` tag, and so is every scalar: neither passes a region, which is what keeps the documented `external fn store_at(mut ptr: i32, ..)` idiom untouched. The region predicate deliberately does not mirror codegen's own compound test, which is private to `inference-wasm-codegen` — a crate this one does not and must not depend on — and it carries no field-less-struct carve-out, because A045 already rejects a field-less struct as an `external fn` parameter type at any array depth; a future relaxation of A045 must revisit it.

Resolution is scope-aware, as in A024: an `external fn` may be declared at a file's top level or inside a `spec`, the two may share a name, and each call is measured against the declaration visible from where it stands. Mutability is read from the argument's *root* binding, so `p`, `p.inner`, `arr[i]` and `(p)` are all judged by the binding they reach into — a projection of a `mut` binding is memory that binding's own declaration already says may change. An argument rooted at no binding at all (a compound literal, the result of a call, a draw) is reported rather than silently accepted, but it is never the only diagnostic such a program gets: A012, A016 and A014/A039 already reject those shapes as arguments, so this half of the rule is defense in depth.

### String Values (errors)

| ID | Struct | Severity | What it checks |
|----|--------|----------|----------------|
| A048 | `StringNotSupported` | error | a `string` value: a string literal, or `string`/`String` as the type of a binding, parameter, return, or struct field |

A048 rejects every position at which a string value could be introduced. `string` and `String` are root-scope builtin type names, so every annotation that spells one type-checks — and nothing after the type checker can act on it. There is no layout for a string in linear memory, so frame layout has no byte size to give one; there is no WebAssembly value type to pass one in, so a signature carrying one has nothing to lower to; and there is no term for a proof to describe one with. The three failure modes that produced were an abort on the literal, an abort one layer earlier in the byte-size computation that lays out a frame or a struct, and a clean unsupported-type error on a signature — none of them a diagnostic anyone could act on.

Covered: a string literal in every expression position; the recorded type of a `let` or of a `const` at function or module scope; a function, method, or `external fn` parameter, including `_: string` and a bare positional `string`; a function, method, or `external fn` return type; and a struct field — with array nesting looked through at any depth, since an array of strings is exactly as unrepresentable as a string and an array type is never a value position on its own. Reporting is per offending construct, so `let s: string = "hi";` reports twice: the annotation and the literal are two separate things to remove. A module-scope `const` is checked in its own right rather than left to A032, for the reason A045 records.

The type name is kept and the values are rejected, so an author who writes `string` is told the feature is not implemented and what to model text with instead — a `[u8; N]` with its bytes written as numbers, or an enum tag when the value is one of a fixed set — rather than being told `string` is an unknown type. Type positions are read from the annotation as written rather than from the resolved struct table, because the predicate is a builtin type kind that `TypeInfo::from_type_id` decides on its own; the binding half reads the type the checker recorded, which is the resolved one. Two documented non-scopes: type aliases, item form and statement form alike, because aliases are nominal in Inference and so name a type at which no value can be produced; and the `self` receiver, whose type is the enclosing struct. `spec` bodies are covered — a spec function is lowered to a real WebAssembly function in proof mode and reaches the same expression lowering — and the proof translation's own rejection covers a string literal in an *assertion term*, which is a different path. The rule is a gate on an unimplemented feature: the day strings are implemented, it is deleted whole.

### Unit Values (errors)

| ID | Struct | Severity | What it checks |
|----|--------|----------|----------------|
| A049 | `UnitAsValue` | error | a `()` value: a unit literal outside the two exempt statement forms, or `()`/`unit` as the type of a binding, parameter, or struct field |

A049 draws the line between the *absence* of a value and a *value of nothing*. The first is the point of the unit type and is implemented: a function whose return type is `()`, `unit`, or omitted returns nothing, and code generation gives it an empty result list. The second is a declaration that cannot be honoured — a unit value carries no information, so it occupies no bytes and has no WebAssembly type. A parameter declared `()` is given no argument slot to arrive in, a binding of it has nothing to store, an array of it has no element size for frame layout to compute, and a struct field of it has no offset that means anything.

Covered: a unit literal, the recorded type of a `let` or of a `const` at function or module scope, a function/method/`external fn` parameter (including `_: ()`), and a struct field — arrays peeled at any depth, both spellings (`()` and `unit`) resolving to the same type kind. **The return type is not covered**, because it is the one place unit means something. Two statement forms are exempt: a unit literal as the whole expression of a `return` or of an expression statement, parentheses peeled. That exemption is load-bearing rather than cosmetic — the parser synthesizes a unit literal for the missing expression of a bare `return;`, so a rule that rejected the literal unconditionally would reject every void function ever written. The exemption reaches the root and nothing below it, so `f(())` is still reported.

The repair the message offers follows the position, on the pattern A047 uses: a declaration is repaired by editing or deleting the declaration, while an expression standing where a value was required has no declaration to edit, so the same advice would send the author looking for one that is not there.

The rule generalizes a judgement the linker already makes on the extern path alone, where lowering an `external fn` signature fails for a parameter whose value type comes back empty. A049 applies it to every function and to the other carrier positions and moves it from link time to analysis; the link-time check stays in place as defence in depth, which is why "has no value representation" is the phrase both use. `spec` bodies are covered for A048's reason. The binding half reads the type the checker recorded rather than the raw annotation: lowering is total, so a `let` with no type child is given a synthesized unit type node — unreachable from a clean parse, because the grammar requires `: type`, but a rule reading the raw annotation would be one grammar relaxation away from rejecting every binding in the language.

### Unnamed Parameters (errors)

| ID | Struct | Severity | What it checks |
|----|--------|----------|----------------|
| A050 | `UnnamedParameter` | error | a parameter of a function with a body declared as a bare positional type (`fn f(i32)`) rather than `name: T` or `_: T` |

A050 requires a parameter of a defined function to be written with a name or with `_`. The grammar admits three spellings, and on a function with a body the bare positional type says strictly less than `_: T` while occupying the same slot: it binds nothing, so the body has no name to read the value through; it cannot be labelled, so a call site that names its arguments has nothing to name it by; and where `_: T` is a deliberate declaration that the parameter exists and is not read, a bare `T` states nothing at all. It is also the grammar's fallback arm, which is what lets a forgotten annotation misparse — `fn f(x)` is a parameter whose *type* is `x`, not a parameter named `x`. Removing the weaker of two spellings for one concept is the direction A033 and A046 already take.

The check is a declaration walk over free functions, struct methods, `spec` functions, and methods of a struct declared inside a `spec`. The index counts the declared parameters from zero with a `self` receiver excluded, so `fn m(self, i32)` reports parameter 0 — the number the type checker already uses when it talks about an argument of that method, since the parameter lists those messages index into are built with the receiver filtered out. A method is rendered `Struct::method`. `external fn` is a documented non-scope: an extern declares an ABI signature with no body to read a parameter in, so a positional type is a complete statement of it, and it is the spelling the corpus uses. It is nonetheless the wrong form on an extern whose linked body *writes* through a parameter, because `mut` is a field of a named parameter alone and an unnamed one cannot carry the write-set contract A047 checks — a recommendation the rule does not enforce, since a read-only extern is a fine use of the bare form. Unlike A048 and A049, this rule is not a gate on an unimplemented feature: `_: T` is supported, and the bare form is rejected because one spelling for the concept is better than two.

## Diagnostic Output Format

```
<line>:<column>: <severity>[<rule_id>]: <message>
```

All diagnostics are sorted by source location (line, then column) before display. Messages follow a `what; why; how` structure separated by semicolons.

Example output for two violations:

```
1:5: error[A001]: break statement is only valid inside a loop body; if you intended to exit the function, use 'return'
3:10: error[A002]: break statement is not allowed inside a 'forall' block; break would interfere with the path exploration required for formal verification; move the break outside the 'forall' block
```

## Usage

```rust
use inference_analysis::{analyze, errors::{AnalysisErrors, AnalysisResult}};
use inference_type_checker::typed_context::TypedContext;

fn run(ctx: &TypedContext) {
    match analyze(ctx) {
        Ok(result) => {
            // Compilation can continue. result.warnings() and result.infos()
            // may still contain non-fatal findings.
            if result.has_findings() {
                eprintln!("{result}");
            }
        }
        Err(errors) => {
            // At least one hard error. errors.errors() is guaranteed non-empty.
            // errors.warnings() and errors.infos() may also be populated.
            eprintln!("{errors}");
            std::process::exit(1);
        }
    }
}
```

The orchestration layer in `core/inference/src/lib.rs` wraps this call and re-exports `analyze()` as part of the public compiler API.

## How to Add a New Rule

1. Create `src/rules/my_rule.rs` using the `rule!` macro (copy an existing simple rule such as `break_outside_loop.rs` as a starting point).
2. Add `pub mod my_rule;` to `src/rules/mod.rs`.
3. Add `&MyRule` to the slice in `all_rules()`.
4. Add a matching `AnalysisDiagnostic` variant to `errors.rs` with `rule_id()` returning the new ID.
5. Update the integration test in `lib.rs` (`rule_ids_match_diagnostic_rule_ids`) to include the new variant.

Rules that need scoping logic beyond `loop_depth` and `nondet_depth` can implement a custom traversal. See `InfiniteLoopWithoutBreak` (`src/rules/infinite_loop_without_break.rs`) for an example: it uses `for_each_function_body()` directly and provides its own recursive descent so that `break` inside a nested loop is not counted as a break for the outer loop.

## The Shared Walker

`walk_function_bodies()` drives traversal for most rules. It visits every statement in every function body (including struct methods, spec functions, and module-level functions) in pre-order and calls a `dyn FnMut` visitor with two arguments:

- `StmtId` — the current statement
- `&WalkContext` — read-only snapshot of traversal state:
  - `loop_depth: u32` — incremented when entering a `Loop` body, decremented on exit
  - `nondet_depth: u32` — incremented when entering a non-det block, decremented on exit
  - `nondet_block_kind: Option<&'static str>` — label of the innermost non-det block (`"forall"`, `"exists"`, `"assume"`, or `"unique"`)

Using `dyn FnMut` instead of a generic parameter avoids monomorphization cost when the number of rules grows.

The walker module also exposes several type-inspection helpers used by multiple rules:

- `array_nesting_depth(kind)` — returns how many array layers deep a type is (`[[i32; 3]; 2]` → 2, `i32` → 0)
- `has_compound_fields(ctx, kind)` — returns true if a struct or array type contains fields that are themselves structs, arrays of structs, or multidimensional arrays; used by A026, A027, and A028
- `fieldless_struct_name(ctx, kind, module_path)` — returns the bare name of the field-less struct a type is, or is an array of at any depth; resolves all four type carriers (canonical `Struct`, bare `Custom`, and both `::`-qualified forms) so a same-named struct in another file is not picked up by its bare name; used by A045
- `is_compound_return_call(arena, expr_id, ctx)` — returns true when an expression is a function call that returns a compound type (struct or array); used by A016, A017, and A018
- `separated_negated_literal(arena, expr_id)` — returns the numeric literal a `-` is applied to but written apart from, measuring separation on offsets rather than source text; used by A046, which rejects the spelling, and by A022, which skips exactly those literals so the two cannot drift apart
- `innermost_element(kind)` — returns the element type at the bottom of any array nesting (`[[i32; 2]; 3]` → `i32`), and a non-array kind unchanged; used by A048 and A049, which ask their question of the element and report the annotation that carries it once rather than once per layer

The position strings the value-rejecting rules name in their messages (`"a struct literal"`, `"the type of a parameter"`, …) live in `src/rules/position.rs` so A045, A048 and A049 cannot drift into naming the same position two ways. The values are asserted by the message tests, so they must not change without those.

## Testing

Unit tests live alongside each source file in `src/`. The integration test `rule_ids_match_diagnostic_rule_ids` in `lib.rs` asserts that:
- The number of registered rules equals the number of `AnalysisDiagnostic` variants.
- Each rule's `id()` matches the `rule_id()` of its corresponding diagnostic variant.

End-to-end tests that compile `.inf` source and assert on diagnostic output live in `tests/src/analysis/` (part of the `inference-tests` crate). Run them with:

```
cargo test -p inference-tests analysis
```

Test files are organized by rule group:

| File | Rules covered |
|------|---------------|
| `rules_a006_a011.rs` | A006–A011 (uzumaki, missing return, lint warnings) |
| `rules_a012_a022.rs` | A012–A022 (codegen restrictions, literal range) |
| `rules_a023.rs` | A023 (uzumaki in reassignment) |
| `rules_a024.rs` | A024 (extern function calls) |
| `rules_a025.rs` | A025 (uninitialized variable) |
| `rules_a026_a028.rs` | A026–A028 (nested compound depth, uzumaki on nested structs, uzumaki on struct arrays) |
| `rules_a029_a030.rs` | A029 (compound literal in compound assign), A030 removal acceptance tests |
| `rules_a031.rs` | A031 (unsupported compound return expression) |
| `rules_a035.rs` | A035 (direct and mutual/indirect recursion) |
| `rules_a036.rs` | A036 (cumulative stack depth exceeded) |
| `rules_a037.rs` | A037 (constant array index out of bounds) |
| `rules_a038.rs` | A038 (uzumaki on compound struct field) |
| `rules_a039.rs` | A039 (struct uzumaki passed as function argument) |
| `rules_a040.rs` | A040 (uzumaki as compound element of an array literal) |
| `rules_a041.rs` | A041 (duplicate function-local name across sibling blocks) |
| `rules_a042.rs` | A042 (non-deterministic construct outside a `spec` declaration) |
| `rules_a044.rs` | A044 (shift count literal out of range) |
| `rules_a045.rs` | A045 (field-less struct values) |
| `rules_a046.rs` | A046 (unary minus separated from the literal it negates) |
| `rules_a047.rs` | A047 (compound argument at a `mut` `external fn` parameter) |
| `rules_a048.rs` | A048 (`string` values) |
| `rules_a049.rs` | A049 (unit values) |
| `rules_a050.rs` | A050 (unnamed parameter on a defined function) |
| `walker_tests.rs` | `walk_function_bodies`, `WalkContext` depth tracking |

## Dependencies

| Crate | Role |
|-------|------|
| `inference-ast` | AST arena types, node kinds, `Location` |
| `inference-type-checker` | `TypedContext` input to every rule |
| `inference-fn-key` | `FnKey` — shared canonical function identity used to key the call graph |
| `thiserror` | Derive `Error` for `AnalysisDiagnostic` |

## Current Limitations

1. The walker visits all statements but does not expose expression-level traversal. Rules that need to inspect expressions must do their own descent.
2. Rules are executed sequentially on a single thread. The infrastructure is designed for parallel execution (rules are `Send + Sync`) but parallelism is not yet enabled.
3. `AssignToImmutable` and `VariableShadowed` remain in the type checker because they depend on scope state that the type checker tracks but the analysis pass does not replicate.
4. Nested compound type support (A026) limits nesting depth to one level. Structs whose fields are structs or arrays of structs are permitted; structs whose fields contain further nested structs or arrays-of-structs are rejected. This bound matches what the code generator can lower.
