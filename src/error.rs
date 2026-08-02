use core::fmt;

use serde::{Deserialize, Serialize};

/// Resource ceilings for diagnostics collected without applying edits.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct DiagnosticLimits {
    /// Maximum number of diagnostics retained in one report.
    pub max_items: usize,
    /// Maximum UTF-8 bytes retained from each expected or actual text value.
    pub max_preview_bytes: usize,
}

impl Default for DiagnosticLimits {
    fn default() -> Self {
        Self {
            max_items: 32,
            max_preview_bytes: 256,
        }
    }
}

/// A half-open UTF-8 byte range in the immutable source revision.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ByteSpan {
    pub start: usize,
    pub end: usize,
}

impl ByteSpan {
    #[must_use]
    pub const fn new(start: usize, end: usize) -> Self {
        Self { start, end }
    }
}

/// A bounded diagnostic rendering of source text.
#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct TextPreview {
    byte_len: usize,
    text: String,
    truncated: bool,
}

impl TextPreview {
    fn new(text: &str, max_bytes: usize) -> Self {
        let mut end = text.len().min(max_bytes);
        while !text.is_char_boundary(end) {
            end -= 1;
        }
        Self {
            byte_len: text.len(),
            text: text[..end].to_owned(),
            truncated: end < text.len(),
        }
    }

    #[must_use]
    pub const fn byte_len(&self) -> usize {
        self.byte_len
    }

    #[must_use]
    pub fn text(&self) -> &str {
        &self.text
    }

    #[must_use]
    pub const fn is_truncated(&self) -> bool {
        self.truncated
    }
}

/// Structured evidence for an exact-before mismatch.
#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct MismatchDetails {
    source_range: ByteSpan,
    expected: TextPreview,
    actual: TextPreview,
}

/// Bounded result of checking a complete edit set without applying it.
#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ValidationReport {
    diagnostics: Vec<EditError>,
    total_diagnostics: usize,
    truncated: bool,
}

impl ValidationReport {
    pub(crate) fn new(diagnostics: Vec<EditError>, total_diagnostics: usize) -> Self {
        Self {
            truncated: diagnostics.len() < total_diagnostics,
            diagnostics,
            total_diagnostics,
        }
    }

    /// Returns true when the full checked set had no validation failures.
    #[must_use]
    pub const fn is_valid(&self) -> bool {
        self.total_diagnostics == 0
    }

    /// Diagnostics retained under [`DiagnosticLimits::max_items`].
    #[must_use]
    pub fn diagnostics(&self) -> &[EditError] {
        &self.diagnostics
    }

    /// Total failures observed, including omitted diagnostics.
    #[must_use]
    pub const fn total_diagnostics(&self) -> usize {
        self.total_diagnostics
    }

    /// Returns true when some observed diagnostics were omitted.
    #[must_use]
    pub const fn is_truncated(&self) -> bool {
        self.truncated
    }
}

impl MismatchDetails {
    #[must_use]
    pub const fn source_range(&self) -> ByteSpan {
        self.source_range
    }

    #[must_use]
    pub const fn expected(&self) -> &TextPreview {
        &self.expected
    }

    #[must_use]
    pub const fn actual(&self) -> &TextPreview {
        &self.actual
    }
}

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
    #[serde(skip_serializing_if = "Option::is_none")]
    mismatch: Option<Box<MismatchDetails>>,
}

impl EditError {
    pub(crate) fn new(code: ErrorCode, message: impl Into<String>) -> Self {
        Self {
            code,
            message: message.into(),
            file_index: None,
            edit_index: None,
            related_edit_index: None,
            mismatch: None,
        }
    }

    pub(crate) fn before_mismatch(
        source_range: ByteSpan,
        expected: &str,
        actual: &str,
        limits: DiagnosticLimits,
    ) -> Self {
        Self {
            code: ErrorCode::BeforeMismatch,
            message: "source text does not match the exact before guard".to_owned(),
            file_index: None,
            edit_index: None,
            related_edit_index: None,
            mismatch: Some(Box::new(MismatchDetails {
                source_range,
                expected: TextPreview::new(expected, limits.max_preview_bytes),
                actual: TextPreview::new(actual, limits.max_preview_bytes),
            })),
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

    /// Bounded expected/actual evidence for [`ErrorCode::BeforeMismatch`].
    #[must_use]
    pub fn mismatch(&self) -> Option<&MismatchDetails> {
        self.mismatch.as_deref()
    }
}

impl fmt::Display for EditError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(formatter, "{}: {}", self.code, self.message)
    }
}

impl std::error::Error for EditError {}
