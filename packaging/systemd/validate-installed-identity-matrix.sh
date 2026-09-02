#!/usr/bin/env bash
set -euo pipefail

script_dir=$(CDPATH= cd -- "$(dirname -- "$0")" && pwd)
project_root=$(CDPATH= cd -- "$script_dir/../.." && pwd)
fixture_source="$project_root/tests/fixtures/t76-standard-cargo"
cargo_home_path=${CARGO_HOME:-"$HOME/.cargo"}
installed_wrapper="$cargo_home_path/bin/antdelirium"
cargo_bin=$(command -v cargo || true)
run_id="$(date -u +%Y%m%dT%H%M%SZ)-${BASHPID:-$$}"
log_root="$project_root/target/t104-installed-identity-logs/$run_id"
fixture_root="$log_root/fixtures"
target_root="$log_root/cargo-targets"

fail() {
    printf '%s\n' "AD-T104-IDENTITY: $1" >&2
    exit 1
}

for command in cp cut date find git grep id jq mkdir sha256sum sleep sort stat systemctl; do
    command -v "$command" >/dev/null 2>&1 \
        || fail "required command is unavailable: $command"
done

[[ -n "$cargo_bin" ]] || fail 'cargo is required.'
[[ $(id -u) -ne 0 ]] || fail 'run this matrix as the non-root operator.'
[[ -f "$fixture_source/Cargo.lock" && ! -L "$fixture_source" ]] \
    || fail 'the maintained synthetic Rust fixture is unavailable.'
[[ ! -e "$fixture_source/guard" ]] \
    || fail 'the maintained fixture must not contain generated state.'
[[ -x "$installed_wrapper" && ! -L "$installed_wrapper" ]] \
    || fail 'the installed Cargo wrapper is unavailable.'

for unit in antdelirium.service antdelirium-agent.socket antdelirium-admin.socket; do
    [[ $(systemctl is-active "$unit" 2>/dev/null || true) == active ]] \
        || fail 'the installed service and activation sockets must be active.'
done

umask 077
mkdir -p "$fixture_root" "$target_root"

finish() {
    local exit_code=$?
    {
        printf 'contract=antdelirium-installed-identity-matrix-v1\n'
        printf 'run_id=%s\n' "$run_id"
        printf 'exit_code=%s\n' "$exit_code"
        printf 'physical_locators=redacted\n'
        printf 'external_project_access=forbidden\n'
        printf 'completed_at=%s\n' "$(date -u +%Y-%m-%dT%H:%M:%SZ)"
    } >"$log_root/result.env"
    printf '%s\n' "AD-T104-IDENTITY: logs=$log_root"
}
trap finish EXIT

copy_fixture() {
    local destination=$1
    mkdir -p "$destination"
    cp -R "$fixture_source/." "$destination/"
}

run_check() {
    local label=$1
    local workspace=$2
    local cargo_target=$3

    (
        cd "$workspace"
        env \
            -u RUSTC \
            -u RUSTC_WRAPPER \
            -u RUSTC_WORKSPACE_WRAPPER \
            -u CARGO_BUILD_RUSTC \
            -u CARGO_BUILD_RUSTC_WRAPPER \
            -u CARGO_BUILD_RUSTC_WORKSPACE_WRAPPER \
            -u ANTDELIRIUM_WRAPPER_DEPTH \
            "$cargo_bin" check --locked --target-dir "$cargo_target"
    ) >"$log_root/$label.stdout" 2>"$log_root/$label.stderr"
}

