#!/usr/bin/env bash
set -euo pipefail

script_dir=$(CDPATH= cd -- "$(dirname -- "$0")" && pwd)
repository_root=$(CDPATH= cd -- "$script_dir/../.." && pwd)
bundle_root="$repository_root/target/t97-protected-branch-authority-bundle"
manifest="$bundle_root/protected-branch-authority-bundle-v1.json"
staged_authority="$bundle_root/usr/libexec/antdelirium/protected-branch-authority.py"
staged_policy="$bundle_root/usr/lib/antdelirium/protected-branch-policy-v1.json"
installed_authority=/usr/libexec/antdelirium/protected-branch-authority.py
installed_policy=/usr/lib/antdelirium/protected-branch-policy-v1.json
anchor_root=/var/lib/antdelirium-branch-gate
private_key="$anchor_root/authority-private.pem"
public_key="$anchor_root/authority-public.pem"
run_id=$(date -u +%Y%m%dT%H%M%SZ)
public_log_root="$repository_root/target/t97-installed-branch-authority-logs/$run_id"
private_log_root="$anchor_root/validation/$run_id"

fail() {
    printf '%s\n' "AD-T97-INSTALLED: $1" >&2
    exit 1
}

finish() {
    local exit_code=$?
    set +e
    {
        printf 'contract=installed-protected-branch-authority-result-v1\n'
        printf 'exit_code=%s\n' "$exit_code"
        printf 'authority_installed=%s\n' "$([[ -f "$installed_authority" ]] && printf yes || printf no)"
        printf 'policy_installed=%s\n' "$([[ -f "$installed_policy" ]] && printf yes || printf no)"
        printf 'private_key_present=%s\n' "$([[ -f "$private_key" ]] && printf yes || printf no)"
        printf 'provider_branch_protection=not-configured\n'
        printf 'completed_at=%s\n' "$(date -u +%Y-%m-%dT%H:%M:%SZ)"
    } >"$private_log_root/result.env"
    runuser -u "$invoking_user" -- mkdir -p "$public_log_root"
    for public_log in install.log result.env verify.out verify.err; do
        if [[ -f "$private_log_root/$public_log" ]]; then
            cat "$private_log_root/$public_log" \
                | runuser -u "$invoking_user" -- tee "$public_log_root/$public_log" \
                    >/dev/null
        fi
    done
    printf '%s\n' "AD-T97-INSTALLED: logs=$public_log_root"
    exit "$exit_code"
}

