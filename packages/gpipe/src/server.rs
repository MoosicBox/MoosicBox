//! Authenticated HTTP boundary for the durable coordinator.
//!
//! This service deliberately binds loopback only. Remote workers connect through
//! a TLS reverse proxy; forwarded identity headers are never trusted.

use crate::{
    coordinator::Coordinator,
    protocol::{Envelope, Identity, Response, ServerConfig},
};
use actix_web::{App, HttpRequest, HttpResponse, HttpServer, web};
use std::{
    net::SocketAddr,
    path::Path,
    sync::Mutex,
    time::{SystemTime, UNIX_EPOCH},
};

struct Service {
    coordinator: Mutex<Coordinator>,
    tokens: Vec<(String, Identity)>,
    triggers: Option<crate::triggers::TriggerConfig>,
    artifact_root: std::path::PathBuf,
}

fn equal_token(left: &str, right: &str) -> bool {
    if left.len() != right.len() {
        return false;
    }
    left.bytes()
        .zip(right.bytes())
        .fold(0u8, |diff, (a, b)| diff | (a ^ b))
        == 0
}

fn token(name: &str) -> Result<String, String> {
    let value =
        std::env::var(name).map_err(|_| format!("missing token environment variable {name}"))?;
    if value.len() < 32 || value.len() > 4096 || !value.bytes().all(|b| b.is_ascii_graphic()) {
        return Err(format!(
            "{name} must contain 32..4096 printable non-space ASCII characters"
        ));
    }
    Ok(value)
}

#[allow(clippy::future_not_send)] // Actix handlers intentionally run on local worker executors.
async fn rpc(
    service: web::Data<Service>,
    request: HttpRequest,
    body: web::Json<Envelope>,
) -> HttpResponse {
    let bearer = request
        .headers()
        .get("authorization")
        .and_then(|v| v.to_str().ok())
        .and_then(|v| v.strip_prefix("Bearer "));
    let identity = bearer.and_then(|value| {
        service
            .tokens
            .iter()
            .find(|(secret, _)| equal_token(secret, value))
            .map(|(_, identity)| identity.clone())
    });
    let Some(identity) = identity else {
        return HttpResponse::Unauthorized().finish();
    };
    let Ok(now) = SystemTime::now().duration_since(UNIX_EPOCH) else {
        return HttpResponse::InternalServerError().finish();
    };
    service.coordinator.lock().map_or_else(
        |_| {
            HttpResponse::ServiceUnavailable().json(Response::Error {
                message: "coordinator unavailable".into(),
            })
        },
        |mut coordinator| {
            HttpResponse::Ok().json(coordinator.handle(&identity, body.into_inner(), now.as_secs()))
        },
    )
}

/// Serve authenticated versioned RPC on a loopback address.
///
/// # Errors
/// * Configuration or token environment variables are invalid.
/// * Tokens overlap, the database cannot be opened, or the listener fails.
/// * The HTTP server fails while running.
#[allow(clippy::future_not_send)] // HttpServer owns a local Actix runtime.
pub async fn serve(config: ServerConfig) -> Result<(), String> {
    serve_with_triggers(config, None).await
}

