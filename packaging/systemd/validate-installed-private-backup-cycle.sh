#!/usr/bin/env bash
set -euo pipefail

script_dir=$(CDPATH= cd -- "$(dirname -- "$0")" && pwd)
project_root=$(CDPATH= cd -- "$script_dir/../.." && pwd)
run_id=$(date -u +%Y%m%dT%H%M%SZ)-${BASHPID:-$$}
log_root="$project_root/target/t98-installed-private-backup-logs/$run_id"
release_binary="$project_root/target/x86_64-unknown-linux-gnu/release/antdelirium"
bundle_root="$project_root/target/t86-system-installation-bundle"
bundle_binary="$bundle_root/usr/libexec/antdelirium/antdelirium"
bundle_manifest="$bundle_root/system-installation-bundle-v1.json"
installed_service=/usr/libexec/antdelirium/antdelirium
installed_wrapper="$HOME/.cargo/bin/antdelirium"
cargo_binary="$HOME/.cargo/bin/cargo"
limited_socket=/run/antdelirium/agent.sock
active_probe_projection=
active_probe_backup=
active_probe_temporary=

fail() {
    printf '%s\n' "AD-T98-INSTALLED: $1" >&2
    exit 1
}

finish() {
    local exit_code=$?
    set +e
    if [[ -n "$active_probe_projection" && -f "$active_probe_backup" ]]; then
        cp --preserve=mode "$active_probe_backup" "$active_probe_temporary"
        mv -f "$active_probe_temporary" "$active_probe_projection"
    fi
    {
        printf 'contract=antdelirium-installed-private-backup-cycle-v1\n'
        printf 'run_id=%s\n' "$run_id"
        printf 'exit_code=%s\n' "$exit_code"
        printf 'external_project_access=forbidden\n'
        printf 'physical_locators=redacted\n'
        printf 'completed_at=%s\n' "$(date -u +%Y-%m-%dT%H:%M:%SZ)"
    } >"$log_root/result.env"
    chmod -R a+rX "$log_root"
    printf '%s\n' "AD-T98-INSTALLED: logs=$log_root"
}

[[ $EUID -ne 0 ]] || fail 'execute this routine as the ordinary operator; it invokes sudo only for installation and protected operations.'
for command in sudo bash tee sed grep date mkdir chmod tail jq sha256sum cut find sort sleep setpriv id cp mv; do
    command -v "$command" >/dev/null 2>&1 \
        || fail "required command is unavailable: $command"
done
[[ -x "$cargo_binary" ]] \
    || fail 'the operator Cargo executable is unavailable.'
[[ -S "$limited_socket" ]] \
    || fail 'the limited service socket is unavailable.'
operator_uid=$(id -u)
operator_gid=$(id -g)

umask 077
mkdir -p "$log_root"
trap finish EXIT

printf '%s\n' 'AD-T98-INSTALLED: validating the already reviewed binary and installation bundle.'
[[ -x "$release_binary" && ! -L "$release_binary" \
    && -x "$bundle_binary" && ! -L "$bundle_binary" \
    && -f "$bundle_manifest" && ! -L "$bundle_manifest" ]] \
    || fail 'the reviewed release bundle is unavailable; run make system-installation-bundle as the ordinary user.'
expected_hash=$(jq -r '.files[] | select(.artifact == "service-binary") | .sha256' \
    "$bundle_manifest")
release_hash=$(sha256sum "$release_binary" | cut -d ' ' -f 1)
bundle_hash=$(sha256sum "$bundle_binary" | cut -d ' ' -f 1)
[[ "$expected_hash" =~ ^[0-9a-f]{64}$ \
    && "$release_hash" == "$expected_hash" \
    && "$bundle_hash" == "$expected_hash" ]] \
    || fail 'the prepared release and installation bundle do not have the reviewed digest.'

installed_hash=
wrapper_hash=
if [[ -x "$installed_service" && ! -L "$installed_service" ]]; then
    installed_hash=$(sha256sum "$installed_service" | cut -d ' ' -f 1)
fi
if [[ -x "$installed_wrapper" && ! -L "$installed_wrapper" ]]; then
    wrapper_hash=$(sha256sum "$installed_wrapper" | cut -d ' ' -f 1)
