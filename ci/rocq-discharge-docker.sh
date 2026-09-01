#!/usr/bin/env sh
# Orchestrate the local emitted-Rocq discharge gate without host compilation.
set -eu

rust_image='rust:1.98-bookworm@sha256:82150a52ec202c1b14d7817e14516c392bb7f5cfebd88f1ed531cb37ebd39922'
busybox_image='busybox:1.37.0@sha256:9db7b59979c38555a39def84a31fb98b5296952f9e3afd4f6f11f05b07adfab0'
registry_volume='inference-cargo-home-rust-1.98'
target_volume='inference-cargo-target-rust-1.98'
target_lock_container='inference-cargo-target-rust-1.98-lock'
owner_label='org.inferara.rocq-discharge.owner'
full_result_lines=5
full_passed_floor=3075
task0_lock_script='trap "exit 0" TERM INT; sleep 3600'
task0_source_empty_script='
        set -eu
        if find /snapshot -mindepth 1 -maxdepth 1 -print -quit | grep . >/dev/null; then
            exit 46
        fi
    '
task0_snapshot_script='
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
    '
task0_fetch_script='
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
task0_cargo_script='
        set -eu
        export RUSTUP_TOOLCHAIN=1.98.0-$(uname -m)-unknown-linux-gnu
        cargo_path=$(rustup which cargo)
        exec "$cargo_path" "$@"
    '

proxy_die() {
    echo 'rocq-discharge-docker: private Docker proxy rejected an unknown Task-0 call shape' >&2
    exit 2
}

digits_only() {
    case "$1" in ''|*[!0-9]*) return 1;; esac
}

valid_run_token() {
    [ "${#1}" -eq 6 ] || return 1
    case "$1" in *[!A-Za-z0-9]*) return 1;; esac
}

