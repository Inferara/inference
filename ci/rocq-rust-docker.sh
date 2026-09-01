#!/usr/bin/env sh
# Run one locked Cargo command in the isolated Rust 1.98 Rocq discharge lane.
set -eu

rust_image='rust:1.98-bookworm@sha256:82150a52ec202c1b14d7817e14516c392bb7f5cfebd88f1ed531cb37ebd39922'
busybox_image='busybox:1.37.0@sha256:9db7b59979c38555a39def84a31fb98b5296952f9e3afd4f6f11f05b07adfab0'
registry_volume='inference-cargo-home-rust-1.98'
target_volume='inference-cargo-target-rust-1.98'
target_lock_container='inference-cargo-target-rust-1.98-lock'
owner_label='org.inferara.rocq-discharge.owner'

usage() {
    echo "usage: $0 cargo <subcommand> [arguments...]" >&2
    exit 2
}

if [ "$#" -lt 2 ] || [ "$1" != cargo ]; then
    usage
fi
shift
cargo_subcommand=$1
shift
case "$cargo_subcommand" in
    +*)
        echo "$0: Cargo toolchain override is not allowed" >&2
        exit 2
        ;;
esac

locked_count=0
offline_count=0
after_separator=0
for cargo_argument in "$@"; do
    if [ "$cargo_argument" = -- ]; then
        after_separator=1
    elif [ "$after_separator" -eq 0 ]; then
        case "$cargo_argument" in
            --locked) locked_count=$((locked_count + 1)) ;;
            --offline) offline_count=$((offline_count + 1)) ;;
        esac
    fi
done
if [ "$locked_count" -gt 1 ] || [ "$offline_count" -gt 1 ]; then
    echo "$0: duplicate Cargo lock or offline option" >&2
    exit 2
fi
if [ "$locked_count" -eq 0 ]; then
    set -- --locked "$@"
fi
if [ "$offline_count" -eq 0 ]; then
    set -- --offline "$@"
fi

repo_root=$(
    CDPATH=
    export CDPATH
    cd -- "$(dirname -- "$0")/.." && pwd
)
lane_lock=$repo_root/ci/rocq-discharge.cargo-lock
if [ ! -f "$lane_lock" ]; then
    echo "$0: missing tracked lane lock: $lane_lock" >&2
    exit 2
fi

newline='
'
carriage=$(printf '\r')
case "$repo_root" in
    *','*|*"$newline"*|*"$carriage"*)
        echo "$0: unsafe Docker mount path" >&2
        exit 2
        ;;
esac

docker_bin=${DOCKER:-docker}
run_id=$$
source_volume=
source_created=0
target_lock_created=0

valid_source_volume() {
    [ -n "$source_volume" ] || return 1
    owner=$("$docker_bin" volume inspect --format "{{ index .Labels \"$owner_label\" }}" "$source_volume" 2>/dev/null || true)
    [ "$owner" = "$run_id" ]
}

valid_target_lock() {
    owner=$("$docker_bin" container inspect --format "{{ index .Config.Labels \"$owner_label\" }}" "$target_lock_container" 2>/dev/null || true)
    [ "$owner" = "$run_id" ]
}

cleanup() {
    status=$?
    trap - EXIT HUP INT TERM

    if [ "$target_lock_created" -eq 1 ] && valid_target_lock; then
        "$docker_bin" container rm -f "$target_lock_container" >/dev/null || true
    fi
    if [ "$source_created" -eq 1 ] && valid_source_volume; then
        "$docker_bin" volume rm "$source_volume" >/dev/null || true
    fi
    exit "$status"
}
trap cleanup EXIT HUP INT TERM

"$docker_bin" volume create "$registry_volume" >/dev/null
"$docker_bin" volume create "$target_volume" >/dev/null
source_volume=$("$docker_bin" volume create --label "$owner_label=$run_id")
source_created=1
if ! valid_source_volume; then
    echo "$0: source volume owner label did not validate" >&2
    exit 1
fi

source_state=0
"$docker_bin" run --rm \
    --read-only \
    --network none \
    --cap-drop ALL \
    --security-opt no-new-privileges \
    --tmpfs /tmp \
    --mount "type=volume,src=$source_volume,dst=/snapshot" \
    "$busybox_image" sh -c '
        set -eu
        if find /snapshot -mindepth 1 -maxdepth 1 -print -quit | grep . >/dev/null; then
            exit 46
        fi
    ' || source_state=$?
