use blazingly_json::{from_str, to_string};
use weavatrix_edit::{ErrorCode, Position, Provenance, TextEdit, apply_edits};

#[test]
fn error_codes_have_stable_wire_names() {
    for (code, wire_name) in [
        (ErrorCode::SchemaMismatch, "SCHEMA_MISMATCH"),
        (ErrorCode::InvalidPlan, "INVALID_PLAN"),
        (ErrorCode::InvalidFile, "INVALID_FILE"),
        (ErrorCode::InvalidEdit, "INVALID_EDIT"),
        (ErrorCode::InvalidPath, "INVALID_PATH"),
        (ErrorCode::UnprovenEdit, "UNPROVEN_EDIT"),
        (ErrorCode::PlanTooLarge, "PLAN_TOO_LARGE"),
        (ErrorCode::PositionOutOfRange, "POSITION_OUT_OF_RANGE"),
        (ErrorCode::BeforeMismatch, "BEFORE_MISMATCH"),
        (ErrorCode::OverlappingEdits, "OVERLAPPING_EDITS"),
        (ErrorCode::OutputTooLarge, "OUTPUT_TOO_LARGE"),
        (ErrorCode::ValidationRejected, "VALIDATION_REJECTED"),
    ] {
        assert_eq!(code.as_str(), wire_name);
        assert_eq!(code.to_string(), wire_name);
        let encoded = to_string(&code).unwrap();
        assert_eq!(encoded, format!("\"{wire_name}\""));
        assert_eq!(from_str::<ErrorCode>(&encoded).unwrap(), code);
    }
}

#[test]
fn structured_errors_include_the_failing_edit_index() {
    let error = apply_edits(
        "abc",
        &[TextEdit::insert(
            Position::new(2, 0),
            "x",
            Provenance::EXACT_LSP,
        )],
    )
    .unwrap_err();
    let json = to_string(&error).unwrap();

    assert!(json.contains("\"code\":\"POSITION_OUT_OF_RANGE\""));
    assert!(json.contains("\"editIndex\":0"));
    assert!(!json.contains("fileIndex"));
    assert!(error.message().contains("line exceeds"));
    assert_eq!(error.file_index(), None);
    assert!(error.to_string().starts_with("POSITION_OUT_OF_RANGE:"));
}
