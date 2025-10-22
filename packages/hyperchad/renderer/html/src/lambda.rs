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

#[derive(Clone)]
pub struct HtmlLambdaResponseProcessor<T: HtmlTagRenderer + Clone + Send + Sync> {
    pub router: Router,
    pub tag_renderer: T,
    pub background: Option<Color>,
    pub title: Option<String>,
    pub description: Option<String>,
    pub viewport: Option<String>,
}

impl<T: HtmlTagRenderer + Clone + Send + Sync> HtmlLambdaResponseProcessor<T> {
    #[must_use]
    pub const fn new(tag_renderer: T, router: Router) -> Self {
        Self {
            router,
            tag_renderer,
            background: None,
            title: None,
            description: None,
            viewport: None,
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
}

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
                if view.fragments.is_empty() {
                    None
                } else {
                    Some(vec![(
                        "X-HyperChad-Fragments".to_string(),
                        "true".to_string(),
                    )])
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
                if !view.fragments.is_empty() {
                    parts.push("\n<!--hyperchad-fragments-->\n".to_string());

                    for fragment in &view.fragments {
                        let html = container_element_to_html(fragment, &self.tag_renderer)
                            .map_err(|e| Box::new(e) as lambda_runtime::Error)?;

                        let html = self.tag_renderer.partial_html(
                            &HEADERS,
                            fragment,
                            html,
                            self.viewport.as_deref(),
                            self.background,
                        );

                        parts.push(html);
                        parts.push("\n".to_string());
                    }

                    parts.push("<!--hyperchad-fragments-end-->\n".to_string());
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
