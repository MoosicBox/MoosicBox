#![cfg_attr(feature = "fail-on-warnings", deny(warnings))]

use std::{
    str::FromStr,
    sync::{Arc, Mutex, OnceLock},
    time::Duration,
};

use actix_web::{error::ErrorInternalServerError, Result};
use async_once_cell::OnceCell;
use async_trait::async_trait;
use aws_config::{BehaviorVersion, SdkConfig};
use aws_lambda_events::apigw::ApiGatewayWebsocketProxyRequestContext;
use aws_sdk_apigatewaymanagement::{
    config::{self, Region},
    operation::post_to_connection::PostToConnectionError,
    primitives::Blob,
    Client,
};
use lambda_runtime::{service_fn, LambdaEvent};
use log::debug;
use moosicbox_core::app::Db;
use moosicbox_ws::api::{
    EventType, Response, WebsocketConnectError, WebsocketContext, WebsocketDisconnectError,
    WebsocketMessageError, WebsocketSendError, WebsocketSender,
};
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

struct ApiGatewayWebsocketSender<'a> {
    api_context: &'a ApiGatewayWebsocketProxyRequestContext,
}

static SHARED_CONFIG: OnceCell<Arc<Mutex<SdkConfig>>> = OnceCell::new();

async fn get_shared_config() -> Arc<Mutex<SdkConfig>> {
    SHARED_CONFIG
        .get_or_init(async {
            Arc::new(Mutex::new(
                aws_config::defaults(BehaviorVersion::v2023_11_09())
                    .region(Region::new("us-east-1"))
                    .load()
                    .await,
            ))
        })
        .await
        .clone()
}

#[async_trait]
impl WebsocketSender for ApiGatewayWebsocketSender<'_> {
    async fn send(&self, connection_id: &str, data: &str) -> Result<(), WebsocketSendError> {
        let domain = self.api_context.domain_name.clone().unwrap();
        let stage = self.api_context.stage.clone().unwrap();
        let endpoint_url = &format!("https://{domain}/{stage}");
        let shared_config = get_shared_config().await;
        let config = (*shared_config.clone().lock().unwrap()).clone();
        let api_management_config = config::Builder::from(&config)
            .endpoint_url(endpoint_url)
            .build();
        let client = Client::from_conf(api_management_config);
        debug!("Sending message to {}", connection_id);
        client
            .post_to_connection()
            .connection_id(connection_id)
            .data(Blob::new(data))
            .send()
            .await
            .map_err(|e| {
                let service_error = e.into_service_error();
                match service_error {
                    PostToConnectionError::GoneException(err) => {
                        WebsocketSendError::Unknown(err.to_string())
                    }
                    PostToConnectionError::ForbiddenException(err) => {
                        WebsocketSendError::Unknown(err.to_string())
                    }
                    PostToConnectionError::LimitExceededException(err) => {
                        WebsocketSendError::Unknown(err.to_string())
                    }
                    PostToConnectionError::PayloadTooLargeException(err) => {
                        WebsocketSendError::Unknown(err.to_string())
                    }
                    _ => WebsocketSendError::Unknown(service_error.to_string()),
                }
            })?;

        Ok(())
    }

    async fn send_all(&self, _data: &str) -> Result<(), WebsocketSendError> {
        Ok(())
    }

    async fn send_all_except(
        &self,
        _connection_id: &str,
        _data: &str,
    ) -> Result<(), WebsocketSendError> {
        Ok(())
    }
}

#[derive(Debug, Error)]
pub enum WebsocketHandlerError {
    #[error(transparent)]
    WebsocketConnectError(#[from] WebsocketConnectError),
    #[error(transparent)]
    WebsocketDisconnectError(#[from] WebsocketDisconnectError),
    #[error(transparent)]
    WebsocketMessageError(#[from] WebsocketMessageError),
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

    static DB: OnceLock<Db> = OnceLock::new();
    let db = DB.get_or_init(|| {
        let library = ::rusqlite::Connection::open_in_memory().unwrap();
        library
            .busy_timeout(Duration::from_millis(10))
            .expect("Failed to set busy timeout");
        Db {
            library: Arc::new(Mutex::new(library)),
        }
    });

    if let Ok(event_type) = EventType::from_str(api_context.clone().event_type.unwrap().as_str()) {
        let context = WebsocketContext {
            connection_id: api_context.clone().connection_id.unwrap(),
        };

        let sender = ApiGatewayWebsocketSender {
            api_context: &api_context,
        };
        let response = match event_type {
            EventType::Connect => moosicbox_ws::api::connect(db, &sender, &context).await?,
            EventType::Disconnect => moosicbox_ws::api::disconnect(db, &sender, &context).await?,
            EventType::Message => {
                moosicbox_ws::api::process_message(db, event.payload, context, &sender).await?
            }
        };

        Ok(response)
    } else {
        Err(WebsocketHandlerError::Unknown("Invalid Event Type".into()))
    }
}
