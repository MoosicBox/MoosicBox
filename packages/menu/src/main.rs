mod handler;

use actix_web::error::ErrorBadRequest;
use lambda_runtime::{service_fn, Error, LambdaEvent};
use moosicbox_core::slim::menu::Album;
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use thiserror::Error;

#[derive(Debug, Error)]
pub enum MenuError {
    #[error(transparent)]
    BadRequest(#[from] actix_web::Error),
    #[error("Internal server error: {error:?}")]
    InternalServerError { error: String },
    #[error("Not Found Error: {error:?}")]
    NotFound { error: String },
}

#[derive(Serialize, Deserialize)]
#[serde(untagged)]
pub enum MenuResponse {
    Albums(Vec<Album>),
    Error(Value),
}

#[actix_web::main]
async fn main() -> Result<(), Error> {
    let func = service_fn(handler_wrapper);
    lambda_runtime::run(func).await?;
    Ok(())
}

fn to_lambda_response(error: MenuError) -> MenuResponse {
    MenuResponse::Error(match error {
        MenuError::BadRequest(error) => json!({
            "body": error.to_string(),
            "statusCode": 400
        }),
        MenuError::NotFound { error } => json!({
            "body": error,
            "statusCode": 404
        }),
        MenuError::InternalServerError { error } => json!({
            "body": error.to_string(),
            "statusCode": 500
        }),
    })
}

async fn handler_wrapper(event: LambdaEvent<Value>) -> Result<MenuResponse, MenuError> {
    return Ok(handler(event).await.unwrap_or_else(to_lambda_response));
}

async fn handler(event: LambdaEvent<Value>) -> Result<MenuResponse, MenuError> {
    let (event, context) = event.into_parts();

    let path = event["rawPath"]
        .as_str()
        .ok_or(MenuError::BadRequest(ErrorBadRequest("Bad request")))?;

    match path.to_lowercase().as_str() {
        "/albums" => handler::albums(&event, &context)
            .await
            .map(MenuResponse::Albums),
        _ => Err(MenuError::NotFound {
            error: format!("Resource '{path}' not found"),
        }),
    }
}
