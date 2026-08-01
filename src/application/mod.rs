mod batch;
mod batch_support;
mod execute;
mod owned;
mod prepare;
mod prepared;
mod ranges;
mod stream;
mod writer;

pub use batch::ByteEditBatch;
pub use owned::{prepare_byte_edits_owned, prepare_byte_edits_owned_with_limits};
pub use prepare::{
    apply_byte_edits, apply_byte_edits_with_limits, apply_edits, apply_edits_with_encoding,
    apply_edits_with_encoding_and_limits, apply_edits_with_limits, prepare_byte_edits,
    prepare_byte_edits_with_limits, prepare_edits, prepare_edits_with_encoding,
    prepare_edits_with_encoding_and_limits, prepare_edits_with_limits,
};
pub use prepared::{AppliedText, OffsetBias, PreparedEdits};
pub use stream::EditChunks;
pub use writer::WriteSummary;

#[derive(Clone, Debug, Eq, PartialEq)]
struct PreparedEdit {
    start: usize,
    end: usize,
    order: usize,
    after: String,
}
