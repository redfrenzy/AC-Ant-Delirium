pub fn max_nesting_error_unit(flag_a: bool, flag_b: bool, flag_c: bool) -> usize {
    if flag_a {
        if flag_b {
            if flag_c {
                3
            } else {
                2
            }
        } else {
            1
        }
    } else {
        0
    }
}
