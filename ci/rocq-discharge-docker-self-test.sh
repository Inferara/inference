#!/usr/bin/env sh
# Exercises rocq-discharge-docker.sh against stateful Docker, Git, and bridge fakes.
set -eu

repo_root=$(
    CDPATH=
    export CDPATH
    cd -- "$(dirname -- "$0")/.." && pwd
)
runner_source=$repo_root/ci/rocq-discharge-docker.sh
rust_runner_source=$repo_root/ci/rocq-rust-docker.sh
work=$(mktemp -d "${TMPDIR:-/tmp}/rocq-discharge-docker-self-test.XXXXXX")
work=$(cd -- "$work" && pwd -P)
trap 'rm -rf "$work"' EXIT HUP INT TERM
test_tmp=$work/tmp
mkdir -m 700 "$test_tmp"
TMPDIR=$test_tmp
FAKE_TMP_ROOT=$test_tmp
export TMPDIR FAKE_TMP_ROOT

fixture=$work/repo
state=$work/state
fake_bin=$work/'fake tools'
fake_docker=$fake_bin/'docker tool'
fake_git=$fake_bin/'git tool'
FAKE_DOCKER_PATH=$fake_docker
export FAKE_DOCKER_PATH
verifier=$fixture/verifier
revision=181cd676662453182b9753d1b19ca933c68770c3
coq_wasm_revision=0fd83fa708922721132b6d6737179568d1f1d553
image_reference=ghcr.io/inferara/wasm-verifier-coq:8.20@sha256:1111111111111111111111111111111111111111111111111111111111111111
image_id=sha256:2222222222222222222222222222222222222222222222222222222222222222
repository_mount=/workspaces/wasm-verifier
rust_image='rust:1.98-bookworm@sha256:82150a52ec202c1b14d7817e14516c392bb7f5cfebd88f1ed531cb37ebd39922'
busybox_image='busybox:1.37.0@sha256:9db7b59979c38555a39def84a31fb98b5296952f9e3afd4f6f11f05b07adfab0'
owner_label='org.inferara.rocq-discharge.owner'

mkdir -p "$fixture/ci" "$fixture/core/wasm-to-v" "$verifier/ci/discharge" "$fake_bin"
cp "$runner_source" "$fixture/ci/rocq-discharge-docker.sh"
cp "$rust_runner_source" "$fixture/ci/rocq-rust-docker.sh"
cp "$repo_root/ci/rocq-discharge.cargo-lock" "$fixture/ci/"
cp "$repo_root/core/wasm-to-v/wasm-verifier-pin.txt" "$fixture/core/wasm-to-v/"
chmod +x "$fixture/ci/rocq-discharge-docker.sh" "$fixture/ci/rocq-rust-docker.sh"

write_container_pin() {
    cat >"$verifier/ci/discharge/container-pin.json" <<PIN
{
  "protocol": 1,
  "image_reference": "$image_reference",
  "image_id": "$image_id",
  "coq_user": "coq",
  "repository_mount": "$repository_mount",
  "coq_version": "8.20.1"
}
PIN
}
write_container_pin_unknown() {
    cat >"$verifier/ci/discharge/container-pin.json" <<PIN
  "unknown": "replacement-for-opening-brace",
  "protocol": 1,
  "image_reference": "$image_reference",
  "image_id": "$image_id",
  "coq_user": "coq",
  "repository_mount": "$repository_mount",
  "coq_version": "8.20.1"
}
PIN
}
write_container_pin_missing_comma() {
    cat >"$verifier/ci/discharge/container-pin.json" <<PIN
{
  "protocol": 1,
  "image_reference": "$image_reference"
  "image_id": "$image_id",
  "coq_user": "coq",
  "repository_mount": "$repository_mount",
  "coq_version": "8.20.1"
}
PIN
}
write_container_pin_trailing_comma() {
    cat >"$verifier/ci/discharge/container-pin.json" <<PIN
{
  "protocol": 1,
  "image_reference": "$image_reference",
  "image_id": "$image_id",
  "coq_user": "coq",
  "repository_mount": "$repository_mount",
  "coq_version": "8.20.1",
}
PIN
}
write_container_pin_reordered() {
    cat >"$verifier/ci/discharge/container-pin.json" <<PIN
{
  "protocol": 1,
  "image_id": "$image_id",
  "image_reference": "$image_reference",
  "coq_user": "coq",
  "repository_mount": "$repository_mount",
  "coq_version": "8.20.1"
}
PIN
}
write_container_pin_alternate_patch() {
    cat >"$verifier/ci/discharge/container-pin.json" <<PIN
{
  "protocol": 1,
  "image_reference": "$image_reference",
  "image_id": "$image_id",
  "coq_user": "coq",
  "repository_mount": "$repository_mount",
  "coq_version": "8.20.2"
}
PIN
}
write_container_pin_malformed() {
    cat >"$verifier/ci/discharge/container-pin.json" <<PIN
{
  "protocol": 1,
  "image_reference": "$image_reference",
  "image_id": "$image_id",
  "coq_user": "coq",
  "repository_mount": "$repository_mount",
  "coq_version": "8.20.1"
]
PIN
}
write_container_pin_duplicate() {
    cat >"$verifier/ci/discharge/container-pin.json" <<PIN
{
  "protocol": 1,
  "image_reference": "$image_reference",
  "image_id": "$image_id",
  "coq_user": "coq",
  "repository_mount": "$repository_mount",
  "coq_version": "8.20.1",
  "coq_version": "8.20.1"
}
PIN
}
write_container_pin

cat >"$fake_git" <<'FAKE_GIT'
#!/usr/bin/env sh
set -eu
state=${FAKE_STATE:?}
printf 'git' >>"$state/git-calls"
for argument in "$@"; do printf ' <%s>' "$argument" >>"$state/git-calls"; done
printf '\n' >>"$state/git-calls"
case "$*" in
    *' rev-parse --verify HEAD:'*)
        printf '%040d\n' 1
        ;;
    *' rev-parse HEAD')
        if [ "${FAKE_MISMATCH:-}" = checkout-revision ]; then
            printf '%040d\n' 0
        else
            printf '%s\n' "${FAKE_REVISION:?}"
        fi
        ;;
    *' status --porcelain --untracked-files=all')
        if [ "${FAKE_MISMATCH:-}" = dirty-checkout ] || [ -f "$state/verifier-dirty" ]; then printf '?? private-proof.tmp\n'; fi
        ;;
    *' diff --quiet HEAD -- '*)
        [ ! -f "$state/verifier-dirty" ]
        ;;
    *' ls-files -v -- '*)
        for contract_path in "$@"; do :; done
        index_tag=H
        case "${FAKE_MISMATCH:-}" in
            skip-index) [ "$contract_path" != ci/discharge/inspect-container.sh ] || index_tag=S ;;
            assume-index) [ "$contract_path" != ci/discharge/inspect-container.sh ] || index_tag=h ;;
        esac
        printf '%s %s\n' "$index_tag" "$contract_path"
        ;;
    *' hash-object -- '*)
        if [ "${FAKE_MISMATCH:-}" = contract-blob ]; then
            printf '%040d\n' 2
        else
            printf '%040d\n' 1
        fi
        ;;
    *) echo "fake git: unexpected arguments: $*" >&2; exit 91 ;;
esac
FAKE_GIT
chmod +x "$fake_git"

cat >"$fake_docker" <<'FAKE_DOCKER'
#!/usr/bin/env sh
set -eu
state=${FAKE_STATE:?}
revision=${FAKE_REVISION:?}
coq_wasm_revision=${FAKE_COQ_WASM_REVISION:?}
image_reference=${FAKE_IMAGE_REFERENCE:?}
image_id=${FAKE_IMAGE_ID:?}
repository_mount=${FAKE_REPOSITORY_MOUNT:?}
mkdir -p "$state/calls" "$state/volumes" "$state/labels" "$state/containers"

next_number() {
    number=$(cat "$state/counter" 2>/dev/null || echo 0)
    number=$((number + 1))
    printf '%s\n' "$number" >"$state/counter"
    printf '%s\n' "$number"
}
record() {
    number=$(next_number)
    call=$state/calls/$number
    : >"$call"
    for argument in "$@"; do printf '%s\n' "$argument" >>"$call"; done
}
event() { number=$(next_number); printf '%s %s\n' "$number" "$*" >>"$state/events"; }
last_arg() { for value in "$@"; do :; done; printf '%s\n' "$value"; }
arg_after() {
    wanted=$1; shift; previous=
    for value in "$@"; do
        if [ "$previous" = "$wanted" ]; then printf '%s\n' "$value"; return 0; fi
        previous=$value
    done
    return 1
}
has_arg() { wanted=$1; shift; for value in "$@"; do [ "$value" = "$wanted" ] && return 0; done; return 1; }
has_pair() {
    wanted=$1; expected=$2; shift 2; previous=
    for value in "$@"; do
        [ "$previous" = "$wanted" ] && [ "$value" = "$expected" ] && return 0
        previous=$value
    done
    return 1
}
mount_source() {
    destination=$1; shift; previous=
    for value in "$@"; do
        if [ "$previous" = --mount ]; then
            case "$value" in
                *"dst=$destination"*) source=${value#*src=}; source=${source%%,*}; printf '%s\n' "$source"; return 0 ;;
            esac
        fi
        previous=$value
    done
    return 1
}
mount_spec() {
    destination=$1; shift; previous=
    for value in "$@"; do
        if [ "$previous" = --mount ]; then
            case "$value" in *"dst=$destination"*) printf '%s\n' "$value"; return 0;; esac
        fi
        previous=$value
    done
    return 1
}
mount_count() {
    destination=$1; shift; previous=; count=0
    for value in "$@"; do
        if [ "$previous" = --mount ]; then
            case "$value" in *"dst=$destination"*) count=$((count + 1));; esac
        fi
        previous=$value
    done
    printf '%s\n' "$count"
}
require_hardened() {
    has_arg --read-only "$@" || { echo 'fake docker: missing --read-only' >&2; exit 70; }
    has_pair --cap-drop ALL "$@" || { echo 'fake docker: missing --cap-drop ALL' >&2; exit 72; }
    has_pair --security-opt no-new-privileges "$@" || { echo 'fake docker: missing no-new-privileges' >&2; exit 73; }
    has_pair --tmpfs /tmp "$@" || { echo 'fake docker: missing tmpfs /tmp' >&2; exit 74; }
}
host_mode() {
    stat -c '%a' "$1" 2>/dev/null || stat -f '%Lp' "$1" 2>/dev/null
}
for argument in "$@"; do
    case "$argument" in *docker.sock*) echo 'fake docker: Docker socket token is forbidden' >&2; exit 75;; esac
done
record "$@"

if [ "$1" = volume ] && [ "$2" = create ]; then
    label=$(arg_after --label "$@" || true)
    name=$(last_arg "$@")
    case "$name" in --label|*=*) echo 'fake docker: anonymous volumes are forbidden in Task 4' >&2; exit 76;; esac
    mkdir -p "$state/volumes/$name"
    printf '%s\n' "${label#*=}" >"$state/labels/$name"
    event "volume-create $name ${label:-none}"
    printf '%s\n' "$name"
    exit 0
fi
if [ "$1" = volume ] && [ "$2" = inspect ]; then
    name=$(last_arg "$@")
    [ ! -f "$state/inspect-error-$name" ] || exit 46
    [ -d "$state/volumes/$name" ] || exit 1
    if has_arg --format "$@"; then cat "$state/labels/$name"; else printf '[{}]\n'; fi
    exit 0
