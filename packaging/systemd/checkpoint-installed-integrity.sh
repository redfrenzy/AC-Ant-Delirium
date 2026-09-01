#!/usr/bin/env bash
set -euo pipefail

script_dir=$(CDPATH= cd -- "$(dirname -- "$0")" && pwd)
repository_root=$(CDPATH= cd -- "$script_dir/../.." && pwd)
bundle_root="$repository_root/target/t96-integrity-authority-bundle"
bundle_manifest="$bundle_root/integrity-authority-bundle-v1.json"
staged_authority="$bundle_root/usr/libexec/antdelirium/checkpoint-authority.py"
installed_authority=/usr/libexec/antdelirium/checkpoint-authority.py
state_root=/var/lib/antdelirium
anchor_root=/var/lib/antdelirium-integrity
private_key="$anchor_root/authority-private.pem"
public_key="$anchor_root/authority-public.pem"
envelope="$anchor_root/checkpoint-envelope.json"
run_id=$(date -u +%Y%m%dT%H%M%SZ)
log_root="$repository_root/target/t96-installed-integrity-logs/$run_id"
units_stopped=false
service_recovery=not-needed

fail() {
    printf '%s\n' "AD-T96-INSTALLED: $1" >&2
    exit 1
}

finish() {
    local exit_code=$?
    set +e
    if [[ "$units_stopped" == true ]]; then
        systemctl start antdelirium-agent.socket antdelirium-admin.socket
        systemctl start antdelirium.service
        if systemctl is-active --quiet antdelirium.service \
            && systemctl is-active --quiet antdelirium-agent.socket \
            && systemctl is-active --quiet antdelirium-admin.socket; then
            service_recovery=passed
        else
            service_recovery=failed
            exit_code=1
        fi
    fi
    {
        printf 'contract=installed-integrity-checkpoint-result-v1\n'
        printf 'exit_code=%s\n' "$exit_code"
        printf 'checkpoint_present=%s\n' "$([[ -f "$envelope" ]] && printf yes || printf no)"
        printf 'service_recovery=%s\n' "$service_recovery"
        printf 'completed_at=%s\n' "$(date -u +%Y-%m-%dT%H:%M:%SZ)"
    } >"$log_root/result.env"
    chmod -R a+rX "$log_root"
    printf '%s\n' "AD-T96-INSTALLED: logs=$log_root"
    exit "$exit_code"
}

