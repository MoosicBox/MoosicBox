use std::sync::{Arc, Mutex, OnceLock};

use actix_web::{error::ErrorInternalServerError, Result};
use async_once_cell::OnceCell;
use async_trait::async_trait;
use aws_config::SdkConfig;
use aws_lambda_events::apigw::ApiGatewayWebsocketProxyRequestContext;
use aws_sdk_apigatewaymanagement::{
    config::{self, Region},
    primitives::Blob,
    Client,
};
use lambda_runtime::{service_fn, LambdaEvent};
use moosicbox_core::app::Db;
use moosicbox_ws::api::{
    EventType, InboundMessageType, Response, WebsocketConnectError, WebsocketContext,
    WebsocketDisconnectError, WebsocketMessageError, WebsocketSendError, WebsocketSender,
};
use serde_json::Value;
use thiserror::Error;

#[actix_rt::main]
async fn main() -> Result<(), actix_web::Error> {
    lambda_runtime::run(service_fn(ws_handler))
        .await
        .map_err(|e| ErrorInternalServerError(format!("Error: {e:?}")))?;
    Ok(())
}

struct Message {
    connection_id: String,
    payload: String,
}

struct ApiGatewayWebsocketSender<'a> {
    messages: &'a mut Vec<Message>,
}

#[async_trait]
impl WebsocketSender for ApiGatewayWebsocketSender<'_> {
    async fn send(&mut self, connection_id: &str, data: &str) -> Result<(), WebsocketSendError> {
        self.messages.push(Message {
            connection_id: connection_id.into(),
            payload: data.into(),
        });
        Ok(())
    }

    async fn send_all(&mut self, _data: &str) -> Result<(), WebsocketSendError> {
        Ok(())
    }

    async fn send_all_except(
        &mut self,
        _connection_id: &str,
        _data: &str,
    ) -> Result<(), WebsocketSendError> {
        Ok(())
    }
}

#[derive(Debug, Error)]
pub enum WebsocketHandlerError {
    #[error(transparent)]
    WebsocketConnectError(WebsocketConnectError),
    #[error(transparent)]
    WebsocketDisconnectError(WebsocketDisconnectError),
    #[error(transparent)]
    WebsocketMessageError(WebsocketMessageError),
    #[error("Unknown")]
    Unknown,
}

pub async fn ws_handler(event: LambdaEvent<Value>) -> Result<Response, WebsocketHandlerError> {
    let api_context = serde_json::from_value::<ApiGatewayWebsocketProxyRequestContext>(
        event.payload.get("requestContext").unwrap().clone(),
    )
    .unwrap();

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

    static DB: OnceLock<Arc<Mutex<Db>>> = OnceLock::new();
    let db = DB.get_or_init(|| {
        let library_db = ::sqlite::open(":memory:").unwrap();
        let db = Db {
            library: library_db,
        };
        Arc::new(Mutex::new(db))
    });

    let mut messages = Vec::new();

    if let Ok(event_type) = serde_json::from_str::<EventType>(
        format!("\"{}\"", api_context.clone().event_type.unwrap().as_str()).as_str(),
    ) {
        let context = WebsocketContext {
            connection_id: api_context.clone().connection_id.unwrap(),
            event_type,
        };

        let mut sender = ApiGatewayWebsocketSender {
            messages: &mut messages,
        };
        let response = match context.event_type {
            EventType::Connect => moosicbox_ws::api::connect(&mut sender, &context)
                .await
                .map_err(WebsocketHandlerError::WebsocketConnectError)?,
            EventType::Disconnect => moosicbox_ws::api::disconnect(&mut sender, &context)
                .await
                .map_err(WebsocketHandlerError::WebsocketDisconnectError)?,
            EventType::Message => {
                let body = serde_json::from_str::<Value>(
                    event.payload.get("body").unwrap().as_str().unwrap(),
                )
                .unwrap();
                let message_type = serde_json::from_str::<InboundMessageType>(
                    format!("\"{}\"", body.get("type").unwrap().as_str().unwrap()).as_str(),
                )
                .unwrap();
                let payload = body.get("payload");
                moosicbox_ws::api::message(db.clone(), &mut sender, payload, message_type, &context)
                    .await
                    .map_err(WebsocketHandlerError::WebsocketMessageError)?
            }
        };

        if !messages.is_empty() {
            let domain = &api_context.domain_name.unwrap();
            let stage = &api_context.stage.unwrap();
            let endpoint_url = &format!("https://{domain}/{stage}");
            let config = (*shared_config.clone().lock().unwrap()).clone();
            let api_management_config = config::Builder::from(&config)
                .endpoint_url(endpoint_url)
                .build();
            let client = Client::from_conf(api_management_config);
            for message in messages {
                println!(
                    "Sending message to {}: {}",
                    message.connection_id, message.payload
                );
                client
                    .post_to_connection()
                    .connection_id(message.connection_id)
                    .data(Blob::new(message.payload))
                    .send()
                    .await
                    .map_err(|_e| WebsocketHandlerError::Unknown)?;
            }
        }

        Ok(response)
    } else {
        Err(WebsocketHandlerError::Unknown)
    }
}
