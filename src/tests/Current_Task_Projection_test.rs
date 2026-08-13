#[cfg(test)]
mod tests {
    use crate::structs::{
        build_current_task_projection, sanitize_public_text, CurrentTaskProjectionDocument,
        ProjectedCurrentTask, ProjectedQueueStatus, ProjectedTaskAction, ProjectionMetadata,
    };

    fn metadata() -> ProjectionMetadata {
        ProjectionMetadata {
            project_id: "prj_00000000000000000000000000".to_string(),
            workspace_id: "wsp_00000000000000000000000000".to_string(),
            source_revision: 7,
            generation: 9,
            fingerprint: format!("{:064x}", 99),
            rendered_at: "2026-08-11T00:00:00Z".to_string(),
        }
    }

    fn document() -> CurrentTaskProjectionDocument {
        CurrentTaskProjectionDocument {
            metadata: metadata(),
            task: Some(ProjectedCurrentTask {
                task_id: "tsk_00000000000000000000000000".to_string(),
                task_revision: 3,
                objective: "Implement the current storage adapter.".to_string(),
                allowed_action: ProjectedTaskAction::Implement,
                acceptance_criteria: vec!["Run the focused test.".to_string()],
                relevant_paths: vec!["src/adapter.rs".to_string()],
            }),
            queue_status: ProjectedQueueStatus::TaskAvailable,
            submission_instruction: "Submit evidence with hierarchy.task.current.submit."
                .to_string(),
        }
    }

    #[test]
    fn renders_only_the_agent_projection_v1_current_task_fields() {
        let projection = build_current_task_projection(&document()).unwrap();
        let json = String::from_utf8(projection.serialized_contract).unwrap();

        for required in [
            "\"contract_version\": \"agent-projection-v1\"",
            "\"artifact_kind\": \"current_task\"",
            "\"project_ref\"",
            "\"workspace_ref\"",
            "\"source_revision\": 7",
            "\"task_id\"",
            "\"task_revision\": 3",
            "\"objective\"",
            "\"allowed_action\": \"implement\"",
            "\"acceptance_criteria\"",
            "\"relevant_paths\"",
            "\"queue_status\": \"task_available\"",
            "\"submission\"",
        ] {
            assert!(
                json.contains(required),
                "missing contract field: {required}"
            );
        }
        for forbidden in [
            "\"plan_id\"",
            "\"plan_title\"",
            "\"complete_plan\"",
            "\"implemented\"",
            "\"task_reviewed\"",
            "\"review_status\"",
            "\"grade\"",
            "\"active_plan\"",
            "\"copy_paste_command\"",
            "\"plugin_path\"",
        ] {
            assert!(
                !json.contains(forbidden),
                "forbidden field leaked: {forbidden}"
            );
        }
    }

    #[test]
    fn sanitizes_absolute_paths_in_legacy_free_text() {
        let sanitized = sanitize_public_text(
            "Work in /home/user/project/src, inspect ~/cache, C:/admin/plan.json or D:\\private\\plan.json.",
        );

        assert_eq!(
            sanitized,
            "Work in [project-root], inspect [project-root], [project-root] or [project-root]"
        );
        assert!(!sanitized.contains("/home"));
        assert!(!sanitized.contains("~/"));
        assert!(!sanitized.contains("C:/"));
        assert!(!sanitized.contains("D:\\"));
    }

    #[test]
    fn rejects_unsanitized_absolute_paths_and_traversal() {
        let mut absolute_objective = document();
        absolute_objective.task.as_mut().unwrap().objective =
            "Inspect /var/lib/antdelirium/state.db".to_string();
        assert!(build_current_task_projection(&absolute_objective).is_err());

        let mut windows_absolute_objective = document();
        windows_absolute_objective.task.as_mut().unwrap().objective =
            "Inspect C:\\AntDelirium\\private\\state.db".to_string();
        assert!(build_current_task_projection(&windows_absolute_objective).is_err());

        let mut traversal = document();
        traversal.task.as_mut().unwrap().relevant_paths = vec!["../plan/task.json".to_string()];
        assert!(build_current_task_projection(&traversal).is_err());
    }

    #[test]
    fn rejects_noncanonical_ids_and_task_presence_mismatches() {
        let mut invalid_id = document();
        invalid_id.task.as_mut().unwrap().task_id = "1.2.3".to_string();
        assert!(build_current_task_projection(&invalid_id).is_err());

        let mut missing_task = document();
        missing_task.task = None;
        assert!(build_current_task_projection(&missing_task).is_err());

        missing_task.queue_status = ProjectedQueueStatus::NoTaskAvailable;
        assert!(build_current_task_projection(&missing_task).is_ok());
    }
}