fi
if [ "$1" = volume ] && [ "$2" = ls ]; then
    [ "$#" -eq 5 ] && [ "$3" = --quiet ] && [ "$4" = --filter ] || exit 45
    filter=$5
    case "$filter" in name=^*\$) name=${filter#name=^}; name=${name%\$};; *) exit 44;; esac
    [ "${FAKE_VOLUME_LS_FAIL:-0}" != 1 ] || exit 43
    if [ "${FAKE_VOLUME_LS_EXISTING:-0}" = 1 ] || [ -d "$state/volumes/$name" ]; then printf '%s\n' "$name"; fi
    [ "${FAKE_VOLUME_LS_EXTRA:-0}" != 1 ] || printf '%s-extra\n' "$name"
    exit 0
fi
if [ "$1" = volume ] && [ "$2" = rm ]; then
    name=$3
    event "volume-rm $name"
    case "${FAKE_VOLUME_RM_FAIL:-}:$name" in
        exchange:*exchange) exit 47 ;;
        exchange-false-success:*exchange) exit 0 ;;
        exchange-hidden-live:*exchange)
            : >"$state/inspect-error-$name"
            exit 0
            ;;
        source:*source|source-false-success:*source|source-hidden-live:*source)
            source_rm_count=$(cat "$state/source-rm-counter" 2>/dev/null || echo 0)
            source_rm_count=$((source_rm_count + 1))
            printf '%s\n' "$source_rm_count" >"$state/source-rm-counter"
            if [ "$source_rm_count" -ge 2 ]; then
                [ "${FAKE_VOLUME_RM_FAIL:-}" = source-false-success ] && exit 0
                if [ "${FAKE_VOLUME_RM_FAIL:-}" = source-hidden-live ]; then
                    : >"$state/inspect-error-$name"
                    exit 0
                fi
                exit 47
            fi
            ;;
    esac
    rm -rf "$state/volumes/$name" "$state/labels/$name"
    exit 0
fi
if [ "$1" = container ] && [ "$2" = create ]; then
    name=$(arg_after --name "$@")
    label=$(arg_after --label "$@")
    require_hardened "$@"
    has_pair --network none "$@" || { echo 'fake docker: lock must have no network' >&2; exit 71; }
    printf '%s\n' "${label#*=}" >"$state/containers/$name.label"
    event "lock-create $name"
    exit 0
fi
if [ "$1" = container ] && [ "$2" = start ]; then event "lock-start $3"; printf '%s\n' "$3"; exit 0; fi
if [ "$1" = container ] && [ "$2" = inspect ]; then
    container=$(last_arg "$@")
    format=$(arg_after --format "$@")
    if [ "$container" = verifier-dev ]; then
        mismatch=${FAKE_MISMATCH:-}
        if [ -f "$state/after-bridge" ]; then mismatch=${FAKE_AFTER_BRIDGE_MISMATCH:-$mismatch}; fi
        case "$format" in
            '{{.State.Running}}') [ "$mismatch" = stopped ] && printf false || printf true ;;
            '{{.Config.Image}}') [ "$mismatch" = image-reference ] && printf wrong/image:latest || printf '%s' "$image_reference" ;;
            '{{.Image}}') [ "$mismatch" = image-id ] && printf 'sha256:%064d' 0 || printf '%s' "$image_id" ;;
            '{{.Config.User}}') [ "$mismatch" = user ] && printf root || printf coq ;;
            '{{range .Mounts}}{{printf "%s\t%s\n" .Destination .Source}}{{end}}')
                case "$mismatch" in
                    mount) printf '/wrong/repository\t%s\n' "${FAKE_VERIFIER_CHECKOUT:?}" ;;
                    mount-source) printf '%s\t/wrong/verifier\n' "$repository_mount" ;;
                    mount-socket) printf '%s\t%s\n/var/run/docker.sock\t/var/run/docker.sock\n' "$repository_mount" "${FAKE_VERIFIER_CHECKOUT:?}" ;;
                    mount-extra-bind|mount-alias-socket) printf '%s\t%s\n/private/socket-alias\t/var/run\n' "$repository_mount" "${FAKE_VERIFIER_CHECKOUT:?}" ;;
                    mount-volume) printf '%s\t%s\n' "$repository_mount" "${FAKE_VERIFIER_CHECKOUT:?}" ;;
                    mount-malformed) printf '%s\t%s\nmalformed\n' "$repository_mount" "${FAKE_VERIFIER_CHECKOUT:?}" ;;
                    mount-duplicate) printf '%s\t%s\n%s\t%s\n' "$repository_mount" "${FAKE_VERIFIER_CHECKOUT:?}" "$repository_mount" "${FAKE_VERIFIER_CHECKOUT:?}" ;;
                    *) printf '%s\t%s\n' "$repository_mount" "${FAKE_VERIFIER_CHECKOUT:?}" ;;
                esac
                ;;
            '{{range .Mounts}}{{printf "%s\t%s\t%s\n" .Type .Destination .Source}}{{end}}')
                case "$mismatch" in
                    mount) printf 'bind\t/wrong/repository\t%s\n' "${FAKE_VERIFIER_CHECKOUT:?}" ;;
                    mount-source) printf 'bind\t%s\t/wrong/verifier\n' "$repository_mount" ;;
                    mount-socket) printf 'bind\t%s\t%s\nbind\t/var/run/docker.sock\t/var/run/docker.sock\n' "$repository_mount" "${FAKE_VERIFIER_CHECKOUT:?}" ;;
                    mount-extra-bind|mount-alias-socket) printf 'bind\t%s\t%s\nbind\t/private/socket-alias\t/var/run\n' "$repository_mount" "${FAKE_VERIFIER_CHECKOUT:?}" ;;
                    mount-volume) printf 'volume\t%s\t%s\n' "$repository_mount" "${FAKE_VERIFIER_CHECKOUT:?}" ;;
                    mount-malformed) printf 'bind\t%s\t%s\nmalformed\n' "$repository_mount" "${FAKE_VERIFIER_CHECKOUT:?}" ;;
                    mount-duplicate) printf 'bind\t%s\t%s\nbind\t%s\t%s\n' "$repository_mount" "${FAKE_VERIFIER_CHECKOUT:?}" "$repository_mount" "${FAKE_VERIFIER_CHECKOUT:?}" ;;
                    *) printf 'bind\t%s\t%s\n' "$repository_mount" "${FAKE_VERIFIER_CHECKOUT:?}" ;;
                esac
                ;;
            *) echo "fake docker: unexpected verifier inspect format: $format" >&2; exit 77 ;;
        esac
    else
        case "$format" in
            '{{.State.Running}}') printf true ;;
            *Labels*) cat "$state/containers/$container.label" ;;
            *) echo "fake docker: unexpected lock inspect format: $format" >&2; exit 78 ;;
        esac
    fi
    exit 0
fi
if [ "$1" = container ] && [ "$2" = rm ]; then event "lock-rm $(last_arg "$@")"; rm -f "$state/containers/$(last_arg "$@").label"; exit 0; fi
if [ "$1" = exec ]; then
    has_pair --user coq "$@" || { echo 'fake docker: inspection must use coq' >&2; exit 79; }
    has_pair --workdir "$repository_mount" "$@" || { echo 'fake docker: wrong inspection workdir' >&2; exit 80; }
    observed_user=coq
    observed_uid=1000
    observed_gid=1000
    observed_revision=$revision
    observed_coq=8.20.1
    observed_origin=https://github.com/WasmCert/WasmCert-Coq.git
    observed_tag=v2.2.0
    observed_coq_wasm=$coq_wasm_revision
    mismatch=${FAKE_MISMATCH:-}
    if [ -f "$state/after-bridge" ]; then mismatch=${FAKE_AFTER_BRIDGE_MISMATCH:-$mismatch}; fi
    case "$mismatch" in
        provenance-user) observed_user=root ;;
        provenance-uid) observed_uid=0 ;;
        provenance-gid) observed_gid=0 ;;
        provenance-uid-padded) observed_uid=01000 ;;
        provenance-gid-padded) observed_gid=01000 ;;
        provenance-revision) observed_revision=$(printf '%040d' 0) ;;
        coq-version) observed_coq=8.19.3 ;;
        coq-version-patch) observed_coq=8.20.2 ;;
        coq-wasm-origin) observed_origin=https://example.invalid/WasmCert-Coq.git ;;
        coq-wasm-tag) observed_tag=v0.0.0 ;;
        coq-wasm-revision) observed_coq_wasm=$(printf '%040d' 0) ;;
    esac
    printf 'coq_user=%s\ncoq_uid=%s\ncoq_gid=%s\nwasm_verifier_revision=%s\ncoq_version=%s\n' \
        "$observed_user" "$observed_uid" "$observed_gid" "$observed_revision" "$observed_coq"
    [ "$mismatch" = provenance-origin-missing ] || printf 'coq_wasm_origin=%s\n' "$observed_origin"
    printf 'coq_wasm_tag=%s\ncoq_wasm_revision=%s\n' "$observed_tag" "$observed_coq_wasm"
    case "$mismatch" in
        provenance-origin-missing) : ;;
        provenance-origin-duplicate) printf 'coq_wasm_origin=%s\n' "$observed_origin" ;;
        provenance-extra) printf 'unexpected=value\n' ;;
    esac
    [ "$mismatch" != provenance-exit ] || exit 48
    exit 0
