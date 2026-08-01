use crate::error::{EditError, ErrorCode};

use super::{
    PreparedEdit,
    ranges::{prepared_output_size, sort_prepared, verify_ranges},
    stream::EditChunks,
    writer::{WriteSummary, write_prepared},
};

/// Successful all-or-nothing in-memory application.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct AppliedText {
    pub text: String,
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

/// Validated, sorted byte edits bound to one immutable source revision.
#[derive(Clone, Debug)]
pub struct PreparedEdits<'source> {
    source: &'source str,
    edits: Vec<PreparedEdit>,
    output_size: usize,
    max_edits: usize,
    max_output_size: usize,
}

impl<'source> PreparedEdits<'source> {
    pub(super) fn from_validated_parts(
        source: &'source str,
        edits: Vec<PreparedEdit>,
        output_size: usize,
        max_edits: usize,
        max_output_size: usize,
    ) -> Self {
        Self {
            source,
            edits,
            output_size,
            max_edits,
            max_output_size,
        }
    }

    /// Applies the already-prepared edits with one output allocation.
    #[must_use]
    pub fn apply(&self) -> AppliedText {
        let mut text = String::with_capacity(self.output_size);
        let mut cursor = 0_usize;
        for edit in &self.edits {
            if edit.start > cursor {
                text.push_str(&self.source[cursor..edit.start]);
                cursor = edit.start;
            }
            text.push_str(&edit.after);
            if edit.end > edit.start {
                cursor = edit.end;
            }
        }
        text.push_str(&self.source[cursor..]);
        AppliedText {
            bytes_before: self.source.len(),
            bytes_after: text.len(),
            edits_applied: self.edits.len(),
            text,
        }
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
        for (offset, edit) in other.edits.iter_mut().enumerate() {
            edit.order = order_base.checked_add(offset).ok_or_else(|| {
                EditError::new(ErrorCode::PlanTooLarge, "merged edit order overflow")
            })?;
        }
        self.edits.extend(other.edits);
        sort_prepared(&mut self.edits);
        self.edits.dedup_by(|right, left| {
            left.start != left.end
                && left.start == right.start
                && left.end == right.end
                && left.after == right.after
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

fn shifted(offset: usize, delta: i128) -> Option<usize> {
    usize::try_from(i128::try_from(offset).ok()?.checked_add(delta)?).ok()
}
