use std::collections::{BTreeMap, BTreeSet};

use blazingly_json::Value;

use super::{BorrowedFileEdit, EditValidationStats, too_large};
use crate::{
    envelope::TEXT_EDIT_FIELDS,
    error::{EditError, ErrorCode},
    limits::PlanLimits,
    model::TextEdit,
    path::{portable_path_key, validate_plan_path},
};

pub(super) fn validate_file_views<'file, I>(
    files: I,
    limits: PlanLimits,
) -> Result<EditValidationStats, EditError>
where
    I: IntoIterator<Item = BorrowedFileEdit<'file>>,
{
    let mut exact_paths = BTreeSet::new();
    let mut portable_paths = BTreeSet::new();
    let mut total_edits = 0_usize;
    let mut total_text_bytes = 0_usize;
    for (file_index, file) in files.into_iter().enumerate() {
        validate_file(file, file_index, limits)?;
        validate_unique_paths(file, file_index, &mut exact_paths, &mut portable_paths)?;
        total_edits = total_edits
            .checked_add(file.edits.len())
            .ok_or_else(|| too_large("total edit count overflow"))?;
        if total_edits > limits.max_total_edits {
            return Err(too_large(format!(
                "plan contains more than {} total edits",
                limits.max_total_edits
            )));
        }
        total_text_bytes = count_text(file.edits, total_text_bytes, limits)?;
    }
    Ok(EditValidationStats {
        total_edits,
        total_text_bytes,
    })
}

pub(crate) fn validate_text_edit(edit: &TextEdit, index: usize) -> Result<(), EditError> {
    validate_extension_keys(&edit.extensions, &TEXT_EDIT_FIELDS, ErrorCode::InvalidEdit)
        .map_err(|error| error.at_edit(index))?;
    let range = edit.range();
    if range.start.line == 0 || range.end.line == 0 {
        return Err(invalid_edit("lines are 1-based", index));
    }
    if range.end < range.start {
        return Err(invalid_edit("edit end precedes its start", index));
    }
    if edit.before == edit.after {
        return Err(invalid_edit("before and after are identical", index));
    }
    if !edit.provenance.is_applicable() {
        return Err(
            EditError::new(ErrorCode::UnprovenEdit, "edit provenance is not applicable")
                .at_edit(index),
        );
    }
    Ok(())
}

fn validate_file(
    file: BorrowedFileEdit<'_>,
    file_index: usize,
    limits: PlanLimits,
) -> Result<(), EditError> {
    validate_extension_keys(
        file.extensions,
        file.reserved_extension_keys,
        ErrorCode::InvalidFile,
    )
    .map_err(|error| error.at_file(file_index))?;
    validate_plan_path(file.path, limits.max_path_bytes)
        .map_err(|error| error.at_file(file_index))?;
    if !valid_sha256(file.sha256) {
        return Err(EditError::new(
            ErrorCode::InvalidFile,
            "sha256 must be 64 lowercase hexadecimal characters",
        )
        .at_file(file_index));
    }
    if file.edits.is_empty() {
        return Err(
            EditError::new(ErrorCode::InvalidFile, "file edits must be non-empty")
                .at_file(file_index),
        );
    }
    if file.edits.len() > limits.max_edits_per_file {
        return Err(too_large(format!(
            "{} contains more than {} edits",
            file.path, limits.max_edits_per_file
        ))
        .at_file(file_index));
    }
    for (edit_index, edit) in file.edits.iter().enumerate() {
        validate_text_edit(edit, edit_index).map_err(|error| error.at_file(file_index))?;
    }
    Ok(())
}

fn validate_unique_paths<'a>(
    file: BorrowedFileEdit<'a>,
    file_index: usize,
    exact_paths: &mut BTreeSet<&'a str>,
    portable_paths: &mut BTreeSet<String>,
) -> Result<(), EditError> {
    if !exact_paths.insert(file.path) {
        return Err(EditError::new(
            ErrorCode::InvalidPlan,
            format!("duplicate file entry: {}", file.path),
        )
        .at_file(file_index));
    }
    if !portable_paths.insert(portable_path_key(file.path)) {
        return Err(EditError::new(
            ErrorCode::InvalidPlan,
            format!(
                "file path aliases another entry on a portable worktree: {}",
                file.path
            ),
        )
        .at_file(file_index));
    }
    Ok(())
}

fn count_text(
    edits: &[TextEdit],
    mut total: usize,
    limits: PlanLimits,
) -> Result<usize, EditError> {
    for edit in edits {
        total = total
            .checked_add(edit.before.len())
            .and_then(|size| size.checked_add(edit.after.len()))
            .ok_or_else(|| too_large("total edit text size overflow"))?;
        if total > limits.max_total_text_bytes {
            return Err(too_large(format!(
                "plan edit text exceeds the {}-byte limit",
                limits.max_total_text_bytes
            )));
        }
    }
    Ok(total)
}

fn valid_sha256(value: &str) -> bool {
    value.len() == 64
        && value
            .bytes()
            .all(|byte| byte.is_ascii_digit() || matches!(byte, b'a'..=b'f'))
}

fn invalid_edit(message: impl Into<String>, index: usize) -> EditError {
    EditError::new(ErrorCode::InvalidEdit, message).at_edit(index)
}

pub(super) fn validate_extension_keys(
    extensions: &BTreeMap<String, Value>,
    reserved: &[&str],
    code: ErrorCode,
) -> Result<(), EditError> {
    if let Some(key) = extensions
        .keys()
        .find(|key| reserved.contains(&key.as_str()))
    {
        return Err(EditError::new(
            code,
            format!("extension field {key:?} collides with a reserved field"),
        ));
    }
    Ok(())
}
