use weavatrix_edit::{ByteEdit, Provenance, prepare_byte_edits};

#[test]
fn normalized_changes_expose_exact_source_and_output_ranges() {
    let prepared = prepare_byte_edits(
        "abcd",
        &[
            ByteEdit::insert(1, "<", Provenance::EXACT_LSP),
            ByteEdit::replace(1..3, "bc", "X", Provenance::RESOLVED),
            ByteEdit::delete(3..4, "d", Provenance::EXTRACTED),
        ],
    )
    .unwrap();

    let changes = prepared.changes().collect::<Vec<_>>();
    assert_eq!(prepared.apply().text, "a<X");
    assert_eq!(changes.len(), 3);
    assert_eq!(changes[0].source_range, 1..1);
    assert_eq!(changes[0].output_range, 1..2);
    assert_eq!(changes[0].before, "");
    assert_eq!(changes[0].after, "<");
    assert_eq!(changes[0].input_order, 0);
    assert_eq!(changes[0].provenance().as_str(), Provenance::EXACT_LSP);
    assert_eq!(changes[0].provenance_count(), 1);
    assert_eq!(changes[1].source_range, 1..3);
    assert_eq!(changes[1].output_range, 2..3);
    assert_eq!(changes[1].before, "bc");
    assert_eq!(changes[1].after, "X");
    assert_eq!(changes[2].source_range, 3..4);
    assert_eq!(changes[2].output_range, 3..3);

    let summary = prepared.change_summary();
    assert_eq!(summary.edits, 3);
    assert_eq!(summary.bytes_before, 4);
    assert_eq!(summary.bytes_after, 3);
    assert_eq!(summary.removed_bytes, 3);
    assert_eq!(summary.inserted_bytes, 2);
}

#[test]
fn union_preserves_distinct_provenance_for_an_identical_replacement() {
    let exact = prepare_byte_edits(
        "abc",
        &[ByteEdit::replace(0..1, "a", "A", Provenance::EXACT_LSP)],
    )
    .unwrap();
    let resolved = prepare_byte_edits(
        "abc",
        &[ByteEdit::replace(0..1, "a", "A", Provenance::RESOLVED)],
    )
    .unwrap();

    let merged = exact.union(resolved).unwrap();
    let changes = merged.changes().collect::<Vec<_>>();
    assert_eq!(changes.len(), 1);
    let provenances = changes[0]
        .provenances()
        .map(Provenance::as_str)
        .collect::<Vec<_>>();
    assert_eq!(provenances, [Provenance::EXACT_LSP, Provenance::RESOLVED]);
    assert_eq!(changes[0].provenance_count(), 2);
}

#[test]
fn union_rebases_but_preserves_each_plan_input_order() {
    let left = prepare_byte_edits(
        "abcd",
        &[ByteEdit::replace(1..2, "b", "B", Provenance::EXACT_LSP)],
    )
    .unwrap();
    let right = prepare_byte_edits(
        "abcd",
        &[
            ByteEdit::replace(3..4, "d", "D", Provenance::RESOLVED),
            ByteEdit::replace(0..1, "a", "A", Provenance::RESOLVED),
        ],
    )
    .unwrap();

    let merged = left.union(right).unwrap();
    let changes = merged.changes().collect::<Vec<_>>();
    assert_eq!(
        changes
            .iter()
            .map(|change| (change.source_range.clone(), change.input_order))
            .collect::<Vec<_>>(),
        [(0..1, 2), (1..2, 0), (3..4, 1)]
    );
}
