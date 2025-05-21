#![cfg_attr(feature = "fail-on-warnings", deny(warnings))]
#![warn(clippy::all, clippy::pedantic, clippy::nursery, clippy::cargo)]
#![allow(clippy::multiple_crate_versions)]

use std::{collections::HashMap, sync::LazyLock};

use bytes::Bytes;
use http::Response;
use hyperchad_renderer::{Content, HtmlTagRenderer, PartialView, View};
use hyperchad_renderer_html::html::container_element_to_html;
use hyperchad_router::{RoutePath, RouteRequest, Router};

pub use http;

#[cfg(feature = "actions")]
mod actions;

#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error(transparent)]
    Http(#[from] http::Error),
    #[error(transparent)]
    IO(#[from] std::io::Error),
    #[error(transparent)]
    Renderer(Box<dyn std::error::Error + Send>),
    #[error(transparent)]
    Recv(#[from] flume::RecvError),
    #[error(transparent)]
    Navigate(#[from] hyperchad_router::NavigateError),
    #[cfg(feature = "_json")]
    #[error(transparent)]
    SerdeJson(#[from] serde_json::Error),
}

#[derive(Debug, Clone)]
pub struct HttpApp<R: HtmlTagRenderer + Sync> {
    pub renderer: R,
    pub router: Router,
    #[cfg(feature = "actions")]
    pub action_tx: Option<
        flume::Sender<(
            String,
            Option<hyperchad_renderer::transformer::actions::logic::Value>,
        )>,
    >,
    #[cfg(feature = "assets")]
    pub static_asset_routes: Vec<hyperchad_renderer::assets::StaticAssetRoute>,
}

impl<R: HtmlTagRenderer + Sync> HttpApp<R> {
    /// # Errors
    ///
    /// * If the request fails to process
    ///
    /// # Panics
    ///
    /// * Shouldn't
    pub async fn process(&self, req: &RouteRequest) -> Result<Response<Bytes>, Error> {
        static HEADERS: LazyLock<HashMap<String, String>> = LazyLock::new(HashMap::new);

        #[cfg(feature = "actions")]
        {
            let route = RoutePath::from("/$action");

            if route.matches(&req.path) {
                let Some(tx) = &self.action_tx else {
                    return Ok(Response::builder().status(204).body(Bytes::new())?);
                };

                return actions::handle_action(tx, req);
            }
        }

        #[cfg(feature = "assets")]
        {
            use std::{path::PathBuf, str::FromStr as _};

            use hyperchad_renderer::assets::{AssetPathTarget, StaticAssetRoute};
            use switchy_async::io::AsyncReadExt as _;

            for StaticAssetRoute { route, target } in &self.static_asset_routes {
                let route_path = match target {
                    AssetPathTarget::File(..) | AssetPathTarget::FileContents(..) => {
                        RoutePath::from(route)
                    }
                    AssetPathTarget::Directory(..) => RoutePath::LiteralPrefix(format!("{route}/")),
                };

                if !route_path.matches(&req.path) {
                    continue;
                }

                let is_directory = matches!(target, AssetPathTarget::Directory(..));

                match target {
                    AssetPathTarget::FileContents(target) => {
                        let extension = PathBuf::from_str(route.as_str())
                            .unwrap()
                            .extension()
                            .and_then(|x| x.to_str().map(str::to_lowercase));

                        let content_type = match extension.as_deref() {
                            Some("js" | "mjs" | "cjs") => "text/javascript;charset=UTF-8",
                            _ => "application/octet-stream",
                        };

                        let response = Response::builder()
                            .status(200)
                            .header("Content-Type", content_type)
                            .body(target.clone())?;

                        return Ok::<_, Error>(response);
                    }
                    AssetPathTarget::File(target) | AssetPathTarget::Directory(target) => {
                        let target = if is_directory {
                            target.join(req.path.clone())
                        } else {
                            target.clone()
                        };

                        let mut file = switchy_fs::unsync::OpenOptions::new()
                            .read(true)
                            .open(target)
                            .await?;

                        let mut buf = vec![];
                        file.read_to_end(&mut buf).await?;

                        let response = Response::builder()
                            .status(200)
                            .header("Content-Type", "text/html")
                            .body(buf.into())?;

                        return Ok::<_, Error>(response);
                    }
                }
            }
        }

        let content = self.router.navigate(req.clone()).await?;

        let html = match content {
            Content::View(View {
                immediate: view, ..
            }) => {
                let content = container_element_to_html(&view, &self.renderer)?;

                self.renderer
                    .root_html(&HEADERS, &view, content, None, None, None, None)
            }
            Content::PartialView(PartialView {
                container: view, ..
            }) => {
                let content = container_element_to_html(&view, &self.renderer)?;

                self.renderer
                    .partial_html(&HEADERS, &view, content, None, None)
            }
            #[cfg(feature = "json")]
            Content::Json(json) => {
                let mut bytes: Vec<u8> = Vec::new();
                serde_json::to_writer(&mut bytes, &json)?;
                return Ok(Response::builder()
                    .status(200)
                    .header("Content-Type", "application/json")
                    .body(bytes.into())?);
            }
        };

        Ok(Response::builder()
            .status(200)
            .header("Content-Type", "text/html")
            .body(html.into_bytes().into())?)
    }
}

impl<R: HtmlTagRenderer + Sync> HttpApp<R> {
    pub const fn new(renderer: R, router: Router) -> Self {
        Self {
            renderer,
            router,
            #[cfg(feature = "actions")]
            action_tx: None,
            #[cfg(feature = "assets")]
            static_asset_routes: vec![],
        }
    }

    #[cfg(feature = "actions")]
    #[must_use]
    pub fn with_action_tx(
        mut self,
        tx: flume::Sender<(
            String,
            Option<hyperchad_renderer::transformer::actions::logic::Value>,
        )>,
    ) -> Self {
        self.action_tx = Some(tx);
        self
    }

    #[cfg(feature = "actions")]
    pub fn set_action_tx(
        &mut self,
        tx: flume::Sender<(
            String,
            Option<hyperchad_renderer::transformer::actions::logic::Value>,
        )>,
    ) {
        self.action_tx = Some(tx);
    }
}
