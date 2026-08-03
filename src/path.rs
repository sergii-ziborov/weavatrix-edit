use crate::error::{EditError, ErrorCode};

/// Validates a portable, repository-relative plan path.
pub fn validate_plan_path(path: &str, max_bytes: usize) -> Result<(), EditError> {
    if path.is_empty() {
        return Err(invalid("file path must be non-empty"));
    }
    if path.len() > max_bytes {
        return Err(invalid(format!(
            "file path exceeds the {max_bytes}-byte limit"
        )));
    }
    if path.starts_with('/') {
        return Err(invalid("file path must be repository-relative"));
    }
    if path.contains('\\') {
        return Err(invalid("file path must use forward slashes"));
    }
    if path.contains(':') {
        return Err(invalid("file path may not contain ':'"));
    }
    if path
        .chars()
        .any(|character| character <= '\u{1f}' || character == '\u{7f}')
    {
        return Err(invalid("file path may not contain control characters"));
    }

    for segment in path.split('/') {
        validate_segment(segment)?;
    }
    Ok(())
}

/// Returns a conservative key for detecting paths that alias on Windows.
#[must_use]
pub fn portable_path_key(path: &str) -> String {
    let mut key = String::with_capacity(path.len());
    for (index, segment) in path.split('/').enumerate() {
        if index != 0 {
            key.push('/');
        }
        let trimmed = segment.trim_end_matches(['.', ' ']);
        if trimmed.is_ascii() {
            for character in trimmed.chars() {
                key.push(character.to_ascii_lowercase());
            }
        } else {
            // `str::to_lowercase` is context-sensitive (Greek final sigma), so
            // non-ASCII segments fold as a whole, exactly like the previous
            // per-segment implementation.
            key.push_str(&trimmed.to_lowercase());
        }
    }
    key
}

fn validate_segment(segment: &str) -> Result<(), EditError> {
    if segment.is_empty() {
        return Err(invalid("file path contains an empty segment"));
    }
    if segment == "." || segment == ".." {
        return Err(invalid("file path may not contain '.' or '..' segments"));
    }
    if segment.ends_with(['.', ' ']) {
        return Err(invalid("file path segments may not end in dots or spaces"));
    }
    if segment.eq_ignore_ascii_case(".git") {
        return Err(invalid("file path may not target .git"));
    }
    if is_windows_device(segment) {
        return Err(invalid("file path may not use a Windows device name"));
    }
    Ok(())
}

fn is_windows_device(segment: &str) -> bool {
    let base = segment.split('.').next().unwrap_or(segment);
    if base.eq_ignore_ascii_case("CON")
        || base.eq_ignore_ascii_case("PRN")
        || base.eq_ignore_ascii_case("AUX")
        || base.eq_ignore_ascii_case("NUL")
        || base.eq_ignore_ascii_case("CONIN$")
        || base.eq_ignore_ascii_case("CONOUT$")
    {
        return true;
    }
    if base.len() == 4 {
        let bytes = base.as_bytes();
        let prefix = &bytes[..3];
        let suffix = bytes[3];
        return (prefix.eq_ignore_ascii_case(b"COM") || prefix.eq_ignore_ascii_case(b"LPT"))
            && matches!(suffix, b'1'..=b'9');
    }
    false
}

fn invalid(message: impl Into<String>) -> EditError {
    EditError::new(ErrorCode::InvalidPath, message)
}

#[cfg(test)]
mod tests {
    #[test]
    fn rejects_portability_and_device_aliases() {
        for path in [
            "",
            "/a.rs",
            "a\\b.rs",
            "C:/a.rs",
            "a//b.rs",
            "./a.rs",
            "../a.rs",
            ".GIT/config",
            "src./a.rs",
            "src/a.rs ",
            "NUL",
            "con.txt",
            "COM1.rs",
            "src/\0bad.rs",
        ] {
            assert!(super::validate_plan_path(path, 4_096).is_err(), "{path:?}");
        }
    }

    #[test]
    fn portable_key_folds_case_and_windows_suffixes() {
        assert_eq!(super::portable_path_key("Src/Foo.RS"), "src/foo.rs");
        assert_eq!(super::portable_path_key("src./Foo "), "src/foo");
    }

    #[test]
    fn portable_key_matches_the_segment_wise_lowercase_reference() {
        for path in [
            "src/lib.rs",
            "Src/Foo.RS",
            "src./Foo ",
            "a//b.rs ",
            "trailing/",
            "src/模块_0001/Файл_0001.TS",
            "greek\u{3a3}/UNIT.rs",
            "mixed\u{130}name/ascii.rs",
            "\u{212b}ngstr\u{f6}m/UNIT.rs",
        ] {
            let reference = path
                .split('/')
                .map(|segment| segment.trim_end_matches(['.', ' ']).to_lowercase())
                .collect::<Vec<_>>()
                .join("/");
            assert_eq!(super::portable_path_key(path), reference, "{path:?}");
        }
    }
}
