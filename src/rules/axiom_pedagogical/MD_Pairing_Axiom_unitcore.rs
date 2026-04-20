use crate::structs::AuditContext;

#[path = "Pedagogical_Tag_Common_unitcore.rs"]
mod Pedagogical_Tag_Common_unitcore;

pub fn patrol(ctx: &mut AuditContext<'_>) {
    let where_enabled = match Pedagogical_Tag_Common_unitcore::resolve_rule_enabled(
        ctx,
        "md_pairing",
        "md_pairing",
        "WHERE",
    ) {
        Ok(value) => value,
        Err(message) => {
            Pedagogical_Tag_Common_unitcore::push_resolution_violation(ctx, "md_pairing", &message);
            return;
        }
    };
    if !where_enabled || !ctx.file_path.ends_with(".rs") {
        return;
    }

    Pedagogical_Tag_Common_unitcore::verify_required_tag_on_functions(
        ctx,
        "md_pairing",
        "[WHERE]",
        "Where documentation",
    );
}
