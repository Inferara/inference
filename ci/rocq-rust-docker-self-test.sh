#!/usr/bin/env sh
# Exercises rocq-rust-docker.sh against a stateful, argv-aware Docker fake.
set -eu

repo_root=$(
    CDPATH=
    export CDPATH
    cd -- "$(dirname -- "$0")/.." && pwd
)
runner_source=$repo_root/ci/rocq-rust-docker.sh
work=$(mktemp -d "${TMPDIR:-/tmp}/rocq-rust-docker-self-test.XXXXXX")
trap 'rm -rf "$work"' EXIT HUP INT TERM

fixture=$work/repo
state=$work/state
fake_bin=$work/'fake docker bin'
fake_docker=$fake_bin/'docker tool'
mkdir -p "$fixture/ci" "$fixture/.git" "$fixture/target" "$state/volumes" "$state/calls" "$fake_bin"
cp "$runner_source" "$fixture/ci/rocq-rust-docker.sh"
chmod +x "$fixture/ci/rocq-rust-docker.sh"
printf 'lane lock\n' >"$fixture/ci/rocq-discharge.cargo-lock"
printf 'ignored host lock\n' >"$fixture/Cargo.lock"
printf 'source payload\n' >"$fixture/source.txt"
printf 'excluded git\n' >"$fixture/.git/HEAD"
printf 'excluded target\n' >"$fixture/target/artifact"

cat >"$fake_docker" <<'FAKE_DOCKER'
#!/usr/bin/env sh
set -eu
state=${FAKE_DOCKER_STATE:?}
fixture=${FAKE_DOCKER_FIXTURE:?}
counter=$(cat "$state/counter" 2>/dev/null || echo 0)
counter=$((counter + 1)); printf '%s\n' "$counter" >"$state/counter"
call=$state/calls/$counter; mkdir "$call"
for argument in "$@"; do printf '%s\n' "$argument" >>"$call/argv"; done
last_arg() { for value in "$@"; do :; done; printf '%s\n' "$value"; }
arg_after() { wanted=$1; shift; previous=; for value in "$@"; do if [ "$previous" = "$wanted" ]; then printf '%s\n' "$value"; return 0; fi; previous=$value; done; return 1; }
mount_volume() { destination=$1; shift; previous=; for value in "$@"; do if [ "$previous" = --mount ]; then case "$value" in *"dst=$destination"*) source=${value#*src=}; source=${source%%,*}; printf '%s\n' "$source"; return 0;; esac; fi; previous=$value; done; return 1; }
volume_dir() { printf '%s/volumes/%s\n' "$state" "$1"; }
label_of() { cat "$(volume_dir "$1")/.label" 2>/dev/null || true; }
has_arg() { wanted=$1; shift; for value in "$@"; do [ "$value" = "$wanted" ] && return 0; done; return 1; }
has_pair() { wanted=$1; expected=$2; shift 2; previous=; for value in "$@"; do [ "$previous" = "$wanted" ] && [ "$value" = "$expected" ] && return 0; previous=$value; done; return 1; }
require_arg() { has_arg "$1" "$@" || { echo "fake docker: missing argv $1" >&2; exit 73; }; }
require_pair() { has_pair "$1" "$2" "$@" || { echo "fake docker: missing argv pair $1 $2" >&2; exit 74; }; }
require_hardened() {
    require_arg --read-only "$@"; require_pair --cap-drop ALL "$@"; require_pair --security-opt no-new-privileges "$@"; require_pair --tmpfs /tmp "$@"
    for value in "$@"; do case "$value" in *docker.sock*) echo 'fake docker: Docker socket mount is forbidden' >&2; exit 75;; esac; done
}

