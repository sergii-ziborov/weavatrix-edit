use crate::{
    coordinates::SparseLineIndex,
    error::{ByteSpan, DiagnosticLimits, EditError, ErrorCode},
    limits::ApplyLimits,
    model::{ByteEdit, Position, PositionEncoding, Provenance, TextEdit},
    validation::validate_text_edit,
};

use super::{
    AppliedText, PreparedEdit, PreparedEdits, ProvenanceSet,
    execute::{finish_apply, finish_prepare},
    ranges::{empty_rank, output_size_from_totals},
};

#[derive(Clone, Copy)]
pub(super) struct Candidate<'edit> {
    pub(super) start: usize,
    pub(super) end: usize,
    pub(super) order: usize,
    pub(super) after: &'edit str,
    pub(super) provenance: &'edit Provenance,
}

/// Validates and applies v1 UTF-16 edits atomically in memory.
pub fn apply_edits(source: &str, edits: &[TextEdit]) -> Result<AppliedText, EditError> {
    apply_edits_with_limits(source, edits, ApplyLimits::default())
}

/// Validates and applies v1 UTF-16 edits with explicit resource limits.
pub fn apply_edits_with_limits(
    source: &str,
    edits: &[TextEdit],
    limits: ApplyLimits,
) -> Result<AppliedText, EditError> {
    apply_edits_with_encoding_and_limits(source, edits, PositionEncoding::Utf16, limits)
}

/// Validates and applies edits whose line/character units use `encoding`.
pub fn apply_edits_with_encoding(
    source: &str,
    edits: &[TextEdit],
    encoding: PositionEncoding,
) -> Result<AppliedText, EditError> {
    apply_edits_with_encoding_and_limits(source, edits, encoding, ApplyLimits::default())
}

/// Validates and applies encoded positions with explicit resource limits.
pub fn apply_edits_with_encoding_and_limits(
    source: &str,
    edits: &[TextEdit],
    encoding: PositionEncoding,
    limits: ApplyLimits,
) -> Result<AppliedText, EditError> {
    let candidates = position_candidates(source, edits, encoding, limits)?;
    finish_apply(source, candidates, limits.max_output_bytes)
}

/// Validates and applies strict UTF-8 byte-range edits atomically in memory.
#[inline]
pub fn apply_byte_edits(source: &str, edits: &[ByteEdit]) -> Result<AppliedText, EditError> {
    apply_byte_edits_with_limits(source, edits, ApplyLimits::default())
}

/// Validates and applies byte-range edits with explicit resource limits.
#[inline]
pub fn apply_byte_edits_with_limits(
    source: &str,
    edits: &[ByteEdit],
    limits: ApplyLimits,
) -> Result<AppliedText, EditError> {
    if let Some(output_size) = preflight_sorted_byte_edits(source, edits, limits)? {
        return Ok(apply_sorted_byte_edits(source, edits, output_size));
    }
    let candidates = byte_candidates_from_validated(edits);
    finish_apply(source, candidates, limits.max_output_bytes)
}

/// Prepares v1 UTF-16 edits once for repeat application or offset mapping.
pub fn prepare_edits<'source>(
    source: &'source str,
    edits: &[TextEdit],
) -> Result<PreparedEdits<'source>, EditError> {
    prepare_edits_with_limits(source, edits, ApplyLimits::default())
}

/// Prepares v1 UTF-16 edits with explicit resource limits.
pub fn prepare_edits_with_limits<'source>(
    source: &'source str,
    edits: &[TextEdit],
    limits: ApplyLimits,
) -> Result<PreparedEdits<'source>, EditError> {
    prepare_edits_with_encoding_and_limits(source, edits, PositionEncoding::Utf16, limits)
}

/// Prepares line/character edits using UTF-8, UTF-16, or UTF-32 units.
pub fn prepare_edits_with_encoding<'source>(
    source: &'source str,
    edits: &[TextEdit],
    encoding: PositionEncoding,
) -> Result<PreparedEdits<'source>, EditError> {
    prepare_edits_with_encoding_and_limits(source, edits, encoding, ApplyLimits::default())
}

