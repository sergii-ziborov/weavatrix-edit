//! Manual decode microbenchmark for plan-sized envelopes.
//!
//! It records where multi-file decode time actually goes: the shipped model
//! against the standalone reference envelope in `flatten_reference`, and
//! `blazingly-json` against `serde_json` on the same corpora. Both pairs
//! measured within noise on a 500-file plan, which is why the envelope keeps
//! `#[serde(flatten)]` and the crate keeps its decoder.
//!
//! Run on demand; the harness skips it by default:
//!
//! ```text
//! cargo test --release --test decode_bench -- --ignored --nocapture --test-threads=1
//! ```

mod flatten_reference;

use std::fmt::Write as _;
use std::time::Instant;

use flatten_reference::FlatEditPlan;
use weavatrix_edit::EditPlan;

/// Replicates the `files-500-edits-500` legacy workload from
/// weavatrix-refactor-plan/tools/benchmarks/workloads.mjs: one edit per file,
/// Unicode paths and labels, and extension keys on the envelope, every file,
/// and every edit.
fn refactor_plan_corpus(file_count: usize) -> Vec<u8> {
    let sha = "a".repeat(64);
    let mut json = String::new();
    let _ = write!(
        json,
        "{{\"schemaVersion\":\"weavatrix.edit-plan.v1\",\"operation\":\"rename_symbol_{file_count}_\u{1F680}\",\"files\":["
    );
    for file_index in 0..file_count {
        if file_index > 0 {
            json.push(',');
        }
        let ordinal = format!("{file_index:04}");
        let before = format!("symbol_{file_index}_0");
        let after = format!("\u{441}\u{438}\u{43C}\u{432}\u{43E}\u{43B}_{file_index}_0_\u{1F680}");
        let nested = if file_index % 10 == 0 {
            format!(
                ",\"nestedEvidence\":{{\"language\":\"TypeScript\",\"tags\":[\"rename\",\"unicode\",{file_index}]}}"
            )
        } else {
            String::new()
        };
        let _ = write!(
            json,
            "{{\"path\":\"src/\u{6A21}\u{5757}_{ordinal}/\u{444}\u{430}\u{439}\u{43B}_{ordinal}.ts\",\"sha256\":\"{sha}\",\"edits\":[{{\"startLine\":1,\"startChar\":0,\"endLine\":1,\"endChar\":{},\"before\":\"{before}\",\"after\":\"{after}\",\"provenance\":\"EXACT_LSP\",\"benchmarkEditOrdinal\":0{nested}}}],\"benchmarkFileOrdinal\":{file_index},\"unicodeLabel\":\"caf\u{E9}_\u{301}_{ordinal}_\u{1F680}\"}}",
            before.len()
        );
    }
    let _ = write!(
        json,
        "],\"completeness\":\"PARTIAL\",\"createdAt\":\"2026-08-02T00:00:00Z\",\"graphRevision\":\"benchmark-revision-{file_count}\",\"benchmarkMetadata\":{{\"revision\":\"refactor-plan-bench-v1\",\"fileCount\":{file_count},\"editsPerFile\":1,\"nested\":{{\"emoji\":\"\u{1F9F5}\",\"normalization\":\"NFC\",\"values\":[null,true,1.25,42]}}}},\"\u{1F600}\":\"astral-key\",\"\u{E000}\":\"private-use-key\"}}"
    );
    json.into_bytes()
}

/// ASCII corpus without extension keys: 500 files x 1 edit of declared fields.
fn plain_corpus(file_count: usize) -> Vec<u8> {
    let sha = "a".repeat(64);
    let mut json = String::new();
    json.push_str(
        "{\"schemaVersion\":\"weavatrix.edit-plan.v1\",\"operation\":\"rename_symbol\",\"files\":[",
    );
    for index in 0..file_count {
        if index > 0 {
            json.push(',');
        }
        let _ = write!(
            json,
            "{{\"path\":\"packages/app-{index:03}/src/module_{index:03}.ts\",\"sha256\":\"{sha}\",\"edits\":[{{\"startLine\":1,\"startChar\":0,\"endLine\":1,\"endChar\":7,\"before\":\"getUser\",\"after\":\"getCustomer\",\"provenance\":\"EXACT_LSP\"}}]}}"
        );
    }
    json.push_str("],\"completeness\":\"COMPLETE\"}");
    json.into_bytes()
}

fn median(mut samples: Vec<f64>) -> f64 {
    samples.sort_by(f64::total_cmp);
    let mid = samples.len() / 2;
    if samples.len().is_multiple_of(2) {
        f64::midpoint(samples[mid - 1], samples[mid])
    } else {
        samples[mid]
    }
}

/// Interleaves the two operations so machine-state drift hits both equally.
fn paired_medians<T, U>(
    runs: usize,
    mut left: impl FnMut() -> T,
    mut right: impl FnMut() -> U,
) -> (f64, f64) {
    let mut left_samples = Vec::with_capacity(runs);
    let mut right_samples = Vec::with_capacity(runs);
    for _ in 0..5 {
        std::hint::black_box(left());
        std::hint::black_box(right());
    }
    for _ in 0..runs {
        let start = Instant::now();
        std::hint::black_box(left());
        left_samples.push(start.elapsed().as_secs_f64() * 1e6);
        let start = Instant::now();
        std::hint::black_box(right());
        right_samples.push(start.elapsed().as_secs_f64() * 1e6);
    }
    (median(left_samples), median(right_samples))
}

#[test]
#[ignore = "manual microbenchmark; run with --ignored --nocapture --test-threads=1"]
fn decode_before_after() {
    for (label, bytes) in [
        ("refactor-plan files-500", refactor_plan_corpus(500)),
        ("plain files-500", plain_corpus(500)),
    ] {
        // The corpora stay mutually decodable and valid.
        let plan = blazingly_json::from_slice::<EditPlan>(&bytes).expect("corpus decodes");
        assert!(plan.validate().is_ok(), "corpus must validate");

        println!("corpus {label}: {} bytes", bytes.len());
        for round in 0..3 {
            let (flat, manual) = paired_medians(
                25,
                || blazingly_json::from_slice::<FlatEditPlan>(&bytes).expect("reference decodes"),
                || blazingly_json::from_slice::<EditPlan>(&bytes).expect("production decodes"),
            );
            println!(
                "  round {round}: reference envelope {flat:.1} us | shipped envelope {manual:.1} us | ratio {:.2}x",
                flat / manual
            );
        }
        for round in 0..3 {
            let (blazingly, serde) = paired_medians(
                25,
                || blazingly_json::from_slice::<EditPlan>(&bytes).expect("blazingly decodes"),
                || serde_json::from_slice::<EditPlan>(&bytes).expect("serde_json decodes"),
            );
            println!(
                "  round {round}: blazingly-json {blazingly:.1} us | serde_json {serde:.1} us | serde/blazingly {:.2}x",
                blazingly / serde
            );
        }
    }
}
