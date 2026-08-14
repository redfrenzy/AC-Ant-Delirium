use std::ffi::{OsStr, OsString};
use std::path::Path;

use crate::public_output::{OutputRouter, PublicError};

/// Actor requested by the invocation shape. This is routing information, never authorization.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum InvocationProfile {
    Wrapper,
    Agent,
    Reviewer,
    Operator,
    Service,
}

/// Internal behavior selected before any legacy argument parser runs.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum InternalBinaryMode {
    AgentCli,
    WrapperPending {
        recursive: bool,
    },
    ReviewerPending,
    OperatorPending,
    ServicePending,
    #[allow(dead_code)]
    LegacyAgentCliRollback,
    #[allow(dead_code)]
    LegacyGuardRollback,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct DispatchPlan {
    pub(crate) profile: InvocationProfile,
    pub(crate) mode: InternalBinaryMode,
}

pub(crate) fn run_authoritative_binary() {
    let args: Vec<OsString> = std::env::args_os().skip(1).collect();
    let wrapper_depth = std::env::var_os("ANTDELIRIUM_WRAPPER_DEPTH");
    let plan = classify_invocation(&args, wrapper_depth.as_deref());

    match plan.mode {
        InternalBinaryMode::AgentCli => crate::agent_cli::run_agent_cli(),
        InternalBinaryMode::WrapperPending { .. } => {
            OutputRouter::exit(PublicError::ServiceUnavailable)
        }
        InternalBinaryMode::ReviewerPending | InternalBinaryMode::OperatorPending => {
            OutputRouter::exit(PublicError::Authorization)
        }
        InternalBinaryMode::ServicePending => crate::service_authority::run_service_mode(),
        InternalBinaryMode::LegacyAgentCliRollback => crate::run_legacy_cli_mode(),
        InternalBinaryMode::LegacyGuardRollback => crate::legacy_guard_mode::run_rollback(),
    }
}

/// Classifies an invocation without consulting cwd, process name, user-controlled profile flags,
/// or protected state. A requested privileged profile reaches only a closed placeholder until its
/// authenticated handler exists; it never falls through to the mature legacy parser.
pub(crate) fn classify_invocation(
    args: &[OsString],
    wrapper_depth: Option<&OsStr>,
) -> DispatchPlan {
    if is_unambiguous_wrapper_invocation(args) {
        return DispatchPlan {
            profile: InvocationProfile::Wrapper,
            mode: InternalBinaryMode::WrapperPending {
                recursive: wrapper_depth_is_active(wrapper_depth),
            },
        };
    }

    let first = args.first().and_then(|arg| arg.to_str());
    match first {
        Some("service" | "daemon") => DispatchPlan {
            profile: InvocationProfile::Service,
            mode: InternalBinaryMode::ServicePending,
        },
        Some("review" | "reviewer") => DispatchPlan {
            profile: InvocationProfile::Reviewer,
            mode: InternalBinaryMode::ReviewerPending,
        },
        Some("operator" | "install" | "uninstall" | "migrate-plugin" | "doctor") => DispatchPlan {
            profile: InvocationProfile::Operator,
            mode: InternalBinaryMode::OperatorPending,
        },
        _ => DispatchPlan {
            profile: InvocationProfile::Agent,
            mode: InternalBinaryMode::AgentCli,
        },
    }
}

fn is_unambiguous_wrapper_invocation(args: &[OsString]) -> bool {
    let Some(compiler) = args.first() else {
        return false;
    };
    if !is_rustc_path(compiler) {
        return false;
    }

    args.iter().skip(1).any(|arg| is_rustc_marker(arg))
}

fn is_rustc_path(value: &OsStr) -> bool {
    let path = Path::new(value);
    let Some(file_name) = path.file_name().and_then(OsStr::to_str) else {
        return false;
    };
    if !matches!(file_name, "rustc" | "rustc.exe") {
        return false;
    }

    path.is_absolute() || path.components().count() > 1
}

fn is_rustc_marker(value: &OsStr) -> bool {
    let Some(value) = value.to_str() else {
        return false;
    };
    matches!(
        value,
        "--crate-name"
            | "--crate-type"
            | "--emit"
            | "--print"
            | "--target"
            | "--sysroot"
            | "--version"
            | "-vV"
    ) || value.starts_with("--crate-name=")
        || value.starts_with("--crate-type=")
        || value.starts_with("--emit=")
        || value.starts_with("--print=")
        || value.starts_with("--target=")
        || value.starts_with("--sysroot=")
}

fn wrapper_depth_is_active(value: Option<&OsStr>) -> bool {
    value
        .and_then(OsStr::to_str)
        .and_then(|value| value.parse::<u32>().ok())
        .is_some_and(|depth| depth > 0)
}

