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
trap 'rm -rf "$work"' EXIT HUP INT TERM

fixture=$work/repo
state=$work/state
fake_bin=$work/'fake tools'
fake_docker=$fake_bin/'docker tool'
fake_git=$fake_bin/'git tool'
verifier=$fixture/verifier
revision=77f1126d5de023d9f8464c60c0137b6321126757
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
    *' rev-parse HEAD')
        if [ "${FAKE_MISMATCH:-}" = checkout-revision ]; then
            printf '%040d\n' 0
        else
            printf '%s\n' "${FAKE_REVISION:?}"
        fi
        ;;
    *' status --porcelain --untracked-files=all')
        if [ "${FAKE_MISMATCH:-}" = dirty-checkout ]; then printf '?? private-proof.tmp\n'; fi
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
    [ -d "$state/volumes/$name" ] || exit 1
    if has_arg --format "$@"; then cat "$state/labels/$name"; else printf '[{}]\n'; fi
    exit 0
fi
if [ "$1" = volume ] && [ "$2" = rm ]; then
    name=$3
    event "volume-rm $name"
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
        case "$format" in
            '{{.State.Running}}') [ "${FAKE_MISMATCH:-}" = stopped ] && printf false || printf true ;;
            '{{.Config.Image}}') [ "${FAKE_MISMATCH:-}" = image-reference ] && printf wrong/image:latest || printf '%s' "$image_reference" ;;
            '{{.Image}}') [ "${FAKE_MISMATCH:-}" = image-id ] && printf 'sha256:%064d' 0 || printf '%s' "$image_id" ;;
            '{{.Config.User}}') [ "${FAKE_MISMATCH:-}" = user ] && printf root || printf coq ;;
            '{{range .Mounts}}{{printf "%s\t%s\n" .Destination .Source}}{{end}}')
                if [ "${FAKE_MISMATCH:-}" = mount ]; then
                    printf '/wrong/repository\t%s\n' "${FAKE_VERIFIER_CHECKOUT:?}"
                elif [ "${FAKE_MISMATCH:-}" = mount-source ]; then
                    printf '%s\t/wrong/verifier\n' "$repository_mount"
                elif [ "${FAKE_MISMATCH:-}" = mount-socket ]; then
                    printf '%s\t%s\n/var/run/docker.sock\t/var/run/docker.sock\n' "$repository_mount" "${FAKE_VERIFIER_CHECKOUT:?}"
                else
                    printf '%s\t%s\n' "$repository_mount" "${FAKE_VERIFIER_CHECKOUT:?}"
                fi
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
    observed_tag=v2.2.0
    observed_coq_wasm=$coq_wasm_revision
    case "${FAKE_MISMATCH:-}" in
        provenance-user) observed_user=root ;;
        provenance-uid) observed_uid=0 ;;
        provenance-gid) observed_gid=0 ;;
        provenance-revision) observed_revision=$(printf '%040d' 0) ;;
        coq-version) observed_coq=8.19.3 ;;
        coq-wasm-tag) observed_tag=v0.0.0 ;;
        coq-wasm-revision) observed_coq_wasm=$(printf '%040d' 0) ;;
    esac
    printf 'coq_user=%s\ncoq_uid=%s\ncoq_gid=%s\nwasm_verifier_revision=%s\ncoq_version=%s\ncoq_wasm_tag=%s\ncoq_wasm_revision=%s\n' \
        "$observed_user" "$observed_uid" "$observed_gid" "$observed_revision" "$observed_coq" "$observed_tag" "$observed_coq_wasm"
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
        elif has_arg test "$@"; then
            if has_arg rocq_dischargeability:: "$@"; then
                printf 'test result: ok. 49 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out\n'
            else
                case "${FAKE_FULL_MODE:-floor}" in
                    empty) : ;;
                    single) printf 'test result: ok. 4000 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out\n' ;;
                    under)
                        printf 'test result: ok. 3070 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out\n'
                        printf 'test result: ok. 4 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out\n'
                        ;;
                    malformed)
                        printf 'test result: ok. many passed; 0 failed; malformed\n'
                        printf 'test result: ok. 4000 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out\n'
                        ;;
                    filtered-only)
                        printf 'test result: ok. 3070 passed; 0 failed; 158 ignored; 0 measured; 1 filtered out\n'
                        printf 'test result: ok. 4 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out\n'
                        printf 'test result: ok. 0 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out\n'
                        printf 'test result: ok. 1 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out\n'
                        printf 'test result: ok. 0 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out\n'
                        ;;
                    failed)
                        printf 'test result: FAILED. 3075 passed; 1 failed; 0 ignored; 0 measured; 0 filtered out\n'
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
        *task4-identity*)
            directory=$state/volumes/$exchange
            {
                cksum "$directory/request.json"
                cksum "$directory/raw/rocq_prime_bounded_example.v"
                cksum "$directory/raw/rocq_exists_spec.v"
                cksum "$directory/raw/rocq_unique_spec.v"
                cksum "$directory/raw/spec_narrow_discharge.v"
                cksum "$directory/raw/rocq_false_certificate.v"
            } >"$staging/exchange.identity.next"
            ;;
        *task4-copy-raw*)
            mkdir -p "$staging/raw"
            cp "$state/volumes/$exchange/raw/"*.v "$staging/raw/"
            ;;
        *task4-check-staged-raw*)
            basename=$(last_arg "$@")
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
cat >"$verifier/ci/discharge/run-docker-batch.sh" <<'BATCH'
#!/usr/bin/env sh
set -eu
[ "$#" -eq 2 ] && [ "$1" = --exchange-volume ] || exit 92
[ "${WASM_VERIFIER_CONTAINER:?}" = verifier-dev ] || exit 93
[ -d "${INFERENCE_WASM_VERIFIER_EVIDENCE_DIR:?}" ] || exit 94
state=${FAKE_STATE:?}; volume=$2
printf 'batch <%s> <%s>\n' "$1" "$2" >>"$state/bridge-calls"
if [ "${FAKE_BRIDGE_FAIL:-}" = batch ]; then
    if [ "${FAKE_BRIDGE_MISSING_LOG:-0}" != 1 ]; then
        (umask 077; printf 'private batch proof log\n' >"$INFERENCE_WASM_VERIFIER_EVIDENCE_DIR/verifier.log")
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
mkdir "$state/volumes/$volume/receipts"
for case_id in prime-bounded exists unique narrow-domain false-spec; do printf 'batch-%s\n' "$case_id" >"$state/volumes/$volume/receipts/$case_id.json"; done
if [ "${FAKE_BATCH_EXTRA:-0}" = 1 ]; then mkdir "$state/volumes/$volume/receipts/extra"; fi
if [ "${FAKE_BRIDGE_MUTATE:-0}" = 1 ]; then
    printf 'coherently replaced request\n' >"$state/volumes/$volume/request.json"
    printf 'coherently replaced raw\n' >"$state/volumes/$volume/raw/rocq_prime_bounded_example.v"