if [ "$source_state" -ne 0 ]; then
    echo "$0: source volume is not empty" >&2
    exit "$source_state"
fi

if ! "$docker_bin" container create --name "$target_lock_container" \
    --label "$owner_label=$run_id" \
    --read-only \
    --network none \
    --cap-drop ALL \
    --security-opt no-new-privileges \
    --tmpfs /tmp \
    "$busybox_image" sh -c 'trap "exit 0" TERM INT; sleep 3600' >/dev/null; then
    echo "$0: target volume is already locked by another lane" >&2
    exit 1
fi
target_lock_created=1
"$docker_bin" container start "$target_lock_container" >/dev/null
if [ "$("$docker_bin" container inspect --format '{{.State.Running}}' "$target_lock_container")" != true ] || ! valid_target_lock; then
    echo "$0: target lock container did not validate" >&2
    exit 1
fi

snapshot_status=0
"$docker_bin" run --rm \
    --read-only \
    --network none \
    --cap-drop ALL \
    --security-opt no-new-privileges \
    --tmpfs /tmp \
    --mount "type=bind,src=$repo_root,dst=/checkout,readonly" \
    --mount "type=volume,src=$source_volume,dst=/snapshot" \
    "$busybox_image" sh -c '
        set -eu
        cd /checkout
        tar --exclude=.git --exclude=target --exclude=Cargo.lock --exclude=./Cargo.lock -cf /tmp/snapshot.tar .
        tar -C /snapshot -xf /tmp/snapshot.tar
        cp /checkout/ci/rocq-discharge.cargo-lock /snapshot/Cargo.lock
        if ! cmp -s /checkout/ci/rocq-discharge.cargo-lock /snapshot/Cargo.lock; then
            exit 45
        fi
        printf "rocq-rust-docker: lane-lock-sha256=%s\\n" "$(sha256sum /checkout/ci/rocq-discharge.cargo-lock | cut -d " " -f1)"
        printf "rocq-rust-docker: snapshot-lock-sha256=%s\\n" "$(sha256sum /snapshot/Cargo.lock | cut -d " " -f1)"
    ' || snapshot_status=$?
if [ "$snapshot_status" -ne 0 ]; then
    if [ "$snapshot_status" -eq 45 ]; then
        echo "$0: snapshot lane lock mismatch" >&2
    else
        echo "$0: failed to create the source snapshot" >&2
    fi
    exit "$snapshot_status"
fi

"$docker_bin" run --rm \
    --read-only \
    --network bridge \
    --cap-drop ALL \
    --security-opt no-new-privileges \
    --tmpfs /tmp \
    --mount "type=volume,src=$source_volume,dst=/workspace,readonly" \
    --mount "type=volume,src=$registry_volume,dst=/cargo-home" \
    --mount "type=volume,src=$target_volume,dst=/cargo-target" \
    -e CARGO_HOME=/cargo-home \
    -e CARGO_TARGET_DIR=/cargo-target \
    "$rust_image" sh -c '
        set -eu
        export RUSTUP_TOOLCHAIN=1.98.0-$(uname -m)-unknown-linux-gnu
        rust_version=$(rustc --version)
        case "$rust_version" in
            "rustc 1.98.0 "*) ;;
            *)
                echo "rocq-rust-docker: expected rustc 1.98.0, got $rust_version" >&2
                exit 1
                ;;
        esac
        cargo_path=$(rustup which cargo)
        exec "$cargo_path" fetch --locked --manifest-path /workspace/Cargo.toml
    '

"$docker_bin" run --rm \
    --read-only \
    --network none \
    --cap-drop ALL \
    --security-opt no-new-privileges \
    --tmpfs /tmp \
    --mount "type=volume,src=$source_volume,dst=/workspace,readonly" \
    --mount "type=volume,src=$registry_volume,dst=/cargo-home" \
    --mount "type=volume,src=$target_volume,dst=/cargo-target" \
    -e CARGO_HOME=/cargo-home \
    -e CARGO_TARGET_DIR=/cargo-target \
    "$rust_image" sh -c '
        set -eu
        export RUSTUP_TOOLCHAIN=1.98.0-$(uname -m)-unknown-linux-gnu
        cargo_path=$(rustup which cargo)
        exec "$cargo_path" "$@"
    ' sh "$cargo_subcommand" --manifest-path /workspace/Cargo.toml "$@"
