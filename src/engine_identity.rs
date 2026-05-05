#![allow(dead_code)]

use std::path::{Path, PathBuf};

pub const ENGINE_PACKAGE_NAME: &str = "ac_ant_delirium";
pub const ENGINE_DISPLAY_NAME: &str = "AC-Ant-Delirium";
pub const DEFAULT_PLUGIN_BINARY_NAME: &str = "ac_ant_delirium";
pub const DEFAULT_RUNTIME_LOCK_NAME: &str = ".ac_ant_delirium_runtime.lock";
pub const DEFAULT_CONFIG_DIR_NAME: &str = "ant_delirium_config";
pub const DEFAULT_GUARD_RELATIVE_PATH: &str =
    "AC_Ant-Delirium_props/ant_delirium_build_guard.rs";
pub const DEFAULT_PLUGIN_BINARY_PLACEHOLDER: &str = "__ANT_DELIRIUM_PLUGIN_BINARY__";
pub const DEFAULT_BINARY_DISPLAY_PLACEHOLDER: &str = "__ANT_DELIRIUM_BINARY_DISPLAY__";

pub fn default_plugin_binary_path(plugin_dir: &Path) -> PathBuf {
    plugin_dir.join(DEFAULT_PLUGIN_BINARY_NAME)
}

pub fn default_runtime_lock_path(plugin_dir: &Path) -> PathBuf {
    plugin_dir.join(DEFAULT_RUNTIME_LOCK_NAME)
}

pub fn canonical_plugin_binary_example() -> String {
    format!("plugins/<project>/{}", DEFAULT_PLUGIN_BINARY_NAME)
}

pub fn relative_plugin_binary_example() -> String {
    format!("../plugins/<project>/{}", DEFAULT_PLUGIN_BINARY_NAME)
}

pub fn render_identity_template(template: &str) -> String {
    template
        .replace(DEFAULT_PLUGIN_BINARY_PLACEHOLDER, DEFAULT_PLUGIN_BINARY_NAME)
        .replace(DEFAULT_BINARY_DISPLAY_PLACEHOLDER, DEFAULT_PLUGIN_BINARY_NAME)
}
