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

### Claim boundary and Phase A status

The design keeps three claims distinct:

1. Vendored-stub elaboration checks generated syntax against the deliberately
   narrowed in-repo signature surface.
2. Pinned real-library elaboration checks the same raw bytes against the actual
   consumer interface and load path.
3. Exact-artifact discharge checks only the named five-case subset: fresh bytes
   equal the committed golden, the unchanged raw module elaborates, independent
   companions close or refute the exact generated types, rebound endpoints end
   in `Qed`, and the assumption audit excludes every raw or unapproved
   dependency.

The first two are elaboration claims, not truth claims. Generated skeletons
still contain `Admitted.`, so either elaboration check can accept a false
statement. Phase A implements the producer fixtures, strict protocol, and
Docker orchestration contract only. It does not contain the verifier-side
manifest, proof companions, discharger, inspector, or adapters and therefore
does not establish end-to-end exact-artifact discharge. That claim becomes
available only after verifier commit B exists and Inference commit C pins and
invokes B.

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
- Run compiler-side local verification in an isolated Linux Rust 1.98 Docker
  environment with a dedicated Cargo target volume.

For implementation and verification of this issue through Task 12, the user
authorized one narrow Rust-only evidence lane: formatting, Clippy, and Rust
tests may run from a validated non-worktree host snapshot with the tracked lane
lock and an isolated exact Rust 1.98 toolchain. No Rocq, proof, or bridge call
uses the host, and this exception does not change the Docker-isolated product
wrapper described below.

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
showing that one tactic script happens to fail. In the pinned libraries that
context generalizes as host-function-class, memory, then host; an explicit
application therefore uses `@Raw.<theorem> _ _ ho`, not
`@Raw.<theorem> ho`.

For `false-spec`, the companion certificate is named exactly
`checked_valid_spec_is_false`. The manifest records that endpoint on the
generated `ValidSpec` theorem with `refute` polarity, and `Rebind.v` applies
`@Proofs.checked_valid_spec_is_false _ _ ho` directly after deriving the
specialized raw proposition. No `checked_refutation` alias or extra audit
endpoint exists.

## Initial case matrix

| Stable case ID | Raw artifact | Exact generated theorem -> companion endpoint | Required result |
| --- | --- | --- | ---: |
| `prime-bounded` | `rocq_prime_bounded_example.v` | `valid_rocq_prime_bounded_example` -> `checked_valid_module`; `valid_rocq_prime_bounded_example__prime_properties_bounded` -> `checked_valid_spec` | 2 proved, 0 refuted |
| `exists` | `rocq_exists_spec.v` | `valid_rocq_exists_spec` -> `checked_valid_module`; `valid_rocq_exists_spec__ReachableDouble` -> `checked_valid_spec`; `valid_exists_rocq_exists_spec__ReachableDouble` -> `checked_valid_exists_spec` | 3 proved, 0 refuted |
| `unique` | `rocq_unique_spec.v` | `valid_rocq_unique_spec` -> `checked_valid_module`; `valid_rocq_unique_spec__UniqueParity` -> `checked_valid_spec`; `valid_unique_rocq_unique_spec__UniqueParity` -> `checked_valid_unique_spec` | 3 proved, 0 refuted |
| `narrow-domain` | `spec_narrow_discharge.v` | `valid_spec_narrow_discharge` -> `checked_valid_module`; `valid_spec_narrow_discharge__NarrowDischarge` -> `checked_valid_spec` | 2 proved, 0 refuted |
| `false-spec` | `rocq_false_certificate.v` | `valid_rocq_false_certificate` -> `checked_valid_module`; `valid_rocq_false_certificate__FalseCertificate` -> `checked_valid_spec_is_false` | 1 proved, 1 refuted |