validate_public_projection() {
    local workspace=$1
    local guard_root="$workspace/guard"
    local todo="$guard_root/to-do_list.json"
    local errors="$guard_root/governance_error.json"

    for _attempt in {1..100}; do
        if [[ -f "$todo" && -f "$errors" ]]; then
            break
        fi
        sleep 0.1
    done

    [[ -d "$guard_root" && ! -L "$guard_root" ]] \
        || fail 'cargo check did not create the public guard directory.'
    mapfile -t entries < <(find "$guard_root" -mindepth 1 -maxdepth 1 -printf '%f\n' | sort)
    if [[ -e "$workspace/.git" ]]; then
        [[ ${#entries[@]} -eq 3 \
            && ${entries[0]} == .gitignore \
            && ${entries[1]} == governance_error.json \
            && ${entries[2]} == to-do_list.json ]] \
            || fail 'a Git fixture contains an unexpected guard entry or unfinished temporary.'
        [[ $(<"$guard_root/.gitignore") == $'/to-do_list.json\n/governance_error.json\n/.gitignore' ]] \
            || fail 'the generated guard ignore policy is not the reviewed contract.'
    else
        [[ ${#entries[@]} -eq 2 \
            && ${entries[0]} == governance_error.json \
            && ${entries[1]} == to-do_list.json ]] \
            || fail 'a non-Git fixture contains something other than the two public projections.'
    fi

    for projection in "$todo" "$errors"; do
        [[ -f "$projection" && ! -L "$projection" ]] \
            || fail 'a public projection is not a regular file.'
        [[ $(stat -c '%a' "$projection") == 600 ]] \
            || fail 'public projections must use mode 0600.'
        jq empty "$projection" || fail 'a public projection is not valid JSON.'
        if LC_ALL=C grep -Eq '/home/|/Users/|[A-Za-z]:\\Users\\|/var/|/run/|/tmp/' "$projection"; then
            fail 'a public projection disclosed a physical locator.'
        fi
    done

    jq -e '
        .contract_version == "agent-projection-v1" and
        .artifact_kind == "current_task" and
        (.project_ref | test("^prj_[0-9a-z]{26}$")) and
        (.workspace_ref | test("^wsp_[0-9a-z]{26}$"))
    ' "$todo" >/dev/null || fail 'the current-task projection violates its public identity contract.'
    jq -e '
        .contract_version == "agent-projection-v1" and
        .artifact_kind == "governance_errors" and
        (.project_ref | test("^prj_[0-9a-z]{26}$")) and
        (.workspace_ref | test("^wsp_[0-9a-z]{26}$"))
    ' "$errors" >/dev/null || fail 'the governance-error projection violates its public identity contract.'
    [[ $(jq -r '.project_ref' "$todo") == $(jq -r '.project_ref' "$errors") ]] \
        || fail 'the projections disagree about project identity.'
    [[ $(jq -r '.workspace_ref' "$todo") == $(jq -r '.workspace_ref' "$errors") ]] \
        || fail 'the projections disagree about workspace identity.'
}

projection_ref() {
    local workspace=$1
    local field=$2
    jq -r ".$field" "$workspace/guard/to-do_list.json"
}

printf '%s\n' 'AD-T104-IDENTITY: running two same-name projects in parallel.'
same_a="$fixture_root/same-name-a/project"
same_b="$fixture_root/same-name-b/project"
copy_fixture "$same_a"
copy_fixture "$same_b"
git -C "$same_a" init -b main >"$log_root/git-same-name-a.stdout" 2>"$log_root/git-same-name-a.stderr"
git -C "$same_b" init -b main >"$log_root/git-same-name-b.stdout" 2>"$log_root/git-same-name-b.stderr"

set +e
run_check same-name-a "$same_a" "$target_root/same-name-a" &
same_a_pid=$!
run_check same-name-b "$same_b" "$target_root/same-name-b" &
same_b_pid=$!
wait "$same_a_pid"
same_a_status=$?
wait "$same_b_pid"
same_b_status=$?
set -e
[[ $same_a_status -eq 0 && $same_b_status -eq 0 ]] \
    || fail 'parallel cargo checks did not both succeed.'

validate_public_projection "$same_a"
validate_public_projection "$same_b"
same_a_project=$(projection_ref "$same_a" project_ref)
same_b_project=$(projection_ref "$same_b" project_ref)
[[ "$same_a_project" != "$same_b_project" ]] \
    || fail 'independent projects with the same package name shared a project identity.'

printf '%s\n' 'AD-T104-IDENTITY: proving a cache-complete check preserves projections.'
before_cache=$(sha256sum \
    "$same_a/guard/to-do_list.json" \
    "$same_a/guard/governance_error.json" | cut -d ' ' -f 1 | sort)
run_check same-name-a-cached "$same_a" "$target_root/same-name-a" \
    || fail 'the cache-complete cargo check failed.'
after_cache=$(sha256sum \
    "$same_a/guard/to-do_list.json" \
    "$same_a/guard/governance_error.json" | cut -d ' ' -f 1 | sort)
[[ "$before_cache" == "$after_cache" ]] \
    || fail 'a cache-complete cargo check corrupted or changed the public projections.'

printf '%s\n' 'AD-T104-IDENTITY: creating two linked worktrees from one synthetic repository.'
worktree_main="$fixture_root/worktree-main"
worktree_linked="$fixture_root/worktree-linked"
copy_fixture "$worktree_main"
git -C "$worktree_main" init -b main >"$log_root/git-init.stdout" 2>"$log_root/git-init.stderr"
git -C "$worktree_main" -c user.name=AntDelirium -c user.email=antdelirium.invalid \
    add Cargo.toml Cargo.lock .cargo/config.toml src/main.rs
git -C "$worktree_main" -c user.name=AntDelirium -c user.email=antdelirium.invalid \
    commit -m fixture >"$log_root/git-commit.stdout" 2>"$log_root/git-commit.stderr"
git -C "$worktree_main" worktree add -b linked "$worktree_linked" \
    >"$log_root/git-worktree.stdout" 2>"$log_root/git-worktree.stderr"

run_check worktree-main "$worktree_main" "$target_root/worktree-main" \
    || fail 'cargo check failed in the main synthetic worktree.'
run_check worktree-linked "$worktree_linked" "$target_root/worktree-linked" \
    || fail 'cargo check failed in the linked synthetic worktree.'
validate_public_projection "$worktree_main"
validate_public_projection "$worktree_linked"

main_project=$(projection_ref "$worktree_main" project_ref)
linked_project=$(projection_ref "$worktree_linked" project_ref)
main_workspace=$(projection_ref "$worktree_main" workspace_ref)
linked_workspace=$(projection_ref "$worktree_linked" workspace_ref)
[[ "$main_project" == "$linked_project" ]] \
    || fail 'linked worktrees did not share the same project identity.'
[[ "$main_workspace" != "$linked_workspace" ]] \
    || fail 'linked worktrees did not receive distinct workspace identities.'

{
    printf 'contract=antdelirium-installed-identity-evidence-v1\n'
    printf 'physical_locators=redacted\n'
    printf 'external_project_access=forbidden\n'
    printf 'same_name_project_count=2\n'
    printf 'same_name_identity_separation=pass\n'
    printf 'parallel_cargo_check=pass\n'
    printf 'cache_complete_check=pass\n'
    printf 'cache_projection_stability=pass\n'
    printf 'linked_worktree_project_sharing=pass\n'
    printf 'linked_worktree_workspace_separation=pass\n'
    printf 'public_projection_contract=pass\n'
} >"$log_root/evidence.env"

printf '%s\n' 'AD-T104-IDENTITY: installed identity, worktree, cache and parallel-build matrix passed.'
