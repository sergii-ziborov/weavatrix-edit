use weavatrix_edit::{ByteEdit, ErrorCode, Provenance, apply_byte_edits, prepare_byte_edits};

#[test]
fn shuffled_disjoint_plans_match_reference_splicer() {
    let source = "abcdefghijklmnopqrstuvwxyz".repeat(64);
    let mut seed = 0x5eed_cafe_f00d_beef_u64;

    for case in 0..2_000 {
        let mut edits = make_disjoint_edits(&source, &mut seed, case);
        let expected = reference_apply(&source, &edits);
        shuffle(&mut edits, &mut seed);
        let actual = apply_byte_edits(&source, &edits).unwrap().text;
        assert_eq!(actual, expected, "case {case}");
        let prepared = prepare_byte_edits(&source, &edits).unwrap().apply().text;
        assert_eq!(prepared, expected, "prepared case {case}");
    }
}

#[test]
fn arbitrary_offsets_never_panic_and_never_split_utf8() {
    let source = "א😀z\r\nend";
    let mut seed = 0x1234_5678_9abc_def0_u64;

    for _ in 0..20_000 {
        let start = bounded(&mut seed, source.len() + 8);
        let end = bounded(&mut seed, source.len() + 8);
        let edit = ByteEdit::replace(start..end, "", "x", Provenance::EXACT_LSP);
        let result = prepare_byte_edits(source, &[edit]);
        if let Ok(prepared) = result {
            assert!(prepared.apply().text.is_char_boundary(0));
        }
    }
}

#[test]
fn overlapping_inputs_are_rejected_for_every_permutation() {
    let source = "abcdefgh";
    let mut edits = vec![
        ByteEdit::replace(1..5, "bcde", "B", Provenance::EXACT_LSP),
        ByteEdit::replace(3..6, "def", "D", Provenance::EXACT_LSP),
        ByteEdit::insert(4, "!", Provenance::EXACT_LSP),
    ];
    let mut seed = 7_u64;
    for _ in 0..100 {
        shuffle(&mut edits, &mut seed);
        assert_eq!(
            prepare_byte_edits(source, &edits).unwrap_err().code(),
            ErrorCode::OverlappingEdits
        );
    }
}

fn make_disjoint_edits(source: &str, seed: &mut u64, case: usize) -> Vec<ByteEdit> {
    let edit_count = 1 + bounded(seed, 24);
    let stride = source.len() / (edit_count + 1);
    let mut edits = Vec::with_capacity(edit_count);
    for index in 0..edit_count {
        let start = (index + 1) * stride;
        let width = 1 + bounded(seed, 3);
        let end = (start + width).min(source.len());
        let replacement = format!("<{case}:{index}>");
        edits.push(ByteEdit::replace(
            start..end,
            &source[start..end],
            replacement,
            Provenance::EXACT_LSP,
        ));
    }
    edits
}

fn reference_apply(source: &str, edits: &[ByteEdit]) -> String {
    let mut ordered = edits.iter().collect::<Vec<_>>();
    ordered.sort_by_key(|edit| edit.start);
    let mut output = String::new();
    let mut cursor = 0;
    for edit in ordered {
        output.push_str(&source[cursor..edit.start]);
        output.push_str(&edit.after);
        cursor = edit.end;
    }
    output.push_str(&source[cursor..]);
    output
}

fn shuffle<T>(values: &mut [T], seed: &mut u64) {
    for index in (1..values.len()).rev() {
        values.swap(index, bounded(seed, index + 1));
    }
}

fn next(seed: &mut u64) -> u64 {
    *seed = seed.wrapping_mul(6_364_136_223_846_793_005).wrapping_add(1);
    *seed
}

fn bounded(seed: &mut u64, upper: usize) -> usize {
    let upper = u64::try_from(upper).unwrap_or(u64::MAX);
    usize::try_from(next(seed) % upper).expect("modulo result must fit in usize")
}