The workflow floor is five cases, eleven proved endpoints, and one refuted
endpoint: twelve audited endpoints in total. The harness asserts the exact
ordered case set and endpoint counts;
the workflow independently parses and checks the structured receipt totals.
The false endpoint name is exactly `checked_valid_spec_is_false`: the manifest
uses it directly and generated `Rebind.v` applies it directly, with no alias or
thirteenth endpoint.

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

wasm-verifier maintains one manifest entry per stable case. The manifest has
these top-level operational-provenance fields, separate from theorem-semantic
metadata:

- `inference_revision`: canonical lowercase 40-hex commit A;
- `coq_wasm_tag`: exactly `"v2.2.0"`, the normalized pinned tag expected from
  the active opam pin; the separate commit hash, not the movable tag, supplies
  immutability;
- `coq_wasm_revision`: exactly
  `"0fd83fa708922721132b6d6737179568d1f1d553"`, the canonical lowercase
  40-hex commit to which that tag resolves;
- `coq_series`: exactly `"8.20"`.

Those values are not duplicated in `container-pin.json`. `single` takes the
immutable coq-wasm/Coq values from its local manifest and compares them with
live opam/source observations before compilation and receipt publication.
`batch` additionally requires the request's coq-wasm tag/revision and Coq
series to equal the manifest. `pinned-inference` additionally requires the
requested Inference revision to equal `inference_revision` before reading A's
goldens with `git show`.

Each case entry contains:

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

The raw scanner tokenizes nested Coq comments and strings before classifying
top-level commands. The manifest-listed ordered `Theorem` declarations and
their exact skeletons are the only accepted proof commands. Ordinary generated
definitions with a body, `Definition name ... := ... .`, remain allowed.
Proof-style `Definition name : T.` without `:=`, every `Program Definition` and
obligation command, extra `Theorem`, and theorem synonyms such as `Lemma`,
`Remark`, `Fact`, `Corollary`, `Proposition`, and `Example` are rejected rather
than escaping manifest coverage. Stray `Proof`/`Goal`, ML module declarations,
and guard-check changes are likewise rejected.

Any other `Admitted.`, `admit`, `Abort.`, direct axiom/parameter/conjecture, or
kernel-check bypass is rejected. Proof companions, generated rebound sources,
and audit sources permit none of those constructs.

The proof-completeness scanner's repeatable `--root PATH` arguments resolve
under the canonical wasm-verifier repository root. Every supplied root must be
an independently real directory after resolution and contain at least one `.v`
file. Duplicate roots, paths outside the repository, empty roots, and unknown
options or positional arguments fail. With no arguments, the scanner preserves
its existing `theories` default. Multi-root scanning is one invocation, so a
hole in the second root cannot escape the result.

## Discharger protocol

The producer's protocol-1 `request.json` is strict JSON: duplicate and unknown
keys fail. Its top-level fields are `protocol`, `wasm_verifier_revision`,
`coq_wasm_tag`, `coq_wasm_revision`, `coq_series`,
`assumption_allowlist_count`, the three exact aggregate count fields, and the
ordered `cases` array. Each case contains only `case_id`, `raw_basename`,
canonical lowercase `raw_sha256`, and its expected proved/refuted counts. The
published exchange contains exactly that request and `raw/`; the request and
all five fresh raw files are immutable across bridge execution.

Inference invokes the configured executable as:

```text
$INFERENCE_WASM_VERIFIER_DISCHARGER \
  --protocol 1 \
  --wasm-verifier-revision <pinned-sha> \
  --case <stable-case-id> \
  <fresh-raw-file.v>
```

The executable writes its single receipt into the empty absolute directory in
`INFERENCE_WASM_VERIFIER_RECEIPT_DIR`, using the stable case ID as the basename
(`<case-id>.json`). Across the ordered five-case run, the caller creates five
distinct fresh empty `0700` directories, one per invocation. A successful
invocation leaves exactly one regular nonsymlink, single-link `0600` receipt in
its caller-owned directory. The wrapper rejects any pre-existing or additional
entry, so a stale receipt cannot be mistaken for the current verdict. Rust
independently requires the exact nonsymlink regular-file set and strictly
validates each receipt's JSON shape and fields.

