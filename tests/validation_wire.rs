use blazingly_json::{from_str, to_string};
use weavatrix_edit::{
    Completeness, EDIT_PLAN_SCHEMA, EditPlan, ErrorCode, FileEdit, MAX_PLAN_OPERATION_BYTES,
    Position, Provenance, TextEdit, TextRange,
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
fn accepts_the_frozen_schema_and_open_operation_names() {
    let plan = EditPlan::new("vendor_specific_refactor", vec![file("src/a.rs")]);
    assert_eq!(plan.schema_version, EDIT_PLAN_SCHEMA);
    assert!(plan.validate().is_ok());

    let mut wrong_schema = plan.clone();
    wrong_schema.schema_version = "weavatrix.edit-plan.v2".to_owned();
    assert_eq!(validation_code(&wrong_schema), ErrorCode::SchemaMismatch);

    let mut empty_operation = plan;
    empty_operation.operation.clear();
    assert_eq!(validation_code(&empty_operation), ErrorCode::InvalidPlan);

    let empty_files = EditPlan::new("rename_symbol", Vec::new());
    assert_eq!(validation_code(&empty_files), ErrorCode::InvalidPlan);
}

#[test]
fn operation_labels_obey_the_absolute_byte_budget() {
    let mut exact = plan_with_file("src/a.rs");
    exact.operation = "x".repeat(MAX_PLAN_OPERATION_BYTES);
    assert!(exact.validate().is_ok());

    let mut oversized = exact;
    oversized.operation.push('x');
    let error = oversized.validate().unwrap_err();
    assert_eq!(error.code(), ErrorCode::PlanTooLarge);

    oversized.operation = "x".repeat(9 * 1024 * 1024);
    let error = oversized.validate().unwrap_err();
    assert_eq!(error.code(), ErrorCode::PlanTooLarge);
}

#[test]
fn accepts_only_the_four_applicable_provenance_tiers() {
    for provenance in [
        Provenance::EXACT_LSP,
        Provenance::RESOLVED,
        Provenance::EXTRACTED,
        Provenance::LEXICAL_EXACT,
    ] {
        let plan = EditPlan::new(
            "rename_symbol",
            vec![FileEdit::new(
                "src/a.rs",
                SHA256,
                vec![edit_with_provenance(provenance)],
            )],
        );
        assert!(plan.validate().is_ok(), "{provenance}");
    }

    for provenance in ["INFERRED", "CONFLICT", "", "EXACT"] {
        let plan = EditPlan::new(
            "rename_symbol",
            vec![FileEdit::new(
                "src/a.rs",
                SHA256,
                vec![edit_with_provenance(provenance)],
            )],
        );
        assert_eq!(
            validation_code(&plan),
            ErrorCode::UnprovenEdit,
            "{provenance}"
        );
    }
}

#[test]
fn validates_completeness_when_present() {
    let mut plan = plan_with_file("src/a.rs");
    assert!(plan.validate().is_ok());

    for value in [Completeness::COMPLETE, Completeness::PARTIAL] {
        plan.completeness = Some(Completeness::new(value));
        assert!(plan.validate().is_ok(), "{value}");
    }

    plan.completeness = Some(Completeness::new("UNKNOWN"));
    assert_eq!(validation_code(&plan), ErrorCode::InvalidPlan);
}

#[test]
fn validates_edit_range_and_exact_before_after_contract() {
    let cases = [
        TextEdit::replace(
            TextRange::new(Position::new(0, 0), Position::new(1, 1)),
            "a",
            "b",
            Provenance::EXACT_LSP,
        ),
        TextEdit::replace(
            TextRange::new(Position::new(2, 0), Position::new(1, 0)),
            "a",
            "b",
            Provenance::EXACT_LSP,
        ),
        TextEdit::replace(
            TextRange::new(Position::new(1, 0), Position::new(1, 1)),
            "same",
            "same",
            Provenance::EXACT_LSP,
        ),
    ];

    for edit in cases {
        let plan = EditPlan::new(
            "rename_symbol",
            vec![FileEdit::new("src/a.rs", SHA256, vec![edit])],
        );
        assert_eq!(validation_code(&plan), ErrorCode::InvalidEdit);
    }
}

#[test]
fn rejects_non_u32_json_positions_before_plan_validation() {
    for start_char in ["-1", "1.5", "4294967296"] {
        let json = format!(
            r#"{{"schemaVersion":"weavatrix.edit-plan.v1","operation":"rename_symbol","files":[{{"path":"src/a.rs","sha256":"{SHA256}","edits":[{{"startLine":1,"startChar":{start_char},"endLine":1,"endChar":1,"before":"a","after":"b","provenance":"EXACT_LSP"}}]}}]}}"#
        );
        assert!(from_str::<EditPlan>(&json).is_err(), "{start_char}");
    }
}

#[test]
fn accepts_only_lowercase_hex_sha256() {
    assert!(plan_with_file("src/a.rs").validate().is_ok());

    for sha in [
        "a",
        "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa",
        "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa",
        "AAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAA",
        "gggggggggggggggggggggggggggggggggggggggggggggggggggggggggggggggg",
    ] {
        let plan = EditPlan::new(
            "rename_symbol",
            vec![FileEdit::new(
                "src/a.rs",
                sha,
                vec![edit_with_provenance(Provenance::EXACT_LSP)],
            )],
        );
        assert_eq!(validation_code(&plan), ErrorCode::InvalidFile, "{sha}");
    }
}

#[test]
fn preserves_envelope_file_and_edit_extensions_across_json_roundtrip() {
    let json = format!(
        r#"{{
            "schemaVersion":"weavatrix.edit-plan.v1",
            "operation":"custom_refactor",
            "createdAt":"2026-08-01T00:00:00Z",
            "graphRevision":"abc123",
            "completeness":"PARTIAL",
            "files":[{{
                "path":"src/a.ts",
                "sha256":"{SHA256}",
                "language":"typescript",
                "edits":[{{
                    "startLine":1,
                    "startChar":0,
                    "endLine":1,
                    "endChar":1,
                    "before":"a",
                    "after":"b",
                    "provenance":"EXACT_LSP",
                    "producer":{{"name":"js-oracle","version":1}}
                }}]
            }}],
            "uncertainReferences":[{{"path":"factory.ts","line":42}}],
            "x-vendor":{{"enabled":true}}
        }}"#
    );

    let plan: EditPlan = from_str(&json).unwrap();
    assert!(plan.validate().is_ok());
    assert!(plan.extensions.contains_key("createdAt"));
    assert!(plan.extensions.contains_key("uncertainReferences"));
    assert!(plan.extensions.contains_key("x-vendor"));
    assert!(plan.files[0].extensions.contains_key("language"));
    assert!(plan.files[0].edits[0].extensions.contains_key("producer"));

    let serialized = to_string(&plan).unwrap();
    let roundtrip: EditPlan = from_str(&serialized).unwrap();
    assert_eq!(roundtrip, plan);
}

#[test]
fn rejects_programmatic_extension_collisions() {
    let mut plan = plan_with_file("src/a.rs");
    plan.extensions
        .insert("schemaVersion".to_owned(), true.into());
    assert_eq!(validation_code(&plan), ErrorCode::InvalidPlan);

    let mut plan = plan_with_file("src/a.rs");
    plan.files[0]
        .extensions
        .insert("path".to_owned(), "shadow.rs".into());
    assert_eq!(validation_code(&plan), ErrorCode::InvalidFile);

    let mut plan = plan_with_file("src/a.rs");
    plan.files[0].edits[0]
        .extensions
        .insert("before".to_owned(), "shadow".into());
    assert_eq!(validation_code(&plan), ErrorCode::InvalidEdit);
}