fi
if [ "$1" = run ]; then
    require_hardened "$@"
    image=''
    for value in "$@"; do
        case "$value" in rust:1.98-bookworm@sha256:*|busybox:1.37.0@sha256:*) image=$value;; esac
    done
    [ -n "$image" ] || { echo 'fake docker: unpinned helper image' >&2; exit 81; }
    script=$(arg_after -c "$@" || true)
    exchange=$(mount_source /exchange "$@" || true)
    exchange_spec=$(mount_spec /exchange "$@" || true)
    staging=$(mount_source /staging "$@" || true)
    if [ -n "$script" ] && case "$script" in *'tar --exclude=.git'*'rocq-discharge.cargo-lock'*) true;; *) false;; esac; then
        has_pair --network none "$@" || { echo 'fake docker: snapshot helper has network' >&2; exit 71; }
        checkout=$(mount_source /checkout "$@")
        lock_hash=$(sha256sum "$checkout/ci/rocq-discharge.cargo-lock" | cut -d ' ' -f 1)
        printf 'rocq-rust-docker: lane-lock-sha256=%s\n' "$lock_hash"
        printf 'rocq-rust-docker: snapshot-lock-sha256=%s\n' "$lock_hash"
        event task0-snapshot
        exit 0
    fi
    if [ -n "$script" ] && case "$script" in *'fetch --locked --manifest-path /workspace/Cargo.toml'*) true;; *) false;; esac; then
        has_pair --network bridge "$@" || { echo 'fake docker: fetch lost bridge network' >&2; exit 82; }
        [ -z "$exchange" ] || { echo 'fake docker: fetch must not mount exchange' >&2; exit 83; }
        event task0-fetch
        exit 0
    fi
    if [ -n "$script" ] && case "$script" in *'exec "$cargo_path" "$@"'*) true;; *) false;; esac; then
        has_pair --network none "$@" || { echo 'fake docker: offline Rust execution has network' >&2; exit 71; }
        event task0-offline
        source_volume=$(mount_source /workspace "$@")
        case "$source_volume" in inference-rocq-discharge-*-source) :;; *) echo "fake docker: unsafe source volume $source_volume" >&2; exit 84;; esac
        has_pair --mount 'type=volume,src=inference-cargo-home-rust-1.98,dst=/cargo-home' "$@" || true
        if has_arg export "$@"; then
            [ -n "$exchange" ] || { echo 'fake docker: export lacks exchange mount' >&2; exit 85; }
            [ "$(mount_count /exchange "$@")" -eq 1 ] && [ "$exchange_spec" = "type=volume,src=$exchange,dst=/exchange" ] || { echo 'fake docker: export exchange mount is not exact writable' >&2; exit 88; }
            directory=$state/volumes/$exchange
            mkdir -p "$directory/raw"
            printf 'request\n' >"$directory/request.json"
            printf 'prime\n' >"$directory/raw/rocq_prime_bounded_example.v"
            printf 'exists\n' >"$directory/raw/rocq_exists_spec.v"
            printf 'unique\n' >"$directory/raw/rocq_unique_spec.v"
            printf 'narrow\n' >"$directory/raw/spec_narrow_discharge.v"
            printf 'false\n' >"$directory/raw/rocq_false_certificate.v"
            event "export $exchange"
        elif has_arg verify "$@"; then
            [ -n "$exchange" ] || { echo 'fake docker: verify lacks exchange mount' >&2; exit 86; }
            [ "$(mount_count /exchange "$@")" -eq 1 ] && [ "$exchange_spec" = "type=volume,src=$exchange,dst=/exchange,readonly" ] || { echo 'fake docker: verify exchange mount is not exact readonly' >&2; exit 89; }
            directory=$state/volumes/$exchange/receipts
            for case_id in prime-bounded exists unique narrow-domain false-spec; do [ -f "$directory/$case_id.json" ] || exit 87; done
            verify_number=$(cat "$state/verify-counter" 2>/dev/null || echo 0); verify_number=$((verify_number + 1)); printf '%s\n' "$verify_number" >"$state/verify-counter"
            cat "$directory/prime-bounded.json" >"$state/verify-$verify_number"
            printf 'rocq-discharge: result=pass cases=5 proved=11 refuted=1\n'
            if [ "${FAKE_MUTATE_EXCHANGE_ON_VERIFY:-0}" = 1 ]; then
                printf 'verify-time mutation\n' >"$state/volumes/$exchange/request.json"
            fi
            if [ "${FAKE_REPLACE_STAGING_ON_VERIFY:-0}" = 1 ]; then
                staging_path=$(find "${FAKE_TMP_ROOT:?}" -mindepth 1 -maxdepth 1 -type d -name 'inference-rocq-discharge.*' | tail -n 1)
                mv "$staging_path" "$staging_path.original"
                mkdir -m 700 "$staging_path"
                printf 'replacement sentinel\n' >"$staging_path/replacement-sentinel"
                printf '%s\n' "$staging_path" >"$state/replaced-staging-path"
            fi
        elif has_arg test "$@"; then
            if has_arg rocq_dischargeability:: "$@"; then
                printf 'test result: ok. 49 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out\n'
            else
                case "${FAKE_FULL_MODE:-floor}" in
                    empty) : ;;
                    single) printf 'test result: ok. 4000 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out\n' ;;
                    under)
                        printf 'test result: ok. 3069 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out\n'
                        printf 'test result: ok. 4 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out\n'
                        printf 'test result: ok. 0 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out\n'
                        printf 'test result: ok. 1 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out\n'
                        printf 'test result: ok. 0 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out\n'
                        ;;
                    malformed)
                        printf 'test result: ok. many passed; 0 failed; malformed\n'
                        printf 'test result: ok. 3070 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out\n'
                        printf 'test result: ok. 4 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out\n'
                        printf 'test result: ok. 1 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out\n'
                        printf 'test result: ok. 0 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out\n'
                        ;;
                    filtered-only)
                        printf 'test result: ok. 3070 passed; 0 failed; 158 ignored; 0 measured; 1 filtered out\n'
                        printf 'test result: ok. 4 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out\n'
                        printf 'test result: ok. 0 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out\n'
                        printf 'test result: ok. 1 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out\n'
                        printf 'test result: ok. 0 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out\n'
                        ;;
                    failed)
                        printf 'test result: FAILED. 0 passed; 1 failed; 0 ignored; 0 measured; 0 filtered out\n'
                        printf 'test result: ok. 3070 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out\n'
                        printf 'test result: ok. 4 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out\n'
                        printf 'test result: ok. 1 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out\n'
                        printf 'test result: ok. 0 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out\n'
                        ;;
                    floor)
                        printf 'test result: ok. 3070 passed; 0 failed; 158 ignored; 0 measured; 0 filtered out\n'
                        printf 'test result: ok. 4 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out\n'
                        printf 'test result: ok. 0 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out\n'
                        printf 'test result: ok. 1 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out\n'
                        printf 'test result: ok. 0 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out\n'
                        ;;
                esac
            fi
        fi
        exit 0
    fi
    case "$script" in
        *task4-fingerprint*)
            directory=$state/volumes/$exchange
            fingerprint_count=$(cat "$state/fingerprint-counter" 2>/dev/null || echo 0)
            fingerprint_count=$((fingerprint_count + 1))
            printf '%s\n' "$fingerprint_count" >"$state/fingerprint-counter"
            digest=$({
                printf 'request.json\000'; cat "$directory/request.json"
                printf 'raw/rocq_prime_bounded_example.v\000'; cat "$directory/raw/rocq_prime_bounded_example.v"
                printf 'raw/rocq_exists_spec.v\000'; cat "$directory/raw/rocq_exists_spec.v"
                printf 'raw/rocq_unique_spec.v\000'; cat "$directory/raw/rocq_unique_spec.v"
                printf 'raw/spec_narrow_discharge.v\000'; cat "$directory/raw/spec_narrow_discharge.v"
                printf 'raw/rocq_false_certificate.v\000'; cat "$directory/raw/rocq_false_certificate.v"
            } | sha256sum | cut -d ' ' -f 1)
            printf '%s\n' "$digest"
            event fingerprint
            if [ -n "${FAKE_FINGERPRINT_MUTATE_AFTER:-}" ] && [ "$fingerprint_count" -eq "$FAKE_FINGERPRINT_MUTATE_AFTER" ]; then
                printf 'between-check mutation\n' >"$directory/request.json"
            fi
            if [ "${FAKE_FINGERPRINT_MODE:-}" = stale-failure ] && [ "$fingerprint_count" -ge 2 ]; then exit 49; fi
            ;;
        *task4-copy-raw*)
            for raw_file in \
                rocq_prime_bounded_example.v rocq_exists_spec.v rocq_unique_spec.v \
                spec_narrow_discharge.v rocq_false_certificate.v
            do
                printf '%s\n' "$script" | grep -F "cat /exchange/raw/$raw_file > /staging/raw/$raw_file" >/dev/null || exit 64
            done
            if printf '%s\n' "$script" | grep -F 'cp /exchange/raw/' >/dev/null; then exit 64; fi
            if [ "${FAKE_REQUIRE_LINUX_STAGING:-0}" = 1 ]; then
                [ "$(host_mode "$staging")" = 755 ] || exit 61
                [ "$(host_mode "$staging/raw")" = 755 ] || exit 61
                for raw_file in \
                    rocq_prime_bounded_example.v rocq_exists_spec.v rocq_unique_spec.v \
                    spec_narrow_discharge.v rocq_false_certificate.v
                do
                    [ -f "$staging/raw/$raw_file" ] && [ "$(host_mode "$staging/raw/$raw_file")" = 666 ] || exit 61
                done
                : >"$state/linux-copy-raw-modes"
            fi
            mkdir -p "$staging/raw"
            for raw_file in \
                rocq_prime_bounded_example.v rocq_exists_spec.v rocq_unique_spec.v \
                spec_narrow_discharge.v rocq_false_certificate.v
            do
                cat "$state/volumes/$exchange/raw/$raw_file" >"$staging/raw/$raw_file"
            done
            ;;
        *task4-check-staged-raw*)
            basename=$(last_arg "$@")
            if [ "${FAKE_REQUIRE_LINUX_STAGING:-0}" = 1 ]; then
                [ "$(host_mode "$staging")" = 755 ] || exit 62
                [ "$(host_mode "$staging/raw")" = 755 ] || exit 62
                [ "$(host_mode "$staging/raw/$basename")" = 644 ] || exit 62
                : >"$state/linux-check-raw-modes"
            fi
            cmp "$state/volumes/$exchange/raw/$basename" "$staging/raw/$basename"
            ;;
        *task4-remove-receipts*)
            receipt_root=$state/volumes/$exchange/receipts
            regular_count=$(find "$receipt_root" -mindepth 1 -maxdepth 1 -type f | wc -l | tr -d ' ')
            total_count=$(find "$receipt_root" -mindepth 1 -maxdepth 1 -print | wc -l | tr -d ' ')
            if [ "$regular_count" -ne 5 ] || [ "$total_count" -ne 5 ]; then
                find "$receipt_root" -mindepth 1 -maxdepth 1 -print | sort >"$state/rejected-batch-layout"
                exit 43
            fi
            rm -rf "$receipt_root"
            ;;
        *task4-copy-receipts*)
            if [ "${FAKE_REQUIRE_LINUX_STAGING:-0}" = 1 ]; then
                [ "$(host_mode "$staging")" = 755 ] || exit 63
                [ "$(host_mode "$staging/receipts")" = 755 ] || exit 63
                for case_id in prime-bounded exists unique narrow-domain false-spec; do
                    [ "$(host_mode "$staging/receipts/$case_id")" = 755 ] || exit 63
                    [ "$(host_mode "$staging/receipts/$case_id/$case_id.json")" = 644 ] || exit 63
                done
                : >"$state/linux-copy-receipt-modes"
            fi
            mkdir "$state/volumes/$exchange/receipts"
            for case_id in prime-bounded exists unique narrow-domain false-spec; do cp "$staging/receipts/$case_id/$case_id.json" "$state/volumes/$exchange/receipts/"; done
            ;;
        *) : ;;
    esac
    has_pair --network none "$@" || { echo 'fake docker: BusyBox helper has network' >&2; exit 71; }
    event helper-run-finish
    exit 0
fi
echo "fake docker: unexpected command: $*" >&2
exit 90
FAKE_DOCKER
chmod +x "$fake_docker"