[[ ${1:-} == '--apply' && $# -eq 1 ]] \
    || fail 'use --apply to install the reviewed authority and seal the protected state.'
[[ $EUID -eq 0 ]] || fail 'execute this reviewed routine with sudo.'
invoking_user=${SUDO_USER:-}
[[ -n "$invoking_user" && "$invoking_user" != root ]] \
    || fail 'SUDO_USER must identify the non-root operator.'

mkdir -p "$log_root"
chmod 0755 "$(dirname "$log_root")" "$log_root"
exec > >(tee -a "$log_root/checkpoint.log") 2>&1
trap finish EXIT

for command in systemctl openssl jq python3 sha256sum install runuser stat tee; do
    command -v "$command" >/dev/null 2>&1 \
        || fail "required command is unavailable: $command"
done
[[ -f "$bundle_manifest" && ! -L "$bundle_manifest" ]] \
    || fail 'render the project-local integrity authority bundle first.'
[[ -f "$staged_authority" && ! -L "$staged_authority" ]] \
    || fail 'the staged integrity authority is unavailable.'
[[ -d "$state_root" && ! -L "$state_root" ]] \
    || fail 'the protected service state is unavailable.'
[[ -f /usr/libexec/antdelirium/antdelirium \
    && ! -L /usr/libexec/antdelirium/antdelirium ]] \
    || fail 'the installed AntDelirium service binary is unavailable.'

jq -e '
    .contract_version == "integrity-authority-bundle-v1" and
    (.files | length) == 1 and
    .files[0].artifact == "checkpoint-authority" and
    .files[0].destination == "/usr/libexec/antdelirium/checkpoint-authority.py" and
    .files[0].mode == "0755" and
    .files[0].owner == "root" and
    .files[0].group == "root" and
    .private_key_included == false and
    .activation_performed == false and
    .global_installation_performed == false
' "$bundle_manifest" >/dev/null || fail 'the integrity authority bundle is invalid.'
expected_hash=$(jq -r '.files[0].sha256' "$bundle_manifest")
[[ $(sha256sum "$staged_authority" | cut -d ' ' -f 1) == "$expected_hash" ]] \
    || fail 'the staged integrity authority differs from its manifest.'

printf '%s\n' 'AD-T96-INSTALLED: installing the reviewed root-owned checkpoint authority.'
install -d -m 0755 /usr/libexec/antdelirium
install -m 0755 -o root -g root "$staged_authority" "$installed_authority"
[[ $(stat -c '%a %U:%G' "$installed_authority") == '755 root:root' ]] \
    || fail 'installed authority metadata is invalid.'
[[ $(sha256sum "$installed_authority" | cut -d ' ' -f 1) == "$expected_hash" ]] \
    || fail 'installed authority differs from the reviewed bundle.'

if [[ ! -e "$anchor_root" ]]; then
    install -d -m 0700 -o root -g root "$anchor_root"
fi
[[ -d "$anchor_root" && ! -L "$anchor_root" ]] \
    || fail 'the external integrity anchor has unsafe metadata.'
if [[ ! -e "$private_key" && ! -e "$public_key" && ! -e "$envelope" ]]; then
    umask 077
    openssl genpkey -algorithm ED25519 -out "$private_key" 2>"$log_root/keygen.err"
    chmod 0600 "$private_key"
fi
if [[ -f "$private_key" && ! -L "$private_key" \
    && ! -e "$public_key" && ! -e "$envelope" ]]; then
    openssl pkey -in "$private_key" -pubout -out "$public_key" \
        2>"$log_root/public-key.err"
    chmod 0644 "$public_key"
else
    [[ -f "$private_key" && ! -L "$private_key" \
        && -f "$public_key" && ! -L "$public_key" ]] \
        || fail 'the external integrity authority is incomplete.'
fi
[[ $(stat -c '%a %U:%G' "$anchor_root") == '700 root:root' ]] \
    || fail 'external anchor directory metadata is invalid.'
[[ $(stat -c '%a %U:%G' "$private_key") == '600 root:root' ]] \
    || fail 'external private key metadata is invalid.'
[[ $(stat -c '%a %U:%G' "$public_key") == '644 root:root' ]] \
    || fail 'external public key metadata is invalid.'

if runuser -u antdelirium -- test -r "$private_key" \
    || runuser -u antdelirium -- test -w "$anchor_root"; then
    fail 'the service identity can access the external signing authority.'
fi
if runuser -u "$invoking_user" -- test -r "$private_key" \
    || runuser -u "$invoking_user" -- test -w "$anchor_root"; then
    fail 'the invoking agent identity can access the external signing authority.'
fi

printf '%s\n' 'AD-T96-INSTALLED: quiescing both sockets and the service for a consistent checkpoint.'
units_stopped=true
systemctl stop antdelirium.service antdelirium-agent.socket antdelirium-admin.socket

python3 -B "$installed_authority" seal \
    --state-root "$state_root" \
    --anchor-root "$anchor_root" \
    --private-key "$private_key" \
    --public-key "$public_key" >"$log_root/seal.out"
[[ -f "$envelope" && ! -L "$envelope" ]] \
    || fail 'the signed checkpoint envelope was not created.'
[[ $(stat -c '%a %U:%G' "$envelope") == '600 root:root' ]] \
    || fail 'signed checkpoint envelope metadata is invalid.'

python3 -B "$installed_authority" verify \
    --state-root "$state_root" \
    --anchor-root "$anchor_root" \
    --public-key "$public_key" >"$log_root/verify.out"

printf '%s\n' 'AD-T96-INSTALLED: restarting the protected service after the signed checkpoint.'
systemctl start antdelirium-agent.socket antdelirium-admin.socket
systemctl start antdelirium.service
systemctl is-active --quiet antdelirium.service
systemctl is-active --quiet antdelirium-agent.socket
systemctl is-active --quiet antdelirium-admin.socket
units_stopped=false

printf '%s\n' 'AD-T96-INSTALLED: external key isolation and signed protected-state checkpoint passed.'
