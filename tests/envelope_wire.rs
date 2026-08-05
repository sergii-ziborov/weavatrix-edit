//! Wire equivalence between the shipped envelope model and an independent
//! reference statement of the same wire shape.
//!
//! Every assertion runs through two independent serde drivers,
//! `blazingly_json` and `serde_json`, because the crate is public and callers
//! may pair the model with either. Success values, serialized bytes, and error
//! messages must all match the reference derive exactly.

mod flatten_reference;

use core::fmt::Debug;

use blazingly_json::Value;
use flatten_reference::{
    FlatEditPlan, FlatFileEdit, FlatTextEdit, from_edit_plan, to_edit_plan, to_file_edit,
    to_text_edit,
};
use serde::de::DeserializeOwned;
use weavatrix_edit::{
    Completeness, DeclaredEditPlan, EDIT_PLAN_SCHEMA, EditPlan, FileEdit, Position, Provenance,
    TextEdit, TextRange,
};

const SHA256: &str = "0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef";

/// The envelope example published in docs/edit-plan-v1.md.
const DOC_FIXTURE: &str = r#"{
  "schemaVersion": "weavatrix.edit-plan.v1",
  "operation": "rename_symbol",
  "completeness": "COMPLETE",
  "files": [
    {
      "path": "src/user.ts",
      "sha256": "0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef",
      "edits": [
        {
          "startLine": 10,
          "startChar": 8,
          "endLine": 10,
          "endChar": 15,
          "before": "getUser",
          "after": "getCustomer",
          "provenance": "EXACT_LSP"
        }
      ]
    }
  ],
  "createdAt": "2026-08-01T12:00:00Z"
}"#;

/// The extension-preserving fixture already exercised by `validation_wire.rs`.
fn extension_fixture() -> String {
    format!(
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
    )
}

/// Rewrites the reference struct names inside an error message so it can be
/// compared with production messages; the pre-0.1.6 derive reported the
/// production names.
fn normalize_reference_message(message: &str) -> String {
    message
        .replace("struct FlatEditPlan", "struct EditPlan")
        .replace("struct FlatFileEdit", "struct FileEdit")
        .replace("struct FlatTextEdit", "struct TextEdit")
}

/// Decodes `json` as production type `P` and reference type `R` through both
/// drivers and requires identical outcomes: equal values on success and equal
/// error messages on failure.
fn assert_decode_parity<P, R>(json: &str, convert: impl Fn(R) -> P) -> Option<P>
where
    P: DeserializeOwned + PartialEq + Debug,
    R: DeserializeOwned,
{
    let production = blazingly_json::from_slice::<P>(json.as_bytes());
    let reference = blazingly_json::from_slice::<R>(json.as_bytes());
    let blazingly_value = match (production, reference) {
        (Ok(production), Ok(reference)) => {
            let reference = convert(reference);
            assert_eq!(production, reference, "blazingly_json decode of {json}");
            Some(production)
        }
        (Err(production), Err(reference)) => {
            assert_eq!(
                production.to_string(),
                normalize_reference_message(&reference.to_string()),
                "blazingly_json error for {json}"
            );
            None
        }
        (production, reference) => panic!(
            "blazingly_json outcomes diverge for {json}: production ok={} reference ok={}",
            production.is_ok(),
            reference.is_ok()
        ),
    };

    let production = serde_json::from_slice::<P>(json.as_bytes());
    let reference = serde_json::from_slice::<R>(json.as_bytes());
    let serde_value = match (production, reference) {
        (Ok(production), Ok(reference)) => {
            let reference = convert(reference);
            assert_eq!(production, reference, "serde_json decode of {json}");
            Some(production)
        }
        (Err(production), Err(reference)) => {
            assert_eq!(
                production.to_string(),
                normalize_reference_message(&reference.to_string()),
                "serde_json error for {json}"
            );
            None
        }
        (production, reference) => panic!(
            "serde_json outcomes diverge for {json}: production ok={} reference ok={}",
            production.is_ok(),
            reference.is_ok()
        ),
    };

    match (blazingly_value, serde_value) {
        (Some(from_blazingly), Some(from_serde)) => {
            assert_eq!(from_blazingly, from_serde, "cross-driver decode of {json}");
            Some(from_blazingly)
        }
        (None, None) => None,
        _ => panic!("drivers disagree on validity of {json}"),
    }
}

