#!/usr/bin/env bash
set -euo pipefail

script_dir=$(CDPATH= cd -- "$(dirname -- "$0")" && pwd)
project_root=$(CDPATH= cd -- "$script_dir/../.." && pwd)
schema="$project_root/docs/contracts/v1/legacy-source-inventory-v1.schema.json"

fail() {
    printf '%s\n' "AD-T82: $1" >&2
    exit 1
}

[[ $# -eq 2 ]] || {
    printf '%s\n' \
        'usage: validate-legacy-source-inventory.sh <absolute-source-root> <absolute-output.json>' >&2
    exit 64
}

source_argument=$1
output_path=$2

for required_command in find sort stat sha256sum jq rg git realpath cmp; do
    command -v "$required_command" >/dev/null 2>&1 \
        || fail "$required_command is required for the source inventory."
done

[[ "$source_argument" == /* ]] || fail 'the source root must be absolute.'
[[ "$output_path" == /* ]] || fail 'the output path must be absolute.'
[[ ! -L "$source_argument" ]] || fail 'the source root cannot be a symlink.'
[[ -d "$source_argument" ]] || fail 'the source root is unavailable.'
[[ -r "$schema" ]] || fail 'the inventory schema is unavailable.'
jq empty "$schema" || fail 'the inventory schema is invalid JSON.'

source_root=$(realpath -e -- "$source_argument")
output_parent=$(dirname -- "$output_path")
[[ -d "$output_parent" ]] || fail 'the output parent must already exist.'
output_parent=$(realpath -e -- "$output_parent")
output_path="$output_parent/$(basename -- "$output_path")"
case "$output_path" in
    "$source_root"|"$source_root"/*) fail 'the evidence cannot be written inside the source.' ;;
esac
[[ ! -e "$output_path" ]] || fail 'the output already exists and will not be replaced.'

probe_id="${BASHPID:-$$}-${RANDOM}"
probe_root="$project_root/target/t82-legacy-source-inventory/$probe_id"
mkdir -p "$probe_root"
umask 077

metadata_signature() {
    stat -c '%d:%i:%s:%f:%u:%g:%y:%z' -- "$1"
}

entry_type() {
    local path=$1
    if [[ -L "$path" ]]; then
        printf '%s' symlink
    elif [[ -f "$path" ]]; then
        printf '%s' regular
    elif [[ -d "$path" ]]; then
        printf '%s' directory
    elif [[ -p "$path" ]]; then
        printf '%s' fifo
    elif [[ -S "$path" ]]; then
        printf '%s' socket
    elif [[ -b "$path" ]]; then
        printf '%s' block-device
    elif [[ -c "$path" ]]; then
        printf '%s' character-device
    else
        printf '%s' other
    fi
}

build_entries() {
    local destination=$1
    local json_count=0
    local json_non_regular_count=0
    local path relative type before after mode uid gid size digest
    local link_target link_target_kind link_target_digest
    local json_lines="$destination.jsonl"
    : >"$json_lines"

    while IFS= read -r -d '' path; do
        relative=${path#"$source_root"/}
        case "$relative" in
            *$'\n'*|*$'\r'*) fail 'source paths containing line breaks are unsupported.' ;;
        esac

        before=$(metadata_signature "$path") \
            || fail 'an entry disappeared during inventory.'
        type=$(entry_type "$path")
        mode=$(stat -c '%a' -- "$path")
        uid=$(stat -c '%u' -- "$path")
        gid=$(stat -c '%g' -- "$path")
        size=$(stat -c '%s' -- "$path")
        digest=
        link_target=
        link_target_kind=none
        link_target_digest=

        if [[ "$type" == regular ]]; then
            digest=$(sha256sum -- "$path" | cut -d ' ' -f 1) \
                || fail 'a regular file could not be hashed.'
        elif [[ "$type" == symlink ]]; then
            link_target=$(readlink -- "$path") \
                || fail 'a symlink target could not be read.'
            link_target_digest=$(printf '%s' "$link_target" | sha256sum | cut -d ' ' -f 1)
            if [[ "$link_target" == /* ]]; then
                link_target_kind=absolute-redacted
                link_target=
            else
                link_target_kind=relative
            fi
        fi

        after=$(metadata_signature "$path") \
            || fail 'an entry disappeared after it was inspected.'
        [[ "$before" == "$after" ]] \
            || fail 'the source changed while an entry was being inspected.'

        case "${relative,,}" in
            *.json)
                json_count=$((json_count + 1))
                if [[ "$type" != regular ]]; then
                    json_non_regular_count=$((json_non_regular_count + 1))
                elif ! jq empty "$path"; then
                    fail "invalid JSON at relative path: $relative"
                fi
                ;;
        esac

        jq -cn \
            --arg path "$relative" \
            --arg type "$type" \
            --arg mode "$mode" \
            --argjson uid "$uid" \
            --argjson gid "$gid" \
            --argjson size "$size" \
            --arg digest "$digest" \
            --arg link_target_kind "$link_target_kind" \
            --arg link_target "$link_target" \
            --arg link_target_digest "$link_target_digest" \
            '{
                path: $path,
                type: $type,
                mode: $mode,
                uid: $uid,
                gid: $gid,
                size: $size,
                sha256: (if $digest == "" then null else $digest end),
                link: {
                    kind: $link_target_kind,
                    target: (if $link_target == "" then null else $link_target end),
                    target_sha256: (if $link_target_digest == "" then null else $link_target_digest end)
                }
            }' >>"$json_lines"
    done < <(find "$source_root" -mindepth 1 -print0 | sort -z)

    [[ $json_non_regular_count -eq 0 ]] \
        || fail 'every path ending in .json must be a regular file.'
    jq -s 'sort_by(.path)' "$json_lines" >"$destination"
    printf '%s\n' "$json_count" >"$destination.json-count"
}

require_array_length() {
    local path=$1
    local selector=$2
    jq -er "$selector | if type == \"array\" then length else error(\"expected array\") end" "$path" \
        || fail "legacy semantic collection is invalid: ${path#"$source_root"/}"
}

build_semantic_counts() {
    local destination=$1
    local plans=0
    local tasks=0
    local task_count path
    local submissions reviews history_events problems transitions
    local schemas artifacts evidence

    for required in \
        "$source_root/plan" \
        "$source_root/task_orchestrator.json" \
        "$source_root/submit_queue.json" \
        "$source_root/governance_history.json" \
        "$source_root/global/transition_task.json" \
        "$source_root/problems"; do
        [[ -e "$required" ]] || fail 'a required legacy semantic surface is absent.'
    done

    while IFS= read -r -d '' path; do
        plans=$((plans + 1))
        task_count=$(require_array_length "$path" '.tasks')
        tasks=$((tasks + task_count))
    done < <(find "$source_root/plan" -type f -name '*.json' -print0 | sort -z)

    submissions=$(require_array_length "$source_root/submit_queue.json" '.queue')
    reviews=$(jq -er \
        '[.queue[]? | select((.review_status? != null) or (.review? != null) or (.reviewed? == true))] | length' \
        "$source_root/submit_queue.json") \
        || fail 'the legacy review collection is invalid.'
    history_events=$(require_array_length "$source_root/governance_history.json" '.entries')
    transitions=$(require_array_length "$source_root/global/transition_task.json" '.transition_tasks')
    problems=$(find "$source_root/problems" -type f -name '*.json' -printf x | wc -c)
    schemas=$(find "$source_root/project_state/schemas" -type f -name '*.schema.json' -printf x | wc -c)
    artifacts=$(find "$source_root/project_state/AutoBrowser_props/Logs/artifacts" -type f -printf x | wc -c)
    evidence=$(find "$source_root/project_state/AutoBrowser_props/evidence" -type f -printf x | wc -c)

    jq -cn \
        --argjson plans "$plans" \
        --argjson tasks "$tasks" \
        --argjson submissions "$submissions" \
        --argjson reviews "$reviews" \
        --argjson history_events "$history_events" \
        --argjson problems "$problems" \
        --argjson transitions "$transitions" \
        --argjson schemas "$schemas" \
        --argjson artifacts "$artifacts" \
        --argjson evidence "$evidence" \
        '{
            plans: $plans,
            tasks: $tasks,
            submissions: $submissions,
            reviews: $reviews,
            history_events: $history_events,
            problems: $problems,
            transitions: $transitions,
            json_schemas: $schemas,
            generated_artifacts: $artifacts,
            task_evidence: $evidence
        }' >"$destination"
}

build_git_state() {
    local destination=$1
    local head branch status_file status_json repository
    status_file="$destination.status"
    if repository=$(GIT_OPTIONAL_LOCKS=0 git -C "$source_root" rev-parse --show-toplevel 2>/dev/null) \
        && [[ "$repository" == "$source_root" ]]; then
        head=$(GIT_OPTIONAL_LOCKS=0 git -C "$source_root" rev-parse HEAD)
        branch=$(GIT_OPTIONAL_LOCKS=0 git -C "$source_root" branch --show-current)
        GIT_OPTIONAL_LOCKS=0 git -C "$source_root" status \
            --porcelain=v1 --untracked-files=all >"$status_file"
        status_json=$(jq -Rsc 'split("\n") | map(select(length > 0))' "$status_file")
        jq -cn \
            --arg head "$head" \
            --arg branch "$branch" \
            --argjson status "$status_json" \
            '{repository: true, head: $head, branch: $branch, worktree_clean: ($status | length == 0), status: $status}' \
            >"$destination"
    else
        jq -n '{repository: false, head: null, branch: null, worktree_clean: null, status: []}' \
            >"$destination"
    fi
}

build_payload() {
    local destination=$1
    local entries="$destination.entries.json"
    local semantics="$destination.semantics.json"
    local git_state="$destination.git.json"
    local json_count root_mode root_uid root_gid

    build_entries "$entries"
    build_semantic_counts "$semantics"
    build_git_state "$git_state"
    json_count=$(<"$entries.json-count")
    root_mode=$(stat -c '%a' -- "$source_root")
    root_uid=$(stat -c '%u' -- "$source_root")
    root_gid=$(stat -c '%g' -- "$source_root")

    jq -nS \
        --arg source_kind legacy-plugin \
        --arg root_mode "$root_mode" \
        --argjson root_uid "$root_uid" \
        --argjson root_gid "$root_gid" \
        --argjson json_count "$json_count" \
        --slurpfile entries "$entries" \
        --slurpfile semantics "$semantics" \
        --slurpfile git "$git_state" \
        '{
            source_kind: $source_kind,
            source_locator: "operator-private",
            root: {type: "directory", mode: $root_mode, uid: $root_uid, gid: $root_gid},
            validation: {json_files: $json_count, valid_json_files: $json_count, invalid_json_files: 0},
            counts: {
                entries: ($entries[0] | length),
                regular_files: ([$entries[0][] | select(.type == "regular")] | length),
                directories: ([$entries[0][] | select(.type == "directory")] | length),
                symlinks: ([$entries[0][] | select(.type == "symlink")] | length),
                other: ([$entries[0][] | select(.type != "regular" and .type != "directory" and .type != "symlink")] | length)
            },
            mode_histogram: ($entries[0] | group_by(.mode) | map({mode: .[0].mode, count: length})),
            ownership_histogram: ($entries[0] | group_by([.uid, .gid]) | map({uid: .[0].uid, gid: .[0].gid, count: length})),
            semantic_counts: $semantics[0],
            git: $git[0],
            entries: $entries[0]
        }' >"$destination"
}

first_payload="$probe_root/payload-first.json"
second_payload="$probe_root/payload-second.json"
build_payload "$first_payload"
build_payload "$second_payload"
cmp -s "$first_payload" "$second_payload" \
    || fail 'two consecutive read-only passes observed different source state.'

inventory_sha256=$(jq -cS . "$first_payload" | sha256sum | cut -d ' ' -f 1)
captured_at=$(date --iso-8601=seconds)
temporary_output="$output_path.$probe_id.tmp"
jq -nS \
    --arg contract_version legacy-source-inventory-v1 \
    --arg captured_at "$captured_at" \
    --arg inventory_sha256 "$inventory_sha256" \
    --slurpfile payload "$first_payload" \
    '{
        contract_version: $contract_version,
        captured_at: $captured_at,
        inventory_sha256: $inventory_sha256,
        consistency: "two-identical-read-only-passes",
        payload: $payload[0]
    }' >"$temporary_output"

if rg -F -q -- "$source_root" "$temporary_output"; then
    fail 'the evidence contains the physical source locator.'
fi
jq -e \
    '.contract_version == "legacy-source-inventory-v1"
     and .consistency == "two-identical-read-only-passes"
     and (.inventory_sha256 | test("^[0-9a-f]{64}$"))
     and .payload.source_locator == "operator-private"
     and .payload.validation.invalid_json_files == 0
     and .payload.validation.json_files == .payload.validation.valid_json_files
     and .payload.counts.entries == (.payload.entries | length)' \
    "$temporary_output" >/dev/null \
    || fail 'the generated source inventory violates its closed contract.'

chmod 0644 "$temporary_output"
mv "$temporary_output" "$output_path"
printf '%s\n' "AD-T82: source inventory validated and written with digest $inventory_sha256"
