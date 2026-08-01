use weavatrix_edit::{
    EditError, ErrorCode, LineIndex, Position, Provenance, TextEdit, TextRange, apply_edits,
};

fn replace(
    start_line: u32,
    start_char: u32,
    end_line: u32,
    end_char: u32,
    before: &str,
    after: &str,
) -> TextEdit {
    TextEdit::replace(
        TextRange::new(
            Position::new(start_line, start_char),
            Position::new(end_line, end_char),
        ),
        before,
        after,
        Provenance::EXACT_LSP,
    )
}

fn insert(line: u32, character: u32, after: &str) -> TextEdit {
    TextEdit::insert(Position::new(line, character), after, Provenance::EXACT_LSP)
}

fn assert_code(error: &EditError, expected: ErrorCode) {
    assert_eq!(error.code(), expected, "unexpected error: {error}");
}

#[test]
fn line_index_matches_v1_lf_utf16_coordinates() {
    let content = "alpha\nbeta\ngamma";
    let index = LineIndex::new(content);

    assert_eq!(index.byte_offset(Position::new(1, 0)).unwrap(), 0);
    assert_eq!(index.byte_offset(Position::new(2, 0)).unwrap(), 6);
    assert_eq!(index.byte_offset(Position::new(3, 5)).unwrap(), 16);

    assert_code(
        &index.byte_offset(Position::new(4, 0)).unwrap_err(),
        ErrorCode::PositionOutOfRange,
    );
    assert_code(
        &index.byte_offset(Position::new(1, 99)).unwrap_err(),
        ErrorCode::PositionOutOfRange,
    );
}

#[test]
fn applies_single_and_multiple_renames_against_original_text() {
    let applied = apply_edits(
        "const getUser = 1\n",
        &[replace(1, 6, 1, 13, "getUser", "getCustomer")],
    )
    .unwrap();
    assert_eq!(applied.text, "const getCustomer = 1\n");
    assert_eq!(applied.edits_applied, 1);

    let content = "getUser(getUser)\n";
    let applied = apply_edits(
        content,
        &[
            replace(1, 0, 1, 7, "getUser", "getCustomer"),
            replace(1, 8, 1, 15, "getUser", "getCustomer"),
        ],
    )
    .unwrap();
    assert_eq!(applied.text, "getCustomer(getCustomer)\n");
}

#[test]
fn preserves_crlf_and_addresses_the_final_empty_line() {
    let content = "first\r\nrename me\r\nlast\r\n";
    let applied = apply_edits(content, &[replace(2, 0, 2, 6, "rename", "renamed")]).unwrap();
    assert_eq!(applied.text, "first\r\nrenamed me\r\nlast\r\n");

    let content = "keep\n";
    let index = LineIndex::new(content);
    assert_eq!(index.line_count(), 2);
    assert_eq!(index.byte_offset(Position::new(2, 0)).unwrap(), 5);
    assert_eq!(
        apply_edits(content, &[insert(2, 0, "added\n")])
            .unwrap()
            .text,
        "keep\nadded\n"
    );
}

#[test]
fn counts_astral_unicode_as_two_utf16_units() {
    let content = "const x = \"😀\"; getUser()\n";
    let applied = apply_edits(content, &[replace(1, 16, 1, 23, "getUser", "getCustomer")]).unwrap();
    assert_eq!(applied.text, "const x = \"😀\"; getCustomer()\n");
}

#[test]
fn rejects_a_position_inside_a_surrogate_pair() {
    let content = "x = \"😀\"\n";
    let error = apply_edits(content, &[replace(1, 5, 1, 6, "😀", "Z")]).unwrap_err();

    assert_code(&error, ErrorCode::PositionOutOfRange);
    assert_eq!(error.edit_index(), Some(0));
}

#[test]
fn exact_before_mismatch_fails_closed() {
    let error = apply_edits(
        "const getUsr = 1\n",
        &[replace(1, 6, 1, 13, "getUser", "getCustomer")],
    )
    .unwrap_err();

    assert_code(&error, ErrorCode::BeforeMismatch);
    assert_eq!(error.edit_index(), Some(0));
}

#[test]
fn before_mismatch_precedes_overlap_detection() {
    let error = apply_edits(
        "abcdefgh\n",
        &[
            replace(1, 0, 1, 4, "WRONG", "x"),
            replace(1, 2, 1, 6, "cdef", "y"),
        ],
    )
    .unwrap_err();

    assert_code(&error, ErrorCode::BeforeMismatch);
    assert_eq!(error.edit_index(), Some(0));
}

#[test]
fn all_positions_are_resolved_before_any_before_text_is_checked() {
    let error = apply_edits(
        "abc\n",
        &[
            replace(1, 0, 1, 1, "WRONG", "x"),
            replace(9, 0, 9, 1, "z", "y"),
        ],
    )
    .unwrap_err();

    assert_code(&error, ErrorCode::PositionOutOfRange);
    assert_eq!(error.edit_index(), Some(1));
}

#[test]
fn rejects_overlapping_replacements_but_accepts_adjacency() {
    let error = apply_edits(
        "abcdefgh\n",
        &[
            replace(1, 0, 1, 4, "abcd", "x"),
            replace(1, 2, 1, 6, "cdef", "y"),
        ],
    )
    .unwrap_err();
    assert_code(&error, ErrorCode::OverlappingEdits);
    assert_eq!(error.edit_index(), Some(0));
    assert_eq!(error.related_edit_index(), Some(1));

    let applied = apply_edits(
        "abc\n",
        &[replace(1, 0, 1, 1, "a", "A"), replace(1, 1, 1, 2, "b", "B")],
    )
    .unwrap();
    assert_eq!(applied.text, "ABc\n");
}

#[test]
fn same_offset_insertions_preserve_plan_array_order() {
    let applied = apply_edits(
        "abcd\n",
        &[insert(1, 4, "1"), insert(1, 4, "2"), insert(1, 4, "3")],
    )
    .unwrap();

    assert_eq!(applied.text, "abcd123\n");
}

#[test]
fn boundary_insertions_are_allowed_but_interior_insertions_conflict() {
    let at_start = apply_edits("abc", &[replace(1, 0, 1, 1, "a", "A"), insert(1, 0, "X")]).unwrap();
    assert_eq!(at_start.text, "XAbc");

    let at_end = apply_edits("abc", &[replace(1, 0, 1, 1, "a", "A"), insert(1, 1, "X")]).unwrap();
    assert_eq!(at_end.text, "AXbc");

    let error =
        apply_edits("abc", &[replace(1, 0, 1, 2, "ab", "AB"), insert(1, 1, "X")]).unwrap_err();
    assert_code(&error, ErrorCode::OverlappingEdits);
}

#[test]
fn multiline_deletion_uses_the_next_line_zero_position() {
    let content = "keep\ndelete me\nkeep too\n";
    let applied = apply_edits(content, &[replace(2, 0, 3, 0, "delete me\n", "")]).unwrap();

    assert_eq!(applied.text, "keep\nkeep too\n");
}
