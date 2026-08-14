#[cfg(test)]
mod tests {
    use crate::structs::{
        build_governance_errors_projection, GovernanceErrorsProjectionDocument,
        ProjectedGovernanceViolation, ProjectionMetadata,
    };

    fn metadata() -> ProjectionMetadata {
        ProjectionMetadata {
            project_id: "prj_00000000000000000000000000".to_string(),
            workspace_id: "wsp_00000000000000000000000000".to_string(),
            source_revision: 7,
            generation: 9,
            fingerprint: format!("{:064x}", 101),
            rendered_at: "2026-08-11T00:00:00Z".to_string(),
        }
    }

    fn document() -> GovernanceErrorsProjectionDocument {
        GovernanceErrorsProjectionDocument {
            metadata: metadata(),
            violations: vec![ProjectedGovernanceViolation {
                violation_id: "vio_00000000000000000000000000".to_string(),
                rule_id: "hierarchy.structural.max_nesting".to_string(),
                summary: "Maximum nesting exceeded.".to_string(),
                path: "src/adapter.rs".to_string(),
                line: 19,
                quick_fix: Some("Extract the nested branch into a helper.".to_string()),
                public_rule_reference: "hierarchy.rule.max_nesting".to_string(),
            }],
        }
    }

    #[test]
    fn renders_only_the_agent_projection_v1_governance_error_fields() {
        let projection = build_governance_errors_projection(&document()).unwrap();
        let json = String::from_utf8(projection.serialized_contract).unwrap();

        for required in [
            "\"contract_version\": \"agent-projection-v1\"",
            "\"artifact_kind\": \"governance_errors\"",
            "\"project_ref\"",
            "\"workspace_ref\"",
            "\"source_revision\": 7",
            "\"violation_id\"",
            "\"rule_id\"",
            "\"summary\"",
            "\"path\": \"src/adapter.rs\"",
            "\"line\": 19",
            "\"quick_fix\"",
            "\"public_rule_reference\"",
        ] {
            assert!(
                json.contains(required),
                "missing contract field: {required}"
            );
        }
        for forbidden in [
            "\"tool\"",
            "\"summary\": {",
            "\"logs\"",
            "\"audited_files\"",
            "\"violation_files\"",
            "\"log_file\"",
            "\"snippet\"",
            "\"detected_dna\"",
            "\"taxonomy_path\"",
            "\"priority\"",
            "\"api_rule_command\"",
            "\"installation_command\"",
        ] {
            assert!(
                !json.contains(forbidden),
                "forbidden field leaked: {forbidden}"
            );
        }
    }

    #[test]
    fn rejects_absolute_and_traversing_violation_paths() {
        let mut absolute = document();
        absolute.violations[0].path = "/home/operator/private.rs".to_string();
        assert!(build_governance_errors_projection(&absolute).is_err());

        let mut windows = document();
        windows.violations[0].path = "C:\\private\\source.rs".to_string();
        assert!(build_governance_errors_projection(&windows).is_err());

        let mut traversal = document();
        traversal.violations[0].path = "src/../../plan/private.json".to_string();
        assert!(build_governance_errors_projection(&traversal).is_err());
    }

    #[test]
    fn rejects_private_paths_in_public_text_and_invalid_lines() {
        let mut summary = document();
        summary.violations[0].summary = "Inspect /srv/antdelirium/private/state.db".to_string();
        assert!(build_governance_errors_projection(&summary).is_err());

        let mut quick_fix = document();
        quick_fix.violations[0].quick_fix = Some("Open D:\\plans\\task.json".to_string());
        assert!(build_governance_errors_projection(&quick_fix).is_err());

        let mut line = document();
        line.violations[0].line = 0;
        assert!(build_governance_errors_projection(&line).is_err());
    }

    #[test]
    fn rejects_administrative_names_and_artifacts_from_governance_errors() {
        let mut protected_path = document();
        protected_path.violations[0].path = "src/task_orchestrator.json".to_string();
        assert!(build_governance_errors_projection(&protected_path).is_err());

        for text in [
            "Inspect governance_history.json.",
            "Set active_plan to continue.",
            "Mark task_reviewed=true.",
        ] {
            let mut protected = document();
            protected.violations[0].quick_fix = Some(text.to_string());
            assert!(
                build_governance_errors_projection(&protected).is_err(),
                "administrative quick fix was accepted: {text}"
            );
        }
    }
}
