use crate::structs::AuditContext;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AxiomParameterResolution {
    pub axiom_id: String,
    pub identity_path: String,
    pub requirement_id: String,
    pub resolved_path: String,
    pub value: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AxiomParameterScope {
    Global,
    Taxonomy,
    TaxonomyThenGlobal,
    GlobalThenTaxonomy,
}

impl<'a> AuditContext<'a> {
    /// [WHAT] Resolve o nó-identidade único de um axioma dentro da taxonomia ativa.
    pub fn resolve_axiom_identity_path(&self, axiom_id: &str) -> Result<String, String> {
        let mut paths = self.config.hierarchy.find_paths_by_id(axiom_id);
        if paths.is_empty() {
            return Err(format!(
                "Axiom '{}' nao encontrado na taxonomia ativa. Defina sua identidade no manifesto de rules.",
                axiom_id
            ));
        }

        paths.sort_by_key(|path| path.matches('.').count());
        let shallowest_depth = paths[0].matches('.').count();
        let shallowest: Vec<String> = paths.into_iter()
            .filter(|path| path.matches('.').count() == shallowest_depth)
            .collect();

        match shallowest.len() {
            1 => Ok(shallowest[0].clone()),
            _ => Err(format!(
                "Axiom '{}' possui identidade ambigua: {}. Deixe apenas um ramo-identidade ativo para este axioma.",
                axiom_id,
                shallowest.join(" | ")
            )),
        }
    }

    /// [WHAT] Resolve todos os requirements declarados para um axioma.
    pub fn resolve_axiom_requirements(&self, axiom_id: &str) -> Result<Vec<String>, String> {
        let identity_path = self.resolve_axiom_identity_path(axiom_id)?;
        let requirements = self.config.hierarchy.find_requirements(&identity_path);

        if requirements.is_empty() {
            return Err(format!(
                "Axiom '{}' nao declarou requirements em '{}'. Defina ao menos um requirement no manifesto de rules.",
                axiom_id, identity_path
            ));
        }

        Ok(requirements)
    }

    /// [WHAT] Resolve um requirement obrigatorio de um axioma para seu valor concreto.
    pub fn require_axiom_parameter(
        &self,
        axiom_id: &str,
        requirement_id: &str,
    ) -> Result<AxiomParameterResolution, String> {
        let identity_path = self.resolve_axiom_identity_path(axiom_id)?;
        let requirements = self.resolve_axiom_requirements(axiom_id)?;

        if !requirements.iter().any(|requirement| requirement == requirement_id) {
            return Err(format!(
                "Axiom '{}' nao declarou o requirement '{}'. Requirements ativos: {}.",
                axiom_id,
                requirement_id,
                requirements.join(", ")
            ));
        }

        let (resolved_path, value) = self.config.hierarchy.resolve_parameter_binding(requirement_id)
            .ok_or_else(|| {
                format!(
                    "Axiom '{}' precisa do requirement '{}', mas nenhum valor concreto foi encontrado. A busca partiu de '{}'.",
                    axiom_id, requirement_id, identity_path
                )
            })?;

        Ok(AxiomParameterResolution {
            axiom_id: axiom_id.to_string(),
            identity_path,
            requirement_id: requirement_id.to_string(),
            resolved_path,
            value,
        })
    }

    /// [WHAT] Resolve o único requirement declarado pelo axioma atual.
    pub fn resolve_single_axiom_requirement(&self, axiom_id: &str) -> Result<(String, String), String> {
        let identity_path = self.resolve_axiom_identity_path(axiom_id)?;
        let requirements = self.resolve_axiom_requirements(axiom_id)?;

        match requirements.len() {
            1 => Ok((identity_path, requirements[0].clone())),
            0 => Err(format!(
                "Axiom '{}' nao declarou requirements em '{}'.",
                axiom_id, identity_path
            )),
            _ => Err(format!(
                "Axiom '{}' declarou requirements multiplos em '{}': {}. Use resolucao explicita ou reduza para um requirement.",
                axiom_id,
                identity_path,
                requirements.join(", ")
            )),
        }
    }

    /// [WHAT] Resolve um unico requirement do axioma conforme o escopo de busca solicitado.
    pub fn require_axiom_parameter_with_scope(
        &self,
        axiom_id: &str,
        scope: AxiomParameterScope,
    ) -> Result<AxiomParameterResolution, String> {
        let (identity_path, requirement_id) = self.resolve_single_axiom_requirement(axiom_id)?;
        self.require_named_axiom_parameter_with_scope_from_identity(
            axiom_id,
            identity_path,
            requirement_id,
            scope,
        )
    }

    pub fn require_named_axiom_parameter_with_scope(
        &self,
        axiom_id: &str,
        requirement_id: &str,
        scope: AxiomParameterScope,
    ) -> Result<AxiomParameterResolution, String> {
        let identity_path = self.resolve_axiom_identity_path(axiom_id)?;
        let requirements = self.resolve_axiom_requirements(axiom_id)?;

        if !requirements.iter().any(|requirement| requirement == requirement_id) {
            return Err(format!(
                "Axiom '{}' nao declarou o requirement '{}'. Requirements ativos: {}.",
                axiom_id,
                requirement_id,
                requirements.join(", ")
            ));
        }

        self.require_named_axiom_parameter_with_scope_from_identity(
            axiom_id,
            identity_path,
            requirement_id.to_string(),
            scope,
        )
    }

    fn require_named_axiom_parameter_with_scope_from_identity(
        &self,
        axiom_id: &str,
        identity_path: String,
        requirement_id: String,
        scope: AxiomParameterScope,
    ) -> Result<AxiomParameterResolution, String> {
        let resolved = match scope {
            AxiomParameterScope::Global => self.resolve_global_requirement(axiom_id, &identity_path, &requirement_id),
            AxiomParameterScope::Taxonomy => self.resolve_taxonomy_requirement(axiom_id, &identity_path, &requirement_id),
            AxiomParameterScope::TaxonomyThenGlobal => self.resolve_taxonomy_requirement(axiom_id, &identity_path, &requirement_id)
                .or_else(|_| self.resolve_global_requirement(axiom_id, &identity_path, &requirement_id)),
            AxiomParameterScope::GlobalThenTaxonomy => self.resolve_global_requirement(axiom_id, &identity_path, &requirement_id)
                .or_else(|_| self.resolve_taxonomy_requirement(axiom_id, &identity_path, &requirement_id)),
        }?;

        Ok(AxiomParameterResolution {
            axiom_id: axiom_id.to_string(),
            identity_path,
            requirement_id,
            resolved_path: resolved.0,
            value: resolved.1,
        })
    }

    pub fn require_global_axiom_parameter(&self, axiom_id: &str) -> Result<AxiomParameterResolution, String> {
        self.require_axiom_parameter_with_scope(axiom_id, AxiomParameterScope::Global)
    }

    pub fn require_named_global_axiom_parameter(
        &self,
        axiom_id: &str,
        requirement_id: &str,
    ) -> Result<AxiomParameterResolution, String> {
        self.require_named_axiom_parameter_with_scope(axiom_id, requirement_id, AxiomParameterScope::Global)
    }

    pub fn require_taxonomy_axiom_parameter(&self, axiom_id: &str) -> Result<AxiomParameterResolution, String> {
        self.require_axiom_parameter_with_scope(axiom_id, AxiomParameterScope::Taxonomy)
    }

    pub fn require_named_taxonomy_axiom_parameter(
        &self,
        axiom_id: &str,
        requirement_id: &str,
    ) -> Result<AxiomParameterResolution, String> {
        self.require_named_axiom_parameter_with_scope(axiom_id, requirement_id, AxiomParameterScope::Taxonomy)
    }

    pub fn require_hybrid_axiom_parameter(&self, axiom_id: &str) -> Result<AxiomParameterResolution, String> {
        self.require_axiom_parameter_with_scope(axiom_id, AxiomParameterScope::TaxonomyThenGlobal)
    }

    pub fn require_named_hybrid_axiom_parameter(
        &self,
        axiom_id: &str,
        requirement_id: &str,
    ) -> Result<AxiomParameterResolution, String> {
        self.require_named_axiom_parameter_with_scope(axiom_id, requirement_id, AxiomParameterScope::TaxonomyThenGlobal)
    }

    fn resolve_global_requirement(
        &self,
        axiom_id: &str,
        identity_path: &str,
        requirement_id: &str,
    ) -> Result<(String, String), String> {
        let local_requirement_path = format!("{}.{}", identity_path, requirement_id);
        if let Some(value) = self.config.hierarchy.search_params(&local_requirement_path) {
            return Ok((local_requirement_path, value));
        }

        self.config.hierarchy.resolve_parameter_binding(requirement_id).ok_or_else(|| {
            format!(
                "Axiom '{}' precisa do requirement global '{}', mas nenhum valor concreto foi encontrado. A busca partiu de '{}'.",
                axiom_id, requirement_id, identity_path
            )
        })
    }

    fn resolve_taxonomy_requirement(
        &self,
        axiom_id: &str,
        _identity_path: &str,
        requirement_id: &str,
    ) -> Result<(String, String), String> {
        self.config.hierarchy
            .resolve_taxonomy_parameter(&self.taxonomy_path, requirement_id)
            .map_err(|_| {
                format!(
                    "Axiom '{}' precisa do requirement taxonomico '{}', mas nenhum valor foi encontrado a partir de '{}'.",
                    axiom_id, requirement_id, self.taxonomy_path
                )
            })
    }
}