fi
BATCH
cat >"$verifier/ci/discharge/run-docker-case.sh" <<'CASE'
#!/usr/bin/env sh
set -eu
[ "$#" -eq 7 ] && [ "$1" = --protocol ] && [ "$2" = 1 ] && [ "$3" = --wasm-verifier-revision ] && [ "$5" = --case ] || exit 95
[ "$4" = "${FAKE_REVISION:?}" ] || exit 96
case_id=$6; raw=$7; receipt_dir=${INFERENCE_WASM_VERIFIER_RECEIPT_DIR:?}; state=${FAKE_STATE:?}
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
CASE
chmod +x "$verifier/ci/discharge/"*.sh

reset_state() {
    rm -rf "$state"
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
    [ "$actual" -eq "$expected" ] || { echo "self-test: $label returned $actual, expected $expected" >&2; exit 1; }
    case "$label" in
        proxy-*) [ -z "$(find "$state/calls" -mindepth 1 -type f -print -quit)" ] || { echo "self-test: $label reached real Docker" >&2; exit 1; } ;;
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
        batch) [ "$(grep -c '^batch ' "$state/bridge-calls")" -eq 1 ]; ! grep -q '^case ' "$state/bridge-calls" ;;
        single) ! grep -q '^batch ' "$state/bridge-calls"; [ "$(grep -c '^case ' "$state/bridge-calls")" -eq 5 ] ;;
        both) [ "$(grep -c '^batch ' "$state/bridge-calls")" -eq 1 ]; [ "$(grep -c '^case ' "$state/bridge-calls")" -eq 5 ] ;;
    esac
