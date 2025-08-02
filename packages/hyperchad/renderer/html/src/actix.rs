use std::{
    collections::HashMap,
    str::FromStr as _,
    sync::{Arc, LazyLock},
};

use actix_web::HttpResponse;
use async_trait::async_trait;
use bytes::Bytes;
use flume::Receiver;
use hyperchad_renderer::{Color, Content, HtmlTagRenderer, PartialView, RendererEvent, View};
use hyperchad_renderer_html_actix::actix_web::{
    error::ErrorInternalServerError, http::header::USER_AGENT,
};
use hyperchad_router::{ClientInfo, ClientOs, RequestInfo, RouteRequest, Router};
use hyperchad_transformer::ResponsiveTrigger;
use switchy::http::models::Method;
use uaparser::{Parser as _, UserAgentParser};

use crate::{HtmlApp, HtmlRenderer, html::container_element_to_html};

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
}

#[derive(Clone)]
pub struct PreparedRequest {
    full: bool,
    req: RouteRequest,
}

#[async_trait]
impl<T: HtmlTagRenderer + Clone + Send + Sync>
    hyperchad_renderer_html_actix::ActixResponseProcessor<PreparedRequest>
    for HtmlActixResponseProcessor<T>
{
    fn prepare_request(
        &self,
        req: actix_web::HttpRequest,
        body: Option<Arc<Bytes>>,
    ) -> Result<PreparedRequest, actix_web::Error> {
        static UA_PARSER: LazyLock<UserAgentParser> = LazyLock::new(|| {
            UserAgentParser::from_bytes(include_bytes!("../ua-regexes.yaml"))
                .expect("Parser creation failed")
        });

        let query = qstring::QString::from(req.query_string())
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
            .cookies()
            .inspect_err(|e| {
                log::error!("Failed to get cookies: {e:?}");
            })
            .map(|x| {
                x.iter()
                    .map(|cookie| (cookie.name().to_string(), cookie.value().to_string()))
                    .collect()
            })
            .unwrap_or_default();

        let path = req.path().to_string();

        let os_name =
            if let Some(Ok(user_agent)) = req.headers().get(USER_AGENT).map(|x| x.to_str()) {
                let os = UA_PARSER.parse_os(user_agent);

                os.family.to_string()
            } else {
                "unknown".to_string()
            };

        Ok(PreparedRequest {
            full: req.path() != "/$sse" && req.headers().get("hx-request").is_none(),
            req: RouteRequest {
                path,
                method: Method::from_str(req.method().as_str())
                    .map_err(ErrorInternalServerError)?,
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

    async fn to_response(&self, req: PreparedRequest) -> Result<HttpResponse, actix_web::Error> {
        let content = self
            .router
            .navigate(req.req.clone())
            .await
            .map_err(ErrorInternalServerError)?;

        match content {
            Some(content) => match &content {
                hyperchad_renderer::Content::View(..) => {
                    let (body, content_type) = self.to_body(content, req).await?;
                    Ok(HttpResponse::Ok()
                        .content_type(content_type.as_str())
                        .body(body))
                }
                hyperchad_renderer::Content::PartialView(PartialView { target, .. }) => {
                    let target = format!("#{target}");
                    let (body, content_type) = self.to_body(content, req).await?;
                    Ok(HttpResponse::Ok()
                        .append_header(("v-fragment", target))
                        .content_type(content_type.as_str())
                        .body(body))
                }
                hyperchad_renderer::Content::Raw { data, content_type } => Ok(HttpResponse::Ok()
                    .content_type(content_type.as_str())
                    .body(data.to_vec())),
                #[cfg(feature = "json")]
                hyperchad_renderer::Content::Json(..) => {
                    let (body, content_type) = self.to_body(content, req).await?;
                    Ok(HttpResponse::Ok()
                        .content_type(content_type.as_str())
                        .body(body))
                }
            },
            None => Ok(HttpResponse::NoContent().finish()),
        }
    }

    async fn to_body(
        &self,
        content: Content,
        req: PreparedRequest,
    ) -> Result<(Bytes, String), actix_web::Error> {
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

                let content = if req.full {
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
                .as_bytes()
                .to_vec()
                .into();

                let content_type = "text/html; charset=utf-8".to_string();

                (content, content_type)
            }
            #[cfg(feature = "json")]
            hyperchad_renderer::Content::Json(x) => {
                let content = serde_json::to_string(&x)
                    .map_err(ErrorInternalServerError)?
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
