use crate::structs::AuditContext;
use std::path::Path;

/// [AESTHETIC CORE - INTERNAL VACUUM]
pub fn patrol(ctx: &mut AuditContext<'_>) {
    let manifest_flag = match ctx.require_named_hybrid_axiom_parameter("manifest_anchoring", "manifest") {
        Ok(binding) => binding.value,
        Err(message) => {
            push_resolution_violation(ctx, "manifest_anchoring", &message);
            return;
        }
    };

    if manifest_flag != "true" {
        return;
    }

    let tag_separator = ctx.config.hierarchy.resolve_parameter("tag_separator")
        .unwrap_or_else(|| "_".to_string());
    let path = Path::new(ctx.file_path);
    let ext = path.extension().and_then(|value| value.to_str()).unwrap_or_default();
    let stem = path.file_stem().and_then(|value| value.to_str()).unwrap_or_default();
    let bypass_manifest = ext == "json" && stem.ends_with("_manifest");

    if (ext == "md" || ext == "json") && stem.contains(&tag_separator) && !bypass_manifest {
        ctx.violations.push(crate::structs::AuditViolation {
            file_path: ctx.file_path.to_string(),
            line: 1,
            rule_id: "hierarchy.identity.manifest_anchoring".to_string(),
            message: "Manifest anchoring violation: child tags cannot own standalone .md/.json docs.".to_string(),
            snippet: Some(stem.to_string()),
            quick_fix: None,
            detected_dna: ctx.detected_dna.clone(),
            taxonomy_path: "hierarchy.identity.manifest_anchoring".to_string(),
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