cat >"$verifier/ci/discharge/inspect-container.sh" <<'INSPECT'
#!/usr/bin/env sh
exit 99
INSPECT
cat >"$verifier/ci/discharge/docker-bridge.sh" <<'HELPER'
#!/usr/bin/env sh
exit 99
HELPER
cat >"$verifier/ci/discharge/run-docker-batch.sh" <<'BATCH'
#!/usr/bin/env sh
set -eu
[ "$#" -eq 2 ] && [ "$1" = --exchange-volume ] || exit 92
[ "${WASM_VERIFIER_CONTAINER:?}" = verifier-dev ] || exit 93
[ -d "${INFERENCE_WASM_VERIFIER_EVIDENCE_DIR:?}" ] || exit 94
[ "${DOCKER:?}" = "${FAKE_DOCKER_PATH:?}" ] || exit 89
[ -z "${INFERENCE_WASM_VERIFIER_RECEIPT_DIR+x}" ] || [ -z "$INFERENCE_WASM_VERIFIER_RECEIPT_DIR" ] || exit 88
state=${FAKE_STATE:?}; volume=$2
printf 'batch <%s> <%s>\n' "$1" "$2" >>"$state/bridge-calls"
if [ "${FAKE_BRIDGE_FAIL:-}" = batch ]; then
    if [ "${FAKE_BRIDGE_MISSING_LOG:-0}" != 1 ]; then
        verifier_log=$INFERENCE_WASM_VERIFIER_EVIDENCE_DIR/verifier.log
        case "${FAKE_VERIFIER_LOG_ATTACK:-}" in
            mode)
                (umask 077; printf 'private batch proof log\n' >"$verifier_log")
                chmod 644 "$verifier_log"
                ;;
            symlink) ln -s "$state/nonclobber-target" "$verifier_log" ;;
            fifo) mkfifo "$verifier_log" ;;
            hardlink) ln "$state/nonclobber-target" "$verifier_log" ;;
            extra)
                (umask 077; printf 'private batch proof log\n' >"$verifier_log")
                (umask 077; printf 'extra private artifact\n' >"$INFERENCE_WASM_VERIFIER_EVIDENCE_DIR/extra.log")
                ;;
            *) (umask 077; printf 'private batch proof log\n' >"$verifier_log") ;;
        esac
        event_number=$(cat "$state/counter" 2>/dev/null || echo 0)
        event_number=$((event_number + 1))
        printf '%s\n' "$event_number" >"$state/counter"
        printf '%s bridge-evidence-written\n' "$event_number" >>"$state/events"
    fi
    printf 'PRIVATE BATCH OUTPUT MUST BE SUPPRESSED\n' >&2
    exit "${FAKE_BRIDGE_STATUS:-37}"
fi
if [ "${FAKE_BRIDGE_STALE_LOG:-0}" = 1 ]; then
    (umask 077; printf 'unexpected success log\n' >"$INFERENCE_WASM_VERIFIER_EVIDENCE_DIR/verifier.log")
fi
case "${FAKE_ATTACK_CAPTURE:-}" in
    symlink)
        rm -f "$INFERENCE_WASM_VERIFIER_EVIDENCE_DIR/bridge-output.log"
        ln -s "$state/nonclobber-target" "$INFERENCE_WASM_VERIFIER_EVIDENCE_DIR/bridge-output.log"
        ;;
    fifo)
        rm -f "$INFERENCE_WASM_VERIFIER_EVIDENCE_DIR/bridge-output.log"
        mkfifo "$INFERENCE_WASM_VERIFIER_EVIDENCE_DIR/bridge-output.log"
        ;;
    hardlink)
        ln "$INFERENCE_WASM_VERIFIER_EVIDENCE_DIR/bridge-output.log" "$state/capture-hardlink-target"
        ;;
esac
if [ "${FAKE_PLANT_FULL_LOG:-0}" = 1 ]; then
    staging_path=$(find "${FAKE_TMP_ROOT:?}" -mindepth 1 -maxdepth 1 -type d -name 'inference-rocq-discharge.*' | sed -n '1p')
    ln -s "$state/nonclobber-target" "$staging_path/inference-tests.log"
fi
if [ -n "${FAKE_REPLACE_FULL_LOG:-}" ]; then
    full_log=$(find "${FAKE_TMP_ROOT:?}" -mindepth 2 -maxdepth 2 -type f -name 'inference-tests.log.*' | sed -n '1p')
    [ -n "$full_log" ] || exit 87
    case "$FAKE_REPLACE_FULL_LOG" in
        hardlink) ln "$full_log" "$state/full-log-hardlink-target" ;;
        symlink|fifo)
            mv "$full_log" "$full_log.original"
            case "$FAKE_REPLACE_FULL_LOG" in
                symlink) ln -s "$state/nonclobber-target" "$full_log" ;;
                fifo) mkfifo "$full_log" ;;
            esac
            ;;
        *) exit 86 ;;
    esac
fi
mkdir "$state/volumes/$volume/receipts"
for case_id in prime-bounded exists unique narrow-domain false-spec; do printf 'batch-%s\n' "$case_id" >"$state/volumes/$volume/receipts/$case_id.json"; done
if [ "${FAKE_BATCH_EXTRA:-0}" = 1 ]; then mkdir "$state/volumes/$volume/receipts/extra"; fi
if [ "${FAKE_BRIDGE_MUTATE:-0}" = 1 ]; then
    printf 'coherently replaced request\n' >"$state/volumes/$volume/request.json"
    printf 'coherently replaced raw\n' >"$state/volumes/$volume/raw/rocq_prime_bounded_example.v"
fi
if [ "${FAKE_DIRTY_NEXT_BRIDGE:-0}" = 1 ]; then : >"$state/verifier-dirty"; fi
if [ "${FAKE_REPLACE_NEXT_BRIDGE:-0}" = 1 ]; then
    next=${FAKE_VERIFIER_CHECKOUT:?}/ci/discharge/run-docker-case.sh
    cp "$next" "$next.replacement"
    chmod 755 "$next.replacement"
    mv "$next.replacement" "$next"
fi
if [ "${FAKE_REPLACE_SHARED_HELPER:-0}" = 1 ]; then
    helper=${FAKE_VERIFIER_CHECKOUT:?}/ci/discharge/docker-bridge.sh
    cp "$helper" "$helper.replacement"
    chmod 755 "$helper.replacement"
    mv "$helper.replacement" "$helper"
fi
if [ -n "${FAKE_AFTER_BRIDGE_MISMATCH:-}" ]; then : >"$state/after-bridge"; fi
BATCH
cat >"$verifier/ci/discharge/run-docker-case.sh" <<'CASE'
#!/usr/bin/env sh
set -eu
[ "$#" -eq 7 ] && [ "$1" = --protocol ] && [ "$2" = 1 ] && [ "$3" = --wasm-verifier-revision ] && [ "$5" = --case ] || exit 95
[ "$4" = "${FAKE_REVISION:?}" ] || exit 96
case_id=$6; raw=$7; receipt_dir=${INFERENCE_WASM_VERIFIER_RECEIPT_DIR:?}; state=${FAKE_STATE:?}
[ "${DOCKER:?}" = "${FAKE_DOCKER_PATH:?}" ] || exit 89
case "$case_id:$raw" in
    prime-bounded:*/rocq_prime_bounded_example.v|exists:*/rocq_exists_spec.v|unique:*/rocq_unique_spec.v|narrow-domain:*/spec_narrow_discharge.v|false-spec:*/rocq_false_certificate.v) : ;;
    *) exit 97 ;;
esac
[ -f "$raw" ] && [ -d "$receipt_dir" ] && [ -z "$(find "$receipt_dir" -mindepth 1 -print -quit)" ] || exit 98
printf 'case <%s> <%s> <%s> <%s> <%s> <%s> <%s>\n' "$@" >>"$state/bridge-calls"
printf 'receipt-dir <%s>\n' "$receipt_dir" >>"$state/bridge-calls"
if [ "${FAKE_BRIDGE_FAIL:-}" = "case:$case_id" ]; then
    (umask 077; printf 'private single proof log\n' >"${INFERENCE_WASM_VERIFIER_EVIDENCE_DIR:?}/verifier.log")
    printf 'PRIVATE SINGLE OUTPUT MUST BE SUPPRESSED\n' >&2
    exit "${FAKE_BRIDGE_STATUS:-38}"
fi
printf 'single-%s\n' "$case_id" >"$receipt_dir/$case_id.json"
if [ "${FAKE_RECEIPT_HARDLINK:-0}" = 1 ] && [ "$case_id" = prime-bounded ]; then
    ln "$receipt_dir/$case_id.json" "$state/receipt-hardlink-target"
    printf '%s\n' "$(dirname "$(dirname "$receipt_dir")")" >"$state/suspect-staging-path"
fi
if [ "${FAKE_BRIDGE_MUTATE_STAGED:-}" = "$case_id" ]; then printf 'mutated staged raw\n' >"$raw"; fi
if [ "${FAKE_RECEIPT_MODE:-0}" = 1 ]; then chmod 755 "$receipt_dir"; fi
if [ "${FAKE_RECEIPT_EXTRA:-0}" = 1 ]; then mkdir "$receipt_dir/extra"; fi
if [ "${FAKE_REPLACE_RECEIPT_DIR:-0}" = 1 ]; then
    mv "$receipt_dir" "$receipt_dir.original"
    mkdir -m 700 "$receipt_dir"
    printf 'replacement\n' >"$receipt_dir/$case_id.json"
fi
if [ "${FAKE_REPLACE_STAGING:-0}" = 1 ] && [ "$case_id" = prime-bounded ]; then
    staging=$(dirname "$(dirname "$raw")")
    mv "$staging" "$staging.original"
    mkdir -m 700 "$staging"
    mkdir -p -m 700 "$staging/receipts/$case_id"
    printf 'replacement\n' >"$staging/receipts/$case_id/$case_id.json"
    printf 'replacement sentinel\n' >"$staging/replacement-sentinel"
    printf '%s\n' "$staging" >"$state/replaced-staging-path"
fi
if [ "${FAKE_REPLACE_PREVIOUS_RECEIPT:-0}" = 1 ] && [ "$case_id" = exists ]; then
    previous=$(dirname "$receipt_dir")/prime-bounded/prime-bounded.json
    printf '%s\n' "$(dirname "$(dirname "$receipt_dir")")" >"$state/suspect-staging-path"
    mv "$previous" "$state/replaced-prime-receipt"
    printf 'replacement with the same safe mode\n' >"$previous"
fi
if [ "${FAKE_COHERENT_MUTATION:-0}" = 1 ] && [ "$case_id" = prime-bounded ]; then
    exchange=$(find "$state/volumes" -mindepth 1 -maxdepth 1 -type d -name 'inference-rocq-discharge-*-exchange' | sed -n '1p')
    printf 'coherent mutation\n' >"$exchange/raw/rocq_prime_bounded_example.v"
    printf 'coherent mutation\n' >"$raw"
    staging=$(dirname "$(dirname "$raw")")
    printf '%064d\n' 0 >"$staging/exchange.identity"
fi
if [ "${FAKE_DIRTY_AFTER_CASE:-}" = "$case_id" ]; then : >"$state/verifier-dirty"; fi
if [ -n "${FAKE_AFTER_BRIDGE_MISMATCH:-}" ] && [ "$case_id" = prime-bounded ]; then : >"$state/after-bridge"; fi
CASE
chmod +x "$verifier/ci/discharge/"*.sh

