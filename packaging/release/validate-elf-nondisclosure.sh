#!/usr/bin/env bash
set -euo pipefail

script_dir=$(CDPATH= cd -- "$(dirname -- "$0")" && pwd)
project_root=$(CDPATH= cd -- "$script_dir/../.." && pwd)
binary="$project_root/target/x86_64-unknown-linux-gnu/release/antdelirium"
evidence_schema="$project_root/docs/contracts/v1/release-elf-audit-v1.schema.json"
run_id="$(date -u +%Y%m%dT%H%M%SZ)-${BASHPID:-$$}"
log_root="$project_root/target/t93-release-elf-audit/$run_id"

fail() {
    printf '%s\n' "AD-T93: $1" >&2
    exit 1
}

finish() {
    local exit_code=$?
    set +e
    {
        printf 'contract=antdelirium-release-elf-audit-v1\n'
        printf 'run_id=%s\n' "$run_id"
        printf 'exit_code=%s\n' "$exit_code"
        printf 'physical_locators=redacted\n'
        printf 'external_project_access=forbidden\n'
        printf 'completed_at=%s\n' "$(date -u +%Y-%m-%dT%H:%M:%SZ)"
    } >"$log_root/result.env"
    printf '%s\n' "AD-T93: logs=$log_root"
    exit "$exit_code"
}

for command in cut date file jq mkdir readelf rg sha256sum stat strings tr wc; do
    command -v "$command" >/dev/null 2>&1 \
        || fail "required command is unavailable: $command"
done
[[ -x "$binary" && ! -L "$binary" ]] \
    || fail 'the reviewed native release is unavailable.'
[[ -f "$evidence_schema" ]] || fail 'the ELF evidence contract is unavailable.'
jq empty "$evidence_schema" || fail 'the ELF evidence contract is invalid.'

umask 077
mkdir -p "$log_root"
trap finish EXIT

file "$binary" >"$log_root/file.txt"
readelf -hW "$binary" >"$log_root/header.txt"
readelf -SW "$binary" >"$log_root/sections.txt"
readelf -lW "$binary" >"$log_root/segments.txt"
readelf -dW "$binary" >"$log_root/dynamic.txt"

rg -q 'ELF 64-bit LSB pie executable.*stripped' "$log_root/file.txt" \
    || fail 'the release is not a stripped 64-bit PIE ELF.'
rg -q '^  Class:[[:space:]]+ELF64$' "$log_root/header.txt" \
    || fail 'the release ELF class is not ELF64.'
rg -q '^  Type:[[:space:]]+DYN ' "$log_root/header.txt" \
    || fail 'the release is not a position-independent DYN executable.'
rg -q '^  Machine:[[:space:]]+Advanced Micro Devices X86-64$' "$log_root/header.txt" \
    || fail 'the release machine does not match the reviewed native target.'

if rg -q '\.(symtab|strtab|debug[^[:space:]]*|gnu_debuglink|comment)([[:space:]]|$)' \
    "$log_root/sections.txt"; then
    fail 'the release retains static symbols, debug data or build-comment metadata.'
fi
if rg -q '\((RPATH|RUNPATH)\)' "$log_root/dynamic.txt"; then
    fail 'the release contains a runtime library search path.'
fi
rg -q 'GNU_RELRO' "$log_root/segments.txt" \
    || fail 'the release does not contain a GNU_RELRO segment.'
rg -q 'BIND_NOW' "$log_root/dynamic.txt" \
    || fail 'the release does not enable immediate binding for full RELRO.'
rg -q 'FLAGS_1.*NOW PIE' "$log_root/dynamic.txt" \
    || fail 'the release dynamic flags do not attest NOW and PIE.'
stack_line=$(rg '^[[:space:]]+GNU_STACK[[:space:]]' "$log_root/segments.txt" || true)
[[ -n "$stack_line" && "$stack_line" != *RWE* ]] \
    || fail 'the release requests an executable process stack.'

physical_pattern='/home/|/Users/|[A-Za-z]:\\Users\\|/root/|/checkout/|/builds?/|/workspace/|AntigravityProjects|Projects/AC-Ant-Delirium|/var/lib/antdelirium|/run/antdelirium'
predictable_log_pattern='/tmp/delirium_|delirium_(debug|error)\.log'
physical_locator_match_count=0
predictable_log_match_count=0
physical_match=$(strings -a "$binary" | rg -m 1 "$physical_pattern" || true)
predictable_log_match=$(strings -a "$binary" | rg -m 1 "$predictable_log_pattern" || true)
if [[ -n "$physical_match" ]]; then
    physical_locator_match_count=1
fi
if [[ -n "$predictable_log_match" ]]; then
    predictable_log_match_count=1
fi
[[ $physical_locator_match_count -eq 0 ]] \
    || fail 'the release contains a physical maintainer, build or protected-service locator.'
[[ $predictable_log_match_count -eq 0 ]] \
    || fail 'the release contains a predictable global log locator.'

administrative_marker_count=$(
    strings -a "$binary" \
        | rg -o 'task_orchestrator\.json|connected\.json|manager_tasks\.json|submit_queue\.json|default_hierarchy\.toml|default_rules\.toml' \
        | wc -l \
        | tr -d ' '
) || true
administrative_marker_count=${administrative_marker_count:-0}
binary_sha256=$(sha256sum "$binary" | cut -d ' ' -f 1)
binary_size_bytes=$(stat -c '%s' "$binary")

jq -n \
    --arg binary_sha256 "$binary_sha256" \
    --argjson binary_size_bytes "$binary_size_bytes" \
    --argjson administrative_marker_count "$administrative_marker_count" \
    '{
        contract_version: "release-elf-audit-v1",
        result: "passed",
        external_project_access: "forbidden",
        binary_sha256: $binary_sha256,
        binary_size_bytes: $binary_size_bytes,
        elf_class: "ELF64",
        elf_machine: "Advanced Micro Devices X86-64",
        elf_type: "DYN",
        stripped: true,
        static_symbol_table: false,
        debug_sections: false,
        comment_section: false,
        rpath: false,
        runpath: false,
        pie: true,
        relro: "full",
        executable_stack: false,
        physical_locator_match_count: 0,
        predictable_log_match_count: 0,
        administrative_marker_count: $administrative_marker_count,
        administrative_marker_classification: "generic-names-no-physical-locator",
        legacy_marker_removal: "t10.2-t10.3",
        enforcement_activated: false
    }' >"$log_root/release-elf-audit-v1.json"

jq -e '
    .contract_version == "release-elf-audit-v1" and
    .result == "passed" and
    .external_project_access == "forbidden" and
    (.binary_sha256 | test("^[0-9a-f]{64}$")) and
    .binary_size_bytes > 0 and
    .elf_class == "ELF64" and
    .elf_machine == "Advanced Micro Devices X86-64" and
    .elf_type == "DYN" and
    .stripped == true and
    .static_symbol_table == false and
    .debug_sections == false and
    .comment_section == false and
    .rpath == false and
    .runpath == false and
    .pie == true and
    .relro == "full" and
    .executable_stack == false and
    .physical_locator_match_count == 0 and
    .predictable_log_match_count == 0 and
    (.administrative_marker_count | type == "number") and
    .administrative_marker_classification == "generic-names-no-physical-locator" and
    .legacy_marker_removal == "t10.2-t10.3" and
    .enforcement_activated == false
' "$log_root/release-elf-audit-v1.json" >/dev/null \
    || fail 'the release ELF evidence violates its closed contract.'

printf '%s\n' 'AD-T93: stripped PIE/full-RELRO release contains no physical or protected locator.'
