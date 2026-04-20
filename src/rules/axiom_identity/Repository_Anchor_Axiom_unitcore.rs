use crate::structs::AuditContext;
use std::path::{Path, PathBuf};

/// [AESTHETIC CORE - INTERNAL VACUUM]
pub fn patrol(ctx: &mut AuditContext<'_>) {
    let repository = match ctx.require_named_global_axiom_parameter("repository_anchor", "repository") {
        Ok(binding) => binding.value,
        Err(message) => {
            push_resolution_violation(ctx, "repository_anchor", &message);
            return;
        }
    };

    if repository.trim().is_empty() {
        return;
    }

    let repo_path = PathBuf::from(&repository);
    if !repo_path.is_absolute() {
        return;
    }

    // Relative file paths are normal in current Swarm; only enforce when file_path is absolute.
    let file_path = Path::new(ctx.file_path);
    if !file_path.is_absolute() {
        return;
    }

    if !file_path.starts_with(&repo_path) {
        ctx.violations.push(crate::structs::AuditViolation {
            file_path: ctx.file_path.to_string(),
            line: 1,
            rule_id: "hierarchy.identity.repository_anchor".to_string(),
            message: format!(
                "Repository anchor violation: '{}' is outside '{}'.",
                ctx.file_path, repository
            ),
            snippet: Some(ctx.file_path.to_string()),
            quick_fix: None,
            detected_dna: ctx.detected_dna.clone(),
            taxonomy_path: "hierarchy.identity.repository_anchor".to_string(),
        });
    }
}

fn push_resolution_violation(ctx: &mut AuditContext<'_>, rule_suffix: &str, message: &str) {
    ctx.violations.push(crate::structs::AuditViolation {
        file_path: ctx.file_path.to_string(),
        line: 1,
        rule_id: format!("hierarchy.identity.{}.resolution", rule_suffix),
        message: message.to_string(),
        snippet: Some(ctx.file_path.to_string()),
        quick_fix: None,
        detected_dna: ctx.detected_dna.clone(),
        taxonomy_path: ctx.taxonomy_path.clone(),
    });
}
