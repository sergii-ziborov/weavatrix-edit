use crate::error::{EditError, ErrorCode};
use crate::model::Provenance;

use core::ops::Range;
use std::sync::OnceLock;

use super::{
    InsertRun, PreparedEdit, ProvenanceSet,
    ranges::{prepared_output_size, sort_prepared, verify_ranges},
    stream::EditChunks,
    writer::{WriteSummary, write_prepared},
};

const MAX_COALESCED_INSERT_BYTES: usize = 64 * 1024;

/// Successful all-or-nothing in-memory application.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct AppliedText {
    pub text: String,
    pub edits_applied: usize,
    pub bytes_before: usize,
    pub bytes_after: usize,
}

/// Aggregate result of applying a prepared plan into caller-owned storage.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct ApplySummary {
    pub edits_applied: usize,
    pub bytes_before: usize,
    pub bytes_after: usize,
}

/// Bias used when an original offset sits on an edit boundary.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum OffsetBias {
    Left,
    Right,
}

/// One normalized edit with exact source and resulting byte ranges.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct PreparedChange<'change> {
    pub source_range: Range<usize>,
    pub output_range: Range<usize>,
    pub before: &'change str,
    pub after: &'change str,
    pub input_order: usize,
    provenance: &'change ProvenanceSet,
}

impl PreparedChange<'_> {
    /// Primary provenance retained from the first equivalent prepared edit.
    #[must_use]
    pub fn provenance(&self) -> &Provenance {
        &self.provenance.primary
    }

    /// Every distinct provenance retained across equivalent unioned edits.
    pub fn provenances(&self) -> impl Iterator<Item = &Provenance> {
        core::iter::once(&self.provenance.primary).chain(self.provenance.additional().iter())
    }

    #[must_use]
    pub fn provenance_count(&self) -> usize {
        1 + self.provenance.additional().len()
    }
}

/// Allocation-free iterator over normalized prepared changes.
#[derive(Clone, Debug)]
pub struct PreparedChanges<'change> {
    source: &'change str,
    edits: core::slice::Iter<'change, PreparedEdit>,
    source_cursor: usize,
    output_cursor: usize,
}

impl<'change> Iterator for PreparedChanges<'change> {
    type Item = PreparedChange<'change>;

    fn next(&mut self) -> Option<Self::Item> {
        let edit = self.edits.next()?;
        let unchanged = edit.start - self.source_cursor;
        let output_start = self.output_cursor + unchanged;
        let output_end = output_start + edit.after.len();
        self.source_cursor = self.source_cursor.max(edit.end);
        self.output_cursor = output_end;
        Some(PreparedChange {
            source_range: edit.start..edit.end,
            output_range: output_start..output_end,
            before: &self.source[edit.start..edit.end],
            after: &edit.after,
            input_order: edit.order,
            provenance: &edit.provenance,
        })
    }

    fn size_hint(&self) -> (usize, Option<usize>) {
        self.edits.size_hint()
    }
}

impl ExactSizeIterator for PreparedChanges<'_> {}
impl core::iter::FusedIterator for PreparedChanges<'_> {}

/// Exact aggregate sizes for a prepared change set.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct ChangeSummary {
    pub edits: usize,
    pub bytes_before: usize,
    pub bytes_after: usize,
    pub removed_bytes: usize,
    pub inserted_bytes: usize,
}

/// Validated, sorted byte edits bound to one immutable source revision.
///
/// Application metadata may retain at most 64 KiB of coalesced insertion text
/// to accelerate repeated same-offset runs while preserving every logical
/// edit. The optional complete output cache is initialized only by
/// [`Self::rendered_text`] and can be released with
/// [`Self::clear_rendered_text`].
#[derive(Debug)]
pub struct PreparedEdits<'source> {
    source: &'source str,
    edits: Vec<PreparedEdit>,
    output_size: usize,
    max_edits: usize,
    max_output_size: usize,
    same_size: bool,
    insert_runs: Vec<InsertRun>,
    rendered: OnceLock<String>,
}

