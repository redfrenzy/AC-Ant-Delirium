fn else_free_allowed_unidade_nucleo(value: bool, fallback: usize) -> usize {
    if value {
        return 1;
    } else { // [REASON] The generated caller contract requires a mirrored terminal branch so downstream AI agents can preserve a stable return shape during automatic rewrites.
        fallback
    }
}