canonical_executable() {
    executable=$1
    case "$executable" in /*) :;; *) return 1;; esac
    link_count=0
    while [ -L "$executable" ]; do
        link_count=$((link_count + 1))
        [ "$link_count" -le 16 ] || return 1
        link=$(readlink "$executable") || return 1
        case "$link" in
            /*) executable=$link ;;
            *) executable=$(dirname -- "$executable")/$link ;;
        esac
    done
    executable_dir=$(CDPATH= cd -- "$(dirname -- "$executable")" && pwd -P) || return 1
    executable=$executable_dir/$(basename -- "$executable")
    [ -f "$executable" ] && [ -x "$executable" ] || return 1
    printf '%s\n' "$executable"
}

proxy_read_source_owner() {
    [ -f "$owner_file" ] && [ ! -L "$owner_file" ] || return 1
    [ "$(stat -c '%a' "$owner_file" 2>/dev/null || stat -f '%Lp' "$owner_file" 2>/dev/null)" = 600 ] || return 1
    [ "$(wc -l <"$owner_file" | tr -d ' ')" -eq 1 ] || return 1
    source_owner=$(sed -n '1p' "$owner_file")
    digits_only "$source_owner" || return 1
    printf '%s\n' "$source_owner"
}

docker_proxy() {
    real_docker=${INFERENCE_ROCQ_DOCKER_PROXY_REAL_DOCKER:?}
    source_volume=${INFERENCE_ROCQ_DOCKER_PROXY_SOURCE_VOLUME:?}
    exchange_volume=${INFERENCE_ROCQ_DOCKER_PROXY_EXCHANGE_VOLUME:?}
    run_owner=${INFERENCE_ROCQ_DOCKER_PROXY_RUN_OWNER:?}
    owner_file=${INFERENCE_ROCQ_DOCKER_PROXY_SOURCE_OWNER_FILE:?}

    canonical_real=$(canonical_executable "$real_docker") || proxy_die
    [ "$canonical_real" = "$real_docker" ] || proxy_die
    canonical_self=$(canonical_executable "$0") || proxy_die
    [ "$canonical_real" != "$canonical_self" ] || proxy_die
    [ ! "$canonical_real" -ef "$canonical_self" ] || proxy_die
    case "$owner_file" in /*) :;; *) proxy_die;; esac
    owner_parent=$(CDPATH= cd -- "$(dirname -- "$owner_file")" && pwd -P) || proxy_die
    [ "$owner_file" = "$owner_parent/$(basename -- "$owner_file")" ] || proxy_die
    [ -d "$owner_parent" ] && [ ! -L "$owner_parent" ] || proxy_die
    [ "$(stat -c '%a' "$owner_parent" 2>/dev/null || stat -f '%Lp' "$owner_parent" 2>/dev/null)" = 700 ] || proxy_die

    case "$source_volume" in inference-rocq-discharge-*-source) :;; *) proxy_die;; esac
    token=${source_volume#inference-rocq-discharge-}
    token=${token%-source}
    valid_run_token "$token" || proxy_die
    [ "$exchange_volume" = "inference-rocq-discharge-$token-exchange" ] || proxy_die
    [ "$run_owner" = "task4-$token" ] || proxy_die
    for argument in "$@"; do case "$argument" in *docker.sock*) proxy_die;; esac; done

    if [ "$#" -eq 3 ] && [ "$1" = volume ] && [ "$2" = create ]; then
        case "$3" in "$registry_volume"|"$target_volume") exec "$real_docker" "$@";; *) proxy_die;; esac
    fi
    if [ "$#" -eq 4 ] && [ "$1" = volume ] && [ "$2" = create ] && [ "$3" = --label ]; then
        case "$4" in "$owner_label="*) requested_source_owner=${4#*=};; *) proxy_die;; esac
        digits_only "$requested_source_owner" || proxy_die
        if [ -e "$owner_file" ] || [ -L "$owner_file" ]; then
            proxy_read_source_owner >/dev/null || proxy_die
        fi
        if "$real_docker" volume inspect "$source_volume" >/dev/null 2>&1; then
            echo 'rocq-discharge-docker: unique source volume already exists' >&2
            exit 1
        fi
        (umask 077; printf '%s\n' "$requested_source_owner" >"$owner_file")
        exec "$real_docker" volume create --label "$4" "$source_volume"
    fi
    if [ "$#" -eq 5 ] && [ "$1" = volume ] && [ "$2" = inspect ] && [ "$3" = --format ] && \
       [ "$4" = "{{ index .Labels \"$owner_label\" }}" ] && [ "$5" = "$source_volume" ]; then
        proxy_read_source_owner >/dev/null || proxy_die
        exec "$real_docker" "$@"
    fi
    if [ "$#" -eq 3 ] && [ "$1" = volume ] && [ "$2" = rm ] && [ "$3" = "$source_volume" ]; then
        proxy_read_source_owner >/dev/null || proxy_die
        exec "$real_docker" "$@"
    fi

    if [ "$#" -eq 19 ] && [ "$1" = container ] && [ "$2" = create ] && \
       [ "$3" = --name ] && [ "$4" = "$target_lock_container" ] && [ "$5" = --label ] && \
       [ "$7" = --read-only ] && [ "$8" = --network ] && [ "$9" = none ] && \
       [ "${10}" = --cap-drop ] && [ "${11}" = ALL ] && [ "${12}" = --security-opt ] && \
       [ "${13}" = no-new-privileges ] && [ "${14}" = --tmpfs ] && [ "${15}" = /tmp ] && \
       [ "${16}" = "$busybox_image" ] && [ "${17}" = sh ] && [ "${18}" = -c ]; then
        [ "${19}" = "$task0_lock_script" ] || proxy_die
        source_owner=$(proxy_read_source_owner) || proxy_die
        [ "$6" = "$owner_label=$source_owner" ] || proxy_die
        exec "$real_docker" "$@"
    fi
    if [ "$#" -eq 3 ] && [ "$1" = container ] && [ "$2" = start ] && [ "$3" = "$target_lock_container" ]; then
        exec "$real_docker" "$@"
    fi
    if [ "$#" -eq 5 ] && [ "$1" = container ] && [ "$2" = inspect ] && [ "$3" = --format ] && \
       [ "$5" = "$target_lock_container" ]; then
        case "$4" in
            '{{.State.Running}}') : ;;
            "{{ index .Config.Labels \"$owner_label\" }}") : ;;
            *) proxy_die ;;
        esac
        exec "$real_docker" "$@"
    fi
    if [ "$#" -eq 4 ] && [ "$1" = container ] && [ "$2" = rm ] && [ "$3" = -f ] && [ "$4" = "$target_lock_container" ]; then
        exec "$real_docker" "$@"
    fi

    proxy_repo_root=$(CDPATH= cd -- "$(dirname -- "$0")/.." && pwd -P) || proxy_die
    source_mount="type=volume,src=$source_volume,dst=/snapshot"
    if [ "$#" -eq 17 ] && [ "$1" = run ] && [ "$2" = --rm ] && [ "$3" = --read-only ] && \
       [ "$4" = --network ] && [ "$5" = none ] && [ "$6" = --cap-drop ] && [ "$7" = ALL ] && \
       [ "$8" = --security-opt ] && [ "$9" = no-new-privileges ] && [ "${10}" = --tmpfs ] && \
       [ "${11}" = /tmp ] && [ "${12}" = --mount ] && [ "${13}" = "$source_mount" ] && \
       [ "${14}" = "$busybox_image" ] && [ "${15}" = sh ] && [ "${16}" = -c ] && \
       [ "${17}" = "$task0_source_empty_script" ]; then
        exec "$real_docker" "$@"
    fi
    checkout_mount="type=bind,src=$proxy_repo_root,dst=/checkout,readonly"
    if [ "$#" -eq 19 ] && [ "$1" = run ] && [ "$2" = --rm ] && [ "$3" = --read-only ] && \
       [ "$4" = --network ] && [ "$5" = none ] && [ "$6" = --cap-drop ] && [ "$7" = ALL ] && \
       [ "$8" = --security-opt ] && [ "$9" = no-new-privileges ] && [ "${10}" = --tmpfs ] && \
       [ "${11}" = /tmp ] && [ "${12}" = --mount ] && [ "${13}" = "$checkout_mount" ] && \
       [ "${14}" = --mount ] && [ "${15}" = "$source_mount" ] && [ "${16}" = "$busybox_image" ] && \
       [ "${17}" = sh ] && [ "${18}" = -c ] && [ "${19}" = "$task0_snapshot_script" ]; then
        exec "$real_docker" "$@"
    fi

    workspace_mount="type=volume,src=$source_volume,dst=/workspace,readonly"
    registry_mount="type=volume,src=$registry_volume,dst=/cargo-home"
    target_mount="type=volume,src=$target_volume,dst=/cargo-target"
    if [ "$#" -eq 25 ] && [ "$1" = run ] && [ "$2" = --rm ] && [ "$3" = --read-only ] && \
       [ "$4" = --network ] && [ "$5" = bridge ] && [ "$6" = --cap-drop ] && [ "$7" = ALL ] && \
       [ "$8" = --security-opt ] && [ "$9" = no-new-privileges ] && [ "${10}" = --tmpfs ] && \
       [ "${11}" = /tmp ] && [ "${12}" = --mount ] && [ "${13}" = "$workspace_mount" ] && \
       [ "${14}" = --mount ] && [ "${15}" = "$registry_mount" ] && [ "${16}" = --mount ] && \
       [ "${17}" = "$target_mount" ] && [ "${18}" = -e ] && [ "${19}" = CARGO_HOME=/cargo-home ] && \
       [ "${20}" = -e ] && [ "${21}" = CARGO_TARGET_DIR=/cargo-target ] && [ "${22}" = "$rust_image" ] && \
       [ "${23}" = sh ] && [ "${24}" = -c ] && [ "${25}" = "$task0_fetch_script" ]; then
        exec "$real_docker" "$@"
    fi

    [ "$#" -ge 25 ] || proxy_die
    [ "$1" = run ] && [ "$2" = --rm ] && [ "$3" = --read-only ] && \
    [ "$4" = --network ] && [ "$5" = none ] && [ "$6" = --cap-drop ] && [ "$7" = ALL ] && \
    [ "$8" = --security-opt ] && [ "$9" = no-new-privileges ] && [ "${10}" = --tmpfs ] && \
    [ "${11}" = /tmp ] && [ "${12}" = --mount ] && [ "${13}" = "$workspace_mount" ] && \
    [ "${14}" = --mount ] && [ "${15}" = "$registry_mount" ] && [ "${16}" = --mount ] && \
    [ "${17}" = "$target_mount" ] && [ "${18}" = -e ] && [ "${19}" = CARGO_HOME=/cargo-home ] && \
    [ "${20}" = -e ] && [ "${21}" = CARGO_TARGET_DIR=/cargo-target ] && [ "${22}" = "$rust_image" ] && \
    [ "${23}" = sh ] && [ "${24}" = -c ] && [ "${25}" = "$task0_cargo_script" ] || proxy_die

    if [ "$#" -eq 38 ] && [ "${26}" = sh ] && [ "${27}" = test ] && \
       [ "${28}" = --manifest-path ] && [ "${29}" = /workspace/Cargo.toml ] && \
       [ "${30}" = --offline ] && [ "${31}" = --locked ] && [ "${32}" = --color ] && \
       [ "${33}" = never ] && [ "${34}" = -p ] && [ "${35}" = inference-tests ] && \
       [ "${36}" = rocq_dischargeability:: ] && [ "${37}" = -- ] && [ "${38}" = --test-threads=1 ]; then
        exec "$real_docker" "$@"
    fi
    if [ "$#" -eq 37 ] && [ "${26}" = sh ] && [ "${27}" = test ] && \
       [ "${28}" = --manifest-path ] && [ "${29}" = /workspace/Cargo.toml ] && \
       [ "${30}" = --offline ] && [ "${31}" = --locked ] && [ "${32}" = --color ] && \
       [ "${33}" = never ] && [ "${34}" = -p ] && [ "${35}" = inference-tests ] && \
       [ "${36}" = -- ] && [ "${37}" = --test-threads=1 ]; then
        exec "$real_docker" "$@"
    fi
    if [ "$#" -eq 39 ] && [ "${26}" = sh ] && [ "${27}" = run ] && \
       [ "${28}" = --manifest-path ] && [ "${29}" = /workspace/Cargo.toml ] && \
       [ "${30}" = --offline ] && [ "${31}" = --locked ] && [ "${32}" = -p ] && \
       [ "${33}" = inference-tests ] && [ "${34}" = --bin ] && [ "${35}" = rocq-discharge ] && \
       [ "${36}" = -- ] && { [ "${37}" = export ] || [ "${37}" = verify ]; } && \
       [ "${38}" = --exchange ] && [ "${39}" = /exchange ]; then
        observed_owner=$("$real_docker" volume inspect --format "{{ index .Labels \"$owner_label\" }}" "$exchange_volume" 2>/dev/null || true)
        [ "$observed_owner" = "$run_owner" ] || { echo 'rocq-discharge-docker: exchange volume owner label changed' >&2; exit 1; }
        exchange_mount="type=volume,src=$exchange_volume,dst=/exchange"
        if [ "${37}" = verify ]; then exchange_mount=$exchange_mount,readonly; fi
        exec "$real_docker" run --rm --read-only --network none --cap-drop ALL \
            --security-opt no-new-privileges --tmpfs /tmp \
            --mount "$workspace_mount" --mount "$registry_mount" --mount "$target_mount" \
            --mount "$exchange_mount" \
            -e CARGO_HOME=/cargo-home -e CARGO_TARGET_DIR=/cargo-target \
            "$rust_image" sh -c "${25}" sh run --manifest-path /workspace/Cargo.toml \
            --offline --locked -p inference-tests --bin rocq-discharge -- "${37}" --exchange /exchange
    fi
    proxy_die
}

if [ "${INFERENCE_ROCQ_DOCKER_PROXY_MODE:-}" = 1 ]; then
    docker_proxy "$@"
fi

usage() {
    echo "usage: $0 --wasm-verifier <absolute-clean-checkout> --container <name> [--adapter batch|single|both] [--full]" >&2
    exit 2
}

fail() {
    phase=$1
    message=$2
    echo "rocq-discharge-docker: phase=$phase $message" >&2
    exit 1
}

safe_text() {
    value=$1
    [ -n "$value" ] || return 1
    [ "${#value}" -le 240 ] || return 1
    newline='
'
    carriage=$(printf '\r')
    tab=$(printf '\t')
    case "$value" in *','*|*"$newline"*|*"$carriage"*|*"$tab"*) return 1;; esac
    printf '%s' "$value" | LC_ALL=C grep '[[:cntrl:]]' >/dev/null 2>&1 && return 1
    return 0
}

safe_absolute_path() {
    value=$1
    safe_text "$value" || return 1
    case "$value" in /*) :;; *) return 1;; esac
    case "$value" in */../*|*/..|/..|*/./*|*/.) return 1;; esac
    return 0
}

