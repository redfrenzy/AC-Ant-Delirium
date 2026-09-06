use super::*;

fn binding<'a>(source: &'a str, wrapper: &'a str) -> ProtectedGateBinding<'a> {
    ProtectedGateBinding {
        project_id: "prj_00000000000000000000000000",
        workspace_id: "wsp_00000000000000000000000000",
        task_id: "tsk_00000000000000000000000000",
        task_revision: 1,
        state_revision: 1,
        review_id: "rev_00000000000000000000000000",
        source_fingerprint: source,
        wrapper_sha256: wrapper,
    }
}

#[test]
fn accepts_only_a_signed_decision_for_the_exact_task_and_source() {
    let source = "a".repeat(64);
    let wrapper = "b".repeat(64);
    let expected = binding(&source, &wrapper);
    let envelope = sign_test_gate(&expected);
    assert_eq!(test_verifier().verify(&envelope, &expected), Ok(()));

    let other_source = "c".repeat(64);
    assert_eq!(
        test_verifier().verify(&envelope, &binding(&other_source, &wrapper)),
        Err(ProtectedGateError::BindingMismatch)
    );
}

#[test]
fn rejects_signature_mutation_and_blocking_decisions() {
    let source = "a".repeat(64);
    let wrapper = "b".repeat(64);
    let expected = binding(&source, &wrapper);
    let mut envelope: Value = serde_json::from_slice(&sign_test_gate(&expected)).unwrap();
    envelope["attestation"]["checkpoint_epoch"] = 2.into();
    assert_eq!(
        test_verifier().verify(&serde_json::to_vec(&envelope).unwrap(), &expected),
        Err(ProtectedGateError::InvalidSignature)
    );

    let mut blocked: Value = serde_json::from_slice(&sign_test_gate(&expected)).unwrap();
    blocked["attestation"]["decision"] = "blocked".into();
    blocked["attestation"]["blocking_violations"] =
        serde_json::json!(["protected-service-unavailable"]);
    let blocked = sign_test_attestation(blocked["attestation"].clone());
    assert_eq!(
        test_verifier().verify(&blocked, &expected),
        Err(ProtectedGateError::Blocked)
    );
}
