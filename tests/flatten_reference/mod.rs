//! Byte-for-byte reference copy of the pre-0.1.6 envelope model.
#![allow(dead_code)] // Each test binary uses its own subset of this module.
//!
//! Until 0.1.5, `TextEdit`, `FileEdit`, and `EditPlan` derived their serde
//! implementations with `#[serde(flatten)]` extension maps. The manual
//! implementations that replaced them must stay observably identical, so this
//! module keeps the old derive-based model verbatim for direct comparisons of
//! serialized bytes, decoded values, and error messages.

use std::collections::BTreeMap;

use blazingly_json::Value;
use serde::{Deserialize, Serialize};
use weavatrix_edit::{Completeness, EditPlan, FileEdit, Provenance, TextEdit};

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct FlatTextEdit {
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

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct FlatFileEdit {
    pub path: String,
    pub sha256: String,
    pub edits: Vec<FlatTextEdit>,
    #[serde(flatten, default, skip_serializing_if = "BTreeMap::is_empty")]
    pub extensions: BTreeMap<String, Value>,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct FlatEditPlan {
    pub schema_version: String,
    pub operation: String,
    pub files: Vec<FlatFileEdit>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub completeness: Option<Completeness>,
    #[serde(flatten, default, skip_serializing_if = "BTreeMap::is_empty")]
    pub extensions: BTreeMap<String, Value>,
}

pub fn to_text_edit(reference: FlatTextEdit) -> TextEdit {
    TextEdit {
        start_line: reference.start_line,
        start_char: reference.start_char,
        end_line: reference.end_line,
        end_char: reference.end_char,
        before: reference.before,
        after: reference.after,
        provenance: reference.provenance,
        extensions: reference.extensions,
    }
}

pub fn to_file_edit(reference: FlatFileEdit) -> FileEdit {
    FileEdit {
        path: reference.path,
        sha256: reference.sha256,
        edits: reference.edits.into_iter().map(to_text_edit).collect(),
        extensions: reference.extensions,
    }
}

pub fn to_edit_plan(reference: FlatEditPlan) -> EditPlan {
    EditPlan {
        schema_version: reference.schema_version,
        operation: reference.operation,
        files: reference.files.into_iter().map(to_file_edit).collect(),
        completeness: reference.completeness,
        extensions: reference.extensions,
    }
}

pub fn from_text_edit(production: &TextEdit) -> FlatTextEdit {
    FlatTextEdit {
        start_line: production.start_line,
        start_char: production.start_char,
        end_line: production.end_line,
        end_char: production.end_char,
        before: production.before.clone(),
        after: production.after.clone(),
        provenance: production.provenance.clone(),
        extensions: production.extensions.clone(),
    }
}

pub fn from_file_edit(production: &FileEdit) -> FlatFileEdit {
    FlatFileEdit {
        path: production.path.clone(),
        sha256: production.sha256.clone(),
        edits: production.edits.iter().map(from_text_edit).collect(),
        extensions: production.extensions.clone(),
    }
}

pub fn from_edit_plan(production: &EditPlan) -> FlatEditPlan {
    FlatEditPlan {
        schema_version: production.schema_version.clone(),
        operation: production.operation.clone(),
        files: production.files.iter().map(from_file_edit).collect(),
        completeness: production.completeness.clone(),
        extensions: production.extensions.clone(),
    }
}
