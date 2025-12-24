//! Generic web server integration for HTML rendering.
//!
//! This module provides integration with generic web servers, enabling HTML
//! rendering in custom server implementations. It converts `HyperChad` routers to
//! web server-compatible handlers that process HTTP requests and generate HTML responses.
//!
//! # Features
//!
//! * Generic HTTP request processing
//! * HTML response generation for custom web servers
//! * Support for static assets and CSS
//! * Framework-agnostic design for maximum flexibility

use std::{
    collections::BTreeMap,
    sync::{Arc, LazyLock},
};

use async_trait::async_trait;
use bytes::Bytes;
use flume::Receiver;
use hyperchad_renderer::{Color, Content, HtmlTagRenderer, RendererEvent};
use hyperchad_router::{ClientInfo, ClientOs, RequestInfo, RouteRequest, Router};
use hyperchad_transformer::ResponsiveTrigger;
use switchy::http::models::Method;
use switchy_http_models::StatusCode;

use crate::{HtmlApp, HtmlRenderer, html::container_element_to_html};

pub use hyperchad_renderer_html_web_server::*;

/// Converts a hyperchad router to a web server-compatible HTML renderer.
///
/// Creates an HTML renderer configured for generic web servers with the provided
/// tag renderer and router configuration.
#[must_use]
pub fn router_to_web_server<T: HtmlTagRenderer + Clone + Send + Sync + 'static>(
    tag_renderer: T,
    value: hyperchad_router::Router,
) -> HtmlRenderer<
    hyperchad_renderer_html_web_server::WebServerApp<
        PreparedRequest,
        HtmlWebServerResponseProcessor<T>,
    >,
> {
    let (publisher, event_rx) = crate::extend::HtmlRendererEventPub::new();

    HtmlRenderer::new(hyperchad_renderer_html_web_server::WebServerApp::new(
        HtmlWebServerResponseProcessor::new(tag_renderer, value),
        event_rx,
    ))
    .with_html_renderer_event_pub(publisher)
}

/// Web server response processor for HTML rendering.
///
/// Processes generic web server requests and generates HTML responses using the
/// configured router and tag renderer.
#[derive(Clone)]
pub struct HtmlWebServerResponseProcessor<T: HtmlTagRenderer + Clone> {
    /// The hyperchad router for handling navigation.
    pub router: Router,
    /// The HTML tag renderer.
    pub tag_renderer: T,
    /// Background color for the page.
    pub background: Option<Color>,
    /// Page title.
    pub title: Option<String>,
    /// Page description.
    pub description: Option<String>,
    /// Viewport meta tag content.
    pub viewport: Option<String>,
    /// CSS URLs from CDN.
    pub css_urls: Vec<String>,
    /// CSS paths for static assets.
    pub css_paths: Vec<String>,
    /// Inline CSS content.
    pub inline_css: Vec<String>,
}

impl<T: HtmlTagRenderer + Clone> HtmlWebServerResponseProcessor<T> {
    /// Creates a new web server response processor.
    #[must_use]
    pub const fn new(tag_renderer: T, router: Router) -> Self {
        Self {
            router,
            tag_renderer,
            background: None,
            title: None,
            description: None,
            viewport: None,
            css_urls: vec![],
            css_paths: vec![],
            inline_css: vec![],
        }
    }
}

