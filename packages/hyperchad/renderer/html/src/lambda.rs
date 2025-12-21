//! AWS Lambda integration for HTML rendering.
//!
//! This module provides integration with AWS Lambda, enabling HTML rendering
//! within serverless Lambda functions. It converts `HyperChad` routers to
//! Lambda-compatible handlers that process Lambda HTTP events and generate
//! HTML responses.
//!
//! # Features
//!
//! * Lambda HTTP request processing with user agent detection
//! * HTML response generation for Lambda functions
//! * Support for static assets and CSS
//! * Optimized for serverless execution

use std::{
    collections::BTreeMap,
    str::FromStr as _,
    sync::{Arc, LazyLock},
};

use async_trait::async_trait;
use bytes::Bytes;
use hyperchad_renderer::{Color, HtmlTagRenderer};
use hyperchad_router::{ClientInfo, ClientOs, RequestInfo, RouteRequest, Router};
use hyperchad_transformer::ResponsiveTrigger;
use lambda_http::{Request, RequestExt as _, http::header::USER_AGENT};
use switchy::http::models::Method;
use uaparser::{Parser as _, UserAgentParser};

use crate::{HtmlApp, HtmlRenderer, html::container_element_to_html};

pub use hyperchad_renderer_html_lambda::*;

/// Converts a hyperchad router to an AWS Lambda-compatible HTML renderer.
///
/// Creates an HTML renderer configured for AWS Lambda with the provided tag renderer
/// and router configuration.
#[must_use]
pub fn router_to_lambda<T: HtmlTagRenderer + Clone + Send + Sync + 'static>(
    tag_renderer: T,
    value: hyperchad_router::Router,
) -> HtmlRenderer<
    hyperchad_renderer_html_lambda::LambdaApp<PreparedRequest, HtmlLambdaResponseProcessor<T>>,
> {
    HtmlRenderer::new(hyperchad_renderer_html_lambda::LambdaApp::new(
        HtmlLambdaResponseProcessor::new(tag_renderer, value),
    ))
}

/// Lambda response processor for HTML rendering.
///
/// Processes AWS Lambda requests and generates HTML responses using the
/// configured router and tag renderer.
#[derive(Clone)]
pub struct HtmlLambdaResponseProcessor<T: HtmlTagRenderer + Clone + Send + Sync> {
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

impl<T: HtmlTagRenderer + Clone + Send + Sync> HtmlLambdaResponseProcessor<T> {
    /// Creates a new Lambda response processor.
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
    for LambdaApp<PreparedRequest, HtmlLambdaResponseProcessor<T>>
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

    #[cfg(feature = "extend")]
    fn with_html_renderer_event_rx(
        self,
        _rx: flume::Receiver<hyperchad_renderer::RendererEvent>,
    ) -> Self {
        self
    }

    #[cfg(feature = "extend")]
    fn set_html_renderer_event_rx(
        &mut self,
        _rx: flume::Receiver<hyperchad_renderer::RendererEvent>,
    ) {
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
        self,
        _behavior: hyperchad_renderer::assets::AssetNotFoundBehavior,
    ) -> Self {
        // Lambda doesn't currently use this behavior - assets are typically
        // served via CloudFront/S3 rather than the Lambda function
        self
    }

