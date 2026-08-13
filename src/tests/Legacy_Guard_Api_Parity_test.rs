use super::*;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum SurvivingLegacyOperation {
    CurrentTaskRead,
    RelatedRuleRead,
    NextActionRead,
    SubmissionCreate,
    SubmissionReadAssigned,
    ReviewDecideAssigned,
    ExplicitAudit,
}

const SURVIVING_OPERATIONS: [SurvivingLegacyOperation; 7] = [
    SurvivingLegacyOperation::CurrentTaskRead,
    SurvivingLegacyOperation::RelatedRuleRead,
    SurvivingLegacyOperation::NextActionRead,
    SurvivingLegacyOperation::SubmissionCreate,
    SurvivingLegacyOperation::SubmissionReadAssigned,
    SurvivingLegacyOperation::ReviewDecideAssigned,
    SurvivingLegacyOperation::ExplicitAudit,
];

fn strings(values: &[&str]) -> Vec<String> {
    values.iter().map(|value| (*value).to_string()).collect()
}

fn classify_surviving_operation(args: &[String]) -> Option<SurvivingLegacyOperation> {
    let mut matches = Vec::new();

    if args.iter().any(|arg| arg == "--next-action") {
        matches.push(SurvivingLegacyOperation::NextActionRead);
    }
    if args.iter().any(|arg| arg == "--submit") {
        matches.push(SurvivingLegacyOperation::SubmissionCreate);
    }
    if args.iter().any(|arg| arg == "--submit-show") {
        matches.push(SurvivingLegacyOperation::SubmissionReadAssigned);
    }
    if args
        .iter()
        .any(|arg| matches!(arg.as_str(), "--submit-accept" | "--submit-reject"))
    {
        matches.push(SurvivingLegacyOperation::ReviewDecideAssigned);
    }
    if args.first().is_some_and(|arg| arg == "audit") {
        matches.push(SurvivingLegacyOperation::ExplicitAudit);
    }

    if let Some(api_index) = args.iter().position(|arg| arg == "--api") {
        let query = args
            .iter()
            .skip(api_index + 1)
            .take_while(|arg| !arg.starts_with("--"))
            .cloned()
            .collect::<Vec<_>>()
            .join(" ");

        if query.starts_with("hierarchy.task.current.submit ") {
            matches.push(SurvivingLegacyOperation::SubmissionCreate);
        } else if is_read_only_current_task_query(&query) {
            matches.push(SurvivingLegacyOperation::CurrentTaskRead);
        } else if query.starts_with("hierarchy.rule.") {
            matches.push(SurvivingLegacyOperation::RelatedRuleRead);
        }
    }

    if matches.len() == 1 {
        matches.into_iter().next()
    } else {
        None
    }
}

fn is_read_only_current_task_query(query: &str) -> bool {
    let reads_task = query == "hierarchy.task"
        || query == "hierarchy.task.current"
        || query.starts_with("hierarchy.task.[");
    let mutates_or_expands_scope = [
        ".implemented",
        ".task_reviewed",
        ".review_status",
        ".grade",
        ".add_task",
        ".needs_more_tasks",
        ".problems",
        ".context",
    ]
    .iter()
    .any(|marker| query.contains(marker));

    reads_task && !mutates_or_expands_scope
}

#[test]
fn old_guard_is_documented_by_source_as_an_unrestricted_forwarder() {
    let source = include_str!("../../get_toml_for_new/ant_delirium_build_guard.rs");

    assert!(source.contains(".arg(\"--project-root\")"));
    assert!(source.contains(".arg(\"--config\")"));
    assert!(source.contains(".args(args)"));
    assert!(!source.contains("SurvivingLegacyOperation"));
}

#[test]
fn parity_allowlist_contains_only_the_seven_frozen_semantic_operations() {
    assert_eq!(SURVIVING_OPERATIONS.len(), 7);
    assert_eq!(
        SURVIVING_OPERATIONS,
        [
            SurvivingLegacyOperation::CurrentTaskRead,
            SurvivingLegacyOperation::RelatedRuleRead,
            SurvivingLegacyOperation::NextActionRead,
            SurvivingLegacyOperation::SubmissionCreate,
            SurvivingLegacyOperation::SubmissionReadAssigned,
            SurvivingLegacyOperation::ReviewDecideAssigned,
            SurvivingLegacyOperation::ExplicitAudit,
        ]
    );
}

