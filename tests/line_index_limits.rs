use core::mem::size_of;

use weavatrix_edit::{
    ApplyLimits, ErrorCode, LineIndex, LineIndexLimits, Position, Provenance, TextEdit, TextRange,
    apply_edits_with_limits, prepare_edits_with_limits,
};

const PROVEN: &str = Provenance::EXACT_LSP;

#[test]
fn fallible_full_index_enforces_line_and_byte_ceilings() {
    assert_eq!(LineIndexLimits::default().max_lines, 1_000_000);
    assert_eq!(LineIndexLimits::default().max_index_bytes, 8 * 1024 * 1024);

    let source = "a\nb\n";
    let exact_index_bytes = 3 * size_of::<usize>();
    let index = LineIndex::try_new(
        source,
        LineIndexLimits {
            max_lines: 3,
            max_index_bytes: exact_index_bytes,
        },
    )
    .unwrap();
    assert_eq!(index.line_count(), 3);
    assert_eq!(
        index.byte_offset(Position::new(3, 0)).unwrap(),
        source.len()
    );

    let line_error = LineIndex::try_new(
        source,
        LineIndexLimits {
            max_lines: 2,
            max_index_bytes: usize::MAX,
        },
    )
    .unwrap_err();
    assert_eq!(line_error.code(), ErrorCode::PlanTooLarge);

    let byte_error = LineIndex::try_new(
        source,
        LineIndexLimits {
            max_lines: 3,
            max_index_bytes: exact_index_bytes - 1,
        },
    )
    .unwrap_err();
    assert_eq!(byte_error.code(), ErrorCode::PlanTooLarge);
}

#[test]
fn one_shot_position_resolution_is_sparse_for_newline_heavy_sources() {
    const NEWLINES: usize = 200_000;
    let mut source = "\n".repeat(NEWLINES);
    source.push_str("needle");
    let target_line = u32::try_from(NEWLINES + 1).unwrap();
    let edit = TextEdit::replace(
        TextRange::new(Position::new(target_line, 0), Position::new(target_line, 6)),
        "needle",
        "thread",
        PROVEN,
    );
    let limits = ApplyLimits {
        max_source_bytes: source.len(),
        max_edits: 1,
        max_output_bytes: source.len(),
    };

    assert_eq!(
        LineIndex::try_new(
            &source,
            LineIndexLimits {
                max_lines: NEWLINES + 1,
                // Enough for a few offsets, but nowhere near the full
                // 200,001-entry line-start table.
                max_index_bytes: 512,
            },
        )
        .unwrap_err()
        .code(),
        ErrorCode::PlanTooLarge
    );

    let prepared = prepare_edits_with_limits(&source, core::slice::from_ref(&edit), limits)
        .expect("one-shot preparation must not allocate a full line table");
    assert_eq!(prepared.bytes_before(), source.len());
    assert_eq!(prepared.bytes_after(), source.len());

    let applied = apply_edits_with_limits(&source, &[edit], limits).unwrap();
    assert!(applied.text.ends_with("thread"));
    assert_eq!(applied.text.len(), source.len());
}

#[test]
fn sparse_resolution_does_not_allocate_by_requested_line_number() {
    let limits = ApplyLimits {
        max_source_bytes: 1,
        max_edits: 1,
        max_output_bytes: 1,
    };
    let distant_line = TextEdit::insert(Position::new(u32::MAX, 0), "x", PROVEN);
    let error = prepare_edits_with_limits("a", &[distant_line], limits).unwrap_err();
    assert_eq!(error.code(), ErrorCode::PositionOutOfRange);
    assert_eq!(error.edit_index(), Some(0));
}