fi
if [[ "$installed_hash" == "$expected_hash" && "$wrapper_hash" == "$expected_hash" ]]; then
    printf '%s\n' 'AD-T98-INSTALLED: reviewed service and Cargo wrapper are already installed.'
else
    printf '%s\n' 'AD-T98-INSTALLED: installing the same binary as service and Cargo wrapper.'
    sudo bash "$script_dir/upgrade-installed-service.sh" 2>&1 \
        | tee "$log_root/upgrade.log"
fi

printf '%s\n' 'AD-T98-INSTALLED: refreshing registered fixtures before selecting empty or resumable canonical state.'
fixture_project=
resume_task_id=
candidate_ordinal=0
while IFS= read -r candidate; do
    candidate_ordinal=$((candidate_ordinal + 1))
    projection="$candidate/guard/to-do_list.json"
    if [[ -d "$candidate" && ! -L "$candidate" \
        && ! -e "$candidate/.git" && ! -L "$candidate/.git" \
        && -f "$candidate/src/main.rs" && ! -L "$candidate/src/main.rs" \
        && -f "$projection" && ! -L "$projection" ]]; then
        active_probe_projection=$projection
        active_probe_backup="$log_root/candidate-$candidate_ordinal.projection.original"
        active_probe_temporary="$candidate/guard/.to-do_list.refresh-$run_id"
        candidate_diagnostic="$log_root/candidate-$candidate_ordinal.audit"
        : >"$candidate_diagnostic"
        cp --preserve=mode "$projection" "$active_probe_backup"
        jq --arg probe "$run_id" \
            '. + {"__antdelirium_refresh_probe": $probe}' \
            "$projection" >"$active_probe_temporary"
        chmod --reference="$projection" "$active_probe_temporary"
        mv -f "$active_probe_temporary" "$projection"
        candidate_target="$log_root/candidate-$candidate_ordinal-target"
        mkdir -m 0700 "$candidate_target"
        if ! sudo setpriv \
            --reuid="$operator_uid" --regid="$operator_gid" --init-groups \
            --no-new-privs --inh-caps=-all --ambient-caps=-all --bounding-set=-all \
            --reset-env env \
            HOME="$HOME" \
            CARGO_HOME="$HOME/.cargo" \
            CARGO_TARGET_DIR="$candidate_target" \
            ANTDELIRIUM_LIMITED_SOCKET="$limited_socket" \
            bash -c '
            exec 9>>"$4"
            export ANTDELIRIUM_SUPERVISOR_DIAGNOSTIC_FD=9
            cd "$1"
            "$2" check || exit 1
            for _attempt in {1..100}; do
                if /usr/bin/grep -Fqx "audit=pass" "$4" && \
                    /usr/bin/jq -e '\''has("__antdelirium_refresh_probe") | not'\'' \
                        "$3" >/dev/null 2>&1; then
                    exit 0
                fi
                if /usr/bin/grep -Eq \
                    "^audit=(plan-unavailable|endpoint-configuration|internal-failure|projection-materialization-failure|(bootstrap|compile-observe|projection-refresh)-(endpoint-configuration|service-unavailable|temporary-failure|protocol-failure|internal-failure))$" \
                    "$4"; then
                    exit 2
                fi
                /usr/bin/sleep 0.05
            done
            exit 1
            ' bash "$candidate" "$cargo_binary" "$projection" "$candidate_diagnostic" \
            >"$log_root/candidate-$candidate_ordinal.stdout" \
            2>"$log_root/candidate-$candidate_ordinal.stderr"; then
            diagnostic_result=$(tail -n 1 "$candidate_diagnostic" 2>/dev/null || true)
            if [[ "$diagnostic_result" =~ ^audit=[a-z-]+$ ]]; then
                printf 'candidate_%s_diagnostic=%s\n' \
                    "$candidate_ordinal" "${diagnostic_result#audit=}" \
                    >>"$log_root/refresh-diagnostics.env"
            else
                printf 'candidate_%s_diagnostic=missing\n' \
                    "$candidate_ordinal" >>"$log_root/refresh-diagnostics.env"
            fi
            cp --preserve=mode "$active_probe_backup" "$active_probe_temporary"
            mv -f "$active_probe_temporary" "$active_probe_projection"
            active_probe_projection=
            active_probe_backup=
            active_probe_temporary=
            continue
        fi
        active_probe_projection=
        active_probe_backup=
        active_probe_temporary=
        if jq -e '
            .contract_version == "agent-projection-v1" and
            .artifact_kind == "current_task" and
            .source_revision == 0 and .task == null and
            .queue_status == "no_task_available"
        ' "$projection" >/dev/null 2>&1; then
            fixture_project=$candidate
            break
        fi
        if jq -e '
            .contract_version == "agent-projection-v1" and
            .artifact_kind == "current_task" and
            .source_revision == 1 and
            (.task.task_id | test("^tsk_[0-9a-hjkmnp-tv-z]{26}$")) and
            .task.task_revision == 1 and
            .queue_status == "task_available"
        ' "$projection" >/dev/null 2>&1; then
            fixture_project=$candidate
            resume_task_id=$(jq -r '.task.task_id' "$projection")
            break
        fi
    fi
