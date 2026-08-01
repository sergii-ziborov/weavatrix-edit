use crate::error::{EditError, ErrorCode};

use super::{
    AppliedText, PreparedEdit, PreparedEdits,
    prepare::Candidate,
    ranges::{empty_rank, output_size, verify_ranges},
};

pub(super) fn finish_prepare<'source>(
    source: &'source str,
    mut candidates: Vec<Candidate<'_>>,
    max_edits: usize,
    max_output_bytes: usize,
) -> Result<PreparedEdits<'source>, EditError> {
    let output_size = validate_candidates(source, &mut candidates, max_output_bytes)?;
    let edits = candidates
        .into_iter()
        .map(|candidate| PreparedEdit {
            start: candidate.start,
            end: candidate.end,
            order: candidate.order,
            after: candidate.after.to_owned(),
        })
        .collect();
    Ok(PreparedEdits::from_validated_parts(
        source,
        edits,
        output_size,
        max_edits,
        max_output_bytes,
    ))
}

pub(super) fn finish_apply(
    source: &str,
    mut candidates: Vec<Candidate<'_>>,
    max_output_bytes: usize,
) -> Result<AppliedText, EditError> {
    let final_size = validate_candidates(source, &mut candidates, max_output_bytes)?;
    let mut text = String::with_capacity(final_size);
    let mut cursor = 0_usize;
    for candidate in &candidates {
        if candidate.start > cursor {
            text.push_str(&source[cursor..candidate.start]);
            cursor = candidate.start;
        }
        text.push_str(candidate.after);
        if candidate.end > candidate.start {
            cursor = candidate.end;
        }
    }
    text.push_str(&source[cursor..]);
    Ok(AppliedText {
        bytes_before: source.len(),
        bytes_after: text.len(),
        edits_applied: candidates.len(),
        text,
    })
}

fn validate_candidates(
    source: &str,
    candidates: &mut [Candidate<'_>],
    max_output_bytes: usize,
) -> Result<usize, EditError> {
    for candidate in candidates.iter() {
        let actual = &source[candidate.start..candidate.end];
        if actual != candidate.before {
            return Err(EditError::new(
                ErrorCode::BeforeMismatch,
                format!("expected {:?}, found {actual:?}", candidate.before),
            )
            .at_edit(candidate.order));
        }
    }
    candidates.sort_unstable_by(candidate_order);
    verify_ranges(
        candidates
            .iter()
            .map(|edit| (edit.start, edit.end, edit.order)),
    )?;
    output_size(
        source.len(),
        candidates
            .iter()
            .map(|edit| (edit.start, edit.end, edit.after.len())),
        max_output_bytes,
    )
}

fn candidate_order(left: &Candidate<'_>, right: &Candidate<'_>) -> core::cmp::Ordering {
    left.start
        .cmp(&right.start)
        .then_with(|| empty_rank(left.start, left.end).cmp(&empty_rank(right.start, right.end)))
        .then_with(|| left.end.cmp(&right.end))
        .then_with(|| left.order.cmp(&right.order))
}
