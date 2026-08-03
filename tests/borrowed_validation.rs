use std::collections::BTreeMap;

use weavatrix_edit::{
    BorrowedFileEdit, EditPlan, ErrorCode, FILE_EDIT_RESERVED_EXTENSION_KEYS, FileEdit,
    MAX_PLAN_OPERATION_BYTES, PlanLimits, Position, Provenance, TextEdit, TextRange,
    validate_file_edits,
};

fn edit() -> TextEdit {
    TextEdit::replace(
        TextRange::new(Position::new(1, 0), Position::new(1, 1)),
        "a",
        "b",
        Provenance::EXACT_LSP,
    )
}

fn file(path: &str) -> FileEdit {
    FileEdit::new(path, "0".repeat(64), vec![edit()])
}

fn borrowed(files: &[FileEdit]) -> Vec<BorrowedFileEdit<'_>> {
    files.iter().map(BorrowedFileEdit::from).collect()
}

#[test]
fn borrowed_views_reuse_original_text_and_return_owned_stats() {
    let files = vec![file("src/a.rs"), file("src/b.rs")];
    let views = borrowed(&files);
    assert!(std::ptr::eq(
        views[0].edits.as_ptr(),
        files[0].edits.as_ptr()
    ));
    assert!(std::ptr::eq(views[0].path.as_ptr(), files[0].path.as_ptr()));

    let stats = validate_file_edits("rename", &views, PlanLimits::default()).unwrap();
    assert_eq!(stats.total_edits(), 2);
    assert_eq!(stats.total_text_bytes(), 4);
}

#[test]
fn edit_plan_and_borrowed_entrypoint_share_file_semantics() {
    let mut cases = Vec::new();
    cases.push(vec![file("src/a.rs")]);

    let mut bad_hash = file("src/a.rs");
    bad_hash.sha256 = "BAD".to_owned();
    cases.push(vec![bad_hash]);

    let mut empty = file("src/a.rs");
    empty.edits.clear();
    cases.push(vec![empty]);

    let mut unproven = file("src/a.rs");
    unproven.edits[0].provenance = Provenance::new("UNKNOWN");
    cases.push(vec![unproven]);

    cases.push(vec![file("Src/A.rs"), file("src/a.rs")]);
    cases.push(vec![file("../escape.rs")]);

    for files in cases {
        let plan = EditPlan::new("rename", files.clone());
        let plan_result = plan.validate_with(PlanLimits::default());
        let views = borrowed(&files);
        let borrowed_result = validate_file_edits("rename", &views, PlanLimits::default());
        assert_eq!(
            plan_result
                .as_ref()
                .ok()
                .map(|value| (value.total_edits(), value.total_text_bytes())),
            borrowed_result
                .as_ref()
                .ok()
                .map(|value| (value.total_edits(), value.total_text_bytes()))
        );
        assert_eq!(
            plan_result
                .as_ref()
                .err()
                .map(weavatrix_edit::EditError::code),
            borrowed_result
                .as_ref()
                .err()
                .map(weavatrix_edit::EditError::code)
        );
    }
}

#[test]
fn arbitrary_borrowed_view_supports_rename_like_sources() {
    let edits = vec![edit()];
    let extensions = BTreeMap::new();
    let hash = "1".repeat(64);
    let view = BorrowedFileEdit {
        path: "old.rs",
        sha256: &hash,
        edits: &edits,
        extensions: &extensions,
        reserved_extension_keys: &["from", "to", "expectedSourceSha256", "edits"],
    };
    let stats = validate_file_edits("move_file", &[view], PlanLimits::default()).unwrap();
    assert_eq!(stats.total_edits(), 1);
}

#[test]
fn operation_and_aggregate_budgets_match_edit_plan_contract() {
    let files = vec![file("a.rs"), file("b.rs")];
    let views = borrowed(&files);
    assert!(validate_file_edits("x", &views, PlanLimits::default()).is_ok());
    assert!(validate_file_edits("", &views, PlanLimits::default()).is_err());
    assert!(
        validate_file_edits(
            &"x".repeat(MAX_PLAN_OPERATION_BYTES + 1),
            &views,
            PlanLimits::default()
        )
        .is_err()
    );
    let error = validate_file_edits(
        "x",
        &views,
        PlanLimits {
            max_files: 1,
            ..PlanLimits::default()
        },
    )
    .unwrap_err();
    assert_eq!(error.code(), ErrorCode::PlanTooLarge);
}

#[test]
fn borrowed_extensions_keep_reserved_field_checks() {
    let mut invalid = file("a.rs");
    invalid
        .extensions
        .insert("path".to_owned(), blazingly_json::Value::Null);
    let views = borrowed(std::slice::from_ref(&invalid));
    assert_eq!(
        validate_file_edits("x", &views, PlanLimits::default())
            .unwrap_err()
            .code(),
        ErrorCode::InvalidFile
    );
}

#[test]
fn borrowed_extensions_use_the_declared_envelope_contract() {
    let edits = vec![edit()];
    let hash = "1".repeat(64);
    let mut extensions = BTreeMap::new();
    extensions.insert("path".to_owned(), blazingly_json::Value::Null);
    extensions.insert("sha256".to_owned(), blazingly_json::Value::Null);
    let rename_keys = &["from", "to", "expectedSourceSha256", "edits"];
    let rename = BorrowedFileEdit {
        path: "old.rs",
        sha256: &hash,
        edits: &edits,
        extensions: &extensions,
        reserved_extension_keys: rename_keys,
    };
    assert!(validate_file_edits("rename", &[rename], PlanLimits::default()).is_ok());

    let mut invalid_extensions = extensions.clone();
    invalid_extensions.insert("from".to_owned(), blazingly_json::Value::Null);
    let invalid_rename = BorrowedFileEdit {
        extensions: &invalid_extensions,
        ..rename
    };
    assert_eq!(
        validate_file_edits("rename", &[invalid_rename], PlanLimits::default())
            .unwrap_err()
            .code(),
        ErrorCode::InvalidFile
    );

    let file_contract = BorrowedFileEdit {
        reserved_extension_keys: FILE_EDIT_RESERVED_EXTENSION_KEYS,
        ..rename
    };
    assert_eq!(
        validate_file_edits("modify", &[file_contract], PlanLimits::default())
            .unwrap_err()
            .code(),
        ErrorCode::InvalidFile
    );
}
