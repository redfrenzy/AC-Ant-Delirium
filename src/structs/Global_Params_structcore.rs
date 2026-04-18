use crate::structs::AuditContext;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AxiomParameterResolution {
    pub axiom_id: String,
    pub identity_path: String,
    pub requirement_id: String,
    pub value: String,
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

        let value = self.config.hierarchy.resolve_parameter(requirement_id).ok_or_else(|| {
            format!(
                "Axiom '{}' precisa do requirement '{}', mas nenhum valor concreto foi encontrado. A busca partiu de '{}'.",
                axiom_id, requirement_id, identity_path
            )
        })?;

        Ok(AxiomParameterResolution {
            axiom_id: axiom_id.to_string(),
            identity_path,
            requirement_id: requirement_id.to_string(),
            value,
        })
    }
}
