use super::*;

const PROJECT_ID: &str = "prj_00000000000000000000000000";
const WORKSPACE_ID: &str = "wsp_00000000000000000000000000";

fn digest(byte: char) -> String {
    std::iter::repeat_n(byte, 64).collect()
}

fn source() -> TaskGenerationSource {
    TaskGenerationSource {
        contract_version: TASK_GENERATION_SOURCE_V1.to_string(),
        request_id: "req_00000000000000000000000000".to_string(),
        source_format: "ordered-objectives-v1".to_string(),
        source_sha256: digest('a'),
        project_id: PROJECT_ID.to_string(),
        workspace_id: WORKSPACE_ID.to_string(),
        expected_state_revision: 0,
        plans: vec![TaskGenerationSourcePlan {
            source_key_sha256: digest('b'),
            objective: "Deliver the verified API".to_string(),
            tasks: vec![TaskGenerationSourceTask {
                source_key_sha256: digest('c'),
                objective: "Implement the closed contract".to_string(),
                acceptance_criteria: vec!["The contract rejects unknown input".to_string()],
                relevant_paths: vec!["src/lib.rs".to_string()],
            }],
        }],
    }
}

fn plans() -> Vec<TaskPackagePlan> {
    vec![TaskPackagePlan {
        plan_id: "pln_00000000000000000000000000".to_string(),
        plan_ordinal: 0,
        source_key_sha256: digest('b'),
        objective: "Deliver the verified API".to_string(),
        tasks: vec![TaskPackageTask {
            task_id: "tsk_00000000000000000000000000".to_string(),
            task_ordinal: 0,
            task_revision: 1,
            source_key_sha256: digest('c'),
            objective: "Implement the closed contract".to_string(),
            acceptance_criteria: vec!["The contract rejects unknown input".to_string()],
            relevant_paths: vec!["src/lib.rs".to_string()],
        }],
    }]
}

#[test]
fn source_contract_is_strict_bounded_and_contains_no_canonical_task_ids() {
    let source = source();
    validate_generation_source(&source).unwrap();
    let serialized = serde_json::to_value(&source).unwrap();
    assert!(serialized.to_string().find("task_id").is_none());

    let mut unknown = serialized;
    unknown["external_plan_path"] = serde_json::json!("/private/plan");
    assert!(serde_json::from_value::<TaskGenerationSource>(unknown).is_err());

    let mut invalid = source;
    invalid.plans[0].tasks[0].relevant_paths = vec!["../plans/private.json".to_string()];
    assert_eq!(
        validate_generation_source(&invalid),
        Err(TaskPackageContractError::Content)
    );
}

#[test]
fn proposal_and_v2_require_explicit_contiguous_order_and_exact_current_scope() {
    let proposal = TaskPackageProposal {
        contract_version: TASK_PACKAGE_PROPOSAL_V1.to_string(),
        proposal_id: "prp_00000000000000000000000000".to_string(),
        request_id: "req_00000000000000000000000000".to_string(),
        project_id: PROJECT_ID.to_string(),
        workspace_id: WORKSPACE_ID.to_string(),
        expected_state_revision: 0,
        source_sha256: digest('a'),
        payload_sha256: digest('d'),
        source_plan_count: 1,
        source_task_count: 1,
        converted_plan_count: 1,
        converted_task_count: 1,
        unconverted_source_keys_sha256: Vec::new(),
        differences: Vec::new(),
        plans: plans(),
        warnings: Vec::new(),
        state: "prepared".to_string(),
    };
    validate_proposal(&proposal).unwrap();

    let package = GovernanceTaskPackageV2 {
        contract_version: GOVERNANCE_TASK_PACKAGE_V2.to_string(),
        request_id: "req_11111111111111111111111111".to_string(),
        project_id: PROJECT_ID.to_string(),
        workspace_id: WORKSPACE_ID.to_string(),
        expected_state_revision: 0,
        proposal_id: proposal.proposal_id,
        proposal_sha256: digest('d'),
        attestation_id: "att_00000000000000000000000000".to_string(),
        attestation_sha256: digest('e'),
        approval_id: "apr_00000000000000000000000000".to_string(),
        approval_sha256: digest('f'),
        active_plan_id: "pln_00000000000000000000000000".to_string(),
        current_task_id: "tsk_00000000000000000000000000".to_string(),
        plans: proposal.plans,
    };
    validate_governance_package_v2(&package).unwrap();

    let mut wrong_order = package.clone();
    wrong_order.plans[0].tasks[0].task_ordinal = 1;
    assert_eq!(
        validate_governance_package_v2(&wrong_order),
        Err(TaskPackageContractError::Order)
    );
    let mut wrong_current = package;
    wrong_current.current_task_id = "tsk_11111111111111111111111111".to_string();
    assert_eq!(
        validate_governance_package_v2(&wrong_current),
        Err(TaskPackageContractError::Scope)
    );
}