/// Clears every extension map, giving the value a declared-only decode must
/// produce from the same document.
fn without_extensions(plan: &EditPlan) -> EditPlan {
    let mut plan = plan.clone();
    plan.extensions.clear();
    for file in &mut plan.files {
        file.extensions.clear();
        for edit in &mut file.edits {
            edit.extensions.clear();
        }
    }
    plan
}

/// `DeclaredEditPlan` is an additive decode path, not a second dialect. It must
/// accept exactly the documents `EditPlan` accepts, reject the rest with the
/// byte-identical message, and recover identical declared data on both drivers.
fn assert_declared_parity(json: &str, capturing: Option<&EditPlan>) {
    let blazingly = blazingly_json::from_slice::<DeclaredEditPlan>(json.as_bytes());
    let serde = serde_json::from_slice::<DeclaredEditPlan>(json.as_bytes());

    let Some(capturing) = capturing else {
        let expected = blazingly_json::from_slice::<EditPlan>(json.as_bytes())
            .expect_err("capturing decode rejects this document");
        assert_eq!(
            blazingly
                .expect_err("declared decode must reject it too")
                .to_string(),
            expected.to_string(),
            "blazingly_json declared-only error for {json}"
        );
        let expected = serde_json::from_slice::<EditPlan>(json.as_bytes())
            .expect_err("capturing decode rejects this document");
        assert_eq!(
            serde
                .expect_err("declared decode must reject it too")
                .to_string(),
            expected.to_string(),
            "serde_json declared-only error for {json}"
        );
        return;
    };

    let expected = without_extensions(capturing);
    assert_eq!(
        *blazingly
            .expect("declared decode accepts what the capturing decode accepts")
            .plan(),
        expected,
        "blazingly_json declared-only decode of {json}"
    );
    assert_eq!(
        *serde
            .expect("declared decode accepts what the capturing decode accepts")
            .plan(),
        expected,
        "serde_json declared-only decode of {json}"
    );
}

fn assert_plan_parity(json: &str) -> Option<EditPlan> {
    let plan = assert_decode_parity::<EditPlan, FlatEditPlan>(json, to_edit_plan);
    assert_declared_parity(json, plan.as_ref());
    plan
}

fn assert_file_parity(json: &str) -> Option<FileEdit> {
    assert_decode_parity::<FileEdit, FlatFileEdit>(json, to_file_edit)
}

fn assert_edit_parity(json: &str) -> Option<TextEdit> {
    assert_decode_parity::<TextEdit, FlatTextEdit>(json, to_text_edit)
}

/// Serializes one plan through both drivers and requires bytes identical to
/// the reference derive holding the same data.
fn assert_serialize_parity(plan: &EditPlan) {
    let reference = from_edit_plan(plan);
    assert_eq!(
        blazingly_json::to_string(plan).expect("production blazingly_json encoding"),
        blazingly_json::to_string(&reference).expect("reference blazingly_json encoding"),
        "blazingly_json bytes for {plan:?}"
    );
    assert_eq!(
        serde_json::to_string(plan).expect("production serde_json encoding"),
        serde_json::to_string(&reference).expect("reference serde_json encoding"),
        "serde_json bytes for {plan:?}"
    );
    assert_eq!(
        serde_json::to_string_pretty(plan).expect("production pretty encoding"),
        serde_json::to_string_pretty(&reference).expect("reference pretty encoding"),
        "serde_json pretty bytes for {plan:?}"
    );
}

