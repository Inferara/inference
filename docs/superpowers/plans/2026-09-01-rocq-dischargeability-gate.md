# Exact Rocq Artifact Dischargeability Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Make issue #450 fail closed by independently proving every generated theorem in four representative fresh Rocq artifacts and independently refuting a known-false fifth artifact.

**Architecture:** Inference generates and hashes untouched raw `.v` files, validates a strict request/receipt protocol, and owns CI orchestration. wasm-verifier imports each raw module under an isolated namespace, proves its exact generated theorem types in separate companions, rebinds those proofs, and audits `Print Assumptions` so no raw `Admitted.` theorem or unreviewed axiom can satisfy the gate.

**Tech Stack:** Rust 1.98.0, Cargo, serde/serde_json, SHA-256, Python 3 standard library, Rocq/Coq 8.20, Dune, opam, Docker, GitHub Actions.

**Spec:** `docs/superpowers/specs/2026-09-01-rocq-dischargeability-gate-design.md`

## Global constraints

- Work directly on `450-bug-fix-rocq-dischargeability` in both repositories. Do not create worktrees and never implement on `main`.
- Every Rust implementation task is assigned to a rust-developer agent. A second agent reviews each Rust task before its commit.
- Follow test-driven development: add the named failing test, run it and record the expected failure, implement only enough to pass, then run the focused regression set.
- Run local compilation and tests only in Docker. Host commands may inspect files, orchestrate Docker, and perform version-control operations; they may not compile Rust or Rocq.
- Keep the existing unbounded `rocq_prime_example` source and golden byte-for-byte unchanged and explicitly uncertified.
- Do not copy generated definitions into wasm-verifier. Companion files import `DischargeCase.Raw` and contain only independent proof support and certificate endpoints.
- Do not splice proof bodies into generated files. `Raw.v` is hashed before and after compilation and must retain the exact producer bytes.
- Use the A → B → C landing sequence. wasm-verifier commit B pins Inference commit A; Inference commit C pins verifier commit B. Never create a circular “latest branch” dependency.
- The public floor is exactly five ordered cases, eleven proved endpoints, and one refuted endpoint. Any addition is a reviewed manifest and floor change.
- Keep private proof diagnostics out of public CI logs. Public failure text is limited to case ID, phase, and one bounded diagnostic line.
- Reuse one Linux Cargo target volume for the whole session. Do not create per-agent target directories.
- Treat Rust 1.98 as this gate's verified build toolchain, not as MSRV evidence. The workspace declares Rust 1.91, but the current `inference-tests` Wasmtime/Cranelift graph requires at least Rust 1.94 and the measured Rust 1.91 Docker build fails before the issue tests run; do not hide or “fix” that separate pre-existing mismatch in this PR.
- If a touched source file would exceed 25,000 tokens, split it by concern before adding more code.

## Stable protocol and case contract

The Inference case order is fixed:

| ID | Source | Module/golden basename | Proved | Refuted |
| --- | --- | --- | ---: | ---: |
| `prime-bounded` | `rocq_prime_bounded_example.inf` | `rocq_prime_bounded_example` | 2 | 0 |
| `exists` | `rocq_exists_spec.inf` | `rocq_exists_spec` | 3 | 0 |
| `unique` | `rocq_unique_spec.inf` | `rocq_unique_spec` | 3 | 0 |
| `narrow-domain` | `spec_narrow_discharge.inf` | `spec_narrow_discharge` | 2 | 0 |
| `false-spec` | `rocq_false_certificate.inf` | `rocq_false_certificate` | 1 | 1 |

`request.json` is strict JSON with protocol `1`, the expected verifier/coq-wasm/Coq pins, the allowlist-size ceiling, ordered case records, and aggregate floors. Each case record contains only its ID, raw basename, SHA-256, and proved/refuted counts. Raw files live at `raw/<basename>.v`.

Each strict `receipts/<case-id>.json` contains:

```json
{
  "protocol": 1,
  "case_id": "prime-bounded",
  "raw_basename": "rocq_prime_bounded_example.v",
  "raw_sha256": "<64 lower-case hex characters>",
  "wasm_verifier_pinned": "<40 lower-case hex characters>",
  "wasm_verifier_observed": "<same revision>",
  "coq_wasm_pinned": "<40 lower-case hex characters>",
  "coq_wasm_observed": "<same revision>",
  "coq_version": "8.20",
  "proved": 2,
  "refuted": 0,
  "audited_endpoints": 2,
  "allowlisted_dependencies": 0,
  "raw_namespace_dependencies": 0,
  "unapproved_dependencies": 0,
  "result": "pass"
}
```

The exact `coq_version` value may include a patch/suffix, but its parsed major/minor must be `8.20`. `allowlisted_dependencies` is the cardinality of the distinct union of allowlisted names across all endpoint audit blocks in that case; repeated names count once and `Closed under the global context` contributes zero. Unknown JSON fields, duplicate keys, wrong directory entries, non-canonical hashes, totals mismatches, stale receipts, and extra receipts all fail.

The configured single-case executable contract is:

```text
INFERENCE_WASM_VERIFIER_RECEIPT_DIR=<new-empty-absolute-directory>
<discharger> --protocol 1 \
  --wasm-verifier-revision <pin> \
  --case <stable-id> \
  <fresh-raw-file.v>
```

## Phase A — producer fixtures, protocol, and Docker exporter in Inference

### Task 0: Establish the reusable Docker test lane

**Files:**

- Create: `ci/rocq-discharge.cargo-lock`
- Create: `ci/rocq-rust-docker.sh`
- Create: `ci/rocq-rust-docker-self-test.sh`

