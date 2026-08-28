#!/usr/bin/env bash
set -euo pipefail

script_dir=$(CDPATH= cd -- "$(dirname -- "$0")" && pwd)
project_root=$(CDPATH= cd -- "$script_dir/../.." && pwd)
service_unit=antdelirium.service
service_account=antdelirium
state_root=/var/lib/antdelirium
state_database="$state_root/registry/state/state.db"
evidence_schema="$project_root/docs/contracts/v1/hostile-isolation-evidence-v1.schema.json"
run_id="$(date -u +%Y%m%dT%H%M%SZ)-${BASHPID:-$$}"
log_root="$project_root/target/t94-hostile-isolation/$run_id"
result=failed
state_root_metadata=unchecked
host_process_metadata=unchecked

fail() {
    printf '%s\n' "AD-T94: $1" >&2
    exit 1
}

finish() {
    local exit_code=$?
    set +e
    {
        printf 'contract=antdelirium-hostile-isolation-v1\n'
        printf 'run_id=%s\n' "$run_id"
        printf 'exit_code=%s\n' "$exit_code"
        printf 'result=%s\n' "$result"
        printf 'physical_locators=redacted\n'
        printf 'external_project_access=forbidden\n'
        printf 'source_mutation=forbidden\n'
        printf 'completed_at=%s\n' "$(date -u +%Y-%m-%dT%H:%M:%SZ)"
    } >"$log_root/result.env"
    printf '%s\n' "AD-T94: logs=$log_root"
    exit "$exit_code"
}

for command in bash cargo date head id jq ls readlink rg stat strace systemctl timeout; do
    command -v "$command" >/dev/null 2>&1 \
        || fail "required command is unavailable: $command"
done
[[ $EUID -ne 0 ]] \
    || fail 'run this hostile-client proof as the ordinary operator, never with sudo.'
[[ -f "$evidence_schema" ]] || fail 'the hostile-isolation evidence contract is unavailable.'
jq empty "$evidence_schema" || fail 'the hostile-isolation evidence contract is invalid.'

umask 077
mkdir -p "$log_root"
trap finish EXIT

printf '%s\n' 'AD-T94: proving client filesystem isolation.'
bash "$script_dir/validate-client-isolation.sh" \
    >"$log_root/client-isolation.stdout" \
    2>"$log_root/client-isolation.stderr" \
    || fail 'the disposable client-isolation gate did not pass.'

printf '%s\n' 'AD-T94: tracing the complete wrapper process tree.'
bash "$script_dir/validate-wrapper-syscalls.sh" \
    >"$log_root/wrapper-syscalls.stdout" \
    2>"$log_root/wrapper-syscalls.stderr" \
    || fail 'the wrapper syscall nondisclosure gate did not pass.'

printf '%s\n' 'AD-T94: proving close-on-exec for inherited descriptors.'
env \
    -u RUSTC_WRAPPER \
    -u RUSTC_WORKSPACE_WRAPPER \
    -u ANTDELIRIUM_WRAPPER_DEPTH \
    cargo test \
        --locked \
        --target x86_64-unknown-linux-gnu \
        service_bootstrap_applies_process_and_descriptor_hardening_in_a_child \
        -- --nocapture \
        >"$log_root/descriptor-test.stdout" \
        2>"$log_root/descriptor-test.stderr" \
    || fail 'the inherited-descriptor exec probe did not pass.'
rg -q 'test result: ok.*1 passed' "$log_root/descriptor-test.stdout" \
    || fail 'the inherited-descriptor test result was inconclusive.'

printf '%s\n' 'AD-T94: probing the installed service from the ordinary operator account.'
[[ $(systemctl is-active "$service_unit") == active ]] \
    || fail 'the installed protected service is not active.'

unit_value() {
    systemctl show "$service_unit" --property "$1" --value
}

service_pid=$(unit_value MainPID)
[[ "$service_pid" =~ ^[0-9]+$ && "$service_pid" -gt 1 ]] \
    || fail 'the installed service has no stable main process.'
[[ $(unit_value User) == "$service_account" ]] \
    || fail 'the installed service does not use its dedicated account.'
[[ $(unit_value Group) == "$service_account" ]] \
    || fail 'the installed service does not use its dedicated group.'
service_uid=$(id -u "$service_account")
[[ "$service_uid" != "$(id -u)" ]] \
    || fail 'the operator and protected service unexpectedly share a uid.'
[[ $(stat -c '%u' "/proc/$service_pid") == "$service_uid" ]] \
    || fail 'the live process owner does not match the protected service account.'

