use crate::{
    error::{EditError, ErrorCode},
    limits::BatchLimits,
    model::ByteEdit,
};

use super::{
    PreparedEdit, PreparedEdits,
    batch_support::{
        Occupancy, Totals, add_totals, admission_order, check_output, find_overlap, into_prepared,
        merge_sorted, stage_edit, validate_before,
    },
    prepare::validate_byte_edit,
    ranges::{compare_prepared, sort_prepared},
};

/// Transactional builder for a bounded batch over one immutable source revision.
///
/// Every range and `before` value always refers to `source`, including edits
/// submitted by later calls. This is intentionally not a current-document edit
/// session: accepted edits do not shift coordinates for subsequent admission.
#[derive(Debug)]
pub struct ByteEditBatch<'source> {
    source: &'source str,
    edits: Vec<PreparedEdit>,
    occupied: Occupancy,
    totals: Totals,
    output_size: usize,
    limits: BatchLimits,
}

impl<'source> ByteEditBatch<'source> {
    /// Starts an empty batch with default hard resource limits.
    pub fn new(source: &'source str) -> Result<Self, EditError> {
        Self::with_limits(source, BatchLimits::default())
    }

    /// Starts an empty batch with explicit hard resource limits.
    pub fn with_limits(source: &'source str, limits: BatchLimits) -> Result<Self, EditError> {
        if source.len() > limits.max_source_bytes {
            return Err(EditError::new(
                ErrorCode::PlanTooLarge,
                "source exceeds the batch source limit",
            ));
        }
        Ok(Self {
            source,
            edits: Vec::new(),
            occupied: Occupancy::new(),
            totals: Totals::empty(),
            output_size: source.len(),
            limits,
        })
    }

    /// Admits one edit or leaves the batch unchanged on failure.
    pub fn push(&mut self, edit: ByteEdit) -> Result<(), EditError> {
        let order = self.edits.len();
        self.ensure_count(order)?;
        validate_byte_edit(self.source, &edit, order)?;
        validate_before(self.source, &edit, order)?;
        let totals = add_totals(self.totals, &edit, self.limits, order)?;
        self.ensure_no_overlap(&edit, order, None)?;
        let output_size = check_output(
            self.source.len(),
            totals,
            self.limits.max_output_bytes,
            Some(order),
        )?;
        self.commit_one(edit, order, totals, output_size);
        Ok(())
    }

    /// Admits every edit atomically, preserving input order at equal offsets.
    pub fn push_batch(&mut self, edits: Vec<ByteEdit>) -> Result<(), EditError> {
        let base = self.edits.len();
        self.ensure_batch_count(edits.len())?;
        for (offset, edit) in edits.iter().enumerate() {
            let order = admission_order(base, offset)?;
            validate_byte_edit(self.source, edit, order)?;
        }
        let mut totals = self.totals;
        for (offset, edit) in edits.iter().enumerate() {
            let order = admission_order(base, offset)?;
            validate_before(self.source, edit, order)?;
        }
        for (offset, edit) in edits.iter().enumerate() {
            let order = admission_order(base, offset)?;
            totals = add_totals(totals, edit, self.limits, order)?;
        }
        let mut staged = Vec::with_capacity(edits.len());
        let mut staged_occupancy = Occupancy::new();
        for (offset, edit) in edits.into_iter().enumerate() {
            let order = admission_order(base, offset)?;
            self.ensure_no_overlap(&edit, order, Some(&staged_occupancy))?;
            stage_edit(edit, order, &mut staged, &mut staged_occupancy);
        }
        let output_size = check_output(
            self.source.len(),
            totals,
            self.limits.max_output_bytes,
            base.checked_add(staged.len())
                .and_then(|value| value.checked_sub(1)),
        )?;
        sort_prepared(&mut staged);
        self.edits = merge_sorted(core::mem::take(&mut self.edits), staged);
        self.occupied.extend(staged_occupancy);
        self.totals = totals;
        self.output_size = output_size;
        Ok(())
    }

    /// Consumes the builder without cloning replacement strings.
    pub fn finish(self) -> Result<PreparedEdits<'source>, EditError> {
        check_output(
            self.source.len(),
            self.totals,
            self.limits.max_output_bytes,
            None,
        )?;
        Ok(PreparedEdits::from_validated_parts(
            self.source,
            self.edits,
            self.output_size,
            self.limits.max_edits,
            self.limits.max_output_bytes,
        ))
    }

    #[must_use]
    pub fn len(&self) -> usize {
        self.edits.len()
    }

    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.edits.is_empty()
    }

    fn ensure_count(&self, order: usize) -> Result<(), EditError> {
        if order >= self.limits.max_edits {
            return Err(EditError::new(
                ErrorCode::PlanTooLarge,
                "edit count exceeds the batch limit",
            )
            .at_edit(order));
        }
        Ok(())
    }

    fn ensure_batch_count(&self, incoming: usize) -> Result<(), EditError> {
        if incoming > self.limits.max_edits.saturating_sub(self.edits.len()) {
            return Err(EditError::new(
                ErrorCode::PlanTooLarge,
                "edit count exceeds the batch limit",
            )
            .at_edit(self.limits.max_edits));
        }
        Ok(())
    }

    fn ensure_no_overlap(
        &self,
        edit: &ByteEdit,
        order: usize,
        staged: Option<&Occupancy>,
    ) -> Result<(), EditError> {
        if let Some(related) = find_overlap(&self.occupied, staged, edit.start, edit.end) {
            return Err(EditError::new(
                ErrorCode::OverlappingEdits,
                format!("edits {related} and {order} overlap"),
            )
            .at_edit(order)
            .with_related_edit(related));
        }
        Ok(())
    }

    fn commit_one(&mut self, edit: ByteEdit, order: usize, totals: Totals, output_size: usize) {
        let prepared = into_prepared(edit, order, &mut self.occupied);
        let position = self
            .edits
            .partition_point(|current| compare_prepared(current, &prepared).is_le());
        self.edits.insert(position, prepared);
        self.totals = totals;
        self.output_size = output_size;
    }
}