impl Clone for PreparedEdits<'_> {
    fn clone(&self) -> Self {
        Self {
            source: self.source,
            edits: self.edits.clone(),
            output_size: self.output_size,
            max_edits: self.max_edits,
            max_output_size: self.max_output_size,
            same_size: self.same_size,
            insert_runs: self.insert_runs.clone(),
            // Materialization is a replay optimization, not part of the edit
            // plan. Avoid unexpectedly cloning an output-sized cache.
            rendered: OnceLock::new(),
        }
    }
}

impl<'source> PreparedEdits<'source> {
    pub(super) fn from_validated_parts(
        source: &'source str,
        edits: Vec<PreparedEdit>,
        output_size: usize,
        max_edits: usize,
        max_output_size: usize,
    ) -> Self {
        let same_size = edits
            .iter()
            .all(|edit| edit.end - edit.start == edit.after.len());
        let insert_runs = coalesce_insert_runs(&edits);
        Self {
            source,
            edits,
            output_size,
            max_edits,
            max_output_size,
            same_size,
            insert_runs,
            rendered: OnceLock::new(),
        }
    }

    /// Applies the already-prepared edits with one output allocation.
    ///
    /// This does not retain an output-sized cache unless the caller explicitly
    /// initialized one through [`Self::rendered_text`].
    #[must_use]
    #[inline]
    pub fn apply(&self) -> AppliedText {
        let text = self.rendered.get().map_or_else(
            || {
                let mut output = String::with_capacity(self.output_size);
                self.fill_output(&mut output);
                output
            },
            Clone::clone,
        );
        AppliedText {
            bytes_before: self.source.len(),
            bytes_after: text.len(),
            edits_applied: self.edits.len(),
            text,
        }
    }

    /// Applies into caller-owned storage, retaining its allocation for replay.
    ///
    /// The output is cleared only after this plan has already completed all
    /// exact-before, Unicode-boundary, overlap, and hard-limit validation.
    /// Reusing the same `String` therefore removes allocator traffic without
    /// weakening the all-or-nothing admission contract. When the caller has
    /// explicitly initialized [`Self::rendered_text`], replay becomes one
    /// contiguous copy; otherwise this walks the normalized edit list without
    /// retaining an output-sized cache.
    #[inline]
    pub fn apply_into(&self, output: &mut String) -> ApplySummary {
        if let Some(rendered) = self.rendered.get() {
            output.clone_from(rendered);
        } else {
            output.clear();
            if output.capacity() < self.output_size {
                output.reserve(self.output_size);
            }
            self.fill_output(output);
        }
        ApplySummary {
            edits_applied: self.edits.len(),
            bytes_before: self.source.len(),
            bytes_after: output.len(),
        }
    }

    /// Applies into a caller-owned byte buffer, retaining its allocation.
    ///
    /// The bytes are guaranteed to be valid UTF-8 because the source and every
    /// replacement are validated Rust strings. This avoids a temporary
    /// `String` when the next stage is a file, socket, hash, or byte pipeline.
    /// When the caller has explicitly initialized [`Self::rendered_text`],
    /// replay becomes one contiguous copy; otherwise this walks the normalized
    /// edit list without retaining an output-sized cache.
    #[inline]
    pub fn apply_into_bytes(&self, output: &mut Vec<u8>) -> ApplySummary {
        output.clear();
        if output.capacity() < self.output_size {
            output.reserve(self.output_size);
        }
        if let Some(rendered) = self.rendered.get() {
            output.extend_from_slice(rendered.as_bytes());
        } else if self.same_size {
            output.extend_from_slice(self.source.as_bytes());
            for edit in &self.edits {
                output[edit.start..edit.end].copy_from_slice(edit.after.as_bytes());
            }
        } else {
            self.fill_output_bytes(output);
        }
        ApplySummary {
            edits_applied: self.edits.len(),
            bytes_before: self.source.len(),
            bytes_after: output.len(),
        }
    }

    #[inline]
    fn fill_output(&self, output: &mut String) {
        if self.same_size {
            output.push_str(self.source);
            for edit in &self.edits {
                output.replace_range(edit.start..edit.end, &edit.after);
            }
            return;
        }
        if !self.insert_runs.is_empty() {
            self.fill_output_with_insert_runs(output);
            return;
        }
        let mut cursor = 0_usize;
        for edit in &self.edits {
            if edit.start > cursor {
                output.push_str(&self.source[cursor..edit.start]);
                cursor = edit.start;
            }
            output.push_str(&edit.after);
            if edit.end > edit.start {
                cursor = edit.end;
            }
        }
        output.push_str(&self.source[cursor..]);
    }