#[cfg(test)]
mod tests {
    use super::*;
    use ac_ant_delirium::engine_identity;

    fn args(values: &[&str]) -> Vec<OsString> {
        values.iter().map(OsString::from).collect()
    }

    #[test]
    fn cargo_manifest_declares_only_the_authoritative_binary() {
        let manifest = include_str!("../Cargo.toml");

        assert_eq!(manifest.matches("[[bin]]").count(), 1);
        assert!(manifest.contains("name = \"antdelirium\""));
        assert!(manifest.contains("path = \"src/main.rs\""));
        assert_eq!(
            env!("CARGO_BIN_NAME"),
            engine_identity::AUTHORITATIVE_BINARY_NAME
        );
    }

    #[test]
    fn classifies_all_five_profiles_before_legacy_parsing() {
        let cases = [
            (
                args(&["/toolchain/bin/rustc", "--crate-name", "demo"]),
                InvocationProfile::Wrapper,
                InternalBinaryMode::WrapperPending { recursive: false },
            ),
            (
                args(&["--api", "hierarchy.task.current"]),
                InvocationProfile::Agent,
                InternalBinaryMode::AgentCli,
            ),
            (
                args(&["review", "assigned"]),
                InvocationProfile::Reviewer,
                InternalBinaryMode::ReviewerPending,
            ),
            (
                args(&["install"]),
                InvocationProfile::Operator,
                InternalBinaryMode::OperatorPending,
            ),
            (
                args(&["service"]),
                InvocationProfile::Service,
                InternalBinaryMode::ServicePending,
            ),
        ];

        for (arguments, expected_profile, expected_mode) in cases {
            let plan = classify_invocation(&arguments, None);
            assert_eq!(plan.profile, expected_profile);
            assert_eq!(plan.mode, expected_mode);
        }
    }

    #[test]
    fn wrapper_requires_a_rustc_path_and_a_compiler_marker() {
        for arguments in [
            args(&["rustc", "--crate-name", "demo"]),
            args(&["/toolchain/bin/rustc", "input.rs"]),
            args(&["/toolchain/bin/compiler", "--crate-name", "demo"]),
            args(&["--api", "/toolchain/bin/rustc", "--crate-name"]),
        ] {
            assert_eq!(
                classify_invocation(&arguments, None).profile,
                InvocationProfile::Agent
            );
        }

        assert_eq!(
            classify_invocation(
                &args(&["C:/toolchain/bin/rustc.exe", "--print", "sysroot"]),
                None
            )
            .profile,
            InvocationProfile::Wrapper
        );
    }

    #[test]
    fn wrapper_depth_changes_only_an_already_valid_wrapper_invocation() {
        let wrapper = args(&["/toolchain/bin/rustc", "--emit=metadata"]);
        assert_eq!(
            classify_invocation(&wrapper, Some(OsStr::new("1"))).mode,
            InternalBinaryMode::WrapperPending { recursive: true }
        );
        assert_eq!(
            classify_invocation(&args(&["install"]), Some(OsStr::new("1"))).profile,
            InvocationProfile::Operator
        );
        assert_eq!(
            classify_invocation(&args(&["--api", "hierarchy"]), Some(OsStr::new("999"))).profile,
            InvocationProfile::Agent
        );
    }

    #[test]
    fn every_administrative_entry_is_routed_away_from_the_legacy_parser() {
        for command in [
            "operator",
            "install",
            "uninstall",
            "migrate-plugin",
            "doctor",
        ] {
            let plan = classify_invocation(&args(&[command]), None);
            assert_eq!(plan.profile, InvocationProfile::Operator);
            assert_eq!(plan.mode, InternalBinaryMode::OperatorPending);
        }

        for command in ["service", "daemon", "review", "reviewer"] {
            assert_ne!(
                classify_invocation(&args(&[command]), None).mode,
                InternalBinaryMode::AgentCli
            );
        }
    }

    #[test]
    fn legacy_entrypoints_remain_compiled_only_as_internal_rollback_modes() {
        assert_eq!(
            InternalBinaryMode::LegacyAgentCliRollback,
            InternalBinaryMode::LegacyAgentCliRollback
        );
        assert_eq!(
            InternalBinaryMode::LegacyGuardRollback,
            InternalBinaryMode::LegacyGuardRollback
        );
    }

    #[test]
    fn legacy_runtime_source_has_no_predictable_global_log_files() {
        let legacy_runtime = include_str!("Main-Logic_unitcore.rs");
        for forbidden in ["/tmp/delirium_", "delirium_debug.log", "delirium_error.log"] {
            assert!(
                !legacy_runtime.contains(forbidden),
                "legacy runtime still exposes predictable log: {forbidden}"
            );
        }
    }
}