- [ ] Reconfirm the Coq baseline in the existing Coq 8.20 verifier devcontainer: `dune build`, proof completeness, and the assumptions audit must pass with the existing ten-name baseline.
- [ ] Reconfirm the Rust baseline with the pinned Rust 1.98 image and exact preinstalled toolchain, using `sh -c` rather than a login shell. The focused `rocq_typecheck::` suite must retain the measured baseline of 23 passed and eight ignored regeneration helpers.
- [ ] Record the failed Rust 1.91 preflight (Wasmtime/Cranelift require Rust 1.94) in the execution notes. Do not report the Rust 1.98 run as validation of the declared 1.91 MSRV.
- [ ] In the pinned Rust container, resolve the current workspace graph once and save the exact generated lock bytes as tracked `ci/rocq-discharge.cargo-lock`. The lane never selects an ignored host lock and never resolves dependencies implicitly after this step.
- [ ] First write the shell self-test against a fake Docker executable and observe it fail because the helper is absent. Cover image/toolchain selection, lane-lock copying and mismatch rejection, explicit Cargo paths, no socket mount, target locking, and exact cleanup targets.
- [ ] Create/reuse one explicitly named Cargo registry volume and one target volume. Serialize access with a validated lock container or equivalent atomic Docker primitive before any later task uses the target volume.
- [ ] Implement `ci/rocq-rust-docker.sh` as the common Phase A command runner: snapshot the checkout, copy the tracked lane lock to snapshot-root `Cargo.lock`, log both hashes, fetch with `--locked`, disable networking, and run the requested Cargo command with `--locked --offline`. Set `RUSTUP_TOOLCHAIN`, `CARGO_HOME=/cargo-home`, and `CARGO_TARGET_DIR=/cargo-target` explicitly; do not mount over `/usr/local/cargo`.
- [ ] Make every later Phase A TDD command use this helper, which Task 4 composes rather than reimplementing.

### Task 1: Extract reusable fresh-Rocq generation support

**Files:**

- Create: `tests/src/rocq_test_support.rs`
- Modify: `tests/src/lib.rs`
- Modify: `tests/src/rocq_typecheck.rs`

- [ ] Assign this task to a rust-developer agent and ask it to preserve every existing generated byte.
- [ ] Add a focused characterization test in `rocq_typecheck.rs` that compares the old helper path and the new support path for `rocq_exists_spec.inf`.
- [ ] Run in `rust:1.98-bookworm@sha256:82150a52ec202c1b14d7817e14516c392bb7f5cfebd88f1ed531cb37ebd39922`:

  ```sh
  cargo test --locked -p inference-tests rocq_typecheck::gate::shared_generator_preserves_existing_output -- --exact
  ```

  Expected before implementation: compile failure because `crate::rocq_test_support` does not exist.

- [ ] Move the single-file `compile_fixture`, `translate`, and `generate_v` implementation into `rocq_test_support.rs` as `pub(crate)` functions. Keep `generate_linked_v` and linked-only types in `rocq_typecheck.rs`.
- [ ] Register `mod rocq_test_support;` in `tests/src/lib.rs` and switch existing golden tests to the shared function.
- [ ] Run the new characterization test and the full `rocq_typecheck::` filter in the Rust container. Expected: existing goldens remain byte-identical; 23 existing behavioral tests pass and eight regeneration helpers remain ignored before new tests are added.
- [ ] Have a second agent review the diff specifically for changed codegen options, fixture lookup, module naming, or translator inputs.

### Task 2: Add bounded-prime, false, and narrow-domain producer goldens

**Files:**

- Create: `tests/test_data/inf/rocq_prime_bounded_example.inf`
- Create: `tests/test_data/inf/rocq_false_certificate.inf`
- Create: `tests/test_data/rocq/rocq_prime_bounded_example.v`
- Create: `tests/test_data/rocq/spec_narrow_discharge.v`
- Create: `tests/test_data/rocq/rocq_false_certificate.v`
- Modify: `tests/src/rocq_typecheck.rs`
- Modify: `tests/src/stock_validity.rs`

- [ ] First add three failing byte-golden tests and their shape assertions. The bounded test must assert the exact nesting of the local-0 lookup guard, signed `n <= 2_000_000_000` antecedent, existing `n > 1`/primality obligations, and two generated theorems, plus no change to the old prime golden. The false test must assert the singleton payload contains `HA_not (term_eq (T_const (Vi32 0)) (T_const (Vi32 0)))`. The narrow test must assert exactly two payloads and two theorems.
- [ ] Add regeneration helpers only inside the existing `#[ignore]`d `regenerate` module, which is the repository’s documented exception for non-behavioral golden writers.
- [ ] Run the three tests in Docker. Expected before fixtures/goldens exist: explicit missing-source or missing-golden failures.
- [ ] Copy the executable `is_prime` body into the bounded sibling and add the source-visible bound before the existing primality assumptions:

  ```inf
  let n: i32 = @;
  assume { assert(n <= 2000000000); }
  assume { assert(n > 1); }
  ```

  Keep the remainder logically identical to `rocq_prime_example.inf`, but name the spec `prime_properties_bounded`.

- [ ] Add the exact negative fixture:

  ```inf
  spec FalseCertificate {
      fn impossible() forall {
          assert(false);
      }
  }
  ```

