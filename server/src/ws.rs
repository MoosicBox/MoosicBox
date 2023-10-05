use std::time::{Duration, Instant};

use actix::prelude::*;
use actix_web_actors::ws;
use moosicbox_ws::api::{
    EventType, InputMessageType, WebsocketContext, WebsocketMessageError, WebsocketSendError,
    WebsocketSender,
};
use thiserror::Error;
use uuid::Uuid;

const HEARTBEAT_INTERVAL: Duration = Duration::from_secs(5);
const CLIENT_TIMEOUT: Duration = Duration::from_secs(10);

pub struct Websocket {
    hb: Instant,
}

struct ActixSender<'a, A: Actor<Context = actix_web_actors::ws::WebsocketContext<A>>> {
    context: &'a mut actix_web_actors::ws::WebsocketContext<A>,
}

impl<A: Actor<Context = actix_web_actors::ws::WebsocketContext<A>>> WebsocketSender
    for ActixSender<'_, A>
{
    fn send(&mut self, _connection_id: &str, data: &str) -> Result<(), WebsocketSendError> {
        self.context.text(data);
        Ok(())
    }
}

impl Websocket {
    pub fn new() -> Self {
        Self { hb: Instant::now() }
    }

    // This function will run on an interval, every 5 seconds to check
    // that the connection is still alive. If it's been more than
    // 10 seconds since the last ping, we'll close the connection.
    fn hb(&self, ctx: &mut <Self as Actor>::Context) {
        ctx.run_interval(HEARTBEAT_INTERVAL, |act, ctx| {
            if Instant::now().duration_since(act.hb) > CLIENT_TIMEOUT {
                ctx.stop();
                return;
            }

            ctx.ping(b"");
        });
    }
}

impl Actor for Websocket {
    type Context = ws::WebsocketContext<Self>;

    // Start the heartbeat process for this connection
    fn started(&mut self, ctx: &mut Self::Context) {
        self.hb(ctx);

        let context = WebsocketContext {
            connection_id: "".into(),
            event_type: EventType::Connect,
        };

        moosicbox_ws::api::connect(&context).unwrap();
    }
}

#[derive(Debug, Error)]
pub enum WebsocketHandlerError {
    #[error(transparent)]
    Protocol(ws::ProtocolError),
    #[error(transparent)]
    WebsocketMessageError(WebsocketMessageError),
    #[error("Unknown")]
    Unknown,
}

impl StreamHandler<Result<ws::Message, ws::ProtocolError>> for Websocket {
    fn handle(&mut self, msg: Result<ws::Message, ws::ProtocolError>, ctx: &mut Self::Context) {
        let result = match msg {
            // Ping/Pong will be used to make sure the connection is still alive
            Ok(ws::Message::Ping(msg)) => {
                self.hb = Instant::now();
                println!("Ping {msg:?}");
                ctx.pong(&msg);
                Ok(())
            }
            Ok(ws::Message::Pong(_)) => {
                self.hb = Instant::now();
                Ok(())
            }
            Ok(ws::Message::Text(text)) => {
                let value = serde_json::from_str::<serde_json::Value>(text.as_ref()).unwrap();
                let connection_id = if let Some(id) = value.get("connectionId") {
                    id.as_str().unwrap().to_string()
                } else {
                    Uuid::new_v4().to_string()
                };
                let context = WebsocketContext {
                    connection_id,
                    event_type: EventType::Message,
                };
                let payload = value.get("payload");
                let message_type = serde_json::from_str::<InputMessageType>(
                    format!("\"{}\"", value.get("type").unwrap().as_str().unwrap()).as_str(),
                )
                .unwrap();
                let mut sender = ActixSender { context: ctx };
                moosicbox_ws::api::message(&mut sender, payload, message_type, &context)
                    .map(|_| ())
                    .map_err(WebsocketHandlerError::WebsocketMessageError)
            }
            Ok(ws::Message::Close(reason)) => {
                ctx.close(reason);
                ctx.stop();
                Ok(())
            }
            Err(error) => {
                ctx.stop();
                Err(WebsocketHandlerError::Protocol(error))
            }
            _ => Err(WebsocketHandlerError::Unknown),
        };

        if let Err(error) = result {
            eprintln!("WebSocket Stream Handler failed! {error:?}")
        }
    }
}
