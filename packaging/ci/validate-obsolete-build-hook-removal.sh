#!/usr/bin/env bash
set -euo pipefail

script_dir=$(CDPATH= cd -- "$(dirname -- "$0")" && pwd)
repository_root=$(CDPATH= cd -- "$script_dir/../.." && pwd)
build_script="$repository_root/build.rs"
run_id=$(date -u +%Y%m%dT%H%M%SZ)-${BASHPID:-$$}
log_root="$repository_root/target/t103-build-hook-removal-logs/$run_id"
cargo_bin=$(command -v cargo || true)

fail() {
    printf '%s\n' "AD-T103-BUILD-HOOK: $1" >&2
    exit 1
}

for required_command in cargo rg; do
    command -v "$required_command" >/dev/null 2>&1 \
        || fail "required command is unavailable: $required_command"
done
[[ -f "$build_script" && ! -L "$build_script" ]] \
    || fail 'the package build script is unavailable.'

for forbidden in \
    run_governance_build_hook \
    ANT_DELIRIUM_BIN \
    ANT_DELIRIUM_CONFIG \
    ANT_DELIRIUM_PROJECT_ROOT \
    ANT_DELIRIUM_ENFORCE_BUILD_HOOK \
    ANT_DELIRIUM_SKIP_BUILD_HOOK \
    governance\ binary\ not\ found \
    ../plugins/ \
    --project-root \
    --config
do
    if rg -F -q -- "$forbidden" "$build_script"; then
        fail 'the package build script still contains an obsolete plugin hook input.'
    fi
done
rg -F -q -- 'generated_static_hierarchy.rs' "$build_script" \
    || fail 'the retained taxonomy compiler is absent.'
rg -F -q -- 'ANT_DELIRIUM_BUILD_GIT_HASH' "$build_script" \
    || fail 'the retained version metadata is absent.'

umask 077
mkdir -p "$log_root"
printf '%s\n' 'AD-T103-BUILD-HOOK: running Cargo with hostile obsolete selectors.'
(
    cd "$repository_root"
    env \
        ANT_DELIRIUM_BIN='/outside/obsolete/plugin-binary' \
        ANT_DELIRIUM_CONFIG='/outside/obsolete/config' \
        ANT_DELIRIUM_PROJECT_ROOT='/outside/obsolete/project' \
        ANT_DELIRIUM_ENFORCE_BUILD_HOOK=1 \
        ANT_DELIRIUM_SKIP_BUILD_HOOK=0 \
        "$cargo_bin" check --locked --offline --all-targets \
            --target x86_64-unknown-linux-gnu
) >"$log_root/cargo.stdout" 2>"$log_root/cargo.stderr" \
    || fail 'Cargo failed after removal of the obsolete package hook.'

if rg -F -q -- '/outside/obsolete/' "$log_root/cargo.stdout" "$log_root/cargo.stderr" \
    || rg -q 'governance binary not found|governance build hook|plugins/ac_ant_delirium' \
        "$log_root/cargo.stdout" "$log_root/cargo.stderr"; then
    fail 'obsolete hook inputs affected the Cargo output.'
fi

cat >"$log_root/evidence.json" <<'EOF'
{
  "contract_version": "obsolete-package-build-hook-removal-v1",
  "legacy_plugin_execution": "absent",
  "legacy_path_selectors": "ignored-by-absence",
  "taxonomy_compiler": "retained",
  "version_metadata": "retained",
  "ordinary_cargo_check": "passed",
  "external_project_access": "forbidden",
  "validation": "pass"
}
EOF

printf '%s\n' 'AD-T103-BUILD-HOOK: obsolete plugin execution was removed from the package build script.'
printf '%s\n' "AD-T103-BUILD-HOOK: logs=$log_root"
