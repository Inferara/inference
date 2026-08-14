# Vendored Rocq signature stub for `wasm-to-v`

A minimal, self-contained pair of Rocq libraries that provide the logical
names `Wasm` and `WasmVerifier` — everything the proof-mode `.v` output
of `core/wasm-to-v` `Require`s. Both declare **signatures only** —
`Parameter`/`Axiom`/`Inductive`/`Record`/transparent sugar `Definition`s
with no denotational semantics and no proofs — just enough for `coqc` to
*type-check* an emitted module.

## Why this exists (issue [#231])

Every `wasm-to-v` test string-matches the emitted `.v`; none ever ran
`coqc` on it. That is how the [#230] `BI_forall`/`BI_exists` arity bug
shipped green: a 1-ary library constructor was applied to two arguments
and every substring assertion still passed. The bug surfaced only when a
compiled `.v` reached the paid, private prover worker.

This stub makes that failure mode a local `coqc` type error. It encodes
the external contract **as the emitter actually writes it** (see
`../ROCQ_CONTRACT.md` and `../src/translator.rs`/`../src/hassert_print.rs`).
Every declaration fixes the arity/shape the emitter emits, so a
mis-aritied or renamed constructor — the exact #230 class, now including
the `hassert` obligation constructors — no longer type-checks against
this file.

The gate that drives it lives in `tests/src/rocq_typecheck.rs`: it
generates a corpus of proof-mode modules in-process (parse →
type-check → codegen → `wasm_to_v`), compiles this stub once with `coqc`
(both logical roots mapped), then compiles each generated module against
it.

## Layout

Two logical namespaces, mapped by `_CoqProject`:

```text
-Q wasm Wasm
-Q wasm_verifier WasmVerifier
```

| Directory | Namespace | Mirrors |
| --- | --- | --- |
| `wasm/` | `Wasm.*` | Vanilla **WasmCert-Coq v2.2.0** — the WASM datatypes, no fork extensions |
| `wasm_verifier/` | `WasmVerifier.*` | wasm-verifier (a private Inferara repository; this stub declares, in-repo, the subset of its interface the emitter can print) — the `hassert` assertion language and the two proof-obligation predicates |

| File | Provides | Mirrors |
| --- | --- | --- |
| `wasm/bytes.v` | `byte`, `list_byte_of_string` (used by the emitted `Mi`/`Me` helpers), and `byte_scope` with an opaque `encode` plus the same 244 two-digit uppercase hex notations over it (used by every emitted `moddata_init`) | wasm-verifier's `coq-wasm` dependency, `theories/bytes.v` — matching the twelve values its hand-written block skips (`#12` .. `#19`, `#1C` .. `#1F`), and without the module-level `Open Scope byte_scope.` it leaves open for importers, so an emitted module has to open the scope its own byte notations need |
| `wasm/numerics.v` | `i32`/`i64`, `Wasm_int.int_of_Z`, `i32m`/`i64m` (used by `Vi32`/`Vi64`) | Vanilla WasmCert |
| `wasm/datatypes.v` | Value/number/reference types, integer operator families, `memarg`, the `basic_instruction` inductive, and the module/section records | Vanilla WasmCert |
| `wasm/host.v` | The `host` typeclass every emitted theorem is stated under (`Class host := {}`) | Vanilla WasmCert |
| `wasm_verifier/Assertions.v` | The `term`/`hassert` inductives and the `term_eq`/`Himpl`/`Hor` sugar — the emittable subset of wasm-verifier's `Assertions.v`, see [Narrowed to what the emitter can print](#narrowed-to-what-the-emitter-can-print) | wasm-verifier, `Assertions.v` |
| `wasm_verifier/Verifier.v` | `Parameter ValidModule : module -> Prop` and `Parameter ValidSpec : forall `{ho:host}, module -> list hassert -> Prop`, the two predicates emitted theorems reference | wasm-verifier, `Verifier.v` |
| `_CoqProject` | Maps both physical directories to their logical library names | — |

Compile manually, `Wasm` first (`WasmVerifier` imports it):

```sh
coqc -Q . Wasm wasm/bytes.v
coqc -Q . Wasm wasm/numerics.v
coqc -Q . Wasm wasm/datatypes.v
coqc -Q . Wasm wasm/host.v
coqc -Q . Wasm -Q . WasmVerifier wasm_verifier/Assertions.v
coqc -Q . Wasm -Q . WasmVerifier wasm_verifier/Verifier.v
```

(`tests/src/rocq_typecheck.rs` does this in dependency order into a
scratch directory automatically; see [How the gate uses it](#how-the-gate-uses-it).)

## Scope: what is and isn't covered

The stub covers the **scalar + non-deterministic-free** instruction
surface Inference's proof-mode codegen actually emits into a *surviving*
(executable) function body — integer arithmetic and comparisons,
memory/local/global access, structured control flow (`if`/`loop`/`br`),
calls — plus every module and section record, plus the emittable subset
of the `hassert` term/assertion language for `spec`-derived obligations.
That subset is strictly smaller than wasm-verifier's own `Assertions.v`,
and deliberately so: see
[Narrowed to what the emitter can print](#narrowed-to-what-the-emitter-can-print)
for the row-by-row list of what was left out and why.

Deliberately **out of scope**, because Inference code never lowers to
them and the corpus can never exercise them:

- **Floating-point, entirely** — the `f32`/`f64` representation types,
  the `T_f32`/`T_f64` number types, float constants, and the float
  operator families (`relop_f`/`binop_f`/`unop_f`). Inference has no
  floating-point.
- **Conversions** — the whole `cvtop` family and `BI_cvtop`. Inference
  codegen emits no conversion instructions.
- **SIMD/vector** (`T_v128`), **GC/reference-typing instructions**, and
  **atomics**.

Their absence is a feature, not a gap: emitted terms are plain
constructor applications, so a narrower stub type-checks exactly the same
modules while making any accidental emission of an unsupported operator a
loud `coqc` error instead of a silently type-checking term. If the
emitter ever starts producing one of these constructs from real
Inference source, extend the corpus in `tests/src/rocq_typecheck.rs` and
the relevant inductive here together.

## The deliberate absence of the fork's non-det constructors

`wasm/datatypes.v`'s `basic_instruction` inductive does **not** declare
`BI_forall`, `BI_exists`, `BI_assume`, `BI_unique`, or
`BI_uzumaki_num` — the `WasmCert-Coq-Essence` fork's non-deterministic
extensions this crate previously targeted. Their absence is intentional
and is itself the regression guard for the module-omission design: a
`spec` function's WASM body is no longer translated at all (the function
is omitted from `mod_funcs`; its logical content becomes a `hassert`
instead), and any non-deterministic instruction surviving into an
*executable* body is rejected as `WasmToVError::UnsupportedFeature`
before reaching the printer (defense-in-depth behind analysis rule A042,
which already makes non-det syntax outside a `spec` declaration a
compile-time error for any Inference-compiled program). Should a
non-deterministic instruction ever leak into the module record again —
an emitter regression, or a hand-crafted `.wasm` bypassing A042 — it
becomes an "unbound constructor" `coqc` error against this stub rather
than a silently type-checking term.

## Proofs are not closed here (`Qed.` → `Admitted.`)

Emitted per-spec and per-module theorems carry an unfilled
`(* TODO: fill the proof *)` body terminated by `Qed.`, which `coqc`
rejects as an incomplete proof (the prover worker is what fills these).
Because this gate asserts **type-checking, not proof closure**, the test
rewrites each `Qed.` terminator to `Admitted.` before running `coqc`.
`Admitted` still fully elaborates and type-checks every `Definition`
(the module record, every instruction term, and every `hassert`
obligation tree — where arity bugs live) and every theorem *statement*
(where a `ValidModule`/`ValidSpec` shape drift surfaces), without
demanding a closed proof. This is a boundary the stub cannot fake with
signatures alone: it deliberately checks statements + definitions, not
proofs.

## How the gate uses it

`tests/src/rocq_typecheck.rs`:

1. Drives the real pipeline in-process for a fixed corpus of `.inf`
   fixtures under `tests/test_data/inf/` (parse → type-check → proof-mode
   `codegen` → `wasm_to_v`), covering both the executable-instruction
   surface and the `hassert` obligation shapes (`ValidSpec`, `term_eq`,
   `Himpl`, `T_app`, `T_local`, `HA_ex`, …).
2. Asserts, independent of whether `coqc` is even installed, that the
   corpus still emits a fixed set of required constructs — so a change
   that silently stops exercising the `coqc` gate's coverage fails a fast,
   always-on check.
3. When `coqc` is available (`COQC` env var, else `coqc` on `PATH`),
   copies this stub into a scratch directory, compiles `Wasm` then
   `WasmVerifier` once, rewrites every generated module's `Qed.` to
   `Admitted.`, and compiles each one against the compiled stub.
4. Otherwise prints a clear "skipped" line and returns `Ok` — CI installs
   `coqc`, so the gate is real there; the corpus-generation and
   proof-surface-coverage checks from step 2 still run locally without it.

## Drift risk

This is a hand-written mirror, not the real libraries, so it can drift:

- **False green** if the emitter and this stub drift *together* (e.g.
  someone renames a constructor in both). The mitigation is that the real
  fix — wiring the actual wasm-verifier and vanilla WasmCert libraries
  into CI — replaces this stub; until then, changes to emitted
  constructors should be reviewed against `../ROCQ_CONTRACT.md`.
- **False red** if this stub encodes an arity the real libraries don't
  share. Keep declarations faithful to `../src/translator.rs` and
  `../src/hassert_print.rs`, which are the source of truth for what is
  emitted, and to the real wasm-verifier source when adding or changing a
  `WasmVerifier.*` declaration.

## History: the retired Essence-fork stub

Before this contract, `rocq-stub/` targeted the private
`WasmCert-Coq-Essence` fork (branch `yoshihiro503@wasm-verifier`) as a
single `-Q . Wasm` namespace, declaring the fork's non-deterministic
`BI_*` constructors (including `BI_unique`, which the fork itself
carries only as commented-out) so the then-current emitter's 2-ary
`ValidModule <mod> <specs>` output would type-check. That stub, its
divergence notes, and its target library are gone: wasm-verifier
(vanilla WasmCert-Coq v2.2.0 plus the `hassert` layer) replaces the
Essence fork as the consumer, and the emitter no longer produces any
construct that library doesn't have.

## Narrowed to what the emitter can print

The `wasm_verifier/` half of this stub started out as a faithful copy of
wasm-verifier's `Assertions.v`, which meant it declared constructors no
fixture could ever produce. A declaration nothing elaborates is not a
contract: `coqc` never looks at it, so its arity or its spelling can
drift out of agreement with the real library and the gate still goes
green — precisely the [#230] failure class this stub exists to catch. So
under [#401] the assertion layer was cut down to the names the emitter
can actually print. Everything left in `Assertions.v` is now reachable
from some fixture in the gate's corpus; these seven were removed:

| Removed declaration | Kind | Why the emitter cannot produce it |
| --- | --- | --- |
| `T_global : N -> term` | `term` constructor | No `HTerm::Global` in the IR, no term tag that decodes to one, no printer arm. Nothing upstream wants the variant either: an Inference specification cannot reference a global at all, so no global read ever becomes a term. |
| `HA_emp : hassert` | `hassert` constructor — the empty heap | Heap fragment: no `HAssert` variant, no assertion tag, no printer arm. Unreachable at the source level too — the memory constructs that would motivate a heap assertion are rejected as **P002** before any assertion is built. |
| `HA_star : hassert -> hassert -> hassert` | `hassert` constructor — separating conjunction | Heap fragment: no `HAssert` variant, no assertion tag, no printer arm; the memory constructs behind it are rejected as **P002**. |
| `HA_iter : term -> term -> hassert -> hassert` | `hassert` constructor — iterated heap fragment | Heap fragment: no `HAssert` variant, no assertion tag, no printer arm; the memory constructs behind it are rejected as **P002**. |
| `HA_pto : term -> term -> hassert` | `hassert` constructor — points-to | Heap fragment: no `HAssert` variant, no assertion tag, no printer arm; the memory constructs behind it are rejected as **P002**. |
| `HA_size : term -> hassert` | `hassert` constructor — heap/memory size | Heap fragment: no `HAssert` variant, no assertion tag, no printer arm; the memory constructs behind it are rejected as **P002**. |
| `Hall (body : hassert) : hassert` | sugar `Definition` — universal quantifier | No `HAssert` variant, no assertion tag, no printer arm. The one construct that would need it is a `forall` block nested inside an `exists` context, which code generation rejects as **P007**. |

"Cannot produce" is three mechanical facts per row, not a judgement
call. `core/hassert/src/ir.rs` has no `HTerm`/`HAssert` variant for any
of the seven. `core/hassert/src/codec.rs` decodes a **closed** tag table
— assertions `0x00`–`0x0A`, terms `0x00`–`0x05`, anything else is a
`DecodeError::UnknownHassertTag`/`UnknownTermTag` — and no tag in it
yields one. `core/wasm-to-v/src/hassert_print.rs` matches both IR enums
exhaustively and has no arm that could write one of these names. The
`P002`/`P007` rejections in the table are the *fourth* line of defense,
not the load-bearing one: they explain why the IR was designed without
the variants in the first place.

`HA_pred` and `pred_eq` stay, even though the printer never writes
either name. `term_eq`, which it writes constantly, is defined as
`HA_pred pred_eq (a :: b :: nil)` — so both are elaborated on every
`term_eq` in every emitted obligation, and deleting either would stop
this file itself from compiling. They are covered by the gate exactly
like any other reachable name; they are simply reached through their
sugar rather than by name.

### Checklist for the real-library swap-in ([#359])

Whoever wires the real libraries in inherits three consequences of this
narrowing. None of them is a blocker; all three are easy to get subtly
wrong.

- **The stub-drift check must treat exactly these seven rows as the
  expected deltas, and fail on an eighth.** The point of a narrowed stub
  is only worth having if the narrowing is itself pinned: a mechanical
  comparison against the real `Assertions.v` should assert that the set
  of declarations present there and absent here is *precisely* the table
  above. An eighth missing name means either the real library grew
  something this stub should mirror, or someone quietly deleted a
  reachable declaration — both are failures, and neither is visible if
  the check only asks "is the stub a subset?".
- **Pointing `-Q` at the real `theories/` restores all seven, and that
  is a widening, not a break.** The real library declares the full
  assertion language; compiling the same corpus against it will simply
  have seven more names in scope that nothing mentions. No emitted module
  changes, no fixture needs touching. What is lost in the swap is the
  narrowing's *secondary* benefit — an accidental `HA_pto` becoming an
  unbound-constructor error locally — so if that guard is wanted after
  the swap, it has to move somewhere that still enforces it (the emitter,
  or the drift check above), because the real library will happily accept
  it.
- **Lifting P007 puts `Hall` back on the emitted surface, so its
  `Definition` must return in the same change.** Supporting a `forall`
  nested inside an `exists` means the printer starts writing `Hall` by
  name; against this stub that is an immediate unbound-constructor error.
  Re-add `Definition Hall (body : hassert) : hassert := HA_not (HA_ex
  (HA_not body)).`, drop its row from the table above, and add a corpus
  fixture that actually emits it — the same three steps any other newly
  reachable declaration needs.

## Follow-up: swap in the real libraries

The authoritative targets are the real `WasmCert-Coq` (v2.2.0) and
`wasm-verifier` repositories. Wiring them into CI needs the real sources
available at build time (a checkout + `.vo` build, or a prebuilt image)
and is intentionally left as a follow-up. When it lands, point the gate's
`-Q` flags at the real libraries' `theories` directories instead of this
one, pin the same wasm-verifier commit this stub was verified against
(`0c5d525e`) so codegen and the proof target cannot drift, and delete
this stub. The corpus and the test harness carry over unchanged.