#[test]
fn surviving_read_operations_keep_their_legacy_intent() {
    let task_args = strings(&["--api", "hierarchy.task.[7.0]"]);
    let task_cli = parse_cli_args(&task_args);
    assert_eq!(
        classify_surviving_operation(&task_args),
        Some(SurvivingLegacyOperation::CurrentTaskRead)
    );
    assert!(task_cli.api_requested);
    assert_eq!(task_cli.api_query.as_deref(), Some("hierarchy.task.[7.0]"));

    let rule_args = strings(&["--api", "hierarchy.rule.max_nesting"]);
    let rule_cli = parse_cli_args(&rule_args);
    assert_eq!(
        classify_surviving_operation(&rule_args),
        Some(SurvivingLegacyOperation::RelatedRuleRead)
    );
    assert_eq!(
        rule_cli.api_query.as_deref(),
        Some("hierarchy.rule.max_nesting")
    );

    let next_args = strings(&["--next-action", "--json"]);
    let next_cli = parse_cli_args(&next_args);
    assert_eq!(
        classify_surviving_operation(&next_args),
        Some(SurvivingLegacyOperation::NextActionRead)
    );
    assert!(next_cli.next_action);
    assert!(next_cli.json_requested);
}

#[test]
fn surviving_submission_operations_keep_their_legacy_intent() {
    let create_args = strings(&[
        "--submit",
        "evidence digest",
        "--topic",
        "task.current",
        "--base-revision",
        "abc123",
    ]);
    let create_cli = parse_cli_args(&create_args);
    assert_eq!(
        classify_surviving_operation(&create_args),
        Some(SurvivingLegacyOperation::SubmissionCreate)
    );
    assert_eq!(create_cli.submit_body.as_deref(), Some("evidence digest"));
    assert_eq!(create_cli.message_topic.as_deref(), Some("task.current"));
    assert_eq!(create_cli.base_revision.as_deref(), Some("abc123"));

    let read_args = strings(&["--submit-show", "SUBMIT001"]);
    let read_cli = parse_cli_args(&read_args);
    assert_eq!(
        classify_surviving_operation(&read_args),
        Some(SurvivingLegacyOperation::SubmissionReadAssigned)
    );
    assert_eq!(read_cli.submit_show.as_deref(), Some("SUBMIT001"));
}

#[test]
fn surviving_review_decision_keeps_accept_and_reject_intent_without_freezing_grade() {
    for decision in ["--submit-accept", "--submit-reject"] {
        let decision_args = strings(&[decision, "SUBMIT001", "--submit-note", "checked"]);
        let decision_cli = parse_cli_args(&decision_args);
        assert_eq!(
            classify_surviving_operation(&decision_args),
            Some(SurvivingLegacyOperation::ReviewDecideAssigned)
        );
        assert_eq!(decision_cli.submit_note.as_deref(), Some("checked"));
        if decision == "--submit-accept" {
            assert_eq!(decision_cli.submit_accept.as_deref(), Some("SUBMIT001"));
        } else {
            assert_eq!(decision_cli.submit_reject.as_deref(), Some("SUBMIT001"));
        }
        assert_eq!(decision_cli.submit_grade, None);
    }
}

#[test]
fn explicit_audit_intent_survives_without_freezing_legacy_configuration_selectors() {
    let audit_args = strings(&["audit"]);
    let audit_cli = parse_cli_args(&audit_args);

    assert_eq!(
        classify_surviving_operation(&audit_args),
        Some(SurvivingLegacyOperation::ExplicitAudit)
    );
    assert_eq!(audit_cli.project_root, None);
    assert_eq!(audit_cli.config_path, None);
}

#[test]
fn dangerous_or_obsolete_pass_through_operations_have_no_parity_guarantee() {
    for rejected in [
        strings(&["--api", "hierarchy.override.mode=implementation"]),
        strings(&[
            "--api",
            "hierarchy.task.[7.0].[7.0.1].implemented=true checked",
        ]),
        strings(&[
            "--api",
            "hierarchy.task.[7.0].[7.0.1].review_status=approved",
        ]),
        strings(&["--api", "hierarchy.task.[7.0].add_task=synthetic"]),
        strings(&["--submit-grade", "10"]),
        strings(&["--submits"]),
        strings(&["config", "dump"]),
        strings(&["--config", "/outside/private"]),
        strings(&["--project-root", "/other/project"]),
        strings(&["--manager-tasks"]),
        strings(&["--getagents"]),
        strings(&["--messages"]),
    ] {
        assert_eq!(classify_surviving_operation(&rejected), None, "{rejected:?}");
    }
}

#[test]
fn mixed_surviving_operations_are_ambiguous_instead_of_being_forwarded() {
    let mixed = strings(&["--next-action", "--submit", "evidence"]);
    assert_eq!(classify_surviving_operation(&mixed), None);
}
