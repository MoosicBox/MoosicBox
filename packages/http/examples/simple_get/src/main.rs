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
