use weavatrix_edit::{
    ApplyLimits, BatchLimits, ByteEdit, ByteEditBatch, ErrorCode, Provenance, prepare_byte_edits,
    prepare_byte_edits_owned, prepare_byte_edits_owned_with_limits, prepare_byte_edits_with_limits,
};

const PROVEN: &str = Provenance::EXACT_LSP;

fn limits() -> BatchLimits {
    BatchLimits {
        max_source_bytes: 100,
        max_edits: 100,
        max_before_bytes: 100,
        max_replacement_bytes: 100,
        max_output_bytes: 100,
    }
}

#[test]
fn owned_prepare_matches_borrowed_success_and_failure() {
    let source = "a😀bc";
    let edits = vec![
        ByteEdit::insert(1, "<", PROVEN),
        ByteEdit::insert(1, ">", PROVEN),
        ByteEdit::replace(5..6, "b", "BETA", PROVEN),
    ];
    let borrowed = prepare_byte_edits(source, &edits).unwrap();
    let owned = prepare_byte_edits_owned(source, edits).unwrap();
    assert_eq!(owned.apply(), borrowed.apply());

    let failures = [
        vec![ByteEdit::replace(0..1, "wrong", "A", PROVEN)],
        vec![ByteEdit::replace(2..5, "", "x", PROVEN)],
        vec![
            ByteEdit::replace(0..2, "a", "A", PROVEN),
            ByteEdit::replace(1..5, "😀", "E", PROVEN),
        ],
    ];
    for edits in failures {
        let borrowed = prepare_byte_edits(source, &edits).unwrap_err();
        let owned = prepare_byte_edits_owned(source, edits).unwrap_err();
        assert_eq!(owned.code(), borrowed.code());
        assert_eq!(owned.edit_index(), borrowed.edit_index());
        assert_eq!(owned.related_edit_index(), borrowed.related_edit_index());
    }
}

#[test]
fn builder_preserves_original_coordinates_boundaries_and_insert_order() {
    let mut batch = ByteEditBatch::new("abcd").unwrap();
    batch.push(ByteEdit::insert(1, "<", PROVEN)).unwrap();
    batch.push(ByteEdit::insert(1, "[", PROVEN)).unwrap();
    batch
        .push(ByteEdit::replace(1..3, "bc", "BC", PROVEN))
        .unwrap();
    batch.push(ByteEdit::insert(3, "]", PROVEN)).unwrap();

    assert_eq!(batch.len(), 4);
    assert!(!batch.is_empty());
    assert_eq!(batch.finish().unwrap().apply().text, "a<[BC]d");
}

#[test]
fn interior_insert_conflicts_are_order_independent_and_transactional() {
    let source = "abcd";
    let insert = ByteEdit::insert(2, "X", PROVEN);
    let replace = ByteEdit::replace(1..3, "bc", "Y", PROVEN);

    let mut singles = ByteEditBatch::new(source).unwrap();
    singles.push(insert.clone()).unwrap();
    let error = singles.push(replace.clone()).unwrap_err();
    assert_eq!(error.code(), ErrorCode::OverlappingEdits);
    assert_eq!(error.edit_index(), Some(1));
    assert_eq!(error.related_edit_index(), Some(0));
    assert_eq!(singles.len(), 1);
    assert_eq!(singles.finish().unwrap().apply().text, "abXcd");

    for edits in [
        vec![insert.clone(), replace.clone()],
        vec![replace.clone(), insert.clone()],
    ] {
        let mut batch = ByteEditBatch::new(source).unwrap();
        let error = batch.push_batch(edits).unwrap_err();
        assert_eq!(error.code(), ErrorCode::OverlappingEdits);
        assert!(batch.is_empty());
    }
}

#[test]
fn batch_matches_borrowed_boundary_matrix() {
    let source = "abcd";
    let cases = [
        vec![
            ByteEdit::insert(2, "X", PROVEN),
            ByteEdit::replace(1..3, "bc", "Y", PROVEN),
        ],
        vec![
            ByteEdit::replace(1..3, "bc", "Y", PROVEN),
            ByteEdit::insert(2, "X", PROVEN),
        ],
        vec![
            ByteEdit::insert(1, "L", PROVEN),
            ByteEdit::replace(1..3, "bc", "Y", PROVEN),
        ],
        vec![
            ByteEdit::replace(1..3, "bc", "Y", PROVEN),
            ByteEdit::insert(3, "R", PROVEN),
        ],
        vec![
            ByteEdit::insert(2, "A", PROVEN),
            ByteEdit::insert(2, "B", PROVEN),
        ],
    ];
    for edits in cases {
        let expected = prepare_byte_edits(source, &edits);
        let mut batch = ByteEditBatch::new(source).unwrap();
        let actual = batch.push_batch(edits);
        match (expected, actual) {
            (Ok(expected), Ok(())) => {
                assert_eq!(batch.finish().unwrap().apply(), expected.apply());
            }
            (Err(expected), Err(actual)) => assert_eq!(actual.code(), expected.code()),
            outcome => panic!("batch parity mismatch: {outcome:?}"),
        }
    }
}

