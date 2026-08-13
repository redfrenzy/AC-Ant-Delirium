#![allow(non_snake_case)]

#[path = "Agent-Runtime_unit.rs"]
mod agent_runtime;

#[path = "Git_unit.rs"]
mod git_unit;

#[path = "Agent-Cli_unit.rs"]
mod agent_cli;

#[path = "Public-Output_unit.rs"]
mod public_output;

#[path = "Service-Authority_unit.rs"]
mod service_authority;

#[path = "Service-Storage_unit.rs"]
mod service_storage;

#[path = "Service-Channel_unit.rs"]
mod service_channel;

#[path = "Service-Authentication_unit.rs"]
mod service_authentication;

#[path = "Service-Hardening_unit.rs"]
mod service_hardening;

#[path = "Binary-Mode_unit.rs"]
mod binary_mode;

mod legacy_guard_mode {
    include!("../get_toml_for_new/ant_delirium_build_guard.rs");

    pub(super) fn run_rollback() -> ! {
        main();
        std::process::exit(1);
    }
}

include!("Main-Logic_unit.rs");

fn main() {
    binary_mode::run_authoritative_binary();
}
