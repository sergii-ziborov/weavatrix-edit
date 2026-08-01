use crate::error::{EditError, ErrorCode};

use super::PreparedEdit;

pub(super) fn sort_prepared(edits: &mut [PreparedEdit]) {
    edits.sort_unstable_by(compare_prepared);
}

pub(super) fn compare_prepared(left: &PreparedEdit, right: &PreparedEdit) -> core::cmp::Ordering {
    left.start
        .cmp(&right.start)
        .then_with(|| empty_rank(left.start, left.end).cmp(&empty_rank(right.start, right.end)))
        .then_with(|| left.end.cmp(&right.end))
        .then_with(|| left.order.cmp(&right.order))
}

pub(super) const fn empty_rank(start: usize, end: usize) -> u8 {
    if start == end { 0 } else { 1 }
}

pub(super) fn verify_ranges(
    ranges: impl Iterator<Item = (usize, usize, usize)>,
) -> Result<(), EditError> {
    let mut active: Option<(usize, usize)> = None;
    for (start, end, order) in ranges {
        if let Some((active_end, active_order)) = active
            && start < active_end
        {
            return Err(EditError::new(
                ErrorCode::OverlappingEdits,
                format!("edits {active_order} and {order} overlap"),
            )
            .at_edit(active_order)
            .with_related_edit(order));
        }
        if end > start {
            active = Some((end, order));
        }
    }
    Ok(())
}

pub(super) fn prepared_output_size(
    source_size: usize,
    edits: &[PreparedEdit],
    maximum: usize,
) -> Result<usize, EditError> {
    output_size(
        source_size,
        edits
            .iter()
            .map(|edit| (edit.start, edit.end, edit.after.len())),
        maximum,
    )
}

pub(super) fn output_size(
    source_size: usize,
    edits: impl Iterator<Item = (usize, usize, usize)>,
    maximum: usize,
) -> Result<usize, EditError> {
    let mut removed = 0_usize;
    let mut added = 0_usize;
    for (start, end, after_size) in edits {
        removed = removed
            .checked_add(end - start)
            .ok_or_else(|| EditError::new(ErrorCode::OutputTooLarge, "output size overflow"))?;
        added = added
            .checked_add(after_size)
            .ok_or_else(|| EditError::new(ErrorCode::OutputTooLarge, "output size overflow"))?;
    }
    let size = source_size
        .checked_sub(removed)
        .and_then(|value| value.checked_add(added))
        .ok_or_else(|| EditError::new(ErrorCode::OutputTooLarge, "output size overflow"))?;
    if size > maximum {
        return Err(EditError::new(
            ErrorCode::OutputTooLarge,
            format!("output exceeds the {maximum}-byte limit"),
        ));
    }
    Ok(size)
}