Exit zero is necessary but not sufficient. A successful executable writes one
machine-readable receipt containing:

- protocol version;
- case ID;
- raw basename;
- raw SHA-256;
- pinned and observed wasm-verifier revisions;
- pinned and observed coq-wasm revisions;
- observed Coq version;
- proved and refuted endpoint counts;
- audited endpoint count and the cardinality of the distinct union of
  allowlisted dependencies across those endpoints;
- raw-namespace and unapproved dependency counts, both zero;
- result `pass`.

The receipt's `coq_version` must match the whole-string grammar
`major.minor`, `major.minor.<numeric-patch>`, or
`major.minor+<nonempty-safe-suffix>`, and its parsed major/minor must be `8.20`.
`8.20.1+suffix`, leading prose, trailing text, and nonnumeric patch components
fail. The local container inspector remains separately exact at `8.20.1`.

The Rust harness validates every field against its input, case expectation, and
pin file. The audited endpoint count must equal the proved-plus-refuted count;
the allowlisted dependency count may not exceed the reviewed allowlist size
recorded in the pin; and both rejected-dependency counts must be zero. A zero
exit with no receipt, a duplicate receipt, duplicate or unknown JSON keys,
malformed data, or a field mismatch is a failure. A deliberately malformed
same-basename raw-file provenance probe must also be rejected before any case
result is trusted; `/bin/true` therefore cannot masquerade as the discharger.

The configured Rust test obtains its case bytes through the same common export
path as the batch lane: it freshly generates all five artifacts, checks each
against its golden, builds the request using the current verifier pin, runs the
malformed provenance probe and five single-case invocations, then feeds the
receipts through the common verifier. It never submits committed goldens
directly to the configured executable.

The protocol transmits no proof source or private library diagnostics back to
Inference. Before either public adapter runs, the Inference wrapper supplies a
fresh identity-checked host directory through
`INFERENCE_WASM_VERIFIER_EVIDENCE_DIR`. It is absolute, private `0700`, and the
only accepted evidence directory. A failing verifier bridge must write the full
log to exactly `verifier.log` there as a regular nonsymlink, single-link `0600`
file before it returns nonzero. Public output contains only a bounded structured
diagnostic and a sanitized local evidence locator; the wrapper retains the
validated private directory on failure and removes it on success. No raw proof
source, receipt content, or private log body is copied into a public artifact.

The verifier exposes three non-overlapping command modes. `single` implements
the configured executable contract above. `batch` consumes a live Inference C
request plus its `raw/` directory. `pinned-inference` reads the immutable A
goldens with `git show` and constructs a verifier-owned request naming its own
clean B checkout. The B-side job cannot reuse A's exported request because that
request still names the pre-B verifier revision.

## Integrity and assumption checks

For every invocation, the discharger:

1. Sets `sys.dont_write_bytecode = True` before local imports and performs the
   production clean-HEAD/revision gate before importing or executing sibling
   implementation modules wherever practicable. Bridges and CI also set
   `PYTHONDONTWRITEBYTECODE=1`, so a read-only probe cannot dirty the checkout.
2. Verifies that the local wasm-verifier checkout is clean and exactly the
   requested pinned revision.
3. Verifies Coq 8.20 and the configured container, then observes the installed
   coq-wasm pin from both its normalized opam URL/tag and the exact commit in
   the switch source checkout. Missing source provenance or either mismatch is
   a hard failure.
4. Hashes the received raw file.
5. Verifies its basename, expected generated skeletons, theorem set, and
   reciprocal-pin hash where applicable.
6. Compiles the raw file unchanged against the real libraries.
7. Hashes it again and requires byte identity.
8. Lexically rejects proof holes and kernel bypasses in companion/gate sources.
9. Compiles the independent proof companion.
10. Compiles every rebound theorem whose target comes from the raw theorem type.
11. Runs `Print Assumptions` for every rebound endpoint.
12. Rejects any raw-namespace assumption and any assumption absent from
    `ci/assumptions.allow`.

