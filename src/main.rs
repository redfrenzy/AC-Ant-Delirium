#![allow(non_snake_case)]

#[path = "Agent-Cli_unit.rs"]
mod agent_cli;

#[path = "Reviewer-Cli_unit.rs"]
mod reviewer_cli;

#[path = "Public-Output_unit.rs"]
mod public_output;

#[path = "Cargo-Installer_unit.rs"]
mod cargo_installer;

#[path = "Cargo-Doctor_unit.rs"]
mod cargo_doctor;

#[allow(dead_code)]
#[path = "Cargo-Config_unit.rs"]
mod cargo_config;

#[path = "Service-Authority_unit.rs"]
mod service_authority;

#[path = "Service-Storage_unit.rs"]
mod service_storage;

#[path = "Task-Package-Install_unit.rs"]
mod task_package_install;

#[path = "Protected-Git-Backup_unit.rs"]
mod protected_git_backup;

#[path = "Protected-Gate_unit.rs"]
mod protected_gate;

#[path = "Service-Provisioning_unit.rs"]
mod service_provisioning;

#[path = "Service-Channel_unit.rs"]
mod service_channel;

#[path = "Service-Authentication_unit.rs"]
mod service_authentication;

#[path = "Service-Hardening_unit.rs"]
mod service_hardening;

#[path = "Binary-Mode_unit.rs"]
mod binary_mode;

#[path = "Rustc-Wrapper_unit.rs"]
mod rustc_wrapper;

#[path = "Limited-Protocol_unit.rs"]
mod limited_protocol;

#[path = "Legacy-Staging_unit.rs"]
mod legacy_staging;

#[path = "Legacy-Normalization_unit.rs"]
mod legacy_normalization;

#[path = "Legacy-Dry-Run_unit.rs"]
mod legacy_dry_run;

#[path = "Legacy-Conversion_unit.rs"]
mod legacy_conversion;

#[path = "Legacy-Attestation_unit.rs"]
mod legacy_attestation;

#[path = "Legacy-Approval_unit.rs"]
mod legacy_approval;

#[path = "Legacy-Cutover_unit.rs"]
mod legacy_cutover;

#[path = "Administrative-Protocol_unit.rs"]
mod administrative_protocol;

#[allow(dead_code)]
#[path = "Task-Package-Contracts_unit.rs"]
mod task_package_contracts;

#[allow(dead_code)]
#[path = "Task-Package-Generator_unit.rs"]
mod task_package_generator;

#[allow(dead_code)]
#[path = "Task-Package-Store_unit.rs"]
mod task_package_store;

fn main() {
    binary_mode::run_authoritative_binary();
}
