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
    AppliedText, ChangeSummary, OffsetBias, PreparedChange, PreparedChanges, PreparedEdits,
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
struct ProvenanceSet {
    primary: Provenance,
    additional: Vec<Provenance>,
}

impl ProvenanceSet {
    fn new(primary: Provenance) -> Self {
        Self {
            primary,
            additional: Vec::new(),
        }
    }

    fn contains(&self, candidate: &Provenance) -> bool {
        self.primary == *candidate || self.additional.contains(candidate)
    }

    fn extend(&mut self, other: Self) {
        for provenance in core::iter::once(other.primary).chain(other.additional) {
            if !self.contains(&provenance) {
                self.additional.push(provenance);
            }
        }
    }
}
