use super::*;
use crate::binary_mode::{classify_invocation, InternalBinaryMode, InvocationProfile};

fn args(values: &[&str]) -> Vec<OsString> {
    values.iter().map(OsString::from).collect()
}

#[test]
fn public_version_contains_only_package_commit_and_agent_schema() {
    let output = execute(
        &args(&["--version"]),
        &Err(PublicError::ServiceUnavailable),
        &UnavailableSubmissionTransport,
    )
    .expect("version must not inspect cwd or protected state");
    let output = output.output();
    let lines = output.lines().collect::<Vec<_>>();

    assert_eq!(lines.len(), 3);
    assert_eq!(
        lines[0],
        format!("antdelirium {}", env!("CARGO_PKG_VERSION"))
    );
    assert_eq!(lines[2], "schema: agent-projection-v1");

    let commit = lines[1]
        .strip_prefix("commit: ")
        .expect("commit label must be stable");
    assert!(
        commit == "unknown"
            || ((7..=64).contains(&commit.len())
                && commit.bytes().all(|byte| byte.is_ascii_hexdigit()))
    );
    for forbidden in [
        "/home/",
        "/var/",
        "plugins/",
        "plan/",
        "state.db",
        "admin-protocol",
        "canonical-state",
        "migration-manifest",
        "build_unix",
    ] {
        assert!(!output.contains(forbidden), "version leaked {forbidden}");
    }
}

#[test]
fn only_the_direct_agent_version_form_is_public() {
    assert_eq!(
        parse_command(&args(&["--version"])),
        Ok(AgentCommand::Version)
    );
    assert_eq!(
        parse_command(&args(&["--version", "extra"])),
        Err(PublicError::Usage)
    );

    let direct = classify_invocation(&args(&["--version"]), None);
    assert_eq!(direct.profile, InvocationProfile::Agent);
    assert_eq!(direct.mode, InternalBinaryMode::AgentCli);

    for (arguments, expected_profile, expected_mode, expected_error) in [
        (
            args(&["operator", "--version"]),
            InvocationProfile::Operator,
            InternalBinaryMode::OperatorPending,
            PublicError::Authorization,
        ),
        (
            args(&["review", "--version"]),
            InvocationProfile::Reviewer,
            InternalBinaryMode::ReviewerPending,
            PublicError::Authorization,
        ),
        (
            args(&["service", "--version"]),
            InvocationProfile::Service,
            InternalBinaryMode::ServicePending,
            PublicError::Authorization,
        ),
        (
            args(&["/toolchain/bin/rustc", "--version"]),
            InvocationProfile::Wrapper,
            InternalBinaryMode::WrapperPending {
                route: crate::rustc_wrapper::WrapperRoute::Governed,
            },
            PublicError::ToolchainNotFound,
        ),
    ] {
        let plan = classify_invocation(&arguments, None);
        assert_eq!(plan.profile, expected_profile);
        assert_eq!(plan.mode, expected_mode);
        let actual_error = match plan.mode {
            InternalBinaryMode::WrapperPending { .. } => PublicError::ToolchainNotFound,
            InternalBinaryMode::WrapperRejected => PublicError::Configuration,
            InternalBinaryMode::ReviewerPending
            | InternalBinaryMode::OperatorPending
            | InternalBinaryMode::ServicePending => PublicError::Authorization,
            InternalBinaryMode::AgentCli
            | InternalBinaryMode::LegacyAgentCliRollback
            | InternalBinaryMode::LegacyGuardRollback => {
                panic!("version probe reached an unexpected mode")
            }
        };
        assert_eq!(actual_error, expected_error);
        assert_eq!(actual_error.exit_code(), expected_error.exit_code());
    }
}
