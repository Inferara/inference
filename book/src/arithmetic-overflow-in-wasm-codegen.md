# Arithmetic Overflow in WASM Codegen

Inference compiles to WebAssembly. WebAssembly integer arithmetic wraps silently on overflow for add, subtract, and multiply. This document explains what that means exactly, how it differs from other languages, what Inference's codegen currently does, and why this matters for a language that targets formal verification.

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

Inference inherits WASM's wrapping semantics for add, subtract, multiply, and negation: those emit bare arithmetic instructions with no overflow guard, and there is no compile-time overflow detection for them. Division is the one exception — signed division overflow (`MIN / -1`) traps at every width, natively for `i32`/`i64` and via a compiler-added guard for the narrow types (see [Division and Modulo](#division-and-modulo)). Separately, a dynamic (runtime-index) array access and a failing `assert` emit their own runtime traps.

### Binary Expression Lowering

`lower_binary_expression` in `core/wasm-codegen/src/compiler.rs` dispatches to the appropriate WASM instruction based on the left operand's type and emits it unconditionally:

```wat
;; Inference: return 2147483647 + 1
i32.const 2147483647
i32.const 1
i32.add          ;; wraps to -2147483648 — no trap, no check
return
```

No overflow check precedes the `i32.add`. The result is exactly what WASM's specification says: `-2147483648`.

### Negation Lowering

`lower_prefix_unary_expression` lowers the unary negation operator `-x` as `0 - x`:

```wat
;; Inference: return -min  (where min = i32::MIN)
i32.const 0
local.get $min   ;; pushes -2147483648
i32.sub          ;; 0 - (-2147483648) wraps to -2147483648
return
```

Negating `i32::MIN` produces `i32::MIN`. This is verifiable against the golden WAT output in `tests/test_data/codegen/wasm/arith_overflow/arith_overflow.wat`:

```wat
(func $i32_neg_min (;6;) (type 6) (result i32)
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

The current behavior therefore matches C's integer promotion rule (arithmetic is done in the promoted width) *and* truncates back to the sub-type's width immediately afterward, so a sub-i32 local never holds a value outside its declared range.

### Division and Modulo

Integer arithmetic wraps at every width, with one deliberate exception: **division overflow traps at every width**. Divide-by-zero and remainder-by-zero pass through as native WASM traps at every width, and so does signed division's single overflow case, `MIN / -1`.

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
| Inference | Defined wrapping | Trap | Signed division overflow traps at every width (narrow types via a compiler-added guard); add/sub/mul/neg wrap |

The critical distinction is between *defined* behavior and *undefined* behavior. C's undefined behavior for signed overflow means the optimizer is allowed to assume overflow never occurs, leading to deleted bounds checks, eliminated branches, and silent wrong results. WASM has no such latitude — the specification fully defines every overflow result, making the behavior predictable regardless of optimization level.

## Compiler Patterns

### rustc to WASM

When Rust compiles to `wasm32-unknown-unknown` in debug mode, it inserts overflow checks for every arithmetic operation on integer types. The check is implemented via the `checked_add` / `checked_sub` / `checked_mul` intrinsics in MIR: each operation returns `Option<T>`, and if the value is `None` (overflow occurred), execution falls through to a `panic` call. On WASM, that panic call lowers to `unreachable`. The net effect is a conditional `unreachable` that fires on overflow:

```wat
;; Conceptual structure of Rust's debug-mode overflow check for i32 + i32
;; (actual Cranelift output may differ in register usage and block layout)
local.get $a
local.get $b
i32.add
local.tee $result
local.get $a
local.get $b
;; check if overflow occurred (Cranelift uses uadd_overflow_trap or equivalent)
i32.gt_s
if
  unreachable    ;; panic!("attempt to add with overflow")
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

When writing a Rocq proof about Inference code, the user must either:

1. Prove that no overflow can occur (typically by establishing bounds on inputs), or
2. Account for wrapping in the proof (the result is defined, so the proof is possible, but it may be unexpected).

Option 1 is more common. A function that receives an `i32` parameter and returns `param + 1` requires the user to prove that the input is less than `i32::MAX` before the proof obligation `result == input + 1` can be discharged over integers. If that precondition is missing, the proof must instead discharge `result == (input + 1) mod 2^32`, which is a different and weaker claim about the function's behavior.

### Overflow Checks as Proof Obligations

A future direction for Inference is to treat compile-time overflow checks not as inserted runtime traps but as proof obligations discharged by the verifier. Under this model:

- In compile mode, the compiler emits bare WASM arithmetic (no checks, maximum performance).
- In proof mode, each arithmetic operation generates a Rocq proof obligation: "the inputs to this operation are within bounds."
- The programmer discharges the obligation via a proof or by establishing sufficient preconditions in the function's spec block.

This would make Inference's overflow handling fundamentally different from Rust's or Zig's: rather than inserting a runtime guard that might or might not be reached, the verifier would guarantee at proof time that the guard is never needed.

## Current Implementation Details

All arithmetic lowering is in `core/wasm-codegen/src/compiler.rs`.

**`lower_binary_expression`** dispatches on the left operand's `TypeInfoKind` using `is_i64_type()` and `is_unsigned_type()`, then emits a single WASM instruction with no surrounding guards:

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

The test suite for overflow behavior is in `tests/src/codegen/wasm/arith_overflow.rs`. It covers eight cases: `i32::MAX + 1`, `i32::MIN - 1`, `i64::MAX + 1`, `i64::MIN - 1`, `u32::MAX + 1`, `i32::MAX * 2`, `-i32::MIN`, and `-i64::MIN`. Each case is verified against a golden WAT file and executed via Wasmtime to confirm the wrapping result at runtime.

## Future Considerations

### Checked Arithmetic Mode

Inference could add a compiler flag (e.g., `--overflow=trap`) that inserts an overflow check before every arithmetic operation in compile mode, identical to what Rust does in debug mode. The check sequence for `a + b` would be:

```wat
;; overflow check for i32.add
local.get $a
local.get $b
i32.add
local.tee $result
local.get $a
local.get $b
;; detect overflow: if (a > 0 && b > i32::MAX - a) || (a < 0 && b < i32::MIN - a)
;; ... conditional unreachable ...
local.get $result
```

This mode would be appropriate for development builds. It has a direct cost in code size and throughput, which is why Rust defaults to wrapping in release mode.

### Constant Folding and Compile-Time Detection

A constant-folding pass could evaluate constant *expressions* at compile time and report an error when the result overflows. This does not require runtime guards — it is purely a front-end diagnostic.

The literal case is already closed: analysis rule **A022 (Literal out of range)** rejects `let a: i8 = 200` at compile time, because 200 exceeds `i8::MAX` (127) and the value could never round-trip through its declared type (see [Static Analysis](static-analysis.md)). What remains open is folding *computed* constants — `127 + 1` assigned to an `i8` still wraps silently.

### Overflow Checks in Non-Deterministic Blocks

Non-deterministic blocks (`forall`, `exists`, `unique`) operate over all possible execution paths. If overflow checks are added as runtime traps in compile mode, those checks would need to be stripped from `spec` blocks (which are excluded from compile mode output) but preserved in proof mode. The interaction between overflow check emission and spec-block stripping would need to be specified explicitly.

### Sub-i32 Truncation

Sub-i32 truncation after arithmetic is implemented (see [Sub-i32 Types](#sub-i32-types-i8-i16-u8-u16) above); for `i8` addition, the emitted sequence is:

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
