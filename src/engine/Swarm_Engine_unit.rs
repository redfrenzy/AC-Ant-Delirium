use crate::parser::CodeScanner;
use crate::rules::axiom_sanitization::Super_Block_Axiom_unit::Super_Block_Axiom_unitcore;
use crate::rules::get_full_axioms;
use crate::structs::{AuditConfig, AuditContext, AuditResult, AuditViolation, SearchFiles};
use std::collections::BTreeMap;
use std::env;
use std::time::Instant;

#[path = "Swarm_Engine_unitcore.rs"]
mod Swarm_Engine_unitcore;

pub struct SwarmEngine;

impl SwarmEngine {
    /// Dispara a patrulha massiva e devolve um resultado estruturado para o sistema de logs.
    pub fn audit(config: &AuditConfig) -> AuditResult {
        let profile_enabled = env::var("ANT_DELIRIUM_PROFILE")
            .map(|value| {
                let value = value.trim();
                value == "1"
                    || value.eq_ignore_ascii_case("true")
                    || value.eq_ignore_ascii_case("yes")
            })
            .unwrap_or(false);
        let audit_start = Instant::now();
        let axioms = get_full_axioms();
        let mut per_axiom_counts: BTreeMap<String, usize> = BTreeMap::new();
        let mut per_axiom_millis: BTreeMap<String, u128> = BTreeMap::new();
        let mut config_preflight = Self::validate_rule_priority_conflicts(config);
        let preflight = Super_Block_Axiom_unitcore::run_preflight(config);
        let preflight_enforced = preflight
            .violations
            .iter()
            .filter(|violation| violation.rule_id.contains(".super_block.enforced."))
            .count();
        let preflight_watch = preflight
            .violations
            .iter()
            .filter(|violation| violation.rule_id.contains(".super_block.watch."))
            .count();
        let jurisdiction_start = Instant::now();
        let mut jurisdiction =
            SearchFiles::resolve_jurisdiction(&config.hierarchy, SearchFiles::ID);
        let resolve_jurisdiction_millis = jurisdiction_start.elapsed().as_millis();
        jurisdiction.sort();
        jurisdiction.dedup();
        let jurisdiction_len = jurisdiction.len();
        let mut logs = vec![format!(
            "[SWARM] Iniciando patrulha massiva com {} axiomas.",
            axioms.len()
        )];
        if !preflight.violations.is_empty() {
            logs.push(format!(
                "[SWARM][super_block] preflight encontrou {} enforced violation(s) e {} watch violation(s).",
                preflight_enforced, preflight_watch
            ));
        }
        let has_config_priority_conflict = !config_preflight.is_empty();
        if preflight.abort_scan || has_config_priority_conflict {
            logs.push(
                "[SWARM] Preflight abortado antes da patrulha de arquivos por violacao critica de configuracao."
                    .to_string(),
            );
            if has_config_priority_conflict {
                logs.push(
                    "[SWARM] Preflight abortado por conflito de prioridade em default_rules.toml."
                        .to_string(),
                );
            }
            config_preflight.extend(preflight.violations);
            return AuditResult {
                violations: config_preflight,
                logs,
            };
        }
        let mut all_violations = Vec::new();
        all_violations.extend(config_preflight);
        all_violations.extend(preflight.violations);
        let tag_separator = Self::resolve_tag_separator(config);
        let prepared_project =
            Swarm_Engine_unitcore::prepare_project_context(config, &jurisdiction, &tag_separator);
        let mut scan_blocks_millis = 0u128;

        for file in prepared_project.context.files.iter() {
            let scan_start = Instant::now();
            let functions = CodeScanner::scan_blocks(&file.content);
            scan_blocks_millis += scan_start.elapsed().as_millis();

            for axiom in axioms.iter() {
                let axiom_start = Instant::now();
                let start_len = all_violations.len();
                {
                    let mut ctx = AuditContext {
                        config,
                        project: &prepared_project.context,
                        violations: &mut all_violations,
                        file_path: &file.path,
                        source: &file.content,
                        functions: &functions,
                        detected_dna: file.detected_dna.clone(),
                        taxonomy_path: file.taxonomy_path.clone(),
                    };
                    axiom.execute(&mut ctx);
                }
                let delta = all_violations.len().saturating_sub(start_len);
                let elapsed = axiom_start.elapsed().as_millis();
                *per_axiom_millis.entry(axiom.id().to_string()).or_default() += elapsed;
                if delta > 0 {
                    *per_axiom_counts.entry(axiom.id().to_string()).or_default() += delta;
                }
            }
        }

        for axiom in axioms.iter() {
            let count = per_axiom_counts.get(axiom.id()).copied().unwrap_or(0);
            if count > 0 {
                logs.push(format!(
                    "[SWARM][{}] {} violation(s) detectada(s).",
                    axiom.id(),
                    count
                ));
            }
        }

        logs.push(format!(
            "[SWARM] Patrulha encerrada com {} violation(s).",
            all_violations.len()
        ));

        if profile_enabled {
            logs.push(format!(
                "[PROFILE] jurisdiction_files={} prepared_files={} known_paths={} resolve_jurisdiction={}ms project_index={}ms file_read={}ms scan_blocks={}ms taxonomy={}ms total={}ms",
                jurisdiction_len,
                prepared_project.context.loaded_file_count(),
                prepared_project.context.known_path_count(),
                resolve_jurisdiction_millis,
                prepared_project.path_index_millis,
                prepared_project.file_read_millis,
                scan_blocks_millis,
                prepared_project.taxonomy_millis,
                audit_start.elapsed().as_millis()
            ));

            for axiom in axioms.iter() {
                let elapsed = per_axiom_millis.get(axiom.id()).copied().unwrap_or(0);
                let count = per_axiom_counts.get(axiom.id()).copied().unwrap_or(0);
                logs.push(format!(
                    "[PROFILE][{}] time={}ms violations={}",
                    axiom.id(),
                    elapsed,
                    count
                ));
            }
        }

        AuditResult {
            violations: all_violations,
            logs,
        }
    }

