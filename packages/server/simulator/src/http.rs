//! HTTP utilities for making requests and parsing responses in simulations.
//!
//! This module provides utilities for working with HTTP in the simulated environment,
//! including making HTTP requests over simulated TCP streams and parsing HTTP responses
//! into structured components.

use std::{collections::BTreeMap, io};

use simvar::switchy::tcp::TcpStream;
use tokio::io::{AsyncReadExt as _, AsyncWriteExt as _};

/// HTTP response with status code, headers, and body.
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
