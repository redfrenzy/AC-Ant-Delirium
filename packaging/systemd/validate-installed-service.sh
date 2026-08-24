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
[[ "$mode" == '--apply' || "$mode" == '--resume' || "$mode" == '--inspect' ]] \
    || fail 'choose exactly --apply, --resume or --inspect.'

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
    if [[ "$mode" == '--apply' || "$mode" == '--resume' ]]; then
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

bundle_manifest="$bundle_root/system-installation-bundle-v1.json"
database_path=/var/lib/antdelirium/registry/state/state.db
state_token_file="$secret_work/state.token"
installation_token_file="$secret_work/installation.token"
bootstrap_plain="$secret_work/bootstrap.json"

require_reviewed_bundle() {
    [[ -f "$bundle_manifest" ]] \
        || fail 'render the project-local installation bundle before running sudo.'
    jq -e '.activation_performed == false and .global_installation_performed == false' \
        "$bundle_manifest" >/dev/null \
        || fail 'staged bundle manifest is not pre-installation evidence.'
}

install_reviewed_bundle() {
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
}

render_bootstrap_manifest() {
    local now_unix manifest_expires grant_expires provisioning_id
    now_unix=$(date +%s)
    manifest_expires=$((now_unix + 600))
    grant_expires=$((now_unix + 2592000))
    provisioning_id="prv_$(openssl rand -hex 16 | cut -c1-26)"
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
}

seal_bootstrap() {
    owns_transient_bootstrap=true
    systemd-creds encrypt --with-key=host --name=antdelirium-bootstrap-v1 \
        "$bootstrap_plain" "$bootstrap_encrypted"
    chmod 0600 "$bootstrap_encrypted"
    install -d -m 0755 /run/systemd/system/antdelirium.service.d
    {
        printf '%s\n' '[Service]'
        printf '%s\n' "LoadCredentialEncrypted=antdelirium-bootstrap-v1:$bootstrap_encrypted"
    } >"$bootstrap_dropin"
    chmod 0600 "$bootstrap_dropin"
}

storage_summary() {
    python3 -c 'import sqlite3,sys; c=sqlite3.connect("file:"+sys.argv[1]+"?mode=ro",uri=True); print(*c.execute("select (select count(*) from initial_capability_provisioning),(select count(*) from capability_grants),(select user_version from pragma_user_version)").fetchone())' "$database_path"
}

pristine_storage_summary() {
    python3 -c 'import sqlite3,sys; c=sqlite3.connect("file:"+sys.argv[1]+"?mode=ro",uri=True); print(*c.execute("select (select count(*) from initial_capability_provisioning),(select count(*) from capability_grants),(select count(*) from projects)+(select count(*) from service_security_events)+(select count(*) from capability_subject_epochs)+(select count(*) from revocations)+(select count(*) from capability_request_receipts)+(select count(*) from provisional_project_registrations)+(select count(*) from legacy_migration_records),(select user_version from pragma_user_version)").fetchone())' "$database_path"
}

wait_for_initial_provisioning() {
    local attempt summary
    for attempt in $(seq 1 100); do
        if [[ -f "$database_path" ]] \
            && summary=$(storage_summary 2>/dev/null) \
            && [[ "$summary" == '1 2 8' ]]
        then
            return 0
        fi
        if systemctl is-failed --quiet antdelirium.service; then
            fail 'service failed during initial provisioning; inspect the AD-PROVISION code in journal.txt.'
        fi
        sleep 0.1
    done
    fail 'timed out waiting for the initial provisioning receipt.'
}

wait_for_stable_service() {
    local attempt
    for attempt in $(seq 1 50); do
        if systemctl is-failed --quiet antdelirium.service; then
            fail 'service failed during the credential-free restart.'
        fi
        if systemctl is-active --quiet antdelirium.service; then
            sleep 1
            systemctl is-active --quiet antdelirium.service \
                || fail 'service did not remain active after the credential-free restart.'
            return 0
        fi
        sleep 0.1
    done
    fail 'timed out waiting for the credential-free service restart.'
}

