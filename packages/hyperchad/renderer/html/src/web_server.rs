use std::{
    collections::BTreeMap,
    sync::{Arc, LazyLock},
};

use async_trait::async_trait;
use bytes::Bytes;
use flume::Receiver;
use hyperchad_renderer::{Color, Content, HtmlTagRenderer, PartialView, RendererEvent, View};
use hyperchad_router::{ClientInfo, ClientOs, RequestInfo, RouteRequest, Router};
use hyperchad_transformer::ResponsiveTrigger;
use switchy::http::models::Method;
use switchy_http_models::StatusCode;

use crate::{HtmlApp, HtmlRenderer, html::container_element_to_html};

pub use hyperchad_renderer_html_web_server::*;

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

#[derive(Clone)]
pub struct HtmlWebServerResponseProcessor<T: HtmlTagRenderer + Clone> {
    pub router: Router,
    pub tag_renderer: T,
    pub background: Option<Color>,
    pub title: Option<String>,
    pub description: Option<String>,
    pub viewport: Option<String>,
}

impl<T: HtmlTagRenderer + Clone> HtmlWebServerResponseProcessor<T> {
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
}

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
            Some(content) => match &content {
                hyperchad_renderer::Content::View(..) => {
                    // FIXME: This needs to attach the content type to the response headers
                    let (body, _content_type) = self.to_body(content, req).await?;
                    Ok(HttpResponse::new(StatusCode::Ok).with_body(body.to_vec()))
                }
                hyperchad_renderer::Content::PartialView(PartialView { target, .. }) => {
                    let _target = format!("#{target}");
                    // FIXME: This needs to attach the content type to the response headers
                    let (body, _content_type) = self.to_body(content, req).await?;
                    Ok(HttpResponse::new(StatusCode::Ok).with_body(body.to_vec()))
                }
                hyperchad_renderer::Content::Raw {
                    data,
                    content_type: _,
                } => Ok(HttpResponse::new(StatusCode::Ok).with_body(data.to_vec())),
                #[cfg(feature = "json")]
                hyperchad_renderer::Content::Json(..) => {
                    let (body, _content_type) = self.to_body(content, req).await?;
                    Ok(HttpResponse::new(StatusCode::Ok).with_body(body.to_vec()))
                }
            },
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
            hyperchad_renderer::Content::View(View {
                immediate: view, ..
            })
            | hyperchad_renderer::Content::PartialView(PartialView {
                container: view, ..
            }) => {
                let content =
                    container_element_to_html(&view, &self.tag_renderer).map_err(|e| {
                        hyperchad_renderer_html_web_server::WebServerError::Http {
                            status_code: StatusCode::InternalServerError,
                            source: Box::new(e),
                        }
                    })?;

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
