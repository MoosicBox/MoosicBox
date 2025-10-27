//! Simple HTTP GET request example using the `switchy_http` crate.
//!
//! This binary demonstrates basic HTTP GET request functionality by accepting a URL
//! as a command-line argument, fetching the content, and printing the response text.
//!
//! # Usage
//!
//! ```bash
//! cargo run --package http_simple_get -- https://example.com
//! ```
//!
//! # Examples
//!
//! Fetch content from a URL:
//!
//! ```bash
//! http_simple_get https://httpbin.org/get
//! ```

/// Errors that can occur when running the simple GET example.
#[derive(Debug, thiserror::Error)]
pub enum Error {
    /// HTTP request error.
    #[error(transparent)]
    Http(#[from] switchy_http::Error),
    /// URL argument was not provided.
    #[error("MissingUrlArgument")]
    MissingUrlArgument,
}

#[tokio::main]
async fn main() -> Result<(), Error> {
    pretty_env_logger::init();

    let Some(url) = std::env::args().nth(1) else {
        return Err(Error::MissingUrlArgument);
    };

    log::info!("args={url:?}");

    let response = switchy_http::Client::new().get(&url).send().await?;

    println!("response: {}", response.text().await?);

    Ok(())
}
