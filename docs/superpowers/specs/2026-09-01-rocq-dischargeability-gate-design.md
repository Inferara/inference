# Rocq Emitted-Artifact Dischargeability Gate Design

**Issue:** [Inferara/inference#450](https://github.com/Inferara/inference/issues/450)

**Repositories:** `Inferara/inference` and `Inference-Global-Software/wasm-verifier`

**Status:** Approved design

## Purpose

The existing Rocq gates establish that generated proof-mode output elaborates
against the vendored signature stub and the pinned real libraries. They do not
establish that any emitted obligation is true: generated theorem skeletons end
in `Admitted.`, and `coqc` accepts a false theorem statement when its proof is
admitted.

The downstream verifier contains closed proofs related to several generated
examples, but those files are adapted copies. Scope-key rewrites and other
manual edits mean a downstream `Qed.` is not evidence about the bytes emitted
by `infc --mode proof`.

This design adds a cross-repository gate that certifies a representative subset
of freshly emitted artifacts without modifying their bytes. It also adds a
kernel-checked negative certificate so a logic change that makes an explicitly
false obligation valid cannot pass as a successful generalization.

## Goals

- Prove every generated theorem in four representative positive artifacts:
  bounded prime, existential reachability, unique reachability, and narrow
  declared-domain obligations.
- Compile the exact raw `.v` bytes produced by the Inference pipeline; do not
  keep or certify an adapted copy of generated definitions.
- Derive each gate theorem's target from the generated theorem itself, avoiding
  a second handwritten statement that can drift.
- Reject a proof that uses any generated `Admitted.` theorem, directly or
  transitively.
- Prove that a deliberately false generated `ValidSpec` obligation is invalid.
- Fail closed on missing cases, missing theorems, added theorems, byte drift,
  pin drift, proof holes, unreviewed assumptions, malformed receipts, and a
  misconfigured no-op oracle.
- Run all local Rocq verification in the pinned Coq 8.20 Docker environment.
- Run compiler-side local verification in an isolated Linux Rust 1.91 Docker
  environment with a dedicated Cargo target volume.

## Non-goals

- Proving all eight currently committed obligation-bearing goldens in the first
  gate. The initial set is a floor that can grow without changing the protocol.
- Removing `Admitted.` from normal generated proof skeletons. Open skeletons are
  the intended handoff format established by issue #358.
- Changing the emitted `%N` scope key to `%num`. `%N` is correct in the
  mathcomp-free generated context; proof companions use `%num` after importing
  MathComp.
- Changing `ValidSpec`, `ValidExistsSpec`, `ValidUniqueSpec`, or the verifier's
  reviewed assumptions baseline.
- Treating the current unbounded prime obligation as a proved false theorem. It
  remains deliberately uncertified; the bounded sibling is the positive case,
  and a separate `assert(false)` fixture is the genuine negative certificate.
- Making the private wasm-verifier implementation or its diagnostics public.

## Root cause

The current real-library lane compiles raw generated files, but compilation is
only an interface/elaboration check because every generated theorem is admitted.
The verifier-side examples close hand-maintained definitions rather than the
fresh producer output. No contract connects a generated theorem's exact type to
an independent closed proof, and no assumption audit excludes the admitted
generated theorem from such a proof's dependency graph.

The fix therefore needs three independent properties:

1. **Artifact identity:** the definitions and theorem statements under proof
   come from the untouched producer output.
2. **Independent closure:** another theorem proves the exact generated theorem
   type without consuming the generated admitted constant.
3. **Negative soundness control:** the verifier proves that a known-false
   generated obligation is not valid.

## Selected architecture

Inference owns sources, fresh generation, committed goldens, the selected-case
floor, and CI orchestration. wasm-verifier owns proof companions, the private
discharge runner, proof-completeness checks, and the assumptions audit.

For each case, the discharger creates an ephemeral directory containing:

- `Raw.v`: the received generated file, copied byte-for-byte;
- `Proofs.v`: verifier-owned independent proof material for the stable case ID;
- `Rebind.v`: generated theorems whose targets are derived from `Raw` theorem
  types and whose bodies use the corresponding independent proofs;
- `Audit.v`: `Print Assumptions` commands for every rebound endpoint.

The raw module is compiled in an isolated logical namespace. Its admissions are
therefore available to proof automation, but they are not trusted: the rebound
theorems make any use visible to `Print Assumptions`, and the audit rejects every
assumption in the raw namespace.

Coq 8.20 can derive the full generalized target, including implicit host and
memory parameters, without a theorem-statement parser:

```coq
Theorem gate_endpoint :
  ltac:(let T := type of @Raw.valid_program in exact T).
Proof.
  exact (@Proofs.checked_valid_program).
Qed.
```

The leading `@` is required: without it, typeclass resolution attempts to fill
the generated theorem's generalized context while its type is being inspected.

For a refutation entry, the generated gate introduces the same host context as
the raw theorem, specializes the raw theorem type to that context, negates its
result proposition, and closes the negation with the companion certificate.
This proves `~ ValidSpec ...` for every generated host context rather than merely
showing that one tactic script happens to fail.

## Initial case matrix

| Stable case ID | Inference artifact | Required result | Endpoints |
| --- | --- | --- | ---: |
| `prime-bounded` | New bounded sibling of `rocq_prime_example.inf` | Prove exact `ValidModule` and `ValidSpec` | 2 proved |
| `exists` | `rocq_exists_spec.inf` | Prove exact `ValidModule`, empty universal `ValidSpec`, and `ValidExistsSpec` | 3 proved |
| `unique` | `rocq_unique_spec.inf` | Prove exact `ValidModule`, empty universal `ValidSpec`, and `ValidUniqueSpec` | 3 proved |
| `narrow-domain` | `spec_narrow_discharge.inf` | Prove exact `ValidModule` and two-payload `ValidSpec` | 2 proved |
| `false-spec` | New spec containing `assert(false)` | Prove exact `ValidModule`; prove the generated `ValidSpec` is invalid | 1 proved, 1 refuted |

The workflow floor is five cases, eleven proved endpoints, and one refuted
endpoint. The harness asserts the exact ordered case set and endpoint counts;
the workflow independently parses and checks the structured receipt totals.

### Prime disposition

The existing unbounded prime source and golden remain unchanged and explicitly
uncertified. The new sibling adds a visible signed upper-bound antecedent using
the verifier's established `N_MAX = 2_000_000_000` proof envelope. Its raw output
is a separate golden and its exact `ValidSpec` becomes the positive certificate.

This preserves the original regression artifact while adding a theorem whose
source-visible assumptions match the proof's arithmetic domain.

### Negative certificate

The negative source is intentionally minimal:

```inf
spec FalseCertificate {
    fn impossible() forall {
        assert(false);
    }
}
```

Its universal payload is the strict nonzero reading of zero, equivalent to
`HA_not (term_eq (T_const (Vi32 0)) (T_const (Vi32 0)))`. The verifier proves
the generated `ValidSpec` invalid by obtaining the all-valuations witness,
selecting the singleton payload, and contradicting the required inequality with
the fact that both literals denote the same `Vi32 0`.

This certificate is independent of function interpretation, runtime state,
locals, imports, and arithmetic bounds. A regression that drops the payload or
makes strictified inequality permissive causes the negation proof to fail.

## Names-only manifest

wasm-verifier maintains one manifest entry per stable case. An entry contains:

- stable case ID;
- expected raw basename;
- expected raw SHA-256 at the reciprocal Inference pin;
- ordered generated theorem names;
- for each theorem, `prove` or `refute` polarity and the companion endpoint;
- expected proved and refuted counts.

The manifest never duplicates theorem types. `Rebind.v` derives those from the
raw constants with `type of @Raw.<name>`. The discharger enumerates generated
theorem skeletons and requires exact ordered agreement with the manifest, so a
new or removed obligation cannot silently fall outside the gate.

The raw file may contain admissions only in the exact generated skeleton form:

```coq
Proof.
  (* TODO: fill the proof *)
Admitted.
```

Any other `Admitted.`, `admit`, `Abort.`, direct axiom/parameter/conjecture, or
kernel-check bypass is rejected. Proof companions, generated rebound sources,
and audit sources permit none of those constructs.

## Discharger protocol

Inference invokes the configured executable as:

```text
$INFERENCE_WASM_VERIFIER_DISCHARGER \
  --protocol 1 \
  --wasm-verifier-revision <pinned-sha> \
  --case <stable-case-id> \
  <fresh-raw-file.v>
```

Exit zero is necessary but not sufficient. A successful executable writes one
machine-readable receipt containing:

- protocol version;
- case ID;
- raw SHA-256;
- pinned and observed wasm-verifier revisions;
- pinned and observed coq-wasm revisions;
- observed Coq version;
- proved and refuted endpoint counts;
- audited assumption count;
- result `pass`.

The Rust harness validates every field against its input, case expectation, and
pin file. A zero exit with no receipt, a duplicate receipt, malformed data, or a
field mismatch is a failure. A deliberately malformed raw-file provenance probe
must also be rejected before any case result is trusted; `/bin/true` therefore
cannot masquerade as the discharger.

The protocol transmits no proof source or private library diagnostics back to
Inference. On failure, the discharger keeps the full log in its local temporary
area and emits only a stable case label plus a bounded first error line.

## Integrity and assumption checks

For every invocation, the discharger:

1. Verifies that the local wasm-verifier checkout is clean and exactly the
   requested pinned revision.
2. Verifies Coq 8.20, the pinned coq-wasm commit, and the configured container.
3. Hashes the received raw file.
4. Verifies its basename, expected generated skeletons, theorem set, and
   reciprocal-pin hash where applicable.
5. Compiles the raw file unchanged against the real libraries.
6. Hashes it again and requires byte identity.
7. Lexically rejects proof holes and kernel bypasses in companion/gate sources.
8. Compiles the independent proof companion.
9. Compiles every rebound theorem whose target comes from the raw theorem type.
10. Runs `Print Assumptions` for every rebound endpoint.
11. Rejects any raw-namespace assumption and any assumption absent from
    `ci/assumptions.allow`.

The per-case footprint may be a subset of `ci/assumptions.allow`; unused
upstream assumptions are not required. wasm-verifier's existing full-build
assumption audit continues to require the complete reviewed baseline, catching
unexpected removal as well as addition at the repository level.

## CI ownership and security

The dischargeability lane is separate from the existing real-library
type-check lane. A green elaboration check must not be reported as a green proof
closure check, and either capability may be configured without the other.

Inference's `rocq-real-library` workflow gains:

- a hosted-runner capability gate for `WASM_VERIFIER_DISCHARGER`;
- a private-runner discharge job using the existing `real-rocq` environment;
- explicit five-case, eleven-proved, and one-refuted floors;
- an explicit `SKIPPED` summary when no discharger is configured;
- a required mode in the private job, where an absent or unusable discharger is
  an error rather than a test skip.

Pull requests retain the existing same-repository `ci:real-rocq` label gate.
The label is scheduling, not authorization. The protected `real-rocq`
environment, private runner group, and outside-collaborator approval remain the
security boundary because the job builds and executes pull-request code. The
workflow must not use `pull_request_target`, and it must not add a path filter
that would break label-triggered runs.

wasm-verifier CI checks its proof companions against the raw committed goldens
from the reciprocal pinned Inference revision. Inference CI checks the current
pull request's freshly generated artifacts against the pinned verifier
checkout. The two directions catch proof drift and producer drift without
making either repository trust a floating sibling checkout.

## Pinning and landing sequence

The existing machine-readable pins remain the source of truth:

- Inference pins wasm-verifier and coq-wasm in
  `core/wasm-to-v/wasm-verifier-pin.txt`.
- wasm-verifier adds a discharge pin for the Inference revision and selected
  artifact hashes, following the same `git show <revision>:<path>` discipline as
  `book/inference-pin.txt`.

The commits land without a circular pin:

1. Inference commit A adds the bounded/false/narrow goldens and producer-side
   case expectations while leaving the external lane unconfigured.
2. wasm-verifier commit B adds proof companions and pins commit A.
3. Inference commit C pins commit B and enables the configured discharge lane.

The live Inference pull-request lane always sends freshly generated bytes, not
commit A's golden, so later producer drift fails even while the historical
reciprocal pin remains stable.

## Docker-only local verification

Host Rocq output is not accepted as verification evidence.

The local verification entry point orchestrates two isolated environments:

1. A `rust:1.91-bookworm` container mounts the Inference source read-only and
   writes Cargo registry and Linux build artifacts to dedicated named volumes.
   In export mode, the Rust harness runs the targeted golden/case tests and
   writes freshly generated raw artifacts plus a machine-readable request to a
   temporary exchange volume.
2. The existing `wasm-verifier` Coq 8.20 devcontainer consumes those exact raw
   files, runs the discharger, and writes receipts to the exchange volume. The
   same Rust container then runs the harness in verify mode, validating every
   receipt and the aggregate floors against the exported request.

The host script only orchestrates containers and validates their exit status;
it does not compile Rust or Rocq. The Rust container is not given the Docker
socket, and the host performs no receipt interpretation. The exchange volume is
deleted after verification, and the Linux Cargo target volume is reused
serially rather than duplicated per lane.

Required local evidence:

- each proof companion builds in the Coq 8.20 container;
- wasm-verifier's full `coqbuild.sh` succeeds, including proof-hole and
  assumptions audits;
- the verifier-side pinned-artifact discharge check succeeds;
- the Inference targeted Rocq/golden/case tests succeed in the Rust container;
- the end-to-end five-case discharge run reports exactly eleven proved and one
  refuted endpoint;
- the broader `inference-tests` crate suite succeeds in the same Rust target
  volume.

## Test-driven implementation and mutation evidence

Implementation begins with failing tests for each contract boundary:

- a selected case cannot pass before its proof companion exists;
- an added or missing raw theorem fails manifest coverage;
- a zero-exit executable without a valid receipt fails;
- a receipt with the wrong raw hash, revision, case, or counts fails;
- a malformed raw file fails the provenance probe;
- a proof implemented with `exact @Raw.<admitted-theorem>` compiles but fails
  the assumptions audit;
- a byte mutation before or during compilation fails the hash check;
- a companion containing an admission, axiom, abort, or kernel bypass fails the
  completeness scan;
- the false fixture's positive `ValidSpec` cannot be certified, while its
  closed negation succeeds;
- each positive artifact closes every generated theorem.

The synthetic admission-use test is load-bearing: it demonstrates that the
assumptions check detects the exact shortcut the architecture exposes by
importing raw admitted output.

## Failure reporting

Failures are grouped by phase: configuration, pin, raw integrity, raw
elaboration, proof completeness, companion compilation, target rebinding, or
assumption audit. The public message names the stable case, phase, and a bounded
diagnostic. The full container log and ephemeral sources remain local for
reproduction.

Skipped capability is never phrased as proof success. Summaries distinguish:

- real-library elaboration checked or skipped;
- selected-artifact discharge checked or skipped;
- exact proved/refuted totals;
- pinned revisions used.

## Documentation and changelog

Inference's Rocq contract documents three separate claims:

1. vendored-stub elaboration;
2. real-library elaboration;
3. exact-artifact dischargeability for the named subset.

wasm-verifier documentation identifies proof companions as certificates over
external raw artifacts, not generated-looking source copies. Both changelogs
record the initial case floor, the negative certificate, and the reciprocal
pins.

## Acceptance criteria

The issue is addressed when all of the following hold:

- Fresh output for all five cases matches its committed Inference golden.
- The four positive artifacts compile unchanged and all ten of their generated
  theorems are independently rebound and proved.
- The false artifact's module theorem is independently proved and its generated
  `ValidSpec` is independently refuted.
- The receipt reports five cases, eleven proofs, one refutation, and the exact
  raw hashes and revisions.
- No rebound theorem depends on a raw generated admission or an assumption
  outside the verifier allowlist.
- The current unbounded prime golden remains preserved and explicitly
  uncertified; the bounded sibling is certified.
- The no-op, missing-obligation, byte-drift, proof-hole, raw-admission-use, and
  false-positive mutation tests all fail for their intended reasons.
- wasm-verifier's full Docker build and audits pass.
- Inference's targeted and broader tests pass in the Rust Docker environment.
- CI reports elaboration and dischargeability as separate, accurately scoped
  claims.
