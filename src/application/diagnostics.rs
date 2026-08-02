use crate::{
    coordinates::SparseLineIndex,
    error::{ByteSpan, DiagnosticLimits, EditError, ErrorCode, ValidationReport},
    limits::ApplyLimits,
    model::{ByteEdit, Position, PositionEncoding, TextEdit},
    validation::validate_text_edit,
};

use super::{
    prepare::{validate_byte_edit, validate_size},
    ranges::{empty_rank, output_size},
};

#[derive(Clone, Copy)]
struct CheckedRange {
    start: usize,
    end: usize,
    order: usize,
    after_len: usize,
}

struct Collector {
    retained: Vec<EditError>,
    total: usize,
    maximum: usize,
}

impl Collector {
    fn new(maximum: usize) -> Self {
        Self {
            retained: Vec::new(),
            total: 0,
            maximum,
        }
    }

    fn push(&mut self, error: EditError) {
        self.total = self.total.saturating_add(1);
        if self.retained.len() < self.maximum {
            self.retained.push(error);
        }
    }

    fn finish(self) -> ValidationReport {
        ValidationReport::new(self.retained, self.total)
    }
}

/// Checks a UTF-16 edit set and returns bounded diagnostics without applying it.
#[must_use]
pub fn diagnose_edits(source: &str, edits: &[TextEdit]) -> ValidationReport {
    diagnose_edits_with_encoding_and_limits(
        source,
        edits,
        PositionEncoding::Utf16,
        ApplyLimits::default(),
        DiagnosticLimits::default(),
    )
}

/// Checks an encoded edit set under explicit application and diagnostic limits.
#[must_use]
pub fn diagnose_edits_with_encoding_and_limits(
    source: &str,
    edits: &[TextEdit],
    encoding: PositionEncoding,
    apply_limits: ApplyLimits,
    diagnostic_limits: DiagnosticLimits,
) -> ValidationReport {
    let mut collector = Collector::new(diagnostic_limits.max_items);
    if let Err(error) = validate_size(source, edits.len(), apply_limits) {
        collector.push(error);
        return collector.finish();
    }

    let requested_lines = edits.iter().map(|edit| (edit.start_line, edit.end_line));
    let lines = match SparseLineIndex::try_for_line_pairs(source, requested_lines) {
        Ok(lines) => lines,
        Err(error) => {
            collector.push(error);
            return collector.finish();
        }
    };

    let mut checked = Vec::with_capacity(edits.len());
    for (order, edit) in edits.iter().enumerate() {
        if let Err(error) = validate_text_edit(edit, order) {
            collector.push(error);
            continue;
        }
        let start = match lines
            .byte_offset_with_encoding(Position::new(edit.start_line, edit.start_char), encoding)
        {
            Ok(start) => start,
            Err(error) => {
                collector.push(error.at_edit(order));
                continue;
            }
        };
        let end = match lines
            .byte_offset_with_encoding(Position::new(edit.end_line, edit.end_char), encoding)
        {
            Ok(end) => end,
            Err(error) => {
                collector.push(error.at_edit(order));
                continue;
            }
        };
        let Some(actual) = source.get(start..end) else {
            collector.push(
                EditError::new(
                    ErrorCode::PositionOutOfRange,
                    "resolved range is invalid for the source",
                )
                .at_edit(order),
            );
            continue;
        };
        if actual != edit.before {
            collector.push(
                EditError::before_mismatch(
                    ByteSpan::new(start, end),
                    &edit.before,
                    actual,
                    diagnostic_limits,
                )
                .at_edit(order),
            );
        }
        checked.push(CheckedRange {
            start,
            end,
            order,
            after_len: edit.after.len(),
        });
    }
    finish_ranges(source.len(), &mut checked, apply_limits, &mut collector);
    collector.finish()
}

/// Checks a UTF-8 byte edit set and returns bounded diagnostics without applying it.
#[must_use]
pub fn diagnose_byte_edits(source: &str, edits: &[ByteEdit]) -> ValidationReport {
    diagnose_byte_edits_with_limits(
        source,
        edits,
        ApplyLimits::default(),
        DiagnosticLimits::default(),
    )
}

/// Checks a byte edit set under explicit application and diagnostic limits.
#[must_use]
pub fn diagnose_byte_edits_with_limits(
    source: &str,
    edits: &[ByteEdit],
    apply_limits: ApplyLimits,
    diagnostic_limits: DiagnosticLimits,
) -> ValidationReport {
    let mut collector = Collector::new(diagnostic_limits.max_items);
    if let Err(error) = validate_size(source, edits.len(), apply_limits) {
        collector.push(error);
        return collector.finish();
    }

    let mut checked = Vec::with_capacity(edits.len());
    for (order, edit) in edits.iter().enumerate() {
        if let Err(error) = validate_byte_edit(source, edit, order) {
            collector.push(error);
            continue;
        }
        let actual = &source[edit.start..edit.end];
        if actual != edit.before {
            collector.push(
                EditError::before_mismatch(
                    ByteSpan::new(edit.start, edit.end),
                    &edit.before,
                    actual,
                    diagnostic_limits,
                )
                .at_edit(order),
            );
        }
        checked.push(CheckedRange {
            start: edit.start,
            end: edit.end,
            order,
            after_len: edit.after.len(),
        });
    }
    finish_ranges(source.len(), &mut checked, apply_limits, &mut collector);
    collector.finish()
}

fn finish_ranges(
    source_len: usize,
    checked: &mut [CheckedRange],
    limits: ApplyLimits,
    collector: &mut Collector,
) {
    checked.sort_unstable_by(checked_order);
    let mut active: Option<CheckedRange> = None;
    let mut overlaps = false;
    for current in checked.iter().copied() {
        if let Some(previous) = active
            && current.start < previous.end
        {
            overlaps = true;
            collector.push(
                EditError::new(
                    ErrorCode::OverlappingEdits,
                    "edit ranges overlap in the immutable source",
                )
                .at_edit(previous.order)
                .with_related_edit(current.order),
            );
        }
        if current.end > current.start && active.is_none_or(|previous| current.end > previous.end) {
            active = Some(current);
        }
    }
    if !overlaps
        && let Err(error) = output_size(
            source_len,
            checked
                .iter()
                .map(|edit| (edit.start, edit.end, edit.after_len)),
            limits.max_output_bytes,
        )
    {
        collector.push(error);
    }
}

fn checked_order(left: &CheckedRange, right: &CheckedRange) -> core::cmp::Ordering {
    left.start
        .cmp(&right.start)
        .then_with(|| empty_rank(left.start, left.end).cmp(&empty_rank(right.start, right.end)))
        .then_with(|| left.end.cmp(&right.end))
        .then_with(|| left.order.cmp(&right.order))
}
