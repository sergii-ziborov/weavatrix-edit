//! Acceptance divergence between the capturing and declared-only decodes.
//!
//! `Capture` materializes every undeclared member into a `Value`, so a member
//! the value model cannot represent fails the whole decode. `Discard` consumes
//! the same member through `IgnoredAny` and never inspects it. Documents that
//! differ only inside undeclared members therefore decode differently.
//!
//! This is a deliberate property of a declared-only decode, not a defect, but
//! it means the two paths are NOT interchangeable for a consumer that relies on
//! decode to reject malformed input.

use weavatrix_edit::{DeclaredEditPlan, EditPlan};

fn envelope_with_plan_member(member: &str) -> String {
    format!(
        r#"{{"schemaVersion":"weavatrix.edit-plan.v1","operation":"rename","files":[{{"path":"src/a.rs","sha256":"{sha}","edits":[{{"startLine":1,"startChar":0,"endLine":1,"endChar":1,"before":"a","after":"b","provenance":"EXACT_LSP"}}]}}],{member}}}"#,
        sha = "a".repeat(64),
    )
}

#[test]
fn undeclared_members_the_value_model_rejects_pass_a_declared_only_decode() {
    for member in [r#""big":1e400"#, r#""big":123456789012345678901234567890"#] {
        let json = envelope_with_plan_member(member);

        let capturing = blazingly_json::from_str::<EditPlan>(&json);
        assert!(
            capturing.is_err(),
            "capturing decode should reject {member}: {capturing:?}"
        );

        let declared = blazingly_json::from_str::<DeclaredEditPlan>(&json)
            .unwrap_or_else(|error| panic!("declared decode should accept {member}: {error}"));
        assert!(
            declared.into_plan().validate().is_ok(),
            "declared decode of {member} should also validate"
        );
    }
}

#[test]
fn documents_without_undeclared_members_decode_identically() {
    let json = envelope_with_plan_member(r#""completeness":"COMPLETE""#);

    let capturing = blazingly_json::from_str::<EditPlan>(&json).expect("capturing decode");
    let declared = blazingly_json::from_str::<DeclaredEditPlan>(&json)
        .expect("declared decode")
        .into_plan();

    assert_eq!(capturing, declared);
}
