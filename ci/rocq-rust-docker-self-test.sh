#!/usr/bin/env sh
# Exercises the Docker boundary of rocq-rust-docker.sh without a Docker daemon.
set -eu

repo_root=$(
    CDPATH=
    export CDPATH
    cd -- "$(dirname -- "$0")/.." && pwd
)
runner=$repo_root/ci/rocq-rust-docker.sh
work=$(mktemp -d "${TMPDIR:-/tmp}/rocq-rust-docker-self-test.XXXXXX")
trap 'rm -rf "$work"' EXIT HUP INT TERM

fake_bin=$work/bin
log=$work/docker.log
state=$work/docker-state
mkdir -p "$fake_bin"

cat >"$fake_bin/docker" <<'FAKE_DOCKER'
#!/usr/bin/env sh
set -eu

log=${FAKE_DOCKER_LOG:?}
state=${FAKE_DOCKER_STATE:?}
docker_args=$*
printf '%s\n' "$*" >>"$log"

case " $* " in
    *' docker.sock '*|*'/docker.sock'*)
        echo "fake docker: Docker socket mounts are forbidden" >&2
        exit 97
        ;;
esac

require_run_option() {
    case " $docker_args " in
        *" $1 "*) ;;
        *)
            echo "fake docker: missing required run option $1" >&2
            exit 98
            ;;
    esac
}

require_network() {
    case " $docker_args " in
        *" --network $1 "*) ;;
        *)
            echo "fake docker: expected network $1" >&2
            exit 105
            ;;
    esac
}

if [ "$1" = volume ] && [ "$2" = create ]; then
    label=
    previous=
    for argument in "$@"; do
        if [ "$previous" = --label ]; then
            label=$argument
            break
        fi
        previous=$argument
    done
    printf '%s\n' "$label" >"$state.volume"
    exit 0
fi

if [ "$1" = volume ] && [ "$2" = inspect ]; then
    cut -d= -f2- "$state.volume"
    exit 0
fi

if [ "$1" = volume ] && [ "$2" = rm ]; then
    case "$3" in
        inference-rocq-rust-source-*) exit 0 ;;
        *)
            echo "fake docker: unexpected volume cleanup target $3" >&2
            exit 99
            ;;
    esac
fi

if [ "$1" = container ] && [ "$2" = create ]; then
    require_run_option --read-only
    require_run_option --network
    require_run_option --cap-drop
    require_run_option --security-opt
    require_run_option --tmpfs
    require_network none
    label=
    previous=
    for argument in "$@"; do
        if [ "$previous" = --label ]; then
            label=$argument
            break
        fi
        previous=$argument
    done
    printf '%s\n' "$label" >"$state"
    exit 0
fi

if [ "$1" = container ] && [ "$2" = start ]; then
    if [ "${FAKE_DOCKER_LOCK_START_FAIL:-0}" = 1 ]; then
        exit 44
    fi
    exit 0
fi

if [ "$1" = container ] && [ "$2" = inspect ]; then
    case " $* " in
        *'.State.Running'*) printf '%s\n' true ;;
        *) cut -d= -f2- "$state" ;;
    esac
    exit 0
fi

if [ "$1" = container ] && [ "$2" = rm ]; then
    [ "$4" = inference-cargo-target-rust-1.98-lock ]
    exit 0
fi

if [ "$1" = run ]; then
    require_run_option --read-only
    require_run_option --cap-drop
    require_run_option --security-opt
    require_run_option --tmpfs

    case " $* " in
        *' cargo fetch --locked --manifest-path /workspace/Cargo.toml'*)
            case " $* " in
                *'rustc --version'*'rustc 1.98.0 '*) ;;
                *)
                    echo "fake docker: fetch must assert rustc 1.98.0" >&2
                    exit 104
                    ;;
            esac
            require_network bridge
            ;;
        *'exec cargo "$@"'*'sh test --locked --offline --manifest-path /workspace/Cargo.toml'*)
            require_network none
            ;;
        *'ci/rocq-discharge.cargo-lock'*'/snapshot/Cargo.lock'*'cmp -s'*)
            require_network none
            if [ "${FAKE_DOCKER_LANE_LOCK_MISMATCH:-0}" = 1 ]; then
                exit 45
            fi
            ;;
        *)
            echo "fake docker: unrecognised run command: $*" >&2
            exit 102
            ;;
    esac
    exit 0