    /// Dispara a patrulha massiva preservando compatibilidade com o fluxo antigo.
    pub fn patrol(config: &AuditConfig) -> Vec<AuditViolation> {
        Self::audit(config).violations
    }

    /// [WHAT] Resolve o DNA detectado e o caminho taxonomico base para um arquivo auditado.
    pub fn resolve_file_taxonomy(config: &AuditConfig, file_path: &str) -> (String, String) {
        let tag_separator = Self::resolve_tag_separator(config);
        Swarm_Engine_unitcore::resolve_file_taxonomy(config, file_path, &tag_separator)
    }

    /// [WHAT] Resolve explicitamente o separador de tags utilizado pelo orquestrador.
    pub fn resolve_tag_separator(config: &AuditConfig) -> String {
        config
            .hierarchy
            .resolve_parameter("tag_separator")
            .unwrap_or_else(|| "_".to_string())
    }

    fn validate_rule_priority_conflicts(config: &AuditConfig) -> Vec<AuditViolation> {
        let mut by_priority: BTreeMap<String, Vec<String>> = BTreeMap::new();

        for (path, node) in config.hierarchy.nodes.iter() {
            let is_rule_manifest_node = node
                .properties
                .keys()
                .any(|key| key.starts_with("requirement::"));
            if !is_rule_manifest_node {
                continue;
            }

            let Some(raw_priority) = node.properties.get("priority") else {
                continue;
            };
            let Ok(value) = raw_priority.parse::<f64>() else {
                continue;
            };
            let normalized = format!("{value:.4}");
            by_priority
                .entry(normalized)
                .or_default()
                .push(path.to_string());
        }

        let mut violations = Vec::new();
        for (priority, mut rules) in by_priority {
            if rules.len() < 2 {
                continue;
            }
            rules.sort();
            let joined = rules.join(", ");
            violations.push(AuditViolation {
                file_path: "ant_delirium_config/default_rules.toml".to_string(),
                line: 1,
                rule_id: "hierarchy.sanitization.rule_priority_conflict".to_string(),
                message: format!(
                    "Priority conflict detected in default_rules.toml: {} rules share priority '{}': {}. The engine requires unique rule priorities to produce a deterministic fix order (P0.xxxx/P1.xxxx). Open the TOML and assign a different priority to each listed rule according to your desired correction order.",
                    rules.len(),
                    priority,
                    joined
                ),
                snippet: Some(format!("priority = {}", priority)),
                quick_fix: Some(
                    "// [CONFIG] Edit default_rules.toml and choose unique `priority` values for each conflicting rule."
                        .to_string(),
                ),
                detected_dna: "rules".to_string(),
                taxonomy_path: "hierarchy.sanitization.rule_priority_conflict".to_string(),
            });
        }

        violations
    }
}
