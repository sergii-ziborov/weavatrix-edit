#![forbid(unsafe_code)]

mod adapters;
mod measure;
mod report;
mod task;
mod workload;

use adapters::Adapters;
use report::{WARMUP_ROUNDS, benchmark_interleaved, print_results};
use workload::{KIB, MIB, Workload};

fn main() {
    let print_raw = std::env::args_os()
        .skip(1)
        .any(|argument| argument == "--raw");
    let workloads = [
        Workload::sparse_mixed("1 KiB / 1 edit", KIB, 1, 31, 1_000),
        Workload::sparse_mixed("100 KiB / 100 edits", 100 * KIB, 100, 31, 50),
        Workload::sparse_mixed("1 MiB / 1,000 edits", MIB, 1_000, 15, 5),
        Workload::sparse_mixed("10 MiB / 10,000 edits", 10 * MIB, 10_000, 9, 1),
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

    for workload in &workloads {
        let adapters = Adapters::new(workload);
        let summaries = benchmark_interleaved(&adapters);
        print_results(workload, &summaries, print_raw);
    }
}