reset_state() {
    rm -rf "$state"
    find "$test_tmp" -mindepth 1 -maxdepth 1 -type d -name 'inference-rocq-discharge.*' -exec rm -rf -- {} +
    mkdir -p "$state/calls" "$state/volumes" "$state/labels" "$state/containers"
    write_container_pin
}
run_wrapper() {
    FAKE_STATE=$state \
    FAKE_REVISION=$revision \
    FAKE_COQ_WASM_REVISION=$coq_wasm_revision \
    FAKE_IMAGE_REFERENCE=$image_reference \
    FAKE_IMAGE_ID=$image_id \
    FAKE_REPOSITORY_MOUNT=$repository_mount \
    FAKE_VERIFIER_CHECKOUT=$verifier \
    DOCKER="$fake_docker" GIT="$fake_git" \
        "$fixture/ci/rocq-discharge-docker.sh" "$@"
}
proxy_token=123456
proxy_source=inference-rocq-discharge-$proxy_token-source
proxy_exchange=inference-rocq-discharge-$proxy_token-exchange
proxy_owner=task4-$proxy_token
proxy_runtime=$work/proxy-runtime
proxy_owner_file=$proxy_runtime/source.owner
mkdir -m 700 "$proxy_runtime"
proxy_call() {
    FAKE_STATE=$state \
    FAKE_REVISION=$revision \
    FAKE_COQ_WASM_REVISION=$coq_wasm_revision \
    FAKE_IMAGE_REFERENCE=$image_reference \
    FAKE_IMAGE_ID=$image_id \
    FAKE_REPOSITORY_MOUNT=$repository_mount \
    FAKE_VERIFIER_CHECKOUT=$verifier \
    INFERENCE_ROCQ_DOCKER_PROXY_MODE=1 \
    INFERENCE_ROCQ_DOCKER_PROXY_REAL_DOCKER="${PROXY_REAL_OVERRIDE:-$fake_docker}" \
    INFERENCE_ROCQ_DOCKER_PROXY_SOURCE_VOLUME="${PROXY_SOURCE_OVERRIDE:-$proxy_source}" \
    INFERENCE_ROCQ_DOCKER_PROXY_EXCHANGE_VOLUME="${PROXY_EXCHANGE_OVERRIDE:-$proxy_exchange}" \
    INFERENCE_ROCQ_DOCKER_PROXY_RUN_OWNER="${PROXY_OWNER_OVERRIDE:-$proxy_owner}" \
    INFERENCE_ROCQ_DOCKER_PROXY_SOURCE_OWNER_FILE="$proxy_owner_file" \
        "$fixture/ci/rocq-discharge-docker.sh" "$@"
}
prepare_proxy_state() {
    reset_state
    mkdir -p "$state/volumes/$proxy_source" "$state/volumes/$proxy_exchange"
    printf '%s\n' "$proxy_owner" >"$state/labels/$proxy_exchange"
    printf '%s\n' 24680 >"$state/labels/$proxy_source"
    printf '%s\n' 24680 >"$proxy_owner_file"
    chmod 600 "$proxy_owner_file"
}
expect_failure() {
    label=$1; shift
    if "$@" >"$work/$label.out" 2>"$work/$label.err"; then
        echo "self-test: $label unexpectedly succeeded" >&2
        exit 1
    fi
}
expect_status() {
    expected=$1; label=$2; shift 2
    set +e
    "$@" >"$work/$label.out" 2>"$work/$label.err"
    actual=$?
    set -e
    [ "$actual" -eq "$expected" ] || {
        echo "self-test: $label returned $actual, expected $expected" >&2
        sed -n '1,20p' "$work/$label.err" >&2
        exit 1
    }
    case "$label" in
        proxy-*) [ -z "$(find "$state/calls" -mindepth 1 -type f -print -quit)" ] || { echo "self-test: $label reached real Docker" >&2; exit 1; } ;;
    esac
}
assert_no_match() {
    match_kind=$1
    pattern=$2
    path=$3
    set +e
    case "$match_kind" in
        fixed) grep -F -q -- "$pattern" "$path" ;;
        regex) grep -q -- "$pattern" "$path" ;;
        *) echo "self-test: invalid negative-match kind $match_kind" >&2; exit 1 ;;
    esac
    match_status=$?
    set -e
    case "$match_status" in
        1) : ;;
        0) echo "self-test: unexpected match $pattern in $path" >&2; exit 1 ;;
        *) echo "self-test: grep failed for $path (status $match_status)" >&2; exit 1 ;;
    esac
}
base_args="--wasm-verifier $verifier --container verifier-dev"

lock_script='trap "exit 0" TERM INT; sleep 3600'
source_empty_script='
        set -eu
        if find /snapshot -mindepth 1 -maxdepth 1 -print -quit | grep . >/dev/null; then
            exit 46
        fi
    '
fetch_script='
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
cargo_script='
        set -eu
        export RUSTUP_TOOLCHAIN=1.98.0-$(uname -m)-unknown-linux-gnu
        cargo_path=$(rustup which cargo)
        exec "$cargo_path" "$@"
    '

# The private Docker proxy is a closed Task-0 protocol, not a permissive flag filter.
prepare_proxy_state
expect_status 2 proxy-lock-extra proxy_call container create --name inference-cargo-target-rust-1.98-lock --label "$owner_label=24680" --read-only --network none --cap-drop ALL --security-opt no-new-privileges --tmpfs /tmp "$busybox_image" sh -c "$lock_script" --privileged
prepare_proxy_state
expect_status 2 proxy-lock-reordered proxy_call container create --name inference-cargo-target-rust-1.98-lock --label "$owner_label=24680" --network none --read-only --cap-drop ALL --security-opt no-new-privileges --tmpfs /tmp "$busybox_image" sh -c "$lock_script"
prepare_proxy_state
expect_status 2 proxy-lock-duplicate-network proxy_call container create --name inference-cargo-target-rust-1.98-lock --label "$owner_label=24680" --read-only --network none --cap-drop ALL --security-opt no-new-privileges --tmpfs /tmp "$busybox_image" sh -c "$lock_script" --network host
prepare_proxy_state
expect_status 2 proxy-lock-altered-script proxy_call container create --name inference-cargo-target-rust-1.98-lock --label "$owner_label=24680" --read-only --network none --cap-drop ALL --security-opt no-new-privileges --tmpfs /tmp "$busybox_image" sh -c "$lock_script; : altered"
prepare_proxy_state
expect_status 2 proxy-inspect-format proxy_call container inspect --format '{{.State.Running}} trailing' inference-cargo-target-rust-1.98-lock
prepare_proxy_state
expect_status 2 proxy-helper-altered proxy_call run --rm --read-only --network none --cap-drop ALL --security-opt no-new-privileges --tmpfs /tmp --mount "type=volume,src=$proxy_source,dst=/snapshot" "$busybox_image" sh -c "$source_empty_script
# marker-preserving alteration"
prepare_proxy_state
expect_status 2 proxy-fetch-altered proxy_call run --rm --read-only --network bridge --cap-drop ALL --security-opt no-new-privileges --tmpfs /tmp --mount "type=volume,src=$proxy_source,dst=/workspace,readonly" --mount type=volume,src=inference-cargo-home-rust-1.98,dst=/cargo-home --mount type=volume,src=inference-cargo-target-rust-1.98,dst=/cargo-target -e CARGO_HOME=/cargo-home -e CARGO_TARGET_DIR=/cargo-target "$rust_image" sh -c "$fetch_script
# marker-preserving alteration"
prepare_proxy_state
expect_status 2 proxy-export-altered-script proxy_call run --rm --read-only --network none --cap-drop ALL --security-opt no-new-privileges --tmpfs /tmp --mount "type=volume,src=$proxy_source,dst=/workspace,readonly" --mount type=volume,src=inference-cargo-home-rust-1.98,dst=/cargo-home --mount type=volume,src=inference-cargo-target-rust-1.98,dst=/cargo-target -e CARGO_HOME=/cargo-home -e CARGO_TARGET_DIR=/cargo-target "$rust_image" sh -c "$cargo_script
# marker-preserving alteration" sh run --manifest-path /workspace/Cargo.toml --offline --locked -p inference-tests --bin rocq-discharge -- export --exchange /exchange
prepare_proxy_state
expect_status 2 proxy-focused-extra-filter proxy_call run --rm --read-only --network none --cap-drop ALL --security-opt no-new-privileges --tmpfs /tmp --mount "type=volume,src=$proxy_source,dst=/workspace,readonly" --mount type=volume,src=inference-cargo-home-rust-1.98,dst=/cargo-home --mount type=volume,src=inference-cargo-target-rust-1.98,dst=/cargo-target -e CARGO_HOME=/cargo-home -e CARGO_TARGET_DIR=/cargo-target "$rust_image" sh -c "$cargo_script" sh test --manifest-path /workspace/Cargo.toml --offline --locked --color never -p inference-tests rocq_dischargeability:: extra-filter -- --test-threads=1
prepare_proxy_state
expect_status 2 proxy-focused-extra-mount proxy_call run --rm --read-only --network none --cap-drop ALL --security-opt no-new-privileges --tmpfs /tmp --mount "type=volume,src=$proxy_source,dst=/workspace,readonly" --mount type=volume,src=inference-cargo-home-rust-1.98,dst=/cargo-home --mount type=volume,src=inference-cargo-target-rust-1.98,dst=/cargo-target --mount type=bind,src=/tmp,dst=/host -e CARGO_HOME=/cargo-home -e CARGO_TARGET_DIR=/cargo-target "$rust_image" sh -c "$cargo_script" sh test --manifest-path /workspace/Cargo.toml --offline --locked --color never -p inference-tests rocq_dischargeability:: -- --test-threads=1
prepare_proxy_state
expect_status 2 proxy-focused-entrypoint proxy_call run --rm --read-only --network none --cap-drop ALL --security-opt no-new-privileges --tmpfs /tmp --entrypoint sh --mount "type=volume,src=$proxy_source,dst=/workspace,readonly" --mount type=volume,src=inference-cargo-home-rust-1.98,dst=/cargo-home --mount type=volume,src=inference-cargo-target-rust-1.98,dst=/cargo-target -e CARGO_HOME=/cargo-home -e CARGO_TARGET_DIR=/cargo-target "$rust_image" sh -c "$cargo_script" sh test --manifest-path /workspace/Cargo.toml --offline --locked --color never -p inference-tests rocq_dischargeability:: -- --test-threads=1
prepare_proxy_state
expect_status 2 proxy-focused-network-host proxy_call run --rm --read-only --network none --network host --cap-drop ALL --security-opt no-new-privileges --tmpfs /tmp --mount "type=volume,src=$proxy_source,dst=/workspace,readonly" --mount type=volume,src=inference-cargo-home-rust-1.98,dst=/cargo-home --mount type=volume,src=inference-cargo-target-rust-1.98,dst=/cargo-target -e CARGO_HOME=/cargo-home -e CARGO_TARGET_DIR=/cargo-target "$rust_image" sh -c "$cargo_script" sh test --manifest-path /workspace/Cargo.toml --offline --locked --color never -p inference-tests rocq_dischargeability:: -- --test-threads=1
prepare_proxy_state
expect_status 2 proxy-export-reversed-locks proxy_call run --rm --read-only --network none --cap-drop ALL --security-opt no-new-privileges --tmpfs /tmp --mount "type=volume,src=$proxy_source,dst=/workspace,readonly" --mount type=volume,src=inference-cargo-home-rust-1.98,dst=/cargo-home --mount type=volume,src=inference-cargo-target-rust-1.98,dst=/cargo-target -e CARGO_HOME=/cargo-home -e CARGO_TARGET_DIR=/cargo-target "$rust_image" sh -c "$cargo_script" sh run --manifest-path /workspace/Cargo.toml --locked --offline -p inference-tests --bin rocq-discharge -- export --exchange /exchange
prepare_proxy_state
PROXY_SOURCE_OVERRIDE=inference-rocq-discharge-654321-source expect_status 2 proxy-cross-token proxy_call volume create inference-cargo-home-rust-1.98
prepare_proxy_state
PROXY_OWNER_OVERRIDE=task4-123456junk expect_status 2 proxy-bad-owner-token proxy_call volume create inference-cargo-home-rust-1.98
prepare_proxy_state
expect_status 2 proxy-bad-source-owner proxy_call volume create --label "$owner_label=24680bad"
reset_state
rm -f "$proxy_owner_file"
FAKE_VOLUME_LS_FAIL=1
export FAKE_VOLUME_LS_FAIL
expect_failure proxy-source-list-failure proxy_call volume create --label "$owner_label=24680"
unset FAKE_VOLUME_LS_FAIL
test ! -d "$state/volumes/$proxy_source"
prepare_proxy_state
PROXY_REAL_OVERRIDE="$fixture/ci/rocq-discharge-docker.sh" expect_status 2 proxy-recursion proxy_call volume create inference-cargo-home-rust-1.98
prepare_proxy_state
PROXY_REAL_OVERRIDE="$fake_bin/../fake tools/docker tool" expect_status 2 proxy-noncanonical-docker proxy_call volume create inference-cargo-home-rust-1.98

