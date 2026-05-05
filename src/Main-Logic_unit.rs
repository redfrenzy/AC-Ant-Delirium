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

/// [WHAT] Entry point focado na revelação ativa de um manifesto e auditoria de governança.
fn main() {
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
#[path = "tests/Main-Logic_test.rs"]
mod tests;
