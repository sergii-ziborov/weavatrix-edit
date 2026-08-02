use crate::{
    error::{ByteSpan, DiagnosticLimits, EditError},
    limits::ApplyLimits,
    model::ByteEdit,
};

use super::{
    PreparedEdit, PreparedEdits, ProvenanceSet,
    prepare::{validate_byte_edit, validate_size},
    ranges::{prepared_output_size, sort_prepared, verify_ranges},
};

/// Prepares owned byte edits while moving their replacement strings into the plan.
pub fn prepare_byte_edits_owned(
    source: &str,
    edits: Vec<ByteEdit>,
) -> Result<PreparedEdits<'_>, EditError> {
    prepare_byte_edits_owned_with_limits(source, edits, ApplyLimits::default())
}

/// Prepares owned byte edits with limits and without cloning replacement strings.
pub fn prepare_byte_edits_owned_with_limits(
    source: &str,
    edits: Vec<ByteEdit>,
    limits: ApplyLimits,
) -> Result<PreparedEdits<'_>, EditError> {
    validate_size(source, edits.len(), limits)?;
    validate_owned_structure(source, &edits)?;
    let source_bytes = source.as_bytes();
    let mut prepared = Vec::with_capacity(edits.len());
    for (order, edit) in edits.into_iter().enumerate() {
        let actual = &source_bytes[edit.start..edit.end];
        if actual != edit.before.as_bytes() {
            let actual = &source[edit.start..edit.end];
            return Err(EditError::before_mismatch(
                ByteSpan::new(edit.start, edit.end),
                &edit.before,
                actual,
                DiagnosticLimits::default(),
            )
            .at_edit(order));
        }
        prepared.push(PreparedEdit {
            start: edit.start,
            end: edit.end,
            order,
            after: edit.after,
            provenance: ProvenanceSet::new(edit.provenance),
        });
    }
    sort_prepared(&mut prepared);
    verify_ranges(
        prepared
            .iter()
            .map(|edit| (edit.start, edit.end, edit.order)),
    )?;
    let output_size = prepared_output_size(source.len(), &prepared, limits.max_output_bytes)?;
    Ok(PreparedEdits::from_validated_parts(
        source,
        prepared,
        output_size,
        limits.max_edits,
        limits.max_output_bytes,
    ))
}

fn validate_owned_structure(source: &str, edits: &[ByteEdit]) -> Result<(), EditError> {
    for (order, edit) in edits.iter().enumerate() {
        validate_byte_edit(source, edit, order)?;
    }
    Ok(())
}