- [ ] Generate all three `.v` files with the explicit regeneration tests inside the Rust container, then rerun the behavioral golden tests without `--ignored`.
- [ ] Add only the bounded-prime and false `.inf` files to the exhaustive `stock_validity` table and `rocq_typecheck::gate::CORPUS`; `spec_narrow_discharge.inf` is already present in both. Add an assertion that every selected source appears exactly once so the existing narrow entry cannot be duplicated.
- [ ] Run Docker tests for the five selected goldens plus `stock_validity::`. Expected: pass; old `tests/test_data/rocq/rocq_prime_example.v` has no diff.
- [ ] Review generated theorem names and record them from the files; do not infer them from naming conventions in later tasks.

### Task 3: Implement strict request/receipt validation and export/verify CLI

**Files:**

- Create: `tests/src/rocq_dischargeability/mod.rs`
- Create: `tests/src/rocq_dischargeability/cases.rs`
- Create: `tests/src/rocq_dischargeability/pin.rs`
- Create: `tests/src/rocq_dischargeability/protocol.rs`
- Create: `tests/src/rocq_dischargeability/direct.rs`
- Create: `tests/src/bin/rocq-discharge.rs`
- Modify: `tests/src/lib.rs`
- Modify: `tests/Cargo.toml`
- Modify: `core/wasm-to-v/wasm-verifier-pin.txt`

- [ ] Assign all Rust work in this task to a rust-developer agent.
- [ ] Add `serde.workspace = true` to normal dependencies and move the already-used `sha2 = "0.11"` from dev-dependencies to normal dependencies. Add no CLI dependency; parse the two fixed subcommands with `std::env::args_os`.
- [ ] Add failing table tests for exact order, unique IDs/basenames, safe `[a-z0-9-]+` IDs, five cases, eleven proved, one refuted, and golden theorem-count agreement.
- [ ] Add failing protocol tests for: unknown JSON fields; duplicate JSON keys; non-canonical hashes; missing/extra/duplicate receipts; wrong case/hash/basename/revision/Coq series/counts/result; `audited_endpoints != proved + refuted`; allowlisted count over the pin ceiling; nonzero raw/unapproved counts; stale non-empty receipt directories; and aggregate floor mismatch.
- [ ] Extend the pin parser with `assumption-allowlist-count 10`, retaining the existing verifier, coq-wasm, tag, and Coq fields as the only sources of truth.
- [ ] Implement `CaseSpec` as a private-invariant type and expose only validated accessors. The static table contains source name, module name, golden path, and expected counts; it contains no theorem type or verifier proof name.
- [ ] Expose a deliberately small public facade from the library crate (`pub mod rocq_dischargeability` with public `export_cli`/`verify_cli` entry points); the separate binary crate calls `inference_tests::rocq_dischargeability`, while case internals remain private.
- [ ] Implement strict serde request/receipt types with `#[serde(deny_unknown_fields)]`. Reject duplicate keys with a custom map visitor before deserializing into the typed structure.
- [ ] Implement `export(exchange)`:

  1. require an existing empty exchange directory;
  2. freshly generate every selected artifact;
  3. compare it byte-for-byte with its committed golden;
  4. write the fresh bytes under `raw/`;
  5. hash those exact written bytes;
  6. atomically write `request.json` last.

- [ ] Implement `verify(exchange)` to validate the request, re-hash raw files, require the exact receipt set, validate every field, and print exactly one success line:

  ```text
  rocq-discharge: result=pass cases=5 proved=11 refuted=1
  ```

- [ ] Implement CLI commands `export --exchange <absolute-dir>` and `verify --exchange <absolute-dir>`. Relative paths, symlinks escaping the exchange root, and non-empty export targets fail.
- [ ] Implement the configured test `rocq_dischargeability::direct::configured_dischargeability_gate`. If the executable is absent and `INFERENCE_ROCQ_DISCHARGE_REQUIRED` is unset, print an explicit `SKIPPED` statement. If required, absence is an error. Before trusting case receipts, invoke the executable with a same-basename byte-mutated raw probe and require rejection.
- [ ] Make that configured test exercise the live producer path: create a temporary exchange, call the common `export` implementation (fresh generation, golden equality, request naming the current verifier pin), run the malformed probe, invoke the single-case executable over all five files under that exchange's `raw/`, then call the common `verify` implementation and emit its sole success marker. It must never discharge committed golden bytes directly.
- [ ] Add fake-discharger tests for no-op success, nonzero exit, no receipt, malformed receipt, duplicate receipt, and valid receipt. Use a small cross-platform helper binary (or both Unix script and Windows `.cmd` implementations); normal test compilation and behavior must not assume a POSIX shell.
- [ ] Add an injectable test runner that mutates the exported raw file after the pre-hash and before the post-hash. Require an explicit `raw integrity` failure, covering mutation during compilation rather than only a file that was already wrong at entry.
- [ ] Run all `rocq_dischargeability::` tests in Docker, then run `cargo fmt --check` there. Have a second agent review path validation, process argument handling, strict JSON behavior, and the exact summary marker.

### Task 4: Add the Docker-only local orchestrator

**Files:**

- Create: `ci/rocq-discharge-docker.sh`
- Create: `ci/rocq-discharge-docker-self-test.sh`
- Modify: `README.md`