fn value(json: &str) -> Value {
    blazingly_json::from_str(json).expect("test extension value parses")
}

fn base_edit() -> TextEdit {
    TextEdit::replace(
        TextRange::new(Position::new(10, 8), Position::new(10, 15)),
        "getUser",
        "getCustomer",
        Provenance::EXACT_LSP,
    )
}

fn base_plan() -> EditPlan {
    EditPlan::new(
        "rename_symbol",
        vec![FileEdit::new("src/user.ts", SHA256, vec![base_edit()])],
    )
}

fn decorated_plan() -> EditPlan {
    let mut edit = base_edit();
    edit.extensions.insert(
        "producer".to_owned(),
        value(r#"{"name":"js-oracle","version":1}"#),
    );
    edit.extensions
        .insert("confidence".to_owned(), value("0.98"));
    edit.extensions
        .insert("\u{1F600}".to_owned(), value(r#""astral-key""#));

    let mut file = FileEdit::new("src/\u{6A21}\u{5757}_0001.ts", SHA256, vec![edit]);
    file.extensions
        .insert("language".to_owned(), value(r#""typescript""#));
    file.extensions.insert(
        "unicodeLabel".to_owned(),
        value("\"caf\u{E9}_\u{301}_\u{1F680}\""),
    );

    let mut plan = EditPlan::new("custom_refactor_\u{1F680}", vec![file]);
    plan.completeness = Some(Completeness::new(Completeness::PARTIAL));
    plan.extensions
        .insert("createdAt".to_owned(), value(r#""2026-08-01T00:00:00Z""#));
    plan.extensions.insert(
        "benchmarkMetadata".to_owned(),
        value(r#"{"nested":{"values":[null,true,1.25,42],"emoji":"🧵"},"revision":"v1"}"#),
    );
    plan.extensions
        .insert("bigUnsigned".to_owned(), value("18446744073709551615"));
    plan.extensions
        .insert("mostNegative".to_owned(), value("-9223372036854775808"));
    plan.extensions
        .insert("nullValue".to_owned(), value("null"));
    plan.extensions
        .insert("\u{E000}".to_owned(), value(r#""private-use-key""#));
    plan.extensions
        .insert(String::new(), value(r#""empty-key""#));
    plan
}

#[test]
fn serialization_is_byte_identical_to_the_flatten_derive() {
    // Minimal plan: no completeness, no extensions anywhere.
    assert_serialize_parity(&base_plan());

    // Fully decorated plan with extensions at every level.
    assert_serialize_parity(&decorated_plan());

    // Structural corners: empty file list, file with no edits, empty strings.
    assert_serialize_parity(&EditPlan::new("noop", Vec::new()));
    let mut empty_edits = base_plan();
    empty_edits.files[0].edits.clear();
    empty_edits.files[0].path = String::new();
    assert_serialize_parity(&empty_edits);

    // Completeness present without any extensions.
    let mut complete = base_plan();
    complete.completeness = Some(Completeness::new(Completeness::COMPLETE));
    assert_serialize_parity(&complete);

    // Extension keys collide with declared names: the derive emitted the
    // duplicate JSON keys, so the manual implementation must as well.
    let mut collision = base_plan();
    collision
        .extensions
        .insert("operation".to_owned(), value(r#""shadow""#));
    assert_serialize_parity(&collision);
}

#[test]
fn serialized_extension_keys_follow_btreemap_order_not_insertion_order() {
    let mut plan = base_plan();
    plan.extensions.insert("zulu".to_owned(), value("1"));
    plan.extensions.insert("alpha".to_owned(), value("2"));
    plan.extensions.insert("Beta".to_owned(), value("3"));
    assert_serialize_parity(&plan);

    let encoded = blazingly_json::to_string(&plan).expect("plan encodes");
    let zulu = encoded.find("\"zulu\"").expect("zulu key present");
    let alpha = encoded.find("\"alpha\"").expect("alpha key present");
    let beta = encoded.find("\"Beta\"").expect("Beta key present");
    assert!(beta < alpha && alpha < zulu, "extensions follow map order");
}

#[test]
fn fixture_documents_decode_identically_and_roundtrip() {
    let doc = assert_plan_parity(DOC_FIXTURE).expect("doc fixture decodes");
    assert!(doc.validate().is_ok());
    assert_eq!(
        doc.extensions["createdAt"],
        value(r#""2026-08-01T12:00:00Z""#)
    );
    assert_serialize_parity(&doc);

    let extension = assert_plan_parity(&extension_fixture()).expect("extension fixture decodes");
    assert!(extension.validate().is_ok());
    assert!(extension.files[0].extensions.contains_key("language"));
    assert!(
        extension.files[0].edits[0]
            .extensions
            .contains_key("producer")
    );
    assert_serialize_parity(&extension);

    // Serialized bytes must decode back to the same value on both drivers.
    let encoded = blazingly_json::to_string(&extension).expect("plan encodes");
    let roundtrip = assert_plan_parity(&encoded).expect("roundtrip decodes");
    assert_eq!(roundtrip, extension);

    // from_str and from_slice expose the same behaviour.
    assert_eq!(
        blazingly_json::from_str::<EditPlan>(&encoded).expect("from_str decodes"),
        roundtrip
    );
}

#[test]
fn unknown_keys_are_collected_and_the_last_duplicate_unknown_key_wins() {
    let json = r#"{"schemaVersion":"weavatrix.edit-plan.v1","operation":"x","files":[],"vendor":1,"vendor":{"deep":[true,null]},"vendor":"final"}"#;
    let plan = assert_plan_parity(json).expect("duplicate unknown keys stay valid");
    assert_eq!(plan.extensions.len(), 1);
    assert_eq!(plan.extensions["vendor"], value(r#""final""#));

    // Declared names are matched exactly: snake_case and case variants are
    // extensions, as is a literal "extensions" key.
    let json = r#"{"schemaVersion":"weavatrix.edit-plan.v1","operation":"x","files":[],"schema_version":"v2","Operation":"y","extensions":{"nested":1}}"#;
    let plan = assert_plan_parity(json).expect("near-miss keys stay extensions");
    assert_eq!(
        plan.extensions.keys().collect::<Vec<_>>(),
        ["Operation", "extensions", "schema_version"]
    );
    assert_eq!(plan.schema_version, EDIT_PLAN_SCHEMA);
}

#[test]
fn escaped_declared_keys_match_like_the_derive() {
    // Backslash built from its code point so the JSON below carries real
    // `\uXXXX` escape sequences.
    let esc = char::from(92u8);

    // `{esc}u0073tartLine` unescapes to "startLine" and must hit the declared
    // field, exactly as the derive's owned-string key path did.
    let json = format!(
        r#"{{"{esc}u0073tartLine":1,"startChar":0,"endLine":1,"endChar":2,"before":"ab","after":"cd","provenance":"EXACT_LSP"}}"#
    );
    let edit = assert_edit_parity(&json).expect("escaped declared key decodes");
    assert_eq!(edit.start_line, 1);
    assert!(edit.extensions.is_empty());

    // An escaped spelling duplicates the plain spelling of the same field.
    let duplicated = format!(
        r#"{{"startLine":1,"{esc}u0073tartLine":2,"startChar":0,"endLine":1,"endChar":2,"before":"ab","after":"cd","provenance":"EXACT_LSP"}}"#
    );
    assert!(assert_edit_parity(&duplicated).is_none());

    // Escaped unknown keys, including astral surrogate pairs, stay extensions.
    let unknown = format!(
        r#"{{"startLine":1,"startChar":0,"endLine":1,"endChar":2,"before":"ab","after":"cd","provenance":"EXACT_LSP","{esc}ud83d{esc}ude00":"emoji","{esc}u0301combining":true}}"#
    );
    let edit = assert_edit_parity(&unknown).expect("escaped unknown keys decode");
    assert!(edit.extensions.contains_key("\u{1F600}"));
    assert!(edit.extensions.contains_key("\u{301}combining"));
}

#[test]
fn duplicate_declared_fields_fail_with_the_derive_message() {
    let plan_cases = [
        r#"{"schemaVersion":"a","schemaVersion":"b","operation":"x","files":[]}"#,
        r#"{"schemaVersion":"a","operation":"x","operation":"y","files":[]}"#,
        r#"{"schemaVersion":"a","operation":"x","files":[],"files":[]}"#,
        r#"{"schemaVersion":"a","operation":"x","files":[],"completeness":"COMPLETE","completeness":"PARTIAL"}"#,
        // Duplicate arriving after unknown keys keeps the same behaviour.
        r#"{"unknownA":1,"schemaVersion":"a","unknownB":2,"schemaVersion":"b","operation":"x","files":[]}"#,
    ];
    for json in plan_cases {
        assert!(assert_plan_parity(json).is_none(), "must fail: {json}");
    }
    let message = blazingly_json::from_str::<EditPlan>(plan_cases[0])
        .expect_err("duplicate schemaVersion fails")
        .to_string();
    assert!(
        message.contains("duplicate field `schemaVersion`"),
        "unexpected message: {message}"
    );

    let file_cases = [
        r#"{"path":"a.ts","path":"b.ts","sha256":"x","edits":[]}"#,
        r#"{"path":"a.ts","sha256":"x","sha256":"y","edits":[]}"#,
        r#"{"path":"a.ts","sha256":"x","edits":[],"edits":[]}"#,
    ];
    for json in file_cases {
        assert!(assert_file_parity(json).is_none(), "must fail: {json}");
    }

    let edit_cases = [
        r#"{"startLine":1,"startLine":2,"startChar":0,"endLine":1,"endChar":2,"before":"a","after":"b","provenance":"EXACT_LSP"}"#,
        r#"{"startLine":1,"startChar":0,"startChar":1,"endLine":1,"endChar":2,"before":"a","after":"b","provenance":"EXACT_LSP"}"#,
        r#"{"startLine":1,"startChar":0,"endLine":1,"endLine":2,"endChar":2,"before":"a","after":"b","provenance":"EXACT_LSP"}"#,
        r#"{"startLine":1,"startChar":0,"endLine":1,"endChar":2,"endChar":3,"before":"a","after":"b","provenance":"EXACT_LSP"}"#,
        r#"{"startLine":1,"startChar":0,"endLine":1,"endChar":2,"before":"a","before":"c","after":"b","provenance":"EXACT_LSP"}"#,
        r#"{"startLine":1,"startChar":0,"endLine":1,"endChar":2,"before":"a","after":"b","after":"c","provenance":"EXACT_LSP"}"#,
        r#"{"startLine":1,"startChar":0,"endLine":1,"endChar":2,"before":"a","after":"b","provenance":"EXACT_LSP","provenance":"RESOLVED"}"#,
    ];
    for json in edit_cases {
        assert!(assert_edit_parity(json).is_none(), "must fail: {json}");
    }
    let message = serde_json::from_str::<TextEdit>(edit_cases[0])
        .expect_err("duplicate startLine fails")
        .to_string();
    assert!(
        message.contains("duplicate field `startLine`"),
        "unexpected message: {message}"
    );
}

#[test]
fn missing_declared_fields_fail_with_the_derive_message() {
    let cases = [
        "{}",
        r#"{"operation":"x","files":[]}"#,
        r#"{"schemaVersion":"a","files":[]}"#,
        r#"{"schemaVersion":"a","operation":"x"}"#,
        // Unknown keys alone never satisfy a declared field.
        r#"{"unknown":1,"another":{"deep":true}}"#,
    ];
    for json in cases {
        assert!(assert_plan_parity(json).is_none(), "must fail: {json}");
    }
    let message = blazingly_json::from_str::<EditPlan>("{}")
        .expect_err("empty object fails")
        .to_string();
    assert!(
        message.contains("missing field `schemaVersion`"),
        "unexpected message: {message}"
    );

    let file_cases = [
        "{}",
        r#"{"sha256":"x","edits":[]}"#,
        r#"{"path":"a.ts","edits":[]}"#,
        r#"{"path":"a.ts","sha256":"x"}"#,
    ];
    for json in file_cases {
        assert!(assert_file_parity(json).is_none(), "must fail: {json}");
    }

    // Drop each edit field once; the reported name must always match.
    let full = r#"{"startLine":1,"startChar":0,"endLine":1,"endChar":2,"before":"a","after":"b","provenance":"EXACT_LSP"}"#;
    let fields = [
        "startLine",
        "startChar",
        "endLine",
        "endChar",
        "before",
        "after",
        "provenance",
    ];
    for field in fields {
        let json = full.replacen(&format!("\"{field}\""), &format!("\"_{field}\""), 1);
        assert!(assert_edit_parity(&json).is_none(), "must fail: {json}");
        let message = serde_json::from_str::<TextEdit>(&json)
            .expect_err("renamed field is missing")
            .to_string();
        assert!(
            message.contains(&format!("missing field `{field}`")),
            "unexpected message for {field}: {message}"
        );
    }

    // completeness stays optional and absent extensions default to empty.
    let plan = assert_plan_parity(r#"{"schemaVersion":"a","operation":"x","files":[]}"#)
        .expect("minimal plan decodes");
    assert_eq!(plan.completeness, None);
    assert!(plan.extensions.is_empty());
    let plan = assert_plan_parity(
        r#"{"schemaVersion":"a","operation":"x","files":[],"completeness":null}"#,
    )
    .expect("null completeness decodes");
    assert_eq!(plan.completeness, None);
}

#[test]
fn wrong_types_fail_with_the_derive_message() {
    let cases = [
        // Wrong envelope shapes.
        "[]",
        "[1,2]",
        r#""plan""#,
        "42",
        "true",
        "null",
        // Wrong declared field types at every level.
        r#"{"schemaVersion":7,"operation":"x","files":[]}"#,
        r#"{"schemaVersion":"a","operation":["x"],"files":[]}"#,
        r#"{"schemaVersion":"a","operation":"x","files":{}}"#,
        r#"{"schemaVersion":"a","operation":"x","files":[7]}"#,
        r#"{"schemaVersion":"a","operation":"x","files":[],"completeness":7}"#,
        r#"{"schemaVersion":"a","operation":"x","files":[{"path":1,"sha256":"x","edits":[]}]}"#,
        r#"{"schemaVersion":"a","operation":"x","files":[{"path":"a.ts","sha256":"x","edits":"none"}]}"#,
        r#"{"schemaVersion":"a","operation":"x","files":[{"path":"a.ts","sha256":"x","edits":[[]]}]}"#,
    ];
    for json in cases {
        assert!(assert_plan_parity(json).is_none(), "must fail: {json}");
    }

    let edit_cases = [
        r#"{"startLine":"1","startChar":0,"endLine":1,"endChar":2,"before":"a","after":"b","provenance":"EXACT_LSP"}"#,
        r#"{"startLine":-1,"startChar":0,"endLine":1,"endChar":2,"before":"a","after":"b","provenance":"EXACT_LSP"}"#,
        r#"{"startLine":1.5,"startChar":0,"endLine":1,"endChar":2,"before":"a","after":"b","provenance":"EXACT_LSP"}"#,
        r#"{"startLine":4294967296,"startChar":0,"endLine":1,"endChar":2,"before":"a","after":"b","provenance":"EXACT_LSP"}"#,
        r#"{"startLine":1,"startChar":0,"endLine":1,"endChar":2,"before":1,"after":"b","provenance":"EXACT_LSP"}"#,
        r#"{"startLine":1,"startChar":0,"endLine":1,"endChar":2,"before":"a","after":"b","provenance":5}"#,
        r#"{"startLine":1,"startChar":0,"endLine":1,"endChar":2,"before":"a","after":"b","provenance":{"tier":"EXACT_LSP"}}"#,
    ];
    for json in edit_cases {
        assert!(assert_edit_parity(json).is_none(), "must fail: {json}");
    }

    // Error text spot checks on both drivers.
    let message = serde_json::from_str::<EditPlan>("[]")
        .expect_err("array envelope fails")
        .to_string();
    assert!(
        message.contains("expected struct EditPlan"),
        "unexpected message: {message}"
    );
    let message = serde_json::from_str::<TextEdit>("7")
        .expect_err("numeric edit fails")
        .to_string();
    assert!(
        message.contains("expected struct TextEdit"),
        "unexpected message: {message}"
    );
    let message = serde_json::from_str::<FileEdit>("null")
        .expect_err("null file fails")
        .to_string();
    assert!(
        message.contains("expected struct FileEdit"),
        "unexpected message: {message}"
    );
}

#[test]
fn extension_values_of_every_json_type_decode_identically() {
    let json = r#"{"schemaVersion":"weavatrix.edit-plan.v1","operation":"x","files":[],
            "aNull":null,"aBool":false,"aTrue":true,
            "anInt":42,"aNegative":-17,"aBigU64":18446744073709551615,
            "aMinI64":-9223372036854775808,"aFloat":1.25,"aSciFloat":1e2,"aNegZero":-0,
            "aString":"plain","aUnicode":"café_́_🚀","anEscape":"tab\tnewline\n",
            "anArray":[1,"two",null,[true],{"deep":-0.5}],
            "anObject":{"nested":{"more":{"leaf":[]}},"sibling":2}}"#;
    let plan = assert_plan_parity(json).expect("every JSON type decodes");
    assert_eq!(plan.extensions.len(), 15);
    assert_eq!(plan.extensions["aBigU64"].as_u64(), Some(u64::MAX));
    assert_eq!(plan.extensions["aMinI64"].as_i64(), Some(i64::MIN));
    assert_eq!(plan.extensions["aFloat"].as_f64(), Some(1.25));
    assert!(plan.extensions["aNull"].is_null());
    assert_serialize_parity(&plan);
}

#[test]
fn declared_only_decode_drops_extensions_at_every_level_and_still_validates() {
    let capturing = assert_plan_parity(&extension_fixture()).expect("fixture decodes");
    assert!(!capturing.extensions.is_empty());
    assert!(!capturing.files[0].extensions.is_empty());
    assert!(!capturing.files[0].edits[0].extensions.is_empty());

    let declared: DeclaredEditPlan =
        blazingly_json::from_str(&extension_fixture()).expect("declared decode succeeds");
    let plan = declared.into_plan();
    assert!(plan.extensions.is_empty(), "envelope extensions dropped");
    assert!(
        plan.files[0].extensions.is_empty(),
        "file extensions dropped"
    );
    assert!(
        plan.files[0].edits[0].extensions.is_empty(),
        "edit extensions dropped"
    );

    // Declared data is untouched, and the recovered plan is a full participant.
    assert_eq!(plan.schema_version, capturing.schema_version);
    assert_eq!(plan.operation, capturing.operation);
    assert_eq!(plan.completeness, capturing.completeness);
    assert_eq!(plan.files[0].path, capturing.files[0].path);
    assert_eq!(
        plan.files[0].edits,
        without_extensions(&capturing).files[0].edits
    );
    assert!(plan.validate().is_ok());

    // Dropping is lossy on purpose: re-encoding emits declared members only.
    let reencoded = blazingly_json::to_string(&plan).expect("declared plan encodes");
    assert!(!reencoded.contains("graphRevision"));
    assert!(!reencoded.contains("\"language\""));
    assert_eq!(
        blazingly_json::from_str::<EditPlan>(&reencoded).expect("re-decodes"),
        plan
    );

    // A reserved member name can never reach an extension map through the wire,
    // so the declared-only path cannot hide a collision the capturing path
    // would have rejected: the duplicate spelling fails as a duplicate field.
    let shadowed = format!(
        r#"{{"schemaVersion":"weavatrix.edit-plan.v1","operation":"x","files":[{{"path":"a.ts","sha256":"{SHA256}","edits":[],"path":"b.ts"}}]}}"#
    );
    assert!(assert_plan_parity(&shadowed).is_none());

    // Programmatically built collisions are still rejected by validation.
    let mut collision = base_plan();
    collision.files[0]
        .extensions
        .insert("path".to_owned(), value(r#""shadow""#));
    assert!(collision.validate().is_err());
}

#[test]
fn deterministic_random_plans_stay_equivalent() {
    let mut state = 0x243F_6A88_85A3_08D3_u64;
    let mut next = move |bound: usize| -> usize {
        state = state
            .wrapping_mul(6_364_136_223_846_793_005)
            .wrapping_add(1_442_695_040_888_963_407);
        usize::try_from(state >> 33).expect("shifted u64 fits usize") % bound.max(1)
    };
    let key_pool = [
        "createdAt",
        "confidence",
        "\u{1F600}",
        "\u{E000}",
        "x-vendor",
        "schema_version",
        "Extensions",
        "",
        "caf\u{E9}\u{301}",
    ];
    let value_pool = [
        "null",
        "true",
        "-12",
        "18446744073709551615",
        "0.5",
        r#""text""#,
        r#""🚀""#,
        r#"[1,[2,[3]],{"k":null}]"#,
        r#"{"a":{"b":[false,1.25]},"z":""}"#,
    ];

    for _ in 0..64 {
        let mut plan = EditPlan::new(
            format!("operation_{}", next(1000)),
            (0..next(4))
                .map(|file| {
                    let mut edits = Vec::new();
                    for edit in 0..next(3) {
                        let mut text_edit = TextEdit::replace(
                            TextRange::new(
                                Position::new(1, u32::try_from(edit).expect("small index")),
                                Position::new(2, 0),
                            ),
                            format!("before_{edit}"),
                            format!("after_{edit}"),
                            Provenance::RESOLVED,
                        );
                        for _ in 0..next(3) {
                            text_edit.extensions.insert(
                                key_pool[next(key_pool.len())].to_owned(),
                                value(value_pool[next(value_pool.len())]),
                            );
                        }
                        edits.push(text_edit);
                    }
                    let mut file_edit = FileEdit::new(format!("src/file_{file}.ts"), SHA256, edits);
                    for _ in 0..next(3) {
                        file_edit.extensions.insert(
                            key_pool[next(key_pool.len())].to_owned(),
                            value(value_pool[next(value_pool.len())]),
                        );
                    }
                    file_edit
                })
                .collect(),
        );
        if next(2) == 1 {
            plan.completeness = Some(Completeness::new(Completeness::COMPLETE));
        }
        for _ in 0..next(4) {
            plan.extensions.insert(
                key_pool[next(key_pool.len())].to_owned(),
                value(value_pool[next(value_pool.len())]),
            );
        }

        assert_serialize_parity(&plan);
        let encoded = blazingly_json::to_string(&plan).expect("random plan encodes");
        let decoded = assert_plan_parity(&encoded).expect("random plan roundtrips");
        assert_eq!(decoded, plan);
    }
}
