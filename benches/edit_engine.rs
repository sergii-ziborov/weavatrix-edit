use std::hint::black_box;
use std::time::{Duration, Instant};

use weavatrix_edit::{Position, Provenance, TextEdit, TextRange, apply_edits};

fn main() {
    let source = "let value = 0;\n".repeat(65_536);
    let edit = TextEdit::replace(
        TextRange::new(Position::new(32_768, 4), Position::new(32_768, 9)),
        "value",
        "result",
        Provenance::EXACT_LSP,
    );
    let mut samples = Vec::with_capacity(31);
    for _ in 0..31 {
        let started = Instant::now();
        let output = apply_edits(black_box(&source), black_box(std::slice::from_ref(&edit)))
            .expect("benchmark edit must apply");
        black_box(output);
        samples.push(started.elapsed());
    }
    samples.sort_unstable();
    println!(
        "1 MiB / 1 verified UTF-16 edit median: {:?}",
        median(&samples)
    );
}

fn median(samples: &[Duration]) -> Duration {
    samples[samples.len() / 2]
}