- [ ] Add shell-level dry-run/self-tests that use a fake bridge and verify exact argument forwarding, cleanup, exact volume selection, nonzero-exit propagation, target-volume serialization, and that no Rust container mount source or destination is `/var/run/docker.sock`.
- [ ] Pin the Rust image by the digest above and assert `rustc 1.98.0`. Select the preinstalled toolchain as `1.98.0-$(uname -m)-unknown-linux-gnu` so x86_64 and aarch64 use the same script without rustup network sync.
- [ ] Compose `ci/rocq-rust-docker.sh`: copy the checkout from a read-only bind into a disposable writable source volume, exclude `.git` and `target`, and copy tracked `ci/rocq-discharge.cargo-lock` to snapshot-root `Cargo.lock`. Require byte identity with the tracked lane lock, log its SHA-256, run `cargo fetch --locked`, then disable the network and run every build/test with `--offline --locked`. Never consult or modify an ignored host lockfile.
- [ ] Pin the copy helper as `busybox:1.37.0@sha256:9db7b59979c38555a39def84a31fb98b5296952f9e3afd4f6f11f05b07adfab0`. Create host staging paths with `mktemp -d`; create container temporary paths as the unprivileged container user; normalize and validate both before copying.
- [ ] Reuse persistent `inference-cargo-home-rust-1.98` and `inference-cargo-target-rust-1.98` volumes. Create unique source/exchange volumes with a run-specific prefix and ownership label. In the trap, remove only exact names that pass both prefix and label checks, then remove the one resolved staging directory. Never clean by glob or unresolved variable.
- [ ] Run the Rust export in a container with no Docker socket. After fetch, use `--network none`, a read-only root filesystem, `--cap-drop ALL`, `no-new-privileges`, and a tmpfs `/tmp`. Apply the same network/read-only/capability/security restrictions to every BusyBox helper container and remove helpers before deleting their volumes.
- [ ] Call `<wasm-verifier>/ci/discharge/run-docker-batch.sh --exchange-volume <name>` on the host. That script is an opaque Docker bridge; this Inference wrapper must not parse receipts or private logs.
- [ ] Run Rust verify against the same exchange volume, again without a Docker socket or network.
- [ ] Support `--adapter batch|single|both` with `both` as the verification default. For `single`, export in Rust Docker, copy each raw file to one validated `0700` host staging directory with the helper, invoke the host `run-docker-case.sh` five times, copy the five receipts back to the exchange volume, and run Rust verify. For `both`, verify batch receipts, replace only the validated receipt directory, then exercise and verify the single-case path over the same immutable request/raw bytes.
- [ ] Before invoking either bridge, inspect the configured running verifier container and require the image ID/reference, unprivileged `coq` execution user, repository mount path, and Coq version recorded in verifier B's `ci/discharge/container-pin.json`, plus the exact clean requested revision and observed coq-wasm provenance used by Dune. Reject any mismatch.
- [ ] Support `--full` to run the targeted tests, end-to-end export/verify, and the entire `inference-tests` crate serially in the same target volume. State that this is crate scope, not the full workspace. After the Phase A tests land, sum every test-binary result line from a clean Docker run, encode that concrete numeric floor in the script (no placeholder), and test the multi-binary parser so a silently empty filter cannot pass.
- [ ] Remove all host staging on success. On failure, first copy the full private verifier log to exactly one validated verifier-host `0700` evidence directory, retain only bounded/sanitized Rust-orchestrator evidence on the public Inference side, print the exact local evidence path, and then remove all helper containers and container-side temporary state. Never publish or retain raw private proof sources in a public artifact.
- [ ] Document prerequisites: Docker, the tracked lane lock, a clean verifier branch at the pinned revision, and the expected running `wasm-verifier` Coq 8.20 container/image. State that an ignored local root `Cargo.lock` is neither read nor accepted as evidence.
- [ ] Do not claim end-to-end success in Phase A: the verifier bridge and proofs do not exist until commit B.

### Task 5: Document and commit Inference producer commit A

**Files:**

- Modify: `core/wasm-to-v/ROCQ_CONTRACT.md`
- Modify: `CHANGELOG.md`
- Modify: `docs/superpowers/specs/2026-09-01-rocq-dischargeability-gate-design.md` only if implementation exposed a reviewed contract correction

- [ ] Document the distinction between vendored-stub elaboration, real-library elaboration, and exact-artifact dischargeability. List the five selected cases and state that the unbounded prime artifact remains uncertified.
- [ ] Record the producer fixtures, strict protocol, negative certificate, Docker-only local path, and A→B→C pin sequence in `CHANGELOG.md` without claiming the verifier lane is enabled yet.
- [ ] Run in the Rust Docker image: selected golden tests, all `rocq_dischargeability::` tests, `stock_validity::`, `cargo fmt --check`, and `cargo clippy -p inference-tests --all-targets -- -D warnings`.
- [ ] Inspect the complete diff and verify no unrelated refactor, old-prime change, generated proof edit, or host-built artifact is present.
- [ ] Commit the Phase A changes with a focused message such as `test: define emitted Rocq discharge contract`.
- [ ] Record the resulting 40-character commit as commit A. All verifier hashes and reciprocal reads in Phase B use that immutable revision, never the moving branch.

## Phase B — independent certificates and discharger in wasm-verifier

### Task 6: Build the fail-closed verifier harness first

**Files (in `../wasm-verifier`):**

- Create: `ci/discharge/manifest.json`
- Create: `ci/discharge/discharger.py`
- Create: `ci/discharge/coq_runner.py`
- Create: `ci/discharge/audit.py`
- Create: `ci/discharge/tests/test_discharger.py`
- Create: `ci/discharge/tests/fixtures/`
- Modify: `ci/check-proof-completeness.py`

