#!/usr/bin/env bash
set -euo pipefail

script_dir=$(CDPATH= cd -- "$(dirname -- "$0")" && pwd)
project_root=$(CDPATH= cd -- "$script_dir/../.." && pwd)
installed_binary=/usr/libexec/antdelirium/antdelirium
operator_credential=/var/lib/antdelirium-operator/state.cred
admin_socket=/run/antdelirium-admin/admin.sock
inventory_path="$project_root/docs/evidence/autobrowser-legacy-source-inventory-v1.json"
run_id=$(date -u +%Y%m%dT%H%M%SZ)
secret_root="/run/antdelirium-candidate-$run_id-$$"
log_root="$project_root/target/t86-installed-candidate-logs/$run_id"
admin_fd=
source_fd=
inventory_fd=
migration_id=
project_id=
workspace_id=
source_sha256=
staging_sha256=
normalized_sha256=
semantic_sha256=
dry_run_sha256=
conversion_sha256=
manifest_sha256=

fail() {
    printf '%s\n' "AD-T86-CANDIDATE: $1" >&2
    exit 1
}

[[ $EUID -eq 0 ]] || fail 'execute this supervisor routine with sudo.'
invoking_user=${SUDO_USER:-}
[[ -n "$invoking_user" && "$invoking_user" != root ]] \
    || fail 'SUDO_USER must identify the non-root operator.'

