pub(crate) const KIB: usize = 1024;
pub(crate) const MIB: usize = 1024 * KIB;

#[derive(Clone, Debug)]
pub(crate) struct NeutralEdit {
    pub(crate) start: usize,
    pub(crate) end: usize,
    pub(crate) before: String,
    pub(crate) after: String,
}

#[derive(Debug)]
pub(crate) struct Workload {
    pub(crate) name: &'static str,
    pub(crate) source: String,
    pub(crate) edits: Vec<NeutralEdit>,
    pub(crate) samples: usize,
    pub(crate) iterations_per_sample: usize,
}

impl Workload {
    pub(crate) fn reversed(mut self, name: &'static str) -> Self {
        self.name = name;
        self.edits.reverse();
        self
    }

    pub(crate) fn sparse_mixed(
        name: &'static str,
        source_bytes: usize,
        edit_count: usize,
        samples: usize,
        iterations_per_sample: usize,
    ) -> Self {
        assert!(edit_count > 0);
        assert!(source_bytes > edit_count);

        let source = "a".repeat(source_bytes);
        let mut edits = Vec::with_capacity(edit_count);
        let mut previous = None;
        for index in 0..edit_count {
            let offset = ((index + 1) * source_bytes) / (edit_count + 1);
            assert_ne!(previous, Some(offset), "generated offsets must be unique");
            previous = Some(offset);
            let (end, before, after) = match index % 4 {
                0 => (offset + 1, "a", "BC"), // growing replacement
                1 => (offset + 1, "a", ""),   // deletion
                2 => (offset + 1, "a", "Z"),  // same-size replacement
                3 => (offset, "", "Q"),       // insertion
                _ => unreachable!(),
            };
            edits.push(NeutralEdit {
                start: offset,
                end,
                before: before.to_owned(),
                after: after.to_owned(),
            });
        }

        Self {
            name,
            source,
            edits,
            samples,
            iterations_per_sample,
        }
    }

    pub(crate) fn replacement_heavy(
        name: &'static str,
        source_bytes: usize,
        edit_count: usize,
        samples: usize,
        iterations_per_sample: usize,
    ) -> Self {
        const WIDTH: usize = 4;
        assert!(edit_count > 0);
        assert!(source_bytes > edit_count * WIDTH);

        let source = "a".repeat(source_bytes);
        let mut edits = Vec::with_capacity(edit_count);
        let mut previous_end = 0;
        for index in 0..edit_count {
            let start = ((index + 1) * source_bytes) / (edit_count + 1);
            let end = start + WIDTH;
            assert!(start >= previous_end && end <= source_bytes);
            previous_end = end;
            edits.push(NeutralEdit {
                start,
                end,
                before: "a".repeat(WIDTH),
                after: format!("R{index:03}"),
            });
        }
        Self {
            name,
            source,
            edits,
            samples,
            iterations_per_sample,
        }
    }

    pub(crate) fn same_offset_insertions(
        name: &'static str,
        source_bytes: usize,
        edit_count: usize,
        samples: usize,
        iterations_per_sample: usize,
    ) -> Self {
        assert!(edit_count > 0);
        let source = "a".repeat(source_bytes);
        let at = source.len() / 2;
        let edits = (0..edit_count)
            .map(|index| NeutralEdit {
                start: at,
                end: at,
                before: String::new(),
                after: format!("I{index:04}"),
            })
            .collect();
        Self {
            name,
            source,
            edits,
            samples,
            iterations_per_sample,
        }
    }

    pub(crate) fn fixture_hash(&self) -> u64 {
        let mut hash = FNV_OFFSET;
        feed(&mut hash, self.source.as_bytes());
        for edit in &self.edits {
            let start = u64::try_from(edit.start).expect("benchmark offset fits u64");
            let end = u64::try_from(edit.end).expect("benchmark offset fits u64");
            feed(&mut hash, &start.to_le_bytes());
            feed(&mut hash, &end.to_le_bytes());
            feed_sized(&mut hash, edit.before.as_bytes());
            feed_sized(&mut hash, edit.after.as_bytes());
        }
        hash
    }

    pub(crate) fn expected_len(&self) -> usize {
        self.edits.iter().fold(self.source.len(), |size, edit| {
            size - (edit.end - edit.start) + edit.after.len()
        })
    }
}

const FNV_OFFSET: u64 = 0xcbf2_9ce4_8422_2325;
const FNV_PRIME: u64 = 0x0000_0100_0000_01b3;

fn feed(hash: &mut u64, bytes: &[u8]) {
    for byte in bytes {
        *hash ^= u64::from(*byte);
        *hash = hash.wrapping_mul(FNV_PRIME);
    }
}

fn feed_sized(hash: &mut u64, bytes: &[u8]) {
    let len = u64::try_from(bytes.len()).expect("benchmark fixture length fits u64");
    feed(hash, &len.to_le_bytes());
    feed(hash, bytes);
}

pub(crate) fn reference_apply(source: &str, edits: &[NeutralEdit]) -> String {
    let mut sorted = edits.iter().collect::<Vec<_>>();
    sorted.sort_by_key(|edit| (edit.start, edit.end));
    let output_size = sorted.iter().fold(source.len(), |size, edit| {
        size - (edit.end - edit.start) + edit.after.len()
    });
    let mut output = String::with_capacity(output_size);
    let mut cursor = 0;
    for edit in sorted {
        assert!(edit.start >= cursor, "reference edits must not overlap");
        assert_eq!(&source[edit.start..edit.end], edit.before);
        output.push_str(&source[cursor..edit.start]);
        output.push_str(&edit.after);
        cursor = edit.end;
    }
    output.push_str(&source[cursor..]);
    output
}

#[cfg(test)]
mod tests {
    use super::{MIB, Workload, reference_apply};

    #[test]
    fn same_offset_insertions_keep_declared_order() {
        let workload = Workload::same_offset_insertions("test", 16, 3, 1, 1);
        assert_eq!(
            reference_apply(&workload.source, &workload.edits),
            "aaaaaaaaI0000I0001I0002aaaaaaaa"
        );
    }

    #[test]
    fn generated_fixtures_are_reproducible() {
        let left = Workload::replacement_heavy("left", MIB, 1_000, 1, 1);
        let right = Workload::replacement_heavy("right", MIB, 1_000, 1, 1);
        assert_eq!(left.fixture_hash(), right.fixture_hash());
    }
}
