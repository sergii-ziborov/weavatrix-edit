use std::iter::FusedIterator;

use super::PreparedEdit;

/// Zero-copy chunks of one already-validated edit result.
///
/// Chunks borrow either the immutable source or prepared replacement text.
/// Empty source spans and empty replacements are skipped.
#[derive(Clone, Debug)]
pub struct EditChunks<'prepared> {
    source: &'prepared str,
    edits: &'prepared [PreparedEdit],
    edit_index: usize,
    cursor: usize,
    finished: bool,
}

impl<'prepared> EditChunks<'prepared> {
    pub(super) const fn new(source: &'prepared str, edits: &'prepared [PreparedEdit]) -> Self {
        Self {
            source,
            edits,
            edit_index: 0,
            cursor: 0,
            finished: false,
        }
    }
}

impl<'prepared> Iterator for EditChunks<'prepared> {
    type Item = &'prepared str;

    fn next(&mut self) -> Option<Self::Item> {
        loop {
            if let Some(edit) = self.edits.get(self.edit_index) {
                if self.cursor < edit.start {
                    let unchanged = &self.source[self.cursor..edit.start];
                    self.cursor = edit.start;
                    return Some(unchanged);
                }
                self.edit_index += 1;
                self.cursor = self.cursor.max(edit.end);
                if !edit.after.is_empty() {
                    return Some(&edit.after);
                }
                continue;
            }
            if self.finished {
                return None;
            }
            self.finished = true;
            if self.cursor < self.source.len() {
                return Some(&self.source[self.cursor..]);
            }
        }
    }

    fn size_hint(&self) -> (usize, Option<usize>) {
        let edits_left = self.edits.len().saturating_sub(self.edit_index);
        (
            0,
            edits_left
                .checked_mul(2)
                .and_then(|size| size.checked_add(1)),
        )
    }
}

impl FusedIterator for EditChunks<'_> {}
