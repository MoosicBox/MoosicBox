//! HTTP utilities for making requests and parsing responses in simulations.
//!
//! This module provides utilities for working with HTTP in the simulated environment,
//! including making HTTP requests over simulated TCP streams and parsing HTTP responses
//! into structured components.

use std::{collections::BTreeMap, io};

use simvar::switchy::tcp::TcpStream;
use tokio::io::{AsyncReadExt as _, AsyncWriteExt as _};

/// HTTP response with status code, headers, and body.
#[derive(Debug)]
pub struct HttpResponse {
    /// HTTP status code.
    pub status_code: u16,
    /// HTTP headers as key-value pairs.
    pub headers: BTreeMap<String, String>,
    /// HTTP response body.
    pub body: String,
}

/// Checks if headers contain expected key-value pairs in order.
///
/// Returns `true` if all expected headers appear in the actual headers in the specified order.
#[must_use]
pub fn headers_contains_in_order(
    expected: &[(String, String)],
    actual: &BTreeMap<String, String>,
) -> bool {
    let mut iter = actual.iter();
    for expected in expected {
        loop {
            if let Some(actual) = iter.next() {
                if &expected.0 == actual.0 && &expected.1 == actual.1 {
                    break;
                }
            } else {
                return false;
            }
        }
    }

    true
}

/// Makes an HTTP request over a simulated TCP stream.
///
/// # Errors
///
/// * If fails to read/write any bytes from/to the `TcpStream`
pub async fn http_request(method: &str, stream: &mut TcpStream, path: &str) -> io::Result<String> {
    let host = "127.0.0.1";

    let request = format!(
        "{method} {path} HTTP/1.1\r\n\
         Host: {host}\r\n\
         Connection: close\r\n\
         \r\n"
    );

    let bytes = request.as_bytes();
    log::trace!(
        "http_request: method={method} path={path} sending {} bytes",
        bytes.len()
    );
    stream.write_all(bytes).await?;
    stream.write_all(&[]).await?;
    stream.flush().await?;

    let mut response = String::new();
    stream.read_to_string(&mut response).await?;

    Ok(response)
}