for property_and_value in \
    NoNewPrivileges=yes \
    LimitCORE=0 \
    ProtectProc=invisible \
    ProcSubset=pid \
    ProtectSystem=strict \
    ProtectHome=yes \
    PrivateTmp=yes \
    PrivateDevices=yes \
    PrivateNetwork=yes; do
    property=${property_and_value%%=*}
    expected=${property_and_value#*=}
    [[ $(unit_value "$property") == "$expected" ]] \
        || fail "installed unit property $property does not match the reviewed contract."
done
[[ -z $(unit_value CapabilityBoundingSet) ]] \
    || fail 'the installed service retains a capability bounding set.'
[[ -z $(unit_value AmbientCapabilities) ]] \
    || fail 'the installed service retains ambient capabilities.'

expect_denied() {
    local label=$1
    shift
    set +e
    timeout 2 "$@" >/dev/null 2>&1
    local status=$?
    set -e
    [[ $status -ne 0 && $status -ne 124 ]] \
        || fail "$label was reachable or the probe was inconclusive."
}

expect_denied 'service environ through proc' head -c 1 "/proc/$service_pid/environ"
expect_denied 'service maps through proc' head -c 1 "/proc/$service_pid/maps"
expect_denied 'service memory through proc' head -c 1 "/proc/$service_pid/mem"
expect_denied 'service descriptor enumeration through proc' ls -A "/proc/$service_pid/fd"
expect_denied 'service descriptor resolution through proc' readlink "/proc/$service_pid/fd/0"
expect_denied 'service cwd resolution through proc' readlink "/proc/$service_pid/cwd"
expect_denied 'service root resolution through proc' readlink "/proc/$service_pid/root"
expect_denied 'service executable resolution through proc' readlink "/proc/$service_pid/exe"

if head -c 1 "/proc/$service_pid/status" >/dev/null 2>&1; then
    host_process_metadata=limited-visible
else
    host_process_metadata=hidden
fi

if stat -- "$state_root" >/dev/null 2>&1; then
    state_root_metadata=metadata-visible
else
    state_root_metadata=hidden
fi
expect_denied 'protected state read access' test -r "$state_root"
expect_denied 'protected state traversal access' test -x "$state_root"
expect_denied 'protected state enumeration' ls -A "$state_root"
expect_denied 'protected state database open' head -c 1 "$state_database"

set +e
LC_ALL=C timeout 2 strace \
    -qq \
    -p "$service_pid" \
    -e trace=none \
    -o /dev/null \
    >"$log_root/strace.stdout" \
    2>"$log_root/strace.stderr"
strace_status=$?
set -e
[[ $strace_status -ne 0 && $strace_status -ne 124 ]] \
    || fail 'strace attached to the protected service.'
rg -q 'ptrace\(PTRACE_(SEIZE|ATTACH).*Operation not permitted' "$log_root/strace.stderr" \
    || fail 'the ptrace denial was not proven by the kernel diagnostic.'
[[ $(systemctl is-active "$service_unit") == active ]] \
    || fail 'the service stopped during the hostile probe.'
[[ $(unit_value MainPID) == "$service_pid" ]] \
    || fail 'the service process changed during the hostile probe.'

jq -n \
    --arg state_root_metadata "$state_root_metadata" \
    --arg host_process_metadata "$host_process_metadata" \
    '{
        contract_version: "hostile-isolation-evidence-v1",
        result: "passed",
        external_project_access: "forbidden",
        source_mutation: "forbidden",
        client_namespace_isolation: "passed",
        wrapper_syscall_nondisclosure: "passed",
        service_account_separation: "passed",
        installed_unit_hardening: "passed",
        proc_environ_read: "denied",
        proc_maps_read: "denied",
        proc_mem_read: "denied",
        proc_fd_enumeration: "denied",
        proc_fd_readlink: "denied",
        proc_cwd_readlink: "denied",
        proc_root_readlink: "denied",
        proc_exe_readlink: "denied",
        host_process_metadata: $host_process_metadata,
        state_root_metadata: $state_root_metadata,
        state_read_access: "denied",
        state_traversal_access: "denied",
        state_enumeration: "denied",
        state_database_open: "denied",
        strace_attach: "denied",
        ptrace_kernel_attach: "denied",
        inherited_descriptor_exec: "closed"
    }' >"$log_root/hostile-isolation-evidence-v1.json"

jq -e '
    .contract_version == "hostile-isolation-evidence-v1" and
    .result == "passed" and
    .external_project_access == "forbidden" and
    .source_mutation == "forbidden" and
    .client_namespace_isolation == "passed" and
    .wrapper_syscall_nondisclosure == "passed" and
    .service_account_separation == "passed" and
    .installed_unit_hardening == "passed" and
    .proc_environ_read == "denied" and
    .proc_maps_read == "denied" and
    .proc_mem_read == "denied" and
    .proc_fd_enumeration == "denied" and
    .proc_fd_readlink == "denied" and
    .proc_cwd_readlink == "denied" and
    .proc_root_readlink == "denied" and
    .proc_exe_readlink == "denied" and
    (.host_process_metadata == "limited-visible" or .host_process_metadata == "hidden") and
    (.state_root_metadata == "metadata-visible" or .state_root_metadata == "hidden") and
    .state_read_access == "denied" and
    .state_traversal_access == "denied" and
    .state_enumeration == "denied" and
    .state_database_open == "denied" and
    .strace_attach == "denied" and
    .ptrace_kernel_attach == "denied" and
    .inherited_descriptor_exec == "closed"
' "$log_root/hostile-isolation-evidence-v1.json" >/dev/null \
    || fail 'the hostile-isolation evidence violates its closed contract.'

result=passed
printf '%s\n' 'AD-T94: client, service, proc, ptrace and descriptor isolation passed.'