if [ "$1" = volume ] && [ "$2" = create ]; then
    label=$(arg_after --label "$@" || true); name=$(last_arg "$@")
    if [ -n "$label" ]; then
        if [ "$name" != "$label" ]; then echo 'fake docker: source volume must be Docker-generated' >&2; exit 70; fi
        name=generated-source-$counter
        printf '%s\n' "$name" >"$state/source-name"
    fi
    directory=$(volume_dir "$name"); mkdir -p "$directory"; printf '%s\n' "${label#*=}" >"$directory/.label"
    case "${FAKE_DOCKER_SOURCE_MODE:-clean}" in foreign) printf '%s\n' foreign-owner >"$directory/.label";; stale) printf stale >"$directory/stale-file";; esac
    printf '%s\n' "$name"; exit 0
fi
if [ "$1" = volume ] && [ "$2" = inspect ]; then label_of "$(last_arg "$@")"; exit 0; fi
if [ "$1" = volume ] && [ "$2" = rm ]; then name=$3; cp -R "$(volume_dir "$name")" "$state/removed-$name" 2>/dev/null || true; rm -rf "$(volume_dir "$name")"; exit 0; fi
if [ "$1" = container ] && [ "$2" = create ]; then label=$(arg_after --label "$@" || true); printf '%s\n' "${label#*=}" >"$state/lock-label"; exit 0; fi
if [ "$1" = container ] && [ "$2" = start ]; then printf true; exit 0; fi
if [ "$1" = container ] && [ "$2" = inspect ]; then case "$(arg_after --format "$@" || true)" in *State.Running*) printf true;; *) cat "$state/lock-label";; esac; exit 0; fi
if [ "$1" = container ] && [ "$2" = rm ]; then exit 0; fi
if [ "$1" = run ]; then
    script=$(arg_after -c "$@" || true); snapshot=$(mount_volume /snapshot "$@" || true)
    if [ -n "$snapshot" ] && case "$script" in *'find /snapshot'*) true;; *) false;; esac; then
        directory=$(volume_dir "$snapshot"); if find "$directory" -mindepth 1 -maxdepth 1 ! -name .label -print -quit | grep . >/dev/null; then exit 46; fi; exit 0
    fi
    if [ -n "$snapshot" ] && case "$script" in *'Cargo.lock'*) true;; *) false;; esac; then
        case "$script" in *'| tar'*) echo 'fake docker: snapshot must not use a tar pipeline' >&2; exit 71;; esac
        directory=$(volume_dir "$snapshot")
        for entry in "$fixture"/* "$fixture"/.[!.]*; do [ -e "$entry" ] || continue; base=$(basename "$entry"); case "$base" in .git|target|Cargo.lock) continue;; esac; cp -R "$entry" "$directory/"; done
        cp "$fixture/ci/rocq-discharge.cargo-lock" "$directory/Cargo.lock"
        if [ "${FAKE_DOCKER_CORRUPT_LANE_LOCK:-0}" = 1 ]; then printf corrupt >"$directory/Cargo.lock"; fi
        cmp -s "$fixture/ci/rocq-discharge.cargo-lock" "$directory/Cargo.lock" || exit 45
        rm -rf "$state/observed-snapshot"; cp -R "$directory" "$state/observed-snapshot"; exit 0
    fi
    case "$script" in
        *'fetch --locked --manifest-path /workspace/Cargo.toml'*)
            require_arg 'rust:1.98-bookworm@sha256:82150a52ec202c1b14d7817e14516c392bb7f5cfebd88f1ed531cb37ebd39922' "$@"
            require_pair --network bridge "$@"; require_hardened "$@"; require_pair -e CARGO_HOME=/cargo-home "$@"; require_pair -e CARGO_TARGET_DIR=/cargo-target "$@"
            case "$script" in *'RUSTUP_TOOLCHAIN=1.98.0-$(uname -m)-unknown-linux-gnu'*'cargo_path=$(rustup which cargo)'*'exec "$cargo_path" fetch'*) ;; *) echo 'fake docker: fetch must execute rustup-resolved Cargo' >&2; exit 76;; esac
            touch "$state/fetch-contract"
            ;;
        *'exec "$cargo_path" "$@"'*)
            require_arg 'rust:1.98-bookworm@sha256:82150a52ec202c1b14d7817e14516c392bb7f5cfebd88f1ed531cb37ebd39922' "$@"
            require_pair --network none "$@"; require_hardened "$@"; require_pair -e CARGO_HOME=/cargo-home "$@"; require_pair -e CARGO_TARGET_DIR=/cargo-target "$@"
            require_arg test "$@"; require_arg --locked "$@"; require_arg --offline "$@"; require_pair --manifest-path /workspace/Cargo.toml "$@"
            case "$script" in *'RUSTUP_TOOLCHAIN=1.98.0-$(uname -m)-unknown-linux-gnu'*'cargo_path=$(rustup which cargo)'*'exec "$cargo_path" "$@"'*) ;; *) echo 'fake docker: execution must use rustup-resolved Cargo' >&2; exit 77;; esac
            touch "$state/offline-contract"
            ;;
    esac
    exit 0
fi
echo "fake docker: unexpected command $1 $2" >&2; exit 72
FAKE_DOCKER
chmod +x "$fake_docker"
volume_dir() { printf '%s/volumes/%s\n' "$state" "$1"; }

run_runner() {
    mode=${1:-clean}; shift || true
    : >"$state/counter"; rm -rf "$state/calls" "$state/observed-snapshot" "$state/volumes"; mkdir -p "$state/calls" "$state/volumes"
    FAKE_DOCKER_STATE=$state FAKE_DOCKER_FIXTURE=$fixture FAKE_DOCKER_SOURCE_MODE=$mode DOCKER="$fake_docker" "$fixture/ci/rocq-rust-docker.sh" "$@"
}
expect_failure() { label=$1; shift; if "$@" >"$work/$label.out" 2>"$work/$label.err"; then echo "self-test: $label unexpectedly succeeded" >&2; exit 1; fi; }

run_runner clean cargo test -p inference-tests rocq_typecheck:: -- --exact
test -f "$state/observed-snapshot/source.txt"
test ! -e "$state/observed-snapshot/.git"
test ! -e "$state/observed-snapshot/target"
cmp -s "$fixture/ci/rocq-discharge.cargo-lock" "$state/observed-snapshot/Cargo.lock"
[ -f "$state/fetch-contract" ] || { echo 'self-test: fetch container contract was not verified' >&2; exit 1; }
[ -f "$state/offline-contract" ] || { echo 'self-test: offline container contract was not verified' >&2; exit 1; }
source_name=$(cat "$state/source-name")
test ! -d "$(volume_dir "$source_name")"
test -d "$(volume_dir inference-cargo-home-rust-1.98)"
test -d "$(volume_dir inference-cargo-target-rust-1.98)"

expect_failure corrupt env FAKE_DOCKER_CORRUPT_LANE_LOCK=1 FAKE_DOCKER_STATE="$state" FAKE_DOCKER_FIXTURE="$fixture" DOCKER="$fake_docker" "$fixture/ci/rocq-rust-docker.sh" cargo test
grep -F 'snapshot lane lock mismatch' "$work/corrupt.err" >/dev/null
expect_failure foreign run_runner foreign cargo test
grep -F 'source volume owner label did not validate' "$work/foreign.err" >/dev/null
expect_failure stale run_runner stale cargo test
grep -F 'source volume is not empty' "$work/stale.err" >/dev/null
expect_failure toolchain run_runner clean cargo +nightly test
grep -F 'Cargo toolchain override is not allowed' "$work/toolchain.err" >/dev/null

unsafe=$work/'repo,unsafe'; mkdir -p "$unsafe/ci"; cp "$runner_source" "$unsafe/ci/rocq-rust-docker.sh"; cp "$fixture/ci/rocq-discharge.cargo-lock" "$unsafe/ci/"
expect_failure unsafe env FAKE_DOCKER_STATE="$state" FAKE_DOCKER_FIXTURE="$fixture" DOCKER="$fake_docker" "$unsafe/ci/rocq-rust-docker.sh" cargo test
grep -F 'unsafe Docker mount path' "$work/unsafe.err" >/dev/null
test ! -e "$state/calls/1"
echo 'rocq-rust-docker self-test: PASS'
