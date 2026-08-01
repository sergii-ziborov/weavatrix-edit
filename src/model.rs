use std::collections::BTreeMap;

use blazingly_json::Value;
use serde::{Deserialize, Serialize};

use crate::{
    error::EditError,
    limits::PlanLimits,
    validation::{ValidatedEditPlan, validate_edit_plan},
};

pub use crate::provenance::Provenance;

/// Frozen JSON contract consumed by Weavatrix Refactor.
pub const EDIT_PLAN_SCHEMA: &str = "weavatrix.edit-plan.v1";

/// A 1-based line and 0-based UTF-16 code-unit position.
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct Position {
    pub line: u32,
    pub character: u32,
}

/// Character-unit convention used for line/character conversion.
#[derive(Clone, Copy, Debug, Default, Eq, Hash, PartialEq)]
pub enum PositionEncoding {
    Utf8,
    #[default]
    Utf16,
    Utf32,
}

impl Position {
    #[must_use]
    pub const fn new(line: u32, character: u32) -> Self {
        Self { line, character }
    }
}

/// A half-open source range.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub struct TextRange {
    pub start: Position,
    pub end: Position,
}

impl TextRange {
    #[must_use]
    pub const fn new(start: Position, end: Position) -> Self {
        Self { start, end }
    }

    #[must_use]
    pub const fn empty(position: Position) -> Self {
        Self::new(position, position)
    }
}

/// Completeness claim made by a planner.
#[derive(Clone, Debug, Eq, Hash, PartialEq, Serialize, Deserialize)]
#[serde(transparent)]
pub struct Completeness(pub String);

impl Completeness {
    pub const COMPLETE: &'static str = "COMPLETE";
    pub const PARTIAL: &'static str = "PARTIAL";

    #[must_use]
    pub fn new(value: impl Into<String>) -> Self {
        Self(value.into())
    }

    #[must_use]
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

/// One exact replacement over the original source text.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TextEdit {
    pub start_line: u32,
    pub start_char: u32,
    pub end_line: u32,
    pub end_char: u32,
    pub before: String,
    pub after: String,
    pub provenance: Provenance,
    #[serde(flatten, default, skip_serializing_if = "BTreeMap::is_empty")]
    pub extensions: BTreeMap<String, Value>,
}

/// A strict UTF-8 byte-range edit for high-throughput prepared application.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ByteEdit {
    pub start: usize,
    pub end: usize,
    pub before: String,
    pub after: String,
    pub provenance: Provenance,
}

impl ByteEdit {
    #[must_use]
    pub fn replace(
        range: core::ops::Range<usize>,
        before: impl Into<String>,
        after: impl Into<String>,
        provenance: impl AsRef<str>,
    ) -> Self {
        Self {
            start: range.start,
            end: range.end,
            before: before.into(),
            after: after.into(),
            provenance: Provenance::new(provenance),
        }
    }

    #[must_use]
    pub fn insert(offset: usize, after: impl Into<String>, provenance: impl AsRef<str>) -> Self {
        Self::replace(offset..offset, "", after, provenance)
    }

    #[must_use]
    pub fn delete(
        range: core::ops::Range<usize>,
        before: impl Into<String>,
        provenance: impl AsRef<str>,
    ) -> Self {
        Self::replace(range, before, "", provenance)
    }
}

impl TextEdit {
    #[must_use]
    pub fn replace(
        range: TextRange,
        before: impl Into<String>,
        after: impl Into<String>,
        provenance: impl AsRef<str>,
    ) -> Self {
        Self {
            start_line: range.start.line,
            start_char: range.start.character,
            end_line: range.end.line,
            end_char: range.end.character,
            before: before.into(),
            after: after.into(),
            provenance: Provenance::new(provenance),
            extensions: BTreeMap::new(),
        }
    }

    #[must_use]
    pub fn insert(
        position: Position,
        after: impl Into<String>,
        provenance: impl AsRef<str>,
    ) -> Self {
        Self::replace(TextRange::empty(position), "", after, provenance)
    }

    #[must_use]
    pub fn delete(
        range: TextRange,
        before: impl Into<String>,
        provenance: impl AsRef<str>,
    ) -> Self {
        Self::replace(range, before, "", provenance)
    }

    #[must_use]
    pub const fn range(&self) -> TextRange {
        TextRange::new(
            Position::new(self.start_line, self.start_char),
            Position::new(self.end_line, self.end_char),
        )
    }
}

/// All edits for one repository-relative UTF-8 source file.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct FileEdit {
    pub path: String,
    pub sha256: String,
    pub edits: Vec<TextEdit>,
    #[serde(flatten, default, skip_serializing_if = "BTreeMap::is_empty")]
    pub extensions: BTreeMap<String, Value>,
}

impl FileEdit {
    #[must_use]
    pub fn new(path: impl Into<String>, sha256: impl Into<String>, edits: Vec<TextEdit>) -> Self {
        Self {
            path: path.into(),
            sha256: sha256.into(),
            edits,
            extensions: BTreeMap::new(),
        }
    }
}

/// Versioned, extensible multi-file edit-plan envelope.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct EditPlan {
    pub schema_version: String,
    pub operation: String,
    pub files: Vec<FileEdit>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub completeness: Option<Completeness>,
    #[serde(flatten, default, skip_serializing_if = "BTreeMap::is_empty")]
    pub extensions: BTreeMap<String, Value>,
}

impl EditPlan {
    #[must_use]
    pub fn new(operation: impl Into<String>, files: Vec<FileEdit>) -> Self {
        Self {
            schema_version: EDIT_PLAN_SCHEMA.to_owned(),
            operation: operation.into(),
            files,
            completeness: None,
            extensions: BTreeMap::new(),
        }
    }

    pub fn validate(&self) -> Result<ValidatedEditPlan<'_>, EditError> {
        validate_edit_plan(self, PlanLimits::default())
    }

    pub fn validate_with(&self, limits: PlanLimits) -> Result<ValidatedEditPlan<'_>, EditError> {
        validate_edit_plan(self, limits)
    }
}
