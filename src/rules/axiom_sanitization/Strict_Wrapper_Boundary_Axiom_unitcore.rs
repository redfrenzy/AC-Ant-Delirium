use crate::governance;
use crate::rules_logs::messages;
use crate::structs::{AuditContext, AuditViolation};

pub fn patrol(ctx: &mut AuditContext<'_>) {
    if !is_libres_wrapper_file(ctx.file_path) {
        return;
    }

    let resolution = match ctx
        .require_named_global_axiom_parameter("strict_wrapper_boundary", "wrapper_boundary_contract")
    {
            Ok(value) => value,
            Err(message) => {
                push_resolution_violation(
                    ctx,
                    "strict_wrapper_boundary",
                    &message,
                );
                return;
            }
        };

    let contract = resolution.value.trim().to_ascii_lowercase();
    if contract == "disabled" || contract == "off" || contract == "false" {
        return;
    }
    for (line, normalized) in collect_glob_reexports(ctx.source).into_iter() {
        ctx.violations.push(AuditViolation {
            file_path: ctx.file_path.to_string(),
            line,
            rule_id: governance::rule_id(
                &resolution.identity_path,
                &format!("{}_glob_reexport", normalized),
            ),
            message: format!(
                "{}: wrapper '{}' must not expose crate '{}' as a full surface (`pub use ...::*`). Expose only explicit boundary items required by the project.",
                messages::DEPS_UNAUTHORIZED,
                ctx.file_path,
                normalized
            ),
            snippet: Some(format!("pub use {}::*;", normalized)),
            quick_fix: Some(format!(
                "# [SANITIZE] Do not export the full crate from src/libres.\n# Export only explicit items used by the project and define AC_* aliases.\n# Abstract example:\n#   pub use {0}::{{TypeA, TypeB}};\n#   pub type AC_TypeA = TypeA;\n#   pub type AC_TypeB = TypeB;",
                normalized
            )),
            detected_dna: ctx.detected_dna.clone(),
            taxonomy_path: resolution.resolved_path.to_string(),
        });
    }
    for (line, normalized, snippet) in collect_direct_reexports_without_ac_alias(ctx.source).into_iter() {
        ctx.violations.push(AuditViolation {
            file_path: ctx.file_path.to_string(),
            line,
            rule_id: governance::rule_id(
                &resolution.identity_path,
                &format!("{}_direct_reexport", normalized),
            ),
            message: format!(
                "{}: wrapper '{}' must not re-export '{}' directly without an AC_* boundary alias. Expose explicit project-facing aliases (`as AC_*` or `pub type AC_* = ...`).",
                messages::DEPS_UNAUTHORIZED,
                ctx.file_path,
                normalized
            ),
            snippet: Some(snippet),
            quick_fix: Some(
                "# [SANITIZE] Replace direct forwarding with an AC_* boundary alias.\n# Abstract examples:\n#   pub use crate_name::Item as AC_Item;\n#   pub type AC_Item = crate_name::Item;"
                    .to_string(),
            ),
            detected_dna: ctx.detected_dna.clone(),
            taxonomy_path: resolution.resolved_path.to_string(),
        });
    }

    if !has_ac_alias_boundary(ctx.source) {
        let wrapper = infer_wrapper_crate_name(ctx.file_path).unwrap_or_else(|| "wrapper".to_string());
        ctx.violations.push(AuditViolation {
            file_path: ctx.file_path.to_string(),
            line: 1,
            rule_id: governance::rule_id(
                &resolution.identity_path,
                &format!("{}_ac_alias_boundary", wrapper),
            ),
            message: format!(
                "{}: wrapper '{}' must expose a strict AC_* boundary and avoid full crate forwarding. Define explicit project-facing aliases (`AC_*`) for each consumed element.",
                messages::DEPS_UNAUTHORIZED,
                ctx.file_path
            ),
            snippet: Some("AC_String / AC_Vec / AC_Map".to_string()),
            quick_fix: Some(
                "# [SANITIZE] In src/libres wrappers, expose only explicit items and map them to AC_* aliases.\n# Abstract std-like boundary example:\n#   pub use crate_name::TypeA;\n#   pub use crate_name::TypeB;\n#   pub type AC_TypeA = TypeA;\n#   pub type AC_TypeB = TypeB;\n# Avoid wildcard/full forwarding of the crate surface."
                    .to_string(),
            ),
            detected_dna: ctx.detected_dna.clone(),
            taxonomy_path: resolution.resolved_path.to_string(),
        });
    }
}