- [ ] Add `--root PATH` (repeatable) to `check-proof-completeness.py` while preserving `theories` as its no-argument default. Extend its self-test for multiple roots, symlink loops, an empty root, and a hole in the second root.
- [ ] Create the manifest from commit A with a top-level canonical 40-hex `inference_revision`, exact ordered theorem names, and SHA-256 values. Its only theorem-semantic metadata is `prove`/`refute` polarity and companion endpoint names; it contains no theorem type. Require `--inference-revision` to equal that persisted A pin, resolve verifier `HEAD` to a canonical 40-hex B revision before request/receipt construction, test both mismatches, and assert exactly five ordered cases, eleven prove entries, one refute entry, and twelve endpoints.
- [ ] Define and test three concrete entry points in `discharger.py`:

  ```text
  discharger.py single --protocol 1 --wasm-verifier-revision <B> --case <id> <raw.v>
  discharger.py batch --request <request.json> --raw-dir <raw-dir> --receipt-dir <new-empty-dir>
  discharger.py pinned-inference --repo <checkout> --inference-revision <A> --wasm-verifier-revision HEAD --receipt-dir <new-empty-dir>
  ```

  `single` writes through `INFERENCE_WASM_VERIFIER_RECEIPT_DIR`. `batch` consumes the request exported by live Inference C. `pinned-inference` reads A's committed goldens with `git show`, constructs a verifier-owned temporary request expecting the clean current B revision, and must never consume A's request because A still names the pre-B verifier pin.
- [ ] Write failing Python unit tests for strict/duplicate-key request parsing, path traversal, wrong pin/hash/basename/theorem order, missing/extra skeletons, noncanonical generated admission shapes, stray theorem-like commands, nested Coq comments, proof holes, zero endpoints, raw mutation during compilation, receipt atomicity, exact coq-wasm provenance, and bounded public diagnostics.
- [ ] Implement lexical raw validation that permits `Admitted.` only in the exact generated skeleton:

  ```coq
  Proof.
    (* TODO: fill the proof *)
  Admitted.
  ```

  Tokenize Coq nested comments and strings correctly. The only top-level theorem/proof commands accepted are the manifest-listed ordered `Theorem` declarations followed by the exact skeleton above. Reject any top-level `Proof` outside such a skeleton, extra `Theorem` declarations, and every `Lemma`, `Remark`, `Fact`, `Corollary`, `Proposition`, `Example`, or `Goal`, plus proof-mode `Definition`, `Program Definition`/obligations, `admit`, `Abort`, axioms/parameters/conjectures, `Declare Module`/`Declare ML Module`, guard-check changes, and kernel-check bypasses everywhere else.

