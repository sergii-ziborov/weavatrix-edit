use std::io::{self, Write};

use super::EditChunks;

/// Metadata returned after a prepared result is fully written.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct WriteSummary {
    pub edits_applied: usize,
    pub bytes_before: usize,
    pub bytes_written: usize,
}

pub(super) fn write_prepared<W: Write + ?Sized>(
    writer: &mut W,
    chunks: EditChunks<'_>,
    source_size: usize,
    edit_count: usize,
    output_size: usize,
) -> io::Result<WriteSummary> {
    for chunk in chunks {
        writer.write_all(chunk.as_bytes())?;
    }
    Ok(WriteSummary {
        edits_applied: edit_count,
        bytes_before: source_size,
        bytes_written: output_size,
    })
}