fn is_libres_wrapper_file(path: &str) -> bool {
    path.ends_with(".rs")
        && (path.starts_with("src/libres/") || path.contains("/src/libres/"))
        && !path.ends_with("/mod.rs")
        && !path.ends_with("_unitcore.rs")
}

fn normalize_crate_name(name: &str) -> String {
    name.trim().to_lowercase().replace('-', "_")
}

fn normalize_line(raw: &str) -> String {
    raw.chars().filter(|ch| !ch.is_whitespace()).collect()
}

fn collect_glob_reexports(source: &str) -> Vec<(usize, String)> {
    let mut out = Vec::new();
    for (idx, raw) in source.lines().enumerate() {
        let compact = normalize_line(raw);
        if !compact.starts_with("pubuse") || !compact.ends_with("::*;") {
            continue;
        }
        let core = compact
            .trim_start_matches("pubuse")
            .trim_end_matches("::*;");
        if core.is_empty() {
            continue;
        }
        out.push((idx + 1, normalize_crate_name(core)));
    }
    out
}

fn collect_direct_reexports_without_ac_alias(source: &str) -> Vec<(usize, String, String)> {
    let mut out = Vec::new();
    for (idx, raw) in source.lines().enumerate() {
        let clean = raw.split("//").next().unwrap_or("").trim();
        if !clean.starts_with("pub use ") || !clean.ends_with(';') {
            continue;
        }
        if clean.contains('{') || clean.contains('}') {
            continue;
        }
        if clean.contains(" as AC_") {
            continue;
        }
        let path = clean
            .trim_start_matches("pub use ")
            .trim_end_matches(';')
            .trim();
        if path.is_empty() || !path.contains("::") {
            continue;
        }
        let normalized = normalize_crate_name(path);
        out.push((idx + 1, normalized, clean.to_string()));
    }
    out
}

fn infer_wrapper_crate_name(path: &str) -> Option<String> {
    let file = path.rsplit('/').next()?;
    let stem = file.strip_suffix(".rs")?;
    let crate_segment = stem.strip_suffix("_unit")?;
    if crate_segment.is_empty() {
        return None;
    }
    Some(normalize_crate_name(crate_segment))
}

fn has_ac_alias_boundary(source: &str) -> bool {
    source.lines().any(|raw| {
        let compact = raw.split("//").next().unwrap_or("").trim();
        if compact.is_empty() {
            return false;
        }
        compact.contains("pub type AC_")
            || (compact.contains("pub use ") && compact.contains(" as AC_"))
    })
}

fn push_resolution_violation(
    ctx: &mut AuditContext<'_>,
    identity_path: &str,
    message: &str,
) {
    ctx.violations.push(AuditViolation {
        file_path: ctx.file_path.to_string(),
        line: 1,
        rule_id: governance::rule_id(identity_path, "configuration"),
        message: format!("Hierarchy resolution failure: {}", message),
        snippet: None,
        quick_fix: Some(
            "# [CONFIG] Ensure the strict wrapper boundary contract is declared (for example: hierarchy.permissions.wrapper_boundary_contract = \"explicit_ac_alias\") and reachable for strict wrapper boundary enforcement.".to_string(),
        ),
        detected_dna: ctx.detected_dna.clone(),
        taxonomy_path: ctx.taxonomy_path.clone(),
    });
}
