# Arithmetic Overflow in WASM Codegen

Inference compiles to WebAssembly. WebAssembly integer arithmetic wraps silently on overflow for add, subtract, and multiply. This document explains what that means exactly, how it differs from other languages, what Inference's codegen does, and why this matters for a language that targets formal verification.

Inference does not pass that behavior through. `+`, `-`, `*` and unary `-` trap when the result leaves the operand type, and `wrapping(e)` is how a program asks for the machine's answer instead: it makes every one of those operators written between its parentheses wrap. `checked(e)` names the other direction, which is what an operator no annotation encloses already does. [Checked and Wrapping Arithmetic](#checked-and-wrapping-arithmetic) is the surface, the guard shapes it emits, and what a proof can say about them. The lowering examples before it are the wrapping ones, and each carries the annotation that selects them.

## The Problem

Every fixed-width integer type has a representable range. When an arithmetic result falls outside that range, the implementation must choose: trap, wrap, saturate, or invoke undefined behavior. The choice is not cosmetic — it determines what programs mean, what optimizers are allowed to do, and what formal proofs must encode.

For a language like Inference, whose core value proposition is verification via Rocq translation, the overflow semantics of every arithmetic operation must be precisely defined. An underspecified overflow behavior makes it impossible to write a sound proof about any computation that could overflow.

## WebAssembly Overflow Semantics

