use crate::structs::AuditContext;
use std::path::{Path, PathBuf};

#[path = "Identity_Axiom_Common_unitcore.rs"]
mod Identity_Axiom_Common_unitcore;

/// [AESTHETIC CORE - INTERNAL VACUUM]
pub fn patrol(ctx: &mut AuditContext<'_>) {
    let repository =
        match ctx.require_named_global_axiom_parameter("repository_anchor", "repository") {
            Ok(binding) => binding.value,
            Err(message) => {
                Identity_Axiom_Common_unitcore::push_resolution_violation(
                    ctx,
                    "repository_anchor",
                    &message,
                );
                return;
            }
        };
    let identity_path = Identity_Axiom_Common_unitcore::resolve_axiom_identity_path_or_fallback(
        ctx,
        "repository_anchor",
    );

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
            rule_id: identity_path.clone(),
            message: format!(
                "Repository anchor violation: '{}' is outside '{}'.",
                ctx.file_path, repository
            ),
            snippet: Some(ctx.file_path.to_string()),
            quick_fix: None,
            detected_dna: ctx.detected_dna.clone(),
            taxonomy_path: identity_path,
        });
    }
}