impl<T: HtmlTagRenderer + Clone + Send + Sync> HtmlApp
    for WebServerApp<PreparedRequest, HtmlWebServerResponseProcessor<T>>
{
    fn tag_renderer(&self) -> &dyn HtmlTagRenderer {
        &self.processor.tag_renderer
    }

    fn with_responsive_trigger(mut self, name: String, trigger: ResponsiveTrigger) -> Self {
        self.processor
            .tag_renderer
            .add_responsive_trigger(name, trigger);
        self
    }

    fn add_responsive_trigger(&mut self, name: String, trigger: ResponsiveTrigger) {
        self.processor
            .tag_renderer
            .add_responsive_trigger(name, trigger);
    }

    fn with_viewport(mut self, viewport: Option<String>) -> Self {
        self.processor.viewport = viewport;
        self
    }

    fn set_viewport(&mut self, viewport: Option<String>) {
        self.processor.viewport = viewport;
    }

    fn with_title(mut self, title: Option<String>) -> Self {
        self.processor.title = title;
        self
    }

    fn set_title(&mut self, title: Option<String>) {
        self.processor.title = title;
    }

    fn with_description(mut self, description: Option<String>) -> Self {
        self.processor.description = description;
        self
    }

    fn set_description(&mut self, description: Option<String>) {
        self.processor.description = description;
    }

    fn with_background(mut self, background: Option<Color>) -> Self {
        self.processor.background = background;
        self
    }

    fn set_background(&mut self, background: Option<Color>) {
        self.processor.background = background;
    }

    fn with_html_renderer_event_rx(mut self, rx: Receiver<RendererEvent>) -> Self {
        self.renderer_event_rx = rx;
        self
    }

    fn set_html_renderer_event_rx(&mut self, rx: Receiver<RendererEvent>) {
        self.renderer_event_rx = rx;
    }

    #[cfg(feature = "assets")]
    fn with_static_asset_routes(
        mut self,
        paths: impl Into<Vec<hyperchad_renderer::assets::StaticAssetRoute>>,
    ) -> Self {
        self.static_asset_routes = paths.into();
        self
    }

    #[cfg(feature = "assets")]
    fn static_asset_routes(
        &self,
    ) -> impl Iterator<Item = &hyperchad_renderer::assets::StaticAssetRoute> {
        self.static_asset_routes.iter()
    }

    #[cfg(feature = "assets")]
    fn with_asset_not_found_behavior(
        mut self,
        behavior: hyperchad_renderer::assets::AssetNotFoundBehavior,
    ) -> Self {
        self.asset_not_found_behavior = behavior;
        self
    }

    #[cfg(feature = "assets")]
    fn set_asset_not_found_behavior(
        &mut self,
        behavior: hyperchad_renderer::assets::AssetNotFoundBehavior,
    ) {
        self.asset_not_found_behavior = behavior;
    }

    fn with_css_url(mut self, url: impl Into<String>) -> Self {
        self.processor.css_urls.push(url.into());
        self
    }

    fn add_css_url(&mut self, url: impl Into<String>) {
        self.processor.css_urls.push(url.into());
    }

    fn with_css_path(mut self, path: impl Into<String>) -> Self {
        self.processor.css_paths.push(path.into());
        self
    }

    fn add_css_path(&mut self, path: impl Into<String>) {
        self.processor.css_paths.push(path.into());
    }

    fn with_inline_css(mut self, css: impl Into<String>) -> Self {
        self.processor.inline_css.push(css.into());
        self
    }

    fn add_inline_css(&mut self, css: impl Into<String>) {
        self.processor.inline_css.push(css.into());
    }

    fn css_urls(&self) -> &[String] {
        &self.processor.css_urls
    }

    fn css_paths(&self) -> &[String] {
        &self.processor.css_paths
    }

    fn inline_css_blocks(&self) -> &[String] {
        &self.processor.inline_css
    }
}

/// Prepared request for web server processing.
///
/// Contains the parsed route request and flags indicating the request type.
#[derive(Clone)]
pub struct PreparedRequest {
    full: bool,
    req: RouteRequest,
}

