use weavatrix_edit::{
    ApplyLimits, ByteEdit, EditPlan, ErrorCode, FileEdit, OffsetBias, PlanLimits, Position,
    PositionEncoding, Provenance, TextEdit, TextRange, apply_byte_edits_with_limits,
    apply_edits_with_encoding_and_limits, apply_edits_with_limits, prepare_byte_edits,
};

fn assert_send_sync<T: Send + Sync>() {}

const PROVEN: &str = Provenance::EXACT_LSP;
const SHA: &str = "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa";

#[test]
fn explicit_limit_wrappers_apply_the_same_contract() {
    let limits = ApplyLimits {
        max_source_bytes: 64,
        max_edits: 8,
        max_output_bytes: 64,
    };
    let utf16 = TextEdit::replace(
        TextRange::new(Position::new(1, 0), Position::new(1, 1)),
        "a",
        "A",
        PROVEN,
    );
    assert_eq!(
        apply_edits_with_limits("a", &[utf16], limits).unwrap().text,
        "A"
    );

    let utf32 = TextEdit::replace(
        TextRange::new(Position::new(1, 0), Position::new(1, 1)),
        "😀",
        "x",
        PROVEN,
    );
    assert_eq!(
        apply_edits_with_encoding_and_limits("😀", &[utf32], PositionEncoding::Utf32, limits,)
            .unwrap()
            .text,
        "x"
    );

    let byte = ByteEdit::replace(0..1, "a", "A", PROVEN);
    assert_eq!(
        apply_byte_edits_with_limits("a", &[byte], limits)
            .unwrap()
            .text,
        "A"
    );
}

#[test]
fn byte_validation_reports_each_failure_class() {
    let mut reversed = ByteEdit::insert(0, "x", PROVEN);
    reversed.start = 2;
    reversed.end = 1;
    assert_eq!(
        prepare_byte_edits("abc", &[reversed]).unwrap_err().code(),
        ErrorCode::InvalidEdit
    );

    let unchanged = ByteEdit::replace(0..1, "a", "a", PROVEN);
    assert_eq!(
        prepare_byte_edits("abc", &[unchanged]).unwrap_err().code(),
        ErrorCode::InvalidEdit
    );

    let unproven = ByteEdit::replace(0..1, "a", "A", "INFERRED");
    assert_eq!(
        prepare_byte_edits("abc", &[unproven]).unwrap_err().code(),
        ErrorCode::UnprovenEdit
    );

    let out_of_range = ByteEdit::replace(0..9, "abc", "A", PROVEN);
    assert_eq!(
        prepare_byte_edits("abc", &[out_of_range])
            .unwrap_err()
            .code(),
        ErrorCode::PositionOutOfRange
    );
}

#[test]
fn source_and_edit_count_limits_are_both_enforced() {
    let source_limit = ApplyLimits {
        max_source_bytes: 2,
        max_edits: 4,
        max_output_bytes: 16,
    };
    assert_eq!(
        apply_byte_edits_with_limits("abc", &[], source_limit)
            .unwrap_err()
            .code(),
        ErrorCode::PlanTooLarge
    );

    let edit_limit = ApplyLimits {
        max_source_bytes: 16,
        max_edits: 0,
        max_output_bytes: 16,
    };
    assert_eq!(
        apply_byte_edits_with_limits("abc", &[ByteEdit::insert(0, "x", PROVEN)], edit_limit)
            .unwrap_err()
            .code(),
        ErrorCode::PlanTooLarge
    );
}

#[test]
fn empty_prepared_set_and_invalid_projection_are_explicit() {
    let prepared = prepare_byte_edits("a😀", &[]).unwrap();
    assert!(prepared.is_empty());
    assert_eq!(prepared.len(), 0);
    assert_eq!(prepared.apply().text, "a😀");
    assert_eq!(prepared.map_offset_forward(2, OffsetBias::Left), None);
    assert_eq!(prepared.map_offset_forward(99, OffsetBias::Right), None);
}

#[test]
fn union_requires_the_same_source_text() {
    let left = prepare_byte_edits("a", &[ByteEdit::insert(0, "x", PROVEN)]).unwrap();
    let right = prepare_byte_edits("b", &[ByteEdit::insert(0, "y", PROVEN)]).unwrap();
    assert_eq!(
        left.union(right).unwrap_err().code(),
        ErrorCode::InvalidEdit
    );
}

#[test]
fn delete_constructors_and_validated_metadata_are_usable() {
    let byte_delete = ByteEdit::delete(0..1, "a", PROVEN);
    assert_eq!(
        apply_byte_edits_with_limits("ab", &[byte_delete], ApplyLimits::default())
            .unwrap()
            .text,
        "b"
    );

    let text_delete = TextEdit::delete(
        TextRange::new(Position::new(1, 0), Position::new(1, 1)),
        "a",
        PROVEN,
    );
    let plan = EditPlan::new(
        "delete_symbol",
        vec![FileEdit::new("src/a.rs", SHA, vec![text_delete])],
    );
    let validated = plan.validate_with(PlanLimits::default()).unwrap();
    assert_eq!(validated.plan().operation, "delete_symbol");
    assert_eq!(validated.total_edits(), 1);
    assert_eq!(validated.total_text_bytes(), 1);
}

#[test]
fn file_validation_error_carries_its_index() {
    let edit = TextEdit::insert(Position::new(1, 0), "x", PROVEN);
    let plan = EditPlan::new(
        "insert",
        vec![
            FileEdit::new("src/a.rs", SHA, vec![edit.clone()]),
            FileEdit::new("NUL", SHA, vec![edit]),
        ],
    );
    let error = plan.validate().unwrap_err();
    assert_eq!(error.code(), ErrorCode::InvalidPath);
    assert_eq!(error.file_index(), Some(1));
    assert!(error.message().contains("device name"));
}

#[test]
fn checked_in_json_schema_is_valid_json() {
    let schema = include_str!("../docs/schema/weavatrix.edit-plan.v1.schema.json");
    let value: blazingly_json::Value = blazingly_json::from_str(schema).unwrap();
    assert!(value.is_object());
}

#[test]
fn prepared_metadata_supports_bounded_parallel_orchestration() {
    assert_send_sync::<weavatrix_edit::PreparedEdits<'static>>();
    let prepared = weavatrix_edit::prepare_byte_edits("source", &[]).unwrap();
    assert_eq!(prepared.bytes_before(), 6);
    assert_eq!(prepared.bytes_after(), 6);
}
