use std::sync::{Arc, Mutex, OnceLock};

use actix_web::{error::ErrorInternalServerError, Result};
use aws_config::SdkConfig;
use aws_lambda_events::apigw::ApiGatewayWebsocketProxyRequestContext;
use aws_sdk_apigatewaymanagement::{
    config::{self, Region},
    primitives::Blob,
    Client,
};
use futures::executor;
use lambda_runtime::{service_fn, LambdaEvent};
use moosicbox_ws::api::{
    EventType, InputMessageType, Response, WebsocketConnectError, WebsocketContext,
    WebsocketSendError, WebsocketSender,
};
use serde_json::Value;

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

impl WebsocketSender for ApiGatewayWebsocketSender<'_> {
    fn send(&mut self, connection_id: &str, data: &str) -> Result<(), WebsocketSendError> {
        self.messages.push(Message {
            connection_id: connection_id.into(),
            payload: data.into(),
        });
        Ok(())
    }
}

pub async fn ws_handler(event: LambdaEvent<Value>) -> Result<Response, WebsocketConnectError> {
    let api_context = serde_json::from_value::<ApiGatewayWebsocketProxyRequestContext>(
        event.payload.get("requestContext").unwrap().clone(),
    )
    .unwrap();

    static SHARED_CONFIG: OnceLock<Arc<Mutex<SdkConfig>>> = OnceLock::new();
    let shared_config = SHARED_CONFIG.get_or_init(|| {
        Arc::new(Mutex::new(executor::block_on(
            aws_config::from_env()
                .region(Region::new("us-east-1"))
                .load(),
        )))
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
            EventType::Connect => moosicbox_ws::api::connect(&context)?,
            EventType::Disconnect => moosicbox_ws::api::disconnect(&context)?,
            EventType::Message => {
                let body = serde_json::from_str::<Value>(
                    event.payload.get("body").unwrap().as_str().unwrap(),
                )
                .unwrap();
                let message_type = serde_json::from_str::<InputMessageType>(
                    format!("\"{}\"", body.get("type").unwrap().as_str().unwrap()).as_str(),
                )
                .unwrap();
                let payload = body.get("payload");
                moosicbox_ws::api::message(&mut sender, payload, message_type, &context)?
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
                    .map_err(|_e| WebsocketConnectError::Unknown)?;
            }
        }

        Ok(response)
    } else {
        Err(WebsocketConnectError::Unknown)
    }
}