#[test]
fn attestation_approval_and_identity_association_bind_exact_digests_and_scopes() {
    let attestation = TaskPackageAttestation {
        contract_version: TASK_PACKAGE_ATTESTATION_V1.to_string(),
        attestation_id: "att_00000000000000000000000000".to_string(),
        proposal_id: "prp_00000000000000000000000000".to_string(),
        proposal_sha256: digest('a'),
        source_sha256: digest('b'),
        project_id: PROJECT_ID.to_string(),
        workspace_id: WORKSPACE_ID.to_string(),
        expected_state_revision: 0,
        verifier_subject_sha256: digest('c'),
        decision: "accepted".to_string(),
        attested_at: "unix:1788739200".to_string(),
        state: "attested".to_string(),
    };
    validate_attestation(&attestation).unwrap();
    let approval = TaskPackageApproval {
        contract_version: TASK_PACKAGE_APPROVAL_V1.to_string(),
        approval_id: "apr_00000000000000000000000000".to_string(),
        proposal_id: attestation.proposal_id.clone(),
        proposal_sha256: attestation.proposal_sha256.clone(),
        attestation_id: attestation.attestation_id.clone(),
        attestation_sha256: digest('d'),
        project_id: PROJECT_ID.to_string(),
        workspace_id: WORKSPACE_ID.to_string(),
        expected_state_revision: 0,
        operator_subject_sha256: digest('e'),
        decision: "approved".to_string(),
        approved_at: "unix:1788739201".to_string(),
        state: "approved".to_string(),
    };
    validate_approval(&approval).unwrap();

    let mut association = MigrationIdentityAssociation {
        contract_version: MIGRATION_IDENTITY_ASSOCIATION_V1.to_string(),
        request_id: "req_00000000000000000000000000".to_string(),
        migration_id: "mig_00000000000000000000000000".to_string(),
        cutover_id: "cut_00000000000000000000000000".to_string(),
        approval_id: approval.approval_id,
        provisional_project_id: PROJECT_ID.to_string(),
        provisional_workspace_id: WORKSPACE_ID.to_string(),
        candidate_project_id: "prj_11111111111111111111111111".to_string(),
        candidate_workspace_id: "wsp_11111111111111111111111111".to_string(),
        project_identity_sha256: digest('1'),
        workspace_identity_sha256: digest('2'),
        candidate_logical_state_sha256: digest('3'),
        cutover_binding_sha256: digest('4'),
        provisional_state_policy: "require-empty".to_string(),
        expected_registry_revision: 0,
    };
    validate_identity_association(&association).unwrap();
    association.candidate_project_id = association.provisional_project_id.clone();
    assert_eq!(
        validate_identity_association(&association),
        Err(TaskPackageContractError::Scope)
    );
}

#[test]
fn authority_matrix_never_assigns_new_operations_to_the_agent() {
    use TaskPackageAdministrativeAction as Action;
    let actions = [
        Action::Prepare,
        Action::Inspect,
        Action::Attest,
        Action::Approve,
        Action::Install,
        Action::IdentityAssociate,
    ];
    for action in actions {
        let policy = authority_policy(action);
        assert_ne!(policy.actor, CapabilityActor::Agent);
        assert!(policy.exact_project);
        assert!(policy.exact_workspace);
        assert!(policy.task_scope_forbidden);
    }
    assert_eq!(
        authority_policy(Action::Attest).operation,
        CapabilityOperation::AttestationCreate
    );
    assert_eq!(
        authority_policy(Action::Install).operation,
        CapabilityOperation::InstallationAdmin
    );
    assert!(authority_matches_exact_scope(
        Action::Install,
        CapabilityActor::Ci,
        CapabilityOperation::InstallationAdmin,
        Some(PROJECT_ID),
        Some(WORKSPACE_ID),
        None,
        PROJECT_ID,
        WORKSPACE_ID,
    ));
    assert!(!authority_matches_exact_scope(
        Action::Install,
        CapabilityActor::Ci,
        CapabilityOperation::InstallationAdmin,
        None,
        None,
        None,
        PROJECT_ID,
        WORKSPACE_ID,
    ));
    assert!(!authority_matches_exact_scope(
        Action::Prepare,
        CapabilityActor::Agent,
        CapabilityOperation::StateAdmin,
        Some(PROJECT_ID),
        Some(WORKSPACE_ID),
        None,
        PROJECT_ID,
        WORKSPACE_ID,
    ));
}

