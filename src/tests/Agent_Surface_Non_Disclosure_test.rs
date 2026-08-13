use super::*;
use crate::binary_mode::{classify_invocation, InternalBinaryMode};

fn args(values: &[&str]) -> Vec<OsString> {
    values.iter().map(OsString::from).collect()
}

fn public_failure(arguments: &[&str]) -> PublicError {
    let arguments = args(arguments);
    match classify_invocation(&arguments, None).mode {
        InternalBinaryMode::AgentCli => execute(
            &arguments,
            &Err(PublicError::ServiceUnavailable),
            &UnavailableSubmissionTransport,
        )
        .expect_err("adversarial invocation must fail"),
        InternalBinaryMode::ReviewerPending
        | InternalBinaryMode::OperatorPending
        | InternalBinaryMode::ServicePending => PublicError::Authorization,
        InternalBinaryMode::WrapperPending { .. } => PublicError::ServiceUnavailable,
        InternalBinaryMode::LegacyAgentCliRollback | InternalBinaryMode::LegacyGuardRollback => {
            panic!("public invocation reached a rollback mode")
        }
    }
}

#[test]
fn help_has_an_exact_four_command_allowlist() {
    let command_lines = AGENT_HELP
        .lines()
        .map(str::trim)
        .filter(|line| line.starts_with("antdelirium "))
        .collect::<Vec<_>>();
    assert_eq!(
        command_lines,
        vec![
            "antdelirium --api hierarchy.task.current",
            "antdelirium --api hierarchy.rule.related",
            "antdelirium next-action",
            "antdelirium --api 'hierarchy.task.current.submit <evidência>'",
        ]
    );

    for forbidden in [
        "install",
        "uninstall",
        "doctor",
        "migrate-plugin",
        "--project-root",
        "--config",
        "--search",
        "config init",
        "config new_task",
        "config hooks",
        "swarm",
        "--getagents",
        "--getprojects",
        "--show-agent",
        "--whoami",
        "--root-checks",
        "--rebase-check",
        "--governance-history",
        "--messages",
        "--thread",
        "--inbox",
        "--broadcast",
        "--feedback",
        "--submits",
        "--submit-show",
        "--submit-note",
        "--submit-grade",
        "--submit-accept",
        "--submit-reject",
        "--manager-task",
        "hierarchy.context",
        "hierarchy.git",
        "hierarchy.override",
        ".implemented",
        ".task_reviewed",
        ".review_status",
        ".grade",
        ".add_task",
        ".needs_more_tasks",
        ".problems",
        "plugins/",
        "plan/",
        "Cargo.toml",
        "state.db",
        "/home/",
        "/var/",
        "/tmp/",
    ] {
        assert!(!AGENT_HELP.contains(forbidden), "help leaked {forbidden}");
    }
}

#[test]
fn protected_and_invented_operations_are_indistinguishable() {
    let attempts = [
        vec!["install"],
        vec!["doctor", "--verbose"],
        vec!["review", "assigned"],
        vec!["service"],
        vec!["--api", "hierarchy.override.active_plan"],
        vec!["--api", "hierarchy.task.current.implemented=true"],
        vec!["--submit-accept", "sub_00000000000000000000000000"],
        vec!["--project-root", "/home/operator/private-project"],
        vec!["--config", "/var/lib/private/default_hierarchy.toml"],
        vec!["--api", "hierarchy.command.that.never.existed"],
        vec!["command-that-never-existed"],
    ];
    let expected = PublicError::Authorization;
    for attempt in attempts {
        let error = public_failure(&attempt);
        assert_eq!(error, expected);
        assert_eq!(error.exit_code(), 77);
        assert_eq!(error.public_message(), expected.public_message());
    }
}

#[test]
fn errors_do_not_echo_adversarial_arguments_ids_or_paths() {
    let probes = [
        vec!["--api", "hierarchy.secret.operation token_super_secret"],
        vec!["--project-root", "/home/operator/private-project"],
        vec!["--config", "C:\\private\\default_hierarchy.toml"],
        vec!["--submit-show", "sub_sensitive_identifier"],
        vec!["--api", "hierarchy.override.active_plan=plan_sensitive"],
    ];
    for probe in probes {
        let error = public_failure(&probe);
        let response = error.public_message();
        for supplied in &probe {
            assert!(!response.contains(supplied));
        }
        for forbidden in [
            "token_super_secret",
            "private-project",
            "default_hierarchy.toml",
            "sub_sensitive_identifier",
            "plan_sensitive",
            "plugins/",
            "plan/",
            "state.db",
            "task_orchestrator",
            "login.json",
        ] {
            assert!(!response.contains(forbidden));
        }
    }
}

#[test]
fn malformed_public_requests_reveal_only_the_public_usage_contract() {
    for probe in [
        vec!["--api"],
        vec!["next-action", "--json"],
        vec!["--api", "hierarchy.task.current.submit"],
        vec![
            "--api",
            "hierarchy.task.current.submit /home/operator/private-proof",
        ],
    ] {
        let error = public_failure(&probe);
        assert_eq!(error, PublicError::Usage);
        assert_eq!(error.exit_code(), 64);
        assert_eq!(error.public_message(), PublicError::Usage.public_message());
    }
}
