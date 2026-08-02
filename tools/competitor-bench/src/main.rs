#![forbid(unsafe_code)]

mod adapters;
mod measure;
mod report;
mod task;
mod workload;

use adapters::Adapters;
use report::{WARMUP_ROUNDS, benchmark_interleaved, print_results};
use task::{Engine, Phase};
use workload::{KIB, MIB, Workload};

fn main() {
    let arguments = std::env::args().skip(1).collect::<Vec<_>>();
    let print_raw = arguments.iter().any(|argument| argument == "--raw");
    let require_reused_string_2x = arguments
        .iter()
        .any(|argument| argument == "--require-reused-string-2x");
    let filter = arguments
        .windows(2)
        .find(|pair| pair[0] == "--filter")
        .map(|pair| pair[1].as_str());
    let workloads = [
        Workload::sparse_mixed("1 KiB / 1 edit", KIB, 1, 31, 1_000),
        Workload::sparse_mixed("100 KiB / 100 edits", 100 * KIB, 100, 31, 50),
        Workload::sparse_mixed("1 MiB / 1,000 edits", MIB, 1_000, 15, 5),
        Workload::sparse_mixed("unused", MIB, 1_000, 15, 5)
            .reversed("1 MiB / 1,000 edits (reversed input)"),
        Workload::sparse_mixed(
            "10 MiB / 2,000 edits (default ceiling)",
            10 * MIB,
            2_000,
            9,
            1,
        ),
        Workload::sparse_mixed(
            "10 MiB / 10,000 edits (custom-limit stress)",
            10 * MIB,
            10_000,
            9,
            1,
        ),
        Workload::replacement_heavy("1 MiB / 1,000 replacements", MIB, 1_000, 15, 5),
        Workload::same_offset_insertions("1 MiB / 1,000 same-offset inserts", MIB, 1_000, 15, 5),
    ];

    println!("weavatrix-edit competitor benchmark");
    println!(
        "target: {}-{}",
        std::env::consts::OS,
        std::env::consts::ARCH
    );
    println!("weavatrix-edit: {}", weavatrix_edit::VERSION);
    println!("pinned: mago-text-edit 1.45.0, ra_ap_text_edit 0.0.241, typst-edit 0.1.0");
    println!("warmup rounds: {WARMUP_ROUNDS}; measured rounds are deterministically rotated");
    println!(
        "all outputs, including stable same-offset insertion order, are checked byte-for-byte before timing"
    );
    println!(
        "textum 0.4.0 is not timed: its in-memory direct-range API is single-edit/persistent-rope, while PatchSet batch apply requires file I/O\n"
    );

    let mut performance_gate_checked = false;
    for workload in &workloads {
        if filter.is_some_and(|needle| !workload.name.contains(needle)) {
            continue;
        }
        let adapters = Adapters::new(workload);
        let summaries = benchmark_interleaved(&adapters);
        print_results(workload, &summaries, print_raw);
        if require_reused_string_2x && workload.name.contains("default ceiling") {
            performance_gate_checked = true;
            let weavatrix = summaries
                .iter()
                .find(|summary| {
                    summary.task.phase == Phase::Reused && summary.task.engine == Engine::Weavatrix
                })
                .expect("caller-buffer Weavatrix result must exist");
            let fastest_competitor = summaries
                .iter()
                .filter(|summary| {
                    summary.task.phase == Phase::Reused
                        && matches!(summary.task.engine, Engine::Mago | Engine::RustAnalyzer)
                })
                .map(|summary| summary.p25)
                .min()
                .expect("caller-buffer competitor result must exist");
            let ratio = fastest_competitor.as_secs_f64() / weavatrix.p75.as_secs_f64();
            println!(
                "default-ceiling reused-String conservative gate (fastest competitor p25 / Weavatrix p75): {ratio:.2}x (required: 2.00x)"
            );
            if ratio < 2.0 {
                eprintln!("reused-String performance gate failed");
                std::process::exit(2);
            }
        }
    }
    if require_reused_string_2x && !performance_gate_checked {
        eprintln!("reused-String performance gate requires the default-ceiling workload");
        std::process::exit(2);
    }
}