All production subprocesses have fixed class limits. Git, opam, provenance,
and other metadata probes time out after 60 seconds; Dune builds and `coqc`
proof commands time out after 1,800 seconds. Injected-runner tests cover both
classes and timeout failures. No production CLI flag can bypass or increase a
limit.

The per-case footprint may be a subset of `ci/assumptions.allow`; unused
upstream assumptions are not required. wasm-verifier's existing full-build
assumption audit continues to require the complete reviewed baseline, catching
unexpected removal as well as addition at the repository level.

An endpoint reported as `Closed under the global context` contributes no name
to that union. A dependency repeated by several endpoint blocks is counted
once. A single-case invocation requires exactly that case's proved-plus-refuted
blocks; complete batch and reciprocal-pin runs require all twelve.

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

The two directions deliberately construct different requests during the
bootstrap. wasm-verifier at B reads raw goldens from commit A but constructs a
verifier-side request whose expected verifier revision is its own clean B
checkout. It cannot consume A's exported request because A still names the old
verifier pin. Only Inference C exports a live request naming B. The live
Inference pull-request lane always sends freshly generated bytes, not commit
A's golden, so later producer drift fails even while the historical reciprocal
pin remains stable.

## Docker-only local verification

Host Rocq output is not accepted as verification evidence.

The local verification entry point orchestrates two isolated environments:

1. A `rust:1.98-bookworm` image pinned by digest runs Rust 1.98.0. The declared
   workspace MSRV is 1.91, but the current `inference-tests` dependency graph
   includes Wasmtime/Cranelift releases whose metadata requires Rust 1.94; the
   measured 1.91 Docker build therefore fails before this issue's tests run.
   This lane verifies issue #450 on the current stable toolchain and is not
   evidence that the separate 1.91 MSRV claim passes. A tracked lane-specific
   lockfile, `ci/rocq-discharge.cargo-lock`, fixes the exact dependency graph
   without changing the repository's ignored root-lock policy. The wrapper
   copies the current Inference checkout into a disposable writable source
   volume, installs that tracked file as snapshot-root `Cargo.lock`, and asserts
   the observed compiler and lock hash before building. Fetching runs once with
   `--locked`; then every build/test is offline and locked. The host checkout
   remains untouched. Cargo registry and Linux build artifacts live in
   dedicated named volumes. Export always freshly generates all five artifacts,
   requires each to equal its committed golden, and writes those raw bytes plus
   a machine-readable request to a temporary exchange volume. The focused test
   filter and the complete `inference-tests` crate run only when `--full` is
   selected.
2. Docker cannot attach that exchange volume to an already-running container.
   A host-side bridge therefore uses a transient volume-mounted copy container
   plus `docker cp` to move the exact request directory into the existing
   `wasm-verifier` Coq 8.20 devcontainer. The verifier runs the batch discharger
   there, and the bridge copies only the receipt directory back to the exchange
   volume. The same bridge also exposes the configured single-case executable
   contract by copying one host raw file in and exactly one receipt out. The
   five ordered invocations use five caller-owned fresh empty `0700` receipt
   directories, each of which must receive exactly one regular nonsymlink,
   single-link `0600` `<case-id>.json`. The same Rust image then runs the
   harness in verify mode, validating every receipt and the aggregate floors
   against the exported request.

The verifier's `ci/discharge/container-pin.json` has this exact canonical
eight-line grammar, including field order, commas, and final newline:

```json
{
  "protocol": 1,
  "image_reference": "coqorg/coq:8.20",
  "image_id": "sha256:e50d77c4c5a9aa0d76ae1b343d79c5f922da3a75054b79c5dc635895438e4674",
  "coq_user": "coq",
  "repository_mount": "/workspaces/wasm-verifier",
  "coq_version": "8.20.1"
}
```