done

reset_state
# shellcheck disable=SC2086
run_wrapper $base_args --full >"$work/full.out"
grep -F 'rocq-discharge-docker: full crate=inference-tests result-lines=5 passed=3075 floor=3075' "$work/full.out" >/dev/null

for floor_mode in empty single under malformed filtered-only failed; do
    reset_state
    # shellcheck disable=SC2086
    expect_failure "floor-$floor_mode" env FAKE_FULL_MODE="$floor_mode" FAKE_STATE="$state" FAKE_REVISION="$revision" FAKE_COQ_WASM_REVISION="$coq_wasm_revision" FAKE_IMAGE_REFERENCE="$image_reference" FAKE_IMAGE_ID="$image_id" FAKE_REPOSITORY_MOUNT="$repository_mount" FAKE_VERIFIER_CHECKOUT="$verifier" DOCKER="$fake_docker" GIT="$fake_git" "$fixture/ci/rocq-discharge-docker.sh" --wasm-verifier "$verifier" --container verifier-dev --full
    grep -F 'phase=full-test-floor' "$work/floor-$floor_mode.err" >/dev/null
done

for mismatch in stopped image-reference image-id user mount mount-source mount-socket provenance-user provenance-uid provenance-gid provenance-revision coq-version coq-wasm-tag coq-wasm-revision checkout-revision dirty-checkout; do
    reset_state
    expect_failure "mismatch-$mismatch" env FAKE_MISMATCH="$mismatch" FAKE_STATE="$state" FAKE_REVISION="$revision" FAKE_COQ_WASM_REVISION="$coq_wasm_revision" FAKE_IMAGE_REFERENCE="$image_reference" FAKE_IMAGE_ID="$image_id" FAKE_REPOSITORY_MOUNT="$repository_mount" FAKE_VERIFIER_CHECKOUT="$verifier" DOCKER="$fake_docker" GIT="$fake_git" "$fixture/ci/rocq-discharge-docker.sh" --wasm-verifier "$verifier" --container verifier-dev
    test ! -e "$state/bridge-calls"
done

for pin_variant in unknown malformed duplicate; do
    reset_state
    case "$pin_variant" in
        unknown) write_container_pin_unknown ;;
        malformed) write_container_pin_malformed ;;
        duplicate) write_container_pin_duplicate ;;
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
! grep -F 'PRIVATE' "$work/bridge-failure.err" >/dev/null
evidence=$(sed -n 's/^rocq-discharge-docker: phase=batch evidence=//p' "$work/bridge-failure.err")
[ -d "$evidence" ] && [ ! -L "$evidence" ] && [ -f "$evidence/verifier.log" ]
evidence_line=$(grep -n 'bridge-evidence-written' "$state/events" | cut -d: -f1)
exchange_remove_line=$(grep -n 'volume-rm inference-rocq-discharge-.*-exchange' "$state/events" | cut -d: -f1)
[ "$evidence_line" -ge 1 ] && [ "$exchange_remove_line" -gt "$evidence_line" ]
rm -rf "$evidence"

reset_state
expect_failure bridge-missing-log env FAKE_BRIDGE_FAIL=batch FAKE_BRIDGE_MISSING_LOG=1 FAKE_STATE="$state" FAKE_REVISION="$revision" FAKE_COQ_WASM_REVISION="$coq_wasm_revision" FAKE_IMAGE_REFERENCE="$image_reference" FAKE_IMAGE_ID="$image_id" FAKE_REPOSITORY_MOUNT="$repository_mount" FAKE_VERIFIER_CHECKOUT="$verifier" DOCKER="$fake_docker" GIT="$fake_git" "$fixture/ci/rocq-discharge-docker.sh" --wasm-verifier "$verifier" --container verifier-dev
grep -F 'phase=evidence-contract' "$work/bridge-missing-log.err" >/dev/null
! grep -F 'evidence=' "$work/bridge-missing-log.err" >/dev/null

