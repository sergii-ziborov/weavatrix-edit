use weavatrix_edit::{
    ApplyLimits, ByteEdit, DiagnosticLimits, ErrorCode, Position, PositionEncoding, Provenance,
    TextEdit, diagnose_byte_edits, diagnose_byte_edits_with_limits, diagnose_edits,
    diagnose_edits_with_encoding_and_limits,
};

use blazingly_json::to_string;

#[test]
fn mismatch_evidence_is_structured_and_strictly_bounded() {
    let source = "actual";
    let edits = [ByteEdit::replace(
        0..source.len(),
        "expected-secret-value",
        "replacement",
        Provenance::EXACT_LSP,
    )];
    let report = diagnose_byte_edits_with_limits(
        source,
        &edits,
        ApplyLimits::default(),
        DiagnosticLimits {
            max_items: 8,
            max_preview_bytes: 5,
        },
    );

    assert!(!report.is_valid());
    assert_eq!(report.total_diagnostics(), 1);
    let error = &report.diagnostics()[0];
    assert_eq!(error.code(), ErrorCode::BeforeMismatch);
    assert!(!error.message().contains("expected-secret-value"));
    let mismatch = error.mismatch().unwrap();
    assert_eq!(mismatch.source_range().start, 0);
    assert_eq!(mismatch.source_range().end, source.len());
    assert_eq!(mismatch.expected().byte_len(), 21);
    assert_eq!(mismatch.expected().text(), "expec");
    assert!(mismatch.expected().is_truncated());
    assert_eq!(mismatch.actual().text(), "actua");
    assert!(mismatch.actual().is_truncated());

    let wire = to_string(&report).unwrap();
    assert!(!wire.contains("expected-secret-value"));
    assert!(wire.contains(r#""text":"expec""#));
    assert!(wire.contains(r#""byteLen":21"#));
}

#[test]
fn report_counts_multiple_failures_and_bounds_retained_items() {
    let edits = [
        ByteEdit::replace(0..2, "wrong-one", "X", Provenance::RESOLVED),
        ByteEdit::replace(1..3, "wrong-two", "Y", Provenance::EXACT_LSP),
    ];
    let report = diagnose_byte_edits_with_limits(
        "abcdef",
        &edits,
        ApplyLimits::default(),
        DiagnosticLimits {
            max_items: 2,
            max_preview_bytes: 4,
        },
    );

    assert_eq!(report.total_diagnostics(), 3);
    assert_eq!(report.diagnostics().len(), 2);
    assert!(report.is_truncated());
    assert!(
        report
            .diagnostics()
            .iter()
            .all(|diagnostic| diagnostic.code() == ErrorCode::BeforeMismatch)
    );
}

#[test]
fn text_diagnostics_continue_after_an_invalid_position() {
    let edits = [
        TextEdit::insert(Position::new(9, 0), "x", Provenance::EXACT_LSP),
        TextEdit::replace(
            weavatrix_edit::TextRange::new(Position::new(1, 0), Position::new(1, 1)),
            "wrong",
            "y",
            Provenance::RESOLVED,
        ),
    ];
    let report = diagnose_edits("abc", &edits);

    assert_eq!(report.total_diagnostics(), 2);
    assert_eq!(
        report.diagnostics()[0].code(),
        ErrorCode::PositionOutOfRange
    );
    assert_eq!(report.diagnostics()[1].code(), ErrorCode::BeforeMismatch);
}

#[test]
fn diagnostics_cover_structural_resource_and_output_failures() {
    let source_limited = ApplyLimits {
        max_source_bytes: 0,
        ..ApplyLimits::default()
    };
    let text_limit = diagnose_edits_with_encoding_and_limits(
        "a",
        &[],
        PositionEncoding::Utf16,
        source_limited,
        DiagnosticLimits::default(),
    );
    assert_eq!(text_limit.diagnostics()[0].code(), ErrorCode::PlanTooLarge);

    let byte_limit =
        diagnose_byte_edits_with_limits("a", &[], source_limited, DiagnosticLimits::default());
    assert_eq!(byte_limit.diagnostics()[0].code(), ErrorCode::PlanTooLarge);

    let invalid_byte = diagnose_byte_edits(
        "a",
        &[ByteEdit::replace(0..1, "a", "a", Provenance::EXACT_LSP)],
    );
    assert_eq!(invalid_byte.diagnostics()[0].code(), ErrorCode::InvalidEdit);

    let output_limited = diagnose_byte_edits_with_limits(
        "a",
        &[ByteEdit::insert(0, "xx", Provenance::EXACT_LSP)],
        ApplyLimits {
            max_output_bytes: 1,
            ..ApplyLimits::default()
        },
        DiagnosticLimits::default(),
    );
    assert_eq!(
        output_limited.diagnostics()[0].code(),
        ErrorCode::OutputTooLarge
    );

    let invalid_text = diagnose_edits_with_encoding_and_limits(
        "a",
        &[
            TextEdit::insert(Position::new(0, 0), "x", Provenance::EXACT_LSP),
            TextEdit::replace(
                weavatrix_edit::TextRange::new(Position::new(1, 0), Position::new(1, 99)),
                "a",
                "b",
                Provenance::EXACT_LSP,
            ),
        ],
        PositionEncoding::Utf16,
        ApplyLimits::default(),
        DiagnosticLimits::default(),
    );
    assert_eq!(invalid_text.total_diagnostics(), 2);
    assert_eq!(invalid_text.diagnostics()[0].code(), ErrorCode::InvalidEdit);
    assert_eq!(
        invalid_text.diagnostics()[1].code(),
        ErrorCode::PositionOutOfRange
    );
}
