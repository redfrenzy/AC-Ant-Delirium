use std::collections::{hash_map::DefaultHasher, BTreeMap, BTreeSet, HashSet};
use std::env;
use std::fs;
use std::fs::OpenOptions;
use std::hash::{Hash, Hasher};
use std::io;
use std::path::{Path, PathBuf};
use std::thread;
use std::time::Duration;
use std::time::{SystemTime, UNIX_EPOCH};

use ac_ant_delirium::structs::{
    AuditResult, ConnectedAgent, GovernanceHistoryEntry, GovernanceTask, HierarchyConfig,
    JsonEntry, JsonValue, ManagerTask, NewAuditEvent, PluginContext, ProjectProjectionWriter,
    ProjectionWriteReceipt, RuntimeCommandContext, SubmitRecord, TodoProjectionContext,
};

#[path = "struct/Agent-Runtime_struct.rs"]
mod agent_runtime_struct;

use self::agent_runtime_struct::{
    AgentSubmitState, AgentSyncState, ProjectSummary, RootOrchestrationFinding, RuntimeLockGuard,
    SyncMetadata, TodoManifestSelection, WorkspaceLoginSnapshot,
};

#[path = "Agent-Runtime_unitcore.rs"]
mod unitcore;

pub struct TodoManifestProjectionTarget<'a> {
    pub writer: &'a dyn ProjectProjectionWriter,
    pub context: &'a TodoProjectionContext,
    pub audit_event: &'a NewAuditEvent,
}

pub fn discover_runtime_context(
    project_root: &Path,
    explicit_config_path: Option<&str>,
    argv0: Option<&str>,
) -> RuntimeCommandContext {
    unitcore::discover_runtime_context(project_root, explicit_config_path, argv0)
}

pub fn sync_agent_registration(plugin: &PluginContext) -> io::Result<Option<ConnectedAgent>> {
    unitcore::sync_agent_registration(plugin)
}

pub fn render_agents_report(plugin: &PluginContext, json_requested: bool) -> io::Result<String> {
    unitcore::render_agents_report(plugin, json_requested)
}

pub fn render_projects_report(plugin: &PluginContext, json_requested: bool) -> io::Result<String> {
    unitcore::render_projects_report(plugin, json_requested)
}

pub fn read_task_orchestrator(
    path: &std::path::Path,
) -> io::Result<Option<ac_ant_delirium::structs::TaskOrchestratorPayload>> {
    unitcore::read_task_orchestrator(path)
}

pub fn write_task_orchestrator(
    path: &std::path::Path,
    payload: &ac_ant_delirium::structs::TaskOrchestratorPayload,
) -> io::Result<()> {
    unitcore::write_task_orchestrator(path, payload)
}

pub fn read_numbered_plan(
    path: &std::path::Path,
) -> io::Result<ac_ant_delirium::structs::NumberedPlan> {
    unitcore::read_numbered_plan(path)
}

pub fn render_next_action(plugin: &PluginContext, json_requested: bool) -> io::Result<String> {
    unitcore::render_next_action(plugin, json_requested)
}

pub fn render_manager_tasks(plugin: &PluginContext, json_requested: bool) -> io::Result<String> {
    unitcore::render_manager_tasks(plugin, json_requested)
}

pub fn render_submit_queue(
    plugin: &PluginContext,
    json_requested: bool,
    filter: Option<&str>,
    submit_id: Option<&str>,
) -> io::Result<String> {
    unitcore::render_submit_queue(plugin, json_requested, filter, submit_id)
}

pub fn render_governance_history(
    plugin: &PluginContext,
    json_requested: bool,
) -> io::Result<String> {
    unitcore::render_governance_history(plugin, json_requested)
}

pub fn update_submit_status(
    plugin: &PluginContext,
    submit_id: &str,
    status: &str,
    note: Option<&str>,
    grade: Option<f64>,
) -> io::Result<String> {
    unitcore::update_submit_status(plugin, submit_id, status, note, grade)
}

pub fn append_manager_task(
    plugin: &PluginContext,
    title: &str,
    body: Option<&str>,
    file_path: Option<&str>,
    priority: i64,
) -> io::Result<String> {
    unitcore::append_manager_task(plugin, title, body, file_path, priority)
}

pub fn close_manager_task(plugin: &PluginContext, task_id: &str) -> io::Result<String> {
    unitcore::close_manager_task(plugin, task_id)
}

pub fn render_rebase_check(plugin: &PluginContext, json_requested: bool) -> io::Result<String> {
    unitcore::render_rebase_check(plugin, json_requested)
}

pub fn render_root_orchestration_violations(
    plugin: &PluginContext,
    json_requested: bool,
) -> io::Result<Option<String>> {
    unitcore::render_root_orchestration_violations(plugin, json_requested)
}

