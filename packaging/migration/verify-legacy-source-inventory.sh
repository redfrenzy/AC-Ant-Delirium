#!/usr/bin/env bash
set -euo pipefail

script_dir=$(CDPATH= cd -- "$(dirname -- "$0")" && pwd)
project_root=$(CDPATH= cd -- "$script_dir/../.." && pwd)
inventory=${1:-$project_root/docs/evidence/autobrowser-legacy-source-inventory-v1.json}
schema="$project_root/docs/contracts/v1/legacy-source-inventory-v1.schema.json"

fail() {
    printf '%s\n' "AD-T82-VERIFY: $1" >&2
    exit 1
}

for required_command in jq sha256sum rg; do
    command -v "$required_command" >/dev/null 2>&1 \
        || fail "$required_command is required to verify the inventory."
done
[[ -r "$inventory" ]] || fail 'the source inventory is unavailable.'
[[ -r "$schema" ]] || fail 'the source inventory schema is unavailable.'
jq empty "$schema" || fail 'the source inventory schema is invalid JSON.'
jq empty "$inventory" || fail 'the source inventory is invalid JSON.'

recorded_sha256=$(jq -er '.inventory_sha256' "$inventory") \
    || fail 'the inventory digest is absent.'
computed_sha256=$(jq -cS '.payload' "$inventory" | sha256sum | cut -d ' ' -f 1)
[[ "$recorded_sha256" == "$computed_sha256" ]] \
    || fail 'the canonical payload digest does not match.'

if rg -F -q -- '/home/' "$inventory"; then
    fail 'the inventory reveals a physical home path.'
fi

jq -e '
    .contract_version == "legacy-source-inventory-v1"
    and .consistency == "two-identical-read-only-passes"
    and (.inventory_sha256 | test("^[0-9a-f]{64}$"))
    and .payload.source_kind == "legacy-plugin"
    and .payload.source_locator == "operator-private"
    and .payload.validation.invalid_json_files == 0
    and .payload.validation.json_files == .payload.validation.valid_json_files
    and .payload.validation.json_files
        == ([.payload.entries[] | select(.type == "regular" and (.path | ascii_downcase | endswith(".json")))] | length)
    and .payload.counts.entries == (.payload.entries | length)
    and .payload.counts.regular_files == ([.payload.entries[] | select(.type == "regular")] | length)
    and .payload.counts.directories == ([.payload.entries[] | select(.type == "directory")] | length)
    and .payload.counts.symlinks == ([.payload.entries[] | select(.type == "symlink")] | length)
    and .payload.counts.other
        == ([.payload.entries[] | select(.type != "regular" and .type != "directory" and .type != "symlink")] | length)
    and ([.payload.entries[].path] as $paths
        | $paths == ($paths | sort)
        and ($paths | length) == ($paths | unique | length)
        and all($paths[]; startswith("/") | not)
        and all($paths[]; (split("/") | index("..")) == null))
    and all(.payload.entries[];
        (.mode | test("^[0-7]{3,4}$"))
        and (.uid >= 0 and .gid >= 0 and .size >= 0)
        and (if .type == "regular"
             then (.sha256 | type == "string" and test("^[0-9a-f]{64}$"))
             else .sha256 == null
             end)
        and (if .type == "symlink"
             then (.link.kind == "relative" or .link.kind == "absolute-redacted")
                  and (.link.target_sha256 | type == "string" and test("^[0-9a-f]{64}$"))
                  and (if .link.kind == "absolute-redacted" then .link.target == null else (.link.target | type == "string") end)
             else .link == {kind: "none", target: null, target_sha256: null}
             end))
    and .payload.mode_histogram
        == (.payload.entries | group_by(.mode) | map({mode: .[0].mode, count: length}))
    and .payload.ownership_histogram
        == (.payload.entries | group_by([.uid, .gid]) | map({uid: .[0].uid, gid: .[0].gid, count: length}))
' "$inventory" >/dev/null || fail 'the inventory violates its closed internal invariants.'

printf '%s\n' "AD-T82-VERIFY: inventory digest and internal invariants passed ($recorded_sha256)."
