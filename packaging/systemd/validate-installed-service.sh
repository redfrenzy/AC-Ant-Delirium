#!/usr/bin/env bash
set -euo pipefail

script_dir=$(CDPATH= cd -- "$(dirname -- "$0")" && pwd)
project_root=$(CDPATH= cd -- "$script_dir/../.." && pwd)
bundle_root="$project_root/target/t86-system-installation-bundle"
log_parent="$project_root/target/t86-root-validation-logs"
run_id=$(date -u +%Y%m%dT%H%M%SZ)
log_root="$log_parent/$run_id"
secret_work="/run/antdelirium-bootstrap-$$"
bootstrap_dropin=/run/systemd/system/antdelirium.service.d/90-bootstrap.conf
bootstrap_encrypted=/run/antdelirium-bootstrap-v1.cred
operator_credential_root=/var/lib/antdelirium-operator
mode=${1:-}
owns_transient_bootstrap=false

fail() {
    printf '%s\n' "AD-T86-ROOT: $1" >&2
    exit 1
}

[[ $EUID -eq 0 ]] || fail 'execute this script with sudo.'
[[ "$mode" == '--apply' || "$mode" == '--inspect' ]] \
    || fail 'choose exactly --apply or --inspect.'

invoking_user=${SUDO_USER:-${ANTDELIRIUM_TEST_USER:-}}
[[ -n "$invoking_user" && "$invoking_user" != root ]] \
    || fail 'SUDO_USER or ANTDELIRIUM_TEST_USER must identify the non-root operator.'
invoking_uid=$(id -u "$invoking_user")

mkdir -p "$log_root"
chmod 0755 "$log_parent" "$log_root"
exec > >(tee -a "$log_root/root-validation.log") 2>&1

collect_diagnostics() {
    local exit_code=$?
    set +e
    {
        printf 'run_id=%s\nmode=%s\nexit_code=%s\n' "$run_id" "$mode" "$exit_code"
        printf 'completed_at=%s\n' "$(date -u +%Y-%m-%dT%H:%M:%SZ)"
    } >"$log_root/result.env"
    systemd --version >"$log_root/systemd-version.txt" 2>&1
    systemctl status antdelirium.service antdelirium-agent.socket antdelirium-admin.socket \
        --no-pager --full >"$log_root/systemctl-status.txt" 2>&1
    systemctl show antdelirium.service antdelirium-agent.socket antdelirium-admin.socket \
        >"$log_root/systemctl-show.txt" 2>&1
    systemctl cat antdelirium.service antdelirium-agent.socket antdelirium-admin.socket \
        >"$log_root/systemctl-cat.txt" 2>&1
    journalctl -b -u antdelirium.service -u antdelirium-agent.socket \
        -u antdelirium-admin.socket --no-pager --output=short-precise \
        >"$log_root/journal.txt" 2>&1
    {
        id antdelirium
        id "$invoking_user"
        getent group antdelirium-client
        getent group antdelirium-admin
    } >"$log_root/identities.txt" 2>&1
    {
        stat -c '%A %a %U:%G %n' \
            /usr/libexec/antdelirium/antdelirium \
            /usr/lib/systemd/system/antdelirium.service \
            /usr/lib/systemd/system/antdelirium-agent.socket \
            /usr/lib/systemd/system/antdelirium-admin.socket \
            /usr/lib/sysusers.d/antdelirium.conf \
            /usr/lib/tmpfiles.d/antdelirium.conf \
            /run/antdelirium/agent.sock \
            /run/antdelirium-admin/admin.sock
    } >"$log_root/files-and-sockets.txt" 2>&1
    if [[ -e /var/lib/antdelirium/registry/state/state.db ]]; then
        python3 -c 'import sqlite3,sys; c=sqlite3.connect("file:"+sys.argv[1]+"?mode=ro",uri=True); print("schema_version="+str(c.execute("pragma user_version").fetchone()[0])); print("provisioning_receipts="+str(c.execute("select count(*) from initial_capability_provisioning").fetchone()[0])); print("capability_grants="+str(c.execute("select count(*) from capability_grants").fetchone()[0])); print("grant_hash_lengths="+",".join(str(r[0]) for r in c.execute("select length(secret_hash) from capability_grants order by grant_id")))' \
            /var/lib/antdelirium/registry/state/state.db \
            >"$log_root/storage-summary.txt" 2>&1
    fi
    if [[ "$mode" == '--apply' ]]; then
        rm -rf "$secret_work"
    fi
    if [[ "$owns_transient_bootstrap" == true ]]; then
        rm -f "$bootstrap_dropin" "$bootstrap_encrypted"
        rmdir /run/systemd/system/antdelirium.service.d 2>/dev/null || true
        systemctl daemon-reload >/dev/null 2>&1 || true
    fi
    chmod -R a+rX "$log_root"
    printf '%s\n' "AD-T86-ROOT: logs=$log_root"
    exit "$exit_code"
}
trap collect_diagnostics EXIT

