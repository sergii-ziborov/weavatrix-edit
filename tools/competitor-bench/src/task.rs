use std::time::Duration;

#[derive(Clone, Copy, Debug, Eq, Ord, PartialEq, PartialOrd)]
pub(crate) enum Engine {
    Weavatrix,
    Mago,
    RustAnalyzer,
    Typst,
}

impl Engine {
    pub(crate) const fn label(self) -> &'static str {
        match self {
            Self::Weavatrix => "weavatrix-edit",
            Self::Mago => "mago-text-edit",
            Self::RustAnalyzer => "ra_ap_text_edit",
            Self::Typst => "typst-edit",
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, Ord, PartialEq, PartialOrd)]
pub(crate) enum Phase {
    BatchApply,
    Prepare,
    Prepared,
    Chunks,
    WriteTo,
}

impl Phase {
    pub(crate) const fn label(self) -> &'static str {
        match self {
            Self::BatchApply => "batch+apply",
            Self::Prepare => "prepare",
            Self::Prepared => "prepared-apply",
            Self::Chunks => "chunks (WV-only)",
            Self::WriteTo => "write-to-Vec (WV-only)",
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, Ord, PartialEq, PartialOrd)]
pub(crate) struct Task {
    pub(crate) phase: Phase,
    pub(crate) engine: Engine,
}

impl Task {
    const fn new(phase: Phase, engine: Engine) -> Self {
        Self { phase, engine }
    }
}

pub(crate) const TASKS: [Task; 12] = [
    Task::new(Phase::BatchApply, Engine::Weavatrix),
    Task::new(Phase::BatchApply, Engine::Mago),
    Task::new(Phase::BatchApply, Engine::RustAnalyzer),
    Task::new(Phase::BatchApply, Engine::Typst),
    Task::new(Phase::Prepare, Engine::Weavatrix),
    Task::new(Phase::Prepare, Engine::Mago),
    Task::new(Phase::Prepare, Engine::RustAnalyzer),
    Task::new(Phase::Prepared, Engine::Weavatrix),
    Task::new(Phase::Prepared, Engine::Mago),
    Task::new(Phase::Prepared, Engine::RustAnalyzer),
    Task::new(Phase::Chunks, Engine::Weavatrix),
    Task::new(Phase::WriteTo, Engine::Weavatrix),
];

#[derive(Debug)]
pub(crate) struct Summary {
    pub(crate) task: Task,
    pub(crate) median: Duration,
    pub(crate) p25: Duration,
    pub(crate) p75: Duration,
    pub(crate) p95: Duration,
    pub(crate) samples: Vec<Duration>,
}
