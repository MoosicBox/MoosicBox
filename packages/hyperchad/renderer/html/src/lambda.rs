use std::{
    collections::HashMap,
    sync::{Arc, LazyLock},
};

use async_trait::async_trait;
use hyperchad_renderer::{Color, HtmlTagRenderer, PartialView, View};
use hyperchad_router::{ClientInfo, ClientOs, RequestInfo, Router};
use hyperchad_transformer::ResponsiveTrigger;
use lambda_http::{Request, RequestExt as _, http::header::USER_AGENT};
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
    #[must_use]
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

    #[must_use]
    fn with_viewport(mut self, viewport: Option<String>) -> Self {
        self.processor.viewport = viewport;
        self
    }

    fn set_viewport(&mut self, viewport: Option<String>) {
        self.processor.viewport = viewport;
    }

    #[must_use]
    fn with_title(mut self, title: Option<String>) -> Self {
        self.processor.title = title;
        self
    }

    fn set_title(&mut self, title: Option<String>) {
        self.processor.title = title;
    }

    #[must_use]
    fn with_description(mut self, description: Option<String>) -> Self {
        self.processor.description = description;
        self
    }

    fn set_description(&mut self, description: Option<String>) {
        self.processor.description = description;
    }

    #[must_use]
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
    #[must_use]
    fn with_static_asset_routes(
        mut self,
        paths: impl Into<Vec<hyperchad_renderer::assets::StaticAssetRoute>>,
    ) -> Self {
        self.static_asset_routes = paths.into();
        self
    }
}

#[derive(Clone)]
pub struct PreparedRequest {
    full: bool,
    path: String,
    info: RequestInfo,
}

#[async_trait]
impl<T: HtmlTagRenderer + Clone + Send + Sync>
    hyperchad_renderer_html_lambda::LambdaResponseProcessor<PreparedRequest>
    for HtmlLambdaResponseProcessor<T>
{
    fn prepare_request(&self, req: Request) -> Result<PreparedRequest, lambda_runtime::Error> {
        static UA_PARSER: LazyLock<UserAgentParser> = LazyLock::new(|| {
            UserAgentParser::from_bytes(include_bytes!("../ua-regexes.yaml"))
                .expect("Parser creation failed")
        });

        let query_string = req.query_string_parameters().to_query_string();
        let query_string = if query_string.is_empty() {
            String::new()
        } else {
            format!("?{query_string}")
        };

        let path = format!("{}{}", req.raw_http_path(), query_string);

        let os_name =
            if let Some(Ok(user_agent)) = req.headers().get(USER_AGENT).map(|x| x.to_str()) {
                let os = UA_PARSER.parse_os(user_agent);

                os.family.to_string()
            } else {
                "unknown".to_string()
            };

        Ok(PreparedRequest {
            full: req.headers().get("hx-request").is_none(),
            path,
            info: RequestInfo {
                client: Arc::new(ClientInfo {
                    os: ClientOs { name: os_name },
                }),
            },
        })
    }

    async fn to_response(&self, req: PreparedRequest) -> Result<Content, lambda_runtime::Error> {
        let content = self
            .router
            .navigate(&req.path, req.info.clone())
            .await
            .map_err(|e| Box::new(e) as lambda_runtime::Error)?;

        self.to_body(content, req).await
    }

    async fn to_body(
        &self,
        content: hyperchad_renderer::Content,
        req: PreparedRequest,
    ) -> Result<Content, lambda_runtime::Error> {
        static HEADERS: LazyLock<HashMap<String, String>> = LazyLock::new(HashMap::new);

        Ok(match content {
            hyperchad_renderer::Content::View(View {
                immediate: view, ..
            })
            | hyperchad_renderer::Content::PartialView(PartialView {
                container: view, ..
            }) => {
                let content = container_element_to_html(&view, &self.tag_renderer)
                    .map_err(|e| Box::new(e) as lambda_runtime::Error)?;

                Content::Html(if req.full {
                    self.tag_renderer.root_html(
                        &HEADERS,
                        &view,
                        content,
                        self.viewport.as_deref(),
                        self.background,
                        self.title.as_deref(),
                        self.description.as_deref(),
                    )
                } else {
                    self.tag_renderer.partial_html(
                        &HEADERS,
                        &view,
                        content,
                        self.viewport.as_deref(),
                        self.background,
                    )
                })
            }
            #[cfg(feature = "json")]
            hyperchad_renderer::Content::Json(value) => Content::Json(value),
        })
    }
}