safe_container_name() {
    safe_text "$1" || return 1
    case "$1" in [A-Za-z0-9]* ) :;; *) return 1;; esac
    case "$1" in *[!A-Za-z0-9_.-]*) return 1;; esac
}

path_mode() {
    stat -c '%a' "$1" 2>/dev/null || stat -f '%Lp' "$1" 2>/dev/null
}

path_identity() {
    stat -c '%u:%d:%i:%a' "$1" 2>/dev/null || stat -f '%u:%d:%i:%Lp' "$1" 2>/dev/null
}

identity_owner() {
    printf '%s\n' "${1%%:*}"
}

validate_owned_directory() {
    directory=$1
    expected_identity=$2
    [ -d "$directory" ] && [ ! -L "$directory" ] || return 1
    [ "$(CDPATH= cd -- "$directory" && pwd -P)" = "$directory" ] || return 1
    actual_identity=$(path_identity "$directory") || return 1
    [ "$actual_identity" = "$expected_identity" ] || return 1
    [ "$(identity_owner "$actual_identity")" = "$current_uid" ] || return 1
    [ "${actual_identity##*:}" = 700 ] || return 1
}

validate_owned_file() {
    file=$1
    expected_identity=$2
    [ -f "$file" ] && [ ! -L "$file" ] || return 1
    actual_identity=$(path_identity "$file") || return 1
    [ "$actual_identity" = "$expected_identity" ] || return 1
    [ "$(identity_owner "$actual_identity")" = "$current_uid" ] || return 1
    [ "${actual_identity##*:}" = 600 ] || return 1
}

validate_tmp_root() {
    directory=$1
    identity=$(path_identity "$directory") || return 1
    owner=$(identity_owner "$identity")
    mode=${identity##*:}
    if [ "$owner" = "$current_uid" ]; then
        case "$mode" in [0-7][0145][0145]) return 0;; esac
    fi
    if [ "$owner" = 0 ]; then
        case "$mode" in 1[0-7][0-7][2367]) return 0;; *) return 1;; esac
    fi
    return 1
}

wasm_verifier=
container=
adapter=both
full=0
seen_verifier=0
seen_container=0
seen_adapter=0
seen_full=0
while [ "$#" -gt 0 ]; do
    case "$1" in
        --wasm-verifier)
            [ "$seen_verifier" -eq 0 ] && [ "$#" -ge 2 ] || usage
            wasm_verifier=$2; seen_verifier=1; shift 2
            ;;
        --container)
            [ "$seen_container" -eq 0 ] && [ "$#" -ge 2 ] || usage
            container=$2; seen_container=1; shift 2
            ;;
        --adapter)
            [ "$seen_adapter" -eq 0 ] && [ "$#" -ge 2 ] || usage
            adapter=$2; seen_adapter=1; shift 2
            ;;
        --full)
            [ "$seen_full" -eq 0 ] || usage
            full=1; seen_full=1; shift
            ;;
        *) usage ;;
    esac
done
[ "$seen_verifier" -eq 1 ] && [ "$seen_container" -eq 1 ] || usage
case "$adapter" in batch|single|both) :;; *) usage;; esac
safe_absolute_path "$wasm_verifier" || fail configuration 'unsafe wasm-verifier checkout path'
safe_container_name "$container" || fail configuration 'unsafe running-container name'
[ -d "$wasm_verifier" ] && [ ! -L "$wasm_verifier" ] || fail configuration 'wasm-verifier checkout is not a nonsymlink directory'
resolved_verifier=$(CDPATH= cd -- "$wasm_verifier" && pwd -P)
[ "$resolved_verifier" = "$wasm_verifier" ] || fail configuration 'wasm-verifier checkout must be canonical'