#[async_trait]
impl<T: HtmlTagRenderer + Clone + Send + Sync> WebServerResponseProcessor<PreparedRequest>
    for HtmlWebServerResponseProcessor<T>
{
    fn prepare_request(
        &self,
        req: HttpRequest,
        body: Option<Arc<Bytes>>,
    ) -> Result<PreparedRequest, hyperchad_renderer_html_web_server::WebServerError> {
        // For now, create a basic implementation
        // In a full implementation, this would parse the request properly
        let path = req.path().to_string();
        let query_string = req.query_string();

        let query = qstring::QString::from(query_string).into_iter().collect();

        // Basic header parsing - in a real implementation this would be more complete
        let headers = BTreeMap::new();
        let cookies = BTreeMap::new();

        let os_name = "unknown".to_string(); // Would parse from User-Agent in real implementation

        Ok(PreparedRequest {
            full: path != "/$sse" && !headers.contains_key("hx-request"),
            req: RouteRequest {
                path,
                method: Method::Get, // Would parse from actual request method
                query,
                headers,
                cookies,
                body,
                info: RequestInfo {
                    client: Arc::new(ClientInfo {
                        os: ClientOs { name: os_name },
                    }),
                },
            },
        })
    }

    async fn to_response(
        &self,
        req: PreparedRequest,
    ) -> Result<HttpResponse, hyperchad_renderer_html_web_server::WebServerError> {
        let content = self.router.navigate(req.req.clone()).await.map_err(|e| {
            hyperchad_renderer_html_web_server::WebServerError::Http {
                status_code: StatusCode::InternalServerError,
                source: Box::new(e),
            }
        })?;

        match content {
            Some(content) => {
                let has_fragments = matches!(&content, hyperchad_renderer::Content::View(v) if !v.fragments.is_empty());
                let delete_selectors = if let hyperchad_renderer::Content::View(v) = &content {
                    if v.delete_selectors.is_empty() {
                        None
                    } else {
                        serde_json::to_string(
                            &v.delete_selectors
                                .iter()
                                .map(ToString::to_string)
                                .collect::<Vec<_>>(),
                        )
                        .ok()
                    }
                } else {
                    None
                };

                match content {
                    hyperchad_renderer::Content::View(view) => {
                        let (body, _content_type) = self
                            .to_body(hyperchad_renderer::Content::View(view), req)
                            .await?;
                        let mut response =
                            HttpResponse::new(StatusCode::Ok).with_body(body.to_vec());

                        if has_fragments {
                            response
                                .headers
                                .insert("X-HyperChad-Fragments".to_string(), "true".to_string());
                        }

                        if let Some(selectors) = delete_selectors {
                            response
                                .headers
                                .insert("X-HyperChad-Delete-Selectors".to_string(), selectors);
                        }

                        Ok(response)
                    }
                    hyperchad_renderer::Content::Raw {
                        data,
                        content_type: _,
                    } => Ok(HttpResponse::new(StatusCode::Ok).with_body(data.to_vec())),
                    #[cfg(feature = "json")]
                    hyperchad_renderer::Content::Json(json) => {
                        let (body, _content_type) = self
                            .to_body(hyperchad_renderer::Content::Json(json), req)
                            .await?;
                        Ok(HttpResponse::new(StatusCode::Ok).with_body(body.to_vec()))
                    }
                }
            }
            None => Ok(HttpResponse::new(StatusCode::NoContent)),
        }
    }

    async fn to_body(
        &self,
        content: Content,
        req: PreparedRequest,
    ) -> Result<(Bytes, String), hyperchad_renderer_html_web_server::WebServerError> {
        static HEADERS: LazyLock<BTreeMap<String, String>> = LazyLock::new(BTreeMap::new);

        Ok(match content {
            hyperchad_renderer::Content::View(view) => {
                let mut parts = Vec::new();

                // Render primary content
                if let Some(primary) = &view.primary {
                    let html =
                        container_element_to_html(primary, &self.tag_renderer).map_err(|e| {
                            hyperchad_renderer_html_web_server::WebServerError::Http {
                                status_code: StatusCode::InternalServerError,
                                source: Box::new(e),
                            }
                        })?;

                    let html = if req.full {
                        self.tag_renderer.root_html(
                            &HEADERS,
                            primary,
                            html,
                            self.viewport.as_deref(),
                            self.background,
                            self.title.as_deref(),
                            self.description.as_deref(),
                            &self.css_urls,
                            &self.css_paths,
                            &self.inline_css,
                        )
                    } else {
                        self.tag_renderer.partial_html(
                            &HEADERS,
                            primary,
                            html,
                            self.viewport.as_deref(),
                            self.background,
                        )
                    };

                    parts.push(html);
                }

                // Render fragments
                for fragment in &view.fragments {
                    parts.push(format!(
                        "\n<!--hyperchad-fragment-->\n{}\n",
                        fragment.selector
                    ));

                    let html = container_element_to_html(&fragment.container, &self.tag_renderer)
                        .map_err(|e| {
                        hyperchad_renderer_html_web_server::WebServerError::Http {
                            status_code: StatusCode::InternalServerError,
                            source: Box::new(e),
                        }
                    })?;

                    let html = self.tag_renderer.partial_html(
                        &HEADERS,
                        &fragment.container,
                        html,
                        self.viewport.as_deref(),
                        self.background,
                    );

                    parts.push(html);
                    parts.push("\n".to_string());
                }

                let body = parts.join("").as_bytes().to_vec().into();
                let content_type = "text/html; charset=utf-8".to_string();

                (body, content_type)
            }
            #[cfg(feature = "json")]
            hyperchad_renderer::Content::Json(x) => {
                let content = serde_json::to_string(&x)
                    .map_err(
                        |e| hyperchad_renderer_html_web_server::WebServerError::Http {
                            status_code: StatusCode::InternalServerError,
                            source: Box::new(e),
                        },
                    )?
                    .as_bytes()
                    .to_vec()
                    .into();

                let content_type = "application/json".to_string();

                (content, content_type)
            }
            hyperchad_renderer::Content::Raw { data, content_type } => (data, content_type),
        })
    }
}
