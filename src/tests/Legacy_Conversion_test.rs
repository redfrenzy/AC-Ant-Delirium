use super::*;

use std::path::PathBuf;

use crate::legacy_normalization::LegacyNormalizationReceipt;

fn normalization_for_one_task() -> ValidatedLegacyNormalization {
    ValidatedLegacyNormalization {
        receipt: LegacyNormalizationReceipt {
            contract_version: "legacy-normalization-receipt-v1".to_string(),
            migration_id: "mig_00000000000000000000000000".to_string(),
            project_id: "prj_00000000000000000000000000".to_string(),
            source_snapshot_sha256: "a".repeat(64),
            staging_snapshot_sha256: "b".repeat(64),
            normalized_snapshot_sha256: "c".repeat(64),
            semantic_snapshot_sha256: "d".repeat(64),
            entries_compared: 1,
            regular_files_compared: 1,
            json_documents_compared: 1,
            physical_references: 0,
            physical_occurrences: 0,
            comparison: "equivalent".to_string(),
            state: "normalized".to_string(),
        },
        semantic_counts: SemanticCounts {
            plans: 1,
            tasks: 1,
            submissions: 0,
            reviews: 0,
            history_events: 0,
            problems: 0,
            transitions: 0,
            json_schemas: 0,
            generated_artifacts: 0,
            task_evidence: 0,
        },
        files: Vec::new(),
    }
}

#[test]
fn conversion_request_binds_the_dry_run_and_all_snapshot_digests() {
    let request = conversion_request(
        "mig_00000000000000000000000000".to_string(),
        "a".repeat(64),
        "b".repeat(64),
        "c".repeat(64),
        "d".repeat(64),
        "e".repeat(64),
    );
    assert_eq!(request.contract_version, "legacy-conversion-request-v1");
    assert_eq!(request.expected_dry_run_report_sha256, "e".repeat(64));
}

#[test]
fn plan_translation_uses_opaque_ids_and_never_infers_completion() {
    let plan = br#"{
      "plan_id":"1.0",
      "title":"private plan",
      "complete_plan":false,
      "tasks":[{
        "step_id":"1.1",
        "task_prompt":"Administrative preamble. Concrete goal: implement the public parser contract. Private tail.",
        "implemented":false,
        "task_reviewed":false,
        "review_status":"none"
      }]
    }"#;
    let files = BTreeMap::from([("plan/one.json", plan.as_slice())]);
    let (plans, bindings, source_objectives, fallback_objectives) =
        parse_plans(&normalization_for_one_task(), &files).unwrap();
    assert_eq!(plans.len(), 1);
    assert_eq!(plans[0].tasks.len(), 1);
    assert!(plans[0].canonical_id.starts_with("pln_"));
    assert!(plans[0].tasks[0].canonical_id.starts_with("tsk_"));
    assert_eq!(bindings.0.len(), 1);
    assert_eq!(bindings.1.len(), 1);
    assert_eq!(bindings.0[0].source_key_sha256.len(), 64);
    assert!(!serde_json::to_string(&bindings).unwrap().contains("1.0"));
    assert_eq!(source_objectives, 1);
    assert_eq!(fallback_objectives, 0);
    assert!(plans[0].tasks[0]
        .payload
        .starts_with(b"canonical-task-projection-v1\0"));
}

#[test]
fn plan_translation_rejects_legacy_completion_as_an_unapproved_lifecycle_decision() {
    let plan = br#"{
      "plan_id":"1.0",
      "complete_plan":false,
      "tasks":[{
        "step_id":"1.1",
        "task_prompt":"Concrete goal: implement a parser.",
        "implemented":true,
        "task_reviewed":false,
        "review_status":"none"
      }]
    }"#;
    let files = BTreeMap::from([("plan/one.json", plan.as_slice())]);
    assert!(matches!(
        parse_plans(&normalization_for_one_task(), &files),
        Err(LegacyConversionError::Source)
    ));
}

#[test]
#[ignore = "requires the operator-provided T8.5 dry-run staging locator"]
fn converts_the_real_t85_autobrowser_generation_without_cutover() {
    let staging_parent = PathBuf::from(
        std::env::var_os("ANTDELIRIUM_T86_STAGING_PARENT").expect("T8.6 staging parent"),
    );
    let request = conversion_request(
        std::env::var("ANTDELIRIUM_T86_MIGRATION_ID").unwrap(),
        std::env::var("ANTDELIRIUM_T86_SOURCE_SHA256").unwrap(),
        std::env::var("ANTDELIRIUM_T86_STAGING_SHA256").unwrap(),
        std::env::var("ANTDELIRIUM_T86_NORMALIZED_SHA256").unwrap(),
        std::env::var("ANTDELIRIUM_T86_SEMANTIC_SHA256").unwrap(),
        std::env::var("ANTDELIRIUM_T86_DRY_RUN_SHA256").unwrap(),
    );
    let report = convert_legacy_migration(&staging_parent, &request).unwrap();
    assert_eq!(report.source_counts, report.imported_counts);
    assert!(!report.approval_eligible);
    assert!(!report.approval_created);
    assert!(!report.cutover_performed);
    assert_eq!(report.manifest_state_after, "staged");
    println!("{}", serde_json::to_string(&report).unwrap());
}
