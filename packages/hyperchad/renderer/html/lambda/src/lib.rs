#![cfg_attr(feature = "fail-on-warnings", deny(warnings))]
#![warn(clippy::all, clippy::pedantic, clippy::nursery, clippy::cargo)]
#![allow(clippy::multiple_crate_versions)]

use std::{io::Write, marker::PhantomData};

use async_trait::async_trait;
use flate2::{Compression, write::GzEncoder};
use hyperchad_renderer::{RenderRunner, ToRenderRunner};
use lambda_http::{
    Request, Response,
    http::header::{CONTENT_ENCODING, CONTENT_TYPE},
    service_fn,
};
use tokio::runtime::Handle;

pub use lambda_http;
pub use lambda_runtime;

pub enum Content {
    Html(String),
    #[cfg(feature = "json")]
    Json(serde_json::Value),
}

#[async_trait]
pub trait LambdaResponseProcessor<T: Send + Sync + Clone> {
    /// # Errors
    ///
    /// * If the request fails to prepare
    fn prepare_request(&self, req: Request) -> Result<T, lambda_runtime::Error>;

    async fn to_response(&self, data: T) -> Result<Content, lambda_runtime::Error>;

    async fn to_body(
        &self,
        content: hyperchad_renderer::Content,
        data: T,
    ) -> Result<Content, lambda_runtime::Error>;
}

#[derive(Clone)]
pub struct LambdaApp<T: Send + Sync + Clone, R: LambdaResponseProcessor<T> + Send + Sync + Clone> {
    pub processor: R,
    #[cfg(feature = "assets")]
    pub static_asset_routes: Vec<hyperchad_renderer::assets::StaticAssetRoute>,
    _phantom: PhantomData<T>,
}

impl<T: Send + Sync + Clone, R: LambdaResponseProcessor<T> + Send + Sync + Clone> LambdaApp<T, R> {
    pub const fn new(to_html: R) -> Self {
        Self {
            processor: to_html,
            #[cfg(feature = "assets")]
            static_asset_routes: vec![],
            _phantom: PhantomData,
        }
    }
}

impl<
    T: Send + Sync + Clone + 'static,
    R: LambdaResponseProcessor<T> + Send + Sync + Clone + 'static,
> ToRenderRunner for LambdaApp<T, R>
{
    fn to_runner(
        self,
        handle: Handle,
    ) -> Result<Box<dyn RenderRunner>, Box<dyn std::error::Error + Send>> {
        Ok(Box::new(LambdaAppRunner { app: self, handle }))
    }
}

pub struct LambdaAppRunner<
    T: Send + Sync + Clone,
    R: LambdaResponseProcessor<T> + Send + Sync + Clone,
> {
    pub app: LambdaApp<T, R>,
    pub handle: Handle,
}

impl<
    T: Send + Sync + Clone + 'static,
    R: LambdaResponseProcessor<T> + Send + Sync + Clone + 'static,
> RenderRunner for LambdaAppRunner<T, R>
{
    /// # Errors
    ///
    /// Will error if html fails to run the event loop.
    #[allow(clippy::too_many_lines)]
    fn run(&mut self) -> Result<(), Box<dyn std::error::Error + Send>> {
        log::debug!("run: starting");

        let app = self.app.clone();
        let func = service_fn(move |event| {
            let app = app.clone();
            async move {
                let data = app.processor.prepare_request(event)?;
                let content = app.processor.to_response(data).await?;

                let mut response = Response::builder()
                    .status(200)
                    .header(CONTENT_ENCODING, "gzip");

                let mut gz = GzEncoder::new(vec![], Compression::default());

                response = match content {
                    Content::Html(x) => {
                        gz.write_all(x.as_bytes())?;
                        response.header(CONTENT_TYPE, "text/html")
                    }
                    #[cfg(feature = "json")]
                    Content::Json(x) => {
                        gz.write_all(serde_json::to_string(&x)?.as_bytes())?;
                        response.header(CONTENT_TYPE, "application/json")
                    }
                };

                let gzip = gz.finish()?;

                let response = response
                    .body(lambda_http::Body::Binary(gzip))
                    .map_err(Box::new)?;

                Ok::<_, lambda_runtime::Error>(response)
            }
        });

        moosicbox_task::block_on_runtime("html server", &self.handle, async move {
            lambda_http::run(func).await
        })
        .map_err(|e| e as Box<dyn std::error::Error + Send>)?;

        log::debug!("run: finished");

        Ok(())
    }
}