/// Serve RPC and optionally signed webhook ingress using trusted templates.
///
/// # Errors
/// * All configuration, authentication and listener errors documented by [`serve`].
#[allow(clippy::future_not_send)]
pub async fn serve_with_triggers(
    config: ServerConfig,
    triggers: Option<crate::triggers::TriggerConfig>,
) -> Result<(), String> {
    let address: SocketAddr = config
        .listen
        .parse()
        .map_err(|_| "listen must be an IP socket address".to_owned())?;
    if !address.ip().is_loopback() {
        return Err(
            "bind loopback and expose through a TLS reverse proxy for remote workers".into(),
        );
    }
    let mut tokens = vec![(token(&config.admin_token_env)?, Identity::Admin)];
    for (id, policy) in &config.workers {
        let secret = token(&policy.token_env)?;
        if tokens
            .iter()
            .any(|(existing, _)| equal_token(existing, &secret))
        {
            return Err("every worker and administrator requires a distinct token".into());
        }
        if policy.cpu == 0
            || policy
                .repositories
                .iter()
                .any(|repo| !config.repositories.contains_key(repo))
        {
            return Err(format!("invalid worker policy: {id}"));
        }
        tokens.push((secret, Identity::Worker(id.clone())));
    }
    let coordinator = Coordinator::open(Path::new(&config.database), config.clone())?;
    let artifact_root = std::path::PathBuf::from(format!("{}.artifacts", config.database));
    let service = web::Data::new(Service {
        coordinator: Mutex::new(coordinator),
        tokens,
        triggers,
        artifact_root,
    });
    HttpServer::new(move || {
        App::new()
            .app_data(service.clone())
            .app_data(web::JsonConfig::default().limit(2 * 1024 * 1024))
            .app_data(web::PayloadConfig::new(crate::triggers::MAX_BODY_BYTES))
            .route("/hooks/github", web::post().to(github_hook))
            .route("/hooks/generic", web::post().to(generic_hook))
            .route("/v1/artifacts/inspect", web::post().to(inspect_artifact))
            .route("/v1/artifacts", web::post().to(artifact_transfer))
            .route("/v1/rpc", web::post().to(rpc))
    })
    .workers(2)
    .bind(address)
    .map_err(|error| error.to_string())?
    .run()
    .await
    .map_err(|error| error.to_string())
}

#[allow(clippy::future_not_send)]
async fn artifact_transfer(
    service: web::Data<Service>,
    request: HttpRequest,
    body: web::Json<crate::transfer::TransferRequest>,
) -> HttpResponse {
    let bearer = header(&request, "authorization").strip_prefix("Bearer ");
    let identity = bearer.and_then(|value| {
        service
            .tokens
            .iter()
            .find(|(secret, _)| equal_token(secret, value))
            .map(|(_, identity)| identity.clone())
    });
    let Some(identity @ Identity::Worker(_)) = identity else {
        return HttpResponse::Unauthorized().finish();
    };
    let Ok(now) = SystemTime::now().duration_since(UNIX_EPOCH) else {
        return HttpResponse::InternalServerError().finish();
    };
    let Ok(mut coordinator) = service.coordinator.lock() else {
        return HttpResponse::ServiceUnavailable().finish();
    };
    // Hold authorization/state lock through the bounded chunk operation: cancellation
    // cannot race an authorized publication into a different attempt's namespace.
    let response = coordinator.handle(
        &identity,
        Envelope {
            version: crate::protocol::PROTOCOL_VERSION,
            request: if let Some(instance) = request
                .headers()
                .get(crate::transfer::WORKER_INSTANCE_HEADER)
                .and_then(|value| value.to_str().ok())
            {
                crate::protocol::Request::ArtifactAccessFor {
                    instance: instance.into(),
                    attempt_id: body.attempt_id.clone(),
                    producer_task: body.producer_task.clone(),
                }
            } else {
                crate::protocol::Request::ArtifactAccess {
                    attempt_id: body.attempt_id.clone(),
                    producer_task: body.producer_task.clone(),
                }
            },
        },
        now.as_secs(),
    );
    let Response::Data { value } = response else {
        return HttpResponse::Forbidden().finish();
    };
    let Some(producer) = value
        .get("producer_attempt_id")
        .and_then(serde_json::Value::as_str)
    else {
        return HttpResponse::InternalServerError().finish();
    };
    crate::transfer::transfer(&service.artifact_root, producer, &body).map_or_else(
        |_| HttpResponse::BadRequest().finish(),
        |response| HttpResponse::Ok().json(response),
    )
}

#[derive(serde::Serialize, serde::Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct ArtifactInspection {
    pub run_id: String,
    pub task_id: String,
    pub attempt_id: String,
    pub name: String,
    pub offset: u64,
}