reset_state
expect_failure bridge-success-log env FAKE_BRIDGE_STALE_LOG=1 FAKE_STATE="$state" FAKE_REVISION="$revision" FAKE_COQ_WASM_REVISION="$coq_wasm_revision" FAKE_IMAGE_REFERENCE="$image_reference" FAKE_IMAGE_ID="$image_id" FAKE_REPOSITORY_MOUNT="$repository_mount" FAKE_VERIFIER_CHECKOUT="$verifier" DOCKER="$fake_docker" GIT="$fake_git" "$fixture/ci/rocq-discharge-docker.sh" --wasm-verifier "$verifier" --container verifier-dev --adapter batch
grep -F 'phase=evidence-contract' "$work/bridge-success-log.err" >/dev/null
! grep -F 'evidence=' "$work/bridge-success-log.err" >/dev/null

reset_state
expect_failure bridge-mutation env FAKE_BRIDGE_MUTATE=1 FAKE_STATE="$state" FAKE_REVISION="$revision" FAKE_COQ_WASM_REVISION="$coq_wasm_revision" FAKE_IMAGE_REFERENCE="$image_reference" FAKE_IMAGE_ID="$image_id" FAKE_REPOSITORY_MOUNT="$repository_mount" FAKE_VERIFIER_CHECKOUT="$verifier" DOCKER="$fake_docker" GIT="$fake_git" "$fixture/ci/rocq-discharge-docker.sh" --wasm-verifier "$verifier" --container verifier-dev --adapter batch
grep -F 'phase=input-integrity' "$work/bridge-mutation.err" >/dev/null
test ! -e "$state/verify-1"

reset_state
expect_failure batch-extra-receipt env FAKE_BATCH_EXTRA=1 FAKE_STATE="$state" FAKE_REVISION="$revision" FAKE_COQ_WASM_REVISION="$coq_wasm_revision" FAKE_IMAGE_REFERENCE="$image_reference" FAKE_IMAGE_ID="$image_id" FAKE_REPOSITORY_MOUNT="$repository_mount" FAKE_VERIFIER_CHECKOUT="$verifier" DOCKER="$fake_docker" GIT="$fake_git" "$fixture/ci/rocq-discharge-docker.sh" --wasm-verifier "$verifier" --container verifier-dev --adapter both
[ "$(grep -c '/receipts/.*\.json$' "$state/rejected-batch-layout")" -eq 5 ]
[ "$(grep -c '/receipts/extra$' "$state/rejected-batch-layout")" -eq 1 ]
! grep -q '^case ' "$state/bridge-calls"

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
expect_failure staged-raw-mutation env FAKE_BRIDGE_MUTATE_STAGED=prime-bounded FAKE_STATE="$state" FAKE_REVISION="$revision" FAKE_COQ_WASM_REVISION="$coq_wasm_revision" FAKE_IMAGE_REFERENCE="$image_reference" FAKE_IMAGE_ID="$image_id" FAKE_REPOSITORY_MOUNT="$repository_mount" FAKE_VERIFIER_CHECKOUT="$verifier" DOCKER="$fake_docker" GIT="$fake_git" "$fixture/ci/rocq-discharge-docker.sh" --wasm-verifier "$verifier" --container verifier-dev --adapter single
grep -F 'phase=input-integrity' "$work/staged-raw-mutation.err" >/dev/null
test ! -e "$state/verify-1"

reset_state
expect_failure staging-replacement env FAKE_REPLACE_STAGING=1 FAKE_STATE="$state" FAKE_REVISION="$revision" FAKE_COQ_WASM_REVISION="$coq_wasm_revision" FAKE_IMAGE_REFERENCE="$image_reference" FAKE_IMAGE_ID="$image_id" FAKE_REPOSITORY_MOUNT="$repository_mount" FAKE_VERIFIER_CHECKOUT="$verifier" DOCKER="$fake_docker" GIT="$fake_git" "$fixture/ci/rocq-discharge-docker.sh" --wasm-verifier "$verifier" --container verifier-dev --adapter single
grep -F 'phase=staging-identity' "$work/staging-replacement.err" >/dev/null
replaced_staging=$(cat "$state/replaced-staging-path")
[ -f "$replaced_staging/replacement-sentinel" ]

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
