use std::collections::BTreeSet;

use blazingly_json::Value;

use crate::{
    error::{EditError, ErrorCode},
    limits::PlanLimits,
    model::{Completeness, EDIT_PLAN_SCHEMA, EditPlan, FileEdit, TextEdit},
    path::{portable_path_key, validate_plan_path},
};

/// Proof that an edit plan passed structural, evidence, path, and budget checks.
#[derive(Clone, Copy, Debug)]
pub struct ValidatedEditPlan<'plan> {
    plan: &'plan EditPlan,
    total_edits: usize,
    total_text_bytes: usize,
}

impl<'plan> ValidatedEditPlan<'plan> {
    #[must_use]
    pub const fn plan(self) -> &'plan EditPlan {
        self.plan
    }

    #[must_use]
    pub const fn total_edits(self) -> usize {
        self.total_edits
    }

    #[must_use]
    pub const fn total_text_bytes(self) -> usize {
        self.total_text_bytes
    }
}

pub fn validate_edit_plan(
    plan: &EditPlan,
    limits: PlanLimits,
) -> Result<ValidatedEditPlan<'_>, EditError> {
    if plan.schema_version != EDIT_PLAN_SCHEMA {
        return Err(EditError::new(
            ErrorCode::SchemaMismatch,
            format!("schemaVersion must be {EDIT_PLAN_SCHEMA}"),
        ));
    }
    validate_extension_keys(
        &plan.extensions,
        &["schemaVersion", "operation", "files", "completeness"],
        ErrorCode::InvalidPlan,
    )?;
    if plan.operation.is_empty() {
        return Err(EditError::new(
            ErrorCode::InvalidPlan,
            "plan.operation is required",
        ));
    }
    if plan.files.is_empty() {
        return Err(EditError::new(
            ErrorCode::InvalidPlan,
            "plan.files must be non-empty",
        ));
    }
    if plan.files.len() > limits.max_files {
        return Err(too_large(format!(
            "plan touches more than {} files",
            limits.max_files
        )));
    }
    validate_completeness(plan.completeness.as_ref())?;

    let mut exact_paths = BTreeSet::new();
    let mut portable_paths = BTreeSet::new();
    let mut total_edits = 0_usize;
    let mut total_text_bytes = 0_usize;

    for (file_index, file) in plan.files.iter().enumerate() {
        validate_file(file, file_index, limits)?;
        if !exact_paths.insert(file.path.as_str()) {
            return Err(EditError::new(
                ErrorCode::InvalidPlan,
                format!("duplicate file entry: {}", file.path),
            )
            .at_file(file_index));
        }
        if !portable_paths.insert(portable_path_key(&file.path)) {
            return Err(EditError::new(
                ErrorCode::InvalidPlan,
                format!(
                    "file path aliases another entry on a portable worktree: {}",
                    file.path
                ),
            )
            .at_file(file_index));
        }
        total_edits = total_edits
            .checked_add(file.edits.len())
            .ok_or_else(|| too_large("total edit count overflow"))?;
        if total_edits > limits.max_total_edits {
            return Err(too_large(format!(
                "plan contains more than {} total edits",
                limits.max_total_edits
            )));
        }
        for edit in &file.edits {
            total_text_bytes = total_text_bytes
                .checked_add(edit.before.len())
                .and_then(|size| size.checked_add(edit.after.len()))
                .ok_or_else(|| too_large("total edit text size overflow"))?;
            if total_text_bytes > limits.max_total_text_bytes {
                return Err(too_large(format!(
                    "plan edit text exceeds the {}-byte limit",
                    limits.max_total_text_bytes
                )));
            }
        }
    }

    Ok(ValidatedEditPlan {
        plan,
        total_edits,
        total_text_bytes,
    })
}

pub(crate) fn validate_text_edit(edit: &TextEdit, index: usize) -> Result<(), EditError> {
    validate_extension_keys(
        &edit.extensions,
        &[
            "startLine",
            "startChar",
            "endLine",
            "endChar",
            "before",
            "after",
            "provenance",
        ],
        ErrorCode::InvalidEdit,
    )
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
        return Err(EditError::new(
            ErrorCode::UnprovenEdit,
            format!("provenance {} is not applicable", edit.provenance.as_str()),
        )
        .at_edit(index));
    }
    Ok(())
}

fn validate_file(file: &FileEdit, file_index: usize, limits: PlanLimits) -> Result<(), EditError> {
    validate_extension_keys(
        &file.extensions,
        &["path", "sha256", "edits"],
        ErrorCode::InvalidFile,
    )
    .map_err(|error| error.at_file(file_index))?;
    validate_plan_path(&file.path, limits.max_path_bytes)
        .map_err(|error| error.at_file(file_index))?;
    if !valid_sha256(&file.sha256) {
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

fn validate_completeness(completeness: Option<&Completeness>) -> Result<(), EditError> {
    let Some(completeness) = completeness else {
        return Ok(());
    };
    if !matches!(
        completeness.as_str(),
        Completeness::COMPLETE | Completeness::PARTIAL
    ) {
        return Err(EditError::new(
            ErrorCode::InvalidPlan,
            "plan.completeness must be COMPLETE or PARTIAL",
        ));
    }
    Ok(())
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

fn too_large(message: impl Into<String>) -> EditError {
    EditError::new(ErrorCode::PlanTooLarge, message)
}

fn validate_extension_keys(
    extensions: &std::collections::BTreeMap<String, Value>,
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
