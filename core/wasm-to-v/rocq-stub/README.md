# Vendored Rocq signature stub for `wasm-to-v`

A minimal, self-contained Rocq library that provides the logical name `Wasm`
with the modules the proof-mode `.v` output of `core/wasm-to-v` requires:
`bytes`, `numerics`, `datatypes`, and `verifier`. It declares **signatures
only** — `Parameter`/`Axiom`/`Inductive`/`Record` with no bodies, no semantics,
no proofs — just enough for `coqc` to *type-check* an emitted module.

## Why this exists (issue [#231])

Every `wasm-to-v` test string-matches the emitted `.v`; none ever ran `coqc` on
it. That is how the [#230] `BI_forall`/`BI_exists` arity bug shipped green: a
1-ary library constructor was applied to two arguments and every substring
assertion still passed. The bug surfaced only when a compiled `.v` reached the
paid, private prover worker.

This stub makes that failure mode a local `coqc` type error. It encodes the
external contract **as the emitter actually writes it** (see
`../ROCQ_CONTRACT.md` and `../src/translator.rs`). Every declaration fixes the
arity/shape the emitter emits, so a mis-aritied or renamed constructor — the
exact #230 class — no longer type-checks against this file.

The gate that drives it lives in `tests/src/rocq_typecheck.rs`: it generates a
corpus of proof-mode modules in-process (parse → type-check → codegen →
`wasm_to_v`), compiles this stub once with `coqc -Q . Wasm`, then compiles each
generated module against it.

## Layout

| File | Provides |
| --- | --- |
| `bytes.v` | `byte`, `list_byte_of_string` (used by the emitted `Mi`/`Me` helpers) |
| `numerics.v` | `i32`/`i64`/`f32`/`f64`, `Wasm_int.int_of_Z`, `i32m`/`i64m` (used by `Vi32`/`Vi64`) |
| `datatypes.v` | value/number/reference/vector types, operator families, `memarg`, the `basic_instruction` inductive (all `BI_*` constructors), and the module/section records |
| `verifier.v` | the `host` typeclass and `ValidModule` predicate the emitted theorems reference |
| `_CoqProject` | maps the physical directory to the logical library name `Wasm` |

Compile manually with:

```sh
coqc -Q . Wasm bytes.v
coqc -Q . Wasm numerics.v
coqc -Q . Wasm datatypes.v
coqc -Q . Wasm verifier.v
```

## Scope: what is and isn't covered

The stub covers the **scalar + non-deterministic** instruction surface that
Inference's proof-mode codegen actually emits — integer arithmetic and
comparisons, memory/local/global access, structured control flow (`if`/`loop`/
`br`), calls, and the `forall`/`exists`/`assume`/`unique`/uzumaki family — plus
every module and section record.

Deliberately **out of scope**, because Inference executable code never lowers to
them and the corpus never exercises them: SIMD/vector, GC/reference-typing,
atomics, and the float-conversion instructions. Several of those are also known
emitter-vs-library divergences (see below), so declaring them here would encode
a shape no real library accepts. If the emitter ever starts producing one of
these from real Inference source, extend the corpus in
`tests/src/rocq_typecheck.rs` and the relevant inductive here together.

## Emitter-vs-library divergences (intentional)

Where the emitter currently writes a construct that the real
WasmCert-Coq-Essence library reportedly does **not** provide, the stub still
declares it **with the arity the emitter writes**, so current `main`
type-checks and the arity stays pinned. These are library-side / emitter-side
concerns tracked upstream, not something this stub tries to "fix":

- **`ValidModule` is 2-ary here** (`module -> list N -> Prop`). The emitter
  writes `ValidModule <mod> <mod>__<Spec>_specs`. This contradicts the prose in
  `../ROCQ_CONTRACT.md` and the `CHANGELOG.md` "Breaking" note, which describe a
  post-#21 split into a 1-ary `ValidModule : module -> Prop` plus a separate
  `ValidSpec : module -> list N -> Prop`. The stub matches the **emitter**, not
  the prose, because the stub's job is to type-check what is emitted today.
  Reconciling emitter, contract prose, and the real library is a follow-up.
- **`BI_unique`** is declared (`block_type -> list basic_instruction -> _`,
  mirroring `BI_assume`). The verifier library has it commented out in its
  `datatypes.v`, so real proofs of `unique` modules do not compile against it;
  the stub declares it anyway so the arity is pinned and `unique` fixtures
  type-check here.

## Proofs are not closed here (`Qed.` → `Admitted.`)

Emitted per-spec theorems carry an unfilled `(* TODO: fill the proof *)` body
terminated by `Qed.`, which `coqc` rejects as an incomplete proof (the prover
worker is what fills these). Because this gate asserts **type-checking, not
proof closure**, the test rewrites each `Qed.` terminator to `Admitted.` before
running `coqc`. `Admitted` still fully elaborates and type-checks every
`Definition` (the module record and all instruction terms — where arity bugs
live) and every theorem *statement* (where a `ValidModule` drift surfaces),
without demanding a closed proof. This is a boundary the stub cannot fake with
signatures alone: it deliberately checks statements + definitions, not proofs.

## Drift risk

This is a hand-written mirror, not the real library, so it can drift:

- **False green** if the emitter and this stub drift *together* (e.g. someone
  renames a constructor in both). The mitigation is that the real fix — wiring
  the actual verifier library into CI — replaces this stub; until then, changes
  to emitted constructors should be reviewed against `../ROCQ_CONTRACT.md`.
- **False red** if this stub encodes an arity the real library doesn't share.
  Keep declarations faithful to `../src/translator.rs` call sites, which is the
  source of truth for what is emitted.

## Follow-up: swap in the real verifier library

The authoritative target is the private
`github.com/Inference-Global-Software/WasmCert-Coq-Essence` (branch
`yoshihiro503@wasm-verifier`, the commit baked into the prover worker AMI at
`/opt/wasmcert-essence`). Wiring it into CI needs org secrets (a checkout +
`.vo` build, or a prebuilt image) and is intentionally left as a follow-up. When
it lands, point the gate's `-Q ... Wasm` at the real library's `theories`
instead of this directory, pin the same commit the worker uses so codegen and
the proof target cannot drift, and delete this stub. The corpus and the test
harness carry over unchanged.
