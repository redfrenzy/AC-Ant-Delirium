use std::collections::BTreeSet;
use std::fmt::Write;
use std::fs;
use std::path::{Path, PathBuf};

use crate::engine::Search_Engine_unit::SearchFiles;
use crate::structs::{AuditConfig, AuditResult, AuditViolation};

use super::GovernanceArtifactPaths;

pub fn format_terminal_report(
    result: &AuditResult,
    config: &AuditConfig,
    artifacts: Option<&GovernanceArtifactPaths>,
) -> String {
    let owned_paths;
    let paths = match artifacts {
        Some(paths) => paths,
        None => {
            owned_paths = resolve_artifact_paths(config, super::GovernanceLogs::DEFAULT_OUTPUT_ROOT);
            &owned_paths
        }
    };

    let mut out = String::new();
    let total = result.violations.len();
    let audited_files = SearchFiles::resolve_jurisdiction(&config.hierarchy, SearchFiles::ID);
    let unique_files = unique_file_count(&audited_files);
    let audited_file_list = unique_sorted_files(&audited_files);
    let (p0, p1, p2, p3) = count_priorities(&result.violations);

    out.push_str("\nAESTHETIC CORE  ▸  Ant Delirium Governance\n");
    out.push_str("==========================================\n");

    if total == 0 {
        let _ = writeln!(out, "Status: COMPLIANT  │  0 violations");
        let _ = writeln!(out, "Audited Files: {}", unique_files);
        let _ = writeln!(out, "Artifacts: {}", paths.output_root);
        out.push('\n');
        return out;
    }

    let _ = writeln!(out, "Status: NON-COMPLIANT  │  {} violations", total);
    let _ = writeln!(out, "Audited Files: {}", unique_files);
    let _ = writeln!(out, "P0-CRITICAL ··· {}", p0);
    let _ = writeln!(out, "P1-STRUCTURAL · {}", p1);
    let _ = writeln!(out, "P2-SEMANTIC ··· {}", p2);
    let _ = writeln!(out, "P3-ARCHIVAL ··· {}", p3);
    out.push('\n');

    out.push_str("[ARTIFACTS]\n");
    let _ = writeln!(out, "▸ terminal: {}", paths.terminal_report_path);
    let _ = writeln!(out, "▸ manifest: {}", paths.violations_manifest_path);
    let _ = writeln!(out, "▸ shadow:   {}", paths.shadow_context_path);
    let _ = writeln!(out, "▸ details:  {}", paths.violations_dir);
    out.push('\n');

    out.push_str("[AUDITED FILES]\n");
    for file_path in &audited_file_list {
        let _ = writeln!(out, "▸ {}", file_path);
    }
    out.push('\n');

    if !result.logs.is_empty() {
        out.push_str("[ORCHESTRATOR LOGS]\n");
        for log in &result.logs {
            let _ = writeln!(out, "▸ {}", log);
        }
        out.push('\n');
    }

    out.push_str("[VIOLATIONS]\n");
    let mut indexed: Vec<(usize, &AuditViolation)> = result.violations.iter().enumerate().collect();
    indexed.sort_by(|(_, left), (_, right)| {
        left.file_path
            .cmp(&right.file_path)
            .then(left.line.cmp(&right.line))
            .then(left.rule_id.cmp(&right.rule_id))
    });

    for (ordinal, (original_idx, violation)) in indexed.iter().enumerate() {
        let (priority, level) = classify_priority(&violation.rule_id);
        let log_path = violation_log_path(paths, *original_idx, violation);
        let _ = writeln!(
            out,
            "{:02}. [{}|{}] {}",
            ordinal + 1,
            priority,
            level,
            violation.rule_id
        );
        let _ = writeln!(out, "    file: {}:{}", violation.file_path, violation.line);
        let _ = writeln!(out, "    dna: {}", empty_as_unknown(&violation.detected_dna));
        let _ = writeln!(out, "    taxonomy: {}", empty_as_unknown(&violation.taxonomy_path));
        let _ = writeln!(out, "    log: {}", log_path.display());
        let _ = writeln!(out, "    message: {}", violation.message);
        if let Some(snippet) = &violation.snippet {
            let _ = writeln!(out, "    snippet: {}", snippet.replace('\n', "\\n"));
        }
        if let Some(quick_fix) = &violation.quick_fix {
            let _ = writeln!(out, "    quick_fix: {}", quick_fix.replace('\n', "\\n"));
        }
    }

    out.push('\n');
    out
}