pub fn release_stale_agent_claims(
    plugin: &PluginContext,
    json_requested: bool,
) -> io::Result<String> {
    unitcore::release_stale_agent_claims(plugin, json_requested)
}

pub fn prune_stale_agents(plugin: &PluginContext, json_requested: bool) -> io::Result<String> {
    unitcore::prune_stale_agents(plugin, json_requested)
}

pub fn render_current_agent_identity(
    plugin: &PluginContext,
    json_requested: bool,
) -> io::Result<String> {
    unitcore::render_current_agent_identity(plugin, json_requested)
}

pub fn render_agent_identity(
    plugin: &PluginContext,
    agent_id: i64,
    json_requested: bool,
) -> io::Result<String> {
    unitcore::render_agent_identity(plugin, agent_id, json_requested)
}

pub fn render_agent_identity_by_agent_id(
    plugin: &PluginContext,
    agent_id: &str,
    json_requested: bool,
) -> io::Result<String> {
    unitcore::render_agent_identity_by_agent_id(plugin, agent_id, json_requested)
}

pub fn refresh_governance_from_audit(
    plugin: &PluginContext,
    audit_result: &AuditResult,
) -> io::Result<Vec<GovernanceTask>> {
    unitcore::refresh_governance_from_audit(plugin, audit_result)
}

pub fn assign_task_to_current_agent_if_missing(
    plugin: &PluginContext,
) -> io::Result<Option<String>> {
    unitcore::assign_task_to_current_agent_if_missing(plugin)
}

pub fn isolate_audit_to_current_agent(
    plugin: &PluginContext,
    audit_result: &AuditResult,
) -> io::Result<AuditResult> {
    unitcore::isolate_audit_to_current_agent(plugin, audit_result)
}

pub fn append_message(
    plugin: &PluginContext,
    target_internal_id: Option<i64>,
    target_agent_id: Option<&str>,
    sender_internal_id: Option<i64>,
    sender_agent_id: Option<&str>,
    topic: Option<&str>,
    operation_kind: Option<&str>,
    branch: Option<&str>,
    base_revision: Option<&str>,
    workspace_revision: Option<&str>,
    repository_revision: Option<&str>,
    body: &str,
) -> io::Result<String> {
    unitcore::append_message(
        plugin,
        target_internal_id,
        target_agent_id,
        sender_internal_id,
        sender_agent_id,
        topic,
        operation_kind,
        branch,
        base_revision,
        workspace_revision,
        repository_revision,
        body,
    )
}

pub fn append_feedback(
    plugin: &PluginContext,
    body: &str,
    context: Option<&str>,
    operation_kind: Option<&str>,
    branch: Option<&str>,
    base_revision: Option<&str>,
    workspace_revision: Option<&str>,
    repository_revision: Option<&str>,
    binary_name: &str,
) -> io::Result<String> {
    unitcore::append_feedback(
        plugin,
        body,
        context,
        operation_kind,
        branch,
        base_revision,
        workspace_revision,
        repository_revision,
        binary_name,
    )
}

pub fn append_submit(
    plugin: &PluginContext,
    body: &str,
    topic: Option<&str>,
    context: Option<&str>,
    branch: Option<&str>,
    base_revision: Option<&str>,
    workspace_revision: Option<&str>,
    repository_revision: Option<&str>,
    binary_name: &str,
) -> io::Result<String> {
    unitcore::append_submit(
        plugin,
        body,
        topic,
        context,
        branch,
        base_revision,
        workspace_revision,
        repository_revision,
        binary_name,
    )
}

pub fn render_current_inbox(plugin: &PluginContext) -> io::Result<String> {
    unitcore::render_current_inbox(plugin)
}

pub fn render_messages_feed(
    plugin: &PluginContext,
    topic_filter: Option<&str>,
    json_requested: bool,
) -> io::Result<String> {
    unitcore::render_messages_feed(plugin, topic_filter, json_requested)
}

pub fn render_plugin_help(plugin: Option<&PluginContext>, binary_name: &str) -> String {
    unitcore::render_plugin_help(plugin, binary_name)
}

pub fn render_plugin_feedback_help(plugin: Option<&PluginContext>, binary_name: &str) -> String {
    unitcore::render_plugin_feedback_help(plugin, binary_name)
}

pub fn render_plugin_installation(plugin: Option<&PluginContext>, binary_name: &str) -> String {
    unitcore::render_plugin_installation(plugin, binary_name)
}

pub fn materialize_project_local_plugin_package(
    project_root: &Path,
    explicit_project_root: Option<&str>,
    explicit_config_path: Option<&str>,
    runtime_plugin: Option<&PluginContext>,
    binary_name: &str,
) -> io::Result<String> {
    unitcore::materialize_project_local_plugin_package(
        project_root,
        explicit_project_root,
        explicit_config_path,
        runtime_plugin,
        binary_name,
    )
}