complete_provisioning_test() {
    systemctl daemon-reload
    systemctl enable antdelirium-agent.socket antdelirium-admin.socket antdelirium.service
    systemctl restart antdelirium.service
    systemctl is-active --quiet antdelirium-agent.socket
    systemctl is-active --quiet antdelirium-admin.socket
    wait_for_initial_provisioning

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
    wait_for_stable_service

    runuser -u "$invoking_user" -- test -S /run/antdelirium/agent.sock
    runuser -u "$invoking_user" -- test -r /run/antdelirium/agent.sock
    runuser -u "$invoking_user" -- test -w /run/antdelirium/agent.sock
    runuser -u "$invoking_user" -- test -S /run/antdelirium-admin/admin.sock
    runuser -u "$invoking_user" -- test -r /run/antdelirium-admin/admin.sock
    runuser -u "$invoking_user" -- test -w /run/antdelirium-admin/admin.sock
}

for command in systemctl systemd-sysusers systemd-tmpfiles systemd-creds openssl jq python3 install usermod runuser cmp seq; do
    command -v "$command" >/dev/null 2>&1 || fail "required command is unavailable: $command"
done
require_reviewed_bundle
install -d -m 0700 "$secret_work"

if [[ "$mode" == '--apply' ]]; then
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
        [[ ! -e "$destination" ]] \
            || fail 'refusing to overwrite an existing host installation artifact.'
    done
    [[ ! -e /var/lib/antdelirium ]] \
        || fail 'refusing to reuse an existing protected state directory.'
    [[ ! -e "$operator_credential_root" ]] \
        || fail 'refusing to overwrite an existing operator credential directory.'

    printf '%s\n' 'AD-T86-ROOT: creating dedicated identities from the reviewed sysusers contract.'
    systemd-sysusers "$bundle_root/usr/lib/sysusers.d/antdelirium.conf"
    install_reviewed_bundle
    usermod -a -G antdelirium-client,antdelirium-admin "$invoking_user"

    openssl rand -base64 32 | tr '+/' '-_' | tr -d '=\n' >"$state_token_file"
    openssl rand -base64 32 | tr '+/' '-_' | tr -d '=\n' >"$installation_token_file"
    chmod 0600 "$state_token_file" "$installation_token_file"
    [[ $(wc -c <"$state_token_file") -eq 43 ]] || fail 'state token generation failed.'
    [[ $(wc -c <"$installation_token_file") -eq 43 ]] \
        || fail 'installation token generation failed.'
    state_capability_id="cap_$(openssl rand -hex 16 | cut -c1-26)"
    installation_capability_id="cap_$(openssl rand -hex 16 | cut -c1-26)"
    render_bootstrap_manifest

    printf '%s\n' 'AD-T86-ROOT: sealing bootstrap and operator descriptors with the host credential key.'
    seal_bootstrap
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
else
    printf '%s\n' 'AD-T86-ROOT: validating the failed first-boot state before recovery.'
    [[ ! -e "$bootstrap_dropin" && ! -e "$bootstrap_encrypted" ]] \
        || fail 'a transient bootstrap artifact still exists; recovery is not safe.'
    for destination in \
        /usr/libexec/antdelirium/antdelirium \
        /usr/lib/systemd/system/antdelirium.service \
        /usr/lib/systemd/system/antdelirium-agent.socket \
        /usr/lib/systemd/system/antdelirium-admin.socket \
        /usr/lib/sysusers.d/antdelirium.conf \
        /usr/lib/tmpfiles.d/antdelirium.conf \
        "$operator_credential_root/state.cred" \
        "$operator_credential_root/installation.cred" \
        "$database_path"
    do
        [[ -f "$destination" && ! -L "$destination" ]] \
            || fail 'the partial installation is missing a required regular artifact.'
    done
    [[ $(stat -c '%a %U:%G' /usr/libexec/antdelirium/antdelirium) == '755 root:root' ]] \
        || fail 'installed binary metadata changed.'
    for installed_contract in \
        /usr/lib/systemd/system/antdelirium.service \
        /usr/lib/systemd/system/antdelirium-agent.socket \
        /usr/lib/systemd/system/antdelirium-admin.socket \
        /usr/lib/sysusers.d/antdelirium.conf \
        /usr/lib/tmpfiles.d/antdelirium.conf
    do
        [[ $(stat -c '%a %U:%G' "$installed_contract") == '644 root:root' ]] \
            || fail 'installed service contract metadata changed.'
    done
    [[ $(stat -c '%a %U:%G' "$database_path") == '600 antdelirium:antdelirium' ]] \
        || fail 'protected registry metadata changed.'
    [[ $(stat -c '%a %U:%G' "$operator_credential_root") == '750 root:antdelirium-admin' ]] \
        || fail 'operator credential directory metadata changed.'
    for descriptor in "$operator_credential_root/state.cred" "$operator_credential_root/installation.cred"; do
        [[ $(stat -c '%a %U:%G' "$descriptor") == '640 root:antdelirium-admin' ]] \
            || fail 'operator credential metadata changed.'
    done
    [[ $(pristine_storage_summary) == '0 0 0 8' ]] \
        || fail 'protected state is not the pristine failed-bootstrap registry.'
    [[ $(getent group antdelirium-admin | cut -d: -f4) == "$invoking_user" \
        && $(getent group antdelirium-client | cut -d: -f4) == "$invoking_user" ]] \
        || fail 'the original operator cannot be identified uniquely from the dedicated groups.'
    for contract in service agent admin sysusers tmpfiles; do
        case "$contract" in
            service) installed=/usr/lib/systemd/system/antdelirium.service; staged="$bundle_root/usr/lib/systemd/system/antdelirium.service" ;;
            agent) installed=/usr/lib/systemd/system/antdelirium-agent.socket; staged="$bundle_root/usr/lib/systemd/system/antdelirium-agent.socket" ;;
            admin) installed=/usr/lib/systemd/system/antdelirium-admin.socket; staged="$bundle_root/usr/lib/systemd/system/antdelirium-admin.socket" ;;
            sysusers) installed=/usr/lib/sysusers.d/antdelirium.conf; staged="$bundle_root/usr/lib/sysusers.d/antdelirium.conf" ;;
            tmpfiles) installed=/usr/lib/tmpfiles.d/antdelirium.conf; staged="$bundle_root/usr/lib/tmpfiles.d/antdelirium.conf" ;;
        esac
        cmp -s "$installed" "$staged" || fail 'the installed service contract differs from the reviewed bundle.'
    done

    systemctl stop antdelirium.service
    install_reviewed_bundle
    for operation in state installation; do
        descriptor_plain="$secret_work/$operation-capability.json"
        systemd-creds --name="antdelirium-operator-$operation-v1" decrypt \
            "$operator_credential_root/$operation.cred" "$descriptor_plain" >/dev/null
        chmod 0600 "$descriptor_plain"
        jq -e '
            .contract_version == "admin-capability-v1" and
            (keys | sort) == ["capability", "contract_version"] and
            (.capability | keys | sort) == ["capability_id", "token"] and
            (.capability.capability_id | test("^cap_[0-9a-f]{26}$")) and
            (.capability.token | test("^[A-Za-z0-9_-]{43}$"))
        ' "$descriptor_plain" >/dev/null || fail 'an operator descriptor is invalid.'
        jq -j '.capability.token' "$descriptor_plain" >"$secret_work/$operation.token"
        chmod 0600 "$secret_work/$operation.token"
    done
    state_capability_id=$(jq -r '.capability.capability_id' "$secret_work/state-capability.json")
    installation_capability_id=$(jq -r '.capability.capability_id' "$secret_work/installation-capability.json")
    render_bootstrap_manifest
    printf '%s\n' 'AD-T86-ROOT: resealing a short-lived bootstrap from the preserved operator descriptors.'
    seal_bootstrap
fi

complete_provisioning_test
printf '%s\n' 'AD-T86-ROOT: installed service, one-use provisioning and credential-free restart passed.'
