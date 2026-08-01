use weavatrix_edit::{
    ApplyLimits, ByteEdit, ErrorCode, Provenance, prepare_byte_edits,
    prepare_byte_edits_with_limits,
};

const PROVEN: &str = Provenance::EXACT_LSP;

#[test]
fn union_preserves_insert_multiplicity_and_the_edit_ceiling() {
    let source = "xy";
    let duplicate_inserts = prepare_byte_edits(
        source,
        &[
            ByteEdit::insert(1, "A", PROVEN),
            ByteEdit::insert(1, "A", PROVEN),
        ],
    )
    .unwrap();
    let empty = prepare_byte_edits(source, &[]).unwrap();
    let retained = duplicate_inserts.union(empty).unwrap();
    assert_eq!(retained.len(), 2);
    assert_eq!(retained.apply().text, "xAAy");

    let left = prepare_byte_edits(source, &[ByteEdit::insert(1, "A", PROVEN)]).unwrap();
    let right = prepare_byte_edits(source, &[ByteEdit::insert(1, "A", PROVEN)]).unwrap();
    assert_eq!(left.union(right).unwrap().apply().text, "xAAy");

    let one_edit = ApplyLimits {
        max_source_bytes: 8,
        max_edits: 1,
        max_output_bytes: 8,
    };
    let left =
        prepare_byte_edits_with_limits(source, &[ByteEdit::insert(0, "L", PROVEN)], one_edit)
            .unwrap();
    let right =
        prepare_byte_edits_with_limits(source, &[ByteEdit::insert(2, "R", PROVEN)], one_edit)
            .unwrap();
    assert_eq!(
        left.union(right).unwrap_err().code(),
        ErrorCode::PlanTooLarge
    );
}
