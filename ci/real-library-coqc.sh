#!/usr/bin/env sh
# Reference oracle for the real-library Rocq lane in `tests/src/rocq_typecheck.rs`.
#
# Contract
# --------
# Invoked as `real-library-coqc.sh <file.v>`. Exit 0 means <file.v> type-checks
# against the REAL coq-wasm v2.2.0 + wasm-verifier libraries; any non-zero exit
# means it does not. Diagnostics go to stderr.
#
# The lane deliberately knows nothing beyond that. Where the private library is
# checked out, how it was built, which Coq built it and whether a container hop
# is involved all stay on this side of the boundary, so none of it is recorded
# in this public repository or in a public CI log. Point
# `INFERENCE_WASM_VERIFIER_COQC` at this script, or at any other executable
# honouring the same contract.
#
# The lane runs a provenance probe through this oracle before it trusts a single
# verdict, so a stand-in that merely exits 0 (`/bin/true`) is rejected rather
# than reported as a clean run.
#
# Modes
# -----
# Docker, for a machine whose own Coq is not 8.20 (a `.vo` built by Coq 8.20
# cannot be loaded by Rocq 9.x, and the error text for that skew reads nothing
# like contract drift):
#
#   WASM_VERIFIER_CONTAINER   a running container carrying Coq 8.20 and
#                             coq-wasm v2.2.0
#   WASM_VERIFIER_THEORIES    path INSIDE that container to the built
#                             `_build/default/theories`
#   WASM_VERIFIER_CONTAINER_USER  optional, defaults to `coq`
#   WASM_VERIFIER_CONTAINER_PATH  optional PATH prefix inside the container,
#                             defaults to the opam switch coqorg images ship
#
# Host, when this machine's own Coq built the library:
#
#   WASM_VERIFIER_THEORIES    path on this machine to the built theories
#   COQC                      optional, defaults to `coqc` on PATH
#
# `coq-wasm` itself is resolved by Coq's own load path in both modes; it is an
# installed library rather than a checkout, so it needs no flag here.

set -eu

if [ "$#" -ne 1 ]; then
    echo "usage: $0 <file.v>" >&2
    exit 2
fi

file=$1

if [ ! -f "$file" ]; then
    echo "$0: no such file: $file" >&2
    exit 2
fi

if [ -z "${WASM_VERIFIER_THEORIES:-}" ]; then
    echo "$0: set WASM_VERIFIER_THEORIES to the built wasm-verifier theories" >&2
    echo "$0: (a wasm-verifier checkout's _build/default/theories, after dune build)" >&2
    exit 2
fi

if [ -n "${WASM_VERIFIER_CONTAINER:-}" ]; then
    user=${WASM_VERIFIER_CONTAINER_USER:-coq}
    # Each invocation gets its own directory because `coqc` writes a `.vo`, a
    # `.glob` and an `.aux` beside its input, and the lane compiles many modules
    # whose names it does not control.
    work=/tmp/inference-real-library-$$-$(basename "$file" .v)
    docker exec -u root "$WASM_VERIFIER_CONTAINER" \
        sh -c "rm -rf '$work' && mkdir -p '$work'" >/dev/null
    docker cp "$file" "$WASM_VERIFIER_CONTAINER:$work/" >/dev/null
    docker exec -u root "$WASM_VERIFIER_CONTAINER" \
        chown -R "$user" "$work" >/dev/null
    # A LOGIN shell, and `opam exec`, so the switch is resolved by opam rather
    # than by a path written down here. A hardcoded `~/.opam/<switch>/bin` ties
    # this script to one image's switch name and fails as an unexplained exit
    # 127 ("coqc: not found") on any other. `WASM_VERIFIER_CONTAINER_PATH` stays
    # as an override for an image that sets neither up.
    status=0
    docker exec -u "$user" "$WASM_VERIFIER_CONTAINER" bash -lc \
        "${WASM_VERIFIER_CONTAINER_PATH:+PATH='$WASM_VERIFIER_CONTAINER_PATH':\$PATH; } \
         cd '$work' && { command -v opam >/dev/null && exec opam exec -- \
             coqc -R '$WASM_VERIFIER_THEORIES' WasmVerifier '$(basename "$file")'; } || \
         exec coqc -R '$WASM_VERIFIER_THEORIES' WasmVerifier '$(basename "$file")'" \
        >&2 || status=$?
    docker exec -u root "$WASM_VERIFIER_CONTAINER" rm -rf "$work" >/dev/null || true
    exit "$status"
fi

exec "${COQC:-coqc}" -R "$WASM_VERIFIER_THEORIES" WasmVerifier "$file" >&2
