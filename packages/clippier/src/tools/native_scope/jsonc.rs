//! JSON-with-comments parsing without corrupting comment-like string contents.

pub(super) fn parse(text: &str) -> Result<serde_json::Value, String> {
    let bytes = text.strip_prefix('\u{feff}').unwrap_or(text).as_bytes();
    let mut clean = bytes.to_vec();
    let mut index = 0;
    let mut string = false;
    while index < bytes.len() {
        match bytes[index] {
            b'\\' if string => {
                index += 2;
                continue;
            }
            b'"' => string = !string,
            b'/' if !string && bytes.get(index + 1) == Some(&b'/') => {
                while index < bytes.len() && bytes[index] != b'\n' {
                    clean[index] = b' ';
                    index += 1;
                }
                continue;
            }
            b'/' if !string && bytes.get(index + 1) == Some(&b'*') => {
                clean[index] = b' ';
                clean[index + 1] = b' ';
                index += 2;
                loop {
                    if index + 1 >= bytes.len() {
                        return Err("unterminated JSON block comment".to_owned());
                    }
                    if bytes[index] == b'*' && bytes[index + 1] == b'/' {
                        clean[index] = b' ';
                        clean[index + 1] = b' ';
                        index += 2;
                        break;
                    }
                    if bytes[index] != b'\n' && bytes[index] != b'\r' {
                        clean[index] = b' ';
                    }
                    index += 1;
                }
                continue;
            }
            _ => {}
        }
        index += 1;
    }
    // Both Biome and dprint accept trailing commas. Preserve byte positions for
    // serde_json's diagnostics and leave commas inside string values untouched.
    index = 0;
    string = false;
    while index < clean.len() {
        match clean[index] {
            b'\\' if string => {
                index += 2;
                continue;
            }
            b'"' => string = !string,
            b',' if !string => {
                let next = clean[index + 1..]
                    .iter()
                    .find(|byte| !byte.is_ascii_whitespace());
                if matches!(next, Some(b'}' | b']')) {
                    clean[index] = b' ';
                }
            }
            _ => {}
        }
        index += 1;
    }
    serde_json::from_slice(&clean).map_err(|error| error.to_string())
}
