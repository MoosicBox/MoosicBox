#![cfg_attr(feature = "fail-on-warnings", deny(warnings))]

use actix_web::{error::ErrorInternalServerError, Result};
use aws_lambda_events::apigw::ApiGatewayWebsocketProxyRequestContext;
use lambda_runtime::{service_fn, LambdaEvent};
use log::debug;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use thiserror::Error;

#[actix_rt::main]
async fn main() -> Result<(), actix_web::Error> {
    env_logger::init();

    lambda_runtime::run(service_fn(ws_handler))
        .await
        .map_err(|e| ErrorInternalServerError(format!("Error: {e:?}")))?;
    Ok(())
}

#[derive(Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Response {
    pub status_code: u16,
    pub body: String,
}

#[derive(Debug, Serialize, Deserialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum EventType {
    Connect,
    Disconnect,
    Message,
}

pub struct WebsocketContext {
    pub connection_id: String,
    pub event_type: EventType,
}

#[derive(Debug, Error)]
pub enum WebsocketHandlerError {
    #[error("Unknown: {0:?}")]
    Unknown(String),
}

pub async fn ws_handler(event: LambdaEvent<Value>) -> Result<Response, WebsocketHandlerError> {
    let response = match handler(event).await {
        Ok(resp) => resp,
        Err(err) => Response {
            status_code: 500,
            body: err.to_string(),
        },
    };

    debug!("Response: {response:?}");

    Ok(response)
}

async fn handler(event: LambdaEvent<Value>) -> Result<Response, WebsocketHandlerError> {
    let api_context = serde_json::from_value::<ApiGatewayWebsocketProxyRequestContext>(
        event.payload.get("requestContext").unwrap().clone(),
    )
    .unwrap();

    if let Ok(event_type) = serde_json::from_str::<EventType>(
        format!("\"{}\"", api_context.clone().event_type.unwrap().as_str()).as_str(),
    ) {
        let context = WebsocketContext {
            connection_id: api_context.clone().connection_id.unwrap(),
            event_type,
        };

        let response = match context.event_type {
            EventType::Connect => Response {
                status_code: 200,
                body: "".into(),
            },
            EventType::Disconnect => Response {
                status_code: 200,
                body: "".into(),
            },
            EventType::Message => Response {
                status_code: 200,
                body: "".into(),
            },
        };

        Ok(response)
    } else {
        Err(WebsocketHandlerError::Unknown("Invalid Event Type".into()))
    }
}
