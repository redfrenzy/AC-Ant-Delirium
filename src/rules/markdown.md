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

    check_markdown_file(md_path, violations);
}

/// [WHAT] Performs the physical check for a markdown file.
/// [WHY] To keep 'verify' logic flat and avoid deep nesting.
fn check_markdown_file(path: &Path, violations: &mut Vec<AuditViolation>) {
    if !path.exists() {
        violations.push(AuditViolation {
            line: 1,
            rule_id: "rule_doc_parity".to_string(),
            message: "Mandatory .md companion file is missing. Archival parity failed.".to_string(),
        });
        return;
    }

    // Basic quality check
    if let Ok(metadata) = path.metadata() {
        if metadata.len() == 0 {
            violations.push(AuditViolation {
                line: 1,
                rule_id: "rule_doc_parity".to_string(),
                message: "Markdown file is empty. Documentation is mandatory for Elite geodes.".to_string(),
            });
        }
    }
}