[[ ${1:-} == '--apply' && $# -eq 1 ]] \
    || fail 'use --apply to install the reviewed protected branch authority.'
[[ $EUID -eq 0 ]] || fail 'execute this reviewed routine with sudo.'
invoking_user=${SUDO_USER:-}
[[ -n "$invoking_user" && "$invoking_user" != root ]] \
    || fail 'SUDO_USER must identify the non-root operator.'

for command in openssl jq python3 sha256sum install runuser stat tee cat rm mkdir chmod; do
    command -v "$command" >/dev/null 2>&1 \
        || fail "required command is unavailable: $command"
done
if [[ ! -e "$anchor_root" ]]; then
    install -d -m 0700 -o root -g root "$anchor_root"
fi
[[ -d "$anchor_root" && ! -L "$anchor_root" ]] \
    || fail 'the protected branch anchor has unsafe metadata.'
install -d -m 0700 -o root -g root "$anchor_root/validation" "$private_log_root"
exec > >(tee -a "$private_log_root/install.log") 2>&1
trap finish EXIT
for input in "$manifest" "$staged_authority" "$staged_policy"; do
    [[ -f "$input" && ! -L "$input" ]] \
        || fail 'render the reviewed protected branch bundle first.'
done

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
    .private_key_included == false and
    .activation_performed == false and
    .provider_configuration_performed == false
' "$manifest" >/dev/null || fail 'the protected branch bundle manifest is invalid.'
expected_authority_sha256=$(jq -r '.files[0].sha256' "$manifest")
expected_policy_sha256=$(jq -r '.files[1].sha256' "$manifest")
[[ $(sha256sum "$staged_authority" | cut -d ' ' -f 1) == "$expected_authority_sha256" ]] \
    || fail 'the staged authority differs from its manifest.'
[[ $(sha256sum "$staged_policy" | cut -d ' ' -f 1) == "$expected_policy_sha256" ]] \
    || fail 'the staged policy differs from its manifest.'

printf '%s\n' 'AD-T97-INSTALLED: installing the reviewed root-owned authority and policy.'
install -d -m 0755 /usr/libexec/antdelirium /usr/lib/antdelirium
[[ -d /usr/libexec/antdelirium && ! -L /usr/libexec/antdelirium \
    && -d /usr/lib/antdelirium && ! -L /usr/lib/antdelirium ]] \
    || fail 'the installed authority directories have unsafe metadata.'
install -m 0755 -o root -g root "$staged_authority" "$installed_authority"
install -m 0644 -o root -g root "$staged_policy" "$installed_policy"
[[ $(sha256sum "$installed_authority" | cut -d ' ' -f 1) == "$expected_authority_sha256" ]] \
    || fail 'the installed authority differs from the reviewed bundle.'
[[ $(sha256sum "$installed_policy" | cut -d ' ' -f 1) == "$expected_policy_sha256" ]] \
    || fail 'the installed policy differs from the reviewed bundle.'
[[ $(stat -c '%a %U:%G' "$installed_authority") == '755 root:root' ]] \
    || fail 'installed authority metadata is invalid.'
[[ $(stat -c '%a %U:%G' "$installed_policy") == '644 root:root' ]] \
    || fail 'installed policy metadata is invalid.'

if [[ ! -e "$private_key" && ! -e "$public_key" ]]; then
    umask 077
    openssl genpkey -algorithm ED25519 -out "$private_key" 2>"$private_log_root/keygen.err"
    chmod 0600 "$private_key"
    openssl pkey -in "$private_key" -pubout -out "$public_key" \
        2>"$private_log_root/public-key.err"
    chmod 0644 "$public_key"
fi
[[ -f "$private_key" && ! -L "$private_key" \
    && -f "$public_key" && ! -L "$public_key" ]] \
    || fail 'the protected branch signing authority is incomplete.'
[[ $(stat -c '%a %U:%G' "$anchor_root") == '700 root:root' ]] \
    || fail 'protected branch anchor metadata is invalid.'
[[ $(stat -c '%a %U:%G' "$private_key") == '600 root:root' ]] \
    || fail 'protected branch private key metadata is invalid.'
[[ $(stat -c '%a %U:%G' "$public_key") == '644 root:root' ]] \
    || fail 'protected branch public key metadata is invalid.'
if runuser -u antdelirium -- test -r "$private_key" \
    || runuser -u antdelirium -- test -w "$anchor_root" \
    || runuser -u "$invoking_user" -- test -r "$private_key" \
    || runuser -u "$invoking_user" -- test -w "$anchor_root"; then
    fail 'an untrusted identity can access the protected branch signing authority.'
fi

printf '%s\n' 'antdelirium-protected-branch-key-check-v1' >"$anchor_root/challenge"
openssl pkeyutl -sign -inkey "$private_key" -rawin \
    -in "$anchor_root/challenge" -out "$anchor_root/challenge.sig" \
    2>"$private_log_root/sign.err"
openssl pkeyutl -verify -pubin -inkey "$public_key" -rawin \
    -in "$anchor_root/challenge" -sigfile "$anchor_root/challenge.sig" \
    >"$private_log_root/verify.out" 2>"$private_log_root/verify.err" \
    || fail 'the protected branch signing key pair did not verify.'
rm -f "$anchor_root/challenge" "$anchor_root/challenge.sig"

printf '%s\n' 'AD-T97-INSTALLED: external branch authority, policy and isolated signing key passed.'
printf '%s\n' 'AD-T97-INSTALLED: provider branch protection remains unconfigured.'
