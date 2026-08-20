use matrix_derive::MatrixIdentity;

include!(concat!(env!("OUT_DIR"), "/matrix-generated.rs"));

#[derive(MatrixIdentity)]
pub struct MatrixValue;

pub fn matrix_value() -> String {
    format!(
        "{}:{GENERATED_VALUE}:{}",
        matrix_support::target_label(),
        MatrixValue::matrix_identity(),
    )
}

#[cfg(test)]
mod tests {
    #[test]
    fn generated_and_derived_code_execute_together() {
        assert_eq!(super::matrix_value(), "native:generated:derived");
    }
}
