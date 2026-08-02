use std::collections::BTreeMap;
use std::hint::black_box;
use std::time::Duration;

use crate::adapters::Adapters;
use crate::task::{Engine, Phase, Summary, TASKS, Task};
use crate::workload::{MIB, Workload};

pub(crate) const WARMUP_ROUNDS: usize = 3;

pub(crate) fn benchmark_interleaved(adapters: &Adapters<'_>) -> Vec<Summary> {
    for round in 0..WARMUP_ROUNDS {
        for offset in 0..TASKS.len() {
            let task = TASKS[(round + offset) % TASKS.len()];
            black_box(adapters.measure(task));
        }
    }

    let mut samples = BTreeMap::<Task, Vec<Duration>>::new();
    for round in 0..adapters.workload.samples {
        for offset in 0..TASKS.len() {
            let task = TASKS[(round + offset) % TASKS.len()];
            samples
                .entry(task)
                .or_default()
                .push(adapters.measure(task));
        }
    }

    TASKS
        .iter()
        .map(|task| {
            summarize(
                *task,
                samples.remove(task).expect("every task was measured"),
            )
        })
        .collect()
}

fn summarize(task: Task, mut values: Vec<Duration>) -> Summary {
    let samples = values.clone();
    values.sort_unstable();
    Summary {
        task,
        median: percentile(&values, 50),
        p25: percentile(&values, 25),
        p75: percentile(&values, 75),
        p95: percentile(&values, 95),
        samples,
    }
}

fn percentile(values: &[Duration], percentile: usize) -> Duration {
    assert!(!values.is_empty());
    assert!((1..=100).contains(&percentile));
    let rank = values
        .len()
        .checked_mul(percentile)
        .and_then(|value| value.checked_add(99))
        .expect("benchmark sample count must not overflow")
        / 100;
    values[rank - 1]
}

pub(crate) fn print_results(workload: &Workload, summaries: &[Summary], print_raw: bool) {
    println!(
        "## {} ({} samples x {} iterations)",
        workload.name, workload.samples, workload.iterations_per_sample
    );
    println!(
        "fixture: fnv1a64:{:016x}; source: {} bytes; edits: {}; expected: {} bytes",
        workload.fixture_hash(),
        workload.source.len(),
        workload.edits.len(),
        workload.expected_len(),
    );
    println!(
        "| phase | engine | median | p25..p75 | p95 | p95/median | phase metric | phase/WV time |"
    );
    println!("|---|---|---:|---:|---:|---:|---:|---:|");
    for summary in summaries {
        print_summary(workload, summaries, summary);
    }
    if print_raw {
        print_raw_samples(summaries);
    }
    println!();
}

fn print_summary(workload: &Workload, summaries: &[Summary], summary: &Summary) {
    let weavatrix = summaries
        .iter()
        .find(|candidate| {
            candidate.task.phase == summary.task.phase && candidate.task.engine == Engine::Weavatrix
        })
        .expect("each phase has a Weavatrix measurement");
    let relative = if matches!(summary.task.phase, Phase::Reused | Phase::ReusedBytes) {
        format!(
            "{:.2}x",
            summary.median.as_secs_f64() / weavatrix.median.as_secs_f64()
        )
    } else {
        "n/a".to_owned()
    };
    let (p95, p95_ratio) = if summary.samples.len() >= 21 {
        (
            format_duration(summary.p95),
            format!(
                "{:.2}x",
                summary.p95.as_secs_f64() / summary.median.as_secs_f64()
            ),
        )
    } else {
        ("n/a (n<21)".to_owned(), "n/a".to_owned())
    };
    let phase_metric = match summary.task.phase {
        Phase::Prepare => format!(
            "{:.1} edits/s",
            workload.edits.len() as f64 / summary.median.as_secs_f64()
        ),
        Phase::BatchApply
        | Phase::Prepared
        | Phase::Reused
        | Phase::ReusedBytes
        | Phase::WriteTo => format!(
            "{:.1} output MiB/s",
            workload.expected_len() as f64 / summary.median.as_secs_f64() / (MIB as f64)
        ),
        Phase::Chunks => "n/a (lengths only)".to_owned(),
    };
    println!(
        "| {} | {} | {} | {}..{} | {} | {} | {} | {} |",
        summary.task.phase.label(),
        summary.task.engine.label(),
        format_duration(summary.median),
        format_duration(summary.p25),
        format_duration(summary.p75),
        p95,
        p95_ratio,
        phase_metric,
        relative,
    );
}

fn print_raw_samples(summaries: &[Summary]) {
    for summary in summaries {
        let values = summary
            .samples
            .iter()
            .map(|sample| sample.as_nanos().to_string())
            .collect::<Vec<_>>()
            .join(",");
        println!(
            "raw_ns_per_op phase={} engine={} values={values}",
            summary.task.phase.label(),
            summary.task.engine.label(),
        );
    }
}

fn format_duration(duration: Duration) -> String {
    let nanos = duration.as_nanos();
    if nanos < 1_000 {
        format!("{nanos} ns")
    } else if nanos < 1_000_000 {
        format!("{:.2} us", nanos as f64 / 1_000.0)
    } else if nanos < 1_000_000_000 {
        format!("{:.2} ms", nanos as f64 / 1_000_000.0)
    } else {
        format!("{:.2} s", duration.as_secs_f64())
    }
}

#[cfg(test)]
mod tests {
    use std::time::Duration;

    use super::percentile;

    #[test]
    fn percentile_uses_nearest_rank() {
        let values = (1..=9).map(Duration::from_nanos).collect::<Vec<_>>();
        assert_eq!(percentile(&values, 50), Duration::from_nanos(5));
        assert_eq!(percentile(&values, 95), Duration::from_nanos(9));
    }
}