if [[ ${1:-} == '--source-root' ]]; then
    [[ $# -eq 2 ]] || fail '--source-root requires exactly one absolute directory.'
    source_locator=$2
    [[ "$source_locator" == /* && -d "$source_locator" && ! -L "$source_locator" ]] \
        || fail 'the private source must be an absolute, existing, non-symlink directory.'
    command -v python3 >/dev/null 2>&1 \
        || fail 'required command is unavailable: python3'
    script_path="$script_dir/prepare-installed-autobrowser-candidate.sh"
    bash_path=$(command -v bash)
    exec python3 - "$script_path" "$source_locator" "$invoking_user" "$bash_path" <<'PY'
import array
import os
import pwd
import socket
import stat
import sys

script_path, source_locator, invoking_user, bash_path = sys.argv[1:]

def fail():
    print(
        "AD-T86-CANDIDATE: the private source descriptor could not be opened by the operator.",
        file=sys.stderr,
    )
    raise SystemExit(1)

try:
    account = pwd.getpwnam(invoking_user)
    parent_socket, child_socket = socket.socketpair(socket.AF_UNIX, socket.SOCK_DGRAM)
    child_pid = os.fork()
except Exception:
    fail()

if child_pid == 0:
    try:
        parent_socket.close()
        os.initgroups(account.pw_name, account.pw_gid)
        os.setgid(account.pw_gid)
        os.setuid(account.pw_uid)
        flags = os.O_RDONLY | os.O_DIRECTORY | os.O_NOFOLLOW
        source_fd = os.open(source_locator, flags)
        metadata = os.fstat(source_fd)
        if not stat.S_ISDIR(metadata.st_mode):
            raise RuntimeError("source-is-not-directory")
        descriptors = array.array("i", [source_fd])
        child_socket.sendmsg(
            [b"D"],
            [(socket.SOL_SOCKET, socket.SCM_RIGHTS, descriptors)],
        )
        os.close(source_fd)
        child_socket.close()
        os._exit(0)
    except Exception:
        try:
            child_socket.send(b"E")
            child_socket.close()
        except Exception:
            pass
        os._exit(2)

child_socket.close()
received_fd = None
try:
    message, ancillary, _flags, _address = parent_socket.recvmsg(
        1, socket.CMSG_SPACE(array.array("i").itemsize)
    )
    for level, kind, data in ancillary:
        if level == socket.SOL_SOCKET and kind == socket.SCM_RIGHTS:
            descriptors = array.array("i")
            descriptors.frombytes(data[: descriptors.itemsize])
            if len(descriptors) == 1:
                received_fd = descriptors[0]
                break
    _pid, status = os.waitpid(child_pid, 0)
    parent_socket.close()
    if (
        message != b"D"
        or received_fd is None
        or not os.WIFEXITED(status)
        or os.WEXITSTATUS(status) != 0
    ):
        fail()
    os.set_inheritable(received_fd, True)
    os.execv(
        bash_path,
        [bash_path, script_path, "--source-fd", str(received_fd)],
    )
except Exception:
    if received_fd is not None:
        os.close(received_fd)
    try:
        os.waitpid(child_pid, 0)
    except Exception:
        pass
    fail()
PY
fi

close_descriptors() {
    set +e
    [[ -z "$admin_fd" ]] || eval "exec ${admin_fd}<&-"
    [[ -z "$source_fd" ]] || eval "exec ${source_fd}<&-"
    [[ -z "$inventory_fd" ]] || eval "exec ${inventory_fd}<&-"
    admin_fd=
    source_fd=
    inventory_fd=
}

close_admin_and_inventory() {
    set +e
    [[ -z "$admin_fd" ]] || eval "exec ${admin_fd}<&-"
    [[ -z "$inventory_fd" ]] || eval "exec ${inventory_fd}<&-"
    admin_fd=
    inventory_fd=
    set -e
}

cleanup() {
    local exit_code=$?
    close_descriptors
    rm -rf "$secret_root"
    {
        printf 'contract=antdelirium-installed-candidate-preparation-v1\n'
        printf 'run_id=%s\nexit_code=%s\n' "$run_id" "$exit_code"
        printf 'migration_id=%s\nproject_id=%s\nworkspace_id=%s\n' \
            "$migration_id" "$project_id" "$workspace_id"
        printf 'source_sha256=%s\nstaging_sha256=%s\n' "$source_sha256" "$staging_sha256"
        printf 'normalized_sha256=%s\nsemantic_sha256=%s\n' \
            "$normalized_sha256" "$semantic_sha256"
        printf 'dry_run_sha256=%s\nconversion_sha256=%s\nmanifest_sha256=%s\n' \
            "$dry_run_sha256" "$conversion_sha256" "$manifest_sha256"
        printf 'source_mutation=forbidden\nphysical_locators=redacted\n'
        printf 'completed_at=%s\n' "$(date -u +%Y-%m-%dT%H:%M:%SZ)"
    } >"$log_root/result.env"
    chmod -R a+rX "$log_root"
    printf '%s\n' "AD-T86-CANDIDATE: logs=$log_root"
    exit "$exit_code"
}

extract_field() {
    local output=$1
    local key=$2
    local token
    for token in $output; do
        case "$token" in
            "$key="*) printf '%s\n' "${token#*=}"; return 0 ;;
        esac
    done
    return 1
}

require_id() {
    local value=$1
    local prefix=$2
    [[ "$value" =~ ^${prefix}_[0-9a-hjkmnp-tv-z]{26}$ ]] \
        || fail "the $prefix identifier returned by the service is invalid."
}

require_sha256() {
    [[ "$1" =~ ^[0-9a-f]{64}$ ]] \
        || fail 'the service returned an invalid digest.'
}

open_admin_descriptor() {
    exec {admin_fd}<"$secret_root/state.json"
}

run_state_operation() {
    local submode=$1
    shift
    open_admin_descriptor
    local output
    output=$(setpriv --reuid="$invoking_uid" --regid="$invoking_gid" --init-groups \
        --no-new-privs --inh-caps=-all --ambient-caps=-all --bounding-set=-all \
        --reset-env env \
        ANTDELIRIUM_ADMIN_SOCKET="$admin_socket" \
        ANTDELIRIUM_ADMIN_CAPABILITY_FD="$admin_fd" \
        "$@" "$installed_binary" migrate-plugin "$submode")
    eval "exec ${admin_fd}<&-"
    admin_fd=
    printf '%s\n' "$output"
}

if [[ ${1:-} == '--source-fd' ]]; then
    inherited_source_fd=${2:-}
    [[ $# -eq 2 && "$inherited_source_fd" =~ ^[0-9]+$ \
        && "$inherited_source_fd" -ge 3 \
        && -d "/proc/$$/fd/$inherited_source_fd" ]] \
        || fail 'the inherited source descriptor is invalid.'
    source_fd=$inherited_source_fd
    unset inherited_source_fd
    shift 2
fi
[[ $# -eq 0 ]] || fail 'use either no argument or --source-root with one directory.'
invoking_uid=$(id -u "$invoking_user")
invoking_gid=$(id -g "$invoking_user")

mkdir -p "$log_root"
chmod 0755 "$(dirname "$log_root")" "$log_root"
exec > >(tee -a "$log_root/candidate-preparation.log") 2>&1
trap cleanup EXIT

for command in systemd-creds setpriv jq id python3 tee stat; do
    command -v "$command" >/dev/null 2>&1 \
        || fail "required command is unavailable: $command"
done
[[ -x "$installed_binary" && ! -L "$installed_binary" ]] \
    || fail 'the installed AntDelirium binary is unavailable.'
[[ -S "$admin_socket" ]] || fail 'the administrative socket is unavailable.'
[[ -f "$operator_credential" && ! -L "$operator_credential" ]] \
    || fail 'the encrypted StateAdmin descriptor is unavailable.'
[[ -f "$inventory_path" && ! -L "$inventory_path" ]] \
    || fail 'the reviewed project-local inventory is unavailable.'

source_sha256=$(jq -er '
    select(.contract_version == "legacy-source-inventory-v1") |
    .inventory_sha256 |
    select(test("^[0-9a-f]{64}$"))
' "$inventory_path") || fail 'the reviewed inventory contract is invalid.'

install -d -m 0700 "$secret_root"
systemd-creds --name=antdelirium-operator-state-v1 decrypt \
    "$operator_credential" "$secret_root/state.json" >/dev/null
chmod 0600 "$secret_root/state.json"
jq -e '
    .contract_version == "admin-capability-v1" and
    (keys | sort) == ["capability", "contract_version"] and
    (.capability | keys | sort) == ["capability_id", "token"] and
    (.capability.capability_id | test("^cap_[0-9a-hjkmnp-tv-z]{26}$")) and
    (.capability.token | test("^[A-Za-z0-9_-]{43}$"))
' "$secret_root/state.json" >/dev/null \
    || fail 'the decrypted StateAdmin descriptor is invalid.'

if [[ -z "$source_fd" ]]; then
    printf '%s' 'Caminho da origem legada (digitação visível; não será registrada): ' >/dev/tty
    IFS= read -r source_locator </dev/tty
    [[ "$source_locator" == /* && -d "$source_locator" && ! -L "$source_locator" ]] \
        || fail 'the private source must be an absolute, existing, non-symlink directory.'
    exec {source_fd}<"$source_locator"
    unset source_locator
fi
[[ -d "/proc/$$/fd/$source_fd" ]] \
    || fail 'the inherited source descriptor is not a directory.'
exec {inventory_fd}<"$inventory_path"
open_admin_descriptor
printf '%s\n' 'AD-T86-CANDIDATE: staging the reviewed source through descriptor-only input.'
stage_output=$(setpriv --reuid="$invoking_uid" --regid="$invoking_gid" --init-groups \
    --no-new-privs --inh-caps=-all --ambient-caps=-all --bounding-set=-all \
    --reset-env env \
    ANTDELIRIUM_ADMIN_SOCKET="$admin_socket" \
    ANTDELIRIUM_ADMIN_CAPABILITY_FD="$admin_fd" \
    ANTDELIRIUM_MIGRATION_SOURCE_FD="$source_fd" \
    ANTDELIRIUM_MIGRATION_INVENTORY_FD="$inventory_fd" \
    ANTDELIRIUM_EXPECTED_SOURCE_SHA256="$source_sha256" \
    "$installed_binary" migrate-plugin)
close_admin_and_inventory
printf '%s\n' "$stage_output"
migration_id=$(extract_field "$stage_output" migration_id) || fail 'staging returned no migration ID.'
project_id=$(extract_field "$stage_output" project_id) || fail 'staging returned no project ID.'
staging_sha256=$(extract_field "$stage_output" staging_sha256) || fail 'staging returned no digest.'
require_id "$migration_id" mig
require_id "$project_id" prj
require_sha256 "$staging_sha256"

exec {inventory_fd}<"$inventory_path"
open_admin_descriptor
printf '%s\n' 'AD-T86-CANDIDATE: normalizing the same descriptor-bound generation.'
normalize_output=$(setpriv --reuid="$invoking_uid" --regid="$invoking_gid" --init-groups \
    --no-new-privs --inh-caps=-all --ambient-caps=-all --bounding-set=-all \
    --reset-env env \
    ANTDELIRIUM_ADMIN_SOCKET="$admin_socket" \
    ANTDELIRIUM_ADMIN_CAPABILITY_FD="$admin_fd" \
    ANTDELIRIUM_MIGRATION_SOURCE_FD="$source_fd" \
    ANTDELIRIUM_MIGRATION_INVENTORY_FD="$inventory_fd" \
    ANTDELIRIUM_EXPECTED_SOURCE_SHA256="$source_sha256" \
    ANTDELIRIUM_MIGRATION_ID="$migration_id" \
    "$installed_binary" migrate-plugin normalize)
close_descriptors
printf '%s\n' "$normalize_output"
normalized_sha256=$(extract_field "$normalize_output" normalized_sha256) \
    || fail 'normalization returned no normalized digest.'
semantic_sha256=$(extract_field "$normalize_output" semantic_sha256) \
    || fail 'normalization returned no semantic digest.'
require_sha256 "$normalized_sha256"
require_sha256 "$semantic_sha256"

printf '%s\n' 'AD-T86-CANDIDATE: creating the installed dry-run report.'
dry_run_output=$(run_state_operation dry-run \
    ANTDELIRIUM_MIGRATION_ID="$migration_id" \
    ANTDELIRIUM_EXPECTED_SOURCE_SHA256="$source_sha256" \
    ANTDELIRIUM_EXPECTED_STAGING_SHA256="$staging_sha256" \
    ANTDELIRIUM_EXPECTED_NORMALIZED_SHA256="$normalized_sha256" \
    ANTDELIRIUM_EXPECTED_SEMANTIC_SHA256="$semantic_sha256")
printf '%s\n' "$dry_run_output"
dry_run_sha256=$(extract_field "$dry_run_output" report_sha256) \
    || fail 'dry-run returned no report digest.'
manifest_sha256=$(extract_field "$dry_run_output" manifest_sha256) \
    || fail 'dry-run returned no manifest digest.'
require_sha256 "$dry_run_sha256"
require_sha256 "$manifest_sha256"

printf '%s\n' 'AD-T86-CANDIDATE: materializing the converted candidate without cutover.'
convert_output=$(run_state_operation convert \
    ANTDELIRIUM_MIGRATION_ID="$migration_id" \
    ANTDELIRIUM_EXPECTED_SOURCE_SHA256="$source_sha256" \
    ANTDELIRIUM_EXPECTED_STAGING_SHA256="$staging_sha256" \
    ANTDELIRIUM_EXPECTED_NORMALIZED_SHA256="$normalized_sha256" \
    ANTDELIRIUM_EXPECTED_SEMANTIC_SHA256="$semantic_sha256" \
    ANTDELIRIUM_EXPECTED_DRY_RUN_SHA256="$dry_run_sha256")
printf '%s\n' "$convert_output"
workspace_id=$(extract_field "$convert_output" workspace_id) \
    || fail 'conversion returned no workspace ID.'
conversion_sha256=$(extract_field "$convert_output" report_sha256) \
    || fail 'conversion returned no report digest.'
require_id "$workspace_id" wsp
require_sha256 "$conversion_sha256"

printf '%s\n' 'AD-T86-CANDIDATE: installed candidate prepared; no capability, approval or cutover was created.'
