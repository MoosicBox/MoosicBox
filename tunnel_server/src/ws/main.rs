#![cfg_attr(feature = "fail-on-warnings", deny(warnings))]

use std::sync::{Arc, Mutex};

use actix_web::{error::ErrorInternalServerError, Result};
use async_once_cell::OnceCell;
use aws_config::SdkConfig;
use aws_lambda_events::apigw::ApiGatewayWebsocketProxyRequestContext;
use aws_sdk_apigatewaymanagement::{
    config::{self, Region},
    operation::post_to_connection::PostToConnectionError,
    primitives::Blob,
    Client,
};
use lambda_runtime::{service_fn, LambdaEvent};
use log::{debug, info};
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

#[derive(Debug, Serialize, Deserialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum InboundMessageType {
    Ping,
    GetConnectionId,
    TunnelRequest,
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

struct Message {
    connection_id: String,
    payload: String,
}

async fn send_message(
    message: Message,
    api_context: &ApiGatewayWebsocketProxyRequestContext,
) -> Result<(), WebsocketHandlerError> {
    static SHARED_CONFIG: OnceCell<Arc<Mutex<SdkConfig>>> = OnceCell::new();
    let shared_config = SHARED_CONFIG
        .get_or_init(async {
            Arc::new(Mutex::new(
                aws_config::from_env()
                    .region(Region::new("us-east-1"))
                    .load()
                    .await,
            ))
        })
        .await;
    let domain = api_context.domain_name.clone().unwrap();
    let stage = api_context.stage.clone().unwrap();
    let endpoint_url = &format!("https://{domain}/{stage}");
    let config = (*shared_config.clone().lock().unwrap()).clone();
    let api_management_config = config::Builder::from(&config)
        .endpoint_url(endpoint_url)
        .build();
    let client = Client::from_conf(api_management_config);
    debug!(
        "Sending message to {}: {}",
        message.connection_id, message.payload
    );
    client
        .post_to_connection()
        .connection_id(message.connection_id)
        .data(Blob::new(message.payload))
        .send()
        .await
        .map_err(|e| {
            let service_error = e.into_service_error();
            match service_error {
                PostToConnectionError::GoneException(err) => {
                    WebsocketHandlerError::Unknown(err.to_string())
                }
                PostToConnectionError::ForbiddenException(err) => {
                    WebsocketHandlerError::Unknown(err.to_string())
                }
                PostToConnectionError::LimitExceededException(err) => {
                    WebsocketHandlerError::Unknown(err.to_string())
                }
                PostToConnectionError::PayloadTooLargeException(err) => {
                    WebsocketHandlerError::Unknown(err.to_string())
                }
                PostToConnectionError::Unhandled(err) => {
                    WebsocketHandlerError::Unknown(err.to_string())
                }
                _ => WebsocketHandlerError::Unknown(service_error.to_string()),
            }
        })?;

    Ok(())
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
            EventType::Connect => {
                info!("Client connected {}", context.connection_id);
                Response {
                    status_code: 200,
                    body: "".into(),
                }
            }
            EventType::Disconnect => Response {
                status_code: 200,
                body: "".into(),
            },
            EventType::Message => {
                debug!("received message {:?}", event.payload);
                let body = serde_json::from_str::<Value>(
                    event.payload.get("body").unwrap().as_str().unwrap(),
                )
                .unwrap();
                let message_type = serde_json::from_str::<InboundMessageType>(
                    format!("\"{}\"", body.get("type").unwrap().as_str().unwrap()).as_str(),
                )
                .unwrap();
                match message_type {
                    InboundMessageType::Ping => {}
                    InboundMessageType::GetConnectionId => {
                        send_message(
                            Message {
                                connection_id: context.connection_id,
                                payload: serde_json::json!({}).to_string(),
                            },
                            &api_context,
                        )
                        .await?;
                    }
                    InboundMessageType::TunnelRequest => {
                        send_message(
                            Message {
                                connection_id: "OTKzgeLXoAMCLCA=".into(), //context.connection_id,
                                payload: body.to_string(),
                            },
                            &api_context,
                        )
                        .await?;
                    }
                }
                Response {
                    status_code: 200,
                    body: "".into(),
                }
            }
        };

        Ok(response)
    } else {
        Err(WebsocketHandlerError::Unknown("Invalid Event Type".into()))
    }
}