#[test]
fn push_batch_rolls_back_and_checks_all_before_values_before_overlap() {
    let mut batch = ByteEditBatch::new("abcdef").unwrap();
    batch.push(ByteEdit::insert(0, "!", PROVEN)).unwrap();
    let error = batch
        .push_batch(vec![
            ByteEdit::replace(1..4, "bcd", "B", PROVEN),
            ByteEdit::replace(2..5, "cde", "C", PROVEN),
            ByteEdit::replace(5..6, "wrong", "F", PROVEN),
        ])
        .unwrap_err();

    assert_eq!(error.code(), ErrorCode::BeforeMismatch);
    assert_eq!(error.edit_index(), Some(3));
    assert_eq!(batch.len(), 1);
    assert_eq!(batch.finish().unwrap().apply().text, "!abcdef");
}

#[test]
fn every_batch_budget_is_hard_and_failed_admission_rolls_back() {
    let mut source_limited = limits();
    source_limited.max_source_bytes = 3;
    assert_eq!(
        ByteEditBatch::with_limits("abcd", source_limited)
            .unwrap_err()
            .code(),
        ErrorCode::PlanTooLarge
    );

    let mut edit_limited = limits();
    edit_limited.max_edits = 1;
    let mut batch = ByteEditBatch::with_limits("ab", edit_limited).unwrap();
    batch.push(ByteEdit::insert(0, "x", PROVEN)).unwrap();
    let error = batch.push(ByteEdit::insert(2, "y", PROVEN)).unwrap_err();
    assert_eq!(
        (error.code(), error.edit_index()),
        (ErrorCode::PlanTooLarge, Some(1))
    );

    let mut before_limited = limits();
    before_limited.max_before_bytes = 1;
    assert_eq!(
        ByteEditBatch::with_limits("ab", before_limited)
            .unwrap()
            .push(ByteEdit::replace(0..2, "ab", "x", PROVEN))
            .unwrap_err()
            .code(),
        ErrorCode::PlanTooLarge
    );

    let mut replacement_limited = limits();
    replacement_limited.max_replacement_bytes = 2;
    let mut batch = ByteEditBatch::with_limits("abc", replacement_limited).unwrap();
    batch.push(ByteEdit::insert(0, "x", PROVEN)).unwrap();
    let error = batch
        .push_batch(vec![
            ByteEdit::insert(1, "y", PROVEN),
            ByteEdit::insert(2, "z", PROVEN),
        ])
        .unwrap_err();
    assert_eq!(
        (error.code(), error.edit_index()),
        (ErrorCode::PlanTooLarge, Some(2))
    );
    assert_eq!(batch.len(), 1);

    let mut output_limited = limits();
    output_limited.max_output_bytes = 4;
    let mut batch = ByteEditBatch::with_limits("abcd", output_limited).unwrap();
    assert_eq!(
        batch
            .push(ByteEdit::insert(0, "x", PROVEN))
            .unwrap_err()
            .code(),
        ErrorCode::OutputTooLarge
    );
    batch
        .push_batch(vec![
            ByteEdit::insert(0, "x", PROVEN),
            ByteEdit::delete(2..3, "c", PROVEN),
        ])
        .unwrap();
    assert_eq!(batch.finish().unwrap().apply().text, "xabd");
}

#[test]
fn output_limit_uses_final_size_and_allows_a_shrinking_batch() {
    let apply_limits = ApplyLimits {
        max_source_bytes: 8,
        max_edits: 2,
        max_output_bytes: 4,
    };
    let edits = vec![
        ByteEdit::insert(0, "x", PROVEN),
        ByteEdit::delete(2..3, "c", PROVEN),
    ];
    assert_eq!(
        prepare_byte_edits_with_limits("abcd", &edits, apply_limits)
            .unwrap()
            .apply()
            .text,
        "xabd"
    );
    assert_eq!(
        prepare_byte_edits_owned_with_limits("abcd", edits, apply_limits)
            .unwrap()
            .apply()
            .text,
        "xabd"
    );

    let too_small = ApplyLimits {
        max_output_bytes: 3,
        ..apply_limits
    };
    assert_eq!(
        prepare_byte_edits_with_limits("abcd", &[], too_small)
            .unwrap_err()
            .code(),
        ErrorCode::OutputTooLarge
    );

    let mut batch_limits = limits();
    batch_limits.max_output_bytes = 3;
    let mut batch = ByteEditBatch::with_limits("abcd", batch_limits).unwrap();
    assert_eq!(
        batch.finish().unwrap_err().code(),
        ErrorCode::OutputTooLarge
    );
    batch = ByteEditBatch::with_limits("abcd", batch_limits).unwrap();
    batch
        .push_batch(vec![ByteEdit::delete(0..1, "a", PROVEN)])
        .unwrap();
    assert_eq!(batch.finish().unwrap().apply().text, "bcd");
}
