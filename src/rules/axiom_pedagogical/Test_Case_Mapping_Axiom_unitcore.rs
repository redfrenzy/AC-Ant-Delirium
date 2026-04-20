use crate::structs::AuditContext;

#[path = "Pedagogical_Tag_Common_unitcore.rs"]
mod Pedagogical_Tag_Common_unitcore;

pub fn patrol(ctx: &mut AuditContext<'_>) {
    let test_enabled = match Pedagogical_Tag_Common_unitcore::resolve_rule_enabled(
        ctx,
        "test_case_mapping",
        "test_case_mapping",
        "TEST",
    ) {
        Ok(value) => value,
        Err(message) => {
            Pedagogical_Tag_Common_unitcore::push_resolution_violation(ctx, "test_case_mapping", &message);
            return;
        }
    };

    if !test_enabled || !ctx.file_path.ends_with(".rs") {
        return;
    }

    Pedagogical_Tag_Common_unitcore::verify_required_tag_on_functions(
        ctx,
        "test_case_mapping",
        "[TEST]",
        "Test mapping",
    );
}