#[allow(clippy::future_not_send)]
async fn inspect_artifact(
    service: web::Data<Service>,
    request: HttpRequest,
    body: web::Json<ArtifactInspection>,
) -> HttpResponse {
    let authorized = header(&request, "authorization")
        .strip_prefix("Bearer ")
        .is_some_and(|value| {
            service.tokens.iter().any(|(secret, identity)| {
                matches!(identity, Identity::Admin) && equal_token(secret, value)
            })
        });
    if !authorized {
        return HttpResponse::Unauthorized().finish();
    }
    let Ok(now) = SystemTime::now().duration_since(UNIX_EPOCH) else {
        return HttpResponse::InternalServerError().finish();
    };
    let Ok(mut coordinator) = service.coordinator.lock() else {
        return HttpResponse::ServiceUnavailable().finish();
    };
    let response = coordinator.handle(
        &Identity::Admin,
        Envelope {
            version: crate::protocol::PROTOCOL_VERSION,
            request: crate::protocol::Request::InspectArtifact {
                run_id: body.run_id.clone(),
                task_id: body.task_id.clone(),
                attempt_id: body.attempt_id.clone(),
            },
        },
        now.as_secs(),
    );
    let Response::Data { value } = response else {
        return HttpResponse::Forbidden().finish();
    };
    let Some(producer) = value
        .get("producer_attempt_id")
        .and_then(serde_json::Value::as_str)
    else {
        return HttpResponse::InternalServerError().finish();
    };
    let transfer = crate::transfer::TransferRequest {
        attempt_id: body.attempt_id.clone(),
        producer_task: Some(body.task_id.clone()),
        name: body.name.clone(),
        offset: body.offset,
        data: Vec::new(),
        finish: false,
    };
    crate::transfer::transfer(&service.artifact_root, producer, &transfer).map_or_else(
        |_| HttpResponse::BadRequest().finish(),
        |response| HttpResponse::Ok().json(response),
    )
}

fn header<'a>(request: &'a HttpRequest, name: &str) -> &'a str {
    request
        .headers()
        .get(name)
        .and_then(|value| value.to_str().ok())
        .unwrap_or("")
}

fn submit_hook(
    service: &Service,
    operation: Result<crate::protocol::Request, String>,
) -> HttpResponse {
    let Ok(operation) = operation else {
        return HttpResponse::BadRequest().finish();
    };
    let Ok(now) = SystemTime::now().duration_since(UNIX_EPOCH) else {
        return HttpResponse::InternalServerError().finish();
    };
    service.coordinator.lock().map_or_else(
        |_| HttpResponse::ServiceUnavailable().finish(),
        |mut coordinator| {
            HttpResponse::Ok().json(coordinator.handle(
                &Identity::Admin,
                Envelope {
                    version: crate::protocol::PROTOCOL_VERSION,
                    request: operation,
                },
                now.as_secs(),
            ))
        },
    )
}

#[allow(clippy::future_not_send)]
async fn github_hook(
    service: web::Data<Service>,
    request: HttpRequest,
    body: web::Bytes,
) -> HttpResponse {
    let Some(config) = &service.triggers else {
        return HttpResponse::NotFound().finish();
    };
    submit_hook(
        &service,
        crate::triggers::github(
            config,
            header(&request, "x-github-delivery"),
            header(&request, "x-github-event"),
            header(&request, "x-hub-signature-256"),
            &body,
        ),
    )
}

#[allow(clippy::future_not_send)]
async fn generic_hook(
    service: web::Data<Service>,
    request: HttpRequest,
    body: web::Bytes,
) -> HttpResponse {
    let Some(config) = &service.triggers else {
        return HttpResponse::NotFound().finish();
    };
    submit_hook(
        &service,
        crate::triggers::generic(
            config,
            header(&request, "x-gpipe-delivery"),
            header(&request, "x-gpipe-signature-256"),
            &body,
        ),
    )
}

#[cfg(test)]
mod tests {
    use super::equal_token;
    #[test]
    fn tokens_require_exact_equality() {
        assert!(equal_token("abc", "abc"));
        assert!(!equal_token("abc", "abd"));
        assert!(!equal_token("abc", "ab"));
    }
}
