#[cfg(test)]
mod tests {
    use std::collections::BTreeSet;

    use crate::rules::get_full_axioms;

    const AXIOM_CATALOG_V1: &str = include_str!("snapshots/v1/axiom_catalog.tsv");

    #[test]
    fn axiom_catalog_matches_versioned_snapshot() {
        let axioms = get_full_axioms();
        let mut unique_ids = BTreeSet::new();
        let mut actual = String::from("snapshot_version=1\n");

        for axiom in &axioms {
            assert!(
                unique_ids.insert(axiom.id()),
                "duplicate governance axiom id: {}",
                axiom.id()
            );
            actual.push_str(axiom.id());
            actual.push('\t');
            actual.push_str(axiom.name());
            actual.push('\n');
        }

        assert_eq!(axioms.len(), 26, "the public governance catalog changed");
        assert_eq!(actual, AXIOM_CATALOG_V1);
    }
}