    fn fill_output_bytes(&self, output: &mut Vec<u8>) {
        if !self.insert_runs.is_empty() {
            self.fill_output_bytes_with_insert_runs(output);
            return;
        }
        let mut cursor = 0_usize;
        let source = self.source.as_bytes();
        for edit in &self.edits {
            if edit.start > cursor {
                output.extend_from_slice(&source[cursor..edit.start]);
                cursor = edit.start;
            }
            output.extend_from_slice(edit.after.as_bytes());
            if edit.end > edit.start {
                cursor = edit.end;
            }
        }
        output.extend_from_slice(&source[cursor..]);
    }

    fn fill_output_with_insert_runs(&self, output: &mut String) {
        let mut cursor = 0_usize;
        self.for_each_execution_chunk(|start, end, after| {
            if start > cursor {
                output.push_str(&self.source[cursor..start]);
                cursor = start;
            }
            output.push_str(after);
            if end > start {
                cursor = end;
            }
        });
        output.push_str(&self.source[cursor..]);
    }

    fn fill_output_bytes_with_insert_runs(&self, output: &mut Vec<u8>) {
        let mut cursor = 0_usize;
        let source = self.source.as_bytes();
        self.for_each_execution_chunk(|start, end, after| {
            if start > cursor {
                output.extend_from_slice(&source[cursor..start]);
                cursor = start;
            }
            output.extend_from_slice(after.as_bytes());
            if end > start {
                cursor = end;
            }
        });
        output.extend_from_slice(&source[cursor..]);
    }

    fn for_each_execution_chunk(&self, mut visit: impl FnMut(usize, usize, &str)) {
        let mut edit_index = 0_usize;
        let mut run_index = 0_usize;
        while edit_index < self.edits.len() {
            if let Some(run) = self.insert_runs.get(run_index)
                && run.first_edit == edit_index
            {
                visit(run.start, run.start, &run.after);
                edit_index = run.past_last_edit;
                run_index += 1;
            } else {
                let edit = &self.edits[edit_index];
                visit(edit.start, edit.end, &edit.after);
                edit_index += 1;
            }
        }
    }

    #[inline]
    fn rendered_output(&self) -> &String {
        self.rendered.get_or_init(|| {
            let mut output = String::with_capacity(self.output_size);
            self.fill_output(&mut output);
            output
        })
    }

    /// Returns a lazily materialized, cached view of the complete output.
    ///
    /// This is the zero-copy replay surface for consumers that can borrow the
    /// result. The first call allocates and renders the output; later calls are
    /// constant-time. Streaming through [`Self::chunks`] or [`Self::write_to`]
    /// does not initialize this cache.
    #[must_use]
    #[inline]
    pub fn rendered_text(&self) -> &str {
        self.rendered_output()
    }

    /// Returns whether [`Self::rendered_text`] currently retains an
    /// output-sized materialization.
    #[must_use]
    pub fn has_rendered_text(&self) -> bool {
        self.rendered.get().is_some()
    }

    /// Releases the optional output-sized materialization.
    ///
    /// The normalized edit plan remains valid and later uncached applications
    /// still use the same atomic admission result.
    pub fn clear_rendered_text(&mut self) {
        drop(self.rendered.take());
    }

