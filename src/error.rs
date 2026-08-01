use core::fmt;

use serde::{Deserialize, Serialize};

/// Stable machine-readable failure categories.
#[derive(Clone, Copy, Debug, Deserialize, Eq, Hash, PartialEq, Serialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum ErrorCode {
    SchemaMismatch,
    InvalidPlan,
    InvalidFile,
    InvalidEdit,
    InvalidPath,
    UnprovenEdit,
    PlanTooLarge,
    PositionOutOfRange,
    BeforeMismatch,
    OverlappingEdits,
    OutputTooLarge,
    ValidationRejected,
}

impl ErrorCode {
    /// Returns the wire-compatible status code.
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::SchemaMismatch => "SCHEMA_MISMATCH",
            Self::InvalidPlan => "INVALID_PLAN",
            Self::InvalidFile => "INVALID_FILE",
            Self::InvalidEdit => "INVALID_EDIT",
            Self::InvalidPath => "INVALID_PATH",
            Self::UnprovenEdit => "UNPROVEN_EDIT",
            Self::PlanTooLarge => "PLAN_TOO_LARGE",
            Self::PositionOutOfRange => "POSITION_OUT_OF_RANGE",
            Self::BeforeMismatch => "BEFORE_MISMATCH",
            Self::OverlappingEdits => "OVERLAPPING_EDITS",
            Self::OutputTooLarge => "OUTPUT_TOO_LARGE",
            Self::ValidationRejected => "VALIDATION_REJECTED",
        }
    }
}

impl fmt::Display for ErrorCode {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(self.as_str())
    }
}

/// A fail-closed validation or application error.
#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct EditError {
    code: ErrorCode,
    message: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    file_index: Option<usize>,
    #[serde(skip_serializing_if = "Option::is_none")]
    edit_index: Option<usize>,
    #[serde(skip_serializing_if = "Option::is_none")]
    related_edit_index: Option<usize>,
}

impl EditError {
    pub(crate) fn new(code: ErrorCode, message: impl Into<String>) -> Self {
        Self {
            code,
            message: message.into(),
            file_index: None,
            edit_index: None,
            related_edit_index: None,
        }
    }

    pub(crate) const fn at_file(mut self, file_index: usize) -> Self {
        self.file_index = Some(file_index);
        self
    }

    pub(crate) const fn at_edit(mut self, edit_index: usize) -> Self {
        self.edit_index = Some(edit_index);
        self
    }

    pub(crate) const fn with_related_edit(mut self, edit_index: usize) -> Self {
        self.related_edit_index = Some(edit_index);
        self
    }

    /// Stable machine-readable code.
    #[must_use]
    pub const fn code(&self) -> ErrorCode {
        self.code
    }

    /// Human-readable diagnostic. Consumers should branch on [`Self::code`].
    #[must_use]
    pub fn message(&self) -> &str {
        &self.message
    }

    /// File index within the plan, when applicable.
    #[must_use]
    pub const fn file_index(&self) -> Option<usize> {
        self.file_index
    }

    /// Edit index within the file or input slice, when applicable.
    #[must_use]
    pub const fn edit_index(&self) -> Option<usize> {
        self.edit_index
    }

    /// The other edit participating in an overlap, when applicable.
    #[must_use]
    pub const fn related_edit_index(&self) -> Option<usize> {
        self.related_edit_index
    }
}

impl fmt::Display for EditError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(formatter, "{}: {}", self.code, self.message)
    }
}

impl std::error::Error for EditError {}
