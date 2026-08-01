use std::{collections::BTreeMap, ops::Bound::Excluded};

use crate::{
    error::{EditError, ErrorCode},
    limits::BatchLimits,
    model::ByteEdit,
};

use super::{PreparedEdit, ranges::compare_prepared};

#[derive(Debug)]
pub(super) struct Occupancy {
    ranges: BTreeMap<usize, (usize, usize)>,
    inserts: BTreeMap<usize, usize>,
}

impl Occupancy {
    pub(super) const fn new() -> Self {
        Self {
            ranges: BTreeMap::new(),
            inserts: BTreeMap::new(),
        }
    }

    pub(super) fn extend(&mut self, other: Self) {
        self.ranges.extend(other.ranges);
        for (offset, order) in other.inserts {
            self.inserts.entry(offset).or_insert(order);
        }
    }

    fn record(&mut self, edit: &ByteEdit, order: usize) {
        if edit.start == edit.end {
            self.inserts.entry(edit.start).or_insert(order);
        } else {
            self.ranges.insert(edit.start, (edit.end, order));
        }
    }
}

#[derive(Clone, Copy, Debug)]
pub(super) struct Totals {
    before: usize,
    replacement: usize,
    removed: usize,
}

impl Totals {
    pub(super) const fn empty() -> Self {
        Self {
            before: 0,
            replacement: 0,
            removed: 0,
        }
    }
}

pub(super) fn admission_order(base: usize, offset: usize) -> Result<usize, EditError> {
    base.checked_add(offset)
        .ok_or_else(|| EditError::new(ErrorCode::PlanTooLarge, "edit index overflow"))
}

pub(super) fn validate_before(
    source: &str,
    edit: &ByteEdit,
    order: usize,
) -> Result<(), EditError> {
    let actual = &source[edit.start..edit.end];
    if actual == edit.before {
        return Ok(());
    }
    Err(EditError::new(
        ErrorCode::BeforeMismatch,
        format!("expected {:?}, found {actual:?}", edit.before),
    )
    .at_edit(order))
}

pub(super) fn add_totals(
    totals: Totals,
    edit: &ByteEdit,
    limits: BatchLimits,
    order: usize,
) -> Result<Totals, EditError> {
    let before = bounded_add(
        totals.before,
        edit.before.len(),
        limits.max_before_bytes,
        order,
    )?;
    let replacement = bounded_add(
        totals.replacement,
        edit.after.len(),
        limits.max_replacement_bytes,
        order,
    )?;
    let removed = totals
        .removed
        .checked_add(edit.end - edit.start)
        .ok_or_else(|| size_error(order))?;
    Ok(Totals {
        before,
        replacement,
        removed,
    })
}

fn bounded_add(
    value: usize,
    added: usize,
    maximum: usize,
    order: usize,
) -> Result<usize, EditError> {
    let total = value.checked_add(added).ok_or_else(|| size_error(order))?;
    if total > maximum {
        return Err(size_error(order));
    }
    Ok(total)
}

fn size_error(order: usize) -> EditError {
    EditError::new(
        ErrorCode::PlanTooLarge,
        "cumulative batch text exceeds its input budget",
    )
    .at_edit(order)
}

pub(super) fn check_output(
    source_size: usize,
    totals: Totals,
    maximum: usize,
    order: Option<usize>,
) -> Result<usize, EditError> {
    let size = source_size
        .checked_sub(totals.removed)
        .and_then(|value| value.checked_add(totals.replacement))
        .ok_or_else(|| EditError::new(ErrorCode::OutputTooLarge, "output size overflow"))?;
    if size <= maximum {
        return Ok(size);
    }
    let error = EditError::new(
        ErrorCode::OutputTooLarge,
        format!("output exceeds the {maximum}-byte limit"),
    );
    Err(order.map_or(error.clone(), |index| error.at_edit(index)))
}

pub(super) fn find_overlap(
    accepted: &Occupancy,
    staged: Option<&Occupancy>,
    start: usize,
    end: usize,
) -> Option<usize> {
    let include_equal = end > start;
    let prior = [Some(accepted), staged]
        .into_iter()
        .flatten()
        .filter_map(|occupied| containing_range(&occupied.ranges, start, include_equal))
        .max_by_key(|(range_start, _)| *range_start);
    if let Some((_, order)) = prior {
        return Some(order);
    }
    if end == start {
        return None;
    }
    [Some(accepted), staged]
        .into_iter()
        .flatten()
        .filter_map(|occupied| {
            occupied
                .ranges
                .range(start..)
                .next()
                .map(|(next, value)| (*next, value.1))
        })
        .filter(|(next, _)| *next < end)
        .min_by_key(|(next, _)| *next)
        .map(|(_, order)| order)
        .or_else(|| interior_insert(accepted, staged, start, end))
}

fn interior_insert(
    accepted: &Occupancy,
    staged: Option<&Occupancy>,
    start: usize,
    end: usize,
) -> Option<usize> {
    [Some(accepted), staged]
        .into_iter()
        .flatten()
        .filter_map(|occupied| {
            occupied
                .inserts
                .range((Excluded(start), Excluded(end)))
                .next()
                .map(|(offset, order)| (*offset, *order))
        })
        .min_by_key(|(offset, _)| *offset)
        .map(|(_, order)| order)
}

fn containing_range(
    ranges: &BTreeMap<usize, (usize, usize)>,
    position: usize,
    include_equal: bool,
) -> Option<(usize, usize)> {
    let found = if include_equal {
        ranges.range(..=position).next_back()
    } else {
        ranges.range(..position).next_back()
    };
    found
        .filter(|(_, (end, _))| *end > position)
        .map(|(start, (_, order))| (*start, *order))
}

pub(super) fn stage_edit(
    edit: ByteEdit,
    order: usize,
    staged: &mut Vec<PreparedEdit>,
    occupied: &mut Occupancy,
) {
    staged.push(into_prepared(edit, order, occupied));
}

pub(super) fn into_prepared(
    edit: ByteEdit,
    order: usize,
    occupied: &mut Occupancy,
) -> PreparedEdit {
    occupied.record(&edit, order);
    PreparedEdit {
        start: edit.start,
        end: edit.end,
        order,
        after: edit.after,
    }
}

pub(super) fn merge_sorted(left: Vec<PreparedEdit>, right: Vec<PreparedEdit>) -> Vec<PreparedEdit> {
    let capacity = left.len().saturating_add(right.len());
    let mut left = left.into_iter().peekable();
    let mut right = right.into_iter().peekable();
    let mut merged = Vec::with_capacity(capacity);
    while let (Some(left_edit), Some(right_edit)) = (left.peek(), right.peek()) {
        let next = if compare_prepared(left_edit, right_edit).is_le() {
            left.next()
        } else {
            right.next()
        };
        if let Some(edit) = next {
            merged.push(edit);
        }
    }
    merged.extend(left);
    merged.extend(right);
    merged
}