reset_state
# shellcheck disable=SC2086
run_wrapper $base_args >"$work/default.out"
grep -F 'batch <--exchange-volume>' "$state/bridge-calls" >/dev/null
[ "$(grep -c '^case ' "$state/bridge-calls")" -eq 5 ]
observed_case_order=$(sed -n 's/^case <[^>]*> <[^>]*> <[^>]*> <[^>]*> <[^>]*> <\([^>]*\)> .*/\1/p' "$state/bridge-calls")
expected_case_order='prime-bounded
exists
unique
narrow-domain
false-spec'
[ "$observed_case_order" = "$expected_case_order" ]
observed_basename_order=$(sed -n 's|^case .* <.*/\([^/>]*\.v\)>$|\1|p' "$state/bridge-calls")
expected_basename_order='rocq_prime_bounded_example.v
rocq_exists_spec.v
rocq_unique_spec.v
spec_narrow_discharge.v
rocq_false_certificate.v'
[ "$observed_basename_order" = "$expected_basename_order" ]
grep -F 'batch-prime-bounded' "$state/verify-1" >/dev/null
grep -F 'single-prime-bounded' "$state/verify-2" >/dev/null
[ "$(sed -n 's/^receipt-dir <\([^>]*\)>$/\1/p' "$state/bridge-calls" | sort -u | wc -l | tr -d ' ')" -eq 5 ]
[ "$(grep -c 'lock-create inference-cargo-target-rust-1.98-lock' "$state/events")" -eq 3 ]
[ "$(grep -c 'lock-rm inference-cargo-target-rust-1.98-lock' "$state/events")" -eq 3 ]
[ "$(grep -c ' task0-snapshot$' "$state/events")" -eq 3 ]
[ "$(grep -c ' task0-fetch$' "$state/events")" -eq 3 ]
expected_lock_hash=$(sha256sum "$fixture/ci/rocq-discharge.cargo-lock" | cut -d ' ' -f 1)
[ "$(grep -c "rocq-rust-docker: lane-lock-sha256=$expected_lock_hash" "$work/default.out")" -eq 3 ]
[ "$(grep -c "rocq-rust-docker: snapshot-lock-sha256=$expected_lock_hash" "$work/default.out")" -eq 3 ]
awk '
    / lock-rm inference-cargo-target-rust-1.98-lock$/ { lock_line=NR }
    / volume-rm inference-rocq-discharge-.*-source$/ {
        if (!lock_line || lock_line >= NR) exit 1
        pairs++
        lock_line=0
    }
    END { if (pairs != 3) exit 1 }
' "$state/events"
test -d "$state/volumes/inference-cargo-home-rust-1.98"
test -d "$state/volumes/inference-cargo-target-rust-1.98"
test -z "$(find "$state/volumes" -mindepth 1 -maxdepth 1 -type d -name 'inference-rocq-discharge-*' -print -quit)"
transient_name=$(sed -n 's/^[0-9][0-9]* volume-create \(inference-rocq-discharge-.*-exchange\) .*/\1/p' "$state/events" | sed -n '1p')
transient_token=${transient_name#inference-rocq-discharge-}; transient_token=${transient_token%-exchange}
[ "${#transient_token}" -eq 6 ]
case "$transient_token" in ''|*[!A-Za-z0-9]*) echo 'self-test: transient volume token was not exact' >&2; exit 1;; esac
grep -R -l -F 'rust:1.98-bookworm@sha256:82150a52ec202c1b14d7817e14516c392bb7f5cfebd88f1ed531cb37ebd39922' "$state/calls" >/dev/null
grep -R -l -F 'busybox:1.37.0@sha256:9db7b59979c38555a39def84a31fb98b5296952f9e3afd4f6f11f05b07adfab0' "$state/calls" >/dev/null
if grep -R -l 'docker\.sock' "$state/calls" >/dev/null 2>&1; then echo 'self-test: Docker socket reached a container argv' >&2; exit 1; fi

for adapter in batch single both; do
    reset_state
    # shellcheck disable=SC2086
    run_wrapper $base_args --adapter "$adapter" >"$work/$adapter.out"
    case "$adapter" in
        batch) [ "$(grep -c '^batch ' "$state/bridge-calls")" -eq 1 ]; assert_no_match regex '^case ' "$state/bridge-calls" ;;
        single) assert_no_match regex '^batch ' "$state/bridge-calls"; [ "$(grep -c '^case ' "$state/bridge-calls")" -eq 5 ] ;;
        both) [ "$(grep -c '^batch ' "$state/bridge-calls")" -eq 1 ]; [ "$(grep -c '^case ' "$state/bridge-calls")" -eq 5 ] ;;
    esac
done

reset_state
FAKE_REQUIRE_LINUX_STAGING=1
export FAKE_REQUIRE_LINUX_STAGING
# shellcheck disable=SC2086
run_wrapper $base_args --adapter single >"$work/linux-staging.out"
unset FAKE_REQUIRE_LINUX_STAGING
test -f "$state/linux-copy-raw-modes"
test -f "$state/linux-check-raw-modes"
test -f "$state/linux-copy-receipt-modes"

# The shared verifier bridge helper is mandatory and identity-checked like every public adapter.
helper_path=$verifier/ci/discharge/docker-bridge.sh
helper_pristine=$work/docker-bridge.pristine
cp "$helper_path" "$helper_pristine"
chmod 755 "$helper_pristine"
reset_state
mv "$helper_path" "$helper_path.missing"
# shellcheck disable=SC2086
expect_failure helper-missing run_wrapper $base_args --adapter batch
mv "$helper_path.missing" "$helper_path"
test ! -e "$state/bridge-calls"

reset_state
ln "$helper_path" "$state/helper-hardlink-target"
# shellcheck disable=SC2086
expect_failure helper-hardlink run_wrapper $base_args --adapter batch
rm "$state/helper-hardlink-target"
test ! -e "$state/bridge-calls"

reset_state
FAKE_REPLACE_SHARED_HELPER=1
export FAKE_REPLACE_SHARED_HELPER
# shellcheck disable=SC2086
expect_failure helper-replaced-after-bridge run_wrapper $base_args --adapter batch
unset FAKE_REPLACE_SHARED_HELPER
rm "$helper_path"
cp "$helper_pristine" "$helper_path"
chmod 755 "$helper_path"
assert_no_match fixed 'rocq-discharge-docker: result=pass' "$work/helper-replaced-after-bridge.out"

# Bridge-owned environment values never override the orchestrator's exact batch/single receipt contract.
reset_state
INFERENCE_WASM_VERIFIER_RECEIPT_DIR=$work/ambient-receipts
export INFERENCE_WASM_VERIFIER_RECEIPT_DIR
# shellcheck disable=SC2086
run_wrapper $base_args --adapter both >"$work/ambient-receipt.out"
unset INFERENCE_WASM_VERIFIER_RECEIPT_DIR
[ "$(grep -c '^case ' "$state/bridge-calls")" -eq 5 ]

# Every bridge boundary is revalidated after the bridge, including mutations to the next adapter.
for boundary_attack in dirty-next replace-next container image-provenance final-single-dirty; do
    reset_state
    case "$boundary_attack" in
        dirty-next) FAKE_DIRTY_NEXT_BRIDGE=1; export FAKE_DIRTY_NEXT_BRIDGE ;;
        replace-next) FAKE_REPLACE_NEXT_BRIDGE=1; export FAKE_REPLACE_NEXT_BRIDGE ;;
        container) FAKE_AFTER_BRIDGE_MISMATCH=image-id; export FAKE_AFTER_BRIDGE_MISMATCH ;;
        image-provenance) FAKE_AFTER_BRIDGE_MISMATCH=coq-wasm-origin; export FAKE_AFTER_BRIDGE_MISMATCH ;;
        final-single-dirty) FAKE_DIRTY_AFTER_CASE=false-spec; export FAKE_DIRTY_AFTER_CASE ;;
    esac
    # shellcheck disable=SC2086
    expect_failure "boundary-$boundary_attack" run_wrapper $base_args --adapter both
    assert_no_match fixed 'rocq-discharge-docker: result=pass' "$work/boundary-$boundary_attack.out"
    unset FAKE_DIRTY_NEXT_BRIDGE FAKE_REPLACE_NEXT_BRIDGE FAKE_AFTER_BRIDGE_MISMATCH FAKE_DIRTY_AFTER_CASE 2>/dev/null || true
done

reset_state
# shellcheck disable=SC2086
run_wrapper $base_args --full >"$work/full.out"
grep -F 'rocq-discharge-docker: full crate=inference-tests result-lines=5 passed=3075 floor=3075' "$work/full.out" >/dev/null

reset_state
printf 'do not clobber\n' >"$state/nonclobber-target"
FAKE_PLANT_FULL_LOG=1
export FAKE_PLANT_FULL_LOG
# shellcheck disable=SC2086
run_wrapper $base_args --full >"$work/full-planted-path.out"
unset FAKE_PLANT_FULL_LOG
[ "$(cat "$state/nonclobber-target")" = 'do not clobber' ]

for full_log_attack in symlink fifo hardlink; do
    reset_state
    full_attack_tmp=$test_tmp/full-$full_log_attack
    mkdir -m 700 "$full_attack_tmp"
    printf 'do not clobber\n' >"$state/nonclobber-target"
    chmod 600 "$state/nonclobber-target"
    saved_tmp=$TMPDIR
    saved_fake_tmp_root=$FAKE_TMP_ROOT
    TMPDIR=$full_attack_tmp
    FAKE_TMP_ROOT=$full_attack_tmp
    FAKE_REPLACE_FULL_LOG=$full_log_attack
    export TMPDIR FAKE_TMP_ROOT FAKE_REPLACE_FULL_LOG
    # shellcheck disable=SC2086
    expect_failure "full-log-replaced-$full_log_attack" run_wrapper $base_args --full
    unset FAKE_REPLACE_FULL_LOG
    TMPDIR=$saved_tmp
    FAKE_TMP_ROOT=$saved_fake_tmp_root
    export TMPDIR FAKE_TMP_ROOT
    grep -F 'phase=full-log-identity' "$work/full-log-replaced-$full_log_attack.err" >/dev/null
    [ "$(cat "$state/nonclobber-target")" = 'do not clobber' ]
    [ "$(find "$full_attack_tmp" -mindepth 1 -maxdepth 1 -type d -name 'inference-rocq-discharge.*' | wc -l | tr -d ' ')" -eq 1 ]
    if [ "$full_log_attack" = hardlink ]; then [ -f "$state/full-log-hardlink-target" ]; fi
    assert_no_match fixed 'rocq-discharge-docker: result=pass' "$work/full-log-replaced-$full_log_attack.out"
