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

Inference inherits WASM's wrapping semantics directly. The compiler emits bare arithmetic instructions with no additional guards. There is no compile-time overflow detection and no runtime overflow check.

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

Sub-i32 types are promoted to `i32` for all arithmetic. Inference does not emit truncation or masking instructions after arithmetic on sub-i32 types. The WASM `i32.add` instruction operates on the full 32-bit value, and the result is stored back as a 32-bit local. This means that arithmetic on `i8` variables can produce results outside the range -128..127 without any compiler-inserted truncation.

This is a known gap tracked in the project. A complete implementation would emit `i32.extend8_s` or a masking sequence after arithmetic on sub-i32 types. The current behavior matches C's integer promotion rules (arithmetic is done in the promoted width) but does not truncate back to the sub-type width on store.

### Division and Modulo

Division and modulo instructions are emitted without compiler-added guards. The `div_s (MIN, -1)` trap and the division-by-zero trap pass through as WASM traps. When triggered, a runtime environment will report an unhandled WASM trap.

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
| Inference | Inherits WASM wrapping | Inherits WASM trap | No compiler-added guards currently |

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

A correctness-oriented future change is to emit truncation after arithmetic on sub-i32 types. For `i8` addition, the correct sequence is:

```wat
local.get $a     ;; i8 stored as i32
local.get $b     ;; i8 stored as i32
i32.add
i32.const 0xff   ;; mask low 8 bits
i32.and
i32.extend8_s    ;; sign-extend from 8 bits (requires sign-ext proposal)
```

Without this truncation, `i8` variables can hold values outside `-128..127` after arithmetic. The current behavior matches C's promotion rules but not the semantic expectation of a typed language with distinct `i8` and `i32` types.
