#[test]
fn workspace_dependencies_are_available() {
    assert_eq!(matrix_app::matrix_value(), "native:generated:derived");
}
