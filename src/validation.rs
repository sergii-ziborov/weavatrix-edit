mod files;

use std::collections::BTreeMap;

use blazingly_json::Value;

use crate::{
    envelope::{EDIT_PLAN_FIELDS, FILE_EDIT_FIELDS},
    error::{EditError, ErrorCode},
    limits::{MAX_PLAN_OPERATION_BYTES, PlanLimits},
    model::{Completeness, EDIT_PLAN_SCHEMA, EditPlan, FileEdit, TextEdit},
};

pub(crate) use files::validate_text_edit;

/// Reserved JSON member names for a [`FileEdit`] extension map.
///
/// These are exactly the declared wire members of a `FileEdit`, so an
/// extension key can never silently shadow one.
pub const FILE_EDIT_RESERVED_EXTENSION_KEYS: &[&str] = &FILE_EDIT_FIELDS;

/// Zero-copy view of one exact file edit set.
#[derive(Clone, Copy, Debug)]
pub struct BorrowedFileEdit<'file> {
    pub path: &'file str,
    pub sha256: &'file str,
    pub edits: &'file [TextEdit],
    pub extensions: &'file BTreeMap<String, Value>,
    /// Member names which the source envelope owns and extensions may not shadow.
    pub reserved_extension_keys: &'file [&'file str],
}

impl<'file> From<&'file FileEdit> for BorrowedFileEdit<'file> {
    fn from(file: &'file FileEdit) -> Self {
        Self {
            path: &file.path,
            sha256: &file.sha256,
            edits: &file.edits,
            extensions: &file.extensions,
            reserved_extension_keys: FILE_EDIT_RESERVED_EXTENSION_KEYS,
        }
    }
}

/// Owned statistics produced by zero-copy file-edit validation.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct EditValidationStats {
    total_edits: usize,
    total_text_bytes: usize,
}

impl EditValidationStats {
    #[must_use]
    pub const fn total_edits(self) -> usize {
        self.total_edits
    }

    #[must_use]
    pub const fn total_text_bytes(self) -> usize {
        self.total_text_bytes
    }
}

/// Proof that an edit plan passed structural, evidence, path, and budget checks.
#[derive(Clone, Copy, Debug)]
pub struct ValidatedEditPlan<'plan> {
    plan: &'plan EditPlan,
    stats: EditValidationStats,
}

impl<'plan> ValidatedEditPlan<'plan> {
    #[must_use]
    pub const fn plan(self) -> &'plan EditPlan {
        self.plan
    }

    #[must_use]
    pub const fn total_edits(self) -> usize {
        self.stats.total_edits()
    }

    #[must_use]
    pub const fn total_text_bytes(self) -> usize {
        self.stats.total_text_bytes()
    }
}

/// Validates a frozen edit-plan envelope and its borrowed file/edit contents.
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
    files::validate_extension_keys(&plan.extensions, &EDIT_PLAN_FIELDS, ErrorCode::InvalidPlan)?;
    validate_collection(&plan.operation, plan.files.len(), limits)?;
    validate_completeness(plan.completeness.as_ref())?;
    let stats = files::validate_file_views(plan.files.iter().map(BorrowedFileEdit::from), limits)?;
    Ok(ValidatedEditPlan { plan, stats })
}

/// Validates arbitrary borrowed file edits with the same engine as [`EditPlan`].
///
/// This entry point owns no schema envelope or completeness claim. It validates
/// the operation label, file/edit structures, paths, hashes, provenance,
/// uniqueness, and every [`PlanLimits`] budget without cloning edit text.
pub fn validate_file_edits(
    operation: &str,
    files: &[BorrowedFileEdit<'_>],
    limits: PlanLimits,
) -> Result<EditValidationStats, EditError> {
    validate_collection(operation, files.len(), limits)?;
    files::validate_file_views(files.iter().copied(), limits)
}

fn validate_collection(
    operation: &str,
    file_count: usize,
    limits: PlanLimits,
) -> Result<(), EditError> {
    if operation.is_empty() {
        return Err(EditError::new(
            ErrorCode::InvalidPlan,
            "plan.operation is required",
        ));
    }
    if operation.len() > MAX_PLAN_OPERATION_BYTES {
        return Err(too_large(format!(
            "plan.operation exceeds the {MAX_PLAN_OPERATION_BYTES}-byte limit"
        )));
    }
    if file_count == 0 {
        return Err(EditError::new(
            ErrorCode::InvalidPlan,
            "plan.files must be non-empty",
        ));
    }
    if file_count > limits.max_files {
        return Err(too_large(format!(
            "plan touches more than {} files",
            limits.max_files
        )));
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

pub(super) fn too_large(message: impl Into<String>) -> EditError {
    EditError::new(ErrorCode::PlanTooLarge, message)
}
