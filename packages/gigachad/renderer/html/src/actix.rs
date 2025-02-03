use std::{
    collections::HashMap,
    sync::{Arc, LazyLock},
};

use async_trait::async_trait;
use gigachad_renderer::{Color, HtmlTagRenderer};
use gigachad_renderer_html_actix::actix_web::{
    error::ErrorInternalServerError, http::header::USER_AGENT,
};
use gigachad_router::{ClientInfo, ClientOs, RequestInfo, Router};
use uaparser::{Parser as _, UserAgentParser};

use crate::{
    html::{container_element_to_html, container_element_to_html_response},
    DefaultHtmlTagRenderer, HtmlApp, HtmlRenderer,
};

pub use gigachad_renderer_html_actix::*;

#[must_use]
pub fn router_to_actix(
    value: gigachad_router::Router,
) -> HtmlRenderer<gigachad_renderer_html_actix::ActixApp<PreparedRequest, HtmlActixResponseProcessor>>
{
    HtmlRenderer::new(gigachad_renderer_html_actix::ActixApp::new(
        HtmlActixResponseProcessor::new(value),
    ))
}

#[derive(Clone)]
pub struct HtmlActixResponseProcessor {
    pub router: Router,
    pub tag_renderer: Arc<Box<dyn HtmlTagRenderer + Send + Sync>>,
    pub background: Option<Color>,
}

impl HtmlActixResponseProcessor {
    #[must_use]
    pub fn new(router: Router) -> Self {
        Self {
            router,
            tag_renderer: Arc::new(Box::new(DefaultHtmlTagRenderer)),
            background: None,
        }
    }
}

impl HtmlApp for ActixApp<PreparedRequest, HtmlActixResponseProcessor> {
    #[must_use]
    fn with_tag_renderer(
        mut self,
        tag_renderer: impl HtmlTagRenderer + Send + Sync + 'static,
    ) -> Self {
        self.processor.tag_renderer = Arc::new(Box::new(tag_renderer));
        self
    }

    #[must_use]
    fn with_background(mut self, background: Option<Color>) -> Self {
        self.processor.background = background;
        self
    }

    fn set_background(&mut self, background: Option<Color>) {
        self.processor.background = background;
    }

    #[cfg(feature = "assets")]
    #[must_use]
    fn with_static_asset_routes(
        mut self,
        paths: impl Into<Vec<gigachad_renderer::assets::StaticAssetRoute>>,
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
impl gigachad_renderer_html_actix::ActixResponseProcessor<PreparedRequest>
    for HtmlActixResponseProcessor
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
            full: req.headers().get("hx-request").is_none(),
            path,
            info: RequestInfo {
                client: Arc::new(ClientInfo {
                    os: ClientOs { name: os_name },
                }),
            },
        })
    }

    async fn to_html(&self, req: PreparedRequest) -> Result<String, actix_web::Error> {
        static HEADERS: LazyLock<HashMap<String, String>> = LazyLock::new(HashMap::new);

        let view = self
            .router
            .navigate(&req.path, req.info)
            .await
            .map_err(ErrorInternalServerError)?;

        if req.full {
            container_element_to_html_response(
                &HEADERS,
                &view.immediate,
                self.background,
                &**self.tag_renderer,
            )
            .map_err(ErrorInternalServerError)
        } else {
            container_element_to_html(&view.immediate, &**self.tag_renderer)
                .map_err(ErrorInternalServerError)
        }
    }
}
