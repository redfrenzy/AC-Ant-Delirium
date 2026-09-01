#!/usr/bin/env bash
set -euo pipefail

script_dir=$(CDPATH= cd -- "$(dirname -- "$0")" && pwd)
repository_root=$(CDPATH= cd -- "$script_dir/../.." && pwd)
source_authority="$script_dir/protected-branch-authority.py"
source_policy="$repository_root/docs/contracts/v1/protected-branch-policy-v1.json"
bundle_root=${1:-"$repository_root/target/t97-protected-branch-authority-bundle"}
staged_authority="$bundle_root/usr/libexec/antdelirium/protected-branch-authority.py"
staged_policy="$bundle_root/usr/lib/antdelirium/protected-branch-policy-v1.json"
manifest="$bundle_root/protected-branch-authority-bundle-v1.json"

fail() {
    printf '%s\n' "AD-T97-BUNDLE: $1" >&2
    exit 1
}

case "$bundle_root" in
    "$repository_root"/target/*) ;;
    *) fail 'bundle destination must be inside the project target directory.' ;;
esac

for command in python3 sha256sum jq install stat; do
    command -v "$command" >/dev/null 2>&1 \
        || fail "required command is unavailable: $command"
done
[[ -f "$source_authority" && ! -L "$source_authority" ]] \
    || fail 'the protected branch authority source is unavailable.'
[[ -f "$source_policy" && ! -L "$source_policy" ]] \
    || fail 'the protected branch policy is unavailable.'
python3 -B - "$source_authority" <<'PY'
import ast
import pathlib
import sys
ast.parse(pathlib.Path(sys.argv[1]).read_text(encoding="utf-8"))
PY
jq -e '
    .contract_version == "protected-branch-policy-v1" and
    .enforcement_mode == "fail-closed-external" and
    .required_status_context == "antdelirium/protected-gate"
' "$source_policy" >/dev/null || fail 'the protected branch policy is invalid.'

mkdir -p "$(dirname "$staged_authority")" "$(dirname "$staged_policy")"
install -m 0755 "$source_authority" "$staged_authority"
install -m 0644 "$source_policy" "$staged_policy"
authority_sha256=$(sha256sum "$staged_authority" | cut -d ' ' -f 1)
policy_sha256=$(sha256sum "$staged_policy" | cut -d ' ' -f 1)

jq -n \
    --arg authority_sha256 "$authority_sha256" \
    --arg policy_sha256 "$policy_sha256" \
    '{
      contract_version: "protected-branch-authority-bundle-v1",
      files: [
        {
          artifact: "protected-branch-authority",
          destination: "/usr/libexec/antdelirium/protected-branch-authority.py",
          mode: "0755",
          owner: "root",
          group: "root",
          sha256: $authority_sha256
        },
        {
          artifact: "protected-branch-policy",
          destination: "/usr/lib/antdelirium/protected-branch-policy-v1.json",
          mode: "0644",
          owner: "root",
          group: "root",
          sha256: $policy_sha256
        }
      ],
      private_key_included: false,
      activation_performed: false,
      provider_configuration_performed: false
    }' >"$manifest"
chmod 0644 "$manifest"

jq -e '
    .contract_version == "protected-branch-authority-bundle-v1" and
    (.files | length) == 2 and
    .files[0].artifact == "protected-branch-authority" and
    .files[0].destination == "/usr/libexec/antdelirium/protected-branch-authority.py" and
    .files[0].mode == "0755" and
    .files[1].artifact == "protected-branch-policy" and
    .files[1].destination == "/usr/lib/antdelirium/protected-branch-policy-v1.json" and
    .files[1].mode == "0644" and
    ([.files[].owner] | all(. == "root")) and
    ([.files[].group] | all(. == "root")) and
    ([.files[].sha256] | all(test("^[0-9a-f]{64}$"))) and
    .private_key_included == false and
    .activation_performed == false and
    .provider_configuration_performed == false
' "$manifest" >/dev/null || fail 'protected branch bundle validation failed.'
[[ $(stat -c '%a' "$staged_authority") == 755 ]] \
    || fail 'staged authority mode is invalid.'
[[ $(stat -c '%a' "$staged_policy") == 644 ]] \
    || fail 'staged policy mode is invalid.'

printf '%s\n' 'AD-T97-BUNDLE: staged the protected branch authority and policy without a private key.'