if [[ "$mode" == '--inspect' ]]; then
    printf '%s\n' 'AD-T86-ROOT: collecting read-only installed-service diagnostics.'
    exit 0
fi

[[ -f "$bundle_root/system-installation-bundle-v1.json" ]] \
    || fail 'render the project-local installation bundle before running sudo.'
jq -e '.activation_performed == false and .global_installation_performed == false' \
    "$bundle_root/system-installation-bundle-v1.json" >/dev/null \
    || fail 'staged bundle manifest is not pre-installation evidence.'

for destination in \
    /usr/libexec/antdelirium/antdelirium \
    /usr/lib/systemd/system/antdelirium.service \
    /usr/lib/systemd/system/antdelirium-agent.socket \
    /usr/lib/systemd/system/antdelirium-admin.socket \
    /usr/lib/sysusers.d/antdelirium.conf \
    /usr/lib/tmpfiles.d/antdelirium.conf \
    "$bootstrap_dropin" \
    "$bootstrap_encrypted"
do
    [[ ! -e "$destination" ]] || fail "refusing to overwrite existing host artifact: $destination"
done
[[ ! -e /var/lib/antdelirium ]] \
    || fail 'refusing to reuse an existing protected state directory.'
[[ ! -e "$operator_credential_root" ]] \
    || fail 'refusing to overwrite an existing operator credential directory.'

for command in systemctl systemd-sysusers systemd-tmpfiles systemd-creds openssl jq python3 install usermod runuser; do
    command -v "$command" >/dev/null 2>&1 || fail "required command is unavailable: $command"
done

printf '%s\n' 'AD-T86-ROOT: creating dedicated identities from the reviewed sysusers contract.'
systemd-sysusers "$bundle_root/usr/lib/sysusers.d/antdelirium.conf"

printf '%s\n' 'AD-T86-ROOT: installing the reviewed binary and units without rebuilding as root.'
install -d -m 0755 /usr/libexec/antdelirium
install -m 0755 "$bundle_root/usr/libexec/antdelirium/antdelirium" \
    /usr/libexec/antdelirium/antdelirium
install -m 0644 "$bundle_root/usr/lib/systemd/system/antdelirium.service" \
    /usr/lib/systemd/system/antdelirium.service
install -m 0644 "$bundle_root/usr/lib/systemd/system/antdelirium-agent.socket" \
    /usr/lib/systemd/system/antdelirium-agent.socket
install -m 0644 "$bundle_root/usr/lib/systemd/system/antdelirium-admin.socket" \
    /usr/lib/systemd/system/antdelirium-admin.socket
install -m 0644 "$bundle_root/usr/lib/sysusers.d/antdelirium.conf" \
    /usr/lib/sysusers.d/antdelirium.conf
install -m 0644 "$bundle_root/usr/lib/tmpfiles.d/antdelirium.conf" \
    /usr/lib/tmpfiles.d/antdelirium.conf
systemd-tmpfiles --create /usr/lib/tmpfiles.d/antdelirium.conf
[[ $(stat -c '%a %U:%G' /run/antdelirium-admin) == '750 root:antdelirium-admin' ]] \
    || fail 'administrative runtime directory ownership is invalid.'

usermod -a -G antdelirium-client,antdelirium-admin "$invoking_user"

install -d -m 0700 "$secret_work"
state_token_file="$secret_work/state.token"
installation_token_file="$secret_work/installation.token"
openssl rand -base64 32 | tr '+/' '-_' | tr -d '=\n' >"$state_token_file"
openssl rand -base64 32 | tr '+/' '-_' | tr -d '=\n' >"$installation_token_file"
chmod 0600 "$state_token_file" "$installation_token_file"
[[ $(wc -c <"$state_token_file") -eq 43 ]] || fail 'state token generation failed.'
[[ $(wc -c <"$installation_token_file") -eq 43 ]] \
    || fail 'installation token generation failed.'

now_unix=$(date +%s)
manifest_expires=$((now_unix + 600))
grant_expires=$((now_unix + 2592000))
provisioning_id="prv_$(openssl rand -hex 16 | cut -c1-26)"
state_capability_id="cap_$(openssl rand -hex 16 | cut -c1-26)"
installation_capability_id="cap_$(openssl rand -hex 16 | cut -c1-26)"
bootstrap_plain="$secret_work/bootstrap.json"

