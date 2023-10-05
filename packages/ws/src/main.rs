use actix_web::{error::ErrorInternalServerError, Result};
use async_trait::async_trait;
use aws_config::endpoint;
use aws_lambda_events::apigw::ApiGatewayWebsocketProxyRequestContext;
use aws_sdk_apigatewaymanagement::{
    config::{self, Region},
    primitives::Blob,
    Client,
};
use lambda_runtime::{service_fn, LambdaEvent};
use moosicbox_ws::api::{
    EventType, Response, WebsocketConnectError, WebsocketContext, WebsocketSendError,
    WebsocketSender,
};
use serde_json::Value;

#[actix_rt::main]
async fn main() -> Result<(), actix_web::Error> {
    lambda_runtime::run(service_fn(ws_handler))
        .await
        .map_err(|e| ErrorInternalServerError(format!("Error: {e:?}")))?;
    Ok(())
}

struct ApiGatewayWebsocketSender<'a> {
    endpoint_url: &'a str,
}

#[async_trait]
impl WebsocketSender for ApiGatewayWebsocketSender<'_> {
    async fn send(&self, connection_id: &str, data: &str) -> Result<(), WebsocketSendError> {
        let shared_config = aws_config::from_env()
            .region(Region::new("us-east-1"))
            .load()
            .await;

        let api_management_config = config::Builder::from(&shared_config)
            .endpoint_url(self.endpoint_url)
            .build();

        let client = Client::from_conf(api_management_config);

        client
            .post_to_connection()
            .connection_id(connection_id)
            .data(Blob::new(data))
            .send()
            .await
            .map_err(|_e| WebsocketSendError::Unknown)?;

        Ok(())
    }
}

pub async fn ws_handler(event: LambdaEvent<Value>) -> Result<Response, WebsocketConnectError> {
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
            EventType::Connect => moosicbox_ws::api::connect(&context).await?,
            EventType::Disconnect => moosicbox_ws::api::disconnect(&context).await?,
            EventType::Message => {
                let domain = &api_context.domain_name.unwrap();
                let stage = &api_context.stage.unwrap();

                let _send = ApiGatewayWebsocketSender {
                    endpoint_url: &format!("https://{domain}/{stage}"),
                };

                moosicbox_ws::api::message(
                    &serde_json::from_str(event.payload.get("body").unwrap().as_str().unwrap())
                        .unwrap(),
                    &context,
                )
                .await?
            }
        };

        Ok(response)
    } else {
        Err(WebsocketConnectError::Unknown)
    }
}