    #[cfg(feature = "assets")]
    fn set_asset_not_found_behavior(
        &mut self,
        _behavior: hyperchad_renderer::assets::AssetNotFoundBehavior,
    ) {
        // Lambda doesn't currently use this behavior
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

/// Prepared request for Lambda processing.
///
/// Contains the parsed route request and flags indicating the request type.
#[derive(Clone)]
pub struct PreparedRequest {
    full: bool,
    req: RouteRequest,
}

#[async_trait]
impl<T: HtmlTagRenderer + Clone + Send + Sync>
    hyperchad_renderer_html_lambda::LambdaResponseProcessor<PreparedRequest>
    for HtmlLambdaResponseProcessor<T>
{
    fn prepare_request(
        &self,
        req: Request,
        body: Option<Arc<Bytes>>,
    ) -> Result<PreparedRequest, lambda_runtime::Error> {
        static UA_PARSER: LazyLock<UserAgentParser> = LazyLock::new(|| {
            UserAgentParser::from_bytes(include_bytes!("../ua-regexes.yaml"))
                .expect("Parser creation failed")
        });

        let query =
            qstring::QString::from(req.query_string_parameters().to_query_string().as_str())
                .into_iter()
                .collect();

        let headers = req
            .headers()
            .iter()
            .map(|(name, value)| {
                (
                    name.to_string(),
                    value.to_str().unwrap_or_default().to_string(),
                )
            })
            .collect();

        let cookies = req
            .headers()
            .get("Cookie")
            .map(|x| parse_cookies(x.to_str().unwrap_or_default()))
            .unwrap_or_default()
            .into_iter()
            .collect();

        let path = req.raw_http_path().to_string();

        let os_name =
            if let Some(Ok(user_agent)) = req.headers().get(USER_AGENT).map(|x| x.to_str()) {
                let os = UA_PARSER.parse_os(user_agent);

                os.family.to_string()
            } else {
                "unknown".to_string()
            };

        Ok(PreparedRequest {
            full: req.headers().get("hx-request").is_none(),
            req: RouteRequest {
                path,
                method: Method::from_str(req.method().as_str()).map_err(Box::new)?,
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
    ) -> Result<Option<(Content, Option<Vec<(String, String)>>)>, lambda_runtime::Error> {
        let content = self
            .router
            .navigate(req.req.clone())
            .await
            .map_err(|e| Box::new(e) as lambda_runtime::Error)?;

        if let Some(content) = content {
            let headers = self.headers(&content);
            let body = self.to_body(content, req).await?;
            Ok(Some((body, headers)))
        } else {
            Ok(None)
        }
    }

    fn headers(&self, content: &hyperchad_renderer::Content) -> Option<Vec<(String, String)>> {
        match content {
            hyperchad_renderer::Content::View(view) => {
                let mut headers = Vec::new();

                if !view.fragments.is_empty() {
                    headers.push(("X-HyperChad-Fragments".to_string(), "true".to_string()));
                }

                if !view.delete_selectors.is_empty()
                    && let Ok(selectors_json) = serde_json::to_string(
                        &view
                            .delete_selectors
                            .iter()
                            .map(ToString::to_string)
                            .collect::<Vec<_>>(),
                    )
                {
                    headers.push(("X-HyperChad-Delete-Selectors".to_string(), selectors_json));
                }

                if headers.is_empty() {
                    None
                } else {
                    Some(headers)
                }
            }
            hyperchad_renderer::Content::Raw { .. } => None,
            #[cfg(feature = "json")]
            hyperchad_renderer::Content::Json(..) => None,
        }
    }

    async fn to_body(
        &self,
        content: hyperchad_renderer::Content,
        req: PreparedRequest,
    ) -> Result<Content, lambda_runtime::Error> {
        static HEADERS: LazyLock<BTreeMap<String, String>> = LazyLock::new(BTreeMap::new);

        Ok(match content {
            hyperchad_renderer::Content::View(view) => {
                let mut parts = Vec::new();

                // Render primary content
                if let Some(primary) = &view.primary {
                    let html = container_element_to_html(primary, &self.tag_renderer)
                        .map_err(|e| Box::new(e) as lambda_runtime::Error)?;

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
                        .map_err(|e| Box::new(e) as lambda_runtime::Error)?;

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

                Content::Html(parts.join(""))
            }
            hyperchad_renderer::Content::Raw { data, content_type } => {
                Content::Raw { data, content_type }
            }
            #[cfg(feature = "json")]
            hyperchad_renderer::Content::Json(value) => Content::Json(value),
        })
    }
}

/// Parses HTTP Cookie header into key-value pairs.
///
/// Splits the cookie header string by semicolons and extracts name-value pairs.
fn parse_cookies(header: &str) -> Vec<(String, String)> {
    header
        .split(';')
        .filter_map(|part| {
            let mut parts = part.trim().splitn(2, '=');
            let key = parts.next()?.trim();
            let value = parts.next()?.trim();
            Some((key.to_string(), value.to_string()))
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test_log::test]
    fn test_parse_cookies_single() {
        let cookies = parse_cookies("session=abc123");
        assert_eq!(cookies.len(), 1);
        assert_eq!(cookies[0], ("session".to_string(), "abc123".to_string()));
    }

    #[test_log::test]
    fn test_parse_cookies_multiple() {
        let cookies = parse_cookies("session=abc123; user=john; theme=dark");
        assert_eq!(cookies.len(), 3);
        assert_eq!(cookies[0], ("session".to_string(), "abc123".to_string()));
        assert_eq!(cookies[1], ("user".to_string(), "john".to_string()));
        assert_eq!(cookies[2], ("theme".to_string(), "dark".to_string()));
    }

    #[test_log::test]
    fn test_parse_cookies_with_whitespace() {
        let cookies = parse_cookies("  session = abc123  ;  user = john  ");
        assert_eq!(cookies.len(), 2);
        assert_eq!(cookies[0], ("session".to_string(), "abc123".to_string()));
        assert_eq!(cookies[1], ("user".to_string(), "john".to_string()));
    }

    #[test_log::test]
    fn test_parse_cookies_empty_string() {
        let cookies = parse_cookies("");
        assert!(cookies.is_empty());
    }

    #[test_log::test]
    fn test_parse_cookies_no_equals() {
        // Cookies without = sign should be filtered out
        let cookies = parse_cookies("invalid_cookie; session=abc123");
        assert_eq!(cookies.len(), 1);
        assert_eq!(cookies[0], ("session".to_string(), "abc123".to_string()));
    }

    #[test_log::test]
    fn test_parse_cookies_value_with_equals() {
        // Cookie values can contain equals signs (e.g., base64 encoded)
        let cookies = parse_cookies("token=abc=123=xyz");
        assert_eq!(cookies.len(), 1);
        assert_eq!(cookies[0], ("token".to_string(), "abc=123=xyz".to_string()));
    }

    #[test_log::test]
    fn test_parse_cookies_empty_value() {
        let cookies = parse_cookies("empty=");
        assert_eq!(cookies.len(), 1);
        assert_eq!(cookies[0], ("empty".to_string(), String::new()));
    }

    #[test_log::test]
    fn test_parse_cookies_semicolon_only() {
        let cookies = parse_cookies(";;;");
        assert!(cookies.is_empty());
    }
}