jq -n \
    --arg provisioning_id "$provisioning_id" \
    --arg state_capability_id "$state_capability_id" \
    --arg installation_capability_id "$installation_capability_id" \
    --rawfile state_token "$state_token_file" \
    --rawfile installation_token "$installation_token_file" \
    --argjson subject_uid "$invoking_uid" \
    --argjson issued_at_unix "$now_unix" \
    --argjson manifest_expires "$manifest_expires" \
    --argjson grant_expires "$grant_expires" \
    '{
        contract_version: "initial-capability-provisioning-v1",
        provisioning_id: $provisioning_id,
        issued_at_unix: $issued_at_unix,
        expires_at_unix: $manifest_expires,
        grants: [
            {capability_id: $state_capability_id, token: $state_token, subject_uid: $subject_uid, actor: "operator", channel: "administrative", operation: "state.admin", project_id: null, workspace_id: null, task_id: null, expires_at_unix: $grant_expires, revocation_epoch: 0},
            {capability_id: $installation_capability_id, token: $installation_token, subject_uid: $subject_uid, actor: "operator", channel: "administrative", operation: "installation.admin", project_id: null, workspace_id: null, task_id: null, expires_at_unix: $grant_expires, revocation_epoch: 0}
        ]
    }' >"$bootstrap_plain"
chmod 0600 "$bootstrap_plain"

printf '%s\n' 'AD-T86-ROOT: sealing bootstrap and operator descriptors with the host credential key.'
systemd-creds encrypt --with-key=host --name=antdelirium-bootstrap-v1 \
    "$bootstrap_plain" "$bootstrap_encrypted"
chmod 0600 "$bootstrap_encrypted"

install -d -m 0750 -o root -g antdelirium-admin "$operator_credential_root"
for operation in state installation; do
    if [[ "$operation" == state ]]; then
        capability_id=$state_capability_id
        token_file=$state_token_file
    else
        capability_id=$installation_capability_id
        token_file=$installation_token_file
    fi
    descriptor_plain="$secret_work/$operation-capability.json"
    jq -n --arg capability_id "$capability_id" --rawfile token "$token_file" \
        '{contract_version:"admin-capability-v1",capability:{capability_id:$capability_id,token:$token}}' \
        >"$descriptor_plain"
    chmod 0600 "$descriptor_plain"
    systemd-creds encrypt --with-key=host --name="antdelirium-operator-$operation-v1" \
        "$descriptor_plain" "$operator_credential_root/$operation.cred"
    chown root:antdelirium-admin "$operator_credential_root/$operation.cred"
    chmod 0640 "$operator_credential_root/$operation.cred"
done

install -d -m 0755 /run/systemd/system/antdelirium.service.d
{
    printf '%s\n' '[Service]'
    printf '%s\n' "LoadCredentialEncrypted=antdelirium-bootstrap-v1:$bootstrap_encrypted"
} >"$bootstrap_dropin"
chmod 0600 "$bootstrap_dropin"
owns_transient_bootstrap=true

systemctl daemon-reload
systemctl enable antdelirium-agent.socket antdelirium-admin.socket antdelirium.service
systemctl start antdelirium.service
systemctl is-active --quiet antdelirium.service
systemctl is-active --quiet antdelirium-agent.socket
systemctl is-active --quiet antdelirium-admin.socket

database_path=/var/lib/antdelirium/registry/state/state.db
[[ -f "$database_path" ]] || fail 'protected registry was not created.'
storage_summary=$(python3 -c 'import sqlite3,sys; c=sqlite3.connect("file:"+sys.argv[1]+"?mode=ro",uri=True); print(*c.execute("select (select count(*) from initial_capability_provisioning),(select count(*) from capability_grants),(select user_version from pragma_user_version)").fetchone())' "$database_path")
[[ "$storage_summary" == '1 2 8' ]] \
    || fail 'initial provisioning receipt, grants or schema version did not match.'
if grep -aF -f "$state_token_file" "$database_path" >/dev/null \
    || grep -aF -f "$installation_token_file" "$database_path" >/dev/null
then
    fail 'a raw capability token appeared in protected storage.'
fi

printf '%s\n' 'AD-T86-ROOT: removing the one-use bootstrap channel and proving a credential-free restart.'
rm -f "$bootstrap_dropin" "$bootstrap_encrypted"
rmdir /run/systemd/system/antdelirium.service.d 2>/dev/null || true
owns_transient_bootstrap=false
systemctl daemon-reload
systemctl restart antdelirium.service
systemctl is-active --quiet antdelirium.service

runuser -u "$invoking_user" -- test -S /run/antdelirium/agent.sock
runuser -u "$invoking_user" -- test -r /run/antdelirium/agent.sock
runuser -u "$invoking_user" -- test -w /run/antdelirium/agent.sock
runuser -u "$invoking_user" -- test -S /run/antdelirium-admin/admin.sock
runuser -u "$invoking_user" -- test -r /run/antdelirium-admin/admin.sock
runuser -u "$invoking_user" -- test -w /run/antdelirium-admin/admin.sock

printf '%s\n' 'AD-T86-ROOT: installed service, one-use provisioning and credential-free restart passed.'
