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
| `wasm_verifier/` | `WasmVerifier.*` | wasm-verifier (a private Inferara repository; this stub declares, in-repo, the subset of its interface the emitter can print) — the `hassert` assertion language and the proof-obligation predicates (`ValidModule`/`ValidSpec`, plus the `ValidExistsSpec`/`ValidUniqueSpec` reachability pair) |

| File | Provides | Mirrors |
| --- | --- | --- |
| `wasm/bytes.v` | `byte`, `list_byte_of_string` (used by the emitted `Mi`/`Me` helpers), and an opaque `encode : Z -> byte` (used by every byte of every emitted `moddata_init`) | wasm-verifier's `coq-wasm` dependency, `theories/bytes.v` — without its `byte_scope` and the 244 two-digit uppercase hex notations it declares there, none of which an emitted module can spell: each expands to arithmetic over the dependency's single hex-digit notations, which stand for bare numerals, so every value whose spelling carries a digit `A` .. `F` elaborates at `nat` and fails against `encode` |
| `wasm/numerics.v` | `i32`/`i64`, `Wasm_int.int_of_Z`, `i32m`/`i64m` (used by `Vi32`/`Vi64`) | Vanilla WasmCert |
| `wasm/datatypes.v` | Value/number/reference types, integer operator families, `memarg`, the `basic_instruction` inductive, and the module/section records | Vanilla WasmCert |
| `wasm/host.v` | The `host` typeclass every emitted theorem is stated under (`Class host := {}`) | Vanilla WasmCert |
| `wasm_verifier/Assertions.v` | The `term`/`hassert` inductives and the `term_eq`/`Himpl`/`Hor`/`Hall` sugar — the emittable subset of wasm-verifier's `Assertions.v`, see [Narrowed to what the emitter can print](#narrowed-to-what-the-emitter-can-print) | wasm-verifier, `Assertions.v` |
| `wasm_verifier/Verifier.v` | `Parameter ValidModule : module -> Prop` and `Parameter ValidSpec : forall `{ho:host}, module -> list hassert -> Prop`, the two predicates every emitted module's theorems reference (the reachability pair lives in `Exists.v`) | wasm-verifier, `Verifier.v` |
| `wasm_verifier/Exists.v` | The concrete `Record reachability_spec` (a `Parameter` type has no fields to elaborate the emitted `{| … |}` literals against) and the `ValidExistsSpec`/`ValidUniqueSpec` predicates the kind-selected theorems for `exists`/`unique`-bodied spec functions reference | wasm-verifier, `theories/Exists.v` |
| `_CoqProject` | Maps both physical directories to their logical library names | — |

Compile manually from this directory, `Wasm` first (`WasmVerifier` imports
it). Each `-Q` maps a *physical* directory to its logical name, exactly as
`_CoqProject` above does — `-Q . Wasm` instead binds `wasm/bytes.v` to
`Wasm.wasm.bytes`, and the first cross-file import fails with "Unable to
locate library `Wasm.bytes`":

```sh
coqc -Q wasm Wasm wasm/bytes.v
coqc -Q wasm Wasm wasm/numerics.v
coqc -Q wasm Wasm wasm/datatypes.v
coqc -Q wasm Wasm wasm/host.v
coqc -Q wasm Wasm -Q wasm_verifier WasmVerifier wasm_verifier/Assertions.v
coqc -Q wasm Wasm -Q wasm_verifier WasmVerifier wasm_verifier/Verifier.v
coqc -Q wasm Wasm -Q wasm_verifier WasmVerifier wasm_verifier/Exists.v
```

This writes a `.vo`, a `.glob` and a `.<module>.aux` beside each source, in
a directory whose `.v` files are tracked. The repo `.gitignore` covers all
three, so a manual compile here leaves `git status` clean. Sweep them with
`git clean -Xdf wasm wasm_verifier` from this directory when done — the two
namespace directories hold nothing else that is ignored, which is why the
sweep names them rather than the stub root.

(`tests/src/rocq_typecheck.rs` does this in dependency order into a
scratch directory automatically; see [How the gate uses it](#how-the-gate-uses-it).)

## Scope: what is and isn't covered

The stub covers the **scalar + non-deterministic-free** instruction
surface Inference's proof-mode codegen actually emits into a body the
emitted module record keeps — executable functions and retained
`exists`/`unique` spec functions, the latter vanilla by construction —
integer arithmetic and comparisons, memory/local/global access,
structured control flow (`if`/`loop`/`br`), calls — plus every module
and section record, plus the emittable subset of the `hassert`
term/assertion language for `spec`-derived obligations and the
`reachability_spec` record those obligations are wrapped in for the
`exists`/`unique` kinds.
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
- **The float-naming half of `cvtop`** — `CVO_trunc`, `CVO_trunc_sat`,
  `CVO_convert`, `CVO_demote`, `CVO_promote` and `CVO_reinterpret`. Each
  needs a float number type on one side or the other, and this stub
  declares none. The two integer-to-integer constructors, `CVO_wrap` and
  `CVO_extend`, *are* declared, alongside `BI_cvtop` and the
  `Unop_extend` sign-extension operator: Inference codegen emits none of
  those either, but a statically-linked external compiled by a real
  toolchain does, and the translator lowers all eight.
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
and is itself the regression guard for the omission/retention design: a
`forall`/plain `spec` function's WASM body is not translated at all (the
function is omitted from `mod_funcs`; its logical content becomes a
`hassert` instead), a retained `exists`/`unique` body is
reachability-lowered to vanilla WASM before it ever reaches this crate,
and any non-deterministic instruction surviving into a body the module
record keeps is rejected as `WasmToVError::UnsupportedFeature` before
reaching the printer (defense-in-depth behind analysis rule A042, which
already makes non-det syntax outside a `spec` declaration a compile-time
error for any Inference-compiled program). Should a non-deterministic
instruction ever leak into the module record again — an emitter
regression, or a hand-crafted `.wasm` bypassing A042 — it becomes an
"unbound constructor" `coqc` error against this stub rather than a
silently type-checking term.

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
  someone renames a constructor in both). This is what the drift check
  described under [What holds this mirror to its originals](#what-holds-this-mirror-to-its-originals)
  closes: a rename agreed between emitter and stub still has to survive a
  comparison against the upstream sources at the pinned revisions, which
  neither of them can move.
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
from some fixture in the gate's corpus; these six were removed.

This table records what [#401] took out of a former faithful copy. It is
**not** the stub-versus-upstream delta, and must not be read as one: the
real `Assertions.v` also carries names this mirror never had, and it has
grown more of them since. That full delta is not maintained by hand at
all — it is measured against the pinned sources and pinned as a digest in
`../wasm-verifier-pin.txt`, so upstream growing or dropping a declaration
is a change somebody is asked about rather than one nobody sees.

| Removed declaration | Kind | Why the emitter cannot produce it |
| --- | --- | --- |
| `T_global : N -> term` | `term` constructor | No `HTerm::Global` in the IR, no term tag that decodes to one, no printer arm. Nothing upstream wants the variant either: an Inference specification cannot reference a global at all, so no global read ever becomes a term. |
| `HA_emp : hassert` | `hassert` constructor — the empty heap | Heap fragment: no `HAssert` variant, no assertion tag, no printer arm. Unreachable at the source level too — the memory constructs that would motivate a heap assertion are rejected as **P002** before any assertion is built. |
| `HA_star : hassert -> hassert -> hassert` | `hassert` constructor — separating conjunction | Heap fragment: no `HAssert` variant, no assertion tag, no printer arm; the memory constructs behind it are rejected as **P002**. |
| `HA_iter : term -> term -> hassert -> hassert` | `hassert` constructor — iterated heap fragment | Heap fragment: no `HAssert` variant, no assertion tag, no printer arm; the memory constructs behind it are rejected as **P002**. |
| `HA_pto : term -> term -> hassert` | `hassert` constructor — points-to | Heap fragment: no `HAssert` variant, no assertion tag, no printer arm; the memory constructs behind it are rejected as **P002**. |
| `HA_size : term -> hassert` | `hassert` constructor — heap/memory size | Heap fragment: no `HAssert` variant, no assertion tag, no printer arm; the memory constructs behind it are rejected as **P002**. |

`Hall` was the seventh row until the universal binder became emittable:
a `forall` block nested inside an `exists` context now translates rather
than raising **P007**, so `Assertions.v` declares
`Hall (body : hassert) : hassert` again and
`spec_quantifier_alternation.inf` is its corpus producer. **P007**
survives only for a `forall` block inside an `exists`/`unique`-quantified
body, where the nested block would have to bind a universal variable over
choices the reachability judgment quantifies operationally — a rejection
about the reachability ABI, not about the assertion language, so it no
longer keeps any name off this stub.

"Cannot produce" is three mechanical facts per row, not a judgement
call. `core/hassert/src/ir.rs` has no `HTerm`/`HAssert` variant for any
of the six. `core/hassert/src/codec.rs` decodes a **closed** tag table
— assertions `0x00`–`0x0B`, terms `0x00`–`0x05`, anything else is a
`DecodeError::UnknownHassertTag`/`UnknownTermTag` — and no tag in it
yields one. `core/wasm-to-v/src/hassert_print.rs` matches both IR enums
exhaustively and has no arm that could write one of these names. The
`P002` rejections in the table are the *fourth* line of defense, not the
load-bearing one: they explain why the IR was designed without the
variants in the first place. That rejection is mode-independent — a
memory construct in an `exists`/`unique` (reachability) body is the same
`P002` — so the reachability kinds opened no path to any of the six.

`HA_pred` and `pred_eq` stay, even though the printer never writes
either name. `term_eq`, which it writes constantly, is defined as
`HA_pred pred_eq (a :: b :: nil)` — so both are elaborated on every
`term_eq` in every emitted obligation, and deleting either would stop
this file itself from compiling. They are covered by the gate exactly
like any other reachable name; they are simply reached through their
sugar rather than by name.

### How the narrowing is pinned

An earlier draft of this section proposed holding the drift check to
"exactly these six rows, and fail on a seventh". That was wrong twice
over, and both errors are worth recording because the second one is the
reason this stub is still here.

It was wrong on the **count**. Measured against the pinned
`Assertions.v`, the declarations upstream has and this mirror does not
number 32, not six: eleven `term`/`hassert` constructors — the six above
plus `T_unop`, `T_cvtop`, `T_testop`, `HA_bytes` and `HA_data` — together
with the `assertion` layer and the `strictify` family. A hand-maintained
table of expected deltas would have been stale on the day it was written,
which is exactly why the narrowing is pinned as a **digest** computed
from the pinned sources instead.

It was wrong on the **remedy**. Pointing `-Q` at the real `theories/`
widens the name space rather than breaking it, so the same corpus still
compiles — and that is the problem, not the reassurance the draft took it
for. The narrowing's guard, an accidental `HA_pto` becoming an
unbound-constructor error, is *lost* by the swap. That guard is real:
compiling a module that names `HA_pto` against this stub is
`The reference HA_pto was not found in the current environment`, and
against the real library it is a clean pass. So this stub is kept rather
than replaced, and the two gates are complementary — the real-library
lane catches what the stub is too narrow to model, and the stub catches
what the real library is too permissive to reject.

`Hall`'s return is the worked example of the reverse move, and the
template for the next name that becomes emittable: a `Definition`
restored here, its row dropped from the table above, and a corpus fixture
that actually emits it — because a declaration nothing produces is a
declaration `coqc` never elaborates, which is the state this narrowing
exists to prevent.

## What holds this mirror to its originals

The authoritative targets are the real `WasmCert-Coq` (v2.2.0, commit
`0fd83fa`) and `wasm-verifier` (commit `77f1126`) libraries, recorded in
`../wasm-verifier-pin.txt`. Two gates hold this stub to them, and both
read the upstream sources at the pinned revisions rather than from
whatever a checkout happens to have on disk.

`tests/src/rocq_stub_drift.rs` compares declarations. In the *fiction*
direction every name and every constructor arity this stub declares must
exist upstream and agree; in the *narrowing* direction the set of
declarations upstream has and this stub does not is digested and pinned,
so upstream growing or dropping one is a change a human is asked about.
The WasmCert half needs no credential and runs in CI; the wasm-verifier
half runs where a checkout exists and skips loudly elsewhere.

`tests/src/rocq_typecheck.rs` compiles the emitted `.v` itself. Its
always-available lane uses this stub; when
`INFERENCE_WASM_VERIFIER_COQC` names an oracle, a second lane compiles
the same modules against the real libraries, after a provenance probe
that rejects a stand-in which merely exits zero.

Deleting this stub is still the eventual goal — it exists because a
signature mirror needs no credential — but it is no longer the only
thing standing between codegen and the proof target.