#[test]
fn receipts_bind_operation_state_and_advance_revisions_explicitly() {
    let receipt = TaskPackageOperationReceipt {
        contract_version: TASK_PACKAGE_OPERATION_RECEIPT_V1.to_string(),
        request_id: "req_00000000000000000000000000".to_string(),
        operation: "prepare".to_string(),
        project_id: PROJECT_ID.to_string(),
        workspace_id: WORKSPACE_ID.to_string(),
        artifact_id: "prp_00000000000000000000000000".to_string(),
        artifact_sha256: digest('a'),
        previous_state_revision: 0,
        resulting_state_revision: 0,
        state: "prepared".to_string(),
    };
    validate_operation_receipt(&receipt).unwrap();
    let mut mismatched = receipt;
    mismatched.state = "installed".to_string();
    assert_eq!(
        validate_operation_receipt(&mismatched),
        Err(TaskPackageContractError::Scope)
    );

    let association = MigrationIdentityAssociationReceipt {
        contract_version: MIGRATION_IDENTITY_ASSOCIATION_RECEIPT_V1.to_string(),
        request_id: "req_00000000000000000000000000".to_string(),
        association_id: "asn_00000000000000000000000000".to_string(),
        migration_id: "mig_00000000000000000000000000".to_string(),
        cutover_id: "cut_00000000000000000000000000".to_string(),
        project_id: PROJECT_ID.to_string(),
        workspace_id: WORKSPACE_ID.to_string(),
        previous_registry_revision: 7,
        resulting_registry_revision: 8,
        association_sha256: digest('b'),
        state: "associated".to_string(),
    };
    validate_identity_association_receipt(&association).unwrap();
}

#[test]
fn versioned_schemas_match_the_compiled_contract_constants() {
    let schemas = [
        (
            include_str!(concat!(
                env!("CARGO_MANIFEST_DIR"),
                "/docs/contracts/v1/task-generation-source-v1.schema.json"
            )),
            "urn:antdelirium:contract:task-generation-source-v1",
            TASK_GENERATION_SOURCE_V1,
        ),
        (
            include_str!(concat!(
                env!("CARGO_MANIFEST_DIR"),
                "/docs/contracts/v1/task-package-proposal-v1.schema.json"
            )),
            "urn:antdelirium:contract:task-package-proposal-v1",
            TASK_PACKAGE_PROPOSAL_V1,
        ),
        (
            include_str!(concat!(
                env!("CARGO_MANIFEST_DIR"),
                "/docs/contracts/v1/task-package-attestation-v1.schema.json"
            )),
            "urn:antdelirium:contract:task-package-attestation-v1",
            TASK_PACKAGE_ATTESTATION_V1,
        ),
        (
            include_str!(concat!(
                env!("CARGO_MANIFEST_DIR"),
                "/docs/contracts/v1/task-package-approval-v1.schema.json"
            )),
            "urn:antdelirium:contract:task-package-approval-v1",
            TASK_PACKAGE_APPROVAL_V1,
        ),
        (
            include_str!(concat!(
                env!("CARGO_MANIFEST_DIR"),
                "/docs/contracts/v1/governance-task-package-v2.schema.json"
            )),
            "urn:antdelirium:contract:governance-task-package-v2",
            GOVERNANCE_TASK_PACKAGE_V2,
        ),
        (
            include_str!(concat!(
                env!("CARGO_MANIFEST_DIR"),
                "/docs/contracts/v1/migration-identity-association-v1.schema.json"
            )),
            "urn:antdelirium:contract:migration-identity-association-v1",
            MIGRATION_IDENTITY_ASSOCIATION_V1,
        ),
        (
            include_str!(concat!(
                env!("CARGO_MANIFEST_DIR"),
                "/docs/contracts/v1/task-package-operation-receipt-v1.schema.json"
            )),
            "urn:antdelirium:contract:task-package-operation-receipt-v1",
            TASK_PACKAGE_OPERATION_RECEIPT_V1,
        ),
        (
            include_str!(concat!(
                env!("CARGO_MANIFEST_DIR"),
                "/docs/contracts/v1/migration-identity-association-receipt-v1.schema.json"
            )),
            "urn:antdelirium:contract:migration-identity-association-receipt-v1",
            MIGRATION_IDENTITY_ASSOCIATION_RECEIPT_V1,
        ),
    ];
    let mut identifiers = std::collections::HashSet::new();
    for (source, expected_id, expected_contract) in schemas {
        let schema: serde_json::Value = serde_json::from_str(source).unwrap();
        assert_eq!(
            schema["$schema"],
            "https://json-schema.org/draft/2020-12/schema"
        );
        assert_eq!(schema["$id"], expected_id);
        assert_eq!(schema["type"], "object");
        assert_eq!(schema["additionalProperties"], false);
        assert_eq!(
            schema["properties"]["contract_version"]["const"],
            expected_contract
        );
        assert!(identifiers.insert(expected_id));
    }
}
