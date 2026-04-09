use crate::AuditViolation;
use crate::config::AuditConfig;
use std::path::Path;

/// [WHAT] Verifies the existence and quality of paired markdown documentation.
/// [WHY] To satisfy the P3 Archival requirement and ensure pedagogical depth.
/// [WHERE] Called during the archival audit phase of the rule engine.
pub fn verify(file_path: &str, _source: &str, _config: &AuditConfig, violations: &mut Vec<AuditViolation>) {
    if file_path.ends_with(".md") { return; }

    let md_path_str = file_path.replace(".rs", ".md");
    let md_path = Path::new(&md_path_str);
    let rs_path = Path::new(file_path);

    if !md_path.exists() {
        push_archival_violation(violations, "Mandatory .md companion file is missing.");
        return;
    }

    validate_markdown_content(md_path, violations);
    check_staleness(rs_path, md_path, violations);
}

/// [WHAT] Validates the content length of a markdown file.
/// [WHY] To ensure that documentation is not just present but meaningful (P3-Archival).
/// [WHERE] Quality control stage of the archival audit.
fn validate_markdown_content(path: &Path, violations: &mut Vec<AuditViolation>) {
    let content = match std::fs::read_to_string(path) {
        Ok(c) => c,
        Err(_) => return,
    };

    if content.trim().len() < 256 {
        push_archival_violation(violations, "Empty Documentation: .md file lacks substantial content (min 256 chars required).");
    }
}

/// [WHAT] Pushes a standardized archival violation to the result set.
/// [WHY] To centralize violation reporting and keep logic shallow.
/// [WHERE] Diagnostic reporting layer.
fn push_archival_violation(violations: &mut Vec<AuditViolation>, message: &str) {
    violations.push(AuditViolation {
        line: 1,
        rule_id: "rule_md_pairing", // Unified rule_id for consistency
        message: message.to_string(),
        snippet: None,
        quick_fix: None,
    });
}

/// [WHAT] Detects modification drift between source and documentation.
/// [WHY] To prevent documentation from becoming stale and out-of-sync with implementation.
/// [WHERE] Final stage of the P3-Archival verification.
fn check_staleness(rs_path: &Path, md_path: &Path, violations: &mut Vec<AuditViolation>) {
    let rs_meta = match rs_path.metadata() { Ok(m) => m, Err(_) => return };
    let md_meta = match md_path.metadata() { Ok(m) => m, Err(_) => return };
    
    let rs_time = match rs_meta.modified() { Ok(t) => t, Err(_) => return };
    let md_time = match md_meta.modified() { Ok(t) => t, Err(_) => return };

    let drift = match rs_time.duration_since(md_time) {
        Ok(d) => d,
        Err(_) => return, // Documentation is newer or same age
    };

    // 48-hour rule (172,800 seconds)
    if drift.as_secs() > 172_800 {
        push_archival_violation(violations, "Stale Documentation: .rs file is > 48h newer than its .md companion. Update docs.");
    }
}
