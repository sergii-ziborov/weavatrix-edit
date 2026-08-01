use std::hint::black_box;
use std::time::{Duration, Instant};

use weavatrix_edit::{apply_byte_edits_with_limits, prepare_byte_edits_owned_with_limits};

use crate::adapters::{Adapters, prepare_mago, prepare_ra};
use crate::task::{Engine, Phase, Task};

impl Adapters<'_> {
    pub(crate) fn measure(&self, task: Task) -> Duration {
        let iterations = self.workload.iterations_per_sample;
        let elapsed = match task.phase {
            Phase::BatchApply => self.measure_batch_apply(task.engine, iterations),
            Phase::Prepare => self.measure_prepare(task.engine, iterations),
            Phase::Prepared => self.measure_prepared(task.engine, iterations),
            Phase::Chunks => self.measure_chunks(iterations),
            Phase::WriteTo => self.measure_write_to(iterations),
        };
        elapsed.div_f64(iterations as f64)
    }

    fn measure_batch_apply(&self, engine: Engine, iterations: usize) -> Duration {
        match engine {
            Engine::Weavatrix => {
                let started = Instant::now();
                for _ in 0..iterations {
                    let output = apply_byte_edits_with_limits(
                        black_box(&self.workload.source),
                        black_box(&self.weavatrix_edits),
                        self.limits,
                    )
                    .expect("Weavatrix one-shot path must apply");
                    black_box(&output.text);
                }
                started.elapsed()
            }
            Engine::Mago => {
                let batches = clone_batches(&self.mago_edits, iterations);
                let started = Instant::now();
                for edits in batches {
                    let output =
                        prepare_mago(black_box(&self.workload.source), black_box(edits)).finish();
                    black_box(&output);
                }
                started.elapsed()
            }
            Engine::RustAnalyzer => {
                let batches = clone_batches(&self.ra_specs, iterations);
                let started = Instant::now();
                for specs in batches {
                    let edit = prepare_ra(black_box(specs));
                    let mut output = black_box(&self.workload.source).clone();
                    edit.apply(&mut output);
                    black_box(&output);
                }
                started.elapsed()
            }
            Engine::Typst => {
                let batches = clone_batches(&self.typst_edits, iterations);
                let started = Instant::now();
                for edits in batches {
                    let output =
                        typst_edit::apply(black_box(&self.workload.source), black_box(edits))
                            .expect("generated Typst edits must apply");
                    black_box(&output);
                }
                started.elapsed()
            }
        }
    }

    fn measure_prepare(&self, engine: Engine, iterations: usize) -> Duration {
        match engine {
            Engine::Weavatrix => {
                let batches = clone_batches(&self.weavatrix_edits, iterations);
                let started = Instant::now();
                for edits in batches {
                    let prepared = prepare_byte_edits_owned_with_limits(
                        black_box(&self.workload.source),
                        black_box(edits),
                        self.limits,
                    )
                    .expect("generated Weavatrix edits must prepare");
                    black_box(prepared);
                }
                started.elapsed()
            }
            Engine::Mago => {
                let batches = clone_batches(&self.mago_edits, iterations);
                let started = Instant::now();
                for edits in batches {
                    let prepared = prepare_mago(black_box(&self.workload.source), black_box(edits));
                    black_box(prepared);
                }
                started.elapsed()
            }
            Engine::RustAnalyzer => {
                let batches = clone_batches(&self.ra_specs, iterations);
                let started = Instant::now();
                for specs in batches {
                    black_box(prepare_ra(black_box(specs)));
                }
                started.elapsed()
            }
            Engine::Typst => {
                unreachable!("typst-edit does not expose a reusable prepared plan")
            }
        }
    }

    fn measure_prepared(&self, engine: Engine, iterations: usize) -> Duration {
        match engine {
            Engine::Weavatrix => {
                let started = Instant::now();
                for _ in 0..iterations {
                    let output = black_box(&self.weavatrix_prepared).apply();
                    black_box(&output.text);
                }
                started.elapsed()
            }
            Engine::Mago => {
                let started = Instant::now();
                for _ in 0..iterations {
                    // `finish` consumes Mago's prepared editor, so cloning is part
                    // of every actual replay from one prepared value.
                    let output = black_box(&self.mago_prepared).clone().finish();
                    black_box(&output);
                }
                started.elapsed()
            }
            Engine::RustAnalyzer => {
                let started = Instant::now();
                for _ in 0..iterations {
                    let mut output = black_box(&self.workload.source).clone();
                    black_box(&self.ra_prepared).apply(&mut output);
                    black_box(&output);
                }
                started.elapsed()
            }
            Engine::Typst => {
                unreachable!("typst-edit does not expose a reusable prepared plan")
            }
        }
    }

    fn measure_chunks(&self, iterations: usize) -> Duration {
        let started = Instant::now();
        for _ in 0..iterations {
            let mut bytes = 0_usize;
            for chunk in black_box(&self.weavatrix_prepared).chunks() {
                bytes = bytes
                    .checked_add(black_box(chunk).len())
                    .expect("benchmark output length must fit usize");
            }
            black_box(bytes);
        }
        started.elapsed()
    }

    fn measure_write_to(&self, iterations: usize) -> Duration {
        let mut outputs = (0..iterations)
            .map(|_| Vec::with_capacity(self.workload.expected_len()))
            .collect::<Vec<_>>();
        let started = Instant::now();
        for output in &mut outputs {
            let summary = black_box(&self.weavatrix_prepared)
                .write_to(output)
                .expect("Vec writes cannot fail");
            black_box(summary);
            black_box(output.as_slice());
        }
        started.elapsed()
    }
}

fn clone_batches<T: Clone>(input: &[T], count: usize) -> Vec<Vec<T>> {
    (0..count).map(|_| input.to_vec()).collect()
}
