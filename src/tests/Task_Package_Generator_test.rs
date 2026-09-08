use super::*;
use crate::task_package_contracts::{
    TaskGenerationSourcePlan, TaskGenerationSourceTask, TASK_GENERATION_SOURCE_V1,
};

fn digest(byte: char) -> String {
    std::iter::repeat_n(byte, 64).collect()
}

fn source() -> TaskGenerationSource {
    TaskGenerationSource {
        contract_version: TASK_GENERATION_SOURCE_V1.to_string(),
        request_id: "req_00000000000000000000000000".to_string(),
        source_format: "ordered-objectives-v1".to_string(),
        source_sha256: digest('a'),
        project_id: "prj_00000000000000000000000000".to_string(),
        workspace_id: "wsp_00000000000000000000000000".to_string(),
        expected_state_revision: 0,
        plans: vec![TaskGenerationSourcePlan {
            source_key_sha256: digest('b'),
            objective: "Implement the deterministic package".to_string(),
            tasks: vec![
                TaskGenerationSourceTask {
                    source_key_sha256: digest('c'),
                    objective: "Preserve the first objective".to_string(),
                    acceptance_criteria: vec!["First result is reproducible".to_string()],
                    relevant_paths: vec!["src/first.rs".to_string()],
                },
                TaskGenerationSourceTask {
                    source_key_sha256: digest('d'),
                    objective: "Preserve the second objective".to_string(),
                    acceptance_criteria: vec!["Second result is reproducible".to_string()],
                    relevant_paths: vec!["src/second.rs".to_string()],
                },
            ],
        }],
    }
}

#[test]
fn expanded_proposal_must_fit_the_administrative_response_before_persistence() {
    let mut input = source();
    let prototype = input.plans[0].tasks[0].clone();
    input.plans[0].tasks = (0..400)
        .map(|index| {
            let mut task = prototype.clone();
            task.source_key_sha256 = format!("{index:064x}");
            task.objective = "x".repeat(10_000);
            task
        })
        .collect();
    // The input fits the frame, but generated IDs and metadata expand it.
    assert!(serde_json::to_vec(&input).unwrap().len() < 4 * 1024 * 1024 - 64 * 1024);
    assert_eq!(
        generate_task_package_proposal(&input),
        Err(TaskPackageContractError::Content)
    );
}

#[test]
fn identical_source_produces_byte_identical_proposals_and_opaque_ids() {
    let first = generate_task_package_proposal(&source()).unwrap();
    let second = generate_task_package_proposal(&source()).unwrap();
    assert_eq!(first, second);
    assert_eq!(
        serde_json::to_vec(&first).unwrap(),
        serde_json::to_vec(&second).unwrap()
    );
    assert_eq!(
        proposal_payload_sha256(&first).unwrap(),
        first.payload_sha256
    );
    assert!(first.proposal_id.starts_with("prp_"));
    assert!(first.plans[0].plan_id.starts_with("pln_"));
    assert!(first.plans[0].tasks[0].task_id.starts_with("tsk_"));
    assert_eq!(first.source_plan_count, 1);
    assert_eq!(first.source_task_count, 2);
    assert_eq!(first.converted_task_count, 2);
    assert!(first.unconverted_source_keys_sha256.is_empty());
    assert!(first.differences.is_empty());
}

#[test]
fn order_is_semantic_and_changes_the_proposal_without_losing_text() {
    let original_source = source();
    let original = generate_task_package_proposal(&original_source).unwrap();
    let mut reordered_source = original_source.clone();
    reordered_source.plans[0].tasks.swap(0, 1);
    let reordered = generate_task_package_proposal(&reordered_source).unwrap();

    assert_ne!(original.payload_sha256, reordered.payload_sha256);
    assert_ne!(
        original.plans[0].tasks[0].task_id,
        reordered.plans[0].tasks[1].task_id
    );
    assert_eq!(
        original.plans[0].tasks[0].objective,
        original_source.plans[0].tasks[0].objective
    );
    assert_eq!(original.plans[0].tasks[0].task_ordinal, 0);
    assert_eq!(reordered.plans[0].tasks[0].task_ordinal, 0);
}

#[test]
fn scope_and_revision_are_bound_to_the_proposal_identity_and_digest() {
    let baseline = generate_task_package_proposal(&source()).unwrap();

    let mut different_workspace = source();
    different_workspace.workspace_id = "wsp_00000000000000000000000001".to_string();
    let workspace_proposal = generate_task_package_proposal(&different_workspace).unwrap();
    assert_ne!(baseline.proposal_id, workspace_proposal.proposal_id);
    assert_ne!(baseline.payload_sha256, workspace_proposal.payload_sha256);

    let mut different_revision = source();
    different_revision.expected_state_revision = 1;
    let revision_proposal = generate_task_package_proposal(&different_revision).unwrap();
    assert_ne!(baseline.proposal_id, revision_proposal.proposal_id);
    assert_ne!(baseline.payload_sha256, revision_proposal.payload_sha256);
}

#[test]
fn duplicate_source_keys_are_rejected_before_any_proposal_is_created() {
    let mut duplicate_task = source();
    duplicate_task.plans[0].tasks[1].source_key_sha256 =
        duplicate_task.plans[0].tasks[0].source_key_sha256.clone();
    assert_eq!(
        generate_task_package_proposal(&duplicate_task),
        Err(TaskPackageContractError::Order)
    );

    let mut duplicate_plan = source();
    let mut second_plan = duplicate_plan.plans[0].clone();
    second_plan.tasks[0].source_key_sha256 = digest('e');
    second_plan.tasks[1].source_key_sha256 = digest('f');
    duplicate_plan.plans.push(second_plan);
    assert_eq!(
        generate_task_package_proposal(&duplicate_plan),
        Err(TaskPackageContractError::Order)
    );
}

#[test]
fn proposal_report_cannot_exceed_limits_or_repeat_unconverted_keys() {
    let mut excessive_count = generate_task_package_proposal(&source()).unwrap();
    excessive_count.source_task_count = 1_025;
    assert_eq!(
        validate_proposal(&excessive_count),
        Err(TaskPackageContractError::Content)
    );

    let mut repeated_unconverted = generate_task_package_proposal(&source()).unwrap();
    repeated_unconverted.unconverted_source_keys_sha256 = vec![digest('e'), digest('e')];
    assert_eq!(
        validate_proposal(&repeated_unconverted),
        Err(TaskPackageContractError::Content)
    );
}

#[test]
fn generator_rejects_legacy_lifecycle_fields_and_has_no_completion_output() {
    let mut value = serde_json::to_value(source()).unwrap();
    value["plans"][0]["tasks"][0]["implemented"] = serde_json::json!(true);
    assert!(serde_json::from_value::<TaskGenerationSource>(value).is_err());

    let proposal = generate_task_package_proposal(&source()).unwrap();
    let serialized = serde_json::to_string(&proposal).unwrap();
    for forbidden in [
        "implemented",
        "completed",
        "review_status",
        "active_plan_id",
        "current_task_id",
    ] {
        assert!(!serialized.contains(forbidden));
    }
}

#[test]
fn generator_is_pure_source_without_filesystem_database_or_environment_access() {
    let implementation = include_str!(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/src/Task-Package-Generator_unit.rs"
    ));
    for forbidden in [
        "std::fs",
        "std::env",
        "PathBuf",
        "ProjectDatabase",
        "AC_Connection",
        "Command::new",
    ] {
        assert!(!implementation.contains(forbidden));
    }
}