done

for floor_mode in empty single under malformed filtered-only failed; do
    reset_state
    # shellcheck disable=SC2086
    expect_failure "floor-$floor_mode" env FAKE_FULL_MODE="$floor_mode" FAKE_STATE="$state" FAKE_REVISION="$revision" FAKE_COQ_WASM_REVISION="$coq_wasm_revision" FAKE_IMAGE_REFERENCE="$image_reference" FAKE_IMAGE_ID="$image_id" FAKE_REPOSITORY_MOUNT="$repository_mount" FAKE_VERIFIER_CHECKOUT="$verifier" DOCKER="$fake_docker" GIT="$fake_git" "$fixture/ci/rocq-discharge-docker.sh" --wasm-verifier "$verifier" --container verifier-dev --full
    grep -F 'phase=full-test-floor' "$work/floor-$floor_mode.err" >/dev/null
done

for mismatch in stopped image-reference image-id user mount mount-source mount-socket mount-extra-bind mount-alias-socket mount-volume mount-malformed mount-duplicate provenance-user provenance-uid provenance-gid provenance-uid-padded provenance-gid-padded provenance-revision provenance-exit provenance-origin-missing provenance-origin-duplicate provenance-extra coq-version coq-version-patch coq-wasm-origin coq-wasm-tag coq-wasm-revision checkout-revision dirty-checkout skip-index assume-index contract-blob; do
    reset_state
    expect_failure "mismatch-$mismatch" env FAKE_MISMATCH="$mismatch" FAKE_STATE="$state" FAKE_REVISION="$revision" FAKE_COQ_WASM_REVISION="$coq_wasm_revision" FAKE_IMAGE_REFERENCE="$image_reference" FAKE_IMAGE_ID="$image_id" FAKE_REPOSITORY_MOUNT="$repository_mount" FAKE_VERIFIER_CHECKOUT="$verifier" DOCKER="$fake_docker" GIT="$fake_git" "$fixture/ci/rocq-discharge-docker.sh" --wasm-verifier "$verifier" --container verifier-dev
    test ! -e "$state/bridge-calls"
done

for pin_variant in unknown malformed duplicate missing-comma trailing-comma reordered alternate-patch; do
    reset_state
    case "$pin_variant" in
        unknown) write_container_pin_unknown ;;
        malformed) write_container_pin_malformed ;;
        duplicate) write_container_pin_duplicate ;;
        missing-comma) write_container_pin_missing_comma ;;
        trailing-comma) write_container_pin_trailing_comma ;;
        reordered) write_container_pin_reordered ;;
        alternate-patch) write_container_pin_alternate_patch ;;
    esac
    # shellcheck disable=SC2086
    expect_failure "pin-$pin_variant" run_wrapper $base_args
    test ! -e "$state/bridge-calls"
done

reset_state
saved_image_id=$image_id
image_id=sha256:gggggggggggggggggggggggggggggggggggggggggggggggggggggggggggggggg
write_container_pin
image_id=$saved_image_id
# shellcheck disable=SC2086
expect_failure pin-nonhex-image run_wrapper $base_args
test ! -e "$state/bridge-calls"

reset_state
expect_status 37 bridge-failure env FAKE_BRIDGE_FAIL=batch FAKE_BRIDGE_STATUS=37 FAKE_STATE="$state" FAKE_REVISION="$revision" FAKE_COQ_WASM_REVISION="$coq_wasm_revision" FAKE_IMAGE_REFERENCE="$image_reference" FAKE_IMAGE_ID="$image_id" FAKE_REPOSITORY_MOUNT="$repository_mount" FAKE_VERIFIER_CHECKOUT="$verifier" DOCKER="$fake_docker" GIT="$fake_git" "$fixture/ci/rocq-discharge-docker.sh" --wasm-verifier "$verifier" --container verifier-dev
[ "$(wc -l <"$work/bridge-failure.err" | tr -d ' ')" -eq 1 ]
assert_no_match fixed 'PRIVATE' "$work/bridge-failure.err"
evidence=$(sed -n 's/^rocq-discharge-docker: phase=batch evidence=//p' "$work/bridge-failure.err")
[ -d "$evidence" ] && [ ! -L "$evidence" ] && [ -f "$evidence/verifier.log" ]
evidence_line=$(grep -n 'bridge-evidence-written' "$state/events" | cut -d: -f1)
exchange_remove_line=$(grep -n 'volume-rm inference-rocq-discharge-.*-exchange' "$state/events" | cut -d: -f1)
[ "$evidence_line" -ge 1 ] && [ "$exchange_remove_line" -gt "$evidence_line" ]
rm -rf "$evidence"

reset_state
expect_status 55 single-bridge-failure env FAKE_BRIDGE_FAIL=case:prime-bounded FAKE_BRIDGE_STATUS=55 FAKE_STATE="$state" FAKE_REVISION="$revision" FAKE_COQ_WASM_REVISION="$coq_wasm_revision" FAKE_IMAGE_REFERENCE="$image_reference" FAKE_IMAGE_ID="$image_id" FAKE_REPOSITORY_MOUNT="$repository_mount" FAKE_VERIFIER_CHECKOUT="$verifier" DOCKER="$fake_docker" GIT="$fake_git" "$fixture/ci/rocq-discharge-docker.sh" --wasm-verifier "$verifier" --container verifier-dev --adapter single
[ "$(wc -l <"$work/single-bridge-failure.err" | tr -d ' ')" -eq 1 ]
assert_no_match fixed 'PRIVATE' "$work/single-bridge-failure.err"
single_evidence=$(sed -n 's/^rocq-discharge-docker: phase=single-prime-bounded evidence=//p' "$work/single-bridge-failure.err")
[ -d "$single_evidence" ] && [ -f "$single_evidence/verifier.log" ]
rm -rf "$single_evidence"

reset_state
expect_failure bridge-missing-log env FAKE_BRIDGE_FAIL=batch FAKE_BRIDGE_MISSING_LOG=1 FAKE_STATE="$state" FAKE_REVISION="$revision" FAKE_COQ_WASM_REVISION="$coq_wasm_revision" FAKE_IMAGE_REFERENCE="$image_reference" FAKE_IMAGE_ID="$image_id" FAKE_REPOSITORY_MOUNT="$repository_mount" FAKE_VERIFIER_CHECKOUT="$verifier" DOCKER="$fake_docker" GIT="$fake_git" "$fixture/ci/rocq-discharge-docker.sh" --wasm-verifier "$verifier" --container verifier-dev
grep -F 'phase=evidence-contract' "$work/bridge-missing-log.err" >/dev/null
assert_no_match fixed 'evidence=' "$work/bridge-missing-log.err"

for verifier_log_attack in mode symlink fifo hardlink extra; do
    reset_state
    evidence_attack_tmp=$test_tmp/evidence-$verifier_log_attack
    mkdir -m 700 "$evidence_attack_tmp"
    printf 'unrelated private inode\n' >"$state/nonclobber-target"
    chmod 600 "$state/nonclobber-target"
    saved_tmp=$TMPDIR
    TMPDIR=$evidence_attack_tmp
    FAKE_BRIDGE_FAIL=batch
    FAKE_VERIFIER_LOG_ATTACK=$verifier_log_attack
    export TMPDIR FAKE_BRIDGE_FAIL FAKE_VERIFIER_LOG_ATTACK
    expect_failure "verifier-log-$verifier_log_attack" run_wrapper $base_args --adapter batch
    unset FAKE_BRIDGE_FAIL FAKE_VERIFIER_LOG_ATTACK
    TMPDIR=$saved_tmp
    export TMPDIR
    grep -F 'phase=evidence-contract' "$work/verifier-log-$verifier_log_attack.err" >/dev/null
    assert_no_match fixed 'PRIVATE' "$work/verifier-log-$verifier_log_attack.err"
    assert_no_match fixed ' evidence=' "$work/verifier-log-$verifier_log_attack.err"
    assert_no_match fixed 'rocq-discharge-docker: result=pass' "$work/verifier-log-$verifier_log_attack.out"
    [ "$(cat "$state/nonclobber-target")" = 'unrelated private inode' ]
    if [ "$verifier_log_attack" = hardlink ]; then
        [ "$(find "$evidence_attack_tmp" -mindepth 1 -maxdepth 1 -type d -name 'wasm-verifier-discharge-evidence.*' | wc -l | tr -d ' ')" -eq 1 ]
    fi
done

reset_state
expect_failure bridge-success-log env FAKE_BRIDGE_STALE_LOG=1 FAKE_STATE="$state" FAKE_REVISION="$revision" FAKE_COQ_WASM_REVISION="$coq_wasm_revision" FAKE_IMAGE_REFERENCE="$image_reference" FAKE_IMAGE_ID="$image_id" FAKE_REPOSITORY_MOUNT="$repository_mount" FAKE_VERIFIER_CHECKOUT="$verifier" DOCKER="$fake_docker" GIT="$fake_git" "$fixture/ci/rocq-discharge-docker.sh" --wasm-verifier "$verifier" --container verifier-dev --adapter batch
grep -F 'phase=evidence-contract' "$work/bridge-success-log.err" >/dev/null
assert_no_match fixed 'evidence=' "$work/bridge-success-log.err"

for capture_attack in symlink fifo hardlink; do
    reset_state
    capture_attack_tmp=$test_tmp/capture-$capture_attack
    mkdir -m 700 "$capture_attack_tmp"
    printf 'do not clobber\n' >"$state/nonclobber-target"
    chmod 600 "$state/nonclobber-target"
    saved_tmp=$TMPDIR
    TMPDIR=$capture_attack_tmp
    FAKE_ATTACK_CAPTURE=$capture_attack
    export TMPDIR FAKE_ATTACK_CAPTURE
    # shellcheck disable=SC2086
    expect_failure "capture-$capture_attack" run_wrapper $base_args --adapter batch
    unset FAKE_ATTACK_CAPTURE
    TMPDIR=$saved_tmp
    export TMPDIR
    grep -F 'phase=evidence-contract' "$work/capture-$capture_attack.err" >/dev/null
    [ "$(cat "$state/nonclobber-target")" = 'do not clobber' ]
    [ "$(find "$capture_attack_tmp" -mindepth 1 -maxdepth 1 -type d -name 'wasm-verifier-discharge-evidence.*' | wc -l | tr -d ' ')" -eq 1 ]
    if [ "$capture_attack" = hardlink ]; then [ -f "$state/capture-hardlink-target" ]; fi
    assert_no_match fixed 'rocq-discharge-docker: result=pass' "$work/capture-$capture_attack.out"
done

reset_state
expect_failure bridge-mutation env FAKE_BRIDGE_MUTATE=1 FAKE_STATE="$state" FAKE_REVISION="$revision" FAKE_COQ_WASM_REVISION="$coq_wasm_revision" FAKE_IMAGE_REFERENCE="$image_reference" FAKE_IMAGE_ID="$image_id" FAKE_REPOSITORY_MOUNT="$repository_mount" FAKE_VERIFIER_CHECKOUT="$verifier" DOCKER="$fake_docker" GIT="$fake_git" "$fixture/ci/rocq-discharge-docker.sh" --wasm-verifier "$verifier" --container verifier-dev --adapter batch
grep -F 'phase=input-integrity' "$work/bridge-mutation.err" >/dev/null
test ! -e "$state/verify-1"

