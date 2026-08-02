/// Bounded validation limits for a multi-file edit plan.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct PlanLimits {
    pub max_files: usize,
    pub max_edits_per_file: usize,
    pub max_total_edits: usize,
    pub max_path_bytes: usize,
    pub max_total_text_bytes: usize,
}

impl Default for PlanLimits {
    fn default() -> Self {
        Self {
            max_files: 500,
            max_edits_per_file: 2_000,
            max_total_edits: 1_000_000,
            max_path_bytes: 4_096,
            max_total_text_bytes: 64 * 1024 * 1024,
        }
    }
}

/// Bounded in-memory application limits for one source file.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct ApplyLimits {
    pub max_source_bytes: usize,
    pub max_edits: usize,
    pub max_output_bytes: usize,
}

impl Default for ApplyLimits {
    fn default() -> Self {
        Self {
            max_source_bytes: 16 * 1024 * 1024,
            max_edits: 2_000,
            max_output_bytes: 64 * 1024 * 1024,
        }
    }
}

/// Resource limits for building a reusable full [`crate::LineIndex`].
///
/// The byte ceiling covers the line-start offset table, not the borrowed source
/// text. Position-based one-shot application uses a separate sparse index
/// bounded by the existing [`ApplyLimits::max_edits`] ceiling.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct LineIndexLimits {
    pub max_lines: usize,
    pub max_index_bytes: usize,
}

impl Default for LineIndexLimits {
    fn default() -> Self {
        Self {
            max_lines: 1_000_000,
            max_index_bytes: 8 * 1024 * 1024,
        }
    }
}

/// Hard resource limits for incrementally building one original-source batch.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct BatchLimits {
    pub max_source_bytes: usize,
    pub max_edits: usize,
    pub max_before_bytes: usize,
    pub max_replacement_bytes: usize,
    pub max_output_bytes: usize,
}

impl Default for BatchLimits {
    fn default() -> Self {
        Self {
            max_source_bytes: 16 * 1024 * 1024,
            max_edits: 2_000,
            max_before_bytes: 16 * 1024 * 1024,
            max_replacement_bytes: 64 * 1024 * 1024,
            max_output_bytes: 64 * 1024 * 1024,
        }
    }
}
