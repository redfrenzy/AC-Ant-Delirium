#[cfg(test)]
mod tests {
    use crate::parser::CodeScanner;
    use crate::rules::axiom_structural::Logical_Density_Axiom_unit::LogicalDensityAxiom;
    use crate::structs::{AuditConfig, AuditContext, GovernanceAxiom, HierarchyConfig};

    #[test]
    fn test_logical_density_axiom_resolves_taxonomy_limit_and_reports_violation() {
        let hierarchy = HierarchyConfig::load_from_folder("ant_delirium_config/type_1");
        let config = AuditConfig {
            hierarchy,
            ..AuditConfig::default()
        };
        let source = r#"
fn overloaded_unitcore() {
    a();
    b();
    c();
    d();
    e();
    f();
    g();
    h();
    i();
    j();
    k();
    l();
    m();
    n();
    o();
    p();
    q();
    r();
    s();
    t();
    u();
    v();
    w();
    x();
    y();
    z();
    aa();
    ab();
    ac();
    ad();
    ae();
    af();
    ag();
    ah();
    ai();
    aj();
    ak();
    al();
    am();
    an();
}
"#;
        let functions = CodeScanner::scan_blocks(source);
        let mut violations = Vec::new();
        let mut ctx = AuditContext {
            file_path: "src/rules/axiom_structural/Logical_Density_Axiom_unitcore.rs",
            source,
            functions: &functions,
            config: &config,
            project: crate::structs::empty_project_context(),
            detected_dna: "unitcore".to_string(),
            taxonomy_path: "hierarchy.files.unit.unitcore".to_string(),
            violations: &mut violations,
        };

        LogicalDensityAxiom::default().execute(&mut ctx);

        assert_eq!(ctx.violations.len(), 1);
        assert_eq!(
            ctx.violations[0].taxonomy_path,
            "hierarchy.files.unit.unitcore.logical_density"
        );
        assert!(
            ctx.violations[0].message.contains("40/40")
                || ctx.violations[0].message.contains("41/40")
                || ctx.violations[0].message.contains("/40")
        );
    }

    #[test]
    fn test_logical_density_axiom_uses_parent_fallback() {
        let hierarchy = HierarchyConfig::load_from_folder("ant_delirium_config/type_1");
        let config = AuditConfig {
            hierarchy,
            ..AuditConfig::default()
        };
        let source = r#"
fn structcore_heavy() {
    a();
    b();
    c();
    d();
    e();
    f();
    g();
    h();
    i();
    j();
    k();
    l();
    m();
    n();
    o();
    p();
    q();
    r();
    s();
    t();
    u();
    v();
    w();
    x();
    y();
    z();
    aa();
    ab();
    ac();
    ad();
    ae();
}
"#;
        let functions = CodeScanner::scan_blocks(source);
        let mut violations = Vec::new();
        let mut ctx = AuditContext {
            file_path: "src/struct/Example_structcore.rs",
            source,
            functions: &functions,
            config: &config,
            project: crate::structs::empty_project_context(),
            detected_dna: "structcore".to_string(),
            taxonomy_path: "hierarchy.files.struct.structcore".to_string(),
            violations: &mut violations,
        };

        LogicalDensityAxiom::default().execute(&mut ctx);

        assert_eq!(ctx.violations.len(), 1);
        assert_eq!(
            ctx.violations[0].taxonomy_path,
            "hierarchy.files.struct.structcore.logical_density"
        );
    }
}