reset_state
FAKE_COHERENT_MUTATION=1
export FAKE_COHERENT_MUTATION
# shellcheck disable=SC2086
expect_failure coherent-bridge-mutation run_wrapper $base_args --adapter single
unset FAKE_COHERENT_MUTATION
grep -F 'phase=input-integrity' "$work/coherent-bridge-mutation.err" >/dev/null
test ! -e "$state/verify-1"

reset_state
FAKE_FINGERPRINT_MODE=stale-failure
export FAKE_FINGERPRINT_MODE
# shellcheck disable=SC2086
expect_failure fingerprint-stale-failure run_wrapper $base_args --adapter batch
unset FAKE_FINGERPRINT_MODE
grep -F 'phase=input-integrity' "$work/fingerprint-stale-failure.err" >/dev/null
test ! -e "$state/bridge-calls"

reset_state
FAKE_FINGERPRINT_MUTATE_AFTER=3
export FAKE_FINGERPRINT_MUTATE_AFTER
# shellcheck disable=SC2086
expect_failure verify-pre-fingerprint run_wrapper $base_args --adapter batch
unset FAKE_FINGERPRINT_MUTATE_AFTER
grep -F 'phase=input-integrity' "$work/verify-pre-fingerprint.err" >/dev/null
test ! -e "$state/verify-1"

reset_state
FAKE_MUTATE_EXCHANGE_ON_VERIFY=1
export FAKE_MUTATE_EXCHANGE_ON_VERIFY
# shellcheck disable=SC2086
expect_failure verify-post-fingerprint run_wrapper $base_args --adapter batch
unset FAKE_MUTATE_EXCHANGE_ON_VERIFY
grep -F 'phase=input-integrity' "$work/verify-post-fingerprint.err" >/dev/null
test -e "$state/verify-1"
assert_no_match fixed 'rocq-discharge-docker: result=pass' "$work/verify-post-fingerprint.out"

reset_state
expect_failure batch-extra-receipt env FAKE_BATCH_EXTRA=1 FAKE_STATE="$state" FAKE_REVISION="$revision" FAKE_COQ_WASM_REVISION="$coq_wasm_revision" FAKE_IMAGE_REFERENCE="$image_reference" FAKE_IMAGE_ID="$image_id" FAKE_REPOSITORY_MOUNT="$repository_mount" FAKE_VERIFIER_CHECKOUT="$verifier" DOCKER="$fake_docker" GIT="$fake_git" "$fixture/ci/rocq-discharge-docker.sh" --wasm-verifier "$verifier" --container verifier-dev --adapter both
[ "$(grep -c '/receipts/.*\.json$' "$state/rejected-batch-layout")" -eq 5 ]
[ "$(grep -c '/receipts/extra$' "$state/rejected-batch-layout")" -eq 1 ]
assert_no_match regex '^case ' "$state/bridge-calls"

for receipt_attack in mode extra replace; do
    reset_state
    case "$receipt_attack" in
        mode) attack_env='FAKE_RECEIPT_MODE=1' ;;
        extra) attack_env='FAKE_RECEIPT_EXTRA=1' ;;
        replace) attack_env='FAKE_REPLACE_RECEIPT_DIR=1' ;;
    esac
    # shellcheck disable=SC2086
    expect_failure "receipt-$receipt_attack" env $attack_env FAKE_STATE="$state" FAKE_REVISION="$revision" FAKE_COQ_WASM_REVISION="$coq_wasm_revision" FAKE_IMAGE_REFERENCE="$image_reference" FAKE_IMAGE_ID="$image_id" FAKE_REPOSITORY_MOUNT="$repository_mount" FAKE_VERIFIER_CHECKOUT="$verifier" DOCKER="$fake_docker" GIT="$fake_git" "$fixture/ci/rocq-discharge-docker.sh" --wasm-verifier "$verifier" --container verifier-dev --adapter single
    test ! -e "$state/verify-1"
done

reset_state
receipt_attack_tmp=$test_tmp/receipt-hardlink
mkdir -m 700 "$receipt_attack_tmp"
saved_tmp=$TMPDIR
TMPDIR=$receipt_attack_tmp
FAKE_RECEIPT_HARDLINK=1
export TMPDIR FAKE_RECEIPT_HARDLINK
# shellcheck disable=SC2086
expect_failure receipt-hardlink run_wrapper $base_args --adapter single
unset FAKE_RECEIPT_HARDLINK
TMPDIR=$saved_tmp
export TMPDIR
grep -F 'phase=single' "$work/receipt-hardlink.err" >/dev/null
test ! -e "$state/verify-1"
[ -f "$state/receipt-hardlink-target" ]
[ "$(find "$receipt_attack_tmp" -mindepth 1 -maxdepth 1 -type d -name 'inference-rocq-discharge.*' | wc -l | tr -d ' ')" -eq 1 ]

reset_state
FAKE_REPLACE_PREVIOUS_RECEIPT=1
export FAKE_REPLACE_PREVIOUS_RECEIPT
# shellcheck disable=SC2086
expect_failure receipt-file-replaced run_wrapper $base_args --adapter single
unset FAKE_REPLACE_PREVIOUS_RECEIPT
grep -F 'phase=single' "$work/receipt-file-replaced.err" >/dev/null
test ! -e "$state/verify-1"
[ -f "$(cat "$state/suspect-staging-path")/receipts/prime-bounded/prime-bounded.json" ]

reset_state
expect_failure staged-raw-mutation env FAKE_BRIDGE_MUTATE_STAGED=prime-bounded FAKE_STATE="$state" FAKE_REVISION="$revision" FAKE_COQ_WASM_REVISION="$coq_wasm_revision" FAKE_IMAGE_REFERENCE="$image_reference" FAKE_IMAGE_ID="$image_id" FAKE_REPOSITORY_MOUNT="$repository_mount" FAKE_VERIFIER_CHECKOUT="$verifier" DOCKER="$fake_docker" GIT="$fake_git" "$fixture/ci/rocq-discharge-docker.sh" --wasm-verifier "$verifier" --container verifier-dev --adapter single
grep -F 'phase=input-integrity' "$work/staged-raw-mutation.err" >/dev/null
test ! -e "$state/verify-1"

reset_state
expect_failure staging-replacement env FAKE_REPLACE_STAGING=1 FAKE_STATE="$state" FAKE_REVISION="$revision" FAKE_COQ_WASM_REVISION="$coq_wasm_revision" FAKE_IMAGE_REFERENCE="$image_reference" FAKE_IMAGE_ID="$image_id" FAKE_REPOSITORY_MOUNT="$repository_mount" FAKE_VERIFIER_CHECKOUT="$verifier" DOCKER="$fake_docker" GIT="$fake_git" "$fixture/ci/rocq-discharge-docker.sh" --wasm-verifier "$verifier" --container verifier-dev --adapter single
grep -F 'phase=staging-identity' "$work/staging-replacement.err" >/dev/null
replaced_staging=$(cat "$state/replaced-staging-path")
[ -f "$replaced_staging/replacement-sentinel" ]

reset_state
FAKE_REPLACE_STAGING_ON_VERIFY=1
export FAKE_REPLACE_STAGING_ON_VERIFY
# shellcheck disable=SC2086
expect_failure staging-replacement-at-success run_wrapper $base_args --adapter batch
unset FAKE_REPLACE_STAGING_ON_VERIFY
grep -F 'phase=staging-identity' "$work/staging-replacement-at-success.err" >/dev/null
replaced_staging=$(cat "$state/replaced-staging-path")
[ -f "$replaced_staging/replacement-sentinel" ]
assert_no_match fixed 'rocq-discharge-docker: result=pass' "$work/staging-replacement-at-success.out"

for cleanup_volume in source source-false-success source-hidden-live exchange exchange-false-success exchange-hidden-live; do
    reset_state
    FAKE_VOLUME_RM_FAIL=$cleanup_volume
    export FAKE_VOLUME_RM_FAIL
    # shellcheck disable=SC2086
    expect_failure "cleanup-volume-busy-$cleanup_volume" run_wrapper $base_args --adapter batch
    unset FAKE_VOLUME_RM_FAIL
    grep -F 'phase=cleanup' "$work/cleanup-volume-busy-$cleanup_volume.err" >/dev/null || {
        sed -n '1,20p' "$work/cleanup-volume-busy-$cleanup_volume.err" >&2
        exit 1
    }
    assert_no_match fixed 'rocq-discharge-docker: result=pass' "$work/cleanup-volume-busy-$cleanup_volume.out"
done

for volume_query_attack in failure existing extra; do
    reset_state
    case "$volume_query_attack" in
        failure) FAKE_VOLUME_LS_FAIL=1; export FAKE_VOLUME_LS_FAIL ;;
        existing) FAKE_VOLUME_LS_EXISTING=1; export FAKE_VOLUME_LS_EXISTING ;;
        extra) FAKE_VOLUME_LS_EXTRA=1; export FAKE_VOLUME_LS_EXTRA ;;
    esac
    # shellcheck disable=SC2086
    expect_failure "volume-query-$volume_query_attack" run_wrapper $base_args --adapter batch
    unset FAKE_VOLUME_LS_FAIL FAKE_VOLUME_LS_EXISTING FAKE_VOLUME_LS_EXTRA 2>/dev/null || true
    grep -F 'phase=cleanup' "$work/volume-query-$volume_query_attack.err" >/dev/null
    assert_no_match fixed 'rocq-discharge-docker: result=pass' "$work/volume-query-$volume_query_attack.out"
done

reset_state
unsafe_tmp=$work/unsafe-world-writable
mkdir -m 777 "$unsafe_tmp"
safe_tmp=$TMPDIR
TMPDIR=$unsafe_tmp
export TMPDIR
# shellcheck disable=SC2086
expect_failure unsafe-tmpdir run_wrapper $base_args --adapter batch
TMPDIR=$safe_tmp
export TMPDIR
grep -F 'phase=configuration' "$work/unsafe-tmpdir.err" >/dev/null
test ! -e "$state/bridge-calls"

if [ "$(id -u)" -eq 0 ]; then
    reset_state
    safe_tmp=$TMPDIR
    TMPDIR=/work
    export TMPDIR
    # shellcheck disable=SC2086
    run_wrapper $base_args --adapter batch >"$work/root-sticky-tmp.out"
    TMPDIR=$safe_tmp
    export TMPDIR
    grep -F 'rocq-discharge-docker: result=pass' "$work/root-sticky-tmp.out" >/dev/null
fi

reset_state
unsafe_newline=$(printf 'bad\nname')
expect_failure relative run_wrapper --wasm-verifier verifier --container verifier-dev
expect_failure traversal run_wrapper --wasm-verifier "$fixture/../repo" --container verifier-dev
expect_failure comma run_wrapper --wasm-verifier "$verifier" --container 'bad,name'
expect_failure newline run_wrapper --wasm-verifier "$verifier" --container "$unsafe_newline"
expect_failure bad-adapter run_wrapper --wasm-verifier "$verifier" --container verifier-dev --adapter other
expect_failure duplicate run_wrapper --wasm-verifier "$verifier" --container verifier-dev --full --full
test ! -e "$state/bridge-calls"

echo 'rocq-discharge-docker self-test: PASS'
