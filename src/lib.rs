#![forbid(unsafe_code)]
#![doc = include_str!("../README.md")]

mod application;
mod coordinates;
mod error;
mod limits;
mod model;
mod path;
mod provenance;
mod validation;

pub use application::{
    AppliedText, ApplySummary, ByteEditBatch, ChangeSummary, EditChunks, OffsetBias,
    PreparedChange, PreparedChanges, PreparedEdits, WriteSummary, apply_byte_edits,
    apply_byte_edits_with_limits, apply_edits, apply_edits_with_encoding,
    apply_edits_with_encoding_and_limits, apply_edits_with_limits, diagnose_byte_edits,
    diagnose_byte_edits_with_limits, diagnose_edits, diagnose_edits_with_encoding_and_limits,
    prepare_byte_edits, prepare_byte_edits_owned, prepare_byte_edits_owned_with_limits,
    prepare_byte_edits_with_limits, prepare_edits, prepare_edits_with_encoding,
    prepare_edits_with_encoding_and_limits, prepare_edits_with_limits,
};
pub use coordinates::LineIndex;
pub use error::{
    ByteSpan, DiagnosticLimits, EditError, ErrorCode, MismatchDetails, TextPreview,
    ValidationReport,
};
pub use limits::{ApplyLimits, BatchLimits, LineIndexLimits, MAX_PLAN_OPERATION_BYTES, PlanLimits};
pub use model::{
    ByteEdit, Completeness, EDIT_PLAN_SCHEMA, EditPlan, FileEdit, Position, PositionEncoding,
    Provenance, TextEdit, TextRange,
};
pub use path::{portable_path_key, validate_plan_path};
pub use validation::{ValidatedEditPlan, validate_edit_plan};

/// Crate version compiled into this library.
pub const VERSION: &str = env!("CARGO_PKG_VERSION");
