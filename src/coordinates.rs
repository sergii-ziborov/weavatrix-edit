use crate::{
    error::{EditError, ErrorCode},
    limits::LineIndexLimits,
    model::{Position, PositionEncoding},
};

use core::mem::size_of;

/// Reusable line index for strict Weavatrix v1 UTF-16 positions.
#[derive(Clone, Debug)]
pub struct LineIndex<'text> {
    text: &'text str,
    starts: Vec<usize>,
}

impl<'text> LineIndex<'text> {
    /// Builds a reusable full line index.
    ///
    /// This compatibility constructor retains its infallible signature. Code
    /// handling untrusted or very large text should use [`Self::try_new`] with
    /// explicit resource ceilings instead.
    ///
    /// # Panics
    ///
    /// Panics if the complete line-start table cannot be represented or
    /// allocated. Use [`Self::try_new`] to receive a bounded error instead.
    #[must_use]
    pub fn new(text: &'text str) -> Self {
        Self::try_new(
            text,
            LineIndexLimits {
                max_lines: usize::MAX,
                max_index_bytes: usize::MAX,
            },
        )
        .unwrap_or_else(|error| panic!("failed to build line index: {error}"))
    }

    /// Builds a reusable full line index within explicit resource limits.
    ///
    /// The source is borrowed rather than copied. The complete line-start table
    /// is counted before allocation, checked against both limits, and reserved
    /// fallibly so hostile newline-heavy input returns an error instead of
    /// relying on an allocator panic.
    pub fn try_new(text: &'text str, limits: LineIndexLimits) -> Result<Self, EditError> {
        let line_count = count_lines(text, limits.max_lines)?;
        check_index_bytes::<usize>(line_count, limits.max_index_bytes)?;

        let mut starts = Vec::new();
        starts
            .try_reserve_exact(line_count)
            .map_err(|_| index_allocation_error())?;
        starts.push(0);
        starts.extend(text.match_indices('\n').map(|(offset, _)| offset + 1));
        Ok(Self { text, starts })
    }

    /// Resolves a 1-based line and 0-based UTF-16 code-unit position.
    ///
    /// The line feed is excluded from the line length. For compatibility with
    /// `weavatrix.edit-plan.v1`, a preceding carriage return remains part of a
    /// CRLF line. Positions inside an astral Unicode scalar fail closed.
    pub fn byte_offset(&self, position: Position) -> Result<usize, EditError> {
        self.byte_offset_with_encoding(position, PositionEncoding::Utf16)
    }

    /// Resolves a strict position using UTF-8, UTF-16, or UTF-32 character units.
    pub fn byte_offset_with_encoding(
        &self,
        position: Position,
        encoding: PositionEncoding,
    ) -> Result<usize, EditError> {
        if position.line == 0 {
            return Err(position_error(position, "line numbers are 1-based"));
        }
        let line_index = usize::try_from(position.line - 1)
            .map_err(|_| position_error(position, "line number is too large"))?;
        let Some(&start) = self.starts.get(line_index) else {
            return Err(position_error(position, "line exceeds file line count"));
        };
        let end = self
            .starts
            .get(line_index + 1)
            .map_or(self.text.len(), |next| next - 1);
        resolve_character(&self.text[start..end], start, position, encoding)
    }

    /// Maps a UTF-8 byte boundary back to a strict line/character position.
    pub fn position_at(
        &self,
        byte_offset: usize,
        encoding: PositionEncoding,
    ) -> Result<Position, EditError> {
        if byte_offset > self.text.len() || !self.text.is_char_boundary(byte_offset) {
            return Err(EditError::new(
                ErrorCode::PositionOutOfRange,
                "byte offset is outside the text or splits a Unicode scalar value",
            ));
        }
        let line_index = self.starts.partition_point(|start| *start <= byte_offset) - 1;
        let start = self.starts[line_index];
        let units = count_units(&self.text[start..byte_offset], encoding);
        Ok(Position::new(
            u32::try_from(line_index + 1).map_err(|_| {
                EditError::new(ErrorCode::PositionOutOfRange, "line number exceeds u32")
            })?,
            u32::try_from(units).map_err(|_| {
                EditError::new(
                    ErrorCode::PositionOutOfRange,
                    "character offset exceeds u32",
                )
            })?,
        ))
    }

