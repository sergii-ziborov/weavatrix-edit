use weavatrix_edit::{
    ApplyLimits, ByteEdit, ErrorCode, OffsetBias, Position, PositionEncoding, Provenance, TextEdit,
    TextRange, apply_byte_edits, apply_edits, apply_edits_with_encoding, prepare_byte_edits,
    prepare_byte_edits_owned_with_limits, prepare_byte_edits_with_limits, prepare_edits,
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
fn prepared_apply_into_reuses_storage_and_reports_exact_totals() {
    let prepared = prepare_byte_edits(
        "alpha beta",
        &[
            ByteEdit::insert(5, "!", PROVEN),
            ByteEdit::replace(6..10, "beta", "gamma", PROVEN),
        ],
    )
    .unwrap();
    let mut output = String::with_capacity(64);
    output.push_str("stale contents");
    let allocation = output.as_ptr();

    let summary = prepared.apply_into(&mut output);
    assert_eq!(output, "alpha! gamma");
    assert_eq!(output.as_ptr(), allocation);
    assert_eq!(summary.edits_applied, 2);
    assert_eq!(summary.bytes_before, 10);
    assert_eq!(summary.bytes_after, output.len());

    prepared.apply_into(&mut output);
    assert_eq!(output, "alpha! gamma");
    assert_eq!(output.as_ptr(), allocation);

    let mut bytes = Vec::with_capacity(64);
    bytes.extend_from_slice(b"stale contents");
    let byte_allocation = bytes.as_ptr();
    let byte_summary = prepared.apply_into_bytes(&mut bytes);
    assert_eq!(bytes, b"alpha! gamma");
    assert_eq!(bytes.as_ptr(), byte_allocation);
    assert_eq!(byte_summary, summary);

    let mut initially_empty = String::new();
    assert_eq!(prepared.apply_into(&mut initially_empty), summary);
    assert_eq!(initially_empty, "alpha! gamma");
    let mut initially_empty_bytes = Vec::new();
    assert_eq!(
        prepared.apply_into_bytes(&mut initially_empty_bytes),
        summary
    );
    assert_eq!(initially_empty_bytes, b"alpha! gamma");
}

#[test]
fn rendered_text_is_explicit_releasable_and_not_cloned_with_the_plan() {
    let mut prepared = prepare_byte_edits(
        "alpha beta",
        &[ByteEdit::replace(6..10, "beta", "gamma", PROVEN)],
    )
    .unwrap();

    // Ordinary application and streaming stay allocation-independent from the
    // optional retained materialization.
    assert!(!prepared.has_rendered_text());
    assert_eq!(prepared.apply().text, "alpha gamma");
    let mut output = String::new();
    prepared.apply_into(&mut output);
    assert!(!prepared.has_rendered_text());

    let cached_pointer = prepared.rendered_text().as_ptr();
    assert!(prepared.has_rendered_text());
    assert_eq!(prepared.rendered_text(), "alpha gamma");
    assert_eq!(prepared.apply().text, "alpha gamma");
    prepared.apply_into(&mut output);
    assert_eq!(output, "alpha gamma");
    let mut bytes = Vec::with_capacity(64);
    let byte_allocation = bytes.as_ptr();
    let byte_summary = prepared.apply_into_bytes(&mut bytes);
    assert_eq!(bytes, b"alpha gamma");
    assert_eq!(bytes.as_ptr(), byte_allocation);
    assert_eq!(byte_summary.bytes_before, 10);
    assert_eq!(byte_summary.bytes_after, 11);
    assert_eq!(byte_summary.edits_applied, 1);

    let cloned = prepared.clone();
    assert!(!cloned.has_rendered_text());
    assert_ne!(cloned.rendered_text().as_ptr(), cached_pointer);

    prepared.clear_rendered_text();
    assert!(!prepared.has_rendered_text());
    assert_eq!(prepared.apply().text, "alpha gamma");
}

#[test]
fn rendered_text_is_invalidated_by_union_and_initialized_once_concurrently() {
    let left = prepare_byte_edits("abcd", &[ByteEdit::replace(1..2, "b", "B", PROVEN)]).unwrap();
    assert_eq!(left.rendered_text(), "aBcd");
    let right = prepare_byte_edits("abcd", &[ByteEdit::replace(3..4, "d", "D", PROVEN)]).unwrap();
    let merged = left.union(right).unwrap();
    assert!(!merged.has_rendered_text());
    assert_eq!(merged.apply().text, "aBcD");

    let pointers = std::thread::scope(|scope| {
        let mut handles = Vec::new();
        for _ in 0..8 {
            handles.push(scope.spawn(|| {
                assert_eq!(merged.rendered_text(), "aBcD");
                merged.rendered_text().as_ptr() as usize
            }));
        }
        handles
            .into_iter()
            .map(|handle| handle.join().unwrap())
            .collect::<Vec<_>>()
    });
    assert!(pointers.iter().all(|pointer| *pointer == pointers[0]));
    assert!(merged.has_rendered_text());
}

#[test]
fn execution_fast_paths_preserve_utf8_and_logical_same_offset_changes() {
    let equal_size = prepare_byte_edits(
        "aéλz",
        &[
            ByteEdit::replace(1..3, "é", "ø", PROVEN),
            ByteEdit::replace(3..5, "λ", "Ж", PROVEN),
        ],
    )
    .unwrap();
    let mut string_output = String::new();
    let mut byte_output = Vec::new();
    equal_size.apply_into(&mut string_output);
    equal_size.apply_into_bytes(&mut byte_output);
    assert_eq!(string_output, "aøЖz");
    assert_eq!(byte_output, "aøЖz".as_bytes());

    let coalesced = prepare_byte_edits(
        "abcd",
        &[
            ByteEdit::insert(1, "A", Provenance::EXACT_LSP),
            ByteEdit::insert(1, "B", Provenance::RESOLVED),
            ByteEdit::replace(1..2, "b", "B", Provenance::EXTRACTED),
            ByteEdit::insert(2, "C", Provenance::LEXICAL_EXACT),
            ByteEdit::insert(2, "D", Provenance::EXACT_LSP),
        ],
    )
    .unwrap();
    let expected = "aABBCDcd";
    assert_eq!(coalesced.apply().text, expected);
    coalesced.apply_into(&mut string_output);
    coalesced.apply_into_bytes(&mut byte_output);
    assert_eq!(string_output, expected);
    assert_eq!(byte_output, expected.as_bytes());
    assert_eq!(coalesced.rendered_text(), expected);

    let changes = coalesced.changes().collect::<Vec<_>>();
    assert_eq!(changes.len(), 5);
    assert_eq!(
        changes
            .iter()
            .map(|change| change.output_range.clone())
            .collect::<Vec<_>>(),
        [1..2, 2..3, 3..4, 4..5, 5..6]
    );
    assert_eq!(
        changes
            .iter()
            .map(|change| change.provenance().as_str())
            .collect::<Vec<_>>(),
        [
            Provenance::EXACT_LSP,
            Provenance::RESOLVED,
            Provenance::EXTRACTED,
            Provenance::LEXICAL_EXACT,
            Provenance::EXACT_LSP,
        ]
    );
}

#[test]
fn owned_prepare_covers_sorted_and_unsorted_admission_paths() {
    let source = "abcdef";
    let limits = ApplyLimits {
        max_source_bytes: source.len(),
        max_edits: 4,
        max_output_bytes: source.len() + 4,
    };
    let unsorted_disjoint = vec![
        ByteEdit::replace(2..3, "c", "C", PROVEN),
        ByteEdit::replace(0..1, "a", "A", PROVEN),
    ];
    assert_eq!(
        prepare_byte_edits_owned_with_limits(source, unsorted_disjoint, limits)
            .unwrap()
            .apply()
            .text,
        "AbCdef"
    );

    let sorted_overlap = vec![
        ByteEdit::replace(0..3, "abc", "ABC", PROVEN),
        ByteEdit::replace(1..4, "bcd", "BCD", PROVEN),
    ];
    let error = prepare_byte_edits_owned_with_limits(source, sorted_overlap, limits).unwrap_err();
    assert_eq!(error.code(), ErrorCode::OverlappingEdits);
    assert_eq!(error.edit_index(), Some(0));
    assert_eq!(error.related_edit_index(), Some(1));

    let unsorted_overlap = vec![
        ByteEdit::replace(1..4, "bcd", "BCD", PROVEN),
        ByteEdit::replace(0..2, "ab", "AB", PROVEN),
    ];
    assert_eq!(
        prepare_byte_edits_owned_with_limits(source, unsorted_overlap, limits)
            .unwrap_err()
            .code(),
        ErrorCode::OverlappingEdits
    );

    let output_limit = ApplyLimits {
        max_output_bytes: source.len(),
        ..limits
    };
    assert_eq!(
        prepare_byte_edits_owned_with_limits(
            source,
            vec![ByteEdit::insert(source.len(), "!", PROVEN)],
            output_limit,
        )
        .unwrap_err()
        .code(),
        ErrorCode::OutputTooLarge
    );
}

#[test]
fn sorted_fast_path_and_unsorted_fallback_are_output_equivalent() {
    let source = "a😀bcdef";
    let sorted = vec![
        ByteEdit::insert(0, "<", PROVEN),
        ByteEdit::replace(5..6, "b", "B", PROVEN),
        ByteEdit::insert(6, "!", PROVEN),
        ByteEdit::replace(8..9, "e", "E", PROVEN),
    ];
    let unsorted = vec![
        sorted[2].clone(),
        sorted[3].clone(),
        sorted[0].clone(),
        sorted[1].clone(),
    ];
    let expected = "<a😀B!cdEf";

    assert_eq!(apply_byte_edits(source, &sorted).unwrap().text, expected);
    assert_eq!(apply_byte_edits(source, &unsorted).unwrap().text, expected);
    assert_eq!(
        prepare_byte_edits(source, &unsorted).unwrap().apply().text,
        expected
    );

    let same_offset_with_unsorted_tail = [
        ByteEdit::insert(6, "A", PROVEN),
        ByteEdit::insert(6, "B", PROVEN),
        ByteEdit::replace(0..1, "a", "A", PROVEN),
    ];
    assert_eq!(
        apply_byte_edits(source, &same_offset_with_unsorted_tail)
            .unwrap()
            .text,
        "A😀bABcdef"
    );
}

#[test]
fn sorted_fast_path_keeps_every_rejection_gate() {
    let overlap = [
        ByteEdit::replace(0..2, "ab", "AB", PROVEN),
        ByteEdit::replace(1..3, "bc", "BC", PROVEN),
    ];
    assert_eq!(
        apply_byte_edits("abcd", &overlap).unwrap_err().code(),
        ErrorCode::OverlappingEdits
    );
    let unsorted_overlap = [overlap[1].clone(), overlap[0].clone()];
    assert_eq!(
        apply_byte_edits("abcd", &unsorted_overlap)
            .unwrap_err()
            .code(),
        ErrorCode::OverlappingEdits
    );

    let split_scalar = [ByteEdit::replace(2..5, "", "x", PROVEN)];
    assert_eq!(
        apply_byte_edits("a😀b", &split_scalar).unwrap_err().code(),
        ErrorCode::PositionOutOfRange
    );

    let limits = ApplyLimits {
        max_source_bytes: 8,
        max_edits: 2,
        max_output_bytes: 4,
    };
    assert_eq!(
        prepare_byte_edits_with_limits("abcd", &[ByteEdit::insert(4, "!", PROVEN)], limits,)
            .unwrap_err()
            .code(),
        ErrorCode::OutputTooLarge
    );

    assert_eq!(
        apply_byte_edits("abcd", &[ByteEdit::replace(0..1, "wrong", "A", PROVEN)],)
            .unwrap_err()
            .code(),
        ErrorCode::BeforeMismatch
    );
}

#[test]
fn byte_structure_precedes_before_mismatch_for_borrowed_and_owned_prepare() {
    let edits = vec![
        ByteEdit::replace(0..1, "wrong", "A", PROVEN),
        ByteEdit::replace(99..100, "", "B", PROVEN),
    ];
    let limits = ApplyLimits {
        max_source_bytes: 8,
        max_edits: 2,
        max_output_bytes: 8,
    };

    let borrowed = prepare_byte_edits_with_limits("abc", &edits, limits).unwrap_err();
    assert_eq!(borrowed.code(), ErrorCode::PositionOutOfRange);
    assert_eq!(borrowed.edit_index(), Some(1));

    let one_shot = weavatrix_edit::apply_byte_edits_with_limits("abc", &edits, limits).unwrap_err();
    assert_eq!(one_shot.code(), ErrorCode::PositionOutOfRange);
    assert_eq!(one_shot.edit_index(), Some(1));

    let owned = prepare_byte_edits_owned_with_limits("abc", edits, limits).unwrap_err();
    assert_eq!(owned.code(), ErrorCode::PositionOutOfRange);
    assert_eq!(owned.edit_index(), Some(1));
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
