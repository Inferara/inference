# Unreachable Emission in Codegen

The Inference compiler emits a WebAssembly `unreachable` instruction before the `end` of every non-void function body. This document explains why, how other compilers handle the same problem, and why the alternatives are worse.

## The Problem

WebAssembly is a stack-typed bytecode format. When a function declares a return type, the WASM validator requires a matching value on the stack at the function's `end` instruction. Consider a function where all control-flow paths exit via explicit `return`:

```wat
(func $if_else_branch (param $x i32) (result i32)
  local.get $x
  i32.const 0
  i32.gt_s
  if
    i32.const 1
    return
  else
    i32.const 0
    return
  end
  ;; <-- nothing on the stack here, but validator expects i32
)
```

Both arms return, so the code after `end` of the `if` block is dead. But the WASM validator does not perform control-flow analysis — it checks the stack type at every reachable textual position. Without something to satisfy the type at function `end`, the module fails validation.

## The Solution

Emit `unreachable` before the function's `end`:

```wat
(func $if_else_branch (param $x i32) (result i32)
  local.get $x
  i32.const 0
  i32.gt_s
  if
    i32.const 1
    return
  else
    i32.const 0
    return
  end
  unreachable   ;; makes stack polymorphic — satisfies any return type
)
```

The `unreachable` instruction is [stack-polymorphic per the WASM specification](https://webassembly.github.io/spec/core/valid/instructions.html): after it, the stack matches any type signature. The [validation algorithm](https://webassembly.github.io/spec/core/appendix/algorithm.html) enters a polymorphic state where the required return type is trivially satisfied.

When the program is correct (all paths return), the `unreachable` is dead code and never executes. When the program has a bug (a path falls through without returning), the `unreachable` traps at runtime — a fail-fast behavior that prevents silent wrong results.

For void functions (`unit` return type), no value is expected on the stack at `end`, so the `unreachable` is omitted.

## WASM Specification Rationale

[WebAssembly design issue #998](https://github.com/WebAssembly/design/issues/998) explains why `unreachable` makes the stack polymorphic:

> There is no control edge from unconditional control transfer instructions (`br`, `br_table`, `return`, `unreachable`) to the following instruction, so no constraint needs to be imposed between them regarding the stack.

The specification deliberately supports this pattern because "disallowing unreachable code would be problematic for producers." The `unreachable` instruction exists precisely to let compilers satisfy validation in dead code positions. See also [design issue #1379](https://github.com/WebAssembly/design/issues/1379) for further discussion of unreachable state typing.

## How the Validation Algorithm Works

The validation algorithm maintains a control stack and an operand stack. Each frame tracks an `unreachable` boolean. When `unreachable` is visited:

1. The frame's `unreachable` flag is set to `true`
2. The operand stack is truncated to the frame's height

After this, any attempt to pop an operand from the stack returns `Bottom` — a special type that matches any expected type. When the function's `end` instruction is reached, the validator pops each result type; in a polymorphic frame, each pop trivially succeeds via `Bottom`. This works for zero, one, or multiple return values.

The same mechanism is used by all unconditional control transfers (`br`, `return`, `br_table`, `throw`). Stack polymorphism is a fundamental design property of the WASM type system, not an incidental feature.

### Compatibility with WASM proposals

No current or proposed WASM extension changes how `unreachable` interacts with function-end validation:

| Proposal | Impact |
|----------|--------|
| Exception handling | No conflict — `throw`/`throw_ref` set unreachable themselves |
| GC | No conflict — `Bottom` matches all reference types |
| Tail calls | No conflict — `return_call` uses the same mechanism |
| Multi-value | No conflict — each result type popped individually, each returns `Bottom` |
| Stack switching | No impact on validation algorithm |

The polymorphic stack mechanism is so fundamental that no proposal can break it without redesigning the entire type system. The WASM Community Group has [explicitly stated](https://github.com/WebAssembly/design/issues/998) that "disallowing unreachable code would be problematic for producers."

## How Production Compilers Handle This

### rustc (Rust)

Rust enforces exhaustive returns through its type system (the `!` / never type and exhaustive match checking) **and** still emits trap/unreachable instructions in codegen as a safety net.

**MIR to codegen.** When MIR contains `TerminatorKind::Unreachable`, the LLVM backend emits `llvm.unreachable` and the Cranelift backend emits `trap(TrapCode::user(1))` with the block marked cold.

**TrapUnreachable.** [PR #45920](https://github.com/rust-lang/rust/pull/45920) enabled LLVM's `TrapUnreachable` flag, converting `unreachable` IR into hardware trap instructions (`ud2` on x86, `unreachable` on WASM). Code-size impact was measured at 0.0%–0.1%. The flag defaults to `true` for all targets including `wasm32-unknown-unknown` — WASM targets do not override it.

**Security motivation.** The PR was motivated by [issue #28728](https://github.com/rust-lang/rust/issues/28728): LLVM removed an infinite loop (optimizing "must progress" assumption), causing code to fall through to a `match` with no arms, executing arbitrary memory. TrapUnreachable converts this from silent corruption to a deterministic trap.

**Intentional design.** In [issue #52998](https://github.com/rust-lang/rust/issues/52998), the Rust team confirmed this is **intentional**: "This behaviour is very much intended" (Nagisa, compiler team). hanna-kruppe clarified: "It's a hardening measure, not something ensuring soundness." [PR #88826](https://github.com/rust-lang/rust/pull/88826), which proposed turning TrapUnreachable off, was **closed without merging** — consensus: keep it on.

**All-paths-return enforcement.** Rust checks this in the type checker (`rustc_hir_typeck`), using the `Diverges` enum to track whether code paths converge or diverge. The `CoerceMany` collector gathers return types and demands the final LUB coerces to the declared return type. This is front-end checking, orthogonal to the codegen safety net.

The pattern is: **front-end enforces all-paths-return, codegen emits trap as defense-in-depth.**

### LLVM / Clang (C/C++)

Clang emits `@llvm.trap()` (debug builds) or `unreachable` (optimized builds) at the end of non-void functions that may fall through. [LLVM issue #75797](https://github.com/llvm/llvm-project/issues/75797) discusses this extensively. A real-world vulnerability ([CVE-2014-9296](https://access.redhat.com/security/cve/cve-2014-9296) in OpenSSL) demonstrated how a missing return value can be exploited when the compiler optimizes the undefined behavior path.

**WASM target specifically.** LLVM **forces** `--trap-unreachable=1` and `--no-trap-after-noreturn=0` for all WASM targets. These flags are forced even if the user explicitly passes different values. Both `@llvm.trap()` and LLVM IR `unreachable` lower to the same WASM `unreachable` opcode. The LLVM WASM backend handles `unreachable` in its assembler type checker ([LLVM review D112953](https://reviews.llvm.org/D112953)).

The active debate in the LLVM community is not about whether to emit traps, but about whether optimized builds should use `unreachable` (which the optimizer can exploit as UB) or `@llvm.trap()` (which always traps). On WASM, this distinction is moot — there is only one trapping instruction.

### GCC

GCC uses `__builtin_unreachable()` to mark code paths that should never execute. The `-funreachable-traps` flag (enabled by default at `-O0`/`-Og`) converts `__builtin_unreachable` to a trap instruction. A [2025 patch](https://gcc.gnu.org/pipermail/gcc-patches/2025-July/690150.html) strengthens this by converting standalone `__builtin_unreachable` to `__builtin_trap` even at higher optimization levels.

Without `__builtin_unreachable`, GCC issues `-Wreturn-type`: "control reaches the end of a non-void function." The [GCC documentation](https://gcc.gnu.org/onlinedocs/gcc/Other-Builtins.html) describes this as intended for "situations where the compiler cannot deduce the unreachability of the code."

### Zig

Zig's self-hosted WASM backend (`src/codegen/wasm/CodeGen.zig`, line 1242-1248) uses the exact same pattern:

> In case we have a return value, but the last instruction is a noreturn (such as a while loop) we emit an unreachable instruction to tell the stack validator that part will never be reached.

All three of Zig's `airTrap`, `airBreakpoint`, and `airUnreachable` emit the same WASM `unreachable` opcode — there is no distinction on WASM targets. Zig also emits `unreachable` in the implicit else path of if-without-else (line 4087).

### Binaryen

Binaryen (the WebAssembly optimizer/toolchain library) explicitly emits `unreachable` opcodes after constructs that make subsequent code dead. From [issue #1056](https://github.com/WebAssembly/binaryen/issues/1056):

> Binaryen IR has an unreachable type for loops, but wasm binaries do not. To fix that, after it emits a loop that is unreachable it also emits an unreachable opcode, which ensures we are in an unreachable zone in the binary.

Binaryen's dead code elimination pass can remove instructions that precede `unreachable` but does NOT remove the `unreachable` instruction itself — it serves as the dead code marker. The `wasm-snip` tool uses `unreachable` as a canonical "removed function" placeholder.

## Formal Verification Perspective

### Mechanised proofs

[Conrad Watt's mechanised WASM specification (CPP 2018)](https://www.cl.cam.ac.uk/~caw77/papers/mechanising-and-verifying-the-webassembly-specification.pdf) provides a fully mechanised Isabelle specification of WASM 1.0 with a verified executable interpreter and type checker. The soundness proof of the WebAssembly type system was mechanised end-to-end. This directly validates that `unreachable`'s stack-polymorphic typing is sound — placing `unreachable` in any context is guaranteed to pass validation regardless of the expected stack type.

["A Type System with Subtyping for WebAssembly's Stack Polymorphism" (ICTAC 2022)](https://link.springer.com/chapter/10.1007/978-3-031-17715-6_20) by McDermott, Morita, and Uustalu formalises the two flavors of stack polymorphism in Agda, proving that every typable instruction sequence has a principal type.

[Iris-Wasm (PLDI 2023)](https://iris-project.org/pdfs/2023-pldi-iris-wasm.pdf) provides a mechanised higher-order separation logic for WASM built on Coq and Iris. In this framework, trapping instructions correspond to false preconditions, making them trivially verifiable.

### CompCert analogy

CompCert (the formally verified C compiler) handles unreachable code through "stuck states" — when the reference interpreter encounters undefined behavior, execution gets stuck. CompCert's correctness theorem only covers defined-behavior paths. For Inference, which targets formal verification via Rocq translation, the `unreachable` instruction makes the "stuck" explicit and deterministic rather than silent.

### Impact on WASM-to-Rocq translation

The `unreachable` instruction at function boundaries provides a clear semantic for the Rocq translator: "this path is impossible; any proof obligation here is vacuously true" (analogous to `False_rect` in Coq). This is beneficial for formal verification — it creates an explicit proof obligation rather than silently accepting potentially incorrect behavior.

## Why the Alternatives Are Worse

### Emit a default return value (e.g., `i32.const 0`)

If a front-end bug allows a missing return and this code executes, silently returning zero is more dangerous than trapping. The `unreachable` approach is strictly safer because it fails fast. This also uses more bytes (2+ bytes for `i32.const 0` vs 1 byte for `unreachable`). [WebAssembly design issue #448](https://github.com/WebAssembly/design/issues/448) explicitly states this concern: "the producer would have to materialize a bogus return value just to make the type check pass."

### Only emit `unreachable` when control-flow analysis proves the end is unreachable

This adds a whole analysis pass to eliminate a single harmless instruction. The `unreachable` is dead code when the program is correct, and a trap when it is not — both are correct behaviors. Control-flow analysis is valuable for compile-time diagnostics (catching missing returns as errors), but it should not gate whether codegen emits a safety net.

### Rely solely on front-end checking (no codegen safety net)

No production compiler does this. Every compiler listed above enforces returns in the front-end **and** emits traps in codegen. Rust's [issue #28728](https://github.com/rust-lang/rust/issues/28728) demonstrated why: LLVM removed an infinite loop, code fell through to a `match` with no arms, executing random memory. Defense-in-depth is standard practice for safety-critical toolchains.

## Cost Analysis

| Metric | Impact |
|--------|--------|
| Code size | 1 byte per non-void function (opcode `0x00`). rustc measured 0.0%–0.1% increase across std. |
| Runtime performance | Zero when dead code. WASM runtimes eliminate dead code after unconditional branches during JIT compilation. |
| Debugging | Positive — traps produce clear `RuntimeError: unreachable executed` with stack traces in all major WASM runtimes. |
| wasm-opt compatibility | Compatible — Binaryen preserves `unreachable` as a terminator; dead code elimination operates on instructions before it, not on the `unreachable` itself. |

## Security Context

[CERT C Coding Standard, rule MSC37-C](https://github.com/stanislaw/awesome-safety-critical/blob/master/Backup/sei-cert-c-coding-standard-2016-v01.pdf): "Ensure that control never reaches the end of a non-void function." This standard documents how flowing off the end of a non-void function has caused real vulnerabilities. Emitting `unreachable` converts this from "undefined behavior" to "deterministic trap."

WebAssembly provides inherent control-flow integrity: all functions and their types are declared at load time, compiled code is immutable, and structured control flow prevents arbitrary jumps. The `unreachable` instruction complements this by ensuring that if execution reaches an impossible point, a deterministic trap occurs rather than silent corruption.

## Inference's Approach

Inference follows the same layered strategy as rustc, LLVM, GCC, Zig, and Binaryen:

1. **Front-end**: The `core/analysis/` crate enforces that all non-void functions return on every code path — rule **A007 (Missing return)** produces a compile-time error for violations (see [Static Analysis](static-analysis.md)).
2. **Codegen**: The `unreachable` instruction before function `end` serves as a safety net. If the front-end has a bug, the program traps instead of silently misbehaving.
