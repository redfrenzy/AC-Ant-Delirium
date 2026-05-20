use std::collections::{BTreeMap, BTreeSet};
use std::env;
use std::fmt::Write;
use std::fs;
use std::path::{Path, PathBuf};

use crate::engine::Search_Engine_unit::SearchFiles;
use crate::structs::{AuditConfig, AuditResult, AuditViolation};

use super::GovernanceArtifactPaths;

const ORDERED_AXIOM_IDS: [&str; 24] = [
    "super_block",
    "max_nesting",
    "max_params",
    "logical_density",
    "type_logic",
    "md_pairing",
    "md_min_chars",
    "what_mission",
    "why_justification",
    "usage_example",
    "test_case_mapping",
    "root_md_pairing",
    "inheritance_visibility",
    "dna_suffix",
    "tag_separator",
    "pascal_case",
    "recursion",
    "manifest_anchoring",
    "name_separator",
    "repository_anchor",
    "restricted_tokens",
    "allowed_crates",
    "token_sanitization",
    "dependency_purification",
];

const CONTEXT_RADIUS: usize = 2;

pub fn format_terminal_report(
    result: &AuditResult,
    config: &AuditConfig,
    artifacts: Option<&GovernanceArtifactPaths>,
) -> String {
    let owned_paths;
    let paths = match artifacts {
        Some(paths) => paths,
        None => {
            owned_paths =
                resolve_artifact_paths(config, super::GovernanceLogs::DEFAULT_OUTPUT_ROOT);
            &owned_paths
        }
    };

    let mut out = String::new();
    let total = result.violations.len();
    let (p0, p1, p2, p3, p4) = count_priorities(&result.violations);
    let audited_files = SearchFiles::resolve_jurisdiction(&config.hierarchy, SearchFiles::ID);
    let audited_file_count = unique_file_count(&audited_files);
    let violation_files = violation_files_from_violations(&result.violations);
    let external_usage_files = external_usage_files_from_violations(&result.violations);
    let shadow_path = config.display_path(&paths.shadow_context_path);
    let manifest_path = config.display_path(&paths.violations_manifest_path);
    let api_help_command = runtime_help_command(config);
    let _ = writeln!(out, "P0: {} erros", p0);
    let _ = writeln!(out, "P1: {} erros", p1);
    let _ = writeln!(out, "P2: {} erros", p2);
    let _ = writeln!(out, "P3: {} erros", p3);
    let _ = writeln!(out, "P4: {} erros", p4);
    out.push('\n');
    let _ = writeln!(out, "Total: {} erros", total);
    out.push('\n');
    let _ = writeln!(out, "Audited Files: {}", audited_file_count);
    let _ = writeln!(out, "Violation Files: {}", violation_files.len());
    if !external_usage_files.is_empty() {
        let _ = writeln!(
            out,
            "External Usage Files: {}",
            external_usage_files.join(", ")
        );
    }
    let _ = writeln!(out, "Violations Manifest: {}", manifest_path);
    let _ = writeln!(out, "Shadow Context: {}", shadow_path);
    out.push('\n');
    let _ = writeln!(out, "API Help: {}", api_help_command);
    if total > 0 {
        out.push('\n');
        let _ = writeln!(
            out,
            "Modo estrito: ativo. A compilacao deve ser interrompida ate a correcao das violacoes."
        );
    } else {
        out.push('\n');
        let _ = writeln!(
            out,
            "Modo estrito: ativo. Nenhuma violacao bloqueante encontrada."
        );
    }
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
            owned_paths =
                resolve_artifact_paths(config, super::GovernanceLogs::DEFAULT_OUTPUT_ROOT);
            &owned_paths
        }
    };

    let mut out = String::new();
    let audited_files = SearchFiles::resolve_jurisdiction(&config.hierarchy, SearchFiles::ID);
    let (p0, p1, p2, p3, p4) = count_priorities(&result.violations);
    let audited_file_list = unique_sorted_files(&audited_files);
    let violation_file_list = violation_files_from_violations(&result.violations);
    let external_usage_files = external_usage_files_from_violations(&result.violations);

    out.push_str("{\n");
    out.push_str("  \"tool\": {\n");
    out.push_str("    \"name\": \"ac_ant_delirium\",\n");
    out.push_str("    \"version\": \"0.1.0\"\n");
    out.push_str("  },\n");
    out.push_str("  \"summary\": {\n");
    let _ = writeln!(out, "    \"total\": {},", result.violations.len());
    let _ = writeln!(
        out,
        "    \"audited_files\": {},",
        unique_file_count(&audited_files)
    );
    let _ = writeln!(out, "    \"p0\": {},", p0);
    let _ = writeln!(out, "    \"p1\": {},", p1);
    let _ = writeln!(out, "    \"p2\": {},", p2);
    let _ = writeln!(out, "    \"p3\": {},", p3);
    let _ = writeln!(out, "    \"p4\": {}", p4);
    out.push_str("  },\n");
    out.push_str("  \"ai_guidance\": {\n");
    let _ = writeln!(
        out,
        "    \"api_help_command\": \"{}\",",
        json_escape(&runtime_help_command(config))
    );
    let _ = writeln!(
        out,
        "    \"installation_command\": \"{}\",",
        json_escape(&runtime_installation_command(config))
    );
    out.push_str("    \"workflow\": [\n");
    let workflow = vec![
        "Read each violation item in priority order.".to_string(),
        "Use api_rule_command to understand the violated rule.".to_string(),
        "Use api_taxonomy_command to inspect the active taxonomy node when needed.".to_string(),
        "Apply quick_fix and then rerun the audit.".to_string(),
    ];
    write_json_string_array_items(&mut out, &workflow, 6);
    out.push_str("    ]\n");
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
    out.push_str("  \"violation_files\": [\n");
    for (idx, file_path) in violation_file_list.iter().enumerate() {
        let comma = item_separator(idx, violation_file_list.len());
        let _ = writeln!(out, "    \"{}\"{}", json_escape(file_path), comma);
    }
    out.push_str("  ],\n");
    out.push_str("  \"external_usage_files\": [\n");
    for (idx, file_path) in external_usage_files.iter().enumerate() {
        let comma = item_separator(idx, external_usage_files.len());
        let _ = writeln!(out, "    \"{}\"{}", json_escape(file_path), comma);
    }
    out.push_str("  ],\n");
    out.push_str("  \"violations\": [\n");

    for (idx, violation) in result.violations.iter().enumerate() {
        let (priority, level) = classify_priority(&violation.rule_id);
        let log_path = rule_log_path(paths, &grouped_rule_key(&violation.rule_id));
        let rule_doc_id = grouped_rule_key(&violation.rule_id);
        let api_rule_path = runtime_rule_doc_path(&config.hierarchy, &rule_doc_id);
        let api_taxonomy_path = runtime_taxonomy_info_path(violation);
        let remediation_kind = remediation_kind(&rule_doc_id);
        let feedback_command = runtime_feedback_command(&api_taxonomy_path);
        let feedback_help_command = runtime_feedback_help_command();
        let taxonomy_guidance = taxonomy_guidance(&api_taxonomy_path);
        let rule_guidance = rule_guidance(&api_rule_path, &rule_doc_id);
        let policy_target = if violation.taxonomy_path.trim().is_empty() {
            api_taxonomy_path.clone()
        } else {
            violation.taxonomy_path.clone()
        };
        out.push_str("    {\n");
        let _ = writeln!(
            out,
            "      \"log_file\": \"{}\",",
            json_escape(&log_path.to_string_lossy())
        );
        let _ = writeln!(
            out,
            "      \"file_path\": \"{}\",",
            json_escape(&violation.file_path)
        );
        out.push_str("      \"offending_files\": [\n");
        let _ = writeln!(
            out,
            "        \"{}\"",
            json_escape(&violation.file_path)
        );
        out.push_str("      ],\n");
        let _ = writeln!(
            out,
            "      \"observed_in\": \"{}\",",
            json_escape(&violation.file_path)
        );
        out.push_str("      \"offense_targets\": [\n");
        let _ = writeln!(
            out,
            "        \"{}\"",
            json_escape(&violation.file_path)
        );
        out.push_str("      ],\n");
        let _ = writeln!(
            out,
            "      \"policy_target\": \"{}\",",
            json_escape(&policy_target)
        );
        let _ = writeln!(out, "      \"line\": {},", violation.line);
        let _ = writeln!(
            out,
            "      \"rule_id\": \"{}\",",
            json_escape(&violation.rule_id)
        );
        let _ = writeln!(out, "      \"priority\": \"{}\",", priority);
        let _ = writeln!(out, "      \"level\": \"{}\",", level);
        let _ = writeln!(
            out,
            "      \"detected_dna\": \"{}\",",
            json_escape(&violation.detected_dna)
        );
        let _ = writeln!(
            out,
            "      \"taxonomy_path\": \"{}\",",
            json_escape(&violation.taxonomy_path)
        );
        let _ = writeln!(
            out,
            "      \"message\": \"{}\",",
            json_escape(&violation.message)
        );
        let _ = writeln!(
            out,
            "      \"rule_doc_id\": \"{}\",",
            json_escape(&rule_doc_id)
        );
        let _ = writeln!(
            out,
            "      \"api_rule_path\": \"{}\",",
            json_escape(&api_rule_path)
        );
        let _ = writeln!(
            out,
            "      \"api_rule_command\": \"{}\",",
            json_escape(&format!(
                "{} --api {}",
                runtime_binary_display(),
                api_rule_path
            ))
        );
        let _ = writeln!(
            out,
            "      \"api_taxonomy_path\": \"{}\",",
            json_escape(&api_taxonomy_path)
        );
        let _ = writeln!(
            out,
            "      \"api_taxonomy_command\": \"{}\",",
            json_escape(&format!(
                "{} --api {}",
                runtime_binary_display(),
                api_taxonomy_path
            ))
        );
        let _ = writeln!(
            out,
            "      \"feedback_command\": \"{}\",",
            json_escape(&feedback_command)
        );
        let _ = writeln!(
            out,
            "      \"feedback_help_command\": \"{}\",",
            json_escape(&feedback_help_command)
        );
        let _ = writeln!(
            out,
            "      \"remediation_kind\": \"{}\",",
            json_escape(remediation_kind)
        );
        out.push_str("      \"taxonomy_guidance\": [\n");
        write_json_string_array_items(&mut out, &taxonomy_guidance, 8);
        out.push_str("      ],\n");
        out.push_str("      \"rule_guidance\": [\n");
        write_json_string_array_items(&mut out, &rule_guidance, 8);
        out.push_str("      ],\n");
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
            owned_paths =
                resolve_artifact_paths(config, super::GovernanceLogs::DEFAULT_OUTPUT_ROOT);
            &owned_paths
        }
    };

    let mut out = String::new();
    let audited_files = SearchFiles::resolve_jurisdiction(&config.hierarchy, SearchFiles::ID);
    let (p0, p1, p2, p3, p4) = count_priorities(&result.violations);
    let audited_file_list = unique_sorted_files(&audited_files);
    let violation_file_list = violation_files_from_violations(&result.violations);
    let context_items = default_shadow_context_lines();
    let tools = if config.info.tools.is_empty() {
        default_shadow_tools()
    } else {
        config.info.tools.clone()
    };
    let terminal_commands = if config.info.terminal_commands.is_empty() {
        default_shadow_terminal_commands(config)
    } else {
        config.info.terminal_commands.clone()
    };
    let max_nesting = resolve_numeric_or_default(&config.hierarchy, "max_nesting");
    let max_params = resolve_numeric_or_default(&config.hierarchy, "max_params");
    let logical_density = resolve_numeric_or_default(&config.hierarchy, "logical_density");
    let restricted_tokens = resolve_array_or_default(
        &config.hierarchy,
        "restricted_tokens",
        vec!["panic!".to_string(), "unwrap".to_string()],
    );
    let allowed_crates = resolve_array_or_default(
        &config.hierarchy,
        "libres",
        vec!["std".to_string(), "core".to_string(), "alloc".to_string()],
    );

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
    out.push_str("          \"priority_order\": \"P0 → P1 → P2 → P3 → P4\",\n");
    let _ = writeln!(
        out,
        "          \"total_violations\": {},",
        result.violations.len()
    );
    let _ = writeln!(
        out,
        "          \"audited_files\": {},",
        unique_file_count(&audited_files)
    );
    let _ = writeln!(
        out,
        "          \"violation_files\": {},",
        violation_file_list.len()
    );
    out.push_str("          \"audited_file_paths\": [\n");
    for (idx, file_path) in audited_file_list.iter().enumerate() {
        let comma = item_separator(idx, audited_file_list.len());
        let _ = writeln!(out, "            \"{}\"{}", json_escape(file_path), comma);
    }
    out.push_str("          ],\n");
    out.push_str("          \"violation_file_paths\": [\n");
    for (idx, file_path) in violation_file_list.iter().enumerate() {
        let comma = item_separator(idx, violation_file_list.len());
        let _ = writeln!(out, "            \"{}\"{}", json_escape(file_path), comma);
    }
    out.push_str("          ],\n");
    out.push_str("          \"context\": [\n");
    write_json_string_array_items(&mut out, &context_items, 12);
    out.push_str("          ],\n");
    out.push_str("          \"tools\": [\n");
    write_json_string_array_items(&mut out, &tools, 12);
    out.push_str("          ],\n");
    out.push_str("          \"terminal_commands\": [\n");
    write_json_string_array_items(&mut out, &terminal_commands, 12);
    out.push_str("          ],\n");
    out.push_str("          \"applied_config\": {\n");
    let _ = writeln!(out, "            \"max_nesting\": {},", max_nesting);
    let _ = writeln!(out, "            \"max_params\": {},", max_params);
    let _ = writeln!(out, "            \"logical_density\": {},", logical_density);
    out.push_str("            \"restricted_tokens\": [\n");
    write_json_string_array_items(&mut out, &restricted_tokens, 14);
    out.push_str("            ],\n");
    out.push_str("            \"allowed_crates\": [\n");
    write_json_string_array_items(&mut out, &allowed_crates, 14);
    out.push_str("            ]\n");
    out.push_str("          },\n");
    out.push_str("          \"summary\": {\n");
    let _ = writeln!(out, "            \"total\": {},", result.violations.len());
    let _ = writeln!(out, "            \"p0\": {},", p0);
    let _ = writeln!(out, "            \"p1\": {},", p1);
    let _ = writeln!(out, "            \"p2\": {},", p2);
    let _ = writeln!(out, "            \"p3\": {},", p3);
    let _ = writeln!(out, "            \"p4\": {}", p4);
    out.push_str("          }\n");
    out.push_str("        }\n");
    out.push_str("      },\n");
    out.push_str("      \"results\": [\n");

    for (idx, violation) in result.violations.iter().enumerate() {
        let (priority, level) = classify_priority(&violation.rule_id);
        let log_path = rule_log_path(paths, &grouped_rule_key(&violation.rule_id));
        let rule_doc_id = grouped_rule_key(&violation.rule_id);
        let api_rule_path = runtime_rule_doc_path(&config.hierarchy, &rule_doc_id);
        let api_taxonomy_path = runtime_taxonomy_info_path(violation);
        let feedback_command = runtime_feedback_command(&api_taxonomy_path);
        let feedback_help_command = runtime_feedback_help_command();
        let taxonomy_guidance = taxonomy_guidance(&api_taxonomy_path);
        let rule_guidance = rule_guidance(&api_rule_path, &rule_doc_id);
        let policy_target = if violation.taxonomy_path.trim().is_empty() {
            api_taxonomy_path.clone()
        } else {
            violation.taxonomy_path.clone()
        };
        out.push_str("        {\n");
        let _ = writeln!(
            out,
            "          \"ruleId\": \"{}\",",
            json_escape(&violation.rule_id)
        );
        let _ = writeln!(out, "          \"level\": \"{}\",", level);
        let _ = writeln!(out, "          \"priority\": \"{}\",", priority);
        let _ = writeln!(
            out,
            "          \"taxonomy\": \"{}\",",
            json_escape(&violation.taxonomy_path)
        );
        let _ = writeln!(
            out,
            "          \"dna\": \"{}\",",
            json_escape(&violation.detected_dna)
        );
        let _ = writeln!(
            out,
            "          \"logFile\": \"{}\",",
            json_escape(&log_path.to_string_lossy())
        );
        let _ = writeln!(
            out,
            "          \"message\": {{ \"text\": \"{}\" }},",
            json_escape(&violation.message)
        );
        out.push_str("          \"properties\": {\n");
        let _ = writeln!(
            out,
            "            \"rule_doc_id\": \"{}\",",
            json_escape(&rule_doc_id),
        );
        let _ = writeln!(
            out,
            "            \"api_rule_path\": \"{}\",",
            json_escape(&api_rule_path),
        );
        let _ = writeln!(
            out,
            "            \"api_taxonomy_path\": \"{}\",",
            json_escape(&api_taxonomy_path),
        );
        let _ = writeln!(
            out,
            "            \"feedback_command\": \"{}\",",
            json_escape(&feedback_command),
        );
        let _ = writeln!(
            out,
            "            \"feedback_help_command\": \"{}\",",
            json_escape(&feedback_help_command),
        );
        let _ = writeln!(
            out,
            "            \"remediation_kind\": \"{}\",",
            json_escape(remediation_kind(&rule_doc_id)),
        );
        out.push_str("            \"offending_files\": [\n");
        let _ = writeln!(
            out,
            "              \"{}\"",
            json_escape(&violation.file_path)
        );
        out.push_str("            ],\n");
        let _ = writeln!(
            out,
            "            \"observed_in\": \"{}\",",
            json_escape(&violation.file_path),
        );
        out.push_str("            \"offense_targets\": [\n");
        let _ = writeln!(
            out,
            "              \"{}\"",
            json_escape(&violation.file_path)
        );
        out.push_str("            ],\n");
        let _ = writeln!(
            out,
            "            \"policy_target\": \"{}\",",
            json_escape(&policy_target),
        );
        out.push_str("            \"taxonomy_guidance\": [\n");
        write_json_string_array_items(&mut out, &taxonomy_guidance, 14);
        out.push_str("            ],\n");
        out.push_str("            \"rule_guidance\": [\n");
        write_json_string_array_items(&mut out, &rule_guidance, 14);
        out.push_str("            ]\n");
        out.push_str("          },\n");
        out.push_str("          \"locations\": [\n");
        out.push_str("            {\n");
        out.push_str("              \"physicalLocation\": {\n");
        let _ = writeln!(
            out,
            "                \"artifactLocation\": {{ \"uri\": \"{}\" }},",
            json_escape(&violation.file_path)
        );
        if let Some(snippet) = &violation.snippet {
            let _ = writeln!(
                out,
                "                \"region\": {{ \"startLine\": {}, \"snippet\": {{ \"text\": \"{}\" }} }}",
                violation.line,
                json_escape(snippet),
            );
        } else {
            let _ = writeln!(
                out,
                "                \"region\": {{ \"startLine\": {} }}",
                violation.line
            );
        }
        out.push_str("              }\n");
        out.push_str("            }\n");
        out.push_str("          ]");
        if let Some(quick_fix) = &violation.quick_fix {
            out.push_str(",\n");
            out.push_str("          \"fixes\": [\n");
            out.push_str("            {\n");
            out.push_str(
                "              \"description\": { \"text\": \"Apply governance quick-fix\" },\n",
            );
            out.push_str("              \"artifactChanges\": [\n");
            out.push_str("                {\n");
            let _ = writeln!(
                out,
                "                  \"artifactLocation\": {{ \"uri\": \"{}\" }},",
                json_escape(&violation.file_path)
            );
            out.push_str("                  \"replacements\": [\n");
            out.push_str("                    {\n");
            let _ = writeln!(
                out,
                "                      \"deletedRegion\": {{ \"startLine\": {}, \"endLine\": {} }},",
                violation.line, violation.line
            );
            let _ = writeln!(
                out,
                "                      \"insertedContent\": {{ \"text\": \"{}\" }}",
                json_escape(quick_fix)
            );
            out.push_str("                    }\n");
            out.push_str("                  ]\n");
            out.push_str("                }\n");
            out.push_str("              ]\n");
            out.push_str("            }\n");
            out.push_str("          ]\n");
        } else {
            out.push('\n');
        }
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
    if Path::new(&paths.violations_dir).exists() {
        fs::remove_dir_all(&paths.violations_dir)?;
    }
    fs::create_dir_all(&paths.violations_dir)?;

    let grouped = group_violations_by_rule(&result.violations);
    for rule_key in ordered_rule_log_keys(&grouped) {
        let detail_path = rule_log_path(&paths, &rule_key);
        let content = match grouped.get(&rule_key) {
            Some(entries) => format_rule_violation_log(&rule_key, entries, Some(config)),
            None => format_rule_violation_log(&rule_key, &[], Some(config)),
        };
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

fn group_violations_by_rule<'a>(
    violations: &'a [AuditViolation],
) -> BTreeMap<String, Vec<(usize, &'a AuditViolation)>> {
    let mut grouped = BTreeMap::new();
    for (idx, violation) in violations.iter().enumerate() {
        let rule_key = grouped_rule_key(&violation.rule_id);
        grouped
            .entry(rule_key)
            .or_insert_with(Vec::new)
            .push((idx, violation));
    }
    grouped
}

fn ordered_rule_log_keys(grouped: &BTreeMap<String, Vec<(usize, &AuditViolation)>>) -> Vec<String> {
    let mut ordered: Vec<String> = ORDERED_AXIOM_IDS
        .iter()
        .map(|id| (*id).to_string())
        .collect();
    let extras: BTreeSet<String> = grouped
        .keys()
        .filter(|rule_key| !ORDERED_AXIOM_IDS.contains(&rule_key.as_str()))
        .cloned()
        .collect();
    ordered.extend(extras);
    ordered
}

fn format_rule_violation_log(
    rule_key: &str,
    entries: &[(usize, &AuditViolation)],
    config: Option<&AuditConfig>,
) -> String {
    let (p0, p1, p2, p3, p4) = count_entry_priorities(entries);
    let mut out = String::new();
    let _ = writeln!(out, "AESTHETIC CORE  ▸  Rule Violations");
    let _ = writeln!(out, "axiom_id={}", rule_key);
    let _ = writeln!(out, "violations={}", entries.len());
    let _ = writeln!(out, "p0={}", p0);
    let _ = writeln!(out, "p1={}", p1);
    let _ = writeln!(out, "p2={}", p2);
    let _ = writeln!(out, "p3={}", p3);
    let _ = writeln!(out, "p4={}", p4);

    if entries.is_empty() {
        let _ = writeln!(out, "status=clean");
        return out;
    }

    let _ = writeln!(out, "status=violations_detected");
    out.push('\n');

    for (group_idx, (index, violation)) in entries.iter().enumerate() {
        let (priority, level) = classify_priority(&violation.rule_id);
        let _ = writeln!(out, "[violation {}]", group_idx + 1);
        let _ = writeln!(out, "ordinal={}", index + 1);
        let _ = writeln!(out, "priority={}", priority);
        let _ = writeln!(out, "level={}", level);
        let _ = writeln!(out, "rule_id={}", violation.rule_id);
        let _ = writeln!(out, "file_path={}", violation.file_path);
        let _ = writeln!(out, "line={}", violation.line);
        let _ = writeln!(
            out,
            "detected_dna={}",
            empty_as_unknown(&violation.detected_dna)
        );
        let _ = writeln!(
            out,
            "taxonomy_path={}",
            empty_as_unknown(&violation.taxonomy_path)
        );
        let _ = writeln!(out, "message={}", violation.message);
        if let Some(snippet) = &violation.snippet {
            let _ = writeln!(out, "snippet={}", snippet);
        }
        if let Some(quick_fix) = &violation.quick_fix {
            let _ = writeln!(out, "quick_fix={}", quick_fix);
        }
        if let Some(config) = config {
            if let Some(context) = render_violation_context_block(config, violation) {
                let _ = writeln!(out, "context:");
                out.push_str(&context);
            }
            let rule_doc_path =
                runtime_rule_doc_path(&config.hierarchy, &grouped_rule_key(&violation.rule_id));
            let taxonomy_path = runtime_taxonomy_info_path(violation);
            let _ = writeln!(
                out,
                "api_rule_command={} --api {}",
                runtime_binary_display(),
                rule_doc_path
            );
            let _ = writeln!(
                out,
                "api_taxonomy_command={} --api {}",
                runtime_binary_display(),
                taxonomy_path
            );
        }
        if group_idx + 1 < entries.len() {
            out.push('\n');
        }
    }
    out
}

fn resolve_artifact_paths(config: &AuditConfig, output_root: &str) -> GovernanceArtifactPaths {
    let output_root_path = config.resolve_project_path(output_root.trim_end_matches('/'));
    let violations_dir_path = output_root_path.join("violations");
    let terminal_report_path = output_root_path.join("governance_terminal.log");
    let violations_manifest_path = output_root_path.join("violations_manifest.json");
    let shadow_context_path = if config.shadow.output.trim().is_empty() {
        output_root_path.join("shadow_context.sarif.json")
    } else {
        config.resolve_project_path(&config.shadow.output)
    };

    GovernanceArtifactPaths {
        output_root: output_root_path.to_string_lossy().to_string(),
        violations_dir: violations_dir_path.to_string_lossy().to_string(),
        terminal_report_path: terminal_report_path.to_string_lossy().to_string(),
        violations_manifest_path: violations_manifest_path.to_string_lossy().to_string(),
        shadow_context_path: shadow_context_path.to_string_lossy().to_string(),
    }
}

fn render_violation_context_block(
    config: &AuditConfig,
    violation: &AuditViolation,
) -> Option<String> {
    if violation.line == 0 {
        return None;
    }

    let source_path = config.resolve_project_path(&violation.file_path);
    let raw = fs::read_to_string(&source_path).ok()?;
    let lines: Vec<&str> = raw.lines().collect();
    if lines.is_empty() {
        return None;
    }

    let line_idx = violation.line.saturating_sub(1);
    if line_idx >= lines.len() {
        return None;
    }

    let start = line_idx.saturating_sub(CONTEXT_RADIUS);
    let end = (line_idx + CONTEXT_RADIUS).min(lines.len().saturating_sub(1));
    let width = (end + 1).to_string().len().max(2);

    let mut out = String::new();
    for current in start..=end {
        let marker = if current == line_idx { '>' } else { ' ' };
        let _ = writeln!(
            out,
            " {} {:>width$} | {}",
            marker,
            current + 1,
            lines[current],
            width = width
        );
        if current == line_idx {
            let pointer_width = lines[current]
                .chars()
                .take_while(|ch| ch.is_whitespace())
                .count()
                + 1;
            let _ = writeln!(
                out,
                "   {:>width$} | {:>pointer_width$}^ {}",
                "",
                "",
                violation.message,
                width = width,
                pointer_width = pointer_width
            );
        }
    }

    Some(out)
}

fn grouped_rule_key(rule_id: &str) -> String {
    for segment in rule_id.split('.') {
        if ORDERED_AXIOM_IDS.contains(&segment) {
            return segment.to_string();
        }
    }
    sanitize_file_name(rule_id)
}

fn rule_log_path(paths: &GovernanceArtifactPaths, rule_key: &str) -> PathBuf {
    let file_name = match ORDERED_AXIOM_IDS
        .iter()
        .position(|axiom_id| *axiom_id == rule_key)
    {
        Some(idx) => format!("{:02}__{}.log", idx + 1, sanitize_file_name(rule_key)),
        None => format!("zz__{}.log", sanitize_file_name(rule_key)),
    };
    Path::new(&paths.violations_dir).join(file_name)
}

fn sanitize_file_name(input: &str) -> String {
    input
        .chars()
        .map(|ch| match ch {
            'a'..='z' | 'A'..='Z' | '0'..='9' => ch,
            _ => '_',
        })
        .collect()
}

fn count_priorities(violations: &[AuditViolation]) -> (usize, usize, usize, usize, usize) {
    let mut counts = (0, 0, 0, 0, 0);
    for violation in violations {
        match classify_priority(&violation.rule_id).0 {
            "P0" => counts.0 += 1,
            "P1" => counts.1 += 1,
            "P2" => counts.2 += 1,
            "P3" => counts.3 += 1,
            _ => counts.4 += 1,
        }
    }
    counts
}

fn count_entry_priorities(
    entries: &[(usize, &AuditViolation)],
) -> (usize, usize, usize, usize, usize) {
    let mut counts = (0, 0, 0, 0, 0);
    for (_, violation) in entries {
        match classify_priority(&violation.rule_id).0 {
            "P0" => counts.0 += 1,
            "P1" => counts.1 += 1,
            "P2" => counts.2 += 1,
            "P3" => counts.3 += 1,
            _ => counts.4 += 1,
        }
    }
    counts
}

fn classify_priority(rule_id: &str) -> (&'static str, &'static str) {
    if rule_id.contains(".super_block.watch.") {
        return ("P1", "warning");
    }
    if rule_id.contains(".super_block.enforced.") {
        return ("P0", "error");
    }
    if rule_id.contains(".sanitization.") || rule_id.contains(".dependency_governance.") {
        return ("P0", "error");
    }
    if rule_id.contains(".identity.recursion.jurisdiction") {
        return ("P1", "warning");
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
    ("P4", "note")
}

fn unique_file_count(paths: &[String]) -> usize {
    let set: BTreeSet<&str> = paths.iter().map(String::as_str).collect();
    set.len()
}

fn unique_sorted_files(paths: &[String]) -> Vec<String> {
    let set: BTreeSet<String> = paths.iter().cloned().collect();
    set.into_iter().collect()
}

fn violation_files_from_violations(violations: &[AuditViolation]) -> Vec<String> {
    let mut files: BTreeSet<String> = BTreeSet::new();
    for violation in violations.iter() {
        if violation.file_path.trim().is_empty() {
            continue;
        }
        files.insert(violation.file_path.clone());
    }
    files.into_iter().collect()
}

fn external_usage_files_from_violations(violations: &[AuditViolation]) -> Vec<String> {
    let mut files: BTreeSet<String> = BTreeSet::new();
    for violation in violations.iter() {
        if !violation.rule_id.contains(".allowed_crates.") {
            continue;
        }
        if !violation.file_path.ends_with(".rs") {
            continue;
        }
        files.insert(violation.file_path.clone());
    }
    files.into_iter().collect()
}

fn empty_as_unknown(value: &str) -> &str {
    if value.is_empty() {
        "<unknown>"
    } else {
        value
    }
}

fn item_separator(idx: usize, len: usize) -> &'static str {
    if idx + 1 < len {
        ","
    } else {
        ""
    }
}

fn option_to_json_string(value: Option<&str>) -> String {
    match value {
        Some(value) => format!("\"{}\"", json_escape(value)),
        None => "null".to_string(),
    }
}

fn write_json_string_array_items(out: &mut String, items: &[String], indent: usize) {
    let pad = " ".repeat(indent);
    for (idx, item) in items.iter().enumerate() {
        let comma = item_separator(idx, items.len());
        let _ = writeln!(out, "{}\"{}\"{}", pad, json_escape(item), comma);
    }
}

fn resolve_numeric_or_default(hierarchy: &crate::structs::HierarchyConfig, key: &str) -> usize {
    hierarchy
        .resolve_parameter(key)
        .and_then(|raw| raw.trim().parse::<usize>().ok())
        .unwrap_or_default()
}

fn resolve_array_or_default(
    hierarchy: &crate::structs::HierarchyConfig,
    key: &str,
    default_values: Vec<String>,
) -> Vec<String> {
    match hierarchy.resolve_parameter(key) {
        Some(raw) => parse_loosely_array_like(&raw).unwrap_or(default_values),
        None => default_values,
    }
}

fn parse_loosely_array_like(raw: &str) -> Option<Vec<String>> {
    let trimmed = raw.trim();
    if !(trimmed.starts_with('[') && trimmed.ends_with(']')) {
        return None;
    }

    let inner = &trimmed[1..trimmed.len() - 1];
    let values: Vec<String> = inner
        .split(',')
        .map(|v| v.trim().trim_matches('"').trim_matches('\''))
        .filter(|v| !v.is_empty())
        .map(|v| v.to_string())
        .collect();

    if values.is_empty() {
        None
    } else {
        Some(values)
    }
}

fn default_shadow_context_lines() -> Vec<String> {
    vec![
        "Each .rs file has a companion *_manifest.json with technical symbols.".to_string(),
        "Read *_manifest.json before editing the source code.".to_string(),
        "Shadow Context includes exact line references for each violation.".to_string(),
        "P0 violations are critical and should be fixed first.".to_string(),
    ]
}

fn default_shadow_tools() -> Vec<String> {
    vec![
        "Shadow Context 2.1.0 — Machine-readable violation report".to_string(),
        "Technical manifests — *_manifest.json per scanned source file".to_string(),
    ]
}

fn default_shadow_terminal_commands(config: &AuditConfig) -> Vec<String> {
    vec![
        "cargo check --target x86_64-unknown-linux-gnu".to_string(),
        "cargo test --target x86_64-unknown-linux-gnu".to_string(),
        runtime_help_command(config),
        runtime_installation_command(config),
    ]
}

fn runtime_help_command(config: &AuditConfig) -> String {
    format!(
        "{} --api {}",
        runtime_binary_display(),
        runtime_help_path(&config.hierarchy)
    )
}

fn runtime_installation_command(config: &AuditConfig) -> String {
    let _ = config;
    format!("{} --installation", runtime_binary_display())
}

fn runtime_feedback_command(taxonomy_path: &str) -> String {
    format!(
        "{} --feedback \"Explain better how to solve this case\" --context {}",
        runtime_binary_display(),
        taxonomy_path
    )
}

fn runtime_feedback_help_command() -> String {
    format!("{} --help.feedback", runtime_binary_display())
}

fn runtime_binary_display() -> String {
    let raw = env::var("ANT_DELIRIUM_BINARY_DISPLAY")
        .ok()
        .filter(|value| !value.trim().is_empty())
        .or_else(|| env::args().next())
        .unwrap_or_else(|| "ac_ant_delirium".to_string());
    shell_quote_command(raw)
}

fn shell_quote_command(raw: String) -> String {
    let normalized = raw.trim().replace('\\', "/");
    if normalized.contains(char::is_whitespace) {
        format!("\"{}\"", normalized.replace('"', "\\\""))
    } else {
        normalized
    }
}

fn runtime_help_path(hierarchy: &crate::structs::HierarchyConfig) -> String {
    for preferred_root in ["hierarchy", "rules", "regras"] {
        if has_semantic_root(hierarchy, preferred_root) {
            return format!("{}.help", preferred_root);
        }
    }

    let mut root_counts: BTreeMap<String, usize> = BTreeMap::new();

    for key in hierarchy.nodes.keys() {
        if key.is_empty() || key.starts_with("runtime.") {
            continue;
        }

        if let Some(root) = key.split('.').next() {
            *root_counts.entry(root.to_string()).or_default() += 1;
        }
    }

    let root = root_counts
        .into_iter()
        .max_by(|(left_root, left_count), (right_root, right_count)| {
            left_count
                .cmp(right_count)
                .then_with(|| right_root.len().cmp(&left_root.len()))
                .then_with(|| right_root.cmp(left_root))
        })
        .map(|(root, _)| root)
        .unwrap_or_else(|| "hierarchy".to_string());

    format!("{}.help", root)
}

fn taxonomy_guidance(taxonomy_path: &str) -> Vec<String> {
    vec![
        format!(
            "Use the taxonomy command to inspect the active project policy for '{}'.",
            taxonomy_path
        ),
        "If the taxonomy explanation is insufficient for the current case, send contextual feedback with --context.".to_string(),
    ]
}

fn rule_guidance(api_rule_path: &str, rule_doc_id: &str) -> Vec<String> {
    vec![
        format!(
            "Use the rule command to inspect the semantic contract behind '{}'.",
            api_rule_path
        ),
        format!(
            "If the remediation remains unclear, append feedback referencing the active taxonomy path for rule '{}'.",
            rule_doc_id
        ),
    ]
}

fn runtime_rule_doc_path(hierarchy: &crate::structs::HierarchyConfig, rule_doc_id: &str) -> String {
    let root = runtime_api_root(hierarchy);
    format!("{}.rule.{}", root, rule_doc_id)
}

fn runtime_api_root(hierarchy: &crate::structs::HierarchyConfig) -> String {
    runtime_help_path(hierarchy)
        .trim_end_matches(".help")
        .to_string()
}

fn runtime_taxonomy_info_path(violation: &AuditViolation) -> String {
    if !violation.taxonomy_path.trim().is_empty() {
        violation.taxonomy_path.clone()
    } else {
        "hierarchy".to_string()
    }
}

fn remediation_kind(rule_doc_id: &str) -> &'static str {
    match rule_doc_id {
        "doc_tags"
        | "md_pairing"
        | "md_min_chars"
        | "what_mission"
        | "why_justification"
        | "usage_example"
        | "test_case_mapping"
        | "root_md_pairing"
        | "inheritance_visibility" => "rewrite_documentation",
        "max_nesting" | "logical_density" | "max_params" | "else_free" => "restructure_function",
        "type_logic" => "align_declaration_dna",
        "dna_suffix" | "tag_separator" | "pascal_case" | "name_separator" | "repository_anchor"
        | "manifest_anchoring" | "recursion" => "align_file_identity",
        "restricted_tokens" | "token_sanitization" => "sanitize_tokens",
        "allowed_crates" | "dependency_purification" | "strict_wrapper_boundary" => "align_dependencies",
        _ => "inspect_rule_contract",
    }
}

fn has_semantic_root(hierarchy: &crate::structs::HierarchyConfig, root: &str) -> bool {
    [
        "structural",
        "identity",
        "pedagogical",
        "sanitization",
        "dependency_governance",
    ]
    .iter()
    .any(|branch| {
        hierarchy
            .nodes
            .contains_key(&format!("{}.{}", root, branch))
    })
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
