use crate::AuditViolation;
use crate::config::AuditConfig;

/// [WHAT] Verifies the use of restricted and poison tokens in the source code.
/// [WHY] To prevent unhandled panic paths and ensure secure code patterns.
/// [WHERE] Called during the structural and safety audit phase of the rule engine.
pub fn verify(file_path: &str, source: &str, config: &AuditConfig, violations: &mut Vec<AuditViolation>) {
    for (idx, line) in source.lines().enumerate() {
        let trimmed = line.trim();
        process_line_tokens(file_path, idx, trimmed, config, violations);
    }
}

/// [WHAT] Analyzes a single line for restricted tokens.
/// [WHY] To keep 'verify' flat and avoid else blocks.
/// [WHERE] Internal utility for token-based safety auditing.
fn process_line_tokens(file_path: &str, idx: usize, trimmed: &str, config: &AuditConfig, violations: &mut Vec<AuditViolation>) {
    // Skip comments
    if trimmed.starts_with("//") || trimmed.starts_with("/*") { return; }

    for (token, hint) in &config.rules.restricted_tokens {
        check_poison_token(file_path, idx, trimmed, token, hint, config, violations);
    }
}

/// [WHAT] Individual check for a specific poison token.
/// [WHY] Encapsulation of token-based safety rules.
/// [WHERE] Internal utility for token-based safety auditing.
fn check_poison_token(file_path: &str, idx: usize, trimmed: &str, token: &str, hint: &str, config: &AuditConfig, violations: &mut Vec<AuditViolation>) {
    if !trimmed.contains(token) { return; }
    
    // Explicit bypass mechanism (Manual bypass via code comments)
    if trimmed.contains("[Safety Required]") || trimmed.contains("[TEMP_ALLOWED]") { return; }

    // Industrial Debug Liberation Phase (ELITE_TRACE support)
    if config.debug.enabled && (token.contains("print") || token.contains("log")) {
        if config.debug.allow_list.iter().any(|p| file_path.contains(p)) {
            return;
        }
    }

    // Protocol Safe Harbor: Restricted Token Isolation
    let is_in_quarantine = file_path.contains("src/restricted_tokens/");
    let mut message = format!("Poison Token: '{}' found. Hint: {}", token, hint);
    let mut quick_fix = Some(format!("// [SAFETY] Isolate this logic in 'src/restricted_tokens/{}.rs' and authorize in delirium.toml.", token.replace("!", "")));

    if is_in_quarantine {
        message = format!("Illegal Logic: Token '{}' found in quarantine but MISSING [ignore_to] authorization in delirium.toml.", token);
        quick_fix = Some(format!("# Add this to delirium.toml:\n# [ignore_to]\n# \"{}\" = [\"rule_poison_tokens\"]", file_path));
    }

    violations.push(AuditViolation {
        line: idx + 1,
        rule_id: "rule_poison_tokens",
        message,
        snippet: Some(trimmed.to_string()),
        quick_fix,
    });
}
