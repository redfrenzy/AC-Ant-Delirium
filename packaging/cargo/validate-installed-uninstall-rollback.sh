#!/usr/bin/env bash
set -euo pipefail

script_dir=$(CDPATH= cd -- "$(dirname -- "$0")" && pwd)
repository_root=$(CDPATH= cd -- "$script_dir/../.." && pwd)
release_binary="$repository_root/target/x86_64-unknown-linux-gnu/release/antdelirium"
limited_socket=/run/antdelirium/agent.sock
cargo_home=${CARGO_HOME:-"$HOME/.cargo"}
installed_binary="$cargo_home/bin/antdelirium"
run_id=$(date -u +%Y%m%dT%H%M%SZ)-${BASHPID:-$$}
log_root="$repository_root/target/t8g-cargo-rollback-logs/$run_id"
fixture_home="$log_root/sentinel-cargo-home"
real_uninstalled=0
recovery=not-needed

fail() {
    printf '%s\n' "AD-T8G-CARGO-ROLLBACK: $1" >&2
    exit 1
}

sha_file() {
    sha256sum "$1" | cut -d ' ' -f 1
}

link_digest() {
    readlink "$1" | sha256sum | cut -d ' ' -f 1
}

restore_real_installation() {
    if [[ $real_uninstalled -eq 1 && -x "$release_binary" ]]; then
        if [[ -e "$backup_root" ]]; then
            CARGO_HOME="$cargo_home" "$release_binary" uninstall \
                >"$log_root/recovery-uninstall.stdout" 2>"$log_root/recovery-uninstall.stderr" \
                || true
        fi
        if ANTDELIRIUM_LIMITED_SOCKET="$limited_socket" \
            CARGO_HOME="$cargo_home" "$release_binary" install \
                >"$log_root/recovery-install.stdout" 2>"$log_root/recovery-install.stderr"; then
            real_uninstalled=0
            recovery=pass
            return 0
        fi
        recovery=failed
        return 1
    fi
}

finish() {
    local exit_code=$?
    set +e
    if [[ $exit_code -ne 0 ]]; then
        restore_real_installation
    fi
    {
        printf 'contract=antdelirium-installed-cargo-rollback-v1\n'
        printf 'run_id=%s\n' "$run_id"
        printf 'exit_code=%s\n' "$exit_code"
        printf 'recovery=%s\n' "$recovery"
        printf 'physical_locators=redacted\n'
        printf 'service_state_mutation=forbidden\n'
        printf 'completed_at=%s\n' "$(date -u +%Y-%m-%dT%H:%M:%SZ)"
    } >"$log_root/result.env"
    chmod -R a+rX "$log_root"
    printf '%s\n' "AD-T8G-CARGO-ROLLBACK: logs=$log_root"
    exit "$exit_code"
}

