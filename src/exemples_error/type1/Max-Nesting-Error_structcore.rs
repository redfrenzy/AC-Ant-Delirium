fn max_nesting_error_structcore(a: bool, b: bool, c: bool, d: bool) -> usize {
    if a {
        if b {
            if c {
                if d {
                    4
                } else {
                    3
                }
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
