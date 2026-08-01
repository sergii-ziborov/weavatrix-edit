use std::io::{self, Write};

use weavatrix_edit::{ByteEdit, Provenance, prepare_byte_edits};

const PROVEN: &str = Provenance::EXACT_LSP;

#[test]
fn prepared_edits_stream_the_same_bytes_as_in_memory_application() {
    let source = "alpha beta gamma";
    let edits = [
        ByteEdit::replace(6..10, "beta", "B", PROVEN),
        ByteEdit::insert(0, "<", PROVEN),
        ByteEdit::insert(source.len(), ">", PROVEN),
    ];
    let prepared = prepare_byte_edits(source, &edits).unwrap();
    let expected = prepared.apply();
    let mut output = Vec::new();

    let summary = prepared.write_to(&mut output).unwrap();

    assert_eq!(output, expected.text.as_bytes());
    assert_eq!(summary.edits_applied, expected.edits_applied);
    assert_eq!(summary.bytes_before, expected.bytes_before);
    assert_eq!(summary.bytes_written, expected.bytes_after);
}

#[test]
fn chunks_preserve_insert_order_and_skip_empty_segments() {
    let source = "abc";
    let edits = [
        ByteEdit::insert(0, "<", PROVEN),
        ByteEdit::insert(0, "[", PROVEN),
        ByteEdit::delete(1..2, "b", PROVEN),
        ByteEdit::insert(3, ">", PROVEN),
    ];
    let prepared = prepare_byte_edits(source, &edits).unwrap();
    let chunks = prepared.chunks().collect::<Vec<_>>();

    assert_eq!(chunks, ["<", "[", "a", "c", ">"]);
    assert_eq!(chunks.concat(), prepared.apply().text);
}

#[test]
fn writer_failures_are_returned_without_panicking() {
    let source = "alpha beta";
    let prepared =
        prepare_byte_edits(source, &[ByteEdit::replace(6..10, "beta", "gamma", PROVEN)]).unwrap();
    let mut writer = FailingWriter {
        remaining: 7,
        bytes: Vec::new(),
    };

    let error = prepared.write_to(&mut writer).unwrap_err();

    assert_eq!(error.kind(), io::ErrorKind::BrokenPipe);
    assert_eq!(writer.bytes, b"alpha g");
}

struct FailingWriter {
    remaining: usize,
    bytes: Vec<u8>,
}

impl Write for FailingWriter {
    fn write(&mut self, buffer: &[u8]) -> io::Result<usize> {
        if self.remaining == 0 {
            return Err(io::Error::new(io::ErrorKind::BrokenPipe, "closed"));
        }
        let written = self.remaining.min(buffer.len());
        self.bytes.extend_from_slice(&buffer[..written]);
        self.remaining -= written;
        Ok(written)
    }

    fn flush(&mut self) -> io::Result<()> {
        Ok(())
    }
}
