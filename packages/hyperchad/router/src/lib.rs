#![cfg_attr(feature = "fail-on-warnings", deny(warnings))]
#![warn(clippy::all, clippy::pedantic, clippy::nursery, clippy::cargo)]
#![allow(clippy::multiple_crate_versions)]

use std::{
    collections::BTreeMap,
    pin::Pin,
    sync::{Arc, RwLock},
};

use bytes::Bytes;
use flume::{Receiver, Sender};
use futures::Future;
use hyperchad_renderer::Content;
pub use hyperchad_transformer::{Container, Element};
use qstring::QString;
use switchy::http::models::Method;
use thiserror::Error;
use tokio::task::JoinHandle;

pub static DEFAULT_CLIENT_INFO: std::sync::LazyLock<std::sync::Arc<ClientInfo>> =
    std::sync::LazyLock::new(|| {
        let os_name = os_info::get().os_type().to_string();
        std::sync::Arc::new(ClientInfo {
            os: ClientOs { name: os_name },
        })
    });

pub type RouteFunc = Arc<
    Box<
        dyn (Fn(
                RouteRequest,
            ) -> Pin<
                Box<
                    dyn Future<Output = Result<Option<Content>, Box<dyn std::error::Error>>> + Send,
                >,
            >) + Send
            + Sync,
    >,
>;

