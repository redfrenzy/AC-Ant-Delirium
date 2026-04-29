use crate::structs::AuditContext;

#[path = "Pedagogical_Tag_Common_unitcore.rs"]
mod Pedagogical_Tag_Common_unitcore;

pub fn patrol(ctx: &mut AuditContext<'_>) {
    if !ctx.file_path.ends_with(".rs") {
        return;
    }
    if Pedagogical_Tag_Common_unitcore::resolve_doc_tags_aggregator_enabled(ctx).unwrap_or(false) {
        return;
    }

    let usage_enabled = match Pedagogical_Tag_Common_unitcore::resolve_rule_enabled(
        ctx,
        "usage_example",
        "usage_example",
        "usage",
    ) {
        Ok(value) => value,
        Err(message) => {
            Pedagogical_Tag_Common_unitcore::push_resolution_violation(
                ctx,
                "usage_example",
                &message,
            );
            return;
        }
    };

    if !usage_enabled {
        return;
    }

    Pedagogical_Tag_Common_unitcore::verify_required_tag_on_functions(
        ctx,
        "usage_example",
        "[USAGE]",
        "Usage example",
    );
}
