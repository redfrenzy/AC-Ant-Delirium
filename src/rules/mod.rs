#[allow(non_snake_case)]
pub mod Rule_Docs_unit;
pub mod axiom_identity;
pub mod axiom_pedagogical;
pub mod axiom_sanitization;
pub mod axiom_structural;

use crate::structs::{AuditConfig, AuditViolation, GovernanceAxiom, ScannedBlock};

// [CATALOG DISCOVERY]
/// Coleta todos os Axiomas de Governança ativos para execução massiva.
pub fn get_full_axioms() -> Vec<Box<dyn GovernanceAxiom>> {
    let mut catalog: Vec<Box<dyn GovernanceAxiom>> = Vec::new();

    // [STRUCTURAL]
    catalog.push(Box::new(
        axiom_structural::Max_Nesting_Axiom_unit::MaxNestingAxiom::default(),
    ));
    catalog.push(Box::new(
        axiom_structural::Max_Params_Axiom_unit::MaxParamsAxiom::default(),
    ));
    catalog.push(Box::new(
        axiom_structural::Logical_Density_Axiom_unit::LogicalDensityAxiom::default(),
    ));
    catalog.push(Box::new(
        axiom_structural::Type_Logic_Axiom_unit::TypeLogicAxiom::default(),
    ));
    catalog.push(Box::new(
        axiom_structural::Else_Free_Axiom_unit::ElseFreeAxiom::default(),
    ));

    // [PEDAGOGICAL]
    catalog.push(Box::new(
        axiom_pedagogical::Doc_Tags_Axiom_unit::DocTagsAxiom::default(),
    ));
    catalog.push(Box::new(
        axiom_pedagogical::MD_Pairing_Axiom_unit::MdPairingAxiom::default(),
    ));
    catalog.push(Box::new(
        axiom_pedagogical::MD_Min_Chars_Axiom_unit::MdMinCharsAxiom::default(),
    ));
    catalog.push(Box::new(
        axiom_pedagogical::What_Mission_Axiom_unit::WhatMissionAxiom::default(),
    ));
    catalog.push(Box::new(
        axiom_pedagogical::Why_Justification_Axiom_unit::WhyJustificationAxiom::default(),
    ));
    catalog.push(Box::new(
        axiom_pedagogical::Usage_Example_Axiom_unit::UsageExampleAxiom::default(),
    ));
    catalog.push(Box::new(
        axiom_pedagogical::Test_Case_Mapping_Axiom_unit::TestCaseMappingAxiom::default(),
    ));
    catalog.push(Box::new(
        axiom_pedagogical::Root_MD_Pairing_Axiom_unit::RootMdPairingAxiom::default(),
    ));
    catalog.push(Box::new(
        axiom_pedagogical::Inheritance_Visibility_Axiom_unit::InheritanceVisibilityAxiom::default(),
    ));

    // [IDENTITY]
    catalog.push(Box::new(
        axiom_identity::DNA_Suffix_Axiom_unit::DnaSuffixAxiom::default(),
    ));
    catalog.push(Box::new(
        axiom_identity::Tag_Separator_Axiom_unit::TagSeparatorAxiom::default(),
    ));
    catalog.push(Box::new(
        axiom_identity::Pascal_Case_Axiom_unit::PascalCaseAxiom::default(),
    ));
    catalog.push(Box::new(
        axiom_identity::Recursion_Axiom_unit::RecursionAxiom::default(),
    ));
    catalog.push(Box::new(
        axiom_identity::Manifest_Anchoring_Axiom_unit::ManifestAnchoringAxiom::default(),
    ));
    catalog.push(Box::new(
        axiom_identity::Name_Separator_Axiom_unit::NameSeparatorAxiom::default(),
    ));
    catalog.push(Box::new(
        axiom_identity::Repository_Anchor_Axiom_unit::RepositoryAnchorAxiom::default(),
    ));

    // [SANITIZATION]
    catalog.push(Box::new(
        axiom_sanitization::Restricted_Tokens_Axiom_unit::RestrictedTokensAxiom::default(),
    ));
    catalog.push(Box::new(
        axiom_sanitization::Super_Block_Axiom_unit::SuperBlockAxiom::default(),
    ));
    catalog.push(Box::new(
        axiom_sanitization::Allowed_Crates_Axiom_unit::AllowedCratesAxiom::default(),
    ));
    catalog.push(Box::new(
        axiom_sanitization::Token_Sanitization_Axiom_unit::TokenSanitizationAxiom::default(),
    ));
    catalog.push(Box::new(axiom_sanitization::Dependency_Purification_Axiom_unit::DependencyPurificationAxiom::default()));

    catalog
}

/// [DEPRECATED] Mantido apenas para compatibilidade temporária durante a transição.
pub fn get_full_catalog() -> Vec<Box<dyn crate::structs::AuditRule>> {
    Vec::new()
}

/// [AESTHETIC CORE - DIAGNOSTIC ORCHESTRATOR]
pub fn run_diagnostics(
    _config: &AuditConfig,
    _file_path: &str,
    _source: &str,
    _functions: &[ScannedBlock<'_>],
    _violations: &mut Vec<AuditViolation>,
    _detected_dna: String,
    _taxonomy_path: String,
) {
    // [TODO] Migrar para executar os Axiomas em vez das regras legadas
}