#[cfg(feature = "serde")]
#[derive(Debug, Error)]
pub enum ParseError {
    #[error(transparent)]
    SerdeJson(#[from] serde_json::Error),
    #[error(transparent)]
    SerdeUrlEncoded(#[from] serde_urlencoded::de::Error),
    #[error("Missing body")]
    MissingBody,
    #[error("Invalid Content-Type")]
    InvalidContentType,
    #[cfg(feature = "form")]
    #[error(transparent)]
    IO(#[from] std::io::Error),
    #[cfg(feature = "form")]
    #[error("Missing boundary")]
    MissingBoundary,
    #[cfg(feature = "form")]
    #[error(transparent)]
    ParseUtf8(#[from] std::string::FromUtf8Error),
    #[cfg(feature = "form")]
    #[error(transparent)]
    Multipart(#[from] mime_multipart::Error),
    #[cfg(feature = "form")]
    #[error("Invalid Content‑Disposition")]
    InvalidContentDisposition,
}

#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct ClientOs {
    pub name: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ClientInfo {
    pub os: ClientOs,
}

impl Default for ClientInfo {
    fn default() -> Self {
        DEFAULT_CLIENT_INFO.as_ref().clone()
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct RequestInfo {
    pub client: Arc<ClientInfo>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RouteRequest {
    pub path: String,
    pub method: Method,
    pub query: BTreeMap<String, String>,
    pub headers: BTreeMap<String, String>,
    pub cookies: BTreeMap<String, String>,
    pub info: RequestInfo,
    pub body: Option<Arc<Bytes>>,
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
            method: Method::Get,
            query: QString::from(query).into_iter().collect(),
            headers: BTreeMap::new(),
            cookies: BTreeMap::new(),
            info,
            body: None,
        }
    }

    #[must_use]
    pub fn content_type(&self) -> Option<&str> {
        self.headers.get("content-type").map(String::as_str)
    }

    /// # Errors
    ///
    /// * If the `Content-Type` header is missing
    /// * If the form is missing
    #[cfg(feature = "form")]
    pub fn parse_form<T: serde::de::DeserializeOwned>(&self) -> Result<T, ParseError> {
        use std::io::{Cursor, Read as _};

        use base64::engine::{Engine as _, general_purpose};
        use hyper_old::header::{ContentDisposition, ContentType, DispositionParam, Headers};
        use mime_multipart::{Node, read_multipart_body};
        use mime_old::Mime;
        use serde_json::{Map, Value};

        fn parse_multipart_json_sync(body: &[u8], content_type: &str) -> Result<Value, ParseError> {
            fn process_nodes(
                nodes: Vec<Node>,
                obj: &mut Map<String, Value>,
            ) -> Result<(), ParseError> {
                for node in nodes {
                    match node {
                        Node::Part(part) => {
                            let cd = part
                                .headers
                                .get::<ContentDisposition>()
                                .ok_or(ParseError::InvalidContentDisposition)?;
                            let field_name = cd
                                .parameters
                                .iter()
                                .find_map(|param| {
                                    if let DispositionParam::Ext(key, val) = param {
                                        if key.eq_ignore_ascii_case("name") {
                                            return Some(val.clone());
                                        }
                                    }
                                    None
                                })
                                .ok_or(ParseError::InvalidContentDisposition)?;

                            let text = String::from_utf8(part.body)?;
                            obj.insert(field_name, Value::String(text));
                        }

                        Node::File(filepart) => {
                            let cd = filepart
                                .headers
                                .get::<ContentDisposition>()
                                .ok_or(ParseError::InvalidContentDisposition)?;
                            let field_name = cd
                                .parameters
                                .iter()
                                .find_map(|param| {
                                    if let DispositionParam::Ext(key, val) = param {
                                        if key.eq_ignore_ascii_case("name") {
                                            return Some(val.clone());
                                        }
                                    }
                                    None
                                })
                                .ok_or(ParseError::InvalidContentDisposition)?;

                            let mut f = std::fs::File::open(&filepart.path)?;
                            let mut data = Vec::new();
                            f.read_to_end(&mut data)?;

                            // base64‑encode
                            let b64 = general_purpose::STANDARD.encode(&data);
                            obj.insert(field_name, Value::String(b64));
                        }

                        Node::Multipart((_hdrs, subparts)) => {
                            process_nodes(subparts, obj)?;
                        }
                    }
                }
                Ok(())
            }

            let mut headers = Headers::new();
            let mime_type: Mime = content_type
                .parse()
                .map_err(|()| ParseError::InvalidContentType)?;
            headers.set(ContentType(mime_type));

            let mut cursor = Cursor::new(body);
            let parts: Vec<Node> = read_multipart_body(&mut cursor, &headers, false)?;

            let mut obj = Map::new();
            process_nodes(parts, &mut obj)?;

            Ok(Value::Object(obj))
        }

        if let Some(form) = &self.body {
            let value = parse_multipart_json_sync(
                form,
                self.content_type().ok_or(ParseError::InvalidContentType)?,
            )?;
            Ok(serde_json::from_value(value)?)
        } else {
            Err(ParseError::MissingBody)
        }
    }

    /// # Errors
    ///
    /// * If the `Content-Type` is not `application/json` or `application/x-www-form-urlencoded`
    /// * If the body is missing
    #[cfg(feature = "serde")]
    pub fn parse_body<T: serde::de::DeserializeOwned>(&self) -> Result<T, ParseError> {
        if let Some(body) = &self.body {
            Ok(serde_json::from_slice(body)?)
        } else {
            Err(ParseError::MissingBody)
        }
    }
}

impl From<Navigation> for RouteRequest {
    fn from(value: Navigation) -> Self {
        Self {
            path: value.0,
            method: Method::Get,
            query: BTreeMap::new(),
            headers: BTreeMap::new(),
            cookies: BTreeMap::new(),
            info: RequestInfo { client: value.1 },
            body: None,
        }
    }
}

impl From<&Navigation> for RouteRequest {
    fn from(value: &Navigation) -> Self {
        value.clone().into()
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum RoutePath {
    Literal(String),
    Literals(Vec<String>),
    LiteralPrefix(String),
}

impl RoutePath {
    #[must_use]
    pub fn matches(&self, path: &str) -> bool {
        match self {
            Self::Literal(route_path) => route_path == path,
            Self::Literals(route_paths) => route_paths.iter().any(|x| x == path),
            Self::LiteralPrefix(route_path) => path.starts_with(route_path),
        }
    }

    #[must_use]
    pub fn strip_match<'a>(&'a self, path: &'a str) -> Option<&'a str> {
        const EMPTY: &str = "";

        match self {
            Self::Literal(..) | Self::Literals(..) => {
                if self.matches(path) {
                    Some(EMPTY)
                } else {
                    None
                }
            }
            Self::LiteralPrefix(route_path) => path.strip_prefix(route_path),
        }
    }
}

impl From<&str> for RoutePath {
    fn from(value: &str) -> Self {
        Self::Literal(value.to_owned())
    }
}

impl From<&String> for RoutePath {
    fn from(value: &String) -> Self {
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
    #[error("Handler error: {0:?}")]
    Handler(Box<dyn std::error::Error + Send + Sync>),
    #[error("Sender error")]
    Sender,
}

#[derive(Clone)]
pub struct Router {
    #[cfg(feature = "static-routes")]
    pub static_routes: Arc<RwLock<Vec<(RoutePath, RouteFunc)>>>,
    pub routes: Arc<RwLock<Vec<(RoutePath, RouteFunc)>>>,
    sender: Sender<Content>,
    pub receiver: Receiver<Content>,
}

impl std::fmt::Debug for Router {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Router")
            .field("sender", &self.sender)
            .field("receiver", &self.receiver)
            .finish_non_exhaustive()
    }
}

impl Default for Router {
    fn default() -> Self {
        Self::new()
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Navigation(String, Arc<ClientInfo>);

impl From<RouteRequest> for Navigation {
    fn from(value: RouteRequest) -> Self {
        let mut query = String::new();

        for (key, value) in &value.query {
            if query.is_empty() {
                query.push('?');
            } else {
                query.push('&');
            }
            query.push_str(key);
            query.push('=');
            query.push_str(value);
        }

        Self(format!("{}{query}", value.path), value.info.client)
    }
}

impl From<&str> for RouteRequest {
    fn from(value: &str) -> Self {
        value.to_string().into()
    }
}

impl From<String> for RouteRequest {
    fn from(value: String) -> Self {
        Self {
            path: value,
            method: Method::Get,
            query: BTreeMap::new(),
            headers: BTreeMap::new(),
            cookies: BTreeMap::new(),
            info: RequestInfo::default(),
            body: None,
        }
    }
}

impl From<&String> for RouteRequest {
    fn from(value: &String) -> Self {
        value.to_string().into()
    }
}

impl From<(&str, ClientInfo)> for RouteRequest {
    fn from(value: (&str, ClientInfo)) -> Self {
        (value.0.to_string(), Arc::new(value.1)).into()
    }
}

impl From<(String, ClientInfo)> for RouteRequest {
    fn from(value: (String, ClientInfo)) -> Self {
        (value.0, Arc::new(value.1)).into()
    }
}

impl From<(&String, ClientInfo)> for RouteRequest {
    fn from(value: (&String, ClientInfo)) -> Self {
        (value.0.to_string(), Arc::new(value.1)).into()
    }
}

impl From<(&str, Arc<ClientInfo>)> for RouteRequest {
    fn from(value: (&str, Arc<ClientInfo>)) -> Self {
        (value.0.to_string(), value.1).into()
    }
}

impl From<(String, Arc<ClientInfo>)> for RouteRequest {
    fn from(value: (String, Arc<ClientInfo>)) -> Self {
        (value.0, RequestInfo { client: value.1 }).into()
    }
}

impl From<(&String, Arc<ClientInfo>)> for RouteRequest {
    fn from(value: (&String, Arc<ClientInfo>)) -> Self {
        (value.0.to_string(), value.1).into()
    }
}

impl From<(&str, RequestInfo)> for RouteRequest {
    fn from(value: (&str, RequestInfo)) -> Self {
        (value.0.to_string(), value.1).into()
    }
}

impl From<(String, RequestInfo)> for RouteRequest {
    fn from(value: (String, RequestInfo)) -> Self {
        let (path, query) = if let Some((path, query)) = value.0.split_once('?') {
            (path.to_string(), query)
        } else {
            (value.0, "")
        };

        Self {
            path,
            method: Method::Get,
            query: QString::from(query).into_iter().collect(),
            headers: BTreeMap::new(),
            cookies: BTreeMap::new(),
            info: value.1,
            body: None,
        }
    }
}

impl From<(&String, RequestInfo)> for RouteRequest {
    fn from(value: (&String, RequestInfo)) -> Self {
        (value.0.to_string(), value.1).into()
    }
}

impl From<&RouteRequest> for Navigation {
    fn from(value: &RouteRequest) -> Self {
        value.clone().into()
    }
}

impl From<&str> for Navigation {
    fn from(value: &str) -> Self {
        Self(value.to_string(), DEFAULT_CLIENT_INFO.clone())
    }
}

impl From<String> for Navigation {
    fn from(value: String) -> Self {
        Self(value, DEFAULT_CLIENT_INFO.clone())
    }
}

impl From<&String> for Navigation {
    fn from(value: &String) -> Self {
        Self(value.clone(), DEFAULT_CLIENT_INFO.clone())
    }
}

impl From<(&str, ClientInfo)> for Navigation {
    fn from(value: (&str, ClientInfo)) -> Self {
        Self(value.0.to_string(), Arc::new(value.1))
    }
}

impl From<(String, ClientInfo)> for Navigation {
    fn from(value: (String, ClientInfo)) -> Self {
        Self(value.0, Arc::new(value.1))
    }
}

impl From<(&String, ClientInfo)> for Navigation {
    fn from(value: (&String, ClientInfo)) -> Self {
        Self(value.0.to_string(), Arc::new(value.1))
    }
}

impl From<(&str, Arc<ClientInfo>)> for Navigation {
    fn from(value: (&str, Arc<ClientInfo>)) -> Self {
        Self(value.0.to_string(), value.1)
    }
}

impl From<(String, Arc<ClientInfo>)> for Navigation {
    fn from(value: (String, Arc<ClientInfo>)) -> Self {
        Self(value.0, value.1)
    }
}

impl From<(&String, Arc<ClientInfo>)> for Navigation {
    fn from(value: (&String, Arc<ClientInfo>)) -> Self {
        Self(value.0.to_string(), value.1)
    }
}

impl From<(&str, RequestInfo)> for Navigation {
    fn from(value: (&str, RequestInfo)) -> Self {
        Self(value.0.to_string(), value.1.client)
    }
}

impl From<(String, RequestInfo)> for Navigation {
    fn from(value: (String, RequestInfo)) -> Self {
        Self(value.0, value.1.client)
    }
}

impl From<(&String, RequestInfo)> for Navigation {
    fn from(value: (&String, RequestInfo)) -> Self {
        Self(value.0.to_string(), value.1.client)
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
        C: TryInto<Content>,
        Response: Into<Option<C>>,
        F: Future<Output = Result<Response, BoxE>> + Send + 'static,
        BoxE: Into<Box<dyn std::error::Error>>,
    >(
        self,
        route: impl Into<RoutePath>,
        handler: impl Fn(RouteRequest) -> F + Send + Sync + Clone + 'static,
    ) -> Self
    where
        C::Error: Into<Box<dyn std::error::Error>>,
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
    #[must_use]
    pub fn with_no_content_result<
        F: Future<Output = Result<(), BoxE>> + Send + 'static,
        BoxE: Into<Box<dyn std::error::Error>>,
    >(
        self,
        route: impl Into<RoutePath>,
        handler: impl Fn(RouteRequest) -> F + Send + Sync + Clone + 'static,
    ) -> Self {
        self.with_route_result::<Content, Option<Content>, _, _>(route, move |req: RouteRequest| {
            let fut = handler(req);
            async move { fut.await.map(|()| None::<Content>).map_err(Into::into) }
        })
    }

    /// # Panics
    ///
    /// Will panic if routes `RwLock` is poisoned.
    #[allow(clippy::needless_pass_by_value)]
    #[must_use]
    pub fn with_static_route_result<
        C: TryInto<Content>,
        Response: Into<Option<C>>,
        F: Future<Output = Result<Response, BoxE>> + Send + 'static,
        BoxE: Into<Box<dyn std::error::Error>>,
    >(
        self,
        #[allow(unused_variables)] route: impl Into<RoutePath>,
        #[allow(unused_variables)] handler: impl Fn(RouteRequest) -> F + Send + Sync + Clone + 'static,
    ) -> Self
    where
        C::Error: Into<Box<dyn std::error::Error>>,
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
    pub fn with_route<
        C: TryInto<Content>,
        Response: Into<Option<C>>,
        F: Future<Output = Response> + Send + 'static,
    >(
        self,
        route: impl Into<RoutePath>,
        handler: impl Fn(RouteRequest) -> F + Send + Sync + Clone + 'static,
    ) -> Self
    where
        C::Error: std::error::Error + 'static,
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
        C: TryInto<Content>,
        Response: Into<Option<C>>,
        F: Future<Output = Response> + Send + 'static,
    >(
        self,
        #[allow(unused_variables)] route: impl Into<RoutePath>,
        #[allow(unused_variables)] handler: impl Fn(RouteRequest) -> F + Send + Sync + Clone + 'static,
    ) -> Self
    where
        C::Error: std::error::Error + 'static,
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
    pub async fn navigate(
        &self,
        navigation: impl Into<RouteRequest>,
    ) -> Result<Option<Content>, NavigateError> {
        let req = navigation.into();

        log::debug!("navigate: method={} path={}", req.method, req.path);

        let handler = self.get_route_func(&req.path);

        Ok(if let Some(handler) = handler {
            match handler(req).await {
                Ok(view) => view,
                Err(e) => {
                    log::error!("Failed to fetch route view: {e:?}");
                    return Err(NavigateError::Handler(Box::new(std::io::Error::other(
                        e.to_string(),
                    ))));
                }
            }
        } else {
            log::warn!("Invalid navigation path={}", req.path);
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
    pub async fn navigate_send(
        &self,
        navigation: impl Into<RouteRequest>,
    ) -> Result<(), NavigateError> {
        let req = navigation.into();

        log::debug!("navigate_send: method={} path={}", req.method, req.path);

        let view = {
            let handler = self.get_route_func(&req.path);

            if let Some(handler) = handler {
                match handler(req).await {
                    Ok(view) => view,
                    Err(e) => {
                        log::error!("Failed to fetch route view: {e:?}");
                        return Err(NavigateError::Handler(Box::new(std::io::Error::other(
                            e.to_string(),
                        ))));
                    }
                }
            } else {
                log::warn!("Invalid navigation path={}", req.path);
                return Err(NavigateError::InvalidPath);
            }
        };

        if let Some(view) = view {
            self.sender.send(view).map_err(|e| {
                log::error!("Failed to send: {e:?}");
                NavigateError::Sender
            })?;
        }

        Ok(())
    }

    /// # Errors
    ///
    /// Will error if there was an error navigating
    #[must_use]
    pub fn navigate_spawn(
        &self,
        navigation: impl Into<RouteRequest>,
    ) -> JoinHandle<Result<(), Box<dyn std::error::Error + Send>>> {
        let navigation = navigation.into();

        log::debug!("navigate_spawn: navigation={navigation:?}");

        self.navigate_spawn_on(&tokio::runtime::Handle::current(), navigation)
    }

    /// # Errors
    ///
    /// Will error if there was an error navigating
    #[must_use]
    pub fn navigate_spawn_on(
        &self,
        handle: &tokio::runtime::Handle,
        navigation: impl Into<RouteRequest>,
    ) -> JoinHandle<Result<(), Box<dyn std::error::Error + Send>>> {
        let navigation = navigation.into();

        log::debug!("navigate_spawn_on: navigation={navigation:?}");

        let router = self.clone();
        moosicbox_task::spawn_on("NativeApp navigate_spawn", handle, async move {
            router
                .navigate_send(navigation)
                .await
                .map_err(|e| Box::new(e) as Box<dyn std::error::Error + Send>)
        })
    }

    #[must_use]
    pub async fn wait_for_navigation(&self) -> Option<Content> {
        self.receiver.recv_async().await.ok()
    }
}

fn gen_route_func<
    C: TryInto<Content>,
    Response: Into<Option<C>>,
    F: Future<Output = Response> + Send + 'static,
>(
    handler: impl Fn(RouteRequest) -> F + Send + Sync + Clone + 'static,
) -> RouteFunc
where
    C::Error: std::error::Error + 'static,
{
    Arc::new(Box::new(move |req| {
        Box::pin({
            let handler = handler.clone();
            async move {
                let resp: Result<Option<Content>, Box<dyn std::error::Error>> = handler(req)
                    .await
                    .into()
                    .map(TryInto::try_into)
                    .transpose()
                    .map_err(|e| {
                        log::error!("Failed to handle route: {e:?}");
                        Box::new(e) as Box<dyn std::error::Error>
                    });
                resp
            }
        })
    }))
}

fn gen_route_func_result<
    C: TryInto<Content>,
    Response: Into<Option<C>>,
    F: Future<Output = Result<Response, BoxE>> + Send + 'static,
    BoxE: Into<Box<dyn std::error::Error>>,
>(
    handler: impl Fn(RouteRequest) -> F + Send + Sync + Clone + 'static,
) -> RouteFunc
where
    C::Error: Into<Box<dyn std::error::Error>>,
{
    Arc::new(Box::new(move |req| {
        Box::pin({
            let handler = handler.clone();
            async move {
                let resp: Result<Response, Box<dyn std::error::Error>> =
                    handler(req).await.map_err(Into::into);
                match resp.map(|x| {
                    let x: Result<Option<Content>, Box<dyn std::error::Error>> = x
                        .into()
                        .map(TryInto::try_into)
                        .transpose()
                        .map_err(Into::into);
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