The [WebAssembly specification, section 4.3.2](https://webassembly.github.io/spec/core/exec/instructions.html#numeric-instructions) defines integer arithmetic as follows:

> The result is computed modulo 2^N, where N is the bit width.

This is a complete and unconditional specification. WASM integer add, subtract, and multiply never trap. They always produce a result by discarding the bits that do not fit. There is no undefined behavior, no implementation-defined behavior, no signal.

### Wrapping Instructions

The following instructions wrap silently on overflow:

| Instruction | Behavior on Overflow |
|-------------|----------------------|
| `i32.add` | Result mod 2^32 |
| `i32.sub` | Result mod 2^32 |
| `i32.mul` | Result mod 2^32 |
| `i64.add` | Result mod 2^64 |
| `i64.sub` | Result mod 2^64 |
| `i64.mul` | Result mod 2^64 |

In two's complement, "result mod 2^N" is the same as taking the low N bits of the mathematical result. `i32::MAX + 1` produces `i32::MIN`. `i32::MIN - 1` produces `i32::MAX`. Multiplying `i32::MAX * 2` produces `-2`. These are not errors — they are the defined results.

### Trapping Instructions

Division and remainder behave differently:

| Instruction | Trap Condition |
|-------------|----------------|
| `i32.div_s` | Divisor is zero; or `(i32::MIN, -1)` (signed overflow) |
| `i32.div_u` | Divisor is zero |
| `i32.rem_s` | Divisor is zero (but `(i32::MIN, -1)` does **not** trap — remainder is 0) |
| `i32.rem_u` | Divisor is zero |
| `i64.div_s` | Divisor is zero; or `(i64::MIN, -1)` (signed overflow) |
| `i64.div_u` | Divisor is zero |
| `i64.rem_s` | Divisor is zero (but `(i64::MIN, -1)` does **not** trap) |
| `i64.rem_u` | Divisor is zero |

The `div_s (MIN, -1)` trap is the one case where division produces a result that cannot be represented: `i32::MIN / -1` would be `2147483648`, which exceeds `i32::MAX`. WASM traps rather than wrap here. The corresponding `rem_s (MIN, -1)` does not trap because the mathematical remainder is 0, which is representable.

This asymmetry between `div_s` and `rem_s` on `(MIN, -1)` is a [common source of confusion](https://github.com/WebAssembly/spec/issues/144) for compiler authors and is worth explicit documentation in any codebase that lowers division.

### Negation

WASM has no integer negation instruction. Negation is computed as `0 - x` using `i32.sub` or `i64.sub`. Because subtraction wraps, negating the minimum value of a signed type wraps back to itself:

```
0 - i32::MIN = 0 - (-2147483648) = 2147483648 mod 2^32 = -2147483648
```

Negating `i32::MIN` gives `i32::MIN`. This is correct two's complement behavior and is the WASM-mandated result.

### Shift Instructions

Shift amounts are masked to the bit width of the value being shifted. For `i32`, the shift amount is masked to 5 bits (values 0–31). For `i64`, the shift amount is masked to 6 bits (values 0–63). Shifting by the full bit width is not a trap — it produces a shift by 0, which is the identity. This is specified in [section 4.3.2](https://webassembly.github.io/spec/core/exec/instructions.html#numeric-instructions) of the WASM specification.

## Inference's Current Approach

A `+`, `-`, `*` or unary `-` written inside a `wrapping(...)` inherits WASM's semantics: it emits the bare arithmetic instruction with no overflow guard. Everywhere else — which is everywhere the source has not asked for the wrap — the same operator emits a guard sequence that traps instead, in both compilation modes; see [Checked and Wrapping Arithmetic](#checked-and-wrapping-arithmetic). The lowerings shown in the rest of this section are the wrapping ones.

Division stands apart from both spellings — signed division overflow (`MIN / -1`) traps at every width whatever encloses it, natively for `i32`/`i64` and via a compiler-added guard for the narrow types (see [Division and Modulo](#division-and-modulo)). Separately, a dynamic (runtime-index) array access and a failing `assert` emit their own runtime traps.

### Binary Expression Lowering

`lower_binary_expression` in `core/wasm-codegen/src/compiler.rs` dispatches to the appropriate WASM instruction based on the left operand's type. Where the operator's effective mode is wrapping it emits that instruction and nothing else:

```wat
;; Inference: return wrapping(2147483647 + 1)
i32.const 2147483647
i32.const 1
i32.add          ;; wraps to -2147483648 — no trap, no check
return
```

No overflow check precedes the `i32.add`. The result is exactly what WASM's specification says: `-2147483648`. Drop the annotation and this particular expression never reaches code generation: both operands are constants and the sum leaves `i32`, which analysis rule **A052** refuses. Written over values the compiler cannot fold, the same addition compiles to a guard and traps.

### Negation Lowering

`lower_prefix_unary_expression` lowers the unary negation operator `-x` as `0 - x`:

```wat
;; Inference: return wrapping(-min)  (where min = i32::MIN)
i32.const 0
local.get $min   ;; pushes -2147483648
i32.sub          ;; 0 - (-2147483648) wraps to -2147483648
return
```

Negating `i32::MIN` produces `i32::MIN` — the one input at which negation has no representable result, and the one input an unmarked `-x` traps on. This is verifiable against the golden WAT output in `tests/test_data/codegen/wasm/arith_overflow/arith_overflow.wat`:

```wat
(func $i32_neg_min_wrap (;6;) (type 6) (result i32)
  (local $min i32)
  i32.const -2147483648
  local.set $min
  i32.const 0
  local.get $min
  i32.sub
  return
  unreachable
)
```

### Unsigned Types and Bit-Pattern Reinterpretation

WASM has no unsigned integer types. All integer values are stored in `i32` or `i64` slots and interpreted as unsigned or signed by the individual instruction. Inference maps `u32` to `i32` and `u64` to `i64` by reinterpreting the bit pattern.

Unsigned literals use `.cast_signed()` in `lower_number_literal`:

```rust
// u32 literal: parse as u32, reinterpret bits as i32
let val = number_literal.value.parse::<u32>()
    .expect("Failed to parse unsigned 32-bit integer literal")
    .cast_signed();
func.instruction(&Instruction::I32Const(val));
```

`u32::MAX` (4294967295) has the bit pattern `0xFFFFFFFF`. When reinterpreted as a two's complement `i32`, that is `-1`. The wrapping behavior is identical: `i32.add(-1, 1)` produces `0`, which is the correct WASM result for `u32::MAX + 1`.

### Sub-i32 Types (i8, i16, u8, u16)

Sub-i32 types are promoted to `i32` for all arithmetic. The WASM `i32.add` instruction operates on the full 32-bit value, so a result that would overflow the sub-type's declared width is possible immediately after the raw operation.

Inference closes that gap by re-narrowing the result at the producing instruction, immediately after the operation and before it is stored to a local. `memory::emit_sub_i32_narrowing` (`core/wasm-codegen/src/memory.rs:652`) emits the shape appropriate to the type:

- **Signed (`i8`, `i16`)**: `shl <32-width>` then `shr_s <32-width>` — shifting the value up so the sub-type's sign bit lands in bit 31, then an arithmetic shift back down, which sign-extends from that bit. For `i8` this is `shl 24` / `shr_s 24`; for `i16`, `shl 16` / `shr_s 16`.
- **Unsigned (`u8`, `u16`)**: `and 0xFF` / `and 0xFFFF` — a zero-extending bitmask.

`i32.extend8_s`/`i32.extend16_s` would express the signed case more directly, but Inference does not use them. The historical reason was that the `wasm-to-v` translator had no case for those opcodes, so `shl`/`shr_s` was the only spelling that stayed translatable to Rocq; the translator now lowers all five sign-extension opcodes to `BI_unop t (Unop_extend n)`, so the constraint no longer binds and the two-instruction decomposition is simply what codegen still emits. Changing it would move every golden `.wasm` for no semantic gain.

This narrowing is emitted at every place a sub-i32 value is produced, not just arithmetic:

- Binary expressions (`lower_binary_expression`, `core/wasm-codegen/src/compiler.rs:4404`) — for every operator except comparisons (`Eq`/`Ne`/`Lt`/`Le`/`Gt`/`Ge`, which produce `bool`, not the operand's sub-type), `Mod`, `And`, `Or`, and `Shr`.
- Unary negation (`core/wasm-codegen/src/compiler.rs:4439`) and unary bitwise-not (`core/wasm-codegen/src/compiler.rs:4456`).
- A scalar uzumaki (`@`) draw of a narrow type — see [Sub-i32 Truncation](#sub-i32-truncation) below; `bool` and enum draws get an analogous `and 1` / `rem_u <variant count>` constraint rather than this mask/shift shape, since their domains aren't sub-i32 integer ranges.

Signed division is the one producer whose *promoted* result can fall outside the narrow type's range in a way this re-narrowing would silently mask rather than merely truncate: for `(MIN, -1)` the promoted quotient is `+128`/`+32768`, which the `shl`/`shr_s` re-narrowing would wrap back to `MIN` — the wrong answer, with no failure signal. That case is caught by an overflow guard emitted *before* the re-narrowing, so division overflow traps instead of wrapping. See [Division and Modulo](#division-and-modulo).

The current behavior therefore matches C's integer promotion rule (arithmetic is done in the promoted width) *and* truncates back to the sub-type's width immediately afterward, so a sub-i32 local never holds a value outside its declared range. Where the operator is guarded rather than wrapping, this re-narrowing is also the overflow test: the guard runs it, compares, and traps exactly when it would have changed the promoted result — see [The guard shapes](#the-guard-shapes).

### Division and Modulo

**Division overflow traps at every width**, and unlike the four arithmetic operators it does so under either annotation and under none: `checked(...)` and `wrapping(...)` never govern `/` or `%`. Divide-by-zero and remainder-by-zero pass through as native WASM traps at every width, and so does signed division's single overflow case, `MIN / -1`.

For `i32`/`i64`, wasm's own `div_s` already traps on `(MIN, -1)`. The narrow signed types (`i8`/`i16`) divide in the promoted i32 width, where the overflowing quotient (`+128`/`+32768`) is representable — so no wasm trap fires — and the mandatory re-narrowing would silently sign-wrap it back to `MIN`. The compiler closes that gap with a guard on the promoted quotient, emitted after `div_s` and before the re-narrowing:

```wat
i32.div_s
local.tee $scratch    ;; single-evaluate the promoted quotient
i32.const 128         ;; 32768 for i16
i32.eq
if (empty)
  unreachable
end
local.get $scratch
```

A single equality is exhaustive because the operands are canonical sign-extended values (ABI entry normalization, producing-instruction re-narrowing, and sign-extending loads all keep a narrow local in range), so `|q| <= |a| <= 2^(w-1)` and the promoted quotient equals `+2^(w-1)` only for `(MIN, -1)`. Running the guard *after* `div_s` preserves the native divide-by-zero trap. The guard is emitted in both compile and proof modes, so a proof carries the same cannot-trap obligation at every width.

`MIN % -1` is `0` at every width — the mathematically correct remainder, always representable — and is intentionally **not** trapped; only `x % 0` traps (natively). The trap *kind* differs by width: the narrow guard traps as `unreachable`, while `i32`/`i64` report wasm's native integer-overflow trap. It is the trap-or-not contract, not the trap code, that is width-uniform.

### Exported ABI Parameter Guards

At the WebAssembly ABI boundary a host may pass any i32 bit pattern for a parameter, so an exported function normalizes or validates each parameter in its prologue. The rule is: normalize where a host convention already assigns every wire value a meaning, and trap where the domain is partial. A narrow integer parameter takes its low bits (the C ABI) and a `bool` takes truthiness (any nonzero is `true`) — both are total maps, so they are normalized silently. An enum parameter is different: only tags `0..N-1` name a variant, so a tag `>= N` names nothing under any convention, and the prologue rejects it with `i32.const N; i32.ge_u; if; unreachable; end` (a negative tag arrives as a huge unsigned value and is caught by the same unsigned compare). A variantless enum is uninhabited, so its guard (`>= 0`) traps on every host call.

This is the same `rem_u` opcode the uzumaki draw uses to constrain an enum draw to `0..N-1` (see [Sub-i32 Truncation](#sub-i32-truncation)), used in a different context. A non-deterministic draw is provenance-free: it needs only a surjection onto the variant domain, and `rem_u N` is a valid one. A host-supplied tag is a concrete input with provenance, so mapping it with `rem_u N` would silently relabel it as a variant the host never named — inventing data. Concrete out-of-domain inputs are therefore trapped, not folded.

## Checked and Wrapping Arithmetic

`checked(e)` and `wrapping(e)` are prefix expression forms that fix how the arithmetic written inside `e` behaves on overflow. `checked(e)` makes every `+`, `-`, `*` and unary `-` between its parentheses trap when the mathematical result leaves the operand type; `wrapping(e)` makes them compute the result modulo two to the power of the width, which is what the bare WebAssembly operator does. The expression has the type of `e`.

Both words are reserved. Nothing in a program may be named `checked` or `wrapping` — not a variable, a constant or a function, not a struct field, a method or an enum variant, and not a source file, whose stem becomes a module path segment.

### What an annotation governs

The rule is lexical and it is about the parentheses. An annotation reaches the `+`, `-`, `*` and unary `-` written between its own, and no others:

- **They nest, innermost first.** In `checked(a * wrapping(b + c))` the multiply traps and the addition wraps. The inner annotation restores wrapping inside a region the outer one had made checked.
- **They do not reach into a callee.** `wrapping(f(x))` annotates the call, not `f`'s body, which keeps whatever its own source says. This is the mistake authors of hashes, mixers and pseudo-random generators make most often, so the compiler rejects an annotation with no operator to govern outright (analysis rule **A053**) rather than letting it read as a guarantee nothing backs.
- **A negative number is one token, not a negation.** `-2147483648` carries its own sign, so `wrapping(-2147483648)` governs no operator and is the same A053 error.
- **`/`, `%`, the shifts and the bitwise operators are never governed.** Signed division traps on its own at the one quotient a width cannot hold, `%` cannot overflow at any width, and a shift discards the bits it moves out by definition.
- **The inner expression may be any scalar.** `checked(a + b > c)` is a boundary shape authors write, and the `bool` the comparison yields is a fine thing to annotate — the arithmetic being governed is inside it. A struct, an array, a unit value or a function has no arithmetic to govern and is a type error.
- **A `const` initializer is an ordinary expression.** `const SCALE: i32 = wrapping(2147483647 + 1);` is how a wrapped constant is written.

An operator no annotation encloses has the language's default mode, which is checked. `wrapping(...)` is therefore the opt-out — the one way to ask for the machine's own answer — and a `checked(...)` that no `wrapping(...)` encloses names the mode already in force and changes nothing. That is reported as a warning (**A054**) rather than an error, because which spelling is redundant is decided by the default rather than by the expression.

### The guard shapes

A guarded operator is emitted inline: the operation, then a test of what it produced, then `unreachable` on the failing side. Nothing is factored into a helper function, because a synthetic function would renumber every index an obligation names its callee by.

There is no wider width to compute a candidate result in — an `i64` product has nowhere to be checked — so each row saves its operands or its result in a scratch local and recomputes the question from those. The scratch is one shared pool per function, reserved before the body is emitted and holding at most three slots per width class present; a function whose governed operators are all inside a `wrapping(...)`, or that has none, reserves nothing.

| Operator | Overflow test | Added code bytes |
|----------|---------------|------------------|
| signed `+` at i32/i64 | `(r <s a) != (b <s 0)` | +23 |
| signed `-` at i32/i64 | `(r <s a) != (b >s 0)` | +23 |
| signed `*` at i32/i64 | `b != 0 && ((b == -1 && a == MIN) \|\| (r /s b) != a)` | +52 / +57 |
| unary `-` at i32/i64 | `r == MIN` | +15 / +20 |
| unsigned `+` at u32/u64 | `r <u b` | +13 |
| unsigned `-` at u32/u64 | `a <u b`, tested before subtracting | +15 |
| unsigned `*` at u32/u64 | `b != 0 && (r /u b) != a` | +30 |
| any of them at i8/i16/u8/u16 | re-narrowing the promoted result changes it | +13 |

Each width class the pool holds costs a further 2 bytes of locals declarations. The figures are instruction bytes over the same source with the operator written `wrapping(...)`. There is no unsigned negation row because there is no unsigned negation: `-x` at a `u32` is a type error, not an operation that could overflow.

Four things about that table are worth a sentence each.

**Multiply is checked by dividing back.** There is no wider width and no conversion opcode in the emitter's vocabulary, so the product is recovered by `r /s b` and compared against `a`. That check has two inputs it must not be handed: `b == 0`, where the division traps, and `(a, b) = (MIN, -1)`, where the division would reach WebAssembly's own `integer overflow` trap. Both arms are therefore explicit and both come before any division runs. Without the `b == -1` arm the multiply would still refuse the overflow, but by the wrong trap.

**Narrow widths are cheap because the promoted value is still there.** An `i8` operator runs at `i32` and is re-narrowed afterwards, so overflow is exactly "re-narrowing changes the result". The guard runs the same re-narrowing every narrow operation already ends with and leaves its value behind, which is why the lowering site does not emit that re-narrowing a second time.

**Every guard traps through `unreachable`.** That is a design constraint, not an accident of the shapes: the Rocq contract enumerates the roles `BI_unreachable` is emitted in, and a guard that reached a native trap instead would be a trap kind that table does not describe. The execution matrix over the catalogue asserts the trap *kind* for exactly this reason.

**The annotation and the default are two spellings of one thing.** `wrapping(e)` emits what the operator emitted before either keyword existed and nothing else, and `checked(e)` emits what an operator with no annotation around it emits. Both directions are pinned by compiling one source under each fallback and comparing bytes, which is also what makes the figures above measurable: each is the difference between one module and the module the same source produces with `wrapping(...)` written around the operator. The catalogue fixture is the case in point: it writes `wrapping(...)` only where a row needs modular arithmetic to be about something else, and every other guard in it comes from the default alone.

Here is the signed `+` row as the golden emits it at i32:

```wat
local.get $a
local.get $b
local.set 3      ;; keep b
local.tee 2      ;; keep a
local.get 3
i32.add
local.tee 4      ;; keep r
local.get 2
i32.lt_s         ;; r <s a
local.get 3
i32.const 0
i32.lt_s         ;; b <s 0
i32.ne           ;; the two disagree exactly when the sum overflowed
if
  unreachable
end
local.get 4
```

### Both modes, and no dial

A guard is emitted in Compile and Proof mode alike. Nothing gates it on the mode, because the Rocq translation has to describe the program that ships: a proof-mode-only guard would be a proof about a module nobody runs, and a compile-mode-only one would be a module nobody proved. The same reasoning rules out a build setting: which operator `a + b` denotes is a property of the language, so it is absent from the compiler's options, the CLI, the manifest and the `infs`↔`infc` ABI. One source compiles to one program.

### The reproducer

Q20.20 fixed point multiplies two scaled numbers and divides the product back by the scale. At `4194304000` the true product needs 64 bits it does not have:

```inference
pub fn fixmul(a: i64, b: i64) -> i64 {
  const ONE: i64 = 1048576;
  return a * b / ONE;
}

pub fn run() -> i64 {
  const H: i64 = 4194304000;
  return fixmul(H, H);
}
```

`run` traps. Write the multiply as `wrapping(a * b)` and it returns -814970044416 for a true value of 16777216000000, which is the failure mode this design exists to end. The language cannot compute the right answer here — there is no wider type, no cast and no widening multiply — so what the guard buys is a loud failure in place of a quiet wrong number.

### What a proof can say about it

An overflow guard is the trap site that makes a no-overflow claim expressible, and `HA_app_ok` — the atom a bare call statement in a specification body emits — is what carries it. Downstream that atom unfolds to total correctness: the callee's body must reduce to a value stack, and a trapped run reduces to `AI_trap` instead. So a `forall` specification that bounds its operands with `assume` and then calls `fixmul` states exactly that no admitted pair overflows:

```inference
spec OverflowRealization {
  fn fixmul_is_realized() forall {
    let a: i64 = @;
    let b: i64 = @;
    assume { assert(-3037000499 <= a && a <= 3037000499); }
    assume { assert(-3037000499 <= b && b <= 3037000499); }
    fixmul(a, b);
  }
}
```

The guard is the whole of what makes that claim informative. Over a `fixmul` whose multiply is written `wrapping(a * b)` the emitted obligation is byte-for-byte the same text and is unconditionally true: with no trap in the callee every argument pair reduces to a value, and each conjunct of the envelope can be ignored. An `HA_app_ok` over wrapping arithmetic constrains nothing about overflow, and its silence must not be read as coverage.

The envelope is exact rather than generous. 3037000499 is the largest N with N² inside `i64`, and an `i64` declaration contributes only a width to the obligation, never a range, so the `assume` is the sole source of the bound and both halves of each pair are load-bearing. `core/wasm-codegen/docs/specification-obligations.md` sets out the emitted obligation and how it is read.

Two positions refuse arithmetic rather than proving anything about it, both in specification bodies. A `forall` or plain body becomes an obligation *term*, whose `+`, `-` and `*` are the modular machine operators with no second operator downstream for an annotation to select, so either annotation there is refused (**P017**, first wording) instead of being silently dropped. An `exists`- or `unique`-quantified body is the opposite case: it is compiled and the judgment reduces it, so an operator whose effective mode traps puts a trap on the path being reduced, which makes the theorem false rather than narrower — refused by the same code's second wording, with `wrapping(...)` accepted throughout as the remedy. Because the judgment reduces callee activations too, a retained body that *reaches* a guarded function through calls is refused as well (**P018**), and a merged body a library supplied is judged the same way at link time.

Read together: a `forall` body states a claim and is never run, so its arithmetic is the wrapping operator and cannot be annotated; an `exists` or `unique` body is run by its own judgment, so its arithmetic has to say which operator it is.

### Trap attribution at run time

Every trap this compiler emits reports the same thing. A bounds check, a narrow division guard, a failing `assert`, an exported entry's enum tag guard and an overflow guard all surface as `wasm trap: wasm 'unreachable' instruction executed`, and no WebAssembly mechanism distinguishes them at the instruction level short of DWARF, which this compiler does not emit.

What is available is function-level attribution, for free. The compiler emits a name section, so a `wasmtime` backtrace names the function that trapped and gives a code offset within it. The recipe is: read the frame name from the backtrace, then disassemble that function and look at the offset. Note that `[build.wasm-opt]` strips the name section, so an optimized artifact gives you an index instead of a name.

Letting an overflow guard reach WebAssembly's native `integer overflow` trap would produce a better message for free, and it is deliberately not done: that would add a trap shape the Rocq contract's role table does not describe, which is a poor trade for a string.

## Comparison with Other Languages

| Language | Add / Sub / Mul | Division by Zero | Notes |
|----------|----------------|-----------------|-------|
| C / C++ | Undefined behavior (signed) | Undefined behavior | Optimizer may delete overflow branches entirely |
| Rust (debug) | Panic via overflow check | Panic | Checks inserted by `rustc` |
| Rust (release) | Wrapping (two's complement) | Panic | `wrapping_add` available explicitly |
| Java | Defined wrapping | `ArithmeticException` | Specified by JLS §15.17 |
| Go | Defined wrapping | Panic | Specified by Go language specification |
| Zig (safe) | Panic via safety check | Panic | `@addWithOverflow` available explicitly |
| Zig (unsafe) | Wrapping | Panic | `+%` wrapping operators available |
| WASM | Defined wrapping | Trap | Full specification in WASM core spec §4.3.2 |
| Inference | Trap, or defined wrapping inside `wrapping(...)` | Trap | The mode is fixed per expression by `checked(e)` / `wrapping(e)`; an unannotated add/sub/mul/neg traps. Signed division overflow traps at every width (narrow types via a compiler-added guard) |

Inference's default is Rust's debug behavior and Zig's safe-mode behavior, with the difference that no build profile takes it away: there is no release mode in which the same source stops trapping. Its spelling of the escape is closest to Zig's `+%` and Rust's `wrapping_add` — both behaviors are written down, and neither is reached by a build setting — but it scopes the choice to a parenthesized expression rather than to an operator or a method call, which is the only form available in a language with no methods on primitives and no width-generic functions. It also differs from every row above in that the choice is not a safety switch: a proof is what the trap is for, and [What a proof can say about it](#what-a-proof-can-say-about-it) is where that cashes out.

The critical distinction is between *defined* behavior and *undefined* behavior. C's undefined behavior for signed overflow means the optimizer is allowed to assume overflow never occurs, leading to deleted bounds checks, eliminated branches, and silent wrong results. WASM has no such latitude — the specification fully defines every overflow result, making the behavior predictable regardless of optimization level.

## Compiler Patterns

### rustc to WASM

When Rust compiles to `wasm32-unknown-unknown` in debug mode, it inserts overflow checks for every arithmetic operation on integer types. The check is implemented via the `checked_add` / `checked_sub` / `checked_mul` intrinsics in MIR: each operation returns `Option<T>`, and if the value is `None` (overflow occurred), execution falls through to a `panic` call. On WASM, that panic call lowers to `unreachable`. The net effect is a conditional `unreachable` that fires on overflow, over the same signed-addition predicate Inference's own row uses — the sum's sign disagrees with the sign the addend predicted:

```wat
;; Conceptual structure of a debug-mode signed overflow check for i32 + i32.
;; Actual rustc output goes through a two-result MIR operation and block
;; layout that this flattens; the predicate is what carries over.
local.get $a
local.get $b
i32.add
local.tee $result
local.get $a
i32.lt_s          ;; r <s a
local.get $b
i32.const 0
i32.lt_s          ;; b <s 0
i32.ne            ;; the two disagree exactly when the sum overflowed
if
  unreachable     ;; panic!("attempt to add with overflow")
end
local.get $result
```

In release mode, `rustc` omits the check and emits a bare `i32.add`. The programmer can opt into explicit wrapping via `i32::wrapping_add()`, which always emits a bare `i32.add` regardless of build profile.

The Rust standard library also provides `i32::checked_add()` (returns `Option<i32>`) and `i32::saturating_add()` (clamps to the boundary), all of which lower to distinct WASM instruction sequences.

### Clang / LLVM to WASM

C's undefined behavior for signed overflow is an optimizer license. When Clang targets WASM with `-O2` or higher, the optimizer may hoist, fold, or eliminate computations on the assumption that signed overflow never occurs. The resulting WASM still wraps at runtime — but the sequence of WASM instructions may not correspond to what the C source code appears to request, because the optimizer has transformed it under the UB assumption.

Unsigned overflow in C is defined wrapping, so Clang emits bare WASM arithmetic for unsigned types at all optimization levels.

`-fwrapv` disables the optimizer's signed overflow assumption, making both signed and unsigned arithmetic lower to bare WASM arithmetic instructions without transformation.

### Zig to WASM

In Zig's safe build mode (`-ODebug` or `-OSafeRelease`), every integer arithmetic operation is accompanied by an overflow check. The check is a `@addWithOverflow` intrinsic that returns a struct of `{value, overflow_flag}`. If the overflow flag is set, Zig calls its panic handler, which in a WASM context emits `unreachable`. In unsafe mode (`-OReleaseSmall`, `-OReleaseFast`), bare WASM arithmetic is emitted. Zig also provides explicit wrapping operators (`+%`, `-%`, `*%`) that unconditionally emit bare WASM arithmetic, mirroring Rust's `wrapping_add` pattern.

## Formal Verification Implications

Overflow behavior is not optional context for formal verification — it is a load-bearing assumption in every arithmetic proof.

### Modeling Integer Arithmetic in Coq

Coq's standard library provides `Coq.ZArith.BinInt` for arbitrary-precision integers (`Z`) and `Coq.NArith.BinNat` for natural numbers (`N`). These are unbounded and do not model machine overflow. To reason about WASM arithmetic, the Rocq translator must encode the modular arithmetic explicitly.

CompCert's `Integers.v` provides a battle-tested model for this. It defines machine integer types as records containing a value field bounded by the bit width, with all arithmetic operations defined as mathematical operations followed by `unsigned z mod (2^wordsize)`. The key lemma is:

```coq
Lemma add_unsigned: forall x y,
  add x y = repr (unsigned x + unsigned y).
```

where `repr n = n mod 2^wordsize`. This is the Coq encoding of WASM's wrap-on-overflow guarantee.

For Inference's WASM-to-Rocq translation, every `i32.add` in the WASM binary must be translated to `Int32.add` (or equivalent), which encodes the modular semantics. A translation that maps `i32.add` to Coq's `Z.add` would be unsound — it would allow the proof to assume no overflow when the runtime behavior does wrap.

### Proof Obligations for Overflow-Free Code

Which of two jobs a proof faces is decided by the source, not by the prover:

1. Where the operator is guarded, prove that no overflow can occur — typically by establishing bounds on the inputs.
2. Where it is written `wrapping(...)`, account for the wrap: the result is defined, so the proof is possible, but it may be unexpected.

The first is the ordinary case. A function that receives an `i32` parameter and returns `param + 1` traps at `i32::MAX`, so a specification that calls it has to bound the argument below `i32::MAX` before the claim that the call is realized can close — and if that bound is missing or too wide, the claim is false rather than weaker. Written `wrapping(param + 1)` the function cannot trap, and what a proof has to discharge instead is `result == (input + 1) mod 2^32`, a different and weaker statement about the function's behavior.

### Overflow Checks as Proof Obligations

Inference treats a checked overflow check as both things at once: a runtime trap in the shipped binary, and the site an obligation is about. The obligation is not per operation and not a separate channel — it is `HA_app_ok` at a call, which the contract already carried for bounds. [What a proof can say about it](#what-a-proof-can-say-about-it) has the mechanism; three properties of it are worth stating here, because each rules out a design a reader might expect.

**The guard is emitted in both modes, not synthesized in proof mode.** A design in which compile mode emits bare arithmetic and proof mode generates an obligation would have the verifier describe a different program than the one that ships, which is precisely what the toolchain's byte-identity rule exists to prevent. The obligation says something about overflow only because the artifact it describes really traps.

**The claim is per call, not per operation.** Obligations are keyed by specification function, and the only carrier that speaks about a body's behavior is the application. So a function nobody names in a specification gets its trap and no theorem, and overflow is *proved* absent only at the argument vectors some specification actually named.

**Discharging it means proving the guard unreachable, which needs a bound.** The programmer supplies that bound as an `assume` envelope over the operands. Where the bound is missing or too wide, the obligation is not weaker — it is false, and it will not close.

## Current Implementation Details

Arithmetic lowering is in `core/wasm-codegen/src/compiler.rs`, and the guard catalogue it consults is `core/wasm-codegen/src/overflow_guard.rs`. There are exactly two source-level arithmetic lowering sites — the binary expression and the prefix unary one — which is what keeps the annotation's reach tractable: the several dozen other add/sub/mul emissions in the crate are frame, offset and index arithmetic reached through different code paths, so they are never inside an annotated expression and are excluded by construction rather than by a predicate.

`overflow_guard::guard_kind` is the single classifier. Both lowering sites ask it what to emit, and the pre-body pass that reserves the scratch pool asks it what to reserve, so a body cannot reach a guard whose scratch was never reserved.

**`lower_binary_expression`** dispatches on the left operand's `TypeInfoKind` using `is_i64_type()` and `is_unsigned_type()`. Where the effective mode is wrapping it emits a single WASM instruction with no surrounding guards:

```rust
OperatorKind::Add => {
    if is_i64 { Instruction::I64Add } else { Instruction::I32Add }
}
```

**`lower_prefix_unary_expression`** handles negation as `0 - x`:

```rust
UnaryOperatorKind::Neg => {
    // emit 0 constant (i32 or i64 depending on type)
    // lower the operand expression
    // emit Sub
}
```

**`lower_number_literal`** uses `.cast_signed()` for unsigned types to perform bit-pattern reinterpretation without value conversion:

```rust
// u32: parse bits as u32, reinterpret as i32 for WASM storage
let val = number_literal.value.parse::<u32>()
    .expect("Failed to parse unsigned 32-bit integer literal")
    .cast_signed();
func.instruction(&Instruction::I32Const(val));
```

The boundary suite is `tests/src/codegen/wasm/arith_overflow.rs`, and it carries eight cases twice. `i32::MAX + 1`, `i32::MIN - 1`, `i64::MAX + 1`, `i64::MIN - 1`, `u32::MAX + 1`, `i32::MAX * 2`, `-i32::MIN` and `-i64::MIN` are each written once with `wrapping(...)` over constants, asserting the modular value, and once over parameters with no annotation, asserting that the call traps on the vector its twin wraps at. Parameters are what makes the second half writable at all: a constant whose result leaves its type is refused by A052 before code generation, and the golden path runs analysis.

The guarded half is `tests/src/codegen/wasm/checked_arith.rs`, over a fixture carrying one exported function per catalogue row plus the reproducer. Its execution matrix runs each row at its boundary vectors and asserts the trap *kind* rather than merely that a trap occurred — the vector that makes that necessary is `(MIN, -1)` at a signed multiply, which is the one an unguarded round-trip check would fail through the machine's `integer overflow` trap instead. It also pins the vectors that must **not** trap, `2^62 * -2` landing exactly on `i64::MIN` among them.

## Limitations and Open Questions

### Reaching a Value the Language Cannot Compute

A guard turns a wrong answer into a trap; it does not produce the right one. The reproducer above needs the high half of a 64-by-64 product, and there is nothing in the language to hold it: no type wider than `i64`, no cast operator, and no widening multiply. An author who needs that value has to change the algorithm, not the annotation.

The narrow types are the same shape of problem at the other end. `i8` and `u8` arithmetic reaches its boundary early, and Rust's escape of widening each operand is unavailable — two different types never combine, and there is no cast to reach a wider one. Declare the arithmetic at the width the result needs; narrow types are for storage and boundaries.

### Constant Folding and Compile-Time Detection

An operation whose operands are known before the program runs can be evaluated at compile time and reported there when the result overflows. That needs no runtime guard: it is purely a front-end diagnostic, and it is the compile-time half of the trap.

Both halves are closed. Analysis rule **A022 (Literal out of range)** rejects `let a: i8 = 200` at compile time, because 200 exceeds `i8::MAX` (127) and the value could never round-trip through its declared type. **A052 (Constant arithmetic overflow)** takes the computed case: `let a: i8 = 127 + 1;` is refused, because both operands fold and the sum leaves the type it is performed at, so the operation is not a value the program computes but a trap it takes on every run that reaches it. Literals, a body's `const` bindings, parentheses, annotations and nested arithmetic fold; a `let` binding and a call do not. An operator inside a `wrapping(...)` folds modularly and is exempt, which after this rule makes `wrapping(127 + 1)` the only way to write a constant that wraps. What stays outside the front end is everything that does not fold — a parameter, a `let` binding, a value a call returns — and for those the emitted guard is the only answer. See [Static Analysis](static-analysis.md).

### Overflow Checks in Non-Deterministic Blocks

The framing this section used to carry — that checks would have to be stripped from `spec` blocks and preserved in proof mode — describes a compiler that no longer exists. A `forall` or plain specification function has no compiled body to strip anything from: it becomes an obligation and is left out of the module's function list entirely. What replaced the question is three rules, and each is about a different body.

**A body that becomes a term cannot be annotated (P017, first wording).** That covers `forall` and plain bodies, and a helper `fn` declared inside a `spec` block, which is translated into an obligation of its own. The obligation's `+`, `-` and `*` are the modular machine operators and there is no other operator downstream for the annotation to name, so both spellings would translate identically. They are refused rather than dropped in silence. The range belongs in an `assume` envelope over the operands, and the arithmetic whose overflow you want proved absent belongs in the executable function the specification claims the realization of.

**A retained body's arithmetic must be wrapping (P017, second wording).** An `exists`- or `unique`-quantified body *is* compiled, and its judgment reduces it. A trap on the path being reduced empties the observation set at the entry that reaches it, so the theorem becomes false rather than narrowed — and in a `unique` body it is worse than false, because a trapping choice shrinks the successful set and can make a uniqueness claim hold for a reason the source never states. `wrapping(...)` is accepted throughout such a body and is the remedy.

**A retained body may not reach a guard through a call (P018).** The judgment reduces callee activations too, so a guard one call deep is the same trap. The walk uses code generation's own call resolution, re-scoped at each hop, counts a callee it cannot resolve as guarded, and skips `external fn` callees — the compiler never sees a dependency's bytes. A merged body is judged instead at link time, off the `inference.checked` section each input carries.

### Sub-i32 Truncation

Sub-i32 truncation after arithmetic is implemented (see [Sub-i32 Types](#sub-i32-types-i8-i16-u8-u16) above); for an `i8` addition written `wrapping(...)`, the emitted sequence is:

```wat
local.get $a     ;; i8 stored as i32
local.get $b     ;; i8 stored as i32
i32.add
i32.const 24     ;; shl/shr_s width for i8
i32.shl
i32.const 24
i32.shr_s        ;; sign-extend from 8 bits without the sign-ext proposal
```

The last producer that did not follow this convention was the scalar uzumaki (`@`) draw: the draw opcode always yields a full-width value, so a narrow-typed `let x: i8 = @;` previously left the drawn value ranging over all of `i32`, not just `-128..127`. `emit_uzumaki_domain_constraint` (`core/wasm-codegen/src/compiler.rs:4196`) now closes this by emitting the same mask / `shl`+`shr_s` shapes immediately after the draw for `i8`/`u8`/`i16`/`u16`, plus two constraints outside the sub-i32-integer case: `bool` gets `i32.and 1`, and a non-empty `enum` gets `i32.rem_u <variant count>` (variant tags are assigned by declaration position, so the range `0..N-1` is always contiguous). A variantless enum draw is left unconstrained — the type is uninhabited, and `rem_u 0` would trap.

The same `bool`/enum constraint is applied to a compound (array/struct) uzumaki leaf before its store (`emit_compound_uzumaki_domain_constraint`, `core/wasm-codegen/src/compiler.rs:4240`). A compound narrow-int leaf needs no separate constraint: the element's `store8`/`store16` truncation, combined with the sign- or zero-extending typed load used to read it back, already realizes the domain on every round trip through memory.

This matters specifically for the non-deterministic blocks in [Overflow Checks in Non-Deterministic Blocks](#overflow-checks-in-non-deterministic-blocks) above: a `forall`/`exists`/`unique` quantifier ranges over every value the draw can produce, so an unconstrained draw of a narrow type made the Rocq-side quantifier range over all `2^32` bit patterns rather than the declared type's actual value set — a soundness gap for exactly the constructs this document's proof-obligation sections depend on. Every mapping above is surjective onto the target domain, so quantifying over the raw draw and then mapping is equivalent to quantifying over the domain directly.
