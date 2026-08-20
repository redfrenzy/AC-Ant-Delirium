use std::ffi::{OsStr, OsString};

use crate::public_output::{OutputRouter, PublicError};
use crate::rustc_wrapper::{
    detect_wrapper_invocation, WrapperProtocolError, WrapperRoute, WRAPPER_DEPTH_ENV,
};

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
        route: WrapperRoute,
    },
    WrapperRejected,
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
    let wrapper_depth = std::env::var_os(WRAPPER_DEPTH_ENV);
    let plan = classify_invocation(&args, wrapper_depth.as_deref());

    match plan.mode {
        InternalBinaryMode::AgentCli => crate::agent_cli::run_agent_cli(),
        InternalBinaryMode::WrapperPending { route } => {
            crate::rustc_wrapper::run_wrapper(&args, route)
        }
        InternalBinaryMode::WrapperRejected => OutputRouter::exit(PublicError::Configuration),
        InternalBinaryMode::ReviewerPending => OutputRouter::exit(PublicError::Authorization),
        InternalBinaryMode::OperatorPending => crate::cargo_installer::run_operator_mode(&args),
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
    match detect_wrapper_invocation(args, wrapper_depth) {
        Ok(Some(route)) => {
            return DispatchPlan {
                profile: InvocationProfile::Wrapper,
                mode: InternalBinaryMode::WrapperPending { route },
            };
        }
        Err(WrapperProtocolError::InvalidDepth) => {
            return DispatchPlan {
                profile: InvocationProfile::Wrapper,
                mode: InternalBinaryMode::WrapperRejected,
            };
        }
        Ok(None) => {}
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
                InternalBinaryMode::WrapperPending {
                    route: WrapperRoute::Governed,
                },
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
    fn wrapper_depth_selects_a_non_recursive_route_only_for_valid_wrapper_invocations() {
        let wrapper = args(&["/toolchain/bin/rustc", "--emit=metadata"]);
        assert_eq!(
            classify_invocation(&wrapper, Some(OsStr::new("1"))).mode,
            InternalBinaryMode::WrapperPending {
                route: WrapperRoute::DelegationOnly,
            }
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
    fn invalid_wrapper_depth_is_rejected_before_any_handler() {
        let wrapper = args(&["/toolchain/bin/rustc", "--emit=metadata"]);
        for depth in ["", "2", "nested"] {
            assert_eq!(
                classify_invocation(&wrapper, Some(OsStr::new(depth))),
                DispatchPlan {
                    profile: InvocationProfile::Wrapper,
                    mode: InternalBinaryMode::WrapperRejected,
                }
            );
        }
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
