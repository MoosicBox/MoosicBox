#![cfg_attr(feature = "fail-on-warnings", deny(warnings))]
#![warn(clippy::all, clippy::pedantic, clippy::nursery, clippy::cargo)]

use std::{
    collections::HashMap,
    pin::Pin,
    sync::{Arc, RwLock},
};

use flume::{Receiver, Sender};
use futures::Future;
use gigachad_renderer::View;
pub use gigachad_transformer::{Container, Element};
use qstring::QString;
use thiserror::Error;
use tokio::task::JoinHandle;

pub type RouteFunc = Arc<
    Box<
        dyn (Fn(
                RouteRequest,
            )
                -> Pin<Box<dyn Future<Output = Result<View, Box<dyn std::error::Error>>> + Send>>)
            + Send
            + Sync,
    >,
>;

#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct ClientOs {
    pub name: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct ClientInfo {
    pub os: ClientOs,
}

#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct RequestInfo {
    pub client: Arc<ClientInfo>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RouteRequest {
    pub path: String,
    pub query: HashMap<String, String>,
    pub info: RequestInfo,
}

impl RouteRequest {
    #[must_use]
    pub fn from_path(path: &str, info: RequestInfo) -> Self {
        let (path, query) = if let Some((path, query)) = path.split_once('?') {
            (path, query)
        } else {
            (path, "")
        };

        Self {
            path: path.to_owned(),
            query: QString::from(query).into_iter().collect(),
            info,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum RoutePath {
    Literal(String),
    Literals(Vec<String>),
}

impl RoutePath {
    #[must_use]
    pub fn matches(&self, path: &str) -> bool {
        match self {
            Self::Literal(route_path) => route_path == path,
            Self::Literals(route_paths) => route_paths.iter().any(|x| x == path),
        }
    }
}

impl From<&str> for RoutePath {
    fn from(value: &str) -> Self {
        Self::Literal(value.to_owned())
    }
}

impl From<&[&str; 1]> for RoutePath {
    fn from(value: &[&str; 1]) -> Self {
        Self::Literals(value.iter().map(ToString::to_string).collect())
    }
}

impl From<&[&str; 2]> for RoutePath {
    fn from(value: &[&str; 2]) -> Self {
        Self::Literals(value.iter().map(ToString::to_string).collect())
    }
}

impl From<&[&str; 3]> for RoutePath {
    fn from(value: &[&str; 3]) -> Self {
        Self::Literals(value.iter().map(ToString::to_string).collect())
    }
}

impl From<&[&str; 4]> for RoutePath {
    fn from(value: &[&str; 4]) -> Self {
        Self::Literals(value.iter().map(ToString::to_string).collect())
    }
}

impl From<&[&str; 5]> for RoutePath {
    fn from(value: &[&str; 5]) -> Self {
        Self::Literals(value.iter().map(ToString::to_string).collect())
    }
}

impl From<&[&str; 6]> for RoutePath {
    fn from(value: &[&str; 6]) -> Self {
        Self::Literals(value.iter().map(ToString::to_string).collect())
    }
}

impl From<&[&str; 7]> for RoutePath {
    fn from(value: &[&str; 7]) -> Self {
        Self::Literals(value.iter().map(ToString::to_string).collect())
    }
}

impl From<&[&str; 8]> for RoutePath {
    fn from(value: &[&str; 8]) -> Self {
        Self::Literals(value.iter().map(ToString::to_string).collect())
    }
}

impl From<&[&str; 9]> for RoutePath {
    fn from(value: &[&str; 9]) -> Self {
        Self::Literals(value.iter().map(ToString::to_string).collect())
    }
}

impl From<&[&str; 10]> for RoutePath {
    fn from(value: &[&str; 10]) -> Self {
        Self::Literals(value.iter().map(ToString::to_string).collect())
    }
}

impl From<&[&str]> for RoutePath {
    fn from(value: &[&str]) -> Self {
        Self::Literals(value.iter().map(ToString::to_string).collect())
    }
}

impl From<Vec<&str>> for RoutePath {
    fn from(value: Vec<&str>) -> Self {
        Self::Literals(value.into_iter().map(ToString::to_string).collect())
    }
}

impl From<String> for RoutePath {
    fn from(value: String) -> Self {
        Self::Literal(value)
    }
}

impl From<&[String]> for RoutePath {
    fn from(value: &[String]) -> Self {
        Self::Literals(value.iter().map(ToString::to_string).collect())
    }
}

impl From<&[&String]> for RoutePath {
    fn from(value: &[&String]) -> Self {
        Self::Literals(value.iter().map(ToString::to_string).collect())
    }
}

impl From<Vec<String>> for RoutePath {
    fn from(value: Vec<String>) -> Self {
        Self::Literals(value)
    }
}

#[derive(Debug, Error)]
pub enum NavigateError {
    #[error("Invalid path")]
    InvalidPath,
    #[error("Handler error")]
    Handler,
    #[error("Sender error")]
    Sender,
}

#[derive(Clone)]
pub struct Router {
    #[cfg(feature = "static-routes")]
    pub static_routes: Arc<RwLock<Vec<(RoutePath, RouteFunc)>>>,
    pub routes: Arc<RwLock<Vec<(RoutePath, RouteFunc)>>>,
    sender: Sender<View>,
    pub receiver: Receiver<View>,
}

impl Default for Router {
    fn default() -> Self {
        Self::new()
    }
}

impl Router {
    #[must_use]
    pub fn new() -> Self {
        let (tx, rx) = flume::unbounded();

        Self {
            #[cfg(feature = "static-routes")]
            static_routes: Arc::new(RwLock::new(vec![])),
            routes: Arc::new(RwLock::new(vec![])),
            sender: tx,
            receiver: rx,
        }
    }

    /// # Panics
    ///
    /// Will panic if routes `RwLock` is poisoned.
    #[must_use]
    pub fn with_route_result<
        Response: TryInto<View>,
        F: Future<Output = Result<Response, BoxE>> + Send + 'static,
        BoxE: Into<Box<dyn std::error::Error>>,
    >(
        self,
        route: impl Into<RoutePath>,
        handler: impl Fn(RouteRequest) -> F + Send + Sync + Clone + 'static,
    ) -> Self
    where
        Response::Error: Into<Box<dyn std::error::Error>>,
    {
        self.routes
            .write()
            .unwrap()
            .push((route.into(), gen_route_func_result(handler)));
        self
    }

    /// # Panics
    ///
    /// Will panic if routes `RwLock` is poisoned.
    #[allow(clippy::needless_pass_by_value)]
    #[must_use]
    pub fn with_static_route_result<
        Response: TryInto<View>,
        F: Future<Output = Result<Response, BoxE>> + Send + 'static,
        BoxE: Into<Box<dyn std::error::Error>>,
    >(
        self,
        #[allow(unused_variables)] route: impl Into<RoutePath>,
        #[allow(unused_variables)] handler: impl Fn(RouteRequest) -> F + Send + Sync + Clone + 'static,
    ) -> Self
    where
        Response::Error: Into<Box<dyn std::error::Error>>,
    {
        #[cfg(feature = "static-routes")]
        self.static_routes
            .write()
            .unwrap()
            .push((route.into(), gen_route_func_result(handler)));
        self
    }

    /// # Panics
    ///
    /// Will panic if routes `RwLock` is poisoned.
    #[must_use]
    pub fn with_route<Response: TryInto<View>, F: Future<Output = Response> + Send + 'static>(
        self,
        route: impl Into<RoutePath>,
        handler: impl Fn(RouteRequest) -> F + Send + Sync + Clone + 'static,
    ) -> Self
    where
        Response::Error: std::error::Error + 'static,
    {
        self.routes
            .write()
            .unwrap()
            .push((route.into(), gen_route_func(handler)));
        self
    }

    /// # Panics
    ///
    /// Will panic if routes `RwLock` is poisoned.
    #[allow(clippy::needless_pass_by_value)]
    #[must_use]
    pub fn with_static_route<
        Response: TryInto<View>,
        F: Future<Output = Response> + Send + 'static,
    >(
        self,
        #[allow(unused_variables)] route: impl Into<RoutePath>,
        #[allow(unused_variables)] handler: impl Fn(RouteRequest) -> F + Send + Sync + Clone + 'static,
    ) -> Self
    where
        Response::Error: std::error::Error + 'static,
    {
        #[cfg(feature = "static-routes")]
        self.static_routes
            .write()
            .unwrap()
            .push((route.into(), gen_route_func(handler)));
        self
    }

    fn get_route_func(&self, path: &str) -> Option<RouteFunc> {
        let dyn_route = self
            .routes
            .read()
            .unwrap()
            .iter()
            .find(|(route, _)| route.matches(path))
            .cloned()
            .map(|(_, handler)| handler);

        #[cfg(feature = "static-routes")]
        if dyn_route.is_none() {
            return self
                .static_routes
                .read()
                .unwrap()
                .iter()
                .find(|(route, _)| route.matches(path))
                .cloned()
                .map(|(_, handler)| handler);
        }

        dyn_route
    }

    /// # Errors
    ///
    /// Will error if `Renderer` implementation fails to render the navigation result.
    ///
    /// # Panics
    ///
    /// Will panic if routes `RwLock` is poisoned.
    pub async fn navigate(&self, path: &str, info: RequestInfo) -> Result<View, NavigateError> {
        log::debug!("navigate: path={path}");

        let req = RouteRequest::from_path(path, info);
        let handler = self.get_route_func(&req.path);

        Ok(if let Some(handler) = handler {
            match handler(req).await {
                Ok(view) => view,
                Err(e) => {
                    log::error!("Failed to fetch route view: {e:?}");
                    return Err(NavigateError::Handler);
                }
            }
        } else {
            log::warn!("Invalid navigation path={path:?}");
            return Err(NavigateError::InvalidPath);
        })
    }

    /// # Errors
    ///
    /// Will error if `Renderer` implementation fails to render the navigation result.
    ///
    /// # Panics
    ///
    /// Will panic if routes `RwLock` is poisoned.
    pub async fn navigate_send(&self, path: &str, info: RequestInfo) -> Result<(), NavigateError> {
        log::debug!("navigate_send: path={path}");

        let view = {
            let req = RouteRequest::from_path(path, info);
            let handler = self.get_route_func(&req.path);

            if let Some(handler) = handler {
                match handler(req).await {
                    Ok(view) => view,
                    Err(e) => {
                        log::error!("Failed to fetch route view: {e:?}");
                        return Err(NavigateError::Handler);
                    }
                }
            } else {
                log::warn!("Invalid navigation path={path:?}");
                return Err(NavigateError::InvalidPath);
            }
        };

        self.sender.send(view).map_err(|e| {
            log::error!("Failed to send: {e:?}");
            NavigateError::Sender
        })?;

        Ok(())
    }

    /// # Errors
    ///
    /// Will error if there was an error navigating
    #[must_use]
    pub fn navigate_spawn(
        &self,
        path: &str,
        info: RequestInfo,
    ) -> JoinHandle<Result<(), Box<dyn std::error::Error + Send>>> {
        log::debug!("navigate_spawn: path={path}");

        self.navigate_spawn_on(&tokio::runtime::Handle::current(), path, info)
    }

    /// # Errors
    ///
    /// Will error if there was an error navigating
    #[must_use]
    pub fn navigate_spawn_on(
        &self,
        handle: &tokio::runtime::Handle,
        path: &str,
        info: RequestInfo,
    ) -> JoinHandle<Result<(), Box<dyn std::error::Error + Send>>> {
        log::debug!("navigate_spawn_on: path={path}");

        let path = path.to_owned();
        let router = self.clone();
        moosicbox_task::spawn_on("NativeApp navigate_spawn", handle, async move {
            router
                .navigate_send(&path, info)
                .await
                .map_err(|e| Box::new(e) as Box<dyn std::error::Error + Send>)
        })
    }

    #[must_use]
    pub async fn wait_for_navigation(&self) -> Option<View> {
        self.receiver.recv_async().await.ok()
    }
}

fn gen_route_func<Response: TryInto<View>, F: Future<Output = Response> + Send + 'static>(
    handler: impl Fn(RouteRequest) -> F + Send + Sync + Clone + 'static,
) -> RouteFunc
where
    Response::Error: std::error::Error + 'static,
{
    Arc::new(Box::new(move |req| {
        Box::pin({
            let handler = handler.clone();
            async move {
                let resp: Result<View, Box<dyn std::error::Error>> =
                    handler(req).await.try_into().map_err(|e| {
                        log::error!("Failed to handle route: {e:?}");
                        Box::new(e) as Box<dyn std::error::Error>
                    });
                resp
            }
        })
    }))
}

fn gen_route_func_result<
    Response: TryInto<View>,
    F: Future<Output = Result<Response, BoxE>> + Send + 'static,
    BoxE: Into<Box<dyn std::error::Error>>,
>(
    handler: impl Fn(RouteRequest) -> F + Send + Sync + Clone + 'static,
) -> RouteFunc
where
    Response::Error: Into<Box<dyn std::error::Error>>,
{
    Arc::new(Box::new(move |req| {
        Box::pin({
            let handler = handler.clone();
            async move {
                let resp: Result<Response, Box<dyn std::error::Error>> =
                    handler(req).await.map_err(Into::into);
                match resp.map(|x| {
                    let x: Result<View, Box<dyn std::error::Error>> =
                        x.try_into().map_err(Into::into);
                    x
                }) {
                    Ok(x) => match x {
                        Ok(x) => Ok(x),
                        Err(e) => Err(e),
                    },
                    Err(e) => Err(e),
                }
            }
        })
    }))
}
