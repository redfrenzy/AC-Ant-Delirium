#!/usr/bin/env bash
set -euo pipefail

script_dir=$(CDPATH= cd -- "$(dirname -- "$0")" && pwd)
repository_root=$(CDPATH= cd -- "$script_dir/../.." && pwd)
source_authority="$script_dir/checkpoint-authority.py"
bundle_root=${1:-"$repository_root/target/t96-integrity-authority-bundle"}
staged_authority="$bundle_root/usr/libexec/antdelirium/checkpoint-authority.py"
manifest="$bundle_root/integrity-authority-bundle-v1.json"

fail() {
    printf '%s\n' "AD-T96-BUNDLE: $1" >&2
    exit 1
}

case "$bundle_root" in
    "$repository_root"/target/*) ;;
    *) fail 'bundle destination must be inside the project target directory.' ;;
esac

for command in python3 sha256sum jq install; do
    command -v "$command" >/dev/null 2>&1 \
        || fail "required command is unavailable: $command"
done
[[ -f "$source_authority" && ! -L "$source_authority" ]] \
    || fail 'the integrity authority source is unavailable.'
python3 - "$source_authority" <<'PY'
import ast
import pathlib
import sys
ast.parse(pathlib.Path(sys.argv[1]).read_text(encoding="utf-8"))
PY

mkdir -p "$(dirname "$staged_authority")"
install -m 0755 "$source_authority" "$staged_authority"
authority_sha256=$(sha256sum "$staged_authority" | cut -d ' ' -f 1)

jq -n --arg authority_sha256 "$authority_sha256" '{
    contract_version: "integrity-authority-bundle-v1",
    files: [{
        artifact: "checkpoint-authority",
        destination: "/usr/libexec/antdelirium/checkpoint-authority.py",
        mode: "0755",
        owner: "root",
        group: "root",
        sha256: $authority_sha256
    }],
    private_key_included: false,
    activation_performed: false,
    global_installation_performed: false
}' >"$manifest"
chmod 0644 "$manifest"

jq -e '
    .contract_version == "integrity-authority-bundle-v1" and
    (.files | length) == 1 and
    .files[0].artifact == "checkpoint-authority" and
    .files[0].destination == "/usr/libexec/antdelirium/checkpoint-authority.py" and
    .files[0].mode == "0755" and
    .files[0].owner == "root" and
    .files[0].group == "root" and
    (.files[0].sha256 | test("^[0-9a-f]{64}$")) and
    .private_key_included == false and
    .activation_performed == false and
    .global_installation_performed == false
' "$manifest" >/dev/null || fail 'integrity authority manifest validation failed.'
[[ $(stat -c '%a' "$staged_authority") == 755 ]] \
    || fail 'staged authority mode is invalid.'
[[ $(stat -c '%a' "$manifest") == 644 ]] \
    || fail 'staged authority manifest mode is invalid.'

printf '%s\n' 'AD-T96-BUNDLE: staged the path-free external checkpoint authority without a private key.'