- [ ] Require `opam exec -- dune build` before discharge. Compile each case in a fresh private directory with the exact load-path shape `opam exec -- coqc -q -Q _build/default/theories WasmVerifier -Q <work> DischargeCase <file>`. Copy the received bytes to `Raw.v`, hash before/after, and compile in order: `Raw.v`, verifier-owned `Proofs.v`, generated `Rebind.v`, generated `Audit.v`.
- [ ] Observe coq-wasm from the installed opam pin, not from repository text alone: require the normalized pin URL/tag, discover exactly one validated coq-wasm source checkout under the active switch (allowing opam's version-suffixed layout), and resolve its `git rev-parse HEAD` to the pinned 40-character commit. Fail closed on zero/multiple candidates, missing Git metadata, or either mismatch; cover all observations with injected tests.
- [ ] Generate positive rebound endpoints with:

  ```coq
  Theorem gate_endpoint :
    ltac:(let T := type of @Raw.generated_theorem in exact T).
  Proof. exact (@Proofs.checked_endpoint). Qed.
  ```

- [ ] For the one refutation, open the same three-binder host context as the generated `ValidSpec` theorem, derive the proposition from `type of (@Raw.generated_theorem _ _ ho)`, negate it, and close it with `@Proofs.checked_refutation _ _ ho`. Do not restate `ValidSpec` manually. A real-Coq test must assert all generalized binders are preserved.
- [ ] Bracket each `Print Assumptions Rebind.gate_endpoint.` output with unique begin/end markers. Require exactly `proved + refuted` parseable blocks in each single-case execution (two or three), and exactly twelve only for a complete `batch`/`pinned-inference` aggregate. Parse both `Closed under the global context` and multi-line `Axioms:` output; reject duplicate/missing markers, shortened or fully qualified Raw namespace dependencies, and every unknown name. Count `allowlisted_dependencies` as the distinct union per case, not the sum of block occurrences.
- [ ] Write receipts to a temporary file, flush and fsync the file, close it, fsync the directory where supported, and rename atomically only after all phases and a post-compilation raw re-hash pass. An injected runner must mutate Raw between compilation and the post-hash and prove that no receipt is published.
- [ ] Add a real-Coq self-test whose cheating companion closes `False` with a raw admitted theorem. It must compile and then fail specifically in the raw-namespace assumptions audit. Also test an independently closed `True` endpoint succeeds.
- [ ] Keep the production revision/clean-tree check strict. Unit tests may inject a revision probe, but must not add a bypassing CLI flag. Commit this harness as a focused intermediate verifier commit, then run the real audit self-test from that clean commit in the Coq 8.20 devcontainer.

### Task 7: Add the negative and narrow-domain companions

**Files (in `../wasm-verifier`):**

- Create: `ci/discharge/proofs/FalseSpec.v`
- Create: `ci/discharge/proofs/NarrowDomain.v`

- [ ] Add manifest-selected case tests first. Expected: `false-spec` and `narrow-domain` fail at “companion missing.”
- [ ] In `FalseSpec.v` import `DischargeCase.Raw` and the verifier semantics/automation libraries. Prove `checked_valid_module` with `valid_module`.
- [ ] Prove `checked_valid_spec_is_false` by applying `ValidSpec_witness` at `aheap_empty`, taking `List.Forall_inv` of the singleton payload, rewriting `ktrue_neq_denoteE`, and contradicting the identical `Vi32 0` denotations. The target mentions only `Raw.rocq_false_certificate` and `Raw.rocq_false_certificate__FalseCertificate_specs`.
- [ ] In `NarrowDomain.v` import Raw before MathComp. Port proof support from `theories/examples/Issue357NarrowDomainExample.v:113` onward, deleting all generated definitions. Bind exact aliases for `Raw.Vi32`, `Raw.spec_narrow_discharge`, `Raw.spec_narrow_discharge__NarrowDischarge_hspec1`, `Raw.spec_narrow_discharge__NarrowDischarge_hspec2`, and `Raw.spec_narrow_discharge__NarrowDischarge_specs`; all other short names remain proof-owned helpers.
- [ ] Export exactly `checked_valid_module` and `checked_valid_spec`. Their statements directly name Raw definitions; neither references a raw generated theorem.
- [ ] Unit-test both companions while dirty through the injected harness, then commit them as a focused intermediate verifier commit and run both real cases from that clean commit in the Coq 8.20 container. Expected totals: three proved, one refuted; every rebound endpoint has zero raw/unapproved assumptions.
- [ ] Mutate the false raw payload to `HA_true` in an ephemeral copy, update only the test's temporary hash, and confirm the refutation compilation fails. Separately mutate only the temporary manifest polarity from `refute` to `prove` and require a target-rebinding type mismatch. For the audit-specific mutation, inject an ephemeral positive companion endpoint whose body is `exact (@Raw.<admitted-valid-spec> _ _ ho)` and point the temporary manifest at it; compilation must succeed and the Raw dependency audit must fail. None of these mutations touches the repository.

### Task 8: Port the exact exists and unique certificates

**Files (in `../wasm-verifier`):**

- Create: `ci/discharge/proofs/Exists.v`
- Create: `ci/discharge/proofs/Unique.v`

- [ ] Add both selected case tests first. Expected: fail at “companion missing.”
- [ ] For `Exists.v`, import Raw, then port proof material beginning with the proof-only imports around `theories/examples/rocq_exists_spec.v:126` through its end. Remove lines that define generated values, functions, module records, hasserts, reachability records, or theorem statements. Bind only exact Raw aliases for `Vi32`, `Vi64`, `double`, `ex_double`, `rocq_exists_spec`, `rocq_exists_spec__ReachableDouble_specs`, `rocq_exists_spec__ReachableDouble_exspec1`, and `rocq_exists_spec__ReachableDouble__ex_specs`.
- [ ] Rename the three final endpoints to `checked_valid_module`, `checked_valid_spec`, and `checked_valid_exists_spec` and state them over `Raw` definitions.
- [ ] For `Unique.v`, import Raw before the MathComp imports, port the proof-only support around `theories/examples/rocq_unique_spec.v:31` and `:112` onward, and remove the generated module/function/spec definitions. Bind only exact Raw aliases for `Vi32`, `uq_parity`, `rocq_unique_spec`, `rocq_unique_spec__UniqueParity_specs`, `rocq_unique_spec__UniqueParity_uqspec1`, and `rocq_unique_spec__UniqueParity__uq_specs`.
- [ ] Rename its endpoints to `checked_valid_module`, `checked_valid_spec`, and `checked_valid_unique_spec`, again over `Raw` definitions.
- [ ] Use `%num` only in handwritten proof text. Never edit/re-delimit the already-compiled raw `%N` bytes.
- [ ] Unit-test each case separately while dirty, commit both companions as a focused intermediate verifier commit, then run them from that clean commit individually and together with Task 7. Expected cumulative totals: nine proved and one refuted across four cases.
- [ ] Run the assumptions audit with a deliberate temporary `exact @Raw.<admitted theorem>` substitution. Compilation must remain green and the audit must turn red, proving the shortcut detector has teeth.

### Task 9: Port and adapt the bounded-prime certificate

**Files (in `../wasm-verifier`):**

- Create: `ci/discharge/proofs/PrimeBounded.v`

- [ ] Add the selected case test first. Expected: fail at “companion missing.”
- [ ] Import Raw before MathComp and import the already compiled `WasmVerifier.examples.rocq_prime_example` proof-support module. Reuse its arithmetic/execution lemmas, but do not copy its roughly 2,800-line proof region, historical generated module, emitted hassert, or final theorem. Its historical outer-bound wrapper is not the exact new payload and cannot serve as the certificate endpoint.
- [ ] Load the legacy module through a qualified module alias so its short generated names do not collide. Before adapting the payload proof, add real-Coq characterization goals that the exact Raw `is_prime` function and module are convertible to the corresponding legacy definitions. Do not assert payload conversion: its new bound nesting intentionally differs.
- [ ] Bind short local notations only to the exact `Raw.rocq_prime_bounded_example` function/module/payload definitions. The certificate may define mathematical helpers such as `N_MAX` and loop invariants; it may not define a second generated-looking module or hassert.
- [ ] Prove `checked_valid_module : ValidModule Raw.rocq_prime_bounded_example`.
- [ ] Prove `checked_valid_spec` directly over `Raw.rocq_prime_bounded_example__prime_properties_bounded_specs`. Unfold the exact raw payload, handle the inner `val_loc 1 = None` guard at its emitted nesting level, split the local-0 typing/signed-bound cases, bridge the exact Raw function and payload shapes to the imported in-range primality computation lemmas, and discharge the out-of-range branch by refuting the visible bound.
- [ ] Unit-test `prime-bounded` while dirty, commit the companion as a focused intermediate verifier commit, then run it from that clean commit in Coq 8.20 before running all five cases. Expected: exactly eleven proved, one refuted, twelve audited endpoints, and zero raw/unapproved dependencies.
- [ ] Confirm the existing unbounded raw golden is not accepted under the `prime-bounded` case ID (basename/hash failure) and has no manifest entry of its own.
- [ ] Ask a proof-focused reviewer to compare the raw bound, interpreter domain, arithmetic cap, and final theorem target. Do not proceed on a merely elaborating adapted statement.

### Task 10: Add reciprocal-pin CI, Docker bridge, docs, and commit B

**Files (in `../wasm-verifier`):**

- Create: `ci/discharge/docker-bridge.sh`
- Create: `ci/discharge/container-pin.json`
- Create: `ci/discharge/run-docker-case.sh`
- Create: `ci/discharge/run-docker-batch.sh`
- Create: `ci/discharge/tests/test_docker_bridge.sh`
- Modify: `.github/workflows/build.yml`
- Modify: `README.md`
- Modify: `ci/check-proof-completeness.py` if review found scanner gaps

- [ ] Record the supported local container contract in `container-pin.json`: image reference `coqorg/coq:8.20`, image/config ID `sha256:e50d77c4c5a9aa0d76ae1b343d79c5f922da3a75054b79c5dc635895438e4674`, user `coq`, repository destination `/workspaces/wasm-verifier`, and Coq `8.20.1`. Keep CI's action-container validation version/provenance based; the local image ID is not assumed portable to another platform/container implementation.
- [ ] First write `test_docker_bridge.sh` with a fake Docker executable and observe failures for the absent adapters. Cover exact single/batch arguments, pin mismatches, unsafe names/paths, wrong user/mount, helper hardening/cleanup, receipt-only copying, and exit propagation.
- [ ] Implement shared `docker-bridge.sh` copy/validation primitives plus both public adapters. `run-docker-batch.sh --exchange-volume <validated-name>` copies `request.json` and `raw/` from the volume to one `mktemp -d` staging directory with the pinned BusyBox helper, `docker cp`s that opaque directory into a fresh path in the already-running verifier container, executes `discharger.py batch` there as `--user coq`, copies only `receipts/` back, and replaces the exchange volume's receipt directory atomically.
- [ ] Implement `run-docker-case.sh` with the exact configured-executable arguments from the protocol and the host `INFERENCE_WASM_VERIFIER_RECEIPT_DIR`. It copies the one raw file into a fresh verifier-container directory, executes `discharger.py single` as `coq`, and copies exactly `<case-id>.json` to the caller's new empty receipt directory. This is the direct host-to-container adapter used by Inference C.
- [ ] Pin the copy helper by the BusyBox digest from Task 4 and harden it with no network, a read-only root, dropped capabilities, and `no-new-privileges`. Validate container/volume names, case IDs, basenames, absolute paths, the local pin's image ID/reference, execution user, repository mount destination, clean requested revision, Coq version, and coq-wasm provenance before copying. Use argument arrays/no `eval`, correct copied-file ownership before executing as `coq`, label helper containers, remove them before volumes, and use traps that validate exact container/staging targets before cleanup.
- [ ] Keep full failure logs private: before container cleanup, copy them to one validated verifier-host `0700` evidence directory and print that local path only to an authorized local operator. Emit only the bounded public diagnostic required by the protocol to Inference/CI output.
- [ ] Add a pinned-artifact CI step that checks out public Inference at commit A into a runner-temp or sibling directory outside the verifier checkout, passes it to `discharger.py pinned-inference`, reads each golden with `git show <A>:<path>`, constructs a verifier-side request expecting the current clean B checkout, and runs all five cases in the Coq 8.20 action container. Never place A inside B's tree, dirty the verifier root, or read an A-era exported request.
- [ ] Run `check-proof-completeness.py --root theories --root ci/discharge/proofs` before Dune and keep the existing no-argument behavior tested. Then run `opam exec -- dune build` before any real discharger command so the exact `WasmVerifier` load path exists.
- [ ] Document exact-artifact certificates, the names-only manifest, local Docker bridge, assumption policy, and why raw admissions are allowed only in `Raw.v`.
- [ ] Before the final commit, run Python unit/static/bridge self-tests, proof-completeness self-test and both roots, `dune build`, and `ci/check-assumptions.sh`. Do not invoke the production pinned-artifact mode while bridge/workflow/docs changes leave the verifier tree dirty. Treat the two-root scanner and `coqbuild.sh`/Dune as separate evidence; do not claim the existing wrapper scans the new proof root unless it is explicitly changed and tested to do so.
- [ ] Commit the bridge/workflow/docs as the final focused Phase B commit such as `ci: certify exact generated Rocq artifacts`, record the resulting clean 40-character HEAD as B, then run the production `pinned-inference` discharge and inspect all five audit blocks/aggregate receipt from that clean revision. Require `rocq-discharge: result=pass cases=5 proved=11 refuted=1`. If it fails, fix and commit again, redefine B, and rerun; never preserve the failed SHA as the pin. Earlier focused proof commits are part of B's ancestry but are not reciprocal pins.

## Phase C — pin verifier B and enable the Inference CI lane

### Task 11: Wire the configured discharger and separate CI claim

**Files (in Inference):**

- Modify: `core/wasm-to-v/wasm-verifier-pin.txt`
- Modify: `tests/src/rocq_dischargeability/pin.rs`
- Modify: `.github/workflows/rocq-real-library.yml`
- Modify: `core/wasm-to-v/ROCQ_CONTRACT.md`
- Modify: `CHANGELOG.md`

- [ ] Assign the Rust pin/parser changes to a rust-developer agent and first add a failing test that expects verifier commit B and the reviewed allowlist ceiling.
- [ ] Replace only the verifier revision in the pin with commit B; retain the existing coq-wasm tag/commit and Coq 8.20 series.
- [ ] Wire the workflow to read `WASM_VERIFIER_DISCHARGER`, document that the private runner's configured value must resolve to B's `ci/discharge/run-docker-case.sh`, and set that environment explicitly only in local Docker tests. Add a workflow-structure test for the single-case executable contract; do not mutate repository/environment variables as part of implementation.
- [ ] Add a hosted `dischargeability-gate` capability job separate from `real-library-gate`. It checks `WASM_VERIFIER_DISCHARGER`, emits an explicit “Selected-artifact dischargeability: SKIPPED” summary when absent, and preserves same-repository `ci:real-rocq` scheduling.
- [ ] Add a private `selected-artifact-discharge` job guarded by the `real-rocq` environment and runner group. Set `INFERENCE_ROCQ_DISCHARGE_REQUIRED=1`, configure the executable, and run only the direct gate filter with `--nocapture`.
- [ ] Require exactly one full-line success marker and exact totals. A normal Cargo “test result: ok” line is not evidence and must not satisfy the workflow check.
- [ ] Preserve `pull_request` rather than `pull_request_target`, keep no path filter, and document that environment approval—not the label—is the security boundary for executing PR Rust code near a private verifier.
- [ ] Assert the bootstrap direction explicitly in tests/docs: verifier B certifies immutable A goldens with its verifier-owned request; live Inference C freshly exports a different request naming B and invokes the single-case adapter. Neither side trusts a request naming the old verifier revision.
- [ ] Update contract/changelog wording from “producer protocol present” to “five-case exact-artifact gate pinned to verifier B,” including the eleven/one floor and negative certificate.
- [ ] Run focused pin, protocol, direct fake-discharger, and workflow-structure tests in the Rust Docker image.
- [ ] Commit Phase C with a focused message such as `ci: require selected Rocq discharge certificates`.

## Final verification and review

### Task 12: Prove the complete result in Docker

**Files:** no intended source changes; mutation artifacts remain ephemeral.

- [ ] Read and apply `superpowers:verification-before-completion` before making any success claim.
- [ ] Ensure both direct repository branches are clean and at the intended A/B/C history; do not create worktrees.
- [ ] From Inference run `ci/rocq-discharge-docker.sh --adapter both --full`. This must perform Rust export, both Coq 8.20 bridge/discharge adapters, Rust receipt verification after each, and the broader `inference-tests` suite without a host compiler.
- [ ] In the verifier Coq 8.20 container first run `check-proof-completeness.py --root theories --root ci/discharge/proofs`, then run its complete `coqbuild.sh` path. Expected: the separate multi-root scan reports zero holes, Dune passes, and the global assumptions audit remains exactly the reviewed ten-dependency baseline.
- [ ] Run the verifier pinned-artifact direction against commit A and the Inference live-artifact direction at commit C. Both must report the exact five/eleven/one floor.
- [ ] Run all load-bearing mutations in containers and require red results for: `/bin/true`, missing receipt, wrong hash, changed raw byte, missing/extra theorem, proof hole, direct raw-admission use, false payload changed to true, and unbounded prime substituted for bounded.
- [ ] Run `cargo fmt --check`, `cargo clippy -p inference-tests --all-targets -- -D warnings`, targeted tests, and full crate-scoped `cargo test -p inference-tests` in the pinned Rust container. Record the executed-test count and require it to meet the reviewed floor encoded by `--full`.
- [ ] Inspect `git diff main...HEAD` and `git status --short` in each repository. Confirm no temporary files, generated `.vo/.glob/.aux` files, Docker artifacts, lockfile changes, or unrelated refactors are tracked.
- [ ] Use `superpowers:requesting-code-review` with separate Rust/protocol, proof/soundness, Docker/security, and CI/pinning review focuses. Address findings through `superpowers:receiving-code-review` and rerun affected Docker checks.
- [ ] Use `superpowers:finishing-a-development-branch` only after every required check is green. Ask the user whether to push/open PRs; do not perform remote writes without that direction.

## Acceptance ledger

- [ ] Five freshly generated raw artifacts equal their committed goldens byte-for-byte.
- [ ] The old unbounded prime artifact is unchanged and outside the certified manifest.
- [ ] Every generated theorem is enumerated: ten positive-artifact endpoints plus the false module endpoint are proved; the false `ValidSpec` endpoint is refuted.
- [ ] Every rebound target is derived from `type of @Raw.<theorem>`; no theorem type is duplicated in JSON or handwritten in the gate.
- [ ] Twelve assumption blocks are parsed; none depends on `DischargeCase.Raw` or an unapproved assumption.
- [ ] Per-case allowlisted counts are distinct unions across endpoint blocks; closed blocks contribute zero and duplicate dependency names contribute once.
- [ ] Both batch-volume and configured single-case Docker adapters pass their self-tests and end-to-end paths without exposing a Docker socket to Rust containers.
- [ ] The reciprocal hashes and A/B/C revisions are immutable and non-circular.
- [ ] Missing capability is visibly `SKIPPED`; required mode is red.
- [ ] Local proof evidence came only from Rust 1.98 and Coq 8.20 Docker environments.
- [ ] Both repositories are clean on direct issue branches and ready for user-directed publication.
