mod batch;
mod batch_support;
mod diagnostics;
mod execute;
mod owned;
mod prepare;
mod prepared;
mod ranges;
mod stream;
mod writer;

use crate::model::Provenance;

pub use batch::ByteEditBatch;
pub use diagnostics::{
    diagnose_byte_edits, diagnose_byte_edits_with_limits, diagnose_edits,
    diagnose_edits_with_encoding_and_limits,
};
pub use owned::{prepare_byte_edits_owned, prepare_byte_edits_owned_with_limits};
pub use prepare::{
    apply_byte_edits, apply_byte_edits_with_limits, apply_edits, apply_edits_with_encoding,
    apply_edits_with_encoding_and_limits, apply_edits_with_limits, prepare_byte_edits,
    prepare_byte_edits_with_limits, prepare_edits, prepare_edits_with_encoding,
    prepare_edits_with_encoding_and_limits, prepare_edits_with_limits,
};
pub use prepared::{
    AppliedText, ApplySummary, ChangeSummary, OffsetBias, PreparedChange, PreparedChanges,
    PreparedEdits,
};
pub use stream::EditChunks;
pub use writer::WriteSummary;

#[derive(Clone, Debug, Eq, PartialEq)]
struct PreparedEdit {
    start: usize,
    end: usize,
    order: usize,
    after: String,
    provenance: ProvenanceSet,
}

#[derive(Clone, Debug, Eq, PartialEq)]
struct InsertRun {
    first_edit: usize,
    past_last_edit: usize,
    start: usize,
    after: String,
}

#[derive(Clone, Debug, Eq, PartialEq)]
struct ProvenanceSet {
    primary: Provenance,
    // Most edits have exactly one provenance. Keep that common case to one
    // pointer instead of embedding an empty three-word Vec in every prepared
    // edit; storage is allocated only when `union` actually merges evidence.
    additional: Option<Box<[Provenance]>>,
}

impl ProvenanceSet {
    fn new(primary: Provenance) -> Self {
        Self {
            primary,
            additional: None,
        }
    }

    fn additional(&self) -> &[Provenance] {
        self.additional.as_deref().unwrap_or(&[])
    }

    fn extend(&mut self, other: Self) {
        let mut retained = self
            .additional
            .take()
            .map_or_else(Vec::new, <[Provenance]>::into_vec);
        for provenance in
            core::iter::once(other.primary).chain(other.additional.into_iter().flatten())
        {
            if self.primary != provenance && !retained.contains(&provenance) {
                retained.push(provenance);
            }
        }
        if !retained.is_empty() {
            self.additional = Some(retained.into_boxed_slice());
        }
    }
}
