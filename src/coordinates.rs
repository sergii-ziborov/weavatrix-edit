use crate::{
    error::{EditError, ErrorCode},
    model::{Position, PositionEncoding},
};

/// Reusable line index for strict Weavatrix v1 UTF-16 positions.
#[derive(Clone, Debug)]
pub struct LineIndex<'text> {
    text: &'text str,
    starts: Vec<usize>,
}

impl<'text> LineIndex<'text> {
    #[must_use]
    pub fn new(text: &'text str) -> Self {
        let mut starts = Vec::with_capacity(text.bytes().filter(|byte| *byte == b'\n').count() + 1);
        starts.push(0);
        starts.extend(text.match_indices('\n').map(|(offset, _)| offset + 1));
        Self { text, starts }
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