/// Prepares encoded line/character edits with explicit resource limits.
pub fn prepare_edits_with_encoding_and_limits<'source>(
    source: &'source str,
    edits: &[TextEdit],
    encoding: PositionEncoding,
    limits: ApplyLimits,
) -> Result<PreparedEdits<'source>, EditError> {
    let candidates = position_candidates(source, edits, encoding, limits)?;
    finish_prepare(
        source,
        candidates,
        limits.max_edits,
        limits.max_output_bytes,
    )
}

fn position_candidates<'edit>(
    source: &str,
    edits: &'edit [TextEdit],
    encoding: PositionEncoding,
    limits: ApplyLimits,
) -> Result<Vec<Candidate<'edit>>, EditError> {
    validate_size(source, edits.len(), limits)?;
    let requested_line_pairs = edits.iter().map(|edit| (edit.start_line, edit.end_line));
    let lines = SparseLineIndex::try_for_line_pairs(source, requested_line_pairs)?;
    let mut candidates = Vec::with_capacity(edits.len());
    for (order, edit) in edits.iter().enumerate() {
        validate_text_edit(edit, order)?;
        let start = lines
            .byte_offset_with_encoding(Position::new(edit.start_line, edit.start_char), encoding)
            .map_err(|error| error.at_edit(order))?;
        let end = lines
            .byte_offset_with_encoding(Position::new(edit.end_line, edit.end_char), encoding)
            .map_err(|error| error.at_edit(order))?;
        candidates.push(Candidate {
            start,
            end,
            order,
            after: &edit.after,
            provenance: &edit.provenance,
        });
    }
    // Position admission intentionally completes for the whole batch before
    // exact-before evidence is checked, matching the v1 deterministic error
    // precedence contract.
    for candidate in &candidates {
        validate_before(
            source,
            candidate.start,
            candidate.end,
            &edits[candidate.order].before,
            candidate.order,
        )?;
    }
    Ok(candidates)
}

/// Prepares strict UTF-8 byte-range edits for the high-throughput path.
#[inline]
pub fn prepare_byte_edits<'source>(
    source: &'source str,
    edits: &[ByteEdit],
) -> Result<PreparedEdits<'source>, EditError> {
    prepare_byte_edits_with_limits(source, edits, ApplyLimits::default())
}

/// Prepares byte-range edits with explicit resource limits.
#[inline]
pub fn prepare_byte_edits_with_limits<'source>(
    source: &'source str,
    edits: &[ByteEdit],
    limits: ApplyLimits,
) -> Result<PreparedEdits<'source>, EditError> {
    if let Some(output_size) = preflight_sorted_byte_edits(source, edits, limits)? {
        let prepared = edits
            .iter()
            .enumerate()
            .map(|(order, edit)| PreparedEdit {
                start: edit.start,
                end: edit.end,
                order,
                after: edit.after.clone(),
                provenance: ProvenanceSet::new(edit.provenance.clone()),
            })
            .collect();
        return Ok(PreparedEdits::from_validated_parts(
            source,
            prepared,
            output_size,
            limits.max_edits,
            limits.max_output_bytes,
        ));
    }
    let candidates = byte_candidates_from_validated(edits);
    finish_prepare(
        source,
        candidates,
        limits.max_edits,
        limits.max_output_bytes,
    )
}