    /// Iterates over the validated output without allocating a final [`String`].
    ///
    /// This is the sink-independent streaming surface. Callers can forward the
    /// borrowed chunks to synchronous or asynchronous writers without adding an
    /// async runtime dependency to this crate.
    #[must_use]
    pub fn chunks(&self) -> EditChunks<'_> {
        EditChunks::new(self.source, &self.edits)
    }

    /// Iterates exact normalized changes without constructing output or a diff.
    ///
    /// Source and output ranges are UTF-8 byte ranges. Multiple inserts at one
    /// source offset retain deterministic input order and receive consecutive
    /// output ranges. Identical replacements merged by [`Self::union`] retain
    /// every distinct provenance label.
    #[must_use]
    pub fn changes(&self) -> PreparedChanges<'_> {
        PreparedChanges {
            source: self.source,
            edits: self.edits.iter(),
            source_cursor: 0,
            output_cursor: 0,
        }
    }

    /// Returns exact edit and byte totals without applying or allocating output.
    #[must_use]
    pub fn change_summary(&self) -> ChangeSummary {
        let removed_bytes = self.edits.iter().map(|edit| edit.end - edit.start).sum();
        let inserted_bytes = self.edits.iter().map(|edit| edit.after.len()).sum();
        ChangeSummary {
            edits: self.edits.len(),
            bytes_before: self.source.len(),
            bytes_after: self.output_size,
            removed_bytes,
            inserted_bytes,
        }
    }

    /// Writes the already-validated result without allocating an output [`String`].
    ///
    /// Edit validation is atomic: construction of this value completed before the
    /// first write. An I/O failure can still leave a non-transactional sink with a
    /// prefix of the result, so callers requiring sink atomicity should write to a
    /// temporary file and rename it after success. This method does not call
    /// [`std::io::Write::flush`] or request durable storage synchronization.
    pub fn write_to<W: std::io::Write + ?Sized>(
        &self,
        writer: &mut W,
    ) -> std::io::Result<WriteSummary> {
        write_prepared(
            writer,
            self.chunks(),
            self.source.len(),
            self.edits.len(),
            self.output_size,
        )
    }

    /// Applies edits and accepts only output approved by `validator`.
    pub fn apply_with_validator(
        &self,
        validator: impl FnOnce(&str) -> bool,
    ) -> Result<AppliedText, EditError> {
        let applied = self.apply();
        if validator(&applied.text) {
            Ok(applied)
        } else {
            Err(EditError::new(
                ErrorCode::ValidationRejected,
                "the output validator rejected the complete edit result",
            ))
        }
    }

    /// Merges another prepared set over identical source text.
    ///
    /// Inserts from `self` precede inserts from `other` at the same offset.
    pub fn union(mut self, mut other: Self) -> Result<Self, EditError> {
        if self.source != other.source {
            return Err(EditError::new(
                ErrorCode::InvalidEdit,
                "prepared edits can only be merged over identical source text",
            ));
        }
        self.max_edits = self.max_edits.min(other.max_edits);
        self.max_output_size = self.max_output_size.min(other.max_output_size);
        let order_base = self
            .edits
            .iter()
            .map(|edit| edit.order)
            .max()
            .map_or(Ok(0), |order| {
                order.checked_add(1).ok_or_else(|| {
                    EditError::new(ErrorCode::PlanTooLarge, "merged edit order overflow")
                })
            })?;
        for edit in &mut other.edits {
            edit.order = order_base.checked_add(edit.order).ok_or_else(|| {
                EditError::new(ErrorCode::PlanTooLarge, "merged edit order overflow")
            })?;
        }
        self.edits.extend(other.edits);
        sort_prepared(&mut self.edits);
        self.edits.dedup_by(|right, left| {
            let identical = left.start != left.end
                && left.start == right.start
                && left.end == right.end
                && left.after == right.after;
            if identical {
                let placeholder = ProvenanceSet::new(right.provenance.primary.clone());
                let other = core::mem::replace(&mut right.provenance, placeholder);
                left.provenance.extend(other);
            }
            identical
        });
        if self.edits.len() > self.max_edits {
            return Err(EditError::new(
                ErrorCode::PlanTooLarge,
                "merged edit count exceeds the application limit",
            ));
        }
        verify_ranges(
            self.edits
                .iter()
                .map(|edit| (edit.start, edit.end, edit.order)),
        )?;
        self.output_size =
            prepared_output_size(self.source.len(), &self.edits, self.max_output_size)?;
        self.same_size = self
            .edits
            .iter()
            .all(|edit| edit.end - edit.start == edit.after.len());
        self.insert_runs = coalesce_insert_runs(&self.edits);
        self.rendered = OnceLock::new();
        Ok(self)
    }

    /// Returns whether an offset lies strictly inside replaced/deleted source.
    #[must_use]
    pub fn invalidates_offset(&self, offset: usize) -> bool {
        self.edits
            .iter()
            .any(|edit| edit.start < offset && offset < edit.end)
    }

    /// Maps an original UTF-8 byte boundary into the resulting text.
    #[must_use]
    pub fn map_offset_forward(&self, offset: usize, bias: OffsetBias) -> Option<usize> {
        if offset > self.source.len() || !self.source.is_char_boundary(offset) {
            return None;
        }
        let mut delta = 0_i128;
        for edit in &self.edits {
            if offset < edit.start {
                break;
            }
            let mapped_start = shifted(edit.start, delta)?;
            if edit.start < offset && offset < edit.end {
                return None;
            }
            if edit.start == edit.end && offset == edit.start {
                if bias == OffsetBias::Left {
                    return Some(mapped_start);
                }
                delta += i128::try_from(edit.after.len()).ok()?;
                continue;
            }
            if offset == edit.start && edit.end > edit.start {
                return Some(match bias {
                    OffsetBias::Left => mapped_start,
                    OffsetBias::Right => mapped_start.checked_add(edit.after.len())?,
                });
            }
            if offset >= edit.end {
                delta += i128::try_from(edit.after.len()).ok()?
                    - i128::try_from(edit.end - edit.start).ok()?;
            }
        }
        shifted(offset, delta)
    }

    #[must_use]
    pub fn len(&self) -> usize {
        self.edits.len()
    }

    /// Returns the immutable source size used to validate this plan.
    #[must_use]
    pub fn bytes_before(&self) -> usize {
        self.source.len()
    }

    /// Returns the exact output size computed before application or streaming.
    #[must_use]
    pub fn bytes_after(&self) -> usize {
        self.output_size
    }

    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.edits.is_empty()
    }
}

