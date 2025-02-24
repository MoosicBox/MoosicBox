use std::{
    collections::HashMap,
    sync::{Arc, LazyLock},
};

use actix_web::{http::header::ContentType, HttpResponse};
use async_trait::async_trait;
use flume::Receiver;
use hyperchad_renderer::{Color, Content, HtmlTagRenderer, PartialView, RendererEvent, View};
use hyperchad_renderer_html_actix::actix_web::{
    error::ErrorInternalServerError, http::header::USER_AGENT,
};
use hyperchad_router::{ClientInfo, ClientOs, RequestInfo, Router};
use hyperchad_transformer::ResponsiveTrigger;
use uaparser::{Parser as _, UserAgentParser};

use crate::{html::container_element_to_html, HtmlApp, HtmlRenderer};

pub use hyperchad_renderer_html_actix::*;

#[must_use]
pub fn router_to_actix<T: HtmlTagRenderer + Clone + Send + Sync + 'static>(
    tag_renderer: T,
    value: hyperchad_router::Router,
) -> HtmlRenderer<
    hyperchad_renderer_html_actix::ActixApp<PreparedRequest, HtmlActixResponseProcessor<T>>,
> {
    let (publisher, event_rx) = crate::extend::HtmlRendererEventPub::new();

    HtmlRenderer::new(hyperchad_renderer_html_actix::ActixApp::new(
        HtmlActixResponseProcessor::new(tag_renderer, value),
        event_rx,
    ))
    .with_html_renderer_event_pub(publisher)
}

#[derive(Clone)]
pub struct HtmlActixResponseProcessor<T: HtmlTagRenderer + Clone> {
    pub router: Router,
    pub tag_renderer: T,
    pub background: Option<Color>,
    pub title: Option<String>,
    pub description: Option<String>,
    pub viewport: Option<String>,
}

impl<T: HtmlTagRenderer + Clone> HtmlActixResponseProcessor<T> {
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
    for ActixApp<PreparedRequest, HtmlActixResponseProcessor<T>>
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

    #[must_use]
    fn with_html_renderer_event_rx(mut self, rx: Receiver<RendererEvent>) -> Self {
        self.renderer_event_rx = rx;
        self
    }

    fn set_html_renderer_event_rx(&mut self, rx: Receiver<RendererEvent>) {
        self.renderer_event_rx = rx;
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
    hyperchad_renderer_html_actix::ActixResponseProcessor<PreparedRequest>
    for HtmlActixResponseProcessor<T>
{
    fn prepare_request(
        &self,
        req: actix_web::HttpRequest,
    ) -> Result<PreparedRequest, actix_web::Error> {
        static UA_PARSER: LazyLock<UserAgentParser> = LazyLock::new(|| {
            UserAgentParser::from_bytes(include_bytes!("../ua-regexes.yaml"))
                .expect("Parser creation failed")
        });

        let query_string = req.query_string();
        let query_string = if query_string.is_empty() {
            String::new()
        } else {
            format!("?{query_string}")
        };

        let path = format!("{}{}", req.path(), query_string);

        let os_name =
            if let Some(Ok(user_agent)) = req.headers().get(USER_AGENT).map(|x| x.to_str()) {
                let os = UA_PARSER.parse_os(user_agent);

                os.family.to_string()
            } else {
                "unknown".to_string()
            };

        Ok(PreparedRequest {
            full: req.path() != "/$sse" && req.headers().get("hx-request").is_none(),
            path,
            info: RequestInfo {
                client: Arc::new(ClientInfo {
                    os: ClientOs { name: os_name },
                }),
            },
        })
    }

    async fn to_response(&self, req: PreparedRequest) -> Result<HttpResponse, actix_web::Error> {
        let content = self
            .router
            .navigate(&req.path, req.info.clone())
            .await
            .map_err(ErrorInternalServerError)?;

        match &content {
            hyperchad_renderer::Content::View(..)
            | hyperchad_renderer::Content::PartialView(..) => {
                let body = self.to_body(content, req).await?;
                Ok(HttpResponse::Ok()
                    .content_type(ContentType::html())
                    .body(body))
            }
            #[cfg(feature = "json")]
            hyperchad_renderer::Content::Json(..) => {
                let body = self.to_body(content, req).await?;
                Ok(HttpResponse::Ok()
                    .content_type(ContentType::json())
                    .body(body))
            }
        }
    }

    async fn to_body(
        &self,
        content: Content,
        req: PreparedRequest,
    ) -> Result<String, actix_web::Error> {
        static HEADERS: LazyLock<HashMap<String, String>> = LazyLock::new(HashMap::new);

        Ok(match content {
            hyperchad_renderer::Content::View(View {
                immediate: view, ..
            })
            | hyperchad_renderer::Content::PartialView(PartialView {
                container: view, ..
            }) => {
                let content = container_element_to_html(&view, &self.tag_renderer)
                    .map_err(ErrorInternalServerError)?;

                if req.full {
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
                }
            }
            #[cfg(feature = "json")]
            hyperchad_renderer::Content::Json(x) => {
                serde_json::to_string(&x).map_err(ErrorInternalServerError)?
            }
        })
    }
}