    #[must_use]
    pub fn line_count(&self) -> usize {
        self.starts.len()
    }
}

#[derive(Clone, Copy, Debug)]
pub(crate) struct SparseLine {
    line: u32,
    start: usize,
    end: usize,
    found: bool,
}

/// A one-shot position resolver whose allocation is proportional to requested
/// edit lines rather than the source's total line count.
pub(crate) struct SparseLineIndex<'text> {
    text: &'text str,
    lines: Vec<SparseLine>,
}

impl<'text> SparseLineIndex<'text> {
    pub(crate) fn try_for_line_pairs(
        text: &'text str,
        requested_line_pairs: impl ExactSizeIterator<Item = (u32, u32)>,
    ) -> Result<Self, EditError> {
        let requested_capacity = requested_line_pairs
            .len()
            .checked_mul(2)
            .ok_or_else(index_byte_limit_error)?;
        let max_index_bytes = requested_capacity
            .checked_mul(size_of::<SparseLine>())
            .ok_or_else(index_byte_limit_error)?;
        check_index_bytes::<SparseLine>(requested_capacity, max_index_bytes)?;
        let max_lines = text.len().checked_add(1).ok_or_else(line_limit_error)?;

        let mut lines = Vec::new();
        lines
            .try_reserve_exact(requested_capacity)
            .map_err(|_| index_allocation_error())?;
        for (start_line, end_line) in requested_line_pairs {
            for line in [start_line, end_line] {
                lines.push(SparseLine {
                    line,
                    start: 0,
                    end: 0,
                    found: false,
                });
            }
        }
        lines.sort_unstable_by_key(|entry| entry.line);
        lines.dedup_by_key(|entry| entry.line);

        let mut current_line = 1_usize;
        if current_line > max_lines {
            return Err(line_limit_error());
        }
        let mut line_start = 0_usize;
        let mut requested = 0_usize;

        for (offset, byte) in text.bytes().enumerate() {
            if byte != b'\n' {
                continue;
            }
            fill_sparse_line(&mut lines, &mut requested, current_line, line_start, offset);
            current_line = current_line.checked_add(1).ok_or_else(line_limit_error)?;
            if current_line > max_lines {
                return Err(line_limit_error());
            }
            line_start = offset + 1;
        }
        fill_sparse_line(
            &mut lines,
            &mut requested,
            current_line,
            line_start,
            text.len(),
        );

        Ok(Self { text, lines })
    }

    pub(crate) fn byte_offset_with_encoding(
        &self,
        position: Position,
        encoding: PositionEncoding,
    ) -> Result<usize, EditError> {
        if position.line == 0 {
            return Err(position_error(position, "line numbers are 1-based"));
        }
        let Ok(index) = self
            .lines
            .binary_search_by_key(&position.line, |entry| entry.line)
        else {
            return Err(position_error(position, "line was not indexed"));
        };
        let line = self.lines[index];
        if !line.found {
            return Err(position_error(position, "line exceeds file line count"));
        }
        resolve_character(
            &self.text[line.start..line.end],
            line.start,
            position,
            encoding,
        )
    }
}

fn fill_sparse_line(
    lines: &mut [SparseLine],
    requested: &mut usize,
    current_line: usize,
    start: usize,
    end: usize,
) {
    while let Some(entry) = lines.get_mut(*requested) {
        let Ok(requested_line) = usize::try_from(entry.line) else {
            break;
        };
        if requested_line > current_line {
            break;
        }
        if requested_line == current_line {
            entry.start = start;
            entry.end = end;
            entry.found = true;
        }
        *requested += 1;
    }
}