repo_root=$(CDPATH= cd -- "$(dirname -- "$0")/.." && pwd -P)
rust_runner=$repo_root/ci/rocq-rust-docker.sh
inference_pin=$repo_root/core/wasm-to-v/wasm-verifier-pin.txt
container_pin=$resolved_verifier/ci/discharge/container-pin.json
batch_bridge=$resolved_verifier/ci/discharge/run-docker-batch.sh
case_bridge=$resolved_verifier/ci/discharge/run-docker-case.sh
container_inspector=ci/discharge/inspect-container.sh
docker_requested=${DOCKER:-docker}
git_bin=${GIT:-git}
[ -x "$rust_runner" ] || fail configuration 'missing Task-0 Rust Docker helper'
[ -f "$inference_pin" ] && [ ! -L "$inference_pin" ] || fail configuration 'missing Inference verifier pin'
[ -f "$container_pin" ] && [ ! -L "$container_pin" ] || fail configuration 'missing verifier container-pin.json prerequisite (Phase A has no bridge contract yet)'
[ -f "$resolved_verifier/$container_inspector" ] && [ ! -L "$resolved_verifier/$container_inspector" ] && [ -x "$resolved_verifier/$container_inspector" ] || fail configuration 'missing verifier container inspection prerequisite (Phase A has no bridge contract yet)'
case "$adapter" in batch|both) [ -f "$batch_bridge" ] && [ ! -L "$batch_bridge" ] && [ -x "$batch_bridge" ] || fail configuration 'missing verifier batch bridge prerequisite (Phase A has no bridge yet)';; esac
case "$adapter" in single|both) [ -f "$case_bridge" ] && [ ! -L "$case_bridge" ] && [ -x "$case_bridge" ] || fail configuration 'missing verifier single bridge prerequisite (Phase A has no bridge yet)';; esac
safe_text "$docker_requested" || fail configuration 'unsafe Docker executable input'
case "$docker_requested" in
    */*) docker_candidate=$docker_requested ;;
    *) docker_candidate=$(command -v "$docker_requested" 2>/dev/null || true) ;;
esac
docker_bin=$(canonical_executable "$docker_candidate") || fail configuration 'Docker executable is missing or noncanonical'
wrapper_executable=$(canonical_executable "$repo_root/ci/rocq-discharge-docker.sh") || fail configuration 'wrapper executable is noncanonical'
[ "$docker_bin" != "$wrapper_executable" ] && [ ! "$docker_bin" -ef "$wrapper_executable" ] || fail configuration 'Docker executable resolves to this wrapper'
current_uid=$(id -u) || fail configuration 'could not determine caller uid'
digits_only "$current_uid" || fail configuration 'caller uid is not canonical numeric data'

pin_value() {
    key=$1
    values=$(sed -n "s/^$key \\([^[:space:]]*\\)$/\\1/p" "$inference_pin")
    [ -n "$values" ] && [ "$(printf '%s\n' "$values" | wc -l | tr -d ' ')" -eq 1 ] || fail pin "invalid Inference pin field $key"
    printf '%s\n' "$values"
}
verifier_revision=$(pin_value revision)
coq_wasm_tag=$(pin_value coq-wasm-tag)
coq_wasm_revision=$(pin_value coq-wasm-commit)
coq_series=$(pin_value coq)
[ "${#verifier_revision}" -eq 40 ] && [ "${#coq_wasm_revision}" -eq 40 ] || fail pin 'noncanonical pinned revision length'
case "$verifier_revision:$coq_wasm_revision" in *[!0-9a-f:]*) fail pin 'noncanonical pinned revision';; esac
safe_text "$coq_wasm_tag" || fail pin 'unsafe coq-wasm tag'
[ "$coq_series" = 8.20 ] || fail pin 'Task 4 requires pinned Coq series 8.20'

container_pin_line() {
    line_number=$1
    key=$2
    comma=$3
    if [ "$comma" = yes ]; then
        sed -n "${line_number}s/^  \"$key\": \"\\([^\"]*\\)\",$/\\1/p" "$container_pin"
    else
        sed -n "${line_number}s/^  \"$key\": \"\\([^\"]*\\)\"$/\\1/p" "$container_pin"
    fi
}
[ "$(wc -l <"$container_pin" | tr -d ' ')" -eq 8 ] || fail pin 'container pin must use the canonical eight-line grammar'
[ "$(sed -n '1p' "$container_pin")" = '{' ] || fail pin 'container pin opening brace is not canonical'
[ "$(sed -n '2p' "$container_pin")" = '  "protocol": 1,' ] || fail pin 'container pin protocol line is not canonical'
pinned_image_reference=$(container_pin_line 3 image_reference yes)
pinned_image_id=$(container_pin_line 4 image_id yes)
pinned_user=$(container_pin_line 5 coq_user yes)
pinned_repository_mount=$(container_pin_line 6 repository_mount yes)
pinned_coq_version=$(container_pin_line 7 coq_version no)
[ -n "$pinned_image_reference" ] && [ -n "$pinned_image_id" ] && [ -n "$pinned_user" ] && [ -n "$pinned_repository_mount" ] && [ -n "$pinned_coq_version" ] || fail pin 'container pin fields or ordering are not canonical'
[ "$(sed -n '8p' "$container_pin")" = '}' ] || fail pin 'container pin closing brace is not canonical'
safe_text "$pinned_image_reference" || fail pin 'unsafe image reference in container pin'
safe_text "$pinned_coq_version" || fail pin 'unsafe Coq version in container pin'
[ "${#pinned_image_id}" -eq 71 ] || fail pin 'invalid image ID length'
case "${pinned_image_id#sha256:}" in
    *[!0-9a-f]*|'') fail pin 'invalid image ID' ;;
esac
case "$pinned_image_id" in sha256:*) :;; *) fail pin 'invalid image ID';; esac
[ "$pinned_user" = coq ] || fail pin 'container pin must require coq user'
safe_absolute_path "$pinned_repository_mount" || fail pin 'unsafe repository mount in container pin'
[ "$pinned_coq_version" = 8.20.1 ] || fail pin 'container pin must require exact Coq 8.20.1'

assert_git_clean() {
    if observed_revision=$("$git_bin" -C "$resolved_verifier" rev-parse HEAD 2>/dev/null); then :; else fail pin 'wasm-verifier revision inspection failed'; fi
    [ "$observed_revision" = "$verifier_revision" ] || fail pin 'wasm-verifier checkout revision mismatch'
    if observed_dirty=$("$git_bin" -C "$resolved_verifier" status --porcelain --untracked-files=all 2>/dev/null); then :; else fail pin 'wasm-verifier cleanliness inspection failed'; fi
    [ -z "$observed_dirty" ] || fail pin 'wasm-verifier checkout is not clean'
    set -- "$container_inspector" ci/discharge/container-pin.json
    case "$adapter" in batch|both) set -- "$@" ci/discharge/run-docker-batch.sh;; esac
    case "$adapter" in single|both) set -- "$@" ci/discharge/run-docker-case.sh;; esac
    "$git_bin" -C "$resolved_verifier" diff --quiet HEAD -- "$@" 2>/dev/null || fail pin 'verifier bridge contract differs from clean Git content'
}

assert_git_clean
inspector_identity=$(path_identity "$resolved_verifier/$container_inspector") || fail pin 'could not record verifier inspector identity'
container_pin_identity=$(path_identity "$container_pin") || fail pin 'could not record container pin identity'
batch_bridge_identity=
case_bridge_identity=
case "$adapter" in batch|both) batch_bridge_identity=$(path_identity "$batch_bridge") || fail pin 'could not record batch bridge identity';; esac
case "$adapter" in single|both) case_bridge_identity=$(path_identity "$case_bridge") || fail pin 'could not record single bridge identity';; esac

validate_contract_file() {
    file=$1
    expected_identity=$2
    [ -f "$file" ] && [ ! -L "$file" ] || return 1
    [ "$(path_identity "$file")" = "$expected_identity" ] || return 1
}

assert_verifier_checkout() {
    assert_git_clean
    validate_contract_file "$resolved_verifier/$container_inspector" "$inspector_identity" && [ -x "$resolved_verifier/$container_inspector" ] || fail pin 'verifier inspector identity changed'
    validate_contract_file "$container_pin" "$container_pin_identity" || fail pin 'container pin identity changed'
    case "$adapter" in batch|both) validate_contract_file "$batch_bridge" "$batch_bridge_identity" && [ -x "$batch_bridge" ] || fail pin 'batch bridge identity changed';; esac
    case "$adapter" in single|both) validate_contract_file "$case_bridge" "$case_bridge_identity" && [ -x "$case_bridge" ] || fail pin 'single bridge identity changed';; esac
}

inspect_container_value() {
    format=$1
    if inspected=$("$docker_bin" container inspect --format "$format" "$container" 2>/dev/null); then :; else fail container 'running container inspection failed'; fi
    printf '%s\n' "$inspected"
}

canonical_positive_decimal() {
    case "$1" in ''|0|*[!0-9]*|0*) return 1;; [1-9]*) return 0;; *) return 1;; esac
}

assert_live_container() {
    actual_running=$(inspect_container_value '{{.State.Running}}')
    [ "$actual_running" = true ] || fail container 'configured verifier container is not running'
    actual_image_reference=$(inspect_container_value '{{.Config.Image}}')
    [ "$actual_image_reference" = "$pinned_image_reference" ] || fail container 'running container image reference mismatch'
    actual_image_id=$(inspect_container_value '{{.Image}}')
    [ "$actual_image_id" = "$pinned_image_id" ] || fail container 'running container image ID mismatch'
    actual_user=$(inspect_container_value '{{.Config.User}}')
    [ "$actual_user" = "$pinned_user" ] || fail container 'running container configured user mismatch'
    mounts=$(inspect_container_value '{{range .Mounts}}{{printf "%s\t%s\n" .Destination .Source}}{{end}}')
    mount_count=0
    mount_tab=$(printf '\t')
    while IFS="$mount_tab" read -r destination source; do
        [ -n "$destination" ] || continue
        safe_absolute_path "$destination" && safe_absolute_path "$source" || fail container 'unsafe running-container mount inspection'
        case "$destination:$source" in *docker.sock*) fail container 'running container exposes a Docker socket mount';; esac
        if [ "$destination" = "$pinned_repository_mount" ]; then
            [ "$source" = "$resolved_verifier" ] || fail container 'repository mount source is not the canonical verifier checkout'
            mount_count=$((mount_count + 1))
        fi
    done <<MOUNTS
$mounts
MOUNTS
    [ "$mount_count" -eq 1 ] || fail container 'running container repository mount destination mismatch'

    provenance_sentinel=__INFERENCE_PROVENANCE_END__
    if provenance=$(
        "$docker_bin" exec --user coq --workdir "$pinned_repository_mount" "$container" "$container_inspector" 2>/dev/null || exit $?
        printf '%s' "$provenance_sentinel"
    ); then :; else fail container 'container provenance inspector exited nonzero'; fi
    [ -n "$provenance" ] && [ "${#provenance}" -le 1200 ] || fail container 'container provenance inspection failed'
    coq_uid=$(printf '%s\n' "$provenance" | sed -n '2s/^coq_uid=//p')
    coq_gid=$(printf '%s\n' "$provenance" | sed -n '3s/^coq_gid=//p')
    canonical_positive_decimal "$coq_uid" && canonical_positive_decimal "$coq_gid" || fail container 'coq user must have canonical positive uid and gid'
    expected_provenance=$(printf 'coq_user=coq\ncoq_uid=%s\ncoq_gid=%s\nwasm_verifier_revision=%s\ncoq_version=8.20.1\ncoq_wasm_origin=https://github.com/WasmCert/WasmCert-Coq.git\ncoq_wasm_tag=%s\ncoq_wasm_revision=%s\n%s' "$coq_uid" "$coq_gid" "$verifier_revision" "$coq_wasm_tag" "$coq_wasm_revision" "$provenance_sentinel")
    [ "$provenance" = "$expected_provenance" ] || fail container 'container provenance is not the exact canonical eight-line contract'
}

assert_verifier_checkout
assert_live_container

tmp_root=${TMPDIR:-/tmp}
safe_absolute_path "$tmp_root" || fail configuration 'unsafe temporary-directory root'
[ -d "$tmp_root" ] && [ ! -L "$tmp_root" ] || fail configuration 'temporary-directory root is not a nonsymlink directory'
tmp_root=$(CDPATH= cd -- "$tmp_root" && pwd -P)
validate_tmp_root "$tmp_root" || fail configuration 'temporary-directory root ownership or permissions are unsafe'
old_umask=$(umask)
umask 077
staging=$(mktemp -d "$tmp_root/inference-rocq-discharge.XXXXXX")
evidence_dir=
evidence_retained=0
evidence_identity=
capture_file=
capture_identity=
umask "$old_umask"
staging=$(CDPATH= cd -- "$staging" && pwd -P)
safe_absolute_path "$staging" || fail configuration 'unsafe resolved staging directory'
chmod 700 "$staging"
staging_identity=$(path_identity "$staging") || fail configuration 'could not record staging directory identity'

run_token=${staging##*.}
valid_run_token "$run_token" || fail configuration 'mktemp returned an invalid random run token'
run_owner=task4-$run_token
source_volume=inference-rocq-discharge-$run_token-source
exchange_volume=inference-rocq-discharge-$run_token-exchange
source_owner_file=$staging/source.owner
exchange_created=0
cleanup_complete=0
staging_suspect=0
evidence_suspect=0
capture_fd_open=0
full_log_fd_open=0
full_log=
full_log_identity=

assert_staging_identity() {
    validate_owned_directory "$staging" "$staging_identity" || { staging_suspect=1; fail staging-identity 'staging directory identity or mode changed'; }
}

volume_owner() {
    if inspected_owner=$("$docker_bin" volume inspect --format "{{ index .Labels \"$owner_label\" }}" "$1" 2>/dev/null); then :; else return $?; fi
    printf '%s\n' "$inspected_owner"
}

cleanup() {
    status=$?
    trap - EXIT
    if [ "$capture_fd_open" -eq 1 ]; then exec 3>&-; capture_fd_open=0; fi
    if [ "$full_log_fd_open" -eq 1 ]; then exec 4>&- 5>&-; full_log_fd_open=0; fi
    [ "$cleanup_complete" -eq 0 ] || exit "$status"
    if [ -f "$source_owner_file" ] && [ ! -L "$source_owner_file" ]; then
        source_owner=$(sed -n '1p' "$source_owner_file")
        if digits_only "$source_owner" && [ "$(volume_owner "$source_volume")" = "$source_owner" ]; then
            "$docker_bin" volume rm "$source_volume" >/dev/null 2>&1 || true
        fi
    fi
    if [ "$exchange_created" -eq 1 ] && [ "$(volume_owner "$exchange_volume")" = "$run_owner" ]; then
        "$docker_bin" volume rm "$exchange_volume" >/dev/null 2>&1 || true
    fi
    if [ -n "$evidence_dir" ] && [ "$evidence_retained" -eq 0 ] && [ "$evidence_suspect" -eq 0 ]; then
        case "$evidence_dir" in
            "$tmp_root"/wasm-verifier-discharge-evidence.*)
                if validate_owned_directory "$evidence_dir" "$evidence_identity" 2>/dev/null; then
                    if [ -z "$capture_file" ] || validate_owned_file "$capture_file" "$capture_identity" 2>/dev/null; then
                        rm -rf "$evidence_dir"
                    fi
                fi
                ;;
        esac
    fi
    if [ "$staging_suspect" -eq 0 ]; then case "$staging" in
        "$tmp_root"/inference-rocq-discharge.*)
            if validate_owned_directory "$staging" "$staging_identity" 2>/dev/null; then
                if [ -z "$full_log" ] || validate_owned_file "$full_log" "$full_log_identity" 2>/dev/null; then
                    rm -rf "$staging"
                fi
            fi
            ;;
    esac; fi
    exit "$status"
}
trap cleanup EXIT
trap 'exit 129' HUP
trap 'exit 130' INT
trap 'exit 143' TERM

if [ "$full" -eq 1 ]; then
    assert_staging_identity
    old_umask=$(umask); umask 077
    full_log=$(mktemp "$staging/inference-tests.log.XXXXXX")
    umask "$old_umask"
    [ -f "$full_log" ] && [ ! -L "$full_log" ] || fail full-log-identity 'could not securely create full-test log'
    chmod 600 "$full_log"
    if exec 4>"$full_log" 5<"$full_log"; then :; else fail full-log-identity 'could not open retained full-test log descriptors'; fi
    full_log_fd_open=1
    full_log_identity=$(path_identity "$full_log") || fail full-log-identity 'could not record full-test log identity'
    validate_owned_file "$full_log" "$full_log_identity" || fail full-log-identity 'full-test log identity or mode was unsafe at creation'
fi

if "$docker_bin" volume inspect "$source_volume" >/dev/null 2>&1 || "$docker_bin" volume inspect "$exchange_volume" >/dev/null 2>&1; then
    fail cleanup 'unique transient volume name collision'
fi
"$docker_bin" volume create --label "$owner_label=$run_owner" "$exchange_volume" >/dev/null
exchange_created=1
[ "$(volume_owner "$exchange_volume")" = "$run_owner" ] || fail cleanup 'exchange volume owner label did not validate'

run_rust() (
    exec 3>&- 4>&- 5>&-
    INFERENCE_ROCQ_DOCKER_PROXY_MODE=1 \
    INFERENCE_ROCQ_DOCKER_PROXY_REAL_DOCKER="$docker_bin" \
    INFERENCE_ROCQ_DOCKER_PROXY_SOURCE_VOLUME="$source_volume" \
    INFERENCE_ROCQ_DOCKER_PROXY_EXCHANGE_VOLUME="$exchange_volume" \
    INFERENCE_ROCQ_DOCKER_PROXY_RUN_OWNER="$run_owner" \
    INFERENCE_ROCQ_DOCKER_PROXY_SOURCE_OWNER_FILE="$source_owner_file" \
    DOCKER="$repo_root/ci/rocq-discharge-docker.sh" "$rust_runner" cargo "$@"
)

busybox_run() {
    "$docker_bin" run --rm \
        --read-only \
        --network none \
        --cap-drop ALL \
        --security-opt no-new-privileges \
        --tmpfs /tmp \
        "$@"
}

if [ "$full" -eq 1 ]; then
    run_rust test --color never -p inference-tests rocq_dischargeability:: -- --test-threads=1
fi
run_rust run -p inference-tests --bin rocq-discharge -- export --exchange /exchange

fingerprint_exchange() {
    if fingerprint_output=$(busybox_run \
        --mount "type=volume,src=$exchange_volume,dst=/exchange,readonly" \
        "$busybox_image" sh -c '
            # task4-fingerprint
            material=/tmp/material
            : >"$material" || exit 41
            for path in \
                request.json \
                raw/rocq_prime_bounded_example.v \
                raw/rocq_exists_spec.v \
                raw/rocq_unique_spec.v \
                raw/spec_narrow_discharge.v \
                raw/rocq_false_certificate.v
            do
                file=/exchange/$path
                [ -f "$file" ] && [ ! -L "$file" ] || exit 42
                printf "%s\000" "$path" >>"$material" || exit 43
                byte_count=$(wc -c <"$file") || exit 44
                set -- $byte_count
                [ "$#" -eq 1 ] || exit 45
                case "$1" in ""|*[!0-9]*) exit 46;; esac
                printf "%s\000" "$1" >>"$material" || exit 47
                cat "$file" >>"$material" || exit 48
                printf "\000" >>"$material" || exit 49
            done
            hash_output=$(sha256sum "$material") || exit 50
            set -- $hash_output
            [ "$#" -eq 2 ] && [ "$2" = "$material" ] || exit 51
            digest=$1
            [ "${#digest}" -eq 64 ] || exit 52
            case "$digest" in *[!0-9a-f]*) exit 53;; esac
            printf "%s\n" "$digest" || exit 54
        ' 2>/dev/null
    ); then :; else return $?; fi
    [ "${#fingerprint_output}" -eq 64 ] || return 55
    case "$fingerprint_output" in *[!0-9a-f]*) return 56;; esac
    printf '%s\n' "$fingerprint_output"
}

if exchange_fingerprint=$(fingerprint_exchange); then :; else fail input-integrity 'could not establish immutable request/raw fingerprint'; fi
bridge_raw_basename=
assert_identity() {
    if current_fingerprint=$(fingerprint_exchange); then :; else fail input-integrity 'immutable request/raw fingerprint helper failed'; fi
    [ "$current_fingerprint" = "$exchange_fingerprint" ] || fail input-integrity 'immutable request/raw fingerprint changed'
}

identity_matches() {
    if current_fingerprint=$(fingerprint_exchange); then :; else return 1; fi
    [ "$current_fingerprint" = "$exchange_fingerprint" ]
}

prepare_evidence() {
    [ -n "$evidence_dir" ] && return
    old_umask=$(umask); umask 077
    evidence_dir=$(mktemp -d "$tmp_root/wasm-verifier-discharge-evidence.XXXXXX")
    umask "$old_umask"
    evidence_dir=$(CDPATH= cd -- "$evidence_dir" && pwd -P)
    safe_absolute_path "$evidence_dir" || fail configuration 'unsafe evidence directory'
    chmod 700 "$evidence_dir"
    evidence_identity=$(path_identity "$evidence_dir") || fail evidence-contract 'could not record private evidence directory identity'
    validate_owned_directory "$evidence_dir" "$evidence_identity" || fail evidence-contract 'private evidence directory was unsafe at creation'
    capture_file=$evidence_dir/bridge-output.log
    old_umask=$(umask); umask 077
    set -C
    : >"$capture_file" || { set +C; umask "$old_umask"; fail evidence-contract 'could not securely create bridge capture'; }
    set +C
    if exec 3>>"$capture_file"; then :; else umask "$old_umask"; fail evidence-contract 'could not open retained bridge capture descriptor'; fi
    capture_fd_open=1
    umask "$old_umask"
    chmod 600 "$capture_file"
    capture_identity=$(path_identity "$capture_file") || fail evidence-contract 'could not record bridge capture identity'
    validate_owned_file "$capture_file" "$capture_identity" || fail evidence-contract 'bridge capture identity or mode was unsafe at creation'
}

validate_evidence_directory() {
    validate_owned_directory "$evidence_dir" "$evidence_identity" &&
        validate_owned_file "$capture_file" "$capture_identity"
}

validate_full_log() {
    [ "$full" -eq 0 ] || validate_owned_file "$full_log" "$full_log_identity"
}

require_evidence_identity() {
    message=$1
    validate_evidence_directory || { evidence_suspect=1; fail evidence-contract "$message"; }
}

require_full_log_identity() {
    message=$1
    validate_full_log || { staging_suspect=1; fail full-log-identity "$message"; }
}

validate_failure_evidence() {
    validate_evidence_directory || return 1
    [ -f "$evidence_dir/verifier.log" ] && [ ! -L "$evidence_dir/verifier.log" ] || return 1
    verifier_log_identity=$(path_identity "$evidence_dir/verifier.log") || return 1
    validate_owned_file "$evidence_dir/verifier.log" "$verifier_log_identity" || return 1
    [ "$(find "$evidence_dir" -mindepth 1 -maxdepth 1 -type f | wc -l | tr -d ' ')" -eq 2 ] || return 1
    [ "$(find "$evidence_dir" -mindepth 1 -maxdepth 1 -print | wc -l | tr -d ' ')" -eq 2 ] || return 1
}

bridge_receipt_dir=
run_bridge() {
    phase=$1
    shift
    assert_identity
    if [ -n "$bridge_raw_basename" ]; then validate_staged_raw "$bridge_raw_basename"; fi
    require_full_log_identity 'full-test log identity or mode changed before bridge invocation'
    prepare_evidence
    require_evidence_identity 'private evidence directory identity or mode changed'
    [ ! -e "$evidence_dir/verifier.log" ] && [ ! -L "$evidence_dir/verifier.log" ] || fail evidence-contract 'stale verifier evidence existed before bridge invocation'
    assert_verifier_checkout
    assert_live_container
    old_umask=$(umask)
    umask 077
    if (
        exec 1>&3 2>&1
        exec 3>&- 4>&- 5>&-
        DOCKER="$docker_bin" \
        INFERENCE_WASM_VERIFIER_EVIDENCE_DIR="$evidence_dir" \
        INFERENCE_WASM_VERIFIER_RECEIPT_DIR="$bridge_receipt_dir" \
        WASM_VERIFIER_CONTAINER="$container" \
        exec "$@"
    )
    then
        umask "$old_umask"
        assert_verifier_checkout
        assert_live_container
        assert_staging_identity
        assert_identity
        if [ -n "$bridge_raw_basename" ]; then validate_staged_raw "$bridge_raw_basename"; fi
        require_full_log_identity 'full-test log identity or mode changed after bridge invocation'
        require_evidence_identity 'private evidence directory identity or mode changed'
        [ ! -e "$evidence_dir/verifier.log" ] && [ ! -L "$evidence_dir/verifier.log" ] || fail evidence-contract 'successful bridge left a private failure log'
        return 0
    else
        status=$?
        umask "$old_umask"
        post_staging_ok=1
        post_identity_ok=1
        post_staged_raw_ok=1
        post_verifier_ok=1
        post_container_ok=1
        post_full_log_ok=1
        (assert_verifier_checkout) >/dev/null 2>&1 || post_verifier_ok=0
        (assert_live_container) >/dev/null 2>&1 || post_container_ok=0
        validate_owned_directory "$staging" "$staging_identity" || post_staging_ok=0
        identity_matches >/dev/null 2>&1 || post_identity_ok=0
        if [ -n "$bridge_raw_basename" ]; then staged_raw_matches "$bridge_raw_basename" >/dev/null 2>&1 || post_staged_raw_ok=0; fi
        validate_full_log || post_full_log_ok=0
        if ! validate_evidence_directory; then
            evidence_suspect=1
            fail evidence-contract 'bridge changed private evidence directory or capture identity'
        fi
        if ! validate_failure_evidence; then
            fail evidence-contract 'bridge failed without one safe private verifier log'
        fi
        evidence_retained=1
        [ "$post_staging_ok" -eq 1 ] || phase=staging-identity
        [ "$post_identity_ok" -eq 1 ] && [ "$post_staged_raw_ok" -eq 1 ] || phase=input-integrity
        [ "$post_verifier_ok" -eq 1 ] || phase=verifier-contract
        [ "$post_container_ok" -eq 1 ] || phase=container-contract
        [ "$post_full_log_ok" -eq 1 ] || { phase=full-log-identity; staging_suspect=1; }
        echo "rocq-discharge-docker: phase=$phase evidence=$evidence_dir" >&2
        exit "$status"
    fi
}

verify_exchange() {
    assert_identity
    if run_rust run -p inference-tests --bin rocq-discharge -- verify --exchange /exchange; then
        verify_status=0
    else
        verify_status=$?
    fi
    assert_identity
    assert_staging_identity
    [ "$verify_status" -eq 0 ] || fail verify "Rust receipt verification failed in pinned Docker (status $verify_status)"
}

run_batch() {
    bridge_receipt_dir=
    run_bridge batch "$batch_bridge" --exchange-volume "$exchange_volume"
    verify_exchange
}

copy_raw_to_staging() {
    mkdir -m 700 "$staging/raw" "$staging/receipts"
    busybox_run \
        --mount "type=volume,src=$exchange_volume,dst=/exchange,readonly" \
        --mount "type=bind,src=$staging,dst=/staging" \
        "$busybox_image" sh -c '
            # task4-copy-raw
            set -eu
            cp /exchange/raw/rocq_prime_bounded_example.v /staging/raw/rocq_prime_bounded_example.v
            cp /exchange/raw/rocq_exists_spec.v /staging/raw/rocq_exists_spec.v
            cp /exchange/raw/rocq_unique_spec.v /staging/raw/rocq_unique_spec.v
            cp /exchange/raw/spec_narrow_discharge.v /staging/raw/spec_narrow_discharge.v
            cp /exchange/raw/rocq_false_certificate.v /staging/raw/rocq_false_certificate.v
            cmp -s /exchange/raw/rocq_prime_bounded_example.v /staging/raw/rocq_prime_bounded_example.v
            cmp -s /exchange/raw/rocq_exists_spec.v /staging/raw/rocq_exists_spec.v
            cmp -s /exchange/raw/rocq_unique_spec.v /staging/raw/rocq_unique_spec.v
            cmp -s /exchange/raw/spec_narrow_discharge.v /staging/raw/spec_narrow_discharge.v
            cmp -s /exchange/raw/rocq_false_certificate.v /staging/raw/rocq_false_certificate.v
        '
}

validate_staged_raw() {
    staged_raw_matches "$1" || fail input-integrity 'staged raw file differs from immutable exchange input'
}

staged_raw_matches() {
    basename=$1
    busybox_run \
        --mount "type=volume,src=$exchange_volume,dst=/exchange,readonly" \
        --mount "type=bind,src=$staging,dst=/staging,readonly" \
        "$busybox_image" sh -c '
            # task4-check-staged-raw
            set -eu
            case "$1" in
                rocq_prime_bounded_example.v|rocq_exists_spec.v|rocq_unique_spec.v|spec_narrow_discharge.v|rocq_false_certificate.v) : ;;
                *) exit 42 ;;
            esac
            [ -f "/exchange/raw/$1" ] && [ ! -L "/exchange/raw/$1" ]
            [ -f "/staging/raw/$1" ] && [ ! -L "/staging/raw/$1" ]
            cmp -s "/exchange/raw/$1" "/staging/raw/$1"
        ' sh "$basename"
}

remove_batch_receipts() {
    assert_identity
    busybox_run \
        --mount "type=volume,src=$exchange_volume,dst=/exchange" \
        "$busybox_image" sh -c '
            # task4-remove-receipts
            set -eu
            directory=/exchange/receipts
            [ -d "$directory" ] && [ ! -L "$directory" ]
            for case_id in prime-bounded exists unique narrow-domain false-spec; do
                [ -f "$directory/$case_id.json" ] && [ ! -L "$directory/$case_id.json" ]
            done
            [ "$(find "$directory" -mindepth 1 -maxdepth 1 -type f | wc -l | tr -d " ")" -eq 5 ]
            [ "$(find "$directory" -mindepth 1 -maxdepth 1 -print | wc -l | tr -d " ")" -eq 5 ]
            rm -f \
                "$directory/prime-bounded.json" \
                "$directory/exists.json" \
                "$directory/unique.json" \
                "$directory/narrow-domain.json" \
                "$directory/false-spec.json"
            rmdir "$directory"
        '
    assert_identity
}

validate_single_receipt_layout() {
    case_id=$1
    directory=$2
    expected_identity=$3
    validate_owned_directory "$directory" "$expected_identity" || { staging_suspect=1; fail single 'single-case receipt directory identity or mode changed'; }
    [ -f "$directory/$case_id.json" ] && [ ! -L "$directory/$case_id.json" ] || { staging_suspect=1; fail single 'missing single-case receipt'; }
    [ "$(find "$directory" -mindepth 1 -maxdepth 1 -type f | wc -l | tr -d ' ')" -eq 1 ] || { staging_suspect=1; fail single 'single-case receipt directory is not exact'; }
    [ "$(find "$directory" -mindepth 1 -maxdepth 1 -print | wc -l | tr -d ' ')" -eq 1 ] || { staging_suspect=1; fail single 'single-case receipt directory contains an extra entry'; }
}

validate_single_receipt() {
    case_id=$1
    directory=$2
    expected_directory_identity=$3
    expected_file_identity=$4
    validate_single_receipt_layout "$case_id" "$directory" "$expected_directory_identity"
    validate_owned_file "$directory/$case_id.json" "$expected_file_identity" || { staging_suspect=1; fail single 'single-case receipt file identity or mode changed'; }
}

copy_single_receipts() {
    assert_identity
    busybox_run \
        --mount "type=volume,src=$exchange_volume,dst=/exchange" \
        --mount "type=bind,src=$staging,dst=/staging,readonly" \
        "$busybox_image" sh -c '
            # task4-copy-receipts
            set -eu
            [ ! -e /exchange/receipts ]
            mkdir -m 700 /exchange/receipts
            cp /staging/receipts/prime-bounded/prime-bounded.json /exchange/receipts/prime-bounded.json
            cp /staging/receipts/exists/exists.json /exchange/receipts/exists.json
            cp /staging/receipts/unique/unique.json /exchange/receipts/unique.json
            cp /staging/receipts/narrow-domain/narrow-domain.json /exchange/receipts/narrow-domain.json
            cp /staging/receipts/false-spec/false-spec.json /exchange/receipts/false-spec.json
        '
    assert_identity
}

run_single() {
    copy_raw_to_staging
    prime_bounded_receipt_dir_identity= prime_bounded_receipt_file_identity=
    exists_receipt_dir_identity= exists_receipt_file_identity=
    unique_receipt_dir_identity= unique_receipt_file_identity=
    narrow_domain_receipt_dir_identity= narrow_domain_receipt_file_identity=
    false_spec_receipt_dir_identity= false_spec_receipt_file_identity=
    for record in \
        'prime-bounded:rocq_prime_bounded_example.v' \
        'exists:rocq_exists_spec.v' \
        'unique:rocq_unique_spec.v' \
        'narrow-domain:spec_narrow_discharge.v' \
        'false-spec:rocq_false_certificate.v'
    do
        case_id=${record%%:*}
        basename=${record#*:}
        receipt_dir=$staging/receipts/$case_id
        mkdir -m 700 "$receipt_dir"
        receipt_identity=$(path_identity "$receipt_dir") || fail single 'could not record single-case receipt directory identity'
        validate_owned_directory "$receipt_dir" "$receipt_identity" || fail single 'single-case receipt directory was unsafe before bridge invocation'
        [ -z "$(find "$receipt_dir" -mindepth 1 -print -quit)" ] || fail single 'single-case receipt directory was not empty before bridge invocation'
        bridge_raw_basename=$basename
        bridge_receipt_dir=$receipt_dir
        run_bridge "single-$case_id" \
            "$case_bridge" --protocol 1 --wasm-verifier-revision "$verifier_revision" --case "$case_id" "$staging/raw/$basename"
        bridge_raw_basename=
        bridge_receipt_dir=
        validate_single_receipt_layout "$case_id" "$receipt_dir" "$receipt_identity"
        receipt_file_identity=$(path_identity "$receipt_dir/$case_id.json") || fail single 'could not record single-case receipt file identity'
        validate_single_receipt "$case_id" "$receipt_dir" "$receipt_identity" "$receipt_file_identity"
        case "$case_id" in
            prime-bounded) prime_bounded_receipt_dir_identity=$receipt_identity; prime_bounded_receipt_file_identity=$receipt_file_identity ;;
            exists) exists_receipt_dir_identity=$receipt_identity; exists_receipt_file_identity=$receipt_file_identity ;;
            unique) unique_receipt_dir_identity=$receipt_identity; unique_receipt_file_identity=$receipt_file_identity ;;
            narrow-domain) narrow_domain_receipt_dir_identity=$receipt_identity; narrow_domain_receipt_file_identity=$receipt_file_identity ;;
            false-spec) false_spec_receipt_dir_identity=$receipt_identity; false_spec_receipt_file_identity=$receipt_file_identity ;;
        esac
        assert_identity
    done
    validate_single_receipt prime-bounded "$staging/receipts/prime-bounded" "$prime_bounded_receipt_dir_identity" "$prime_bounded_receipt_file_identity"
    validate_single_receipt exists "$staging/receipts/exists" "$exists_receipt_dir_identity" "$exists_receipt_file_identity"
    validate_single_receipt unique "$staging/receipts/unique" "$unique_receipt_dir_identity" "$unique_receipt_file_identity"
    validate_single_receipt narrow-domain "$staging/receipts/narrow-domain" "$narrow_domain_receipt_dir_identity" "$narrow_domain_receipt_file_identity"
    validate_single_receipt false-spec "$staging/receipts/false-spec" "$false_spec_receipt_dir_identity" "$false_spec_receipt_file_identity"
    copy_single_receipts
    verify_exchange
}

case "$adapter" in
    batch) run_batch ;;
    single) run_single ;;
    both)
        run_batch
        remove_batch_receipts
        run_single
        ;;
esac

parse_full_results() {
    summary=$(LC_ALL=C awk -v expected_lines="$full_result_lines" -v floor="$full_passed_floor" '
        /^test result:/ {
            lines++
            if ($0 !~ /^test result: ok\. [0-9][0-9]* passed; 0 failed; [0-9][0-9]* ignored; [0-9][0-9]* measured; [0-9][0-9]* filtered out(; finished in .*)?$/) bad=1
            if ($2 == "result:" && $3 == "ok.") {
                passed += $4 + 0
                ignored += $8 + 0
                filtered += $12 + 0
            }
        }
        END {
            if (bad || lines != expected_lines || passed < floor || filtered != 0) exit 1
            printf "%d %d %d\n", lines, passed, ignored
        }
    ') || fail full-test-floor 'crate test result lines were empty, malformed, filtered-only, under floor, or wrong in count'
    set -- $summary
    echo "rocq-discharge-docker: full crate=inference-tests result-lines=$1 passed=$2 floor=$full_passed_floor ignored=$3"
}

if [ "$full" -eq 1 ]; then
    require_full_log_identity 'full-test log identity or mode changed before crate test'
    if run_rust test --color never -p inference-tests -- --test-threads=1 >&4 2>&1; then
        require_full_log_identity 'full-test log identity or mode changed after crate test'
        parse_full_results <&5
        exec 4>&- 5>&-
        full_log_fd_open=0
    else
        status=$?
        fail full-test "inference-tests crate failed in pinned Docker (status $status)"
    fi
fi

verified_success_cleanup() {
    assert_staging_identity
    assert_identity
    require_full_log_identity 'full-test log identity or mode changed before success cleanup'
    if [ -n "$evidence_dir" ]; then
        require_evidence_identity 'private evidence directory or capture identity changed before success cleanup'
        [ ! -e "$evidence_dir/verifier.log" ] && [ ! -L "$evidence_dir/verifier.log" ] || fail evidence-contract 'successful run retained a private failure log'
        [ "$(find "$evidence_dir" -mindepth 1 -maxdepth 1 -type f | wc -l | tr -d ' ')" -eq 1 ] || fail evidence-contract 'successful evidence directory is not exact'
        [ "$(find "$evidence_dir" -mindepth 1 -maxdepth 1 -print | wc -l | tr -d ' ')" -eq 1 ] || fail evidence-contract 'successful evidence directory contains an extra entry'
    fi
    if [ "$capture_fd_open" -eq 1 ]; then exec 3>&-; capture_fd_open=0; fi
    if [ "$full_log_fd_open" -eq 1 ]; then exec 4>&- 5>&-; full_log_fd_open=0; fi
    if [ -n "$evidence_dir" ]; then
        case "$evidence_dir" in "$tmp_root"/wasm-verifier-discharge-evidence.*) :;; *) fail cleanup 'unsafe evidence cleanup target';; esac
        rm -rf "$evidence_dir" || fail cleanup 'could not remove private evidence directory'
        [ ! -e "$evidence_dir" ] && [ ! -L "$evidence_dir" ] || fail cleanup 'private evidence directory survived cleanup'
        evidence_dir=
    fi
    if "$docker_bin" volume inspect "$source_volume" >/dev/null 2>&1; then
        [ -f "$source_owner_file" ] && [ ! -L "$source_owner_file" ] || fail cleanup 'source volume survived without a safe owner record'
        source_owner=$(sed -n '1p' "$source_owner_file")
        digits_only "$source_owner" && [ "$(volume_owner "$source_volume")" = "$source_owner" ] || fail cleanup 'source volume owner label did not validate at success cleanup'
        "$docker_bin" volume rm "$source_volume" >/dev/null 2>&1 || fail cleanup 'could not remove owned source volume'
        if "$docker_bin" volume inspect "$source_volume" >/dev/null 2>&1; then fail cleanup 'owned source volume survived cleanup'; fi
    fi
    [ "$(volume_owner "$exchange_volume")" = "$run_owner" ] || fail cleanup 'exchange volume owner label did not validate at success cleanup'
    "$docker_bin" volume rm "$exchange_volume" >/dev/null 2>&1 || fail cleanup 'could not remove owned exchange volume'
    exchange_created=0
    if "$docker_bin" volume inspect "$exchange_volume" >/dev/null 2>&1; then fail cleanup 'owned exchange volume survived cleanup'; fi
    assert_staging_identity
    require_full_log_identity 'full-test log identity changed at staging cleanup boundary'
    case "$staging" in "$tmp_root"/inference-rocq-discharge.*) :;; *) fail cleanup 'unsafe staging cleanup target';; esac
    rm -rf "$staging" || fail cleanup 'could not remove owned staging directory'
    [ ! -e "$staging" ] && [ ! -L "$staging" ] || fail cleanup 'owned staging directory survived cleanup'
    cleanup_complete=1
}

verified_success_cleanup
echo "rocq-discharge-docker: result=pass adapter=$adapter verifier=$verifier_revision coq=$pinned_coq_version"