fn preflight_sorted_byte_edits(
    source: &str,
    edits: &[ByteEdit],
    limits: ApplyLimits,
) -> Result<Option<usize>, EditError> {
    validate_size(source, edits.len(), limits)?;
    let mut sorted = true;
    let mut previous: Option<&ByteEdit> = None;
    let mut active: Option<(usize, usize)> = None;
    let mut overlap = None;
    let mut removed = 0_usize;
    let mut added = 0_usize;
    let mut output_overflow = false;
    let mut first_before_mismatch = None;

    for (order, edit) in edits.iter().enumerate() {
        validate_byte_edit(source, edit, order)?;
        if first_before_mismatch.is_none()
            && source.as_bytes()[edit.start..edit.end] != *edit.before.as_bytes()
        {
            first_before_mismatch = Some(order);
        }
        if let Some(left) = previous
            && byte_edit_order(left, edit).is_gt()
        {
            sorted = false;
        }
        if let Some((active_end, active_order)) = active
            && edit.start < active_end
            && overlap.is_none()
        {
            overlap = Some((active_order, order));
        }
        if edit.end > edit.start {
            active = Some((edit.end, order));
        }
        removed = removed
            .checked_add(edit.end - edit.start)
            .unwrap_or_else(|| {
                output_overflow = true;
                usize::MAX
            });
        added = added.checked_add(edit.after.len()).unwrap_or_else(|| {
            output_overflow = true;
            usize::MAX
        });
        previous = Some(edit);
    }

    if let Some(order) = first_before_mismatch {
        let edit = &edits[order];
        validate_before(source, edit.start, edit.end, &edit.before, order)?;
    }

    if !sorted {
        return Ok(None);
    }
    if let Some((left, right)) = overlap {
        return Err(EditError::new(
            ErrorCode::OverlappingEdits,
            format!("edits {left} and {right} overlap"),
        )
        .at_edit(left)
        .with_related_edit(right));
    }
    if output_overflow {
        return Err(EditError::new(
            ErrorCode::OutputTooLarge,
            "output size overflow",
        ));
    }
    output_size_from_totals(source.len(), removed, added, limits.max_output_bytes).map(Some)
}

fn byte_edit_order(left: &ByteEdit, right: &ByteEdit) -> core::cmp::Ordering {
    left.start
        .cmp(&right.start)
        .then_with(|| empty_rank(left.start, left.end).cmp(&empty_rank(right.start, right.end)))
        .then_with(|| left.end.cmp(&right.end))
}

fn apply_sorted_byte_edits(source: &str, edits: &[ByteEdit], output_size: usize) -> AppliedText {
    let mut text = String::with_capacity(output_size);
    let mut cursor = 0_usize;
    for edit in edits {
        if edit.start > cursor {
            text.push_str(&source[cursor..edit.start]);
            cursor = edit.start;
        }
        text.push_str(&edit.after);
        if edit.end > edit.start {
            cursor = edit.end;
        }
    }
    text.push_str(&source[cursor..]);
    AppliedText {
        bytes_before: source.len(),
        bytes_after: text.len(),
        edits_applied: edits.len(),
        text,
    }
}

fn byte_candidates_from_validated(edits: &[ByteEdit]) -> Vec<Candidate<'_>> {
    let mut candidates = Vec::with_capacity(edits.len());
    for (order, edit) in edits.iter().enumerate() {
        candidates.push(Candidate {
            start: edit.start,
            end: edit.end,
            order,
            after: &edit.after,
            provenance: &edit.provenance,
        });
    }
    candidates
}

fn validate_before(
    source: &str,
    start: usize,
    end: usize,
    expected: &str,
    order: usize,
) -> Result<(), EditError> {
    let actual = &source[start..end];
    if actual == expected {
        return Ok(());
    }
    Err(EditError::before_mismatch(
        ByteSpan::new(start, end),
        expected,
        actual,
        DiagnosticLimits::default(),
    )
    .at_edit(order))
}

pub(super) fn validate_byte_edit(
    source: &str,
    edit: &ByteEdit,
    order: usize,
) -> Result<(), EditError> {
    if edit.end < edit.start || edit.before == edit.after {
        return Err(EditError::new(ErrorCode::InvalidEdit, "invalid byte edit").at_edit(order));
    }
    if !edit.provenance.is_applicable() {
        return Err(
            EditError::new(ErrorCode::UnprovenEdit, "byte edit is not proven").at_edit(order),
        );
    }
    if source.get(edit.start..edit.end).is_none() {
        return Err(EditError::new(
            ErrorCode::PositionOutOfRange,
            "byte range is out of bounds or splits a Unicode scalar value",
        )
        .at_edit(order));
    }
    Ok(())
}

pub(super) fn validate_size(
    source: &str,
    edits: usize,
    limits: ApplyLimits,
) -> Result<(), EditError> {
    if source.len() > limits.max_source_bytes || edits > limits.max_edits {
        return Err(EditError::new(
            ErrorCode::PlanTooLarge,
            "source or edit count exceeds application limits",
        ));
    }
    Ok(())
}