fn count_lines(text: &str, max_lines: usize) -> Result<usize, EditError> {
    let mut lines = 1_usize;
    if lines > max_lines {
        return Err(line_limit_error());
    }
    for byte in text.bytes() {
        if byte == b'\n' {
            lines = lines.checked_add(1).ok_or_else(line_limit_error)?;
            if lines > max_lines {
                return Err(line_limit_error());
            }
        }
    }
    Ok(lines)
}

fn check_index_bytes<Entry>(entries: usize, max_index_bytes: usize) -> Result<(), EditError> {
    let Some(index_bytes) = entries.checked_mul(size_of::<Entry>()) else {
        return Err(index_byte_limit_error());
    };
    if index_bytes > max_index_bytes {
        return Err(index_byte_limit_error());
    }
    Ok(())
}

fn line_limit_error() -> EditError {
    EditError::new(ErrorCode::PlanTooLarge, "line count exceeds index limits")
}

fn index_byte_limit_error() -> EditError {
    EditError::new(ErrorCode::PlanTooLarge, "line index exceeds its byte limit")
}

fn index_allocation_error() -> EditError {
    EditError::new(
        ErrorCode::PlanTooLarge,
        "line index allocation could not be reserved",
    )
}

fn resolve_character(
    line: &str,
    absolute_start: usize,
    position: Position,
    encoding: PositionEncoding,
) -> Result<usize, EditError> {
    let target = usize::try_from(position.character)
        .map_err(|_| position_error(position, "character is too large"))?;
    if line.is_ascii() || encoding == PositionEncoding::Utf8 {
        if encoding == PositionEncoding::Utf8 && !line.is_char_boundary(target) {
            return Err(position_error(
                position,
                "UTF-8 character offset splits a Unicode scalar value",
            ));
        }
        return (target <= line.len())
            .then_some(absolute_start + target)
            .ok_or_else(|| position_error(position, "character exceeds line length"));
    }

    let mut utf16_offset = 0_usize;
    for (byte_offset, character) in line.char_indices() {
        if utf16_offset == target {
            return Ok(absolute_start + byte_offset);
        }
        let next = utf16_offset
            + match encoding {
                PositionEncoding::Utf8 => character.len_utf8(),
                PositionEncoding::Utf16 => character.len_utf16(),
                PositionEncoding::Utf32 => 1,
            };
        if target < next {
            return Err(position_error(
                position,
                "character splits a Unicode scalar value",
            ));
        }
        utf16_offset = next;
    }
    if utf16_offset == target {
        Ok(absolute_start + line.len())
    } else {
        Err(position_error(position, "character exceeds line length"))
    }
}

fn count_units(text: &str, encoding: PositionEncoding) -> usize {
    match encoding {
        PositionEncoding::Utf8 => text.len(),
        PositionEncoding::Utf16 => text.encode_utf16().count(),
        PositionEncoding::Utf32 => text.chars().count(),
    }
}

fn position_error(position: Position, message: &str) -> EditError {
    EditError::new(
        ErrorCode::PositionOutOfRange,
        format!("{message} at {}:{}", position.line, position.character),
    )
}

#[cfg(test)]
mod tests {
    use crate::{error::ErrorCode, model::Position};

    #[test]
    fn indexes_lf_crlf_and_final_empty_line() {
        let index = super::LineIndex::new("a\r\nemoji 😀\n");
        assert_eq!(index.line_count(), 3);
        assert_eq!(index.byte_offset(Position::new(1, 2)).unwrap(), 2);
        assert_eq!(index.byte_offset(Position::new(2, 8)).unwrap(), 13);
        assert_eq!(index.byte_offset(Position::new(3, 0)).unwrap(), 14);
    }

    #[test]
    fn rejects_one_past_lf_and_split_surrogate() {
        let index = super::LineIndex::new("a\nb😀");
        assert_eq!(
            index.byte_offset(Position::new(1, 2)).unwrap_err().code(),
            ErrorCode::PositionOutOfRange
        );
        assert_eq!(
            index.byte_offset(Position::new(2, 2)).unwrap_err().code(),
            ErrorCode::PositionOutOfRange
        );
    }
}
