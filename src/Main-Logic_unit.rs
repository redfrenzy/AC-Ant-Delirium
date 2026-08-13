#[cfg(test)]
use std::env;
#[cfg(test)]
use std::io;
#[cfg(test)]
use std::path::{Path, PathBuf};
#[cfg(test)]
use ac_ant_delirium::structs::{AuditConfig, CliArgs, HierarchyConfig, PluginContext};

#[path = "Main-Logic_unitcore.rs"]
mod main_logic_unitcore;

/// [WHAT] Modo interno que preserva a CLI madura durante a unificação do executável.
pub(crate) fn run_legacy_cli_mode() {
    main_logic_unitcore::run_main();
}

#[cfg(test)]
fn parse_cli_args(args: &[String]) -> CliArgs {
    main_logic_unitcore::parse_cli_args(args)
}

#[cfg(test)]
fn detect_project_root(cli_root: Option<&str>) -> PathBuf {
    main_logic_unitcore::detect_project_root(cli_root)
}

#[cfg(test)]
fn load_hierarchy_from_path(path: &Path) -> HierarchyConfig {
    main_logic_unitcore::load_hierarchy_from_path(path)
}

#[cfg(test)]
fn refresh_runtime_governance_state(
    plugin: &PluginContext,
    config: &AuditConfig,
) -> io::Result<()> {
    main_logic_unitcore::refresh_runtime_governance_state(plugin, config)
}

#[cfg(test)]
fn cargo_warning_parameter(hierarchy: &HierarchyConfig, parameter: &str) -> Option<String> {
    main_logic_unitcore::cargo_warning_parameter(hierarchy, parameter)
}

#[cfg(test)]
fn render_cargo_warning(
    hierarchy: &HierarchyConfig,
    template_key: &str,
    plan_id: &str,
    pending_tasks: usize,
    mode: &str,
    instruction: &str,
) -> Option<String> {
    main_logic_unitcore::render_cargo_warning(
        hierarchy,
        template_key,
        plan_id,
        pending_tasks,
        mode,
        instruction,
    )
}

#[cfg(test)]
fn render_warning_template(
    template: &str,
    plan_id: &str,
    pending_tasks: usize,
    mode: &str,
    instruction: &str,
    manifest: &str,
) -> String {
    main_logic_unitcore::render_warning_template(
        template,
        plan_id,
        pending_tasks,
        mode,
        instruction,
        manifest,
    )
}

#[cfg(test)]
#[path = "tests/Main-Logic_test.rs"]
mod tests;

#[cfg(test)]
#[path = "tests/Legacy_Guard_Api_Parity_test.rs"]
mod legacy_guard_api_parity_tests;