done < <(
    find "$project_root/target/t8g-generic-installation-logs" \
        -mindepth 2 -maxdepth 2 -type d -name project -printf '%T@ %p\n' 2>/dev/null \
        | sort -rn | cut -d ' ' -f 2-
)
if [[ -z "$fixture_project" && -s "$log_root/refresh-diagnostics.env" ]]; then
    printf '%s\n' 'AD-T98-INSTALLED: audit worker diagnostics:' >&2
    sed 's/^/AD-T98-INSTALLED: /' "$log_root/refresh-diagnostics.env" >&2
fi
[[ -n "$fixture_project" ]] \
    || fail 'no authoritatively refreshed empty or resumable internal Rust fixture is available.'

printf '%s\n' 'AD-T98-INSTALLED: proving baseline, submission, review and advance through the installed service.'
cycle_arguments=(--project-root "$fixture_project")
if [[ -n "$resume_task_id" ]]; then
    cycle_arguments+=(--resume-task-id "$resume_task_id")
fi
sudo bash "$script_dir/validate-installed-task-cycle.sh" \
    "${cycle_arguments[@]}" 2>&1 \
    | tee "$log_root/task-cycle.log"
cycle_log=$(sed -n 's/^AD-T8G-CYCLE: logs=//p' "$log_root/task-cycle.log" | tail -n 1)
[[ "$cycle_log" == "$project_root"/target/t8g-task-cycle-logs/* \
    && -f "$cycle_log/acceptance.env" && ! -L "$cycle_log/acceptance.env" \
    && -f "$cycle_log/source-context.json" && ! -L "$cycle_log/source-context.json" ]] \
    || fail 'the installed private-backup cycle did not return complete evidence.'

for evidence in \
    'reviewer_source_context=pass' \
    'reviewer_source_change=src/main.rs:modified' \
    'private_checkpoint_cycle=baseline,submission,review,advance' \
    'project_local_git=absent' \
    'canonical_task_advanced=yes' \
    'canonical_plan_completed=yes'; do
    grep -Fqx "$evidence" "$cycle_log/acceptance.env" \
        || fail "installed evidence is missing: $evidence"
done
if grep -Eq '/home/|/Users/|[A-Za-z]:\\\\Users\\\\|/var/|/run/|/tmp/' \
    "$cycle_log/source-context.json"; then
    fail 'the reviewer source context disclosed a physical locator.'
fi

{
    printf 'contract=antdelirium-installed-private-backup-evidence-v1\n'
    printf 'single_binary_service_and_wrapper=pass\n'
    printf 'rust_fixture_without_git=pass\n'
    printf 'ordinary_cargo_check=pass\n'
    printf 'private_checkpoint_cycle=baseline,submission,review,advance\n'
    printf 'reviewer_source_context=pass\n'
    printf 'physical_locator_disclosure=none\n'
    printf 'external_project_access=forbidden\n'
} >"$log_root/acceptance.env"

printf '%s\n' 'AD-T98-INSTALLED: installed private backup and reviewer-context cycle passed.'
