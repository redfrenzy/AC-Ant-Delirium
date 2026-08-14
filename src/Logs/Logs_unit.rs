use crate::restricted_tokens::out::elite_out;
use crate::structs::{
    AtomicProjectProjectionWriter, AuditConfig, AuditResult, GovernanceErrorsRefreshRequest,
};

#[path = "Logs_unitcore.rs"]
mod internal;

#[derive(Default, Debug, Clone)]
pub struct GovernanceArtifactPaths {
    pub output_root: String,
    pub violations_dir: String,
    pub terminal_report_path: String,
    pub violations_manifest_path: String,
    pub shadow_context_path: String,
}

pub struct GovernanceLogs;

pub struct GovernanceErrorsProjectionTarget<'a> {
    pub writer: &'a AtomicProjectProjectionWriter,
    pub request: &'a GovernanceErrorsRefreshRequest,
}

impl GovernanceLogs {
    pub const DEFAULT_OUTPUT_ROOT: &str = "AC_Ant-Delirium_props/Logs/artifacts";

    pub fn format_terminal_report(
        result: &AuditResult,
        config: &AuditConfig,
        artifacts: Option<&GovernanceArtifactPaths>,
    ) -> String {
        internal::format_terminal_report(result, config, artifacts)
    }

    pub fn to_shadow_context(
        result: &AuditResult,
        config: &AuditConfig,
        artifacts: Option<&GovernanceArtifactPaths>,
    ) -> String {
        internal::to_shadow_context(result, config, artifacts)
    }

    pub fn to_violation_manifest(
        result: &AuditResult,
        config: &AuditConfig,
        artifacts: Option<&GovernanceArtifactPaths>,
    ) -> String {
        internal::to_violation_manifest(result, config, artifacts)
    }

    pub fn write_artifacts(
        result: &AuditResult,
        config: &AuditConfig,
        output_root: &str,
        projection_target: Option<GovernanceErrorsProjectionTarget<'_>>,
    ) -> std::io::Result<GovernanceArtifactPaths> {
        internal::write_artifacts(result, config, output_root, projection_target)
    }

    pub fn emit_terminal_report(
        result: &AuditResult,
        config: &AuditConfig,
        artifacts: Option<&GovernanceArtifactPaths>,
    ) {
        elite_out(&Self::format_terminal_report(result, config, artifacts));
    }
}