fn coalesce_insert_runs(edits: &[PreparedEdit]) -> Vec<InsertRun> {
    let mut runs = Vec::new();
    let mut retained_bytes = 0_usize;
    let mut first = 0_usize;
    while first < edits.len() {
        let start = edits[first].start;
        if edits[first].end != start {
            first += 1;
            continue;
        }
        let mut past_last = first + 1;
        while past_last < edits.len()
            && edits[past_last].start == start
            && edits[past_last].end == start
        {
            past_last += 1;
        }
        if past_last - first > 1 {
            let run_bytes = edits[first..past_last]
                .iter()
                .map(|edit| edit.after.len())
                .sum();
            if run_bytes <= MAX_COALESCED_INSERT_BYTES - retained_bytes {
                let mut after = String::with_capacity(run_bytes);
                for edit in &edits[first..past_last] {
                    after.push_str(&edit.after);
                }
                runs.push(InsertRun {
                    first_edit: first,
                    past_last_edit: past_last,
                    start,
                    after,
                });
                retained_bytes += run_bytes;
            }
        }
        first = past_last;
    }
    runs
}

fn shifted(offset: usize, delta: i128) -> Option<usize> {
    usize::try_from(i128::try_from(offset).ok()?.checked_add(delta)?).ok()
}

#[cfg(test)]
mod tests {
    use crate::{application::ProvenanceSet, model::Provenance};

    use super::{MAX_COALESCED_INSERT_BYTES, PreparedEdit, coalesce_insert_runs};

    fn prepared(start: usize, end: usize, after: String, order: usize) -> PreparedEdit {
        PreparedEdit {
            start,
            end,
            order,
            after,
            provenance: ProvenanceSet::new(Provenance::new(Provenance::EXACT_LSP)),
        }
    }

    #[test]
    fn coalesced_insert_storage_has_one_global_hard_ceiling() {
        let half = MAX_COALESCED_INSERT_BYTES / 2;
        let edits = vec![
            prepared(0, 0, "a".repeat(half / 2), 0),
            prepared(0, 0, "b".repeat(half / 2), 1),
            prepared(1, 2, "X".to_owned(), 2),
            prepared(2, 2, "c".repeat(half / 2 + 1), 3),
            prepared(2, 2, "d".repeat(half / 2), 4),
        ];
        let runs = coalesce_insert_runs(&edits);
        assert_eq!(runs.len(), 1);
        assert_eq!(runs[0].after.len(), half);
        assert!(
            runs.iter().map(|run| run.after.len()).sum::<usize>() <= MAX_COALESCED_INSERT_BYTES
        );

        let oversized = vec![
            prepared(0, 0, "a".repeat(half + 1), 0),
            prepared(0, 0, "b".repeat(half), 1),
        ];
        assert!(coalesce_insert_runs(&oversized).is_empty());
    }
}
