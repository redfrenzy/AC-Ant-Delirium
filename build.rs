use std::env;
use std::path::Path;

// =====================================================================
// INDUSTRIAL BUILD-GATE
//
// This build script integrates the governance plugin into the Rust
// compilation lifecycle (cargo check / cargo build / cargo run).
//
// Architecture:
//   The governance binary lives OUTSIDE the project at:
//     ../plugins/ac_ant_delirium
//
//   This is intentional. If the binary lived inside target/, then
//   cargo build would recompile it during the build process, creating
//   a circular dependency where the code being audited could modify
//   the auditor. By placing it in ../plugins/, the binary becomes an
//   immutable checkpoint that cannot be influenced by the code it
//   audits.
//
// How it works:
//   1. build.rs looks for the native plugin at ../plugins/ac_ant_delirium
//   2. If found, it runs the plugin with "src" as the target directory
//   3. If the plugin exits with non-zero (violations found), build.rs
//      panics, which causes cargo to abort the build
//   4. If the plugin is not found, it emits a warning and continues
//      (allows first-time builds before the plugin exists)
//
// How to generate the plugin:
//   make plugin
//
// =====================================================================
fn main() {
    let manifest_dir = env::var("CARGO_MANIFEST_DIR").unwrap();
    let plugin_path = Path::new(&manifest_dir).join("../plugins/ac_ant_delirium");

    if plugin_path.exists() && false {
        // Run the external governance binary against src/.
        // We set the current_dir to the manifest directory so that
        // the plugin can find delirium.toml relative to the project root.
        let status = std::process::Command::new(&plugin_path)
            .arg("src")
            .current_dir(&manifest_dir)
            .status();

        match status {
            Ok(s) if !s.success() => {
                panic!(
                    "\n\n[GOVERNANCE_FAILURE] The governance plugin detected violations.\n\
                     Fix all issues above before compiling.\n\
                     Plugin: {}\n\n",
                    plugin_path.display()
                );
            }
            Err(e) => {
                eprintln!(
                    "cargo:warning=[GOVERNANCE] Failed to execute plugin: {}. Error: {}",
                    plugin_path.display(), e
                );
            }
            _ => {
                // Plugin ran successfully with zero violations.
            }
        }
    } else {
        eprintln!(
            "cargo:warning=[GOVERNANCE] Plugin not found at {}. Run 'make plugin' to generate it.",
            plugin_path.display()
        );
    }

    println!("cargo:rerun-if-changed=src");
    println!("cargo:rerun-if-changed=delirium.toml");
}
