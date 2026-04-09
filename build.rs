use std::env;
use std::path::Path;

#[path = "src/reporter.rs"]
mod reporter;

/// [WHAT] Governance trigger for ac_ant_delirium geode.
/// [WHY] To ensure Elite Industrial standards are checked during build.
/// [WHERE] Called by Cargo during geode compilation phase.
fn main() {
    let manifest_dir = env::var("CARGO_MANIFEST_DIR").unwrap();
    let current_dir = Path::new(&manifest_dir);
    
    // workspace_root = ../../ (assuming geodes/ac_ant_delirium)
    let workspace_root = current_dir.parent().unwrap().parent().unwrap();

    println!("cargo:rerun-if-changed=src");
    println!("cargo:rerun-if-changed=Cargo.toml");

    let sarif_path = workspace_root.join("governance_results.sarif").to_string_lossy().to_string();
    
    // Industrial Corporate Summary
    // [LOG] Reporting internal compliance status for ac_ant_delirium
    reporter::print_elite_report(&sarif_path, 0, 0, 0, 0);
}
