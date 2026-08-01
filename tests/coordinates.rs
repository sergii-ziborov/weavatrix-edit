use weavatrix_edit::{ErrorCode, LineIndex, Position, PositionEncoding};

#[test]
fn every_utf8_boundary_roundtrips_in_each_position_encoding() {
    let source = "a😀\r\nאב\nfinal";
    let index = LineIndex::new(source);

    for byte in 0..=source.len() {
        if !source.is_char_boundary(byte) {
            continue;
        }
        for encoding in [
            PositionEncoding::Utf8,
            PositionEncoding::Utf16,
            PositionEncoding::Utf32,
        ] {
            let position = index.position_at(byte, encoding).unwrap();
            assert_eq!(
                index.byte_offset_with_encoding(position, encoding).unwrap(),
                byte,
                "byte={byte}, encoding={encoding:?}, position={position:?}"
            );
        }
    }
}

#[test]
fn reverse_mapping_rejects_out_of_bounds_and_split_scalars() {
    let source = "a😀z";
    let index = LineIndex::new(source);

    for offset in [2, 3, 4, source.len() + 1] {
        assert_eq!(
            index
                .position_at(offset, PositionEncoding::Utf16)
                .unwrap_err()
                .code(),
            ErrorCode::PositionOutOfRange
        );
    }
}

#[test]
fn forward_mapping_is_strict_for_all_encodings() {
    let source = "😀";
    let index = LineIndex::new(source);

    assert_eq!(
        index
            .byte_offset_with_encoding(Position::new(0, 0), PositionEncoding::Utf16)
            .unwrap_err()
            .code(),
        ErrorCode::PositionOutOfRange
    );
    assert_eq!(
        index
            .byte_offset_with_encoding(Position::new(1, 1), PositionEncoding::Utf8)
            .unwrap_err()
            .code(),
        ErrorCode::PositionOutOfRange
    );
    assert_eq!(
        index
            .byte_offset_with_encoding(Position::new(1, 3), PositionEncoding::Utf16)
            .unwrap_err()
            .code(),
        ErrorCode::PositionOutOfRange
    );
    assert_eq!(
        index
            .byte_offset_with_encoding(Position::new(1, 2), PositionEncoding::Utf32)
            .unwrap_err()
            .code(),
        ErrorCode::PositionOutOfRange
    );
}

#[test]
fn line_feed_boundary_maps_to_the_preceding_line_end() {
    let index = LineIndex::new("ab\ncd");
    for encoding in [
        PositionEncoding::Utf8,
        PositionEncoding::Utf16,
        PositionEncoding::Utf32,
    ] {
        assert_eq!(index.position_at(2, encoding).unwrap(), Position::new(1, 2));
        assert_eq!(index.position_at(3, encoding).unwrap(), Position::new(2, 0));
    }
}