pub fn to_violation_manifest(
    result: &AuditResult,
    config: &AuditConfig,
    artifacts: Option<&GovernanceArtifactPaths>,
) -> String {
    let owned_paths;
    let paths = match artifacts {
        Some(paths) => paths,
        None => {
            owned_paths = resolve_artifact_paths(config, super::GovernanceLogs::DEFAULT_OUTPUT_ROOT);
            &owned_paths
        }
    };

    let mut out = String::new();
    let audited_files = SearchFiles::resolve_jurisdiction(&config.hierarchy, SearchFiles::ID);
    let (p0, p1, p2, p3) = count_priorities(&result.violations);
    let audited_file_list = unique_sorted_files(&audited_files);

    out.push_str("{\n");
    out.push_str("  \"tool\": {\n");
    out.push_str("    \"name\": \"ac_ant_delirium\",\n");
    out.push_str("    \"version\": \"0.1.0\"\n");
    out.push_str("  },\n");
    out.push_str("  \"summary\": {\n");
    let _ = writeln!(out, "    \"total\": {},", result.violations.len());
    let _ = writeln!(out, "    \"audited_files\": {},", unique_file_count(&audited_files));
    let _ = writeln!(out, "    \"p0\": {},", p0);
    let _ = writeln!(out, "    \"p1\": {},", p1);
    let _ = writeln!(out, "    \"p2\": {},", p2);
    let _ = writeln!(out, "    \"p3\": {}", p3);
    out.push_str("  },\n");
    out.push_str("  \"logs\": [\n");
    for (idx, log) in result.logs.iter().enumerate() {
        let comma = item_separator(idx, result.logs.len());
        let _ = writeln!(out, "    \"{}\"{}", json_escape(log), comma);
    }
    out.push_str("  ],\n");
    out.push_str("  \"audited_files\": [\n");
    for (idx, file_path) in audited_file_list.iter().enumerate() {
        let comma = item_separator(idx, audited_file_list.len());
        let _ = writeln!(out, "    \"{}\"{}", json_escape(file_path), comma);
    }
    out.push_str("  ],\n");
    out.push_str("  \"violations\": [\n");

    for (idx, violation) in result.violations.iter().enumerate() {
        let (priority, level) = classify_priority(&violation.rule_id);
        let log_path = violation_log_path(paths, idx, violation);
        out.push_str("    {\n");
        let _ = writeln!(out, "      \"log_file\": \"{}\",", json_escape(&log_path.to_string_lossy()));
        let _ = writeln!(out, "      \"file_path\": \"{}\",", json_escape(&violation.file_path));
        let _ = writeln!(out, "      \"line\": {},", violation.line);
        let _ = writeln!(out, "      \"rule_id\": \"{}\",", json_escape(&violation.rule_id));
        let _ = writeln!(out, "      \"priority\": \"{}\",", priority);
        let _ = writeln!(out, "      \"level\": \"{}\",", level);
        let _ = writeln!(out, "      \"detected_dna\": \"{}\",", json_escape(&violation.detected_dna));
        let _ = writeln!(out, "      \"taxonomy_path\": \"{}\",", json_escape(&violation.taxonomy_path));
        let _ = writeln!(out, "      \"message\": \"{}\",", json_escape(&violation.message));
        let _ = writeln!(
            out,
            "      \"snippet\": {},",
            option_to_json_string(violation.snippet.as_deref())
        );
        let _ = writeln!(
            out,
            "      \"quick_fix\": {}",
            option_to_json_string(violation.quick_fix.as_deref())
        );
        let comma = item_separator(idx, result.violations.len());
        let _ = writeln!(out, "    }}{}", comma);
    }

    out.push_str("  ]\n");
    out.push('}');
    out
}