pub fn materialize_managed_git_hooks(
    project_root: &Path,
    explicit_project_root: Option<&str>,
    runtime_plugin: Option<&PluginContext>,
    binary_name: &str,
) -> io::Result<String> {
    unitcore::materialize_managed_git_hooks(
        project_root,
        explicit_project_root,
        runtime_plugin,
        binary_name,
    )
}

pub fn validate_managed_orchestration_request(
    project_root: &Path,
    explicit_project_root: Option<&str>,
    runtime_plugin: Option<&PluginContext>,
    request_kind: &str,
    json_requested: bool,
) -> io::Result<String> {
    unitcore::validate_managed_orchestration_request(
        project_root,
        explicit_project_root,
        runtime_plugin,
        request_kind,
        json_requested,
    )
}

pub fn build_api_overrides(
    plugin: &PluginContext,
    hierarchy: &HierarchyConfig,
    binary_name: &str,
) -> io::Result<Vec<(String, String)>> {
    unitcore::build_api_overrides(plugin, hierarchy, binary_name)
}

pub fn handle_task_api(plugin: &PluginContext, query: &str) -> io::Result<String> {
    unitcore::handle_task_api(plugin, query)
}

pub fn write_todo_manifest(
    plugin: &PluginContext,
    output_root: &Path,
    projection_target: Option<TodoManifestProjectionTarget<'_>>,
) -> io::Result<Option<ProjectionWriteReceipt>> {
    unitcore::write_todo_manifest(plugin, output_root, projection_target)
}

pub fn projected_implementation_pending_count(
    plugin: &PluginContext,
    orchestrator: &ac_ant_delirium::structs::TaskOrchestratorPayload,
    plan_id: &str,
    real_pending_count: usize,
) -> io::Result<usize> {
    unitcore::projected_implementation_pending_count(
        plugin,
        orchestrator,
        plan_id,
        real_pending_count,
    )
}

pub fn materialize_new_task(plugin: Option<&PluginContext>, title: &str) -> io::Result<String> {
    unitcore::materialize_new_task(plugin, title)
}

#[cfg(test)]
mod test_bridge {
    pub(super) use super::unitcore::{
        agent_id_for_workspace_path, assign_governance_task, claim_manager_task_for_agent,
        copy_binary_if_needed, directory_contains_json, expected_login_path_from_workspace_path,
        final_plan_step_id, infer_bootstrap_context, is_manager_task_active,
        is_valid_registered_workspace_path, load_hierarchy_from_path,
        mark_current_transition_task_implemented, match_plugin_context, normalize_path,
        normalize_path_str, parse_step_id_segments, plugin_dir_from_config_target,
        project_transition_step_id, projected_transition_task, read_connected_agents,
        read_governance_history, read_governance_tasks, read_manager_tasks, read_submit_records,
        read_transition_task_definitions, render_new_project_build_guard,
        render_new_project_hierarchy, render_new_project_task_orchestrator,
        transition_task_template_count, unix_timestamp, write_connected_agents,
        write_default_template_if_missing, write_entry_collection, write_governance_tasks,
        write_manager_tasks, DEFAULT_CONNECTED_JSON, DEFAULT_CONTEXT_JSON, DEFAULT_FEEDBACK_JSON,
        DEFAULT_GITHOOKS_README, DEFAULT_GOVERNANCE_HISTORY_JSON, DEFAULT_GOVERNANCE_JSON,
        DEFAULT_LOCK_GOVERNANCE_TEMPLATE_SH, DEFAULT_MANAGER_TASKS_JSON,
        DEFAULT_MERGE_REQUEST_TEMPLATE, DEFAULT_MESSAGES_JSON,
        DEFAULT_NEW_PROJECT_BUILD_GUARD_TEMPLATE, DEFAULT_NEW_PROJECT_HIERARCHY,
        DEFAULT_NEW_PROJECT_RULES, DEFAULT_PROJECT_STATE_GITIGNORE, DEFAULT_PUSH_REQUEST_TEMPLATE,
        DEFAULT_SAMPLE_PLAN_JSON, DEFAULT_SCRIPTS_README, DEFAULT_SUBMIT_QUEUE_JSON,
        DEFAULT_TASK_INDEX_TXT, DEFAULT_TASK_ORCHESTRATOR_JSON, DEFAULT_TRANSITION_TASK_JSON,
        DEFAULT_UNLOCK_GOVERNANCE_TEMPLATE_SH, DEFAULT_UPDATE_PLUGIN_BINARY_TEMPLATE_SH,
    };
}

#[cfg(test)]
#[path = "tests/Agent-Runtime_test.rs"]
mod tests;
