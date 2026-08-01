use weavatrix_edit::{
    EditPlan, ErrorCode, FileEdit, PlanLimits, Position, Provenance, TextEdit, TextRange,
    validate_plan_path,
};

const SHA256: &str = "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa";

fn edit_with_provenance(provenance: &str) -> TextEdit {
    TextEdit::replace(
        TextRange::new(Position::new(1, 0), Position::new(1, 1)),
        "a",
        "b",
        provenance,
    )
}

fn file(path: impl Into<String>) -> FileEdit {
    FileEdit::new(
        path,
        SHA256,
        vec![edit_with_provenance(Provenance::EXACT_LSP)],
    )
}

fn plan_with_file(path: &str) -> EditPlan {
    EditPlan::new("rename_symbol", vec![file(path)])
}

fn validation_code(plan: &EditPlan) -> ErrorCode {
    plan.validate().unwrap_err().code()
}

#[test]
fn rejects_unsafe_and_nonportable_paths() {
    for path in [
        "",
        "/a.rs",
        "a\\b.rs",
        "C:/a.rs",
        "a//b.rs",
        "a/./b.rs",
        "a/../b.rs",
        ".GIT/config",
        "src/.git./hooks.rs",
        ".git /x",
        "a.rs:stream",
        "src./a.rs",
        "src/a.rs ",
        "NUL",
        "con.txt",
        "COM1.rs",
        "src/\0bad.rs",
        "src/\u{7f}bad.rs",
    ] {
        let error = validate_plan_path(path, 4_096).unwrap_err();
        assert_eq!(error.code(), ErrorCode::InvalidPath, "{path:?}");
    }

    for path in [
        "src/lib.rs",
        "\u{65e5}\u{672c}\u{8a9e}/\u{1f600}.rs",
        "foo..bar/a.rs",
    ] {
        assert!(validate_plan_path(path, 4_096).is_ok(), "{path:?}");
    }
}

#[test]
fn rejects_exact_and_portable_path_aliases() {
    let exact = EditPlan::new("rename_symbol", vec![file("src/a.rs"), file("src/a.rs")]);
    assert_eq!(validation_code(&exact), ErrorCode::InvalidPlan);

    let case_alias = EditPlan::new(
        "rename_symbol",
        vec![file("Src/Foo.rs"), file("src/foo.RS")],
    );
    assert_eq!(validation_code(&case_alias), ErrorCode::InvalidPlan);
}

#[test]
fn default_file_and_per_file_edit_limits_match_the_js_contract() {
    let at_file_limit = EditPlan::new(
        "rename_symbol",
        (0..500)
            .map(|index| file(format!("src/{index}.rs")))
            .collect(),
    );
    assert!(at_file_limit.validate().is_ok());

    let over_file_limit = EditPlan::new(
        "rename_symbol",
        (0..501)
            .map(|index| file(format!("src/{index}.rs")))
            .collect(),
    );
    assert_eq!(validation_code(&over_file_limit), ErrorCode::PlanTooLarge);

    let at_edit_limit = EditPlan::new(
        "rename_symbol",
        vec![FileEdit::new(
            "src/a.rs",
            SHA256,
            (0..2_000)
                .map(|_| edit_with_provenance(Provenance::EXACT_LSP))
                .collect(),
        )],
    );
    assert!(at_edit_limit.validate().is_ok());

    let over_edit_limit = EditPlan::new(
        "rename_symbol",
        vec![FileEdit::new(
            "src/a.rs",
            SHA256,
            (0..2_001)
                .map(|_| edit_with_provenance(Provenance::EXACT_LSP))
                .collect(),
        )],
    );
    assert_eq!(validation_code(&over_edit_limit), ErrorCode::PlanTooLarge);
}

#[test]
fn enforces_global_edit_text_and_path_budgets() {
    let two_files = EditPlan::new("rename_symbol", vec![file("src/a.rs"), file("src/b.rs")]);
    let one_total_edit = PlanLimits {
        max_total_edits: 1,
        ..PlanLimits::default()
    };
    assert_eq!(
        two_files.validate_with(one_total_edit).unwrap_err().code(),
        ErrorCode::PlanTooLarge
    );

    let plan = plan_with_file("src/a.rs");
    let exact_text_budget = PlanLimits {
        max_total_text_bytes: 2,
        ..PlanLimits::default()
    };
    assert!(plan.validate_with(exact_text_budget).is_ok());
    let short_text_budget = PlanLimits {
        max_total_text_bytes: 1,
        ..PlanLimits::default()
    };
    assert_eq!(
        plan.validate_with(short_text_budget).unwrap_err().code(),
        ErrorCode::PlanTooLarge
    );

    assert!(validate_plan_path("a.rs", 4).is_ok());
    assert_eq!(
        validate_plan_path("a.rs", 3).unwrap_err().code(),
        ErrorCode::InvalidPath
    );
}