[[ ${1:-} == --apply && $# -eq 1 ]] \
    || fail 'use --apply to execute the recoverable installed Cargo rollback gate.'
[[ $EUID -ne 0 ]] || fail 'run this gate as the non-root Cargo owner.'
[[ "$cargo_home" == /* && -d "$cargo_home" && ! -L "$cargo_home" ]] \
    || fail 'Cargo home must be an absolute, existing, non-symlink directory.'

for command in cargo rustc jq sha256sum stat readlink install mkdir cmp systemctl; do
    command -v "$command" >/dev/null 2>&1 \
        || fail "required command is unavailable: $command"
done
[[ -x "$release_binary" && ! -L "$release_binary" ]] \
    || fail 'the reviewed release binary is unavailable.'
[[ -x "$installed_binary" && ! -L "$installed_binary" ]] \
    || fail 'the installed Cargo wrapper is unavailable.'
[[ -S "$limited_socket" ]] || fail 'the limited service socket is unavailable.'
[[ $(systemctl is-active antdelirium.service 2>/dev/null || true) == active ]] \
    || fail 'the protected service must remain active throughout the Cargo-only gate.'

umask 077
install -d -m 0700 "$log_root" "$fixture_home" "$fixture_home/bin"
trap finish EXIT

release_hash=$(sha_file "$release_binary")
installed_hash=$(sha_file "$installed_binary")
[[ "$release_hash" == "$installed_hash" ]] \
    || fail 'the installed wrapper differs from the reviewed release.'

cargo_proxy="$cargo_home/bin/cargo"
rustc_proxy="$cargo_home/bin/rustc"
[[ -L "$cargo_proxy" && -L "$rustc_proxy" ]] \
    || fail 'the Cargo and rustc rustup proxies are not symbolic links.'
cargo_link_before=$(link_digest "$cargo_proxy")
rustc_link_before=$(link_digest "$rustc_proxy")
cargo_hash_before=$(sha_file "$cargo_proxy")
rustc_hash_before=$(sha_file "$rustc_proxy")

backup_root="$cargo_home/.antdelirium-install"
backup_receipt="$backup_root/config-backup-v1.json"
backup_payload="$backup_root/config.toml.original"
config_file="$cargo_home/config.toml"
[[ -d "$backup_root" && ! -L "$backup_root" \
    && -f "$backup_receipt" && ! -L "$backup_receipt" \
    && -f "$backup_payload" && ! -L "$backup_payload" \
    && -f "$config_file" && ! -L "$config_file" ]] \
    || fail 'the installed Cargo configuration ownership proof is unavailable.'
jq -e '.contract_version == "cargo-config-backup-v1"' "$backup_receipt" >/dev/null \
    || fail 'the installed Cargo ownership receipt is invalid.'
original_present=$(jq -r '.original_present' "$backup_receipt")
original_hash=$(jq -r '.original_sha256' "$backup_receipt")
original_mode=$(jq -r '.original_mode // empty' "$backup_receipt")
installed_config_hash=$(jq -r '.installed_sha256' "$backup_receipt")
[[ "$original_present" == true || "$original_present" == false ]] \
    || fail 'the installed Cargo ownership receipt has an invalid presence flag.'
expected_installed_mode=600
if [[ "$original_present" == true ]]; then
    [[ "$original_mode" =~ ^[0-9]+$ ]] \
        || fail 'the installed Cargo ownership receipt has an invalid original mode.'
    printf -v original_mode_octal '%o' "$original_mode"
    expected_installed_mode=$original_mode_octal
fi
[[ $(sha_file "$backup_payload") == "$original_hash" \
    && $(sha_file "$config_file") == "$installed_config_hash" \
    && $(stat -c '%a' "$config_file") == "$expected_installed_mode" ]] \
    || fail 'the installed Cargo configuration hashes do not match their ownership proof.'

sentinel_config="$fixture_home/config.toml"
printf '[net]\nretry = 7\n\n[build]\njobs = 2\n' >"$sentinel_config"
chmod 0640 "$sentinel_config"
sentinel_hash=$(sha_file "$sentinel_config")
sentinel_mode=$(stat -c '%a' "$sentinel_config")
printf '%s\n' 'AD-T8G-CARGO-ROLLBACK: proving preservation of unrelated configuration in a disposable Cargo home.'
ANTDELIRIUM_LIMITED_SOCKET="$limited_socket" CARGO_HOME="$fixture_home" \
    "$release_binary" install >"$log_root/sentinel-install.stdout"
[[ -x "$fixture_home/bin/antdelirium" ]] \
    || fail 'the disposable installation did not publish its wrapper.'
CARGO_HOME="$fixture_home" "$fixture_home/bin/antdelirium" uninstall \
    >"$log_root/sentinel-uninstall.stdout"
[[ ! -e "$fixture_home/bin/antdelirium" \
    && ! -e "$fixture_home/.antdelirium-install" \
    && $(sha_file "$sentinel_config") == "$sentinel_hash" \
    && $(stat -c '%a' "$sentinel_config") == "$sentinel_mode" ]] \
    || fail 'uninstall did not restore the unrelated sentinel configuration exactly.'

printf '%s\n' 'AD-T8G-CARGO-ROLLBACK: uninstalling only the owned real Cargo integration.'
real_uninstalled=1
CARGO_HOME="$cargo_home" "$installed_binary" uninstall \
    >"$log_root/real-uninstall.stdout"
[[ ! -e "$installed_binary" && ! -e "$backup_root" ]] \
    || fail 'the owned wrapper or its ownership directory survived uninstall.'
if [[ "$original_present" == true ]]; then
    [[ -f "$config_file" && ! -L "$config_file" \
        && $(sha_file "$config_file") == "$original_hash" \
        && $(stat -c '%a' "$config_file") == "$original_mode_octal" ]] \
        || fail 'the original real Cargo configuration was not restored.'
else
    [[ ! -e "$config_file" ]] \
        || fail 'the installation-created Cargo configuration survived uninstall.'
fi
[[ $(link_digest "$cargo_proxy") == "$cargo_link_before" \
    && $(link_digest "$rustc_proxy") == "$rustc_link_before" \
    && $(sha_file "$cargo_proxy") == "$cargo_hash_before" \
    && $(sha_file "$rustc_proxy") == "$rustc_hash_before" ]] \
    || fail 'a rustup proxy changed during uninstall.'
cargo --version >"$log_root/cargo-after-uninstall.stdout"
rustc --version >"$log_root/rustc-after-uninstall.stdout"

printf '%s\n' 'AD-T8G-CARGO-ROLLBACK: reinstalling the reviewed wrapper and Cargo integration.'
ANTDELIRIUM_LIMITED_SOCKET="$limited_socket" CARGO_HOME="$cargo_home" \
    "$release_binary" install >"$log_root/real-reinstall.stdout"
real_uninstalled=0
[[ -x "$installed_binary" && ! -L "$installed_binary" \
    && $(stat -c '%a' "$installed_binary") == 755 \
    && $(sha_file "$installed_binary") == "$release_hash" ]] \
    || fail 'the reviewed wrapper was not restored exactly.'
[[ -f "$config_file" && $(sha_file "$config_file") == "$installed_config_hash" ]] \
    || fail 'the owned Cargo integration was not restored exactly.'
[[ $(link_digest "$cargo_proxy") == "$cargo_link_before" \
    && $(link_digest "$rustc_proxy") == "$rustc_link_before" \
    && $(sha_file "$cargo_proxy") == "$cargo_hash_before" \
    && $(sha_file "$rustc_proxy") == "$rustc_hash_before" ]] \
    || fail 'a rustup proxy changed during reinstall.'
"$installed_binary" doctor >"$log_root/doctor.stdout"
jq -Rn '[inputs] | any(. == "result=ready")' <"$log_root/doctor.stdout" \
    | jq -e '. == true' >/dev/null \
    || fail 'doctor did not report the restored integration ready.'

{
    printf 'contract=antdelirium-installed-cargo-rollback-evidence-v1\n'
    printf 'release_sha256=%s\n' "$release_hash"
    printf 'sentinel_config_restored=exact\n'
    printf 'real_original_config_restored=exact\n'
    printf 'cargo_proxy_preserved=yes\n'
    printf 'rustc_proxy_preserved=yes\n'
    printf 'cargo_proxy_after_uninstall=pass\n'
    printf 'rustc_proxy_after_uninstall=pass\n'
    printf 'wrapper_reinstalled=exact\n'
    printf 'cargo_integration_reinstalled=exact\n'
    printf 'doctor=ready\n'
    printf 'service_state_mutation=none\n'
} >"$log_root/acceptance.env"
printf '%s\n' 'AD-T8G-CARGO-ROLLBACK: installed Cargo uninstall and restoration passed.'
