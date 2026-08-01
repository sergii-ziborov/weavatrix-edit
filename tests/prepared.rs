use weavatrix_edit::{
    ApplyLimits, ByteEdit, ErrorCode, OffsetBias, Position, PositionEncoding, Provenance, TextEdit,
    TextRange, apply_byte_edits, apply_edits, apply_edits_with_encoding, prepare_byte_edits,
    prepare_byte_edits_with_limits, prepare_edits,
};

const PROVEN: &str = Provenance::EXACT_LSP;

#[test]
fn byte_fast_path_is_atomic_and_preserves_insert_order() {
    let source = "alpha beta";
    let edits = [
        ByteEdit::replace(6..10, "beta", "gamma", PROVEN),
        ByteEdit::insert(5, "-one", PROVEN),
        ByteEdit::insert(5, "-two", PROVEN),
    ];

    let applied = apply_byte_edits(source, &edits).unwrap();
    assert_eq!(applied.text, "alpha-one-two gamma");
    assert_eq!(applied.edits_applied, 3);
    assert_eq!(applied.bytes_before, source.len());
    assert_eq!(applied.bytes_after, applied.text.len());
}

#[test]
fn inserts_are_allowed_at_replacement_boundaries_only() {
    let source = "abcd";
    let boundary_edits = [
        ByteEdit::replace(1..3, "bc", "BC", PROVEN),
        ByteEdit::insert(1, "<", PROVEN),
        ByteEdit::insert(3, ">", PROVEN),
    ];
    assert_eq!(
        apply_byte_edits(source, &boundary_edits).unwrap().text,
        "a<BC>d"
    );

    let interior = [
        ByteEdit::replace(1..3, "bc", "BC", PROVEN),
        ByteEdit::insert(2, "!", PROVEN),
    ];
    let error = apply_byte_edits(source, &interior).unwrap_err();
    assert_eq!(error.code(), ErrorCode::OverlappingEdits);
    assert_eq!(error.edit_index(), Some(0));
    assert_eq!(error.related_edit_index(), Some(1));
}

#[test]
fn all_positions_resolve_before_any_before_check() {
    let source = "abc";
    let edits = [
        TextEdit::replace(
            TextRange::new(Position::new(1, 0), Position::new(1, 1)),
            "wrong",
            "A",
            PROVEN,
        ),
        TextEdit::insert(Position::new(2, 0), "!", PROVEN),
    ];

    let error = apply_edits(source, &edits).unwrap_err();
    assert_eq!(error.code(), ErrorCode::PositionOutOfRange);
    assert_eq!(error.edit_index(), Some(1));
}

#[test]
fn preparation_rejects_before_mismatch_without_partial_output() {
    let source = "one two";
    let edits = [
        ByteEdit::replace(0..3, "one", "ONE", PROVEN),
        ByteEdit::replace(4..7, "bad", "TWO", PROVEN),
    ];
    let error = prepare_byte_edits(source, &edits).unwrap_err();
    assert_eq!(error.code(), ErrorCode::BeforeMismatch);
    assert_eq!(error.edit_index(), Some(1));
    assert_eq!(source, "one two");
}

#[test]
fn byte_ranges_must_follow_utf8_scalar_boundaries() {
    let source = "a😀b";
    let error =
        prepare_byte_edits(source, &[ByteEdit::replace(2..5, "", "x", PROVEN)]).unwrap_err();
    assert_eq!(error.code(), ErrorCode::PositionOutOfRange);
}

#[test]
fn prepared_union_maps_offsets_and_rejects_conflicts() {
    let source = "abcdef";
    let left = prepare_byte_edits(source, &[ByteEdit::replace(1..3, "bc", "B", PROVEN)]).unwrap();
    let right = prepare_byte_edits(source, &[ByteEdit::insert(5, "!", PROVEN)]).unwrap();
    let merged = left.union(right).unwrap();

    assert_eq!(merged.apply().text, "aBde!f");
    assert!(merged.invalidates_offset(2));
    assert!(!merged.invalidates_offset(1));
    assert_eq!(merged.map_offset_forward(0, OffsetBias::Left), Some(0));
    assert_eq!(merged.map_offset_forward(1, OffsetBias::Left), Some(1));
    assert_eq!(merged.map_offset_forward(1, OffsetBias::Right), Some(2));
    assert_eq!(merged.map_offset_forward(2, OffsetBias::Left), None);
    assert_eq!(merged.map_offset_forward(3, OffsetBias::Left), Some(2));
    assert_eq!(merged.map_offset_forward(5, OffsetBias::Left), Some(4));
    assert_eq!(merged.map_offset_forward(5, OffsetBias::Right), Some(5));
    assert_eq!(merged.map_offset_forward(6, OffsetBias::Right), Some(6));

    let overlap_a =
        prepare_byte_edits(source, &[ByteEdit::replace(1..3, "bc", "B", PROVEN)]).unwrap();
    let overlap_b =
        prepare_byte_edits(source, &[ByteEdit::replace(2..4, "cd", "C", PROVEN)]).unwrap();
    assert_eq!(
        overlap_a.union(overlap_b).unwrap_err().code(),
        ErrorCode::OverlappingEdits
    );
}

