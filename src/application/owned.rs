use crate::{
    error::{ByteSpan, DiagnosticLimits, EditError, ErrorCode},
    limits::ApplyLimits,
    model::ByteEdit,
};

use super::{
    PreparedEdit, PreparedEdits, ProvenanceSet,
    prepare::{validate_byte_edit, validate_size},
    ranges::{compare_prepared, output_size_from_totals, sort_prepared, verify_ranges},
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
    let source_bytes = source.as_bytes();
    let mut prepared = Vec::with_capacity(edits.len());
    let mut sorted = true;
    let mut active: Option<(usize, usize)> = None;
    let mut overlap = None;
    let mut removed = 0_usize;
    let mut added = 0_usize;
    let mut output_overflow = false;
    let mut first_before_error = None;
    for (order, edit) in edits.into_iter().enumerate() {
        validate_byte_edit(source, &edit, order)?;
        let actual = &source_bytes[edit.start..edit.end];
        if first_before_error.is_none() && actual != edit.before.as_bytes() {
            let actual = &source[edit.start..edit.end];
            first_before_error = Some(
                EditError::before_mismatch(
                    ByteSpan::new(edit.start, edit.end),
                    &edit.before,
                    actual,
                    DiagnosticLimits::default(),
                )
                .at_edit(order),
            );
        }
        let next = PreparedEdit {
            start: edit.start,
            end: edit.end,
            order,
            after: edit.after,
            provenance: ProvenanceSet::new(edit.provenance),
        };
        if let Some(previous) = prepared.last()
            && compare_prepared(previous, &next).is_gt()
        {
            sorted = false;
        }
        if let Some((active_end, active_order)) = active
            && next.start < active_end
            && overlap.is_none()
        {
            overlap = Some((active_order, order));
        }
        if next.end > next.start {
            active = Some((next.end, order));
        }
        removed = removed
            .checked_add(next.end - next.start)
            .unwrap_or_else(|| {
                output_overflow = true;
                usize::MAX
            });
        added = added.checked_add(next.after.len()).unwrap_or_else(|| {
            output_overflow = true;
            usize::MAX
        });
        prepared.push(next);
    }
    if let Some(error) = first_before_error {
        return Err(error);
    }
    if sorted {
        if let Some((left, right)) = overlap {
            return Err(EditError::new(
                ErrorCode::OverlappingEdits,
                format!("edits {left} and {right} overlap"),
            )
            .at_edit(left)
            .with_related_edit(right));
        }
    } else {
        sort_prepared(&mut prepared);
        verify_ranges(
            prepared
                .iter()
                .map(|edit| (edit.start, edit.end, edit.order)),
        )?;
    }
    if output_overflow {
        return Err(EditError::new(
            ErrorCode::OutputTooLarge,
            "output size overflow",
        ));
    }
    let output_size =
        output_size_from_totals(source.len(), removed, added, limits.max_output_bytes)?;
    Ok(PreparedEdits::from_validated_parts(
        source,
        prepared,
        output_size,
        limits.max_edits,
        limits.max_output_bytes,
    ))
}