/// Parses a raw HTTP response string into structured components.
///
/// # Errors
///
/// * If invalid HTTP response format
/// * If no headers on HTTP response
/// * If invalid status line
/// * If invalid status code
///
/// # Panics
///
/// This function does not panic.
pub fn parse_http_response(raw_response: &str) -> Result<HttpResponse, &'static str> {
    // Split the response into headers and body sections
    let parts: Vec<&str> = raw_response.split("\r\n\r\n").collect();
    if parts.len() < 2 {
        return Err("Invalid HTTP response format");
    }

    let headers_section = parts[0];
    let body = parts[1..].join("\r\n\r\n"); // Join in case there are \r\n\r\n sequences in the body

    // Parse the headers section
    let header_lines: Vec<&str> = headers_section.split("\r\n").collect();
    if header_lines.is_empty() {
        return Err("No headers found");
    }

    // Parse the status line
    let status_line = header_lines[0];
    let status_parts: Vec<&str> = status_line.split_whitespace().collect();
    if status_parts.len() < 3 {
        return Err("Invalid status line");
    }

    // Extract the status code
    let Ok(status_code) = status_parts[1].parse::<u16>() else {
        return Err("Invalid status code");
    };

    // Parse the headers
    let mut headers = BTreeMap::new();
    for line in header_lines.into_iter().skip(1) {
        if let Some(colon_pos) = line.find(':') {
            let key = line[..colon_pos].trim();
            let value = line[colon_pos + 1..].trim();
            headers.insert(key.to_string(), value.to_string());
        }
    }

    // If Content-Length is specified, we might want to truncate the body accordingly
    if let Some(content_length_str) = headers.iter().find_map(|(key, value)| {
        if key == "Content-Length" {
            Some(value)
        } else {
            None
        }
    }) && let Ok(content_length) = content_length_str.parse::<usize>()
    {
        // Ensure we don't read beyond the specified content length
        // This is a simplification; actual HTTP might have complex encoding
        if body.len() >= content_length {
            let truncated_body = &body[..content_length];
            return Ok(HttpResponse {
                status_code,
                headers,
                body: truncated_body.to_string(),
            });
        }
    }

    Ok(HttpResponse {
        status_code,
        headers,
        body,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    mod headers_contains_in_order {
        use super::*;

        #[test_log::test]
        fn returns_true_for_empty_expected_headers() {
            let expected: &[(String, String)] = &[];
            let actual = BTreeMap::new();
            assert!(headers_contains_in_order(expected, &actual));
        }

        #[test_log::test]
        fn returns_true_when_single_header_matches_exactly() {
            let expected = vec![("content-type".to_string(), "application/json".to_string())];
            let mut actual = BTreeMap::new();
            actual.insert("content-type".to_string(), "application/json".to_string());
            assert!(headers_contains_in_order(&expected, &actual));
        }

        #[test_log::test]
        fn returns_true_when_headers_appear_in_order_with_extras() {
            let expected = vec![
                ("a".to_string(), "1".to_string()),
                ("c".to_string(), "3".to_string()),
            ];
            let mut actual = BTreeMap::new();
            actual.insert("a".to_string(), "1".to_string());
            actual.insert("b".to_string(), "2".to_string());
            actual.insert("c".to_string(), "3".to_string());
            assert!(headers_contains_in_order(&expected, &actual));
        }

        #[test_log::test]
        fn returns_false_when_expected_header_is_missing() {
            let expected = vec![("missing".to_string(), "value".to_string())];
            let actual = BTreeMap::new();
            assert!(!headers_contains_in_order(&expected, &actual));
        }

        #[test_log::test]
        fn returns_false_when_header_value_does_not_match() {
            let expected = vec![("content-type".to_string(), "text/html".to_string())];
            let mut actual = BTreeMap::new();
            actual.insert("content-type".to_string(), "application/json".to_string());
            assert!(!headers_contains_in_order(&expected, &actual));
        }

        #[test_log::test]
        fn returns_false_when_headers_appear_out_of_order() {
            let expected = vec![
                ("z".to_string(), "26".to_string()),
                ("a".to_string(), "1".to_string()),
            ];
            let mut actual = BTreeMap::new();
            actual.insert("a".to_string(), "1".to_string());
            actual.insert("z".to_string(), "26".to_string());
            // BTreeMap iterates in sorted order: a, z
            // Expected wants z before a, which cannot happen
            assert!(!headers_contains_in_order(&expected, &actual));
        }
    }

    mod parse_http_response {
        use super::*;

        #[test_log::test]
        fn parses_basic_http_response() {
            let raw = "HTTP/1.1 200 OK\r\nContent-Type: application/json\r\n\r\n{\"healthy\":true}";
            let result = parse_http_response(raw).unwrap();
            assert_eq!(result.status_code, 200);
            assert_eq!(
                result.headers.get("Content-Type"),
                Some(&"application/json".to_string())
            );
            assert_eq!(result.body, "{\"healthy\":true}");
        }

        #[test_log::test]
        fn parses_response_with_multiple_headers() {
            let raw = "HTTP/1.1 404 Not Found\r\nContent-Type: text/plain\r\nX-Custom: value\r\n\r\nNot found";
            let result = parse_http_response(raw).unwrap();
            assert_eq!(result.status_code, 404);
            assert_eq!(result.headers.len(), 2);
            assert_eq!(
                result.headers.get("Content-Type"),
                Some(&"text/plain".to_string())
            );
            assert_eq!(result.headers.get("X-Custom"), Some(&"value".to_string()));
            assert_eq!(result.body, "Not found");
        }

        #[test_log::test]
        fn handles_body_with_crlf_crlf_sequence() {
            let raw = "HTTP/1.1 200 OK\r\n\r\nLine1\r\n\r\nLine2";
            let result = parse_http_response(raw).unwrap();
            assert_eq!(result.status_code, 200);
            assert_eq!(result.body, "Line1\r\n\r\nLine2");
        }

        #[test_log::test]
        fn truncates_body_to_content_length_when_specified() {
            let raw = "HTTP/1.1 200 OK\r\nContent-Length: 5\r\n\r\nHello World Extra";
            let result = parse_http_response(raw).unwrap();
            assert_eq!(result.status_code, 200);
            assert_eq!(result.body, "Hello");
        }

        #[test_log::test]
        fn does_not_truncate_when_body_shorter_than_content_length() {
            let raw = "HTTP/1.1 200 OK\r\nContent-Length: 100\r\n\r\nShort";
            let result = parse_http_response(raw).unwrap();
            assert_eq!(result.body, "Short");
        }

        #[test_log::test]
        fn returns_error_for_missing_body_separator() {
            let raw = "HTTP/1.1 200 OK\r\nContent-Type: text/plain";
            let result = parse_http_response(raw);
            assert_eq!(result.unwrap_err(), "Invalid HTTP response format");
        }

        #[test_log::test]
        fn returns_error_for_invalid_status_line() {
            let raw = "HTTP/1.1\r\n\r\nBody";
            let result = parse_http_response(raw);
            assert_eq!(result.unwrap_err(), "Invalid status line");
        }

        #[test_log::test]
        fn returns_error_for_non_numeric_status_code() {
            let raw = "HTTP/1.1 ABC OK\r\n\r\nBody";
            let result = parse_http_response(raw);
            assert_eq!(result.unwrap_err(), "Invalid status code");
        }

        #[test_log::test]
        fn parses_response_with_empty_body() {
            let raw = "HTTP/1.1 204 No Content\r\n\r\n";
            let result = parse_http_response(raw).unwrap();
            assert_eq!(result.status_code, 204);
            assert_eq!(result.body, "");
        }

        #[test_log::test]
        fn parses_various_status_codes() {
            let test_cases = [
                ("HTTP/1.1 100 Continue\r\n\r\n", 100),
                ("HTTP/1.1 301 Moved Permanently\r\n\r\n", 301),
                ("HTTP/1.1 400 Bad Request\r\n\r\n", 400),
                ("HTTP/1.1 500 Internal Server Error\r\n\r\n", 500),
            ];
            for (raw, expected_code) in test_cases {
                let result = parse_http_response(raw).unwrap();
                assert_eq!(result.status_code, expected_code, "Failed for input: {raw}");
            }
        }

        #[test_log::test]
        fn handles_headers_with_colons_in_value() {
            let raw = "HTTP/1.1 200 OK\r\nDate: Mon, 01 Jan 2024 00:00:00 GMT\r\n\r\nBody";
            let result = parse_http_response(raw).unwrap();
            assert_eq!(
                result.headers.get("Date"),
                Some(&"Mon, 01 Jan 2024 00:00:00 GMT".to_string())
            );
        }

        #[test_log::test]
        fn trims_whitespace_from_header_keys_and_values() {
            let raw = "HTTP/1.1 200 OK\r\n  Key  :  Value  \r\n\r\nBody";
            let result = parse_http_response(raw).unwrap();
            assert_eq!(result.headers.get("Key"), Some(&"Value".to_string()));
        }
    }
}