#[test]
fn union_preserves_left_then_right_insert_order_and_deduplicates() {
    let source = "xy";
    let left = prepare_byte_edits(source, &[ByteEdit::insert(1, "A", PROVEN)]).unwrap();
    let right = prepare_byte_edits(source, &[ByteEdit::insert(1, "B", PROVEN)]).unwrap();
    assert_eq!(left.union(right).unwrap().apply().text, "xABy");

    let duplicate_a =
        prepare_byte_edits(source, &[ByteEdit::replace(0..1, "x", "X", PROVEN)]).unwrap();
    let duplicate_b =
        prepare_byte_edits(source, &[ByteEdit::replace(0..1, "x", "X", PROVEN)]).unwrap();
    let deduplicated = duplicate_a.union(duplicate_b).unwrap();
    assert_eq!(deduplicated.len(), 1);
    assert_eq!(deduplicated.apply().text, "Xy");
}

#[test]
fn union_preserves_the_stricter_output_limit() {
    let limits = ApplyLimits {
        max_source_bytes: 8,
        max_edits: 8,
        max_output_bytes: 2,
    };
    let left =
        prepare_byte_edits_with_limits("a", &[ByteEdit::insert(0, "x", PROVEN)], limits).unwrap();
    let right =
        prepare_byte_edits_with_limits("a", &[ByteEdit::insert(1, "y", PROVEN)], limits).unwrap();

    assert_eq!(
        left.union(right).unwrap_err().code(),
        ErrorCode::OutputTooLarge
    );
}

#[test]
fn output_validator_and_application_limits_fail_closed() {
    let prepared =
        prepare_byte_edits("let x = 1;", &[ByteEdit::replace(8..9, "1", "2", PROVEN)]).unwrap();
    assert_eq!(
        prepared
            .apply_with_validator(|text| text.ends_with(';'))
            .unwrap()
            .text,
        "let x = 2;"
    );
    assert_eq!(
        prepared.apply_with_validator(|_| false).unwrap_err().code(),
        ErrorCode::ValidationRejected
    );

    let limits = ApplyLimits {
        max_source_bytes: 32,
        max_edits: 1,
        max_output_bytes: 4,
    };
    let error =
        prepare_byte_edits_with_limits("abc", &[ByteEdit::insert(3, "def", PROVEN)], limits)
            .unwrap_err();
    assert_eq!(error.code(), ErrorCode::OutputTooLarge);
}

#[test]
fn line_edits_support_utf8_utf16_and_utf32_units() {
    let source = "a😀z";
    let utf8 = TextEdit::replace(
        TextRange::new(Position::new(1, 1), Position::new(1, 5)),
        "😀",
        "x",
        PROVEN,
    );
    let utf16 = TextEdit::replace(
        TextRange::new(Position::new(1, 1), Position::new(1, 3)),
        "😀",
        "x",
        PROVEN,
    );
    let utf32 = TextEdit::replace(
        TextRange::new(Position::new(1, 1), Position::new(1, 2)),
        "😀",
        "x",
        PROVEN,
    );

    assert_eq!(
        apply_edits_with_encoding(source, &[utf8], PositionEncoding::Utf8)
            .unwrap()
            .text,
        "axz"
    );
    assert_eq!(prepare_edits(source, &[utf16]).unwrap().apply().text, "axz");
    assert_eq!(
        apply_edits_with_encoding(source, &[utf32], PositionEncoding::Utf32)
            .unwrap()
            .text,
        "axz"
    );
}