It deliberately does not duplicate the manifest's coq-wasm tag/revision or
Coq series. The image ID is the supported local image/config identity, not a
portable action-container assertion. The local inspector
`ci/discharge/inspect-container.sh` must exit zero and emit exactly these eight
newline-terminated records, in order, with no duplicate or extra output:

```text
coq_user=coq
coq_uid=<canonical-positive-decimal>
coq_gid=<canonical-positive-decimal>
wasm_verifier_revision=<canonical-lowercase-40-hex-B>
coq_version=8.20.1
coq_wasm_origin=https://github.com/WasmCert/WasmCert-Coq.git
coq_wasm_tag=v2.2.0
coq_wasm_revision=0fd83fa708922721132b6d6737179568d1f1d553
```

The running local verifier container must expose exactly one mount in total:
a `bind` whose source is the canonical verifier checkout and whose destination
is `/workspaces/wasm-verifier`. Any extra bind or volume, Docker socket, alias,
parent mount, different type/source/destination, or second entry fails. A
literal `/var/run/docker.sock` substring scan is not an adequate replacement
for this exact mount-list comparison.

The five verifier-side bridge contract artifacts are
`container-pin.json`, executable `inspect-container.sh`, executable shared
`docker-bridge.sh`, and the two executable public adapters
`run-docker-batch.sh` and `run-docker-case.sh`. The Inference wrapper checks the
pin, inspector, shared helper, and each adapter configured for the selected
mode before and after every bridge; mode `both` therefore checks all five.
Every artifact must match clean Git content and remain a regular nonsymlink
with one hard link. `run-docker-batch.sh` accepts exactly
`--exchange-volume <validated-name>`. `run-docker-case.sh` accepts exactly the
protocol-1 single-case argv and the wrapper-supplied caller-owned fresh empty
`0700` receipt directory, then returns exactly the corresponding regular
nonsymlink, single-link `0600` receipt. Both source their common
copy/container/cleanup primitives from the shared helper and set
`PYTHONDONTWRITEBYTECODE=1` for Python execution.

The host script only orchestrates containers, copies opaque directories, and
validates container and bridge contract status; it does not compile Rust or
Rocq and performs no receipt interpretation. The Rust container is not given
the Docker socket. The bridge receives the validated private evidence
directory contract described above. The exchange and source-snapshot volumes
are deleted after verification using exact-name enumeration and ownership
checks, while the Linux Cargo target and registry volumes are reused serially
rather than duplicated per lane.

Once verifier B exists and Inference C pins it, complete local evidence is:

- each proof companion builds in the Coq 8.20 container;
- the proof-completeness scanner reports zero holes across both `theories` and
  `ci/discharge/proofs`, and wasm-verifier's full `coqbuild.sh` succeeds with
  its existing assumptions audit;
- the verifier-side pinned-artifact discharge check succeeds;
- the Inference targeted Rocq/golden/case tests succeed in the Rust container;
- the end-to-end five-case discharge run reports exactly eleven proved and one
  refuted endpoint;
- both the batch-volume and configured single-case host bridges independently
  produce receipts that the Rust verifier accepts;
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
assumption audit. The public message names the stable case and phase, gives one
bounded structured diagnostic, and emits a sanitized local evidence locator.
The full container log and ephemeral sources remain private and local for
reproduction; their contents are never copied into public output.

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
external raw artifacts, not generated-looking source copies. Inference's
changelog and wasm-verifier's verifier/CI documentation record the initial case
floor, the negative certificate, and the reciprocal pins (wasm-verifier does
not currently maintain a changelog).

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
- Inference's targeted and broader Rust tests pass under the exact Rust 1.98
  tracked-lock lane; the canonical product wrapper independently runs its Rust
  phases in Docker, while the explicitly authorized implementation and final
  verification evidence lane may use validated host snapshots through Task 12.
- CI reports elaboration and dischargeability as separate, accurately scoped
  claims.
