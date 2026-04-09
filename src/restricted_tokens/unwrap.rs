//@ [WHAT] Unauthorized Safe Harbor for unwrap logic.
//@ [WHY] To test the 'Illegal Logic' detection when TOML authorization is missing.

pub fn illegal_unwrap(val: Option<i32>) -> i32 {
    // This UNWRAP is in the correct folder, but NOT authorized in delirium.toml!
    val.unwrap() 
}