pub fn to_shadow_context(
    result: &AuditResult,
    config: &AuditConfig,
    artifacts: Option<&GovernanceArtifactPaths>,
) -> String {
    let owned_paths;
    let paths = match artifacts {
        Some(paths) => paths,
        None => {
            owned_paths = resolve_artifact_paths(config, super::GovernanceLogs::DEFAULT_OUTPUT_ROOT);
            &owned_paths
        }
    };

    let mut out = String::new();
    let audited_files = SearchFiles::resolve_jurisdiction(&config.hierarchy, SearchFiles::ID);
    let (p0, p1, p2, p3) = count_priorities(&result.violations);
    let audited_file_list = unique_sorted_files(&audited_files);

    out.push_str("{\n");
    out.push_str("  \"version\": \"2.1.0\",\n");
    out.push_str("  \"$schema\": \"https://schemastore.azurewebsites.net/schemas/json/sarif-2.1.0-rtm.4.json\",\n");
    out.push_str("  \"runs\": [\n");
    out.push_str("    {\n");
    out.push_str("      \"tool\": {\n");
    out.push_str("        \"driver\": {\n");
    out.push_str("          \"name\": \"ac_ant_delirium\",\n");
    out.push_str("          \"version\": \"0.1.0\"\n");
    out.push_str("        }\n");
    out.push_str("      },\n");
    out.push_str("      \"properties\": {\n");
    out.push_str("        \"governance\": {\n");
    let _ = writeln!(
        out,
        "          \"terminal_report\": \"{}\",",
        json_escape(&paths.terminal_report_path)
    );
        let _ = writeln!(
            out,
            "          \"violations_manifest\": \"{}\",",
            json_escape(&paths.violations_manifest_path)
        );
        let _ = writeln!(out, "          \"audited_files\": {},", unique_file_count(&audited_files));
        out.push_str("          \"audited_file_paths\": [\n");
        for (idx, file_path) in audited_file_list.iter().enumerate() {
            let comma = item_separator(idx, audited_file_list.len());
            let _ = writeln!(out, "            \"{}\"{}", json_escape(file_path), comma);
        }
        out.push_str("          ],\n");
        out.push_str("          \"summary\": {\n");
    let _ = writeln!(out, "            \"total\": {},", result.violations.len());
    let _ = writeln!(out, "            \"p0\": {},", p0);
    let _ = writeln!(out, "            \"p1\": {},", p1);
    let _ = writeln!(out, "            \"p2\": {},", p2);
    let _ = writeln!(out, "            \"p3\": {}", p3);
    out.push_str("          }\n");
    out.push_str("        }\n");
    out.push_str("      },\n");
    out.push_str("      \"results\": [\n");

    for (idx, violation) in result.violations.iter().enumerate() {
        let (priority, level) = classify_priority(&violation.rule_id);
        let log_path = violation_log_path(paths, idx, violation);
        out.push_str("        {\n");
        let _ = writeln!(out, "          \"ruleId\": \"{}\",", json_escape(&violation.rule_id));
        let _ = writeln!(out, "          \"level\": \"{}\",", level);
        let _ = writeln!(out, "          \"priority\": \"{}\",", priority);
        let _ = writeln!(out, "          \"taxonomy\": \"{}\",", json_escape(&violation.taxonomy_path));
        let _ = writeln!(out, "          \"dna\": \"{}\",", json_escape(&violation.detected_dna));
        let _ = writeln!(out, "          \"logFile\": \"{}\",", json_escape(&log_path.to_string_lossy()));
        let _ = writeln!(
            out,
            "          \"message\": {{ \"text\": \"{}\" }},",
            json_escape(&violation.message)
        );
        out.push_str("          \"locations\": [\n");
        out.push_str("            {\n");
        out.push_str("              \"physicalLocation\": {\n");
        let _ = writeln!(
            out,
            "                \"artifactLocation\": {{ \"uri\": \"{}\" }},",
            json_escape(&violation.file_path)
        );
        let _ = writeln!(out, "                \"region\": {{ \"startLine\": {} }}", violation.line);
        out.push_str("              }\n");
        out.push_str("            }\n");
        out.push_str("          ]\n");
        let comma = item_separator(idx, result.violations.len());
        let _ = writeln!(out, "        }}{}", comma);
    }

    out.push_str("      ]\n");
    out.push_str("    }\n");
    out.push_str("  ]\n");
    out.push('}');
    out
}

pub fn write_artifacts(
    result: &AuditResult,
    config: &AuditConfig,
    output_root: &str,
) -> std::io::Result<GovernanceArtifactPaths> {
    let paths = resolve_artifact_paths(config, output_root);
    fs::create_dir_all(&paths.output_root)?;
    fs::create_dir_all(&paths.violations_dir)?;

    for (idx, violation) in result.violations.iter().enumerate() {
        let detail_path = violation_log_path(&paths, idx, violation);
        let content = format_violation_detail(idx, violation);
        fs::write(detail_path, content)?;
    }

    let terminal_report = format_terminal_report(result, config, Some(&paths));
    fs::write(&paths.terminal_report_path, terminal_report)?;

    let manifest = to_violation_manifest(result, config, Some(&paths));
    fs::write(&paths.violations_manifest_path, manifest)?;

    let shadow = to_shadow_context(result, config, Some(&paths));
    fs::write(&paths.shadow_context_path, shadow)?;

    Ok(paths)
}

