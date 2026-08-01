use crate::{
    coordinates::LineIndex,
    error::{EditError, ErrorCode},
    limits::ApplyLimits,
    model::{ByteEdit, Position, PositionEncoding, TextEdit},
    validation::validate_text_edit,
};

use super::{
    AppliedText, PreparedEdits,
    execute::{finish_apply, finish_prepare},
};

#[derive(Clone, Copy)]
pub(super) struct Candidate<'edit> {
    pub(super) start: usize,
    pub(super) end: usize,
    pub(super) order: usize,
    pub(super) before: &'edit str,
    pub(super) after: &'edit str,
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
pub fn apply_byte_edits(source: &str, edits: &[ByteEdit]) -> Result<AppliedText, EditError> {
    apply_byte_edits_with_limits(source, edits, ApplyLimits::default())
}

/// Validates and applies byte-range edits with explicit resource limits.
pub fn apply_byte_edits_with_limits(
    source: &str,
    edits: &[ByteEdit],
    limits: ApplyLimits,
) -> Result<AppliedText, EditError> {
    let candidates = byte_candidates(source, edits, limits)?;
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
    let lines = LineIndex::new(source);
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
            before: &edit.before,
            after: &edit.after,
        });
    }
    Ok(candidates)
}

/// Prepares strict UTF-8 byte-range edits for the high-throughput path.
pub fn prepare_byte_edits<'source>(
    source: &'source str,
    edits: &[ByteEdit],
) -> Result<PreparedEdits<'source>, EditError> {
    prepare_byte_edits_with_limits(source, edits, ApplyLimits::default())
}

/// Prepares byte-range edits with explicit resource limits.
pub fn prepare_byte_edits_with_limits<'source>(
    source: &'source str,
    edits: &[ByteEdit],
    limits: ApplyLimits,
) -> Result<PreparedEdits<'source>, EditError> {
    let candidates = byte_candidates(source, edits, limits)?;
    finish_prepare(
        source,
        candidates,
        limits.max_edits,
        limits.max_output_bytes,
    )
}

fn byte_candidates<'edit>(
    source: &str,
    edits: &'edit [ByteEdit],
    limits: ApplyLimits,
) -> Result<Vec<Candidate<'edit>>, EditError> {
    validate_size(source, edits.len(), limits)?;
    let mut candidates = Vec::with_capacity(edits.len());
    for (order, edit) in edits.iter().enumerate() {
        validate_byte_edit(source, edit, order)?;
        candidates.push(Candidate {
            start: edit.start,
            end: edit.end,
            order,
            before: &edit.before,
            after: &edit.after,
        });
    }
    Ok(candidates)
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