fi

echo "fake docker: unexpected invocation: $*" >&2
exit 103
FAKE_DOCKER
chmod +x "$fake_bin/docker"

assert_contains() {
    expected=$1
    if ! grep -F -- "$expected" "$log" >/dev/null; then
        echo "self-test: expected Docker invocation to contain: $expected" >&2
        exit 1
    fi
}

assert_absent() {
    unexpected=$1
    if grep -F -- "$unexpected" "$log" >/dev/null; then
        echo "self-test: unexpected Docker invocation contained: $unexpected" >&2
        exit 1
    fi
}

run_runner() {
    PATH="$fake_bin:$PATH" \
        FAKE_DOCKER_LOG=$log \
        FAKE_DOCKER_STATE=$state \
        "$runner" cargo test -p inference-tests \
        'rocq_typecheck::gate::generated_output_type_checks' -- --exact
}

: >"$log"
run_runner

assert_contains 'rust:1.98-bookworm@sha256:82150a52ec202c1b14d7817e14516c392bb7f5cfebd88f1ed531cb37ebd39922'
assert_contains 'busybox:1.37.0@sha256:9db7b59979c38555a39def84a31fb98b5296952f9e3afd4f6f11f05b07adfab0'
expected_toolchain='RUSTUP_TOOLCHAIN=1.98.0-$(uname -m)-unknown-linux-gnu'
assert_contains "$expected_toolchain"
assert_contains 'CARGO_HOME=/cargo-home'
assert_contains 'CARGO_TARGET_DIR=/cargo-target'
assert_contains 'ci/rocq-discharge.cargo-lock'
assert_contains '/snapshot/Cargo.lock'
assert_contains 'cmp -s'
assert_contains '--exclude=./Cargo.lock'
assert_contains '--exclude=Cargo.lock'
assert_contains 'inference-cargo-home-rust-1.98'
assert_contains 'inference-cargo-target-rust-1.98'
assert_contains 'inference-cargo-target-rust-1.98-lock'
assert_contains 'cargo fetch --locked --manifest-path /workspace/Cargo.toml'
assert_contains 'exec cargo "$@"'
assert_contains 'sh test --locked --offline --manifest-path /workspace/Cargo.toml'
assert_contains 'volume rm inference-rocq-rust-source-'
assert_contains 'container rm -f inference-cargo-target-rust-1.98-lock'
assert_absent 'volume rm inference-cargo-home-rust-1.98'
assert_absent 'volume rm inference-cargo-target-rust-1.98'

: >"$log"
if PATH="$fake_bin:$PATH" \
    FAKE_DOCKER_LOG=$log \
    FAKE_DOCKER_STATE=$state \
    FAKE_DOCKER_LANE_LOCK_MISMATCH=1 \
    "$runner" cargo test -p inference-tests \
    'rocq_typecheck::gate::generated_output_type_checks' -- --exact \
    >"$work/mismatch.stdout" 2>"$work/mismatch.stderr"; then
    echo 'self-test: lane-lock mismatch unexpectedly succeeded' >&2
    exit 1
fi
if ! grep -F 'snapshot lane lock mismatch' "$work/mismatch.stderr" >/dev/null; then
    echo 'self-test: lane-lock mismatch did not report the expected failure' >&2
    exit 1
fi

: >"$log"
if PATH="$fake_bin:$PATH" \
    FAKE_DOCKER_LOG=$log \
    FAKE_DOCKER_STATE=$state \
    FAKE_DOCKER_LOCK_START_FAIL=1 \
    "$runner" cargo test -p inference-tests \
    'rocq_typecheck::gate::generated_output_type_checks' -- --exact \
    >"$work/lock.stdout" 2>"$work/lock.stderr"; then
    echo 'self-test: failed target-lock start unexpectedly succeeded' >&2
    exit 1
fi
assert_contains 'container rm -f inference-cargo-target-rust-1.98-lock'

echo 'rocq-rust-docker self-test: PASS'