fn format_violation_detail(index: usize, violation: &AuditViolation) -> String {
    let (priority, level) = classify_priority(&violation.rule_id);
    let mut out = String::new();
    let _ = writeln!(out, "AESTHETIC CORE  ▸  Violation Detail");
    let _ = writeln!(out, "ordinal={}", index + 1);
    let _ = writeln!(out, "priority={}", priority);
    let _ = writeln!(out, "level={}", level);
    let _ = writeln!(out, "rule_id={}", violation.rule_id);
    let _ = writeln!(out, "file_path={}", violation.file_path);
    let _ = writeln!(out, "line={}", violation.line);
    let _ = writeln!(out, "detected_dna={}", empty_as_unknown(&violation.detected_dna));
    let _ = writeln!(out, "taxonomy_path={}", empty_as_unknown(&violation.taxonomy_path));
    let _ = writeln!(out, "message={}", violation.message);
    if let Some(snippet) = &violation.snippet {
        let _ = writeln!(out, "snippet={}", snippet);
    }
    if let Some(quick_fix) = &violation.quick_fix {
        let _ = writeln!(out, "quick_fix={}", quick_fix);
    }
    out
}

fn resolve_artifact_paths(config: &AuditConfig, output_root: &str) -> GovernanceArtifactPaths {
    let output_root = output_root.trim_end_matches('/').to_string();
    let violations_dir = format!("{}/violations", output_root);
    let terminal_report_path = format!("{}/governance_terminal.log", output_root);
    let violations_manifest_path = format!("{}/violations_manifest.json", output_root);
    let shadow_context_path = if config.shadow.output.trim().is_empty() {
        format!("{}/shadow_context.sarif.json", output_root)
    } else {
        config.shadow.output.clone()
    };

    GovernanceArtifactPaths {
        output_root,
        violations_dir,
        terminal_report_path,
        violations_manifest_path,
        shadow_context_path,
    }
}

fn violation_log_path(
    paths: &GovernanceArtifactPaths,
    idx: usize,
    violation: &AuditViolation,
) -> PathBuf {
    let file_tag = sanitize_file_name(&violation.file_path);
    let rule_tag = sanitize_file_name(&violation.rule_id);
    Path::new(&paths.violations_dir)
        .join(format!("{:04}__{}__{}.log", idx + 1, rule_tag, file_tag))
}

fn sanitize_file_name(input: &str) -> String {
    input.chars()
        .map(|ch| match ch {
            'a'..='z' | 'A'..='Z' | '0'..='9' => ch,
            _ => '_',
        })
        .collect()
}

fn count_priorities(violations: &[AuditViolation]) -> (usize, usize, usize, usize) {
    let mut counts = (0, 0, 0, 0);
    for violation in violations {
        match classify_priority(&violation.rule_id).0 {
            "P0" => counts.0 += 1,
            "P1" => counts.1 += 1,
            "P2" => counts.2 += 1,
            _ => counts.3 += 1,
        }
    }
    counts
}

fn classify_priority(rule_id: &str) -> (&'static str, &'static str) {
    if rule_id.contains(".sanitization.") || rule_id.contains(".dependency_governance.") {
        return ("P0", "error");
    }
    if rule_id.contains(".structural.") {
        return ("P1", "warning");
    }
    if rule_id.contains(".identity.") {
        return ("P2", "note");
    }
    if rule_id.contains(".pedagogical.") {
        return ("P3", "note");
    }
    ("P2", "note")
}

fn unique_file_count(paths: &[String]) -> usize {
    let set: BTreeSet<&str> = paths.iter().map(String::as_str).collect();
    set.len()
}

fn unique_sorted_files(paths: &[String]) -> Vec<String> {
    let set: BTreeSet<String> = paths.iter().cloned().collect();
    set.into_iter().collect()
}

fn empty_as_unknown(value: &str) -> &str {
    if value.is_empty() { "<unknown>" } else { value }
}

fn item_separator(idx: usize, len: usize) -> &'static str {
    if idx + 1 < len { "," } else { "" }
}

fn option_to_json_string(value: Option<&str>) -> String {
    match value {
        Some(value) => format!("\"{}\"", json_escape(value)),
        None => "null".to_string(),
    }
}

fn json_escape(value: &str) -> String {
    let mut out = String::with_capacity(value.len() + 8);
    for ch in value.chars() {
        match ch {
            '\\' => out.push_str("\\\\"),
            '"' => out.push_str("\\\""),
            '\n' => out.push_str("\\n"),
            '\r' => out.push_str("\\r"),
            '\t' => out.push_str("\\t"),
            _ => out.push(ch),
        }
    }
    out
}
