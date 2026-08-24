#!/usr/bin/env bash
set -euo pipefail

script_dir=$(CDPATH= cd -- "$(dirname -- "$0")" && pwd)
project_root=$(CDPATH= cd -- "$script_dir/../.." && pwd)
release_binary="$project_root/target/x86_64-unknown-linux-gnu/release/antdelirium"
bundle_root=${1:-"$project_root/target/t86-system-installation-bundle"}

fail() {
    printf '%s\n' "AD-T86-BUNDLE: $1" >&2
    exit 1
}

case "$bundle_root" in
    "$project_root"/target/*) ;;
    *) fail 'bundle destination must be inside the project target directory.' ;;
esac

[[ -x "$release_binary" ]] || fail 'the native release binary must be built first.'
command -v sha256sum >/dev/null 2>&1 || fail 'sha256sum is required.'
command -v jq >/dev/null 2>&1 || fail 'jq is required.'
command -v systemd-analyze >/dev/null 2>&1 || fail 'systemd-analyze is required.'
command -v systemd-sysusers >/dev/null 2>&1 || fail 'systemd-sysusers is required.'
command -v systemd-tmpfiles >/dev/null 2>&1 || fail 'systemd-tmpfiles is required.'

service_dir="$bundle_root/usr/lib/systemd/system"
sysusers_dir="$bundle_root/usr/lib/sysusers.d"
tmpfiles_dir="$bundle_root/usr/lib/tmpfiles.d"
libexec_dir="$bundle_root/usr/libexec/antdelirium"
manifest="$bundle_root/system-installation-bundle-v1.json"
validation_root="$project_root/target/t86-systemd-unit-validation"

mkdir -p "$service_dir" "$sysusers_dir" "$tmpfiles_dir" "$libexec_dir"
install -m 0755 "$release_binary" "$libexec_dir/antdelirium"
install -m 0644 "$script_dir/antdelirium.sysusers" "$sysusers_dir/antdelirium.conf"
install -m 0644 "$script_dir/antdelirium.tmpfiles" "$tmpfiles_dir/antdelirium.conf"

sed \
    -e 's|@LIBEXECDIR@|/usr/libexec/antdelirium|g' \
    "$script_dir/antdelirium.service.in" >"$service_dir/antdelirium.service"
sed \
    -e 's|@RUNDIR@|/run/antdelirium|g' \
    "$script_dir/antdelirium-agent.socket.in" >"$service_dir/antdelirium-agent.socket"
sed \
    -e 's|@ADMIN_RUNDIR@|/run/antdelirium-admin|g' \
    "$script_dir/antdelirium-admin.socket.in" >"$service_dir/antdelirium-admin.socket"
chmod 0644 \
    "$service_dir/antdelirium.service" \
    "$service_dir/antdelirium-agent.socket" \
    "$service_dir/antdelirium-admin.socket"

if rg -n '@[A-Z_]+@' "$service_dir" "$sysusers_dir" "$tmpfiles_dir" >/dev/null; then
    fail 'an unresolved deployment placeholder remains.'
fi

mkdir -p "$validation_root"
sed \
    -e "s|/usr/libexec/antdelirium/antdelirium|$libexec_dir/antdelirium|g" \
    "$service_dir/antdelirium.service" >"$validation_root/antdelirium.service"
install -m 0644 \
    "$service_dir/antdelirium-agent.socket" \
    "$service_dir/antdelirium-admin.socket" \
    "$validation_root/"
systemd-analyze verify \
    "$validation_root/antdelirium.service" \
    "$validation_root/antdelirium-agent.socket" \
    "$validation_root/antdelirium-admin.socket" >/dev/null
systemd-sysusers --dry-run "$sysusers_dir/antdelirium.conf" >/dev/null 2>&1
systemd-tmpfiles --cat-config "$tmpfiles_dir/antdelirium.conf" >/dev/null
[[ $(<"$tmpfiles_dir/antdelirium.conf") == \
    'd /run/antdelirium-admin 0750 root antdelirium-admin -' ]] \
    || fail 'administrative tmpfiles contract is invalid.'

binary_sha256=$(sha256sum "$libexec_dir/antdelirium" | cut -d ' ' -f 1)
source_binary_sha256=$(sha256sum "$release_binary" | cut -d ' ' -f 1)
[[ "$binary_sha256" == "$source_binary_sha256" ]] || fail 'staged binary differs from release.'
service_sha256=$(sha256sum "$service_dir/antdelirium.service" | cut -d ' ' -f 1)
agent_socket_sha256=$(sha256sum "$service_dir/antdelirium-agent.socket" | cut -d ' ' -f 1)
admin_socket_sha256=$(sha256sum "$service_dir/antdelirium-admin.socket" | cut -d ' ' -f 1)
sysusers_sha256=$(sha256sum "$sysusers_dir/antdelirium.conf" | cut -d ' ' -f 1)
tmpfiles_sha256=$(sha256sum "$tmpfiles_dir/antdelirium.conf" | cut -d ' ' -f 1)

jq -n \
    --arg binary_sha256 "$binary_sha256" \
    --arg service_sha256 "$service_sha256" \
    --arg agent_socket_sha256 "$agent_socket_sha256" \
    --arg admin_socket_sha256 "$admin_socket_sha256" \
    --arg sysusers_sha256 "$sysusers_sha256" \
    --arg tmpfiles_sha256 "$tmpfiles_sha256" \
    '{
        contract_version: "system-installation-bundle-v1",
        limited_endpoint: "/run/antdelirium/agent.sock",
        service_identity: "antdelirium",
        client_group: "antdelirium-client",
        administrative_group: "antdelirium-admin",
        files: [
            {artifact: "service-binary", destination: "/usr/libexec/antdelirium/antdelirium", mode: "0755", owner: "root", group: "root", sha256: $binary_sha256},
            {artifact: "service-unit", destination: "/usr/lib/systemd/system/antdelirium.service", mode: "0644", owner: "root", group: "root", sha256: $service_sha256},
            {artifact: "limited-socket-unit", destination: "/usr/lib/systemd/system/antdelirium-agent.socket", mode: "0644", owner: "root", group: "root", sha256: $agent_socket_sha256},
            {artifact: "administrative-socket-unit", destination: "/usr/lib/systemd/system/antdelirium-admin.socket", mode: "0644", owner: "root", group: "root", sha256: $admin_socket_sha256},
            {artifact: "system-identities", destination: "/usr/lib/sysusers.d/antdelirium.conf", mode: "0644", owner: "root", group: "root", sha256: $sysusers_sha256},
            {artifact: "administrative-runtime-directory", destination: "/usr/lib/tmpfiles.d/antdelirium.conf", mode: "0644", owner: "root", group: "root", sha256: $tmpfiles_sha256}
        ],
        activation_performed: false,
        global_installation_performed: false
    }' >"$manifest"
chmod 0644 "$manifest"

jq -e '
    .contract_version == "system-installation-bundle-v1" and
    .limited_endpoint == "/run/antdelirium/agent.sock" and
    (.files | length) == 6 and
    ([.files[].artifact] | sort) == ["administrative-runtime-directory", "administrative-socket-unit", "limited-socket-unit", "service-binary", "service-unit", "system-identities"] and
    ([.files[].destination] | sort) == ["/usr/lib/systemd/system/antdelirium-admin.socket", "/usr/lib/systemd/system/antdelirium-agent.socket", "/usr/lib/systemd/system/antdelirium.service", "/usr/lib/sysusers.d/antdelirium.conf", "/usr/lib/tmpfiles.d/antdelirium.conf", "/usr/libexec/antdelirium/antdelirium"] and
    ([.files[].sha256 | test("^[0-9a-f]{64}$")] | all) and
    .activation_performed == false and
    .global_installation_performed == false
' "$manifest" >/dev/null || fail 'bundle manifest validation failed.'

[[ $(find "$bundle_root" -type f | wc -l) -eq 7 ]] \
    || fail 'bundle contains an unexpected file.'
[[ $(stat -c '%a' "$libexec_dir/antdelirium") == 755 ]] \
    || fail 'staged binary mode is invalid.'
for unit in \
    "$service_dir/antdelirium.service" \
    "$service_dir/antdelirium-agent.socket" \
    "$service_dir/antdelirium-admin.socket" \
    "$sysusers_dir/antdelirium.conf" \
    "$tmpfiles_dir/antdelirium.conf" \
    "$manifest"
do
    [[ $(stat -c '%a' "$unit") == 644 ]] || fail 'staged metadata mode is invalid.'
done

printf '%s\n' 'AD-T86-BUNDLE: staged system identities, units and the single release binary.'
