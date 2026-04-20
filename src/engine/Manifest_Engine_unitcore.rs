use std::collections::HashMap;
use std::fs;
use std::path::{Path, PathBuf};

use crate::engine::Swarm_Engine_unit::SwarmEngine;
use crate::parser::{clean_line, CodeScanner};
use crate::structs::AuditConfig;

#[derive(Clone)]
struct ManifestEntry {
    name: String,
    kind: String,
    start_line: usize,
    end_line: usize,
    complexity: usize,
    narrative: String,
    source_file: String,
}

pub fn generate_file_manifests(config: &AuditConfig, files: &[String]) -> std::io::Result<Vec<String>> {
    let tag_separator = SwarmEngine::resolve_tag_separator(config);
    let mut grouped: HashMap<PathBuf, Vec<ManifestEntry>> = HashMap::new();

    for file_path in files {
        if !file_path.ends_with(".rs") {
            continue;
        }

        let source = match fs::read_to_string(file_path) {
            Ok(content) => content,
            Err(_) => continue,
        };

        let (detected_dna, taxonomy_path) = SwarmEngine::resolve_file_taxonomy(config, file_path);
        let manifest_anchor = resolve_manifest_anchor_path(config, &taxonomy_path);
        if manifest_anchor.is_none() {
            continue;
        }

        let blocks = CodeScanner::scan_blocks(&source);
        if blocks.is_empty() {
            continue;
        }

        let manifest_path = build_manifest_path(
            file_path,
            &detected_dna,
            manifest_anchor.as_deref(),
            &tag_separator,
        );

        let source_name = Path::new(file_path)
            .file_name()
            .and_then(|v| v.to_str())
            .unwrap_or(file_path)
            .to_string();

        let entries: Vec<ManifestEntry> = blocks
            .into_iter()
            .filter(|block| block.kind != "_module")
            .map(|block| ManifestEntry {
                name: block.name,
                kind: normalize_kind(&block.kind).to_string(),
                start_line: block.start_line,
                end_line: block.end_line,
                complexity: compute_complexity(block.body),
                narrative: block.docstring,
                source_file: source_name.clone(),
            })
            .collect();

        if entries.is_empty() {
            continue;
        }

        grouped.entry(manifest_path).or_default().extend(entries);
    }

    let mut written_paths = Vec::new();
    for (manifest_path, mut entries) in grouped {
        entries.sort_by(|left, right| {
            left.source_file
                .cmp(&right.source_file)
                .then_with(|| left.start_line.cmp(&right.start_line))
                .then_with(|| left.name.cmp(&right.name))
        });

        let payload = serialize_manifest_entries(&entries);
        if let Some(parent) = manifest_path.parent() {
            let _ = fs::create_dir_all(parent);
        }
        fs::write(&manifest_path, payload)?;
        written_paths.push(manifest_path.to_string_lossy().to_string());
    }

    written_paths.sort();
    Ok(written_paths)
}

fn resolve_manifest_anchor_path(config: &AuditConfig, taxonomy_path: &str) -> Option<String> {
    let mut current = taxonomy_path.trim().trim_matches('.').to_string();
    if current.is_empty() {
        return None;
    }

    loop {
        let manifest_key = format!("{}.manifest", current);
        if let Some(value) = config.hierarchy.search_params(&manifest_key) {
            if parse_bool_like(&value) {
                return Some(current);
            }
        }

        match current.rsplit_once('.') {
            Some((parent, _)) => current = parent.to_string(),
            None => break,
        }
    }

    None
}

fn build_manifest_path(
    file_path: &str,
    detected_dna: &str,
    anchor_path: Option<&str>,
    tag_separator: &str,
) -> PathBuf {
    let path = Path::new(file_path);
    let parent = path.parent().unwrap_or_else(|| Path::new("."));
    let stem = path.file_stem().and_then(|value| value.to_str()).unwrap_or_default();

    let manifest_stem = if detected_dna.is_empty() {
        stem.to_string()
    } else {
        let anchor_dna = anchor_path
            .and_then(|value| value.rsplit('.').next())
            .unwrap_or(detected_dna);
        let core = strip_detected_suffix(stem, detected_dna, tag_separator);

        if anchor_dna == detected_dna {
            stem.to_string()
        } else {
            format!("{}{}{}", core, tag_separator, anchor_dna)
        }
    };

    parent.join(format!("{}_manifest.json", manifest_stem))
}

fn strip_detected_suffix<'a>(stem: &'a str, detected_dna: &str, tag_separator: &str) -> &'a str {
    match stem.rsplit_once(tag_separator) {
        Some((base, suffix)) if suffix == detected_dna => base,
        _ => stem,
    }
}

fn parse_bool_like(value: &str) -> bool {
    match value.trim().to_ascii_lowercase().as_str() {
        "false" | "0" | "off" | "no" => false,
        _ => true,
    }
}

fn normalize_kind(kind: &str) -> &str {
    match kind {
        "function" => "function",
        "struct" => "struct",
        "enum" => "enum",
        "trait" => "trait",
        "macro" => "macro",
        "impl" => "impl",
        _ => "unknown",
    }
}

fn compute_complexity(body: &str) -> usize {
    let mut count = 0;
    for line in body.lines() {
        let cleaned = clean_line(line);
        let trimmed = cleaned.trim();
        if trimmed.is_empty() {
            continue;
        }
        count += 1;
    }
    count
}

fn serialize_manifest_entries(entries: &[ManifestEntry]) -> String {
    let mut out = String::new();
    out.push_str("[\n");

    for (idx, entry) in entries.iter().enumerate() {
        let comma = if idx + 1 == entries.len() { "" } else { "," };
        out.push_str("  { ");
        out.push_str(&format!(
            "\"source\": \"{}\", \"name\": \"{}\", \"kind\": \"{}\", \"range\": [{}, {}], \"complexity\": {}, \"narrative\": \"{}\" }}{}",
            json_escape(&entry.source_file),
            json_escape(&entry.name),
            json_escape(&entry.kind),
            entry.start_line,
            entry.end_line,
            entry.complexity,
            json_escape(&entry.narrative),
            comma,
        ));
        out.push('\n');
    }

    out.push_str("]\n");
    out
}

fn json_escape(value: &str) -> String {
    value
        .replace('\\', "\\\\")
        .replace('\"', "\\\"")
        .replace('\n', "\\n")
        .replace('\r', "\\r")
        .replace('\t', "\\t")
}
