#![cfg_attr(feature = "fail-on-warnings", deny(warnings))]
#![warn(clippy::all, clippy::pedantic, clippy::nursery, clippy::cargo)]

use std::{io::Write, marker::PhantomData};

use async_trait::async_trait;
use flate2::{write::GzEncoder, Compression};
use hyperchad_renderer::{RenderRunner, ToRenderRunner};
use lambda_http::{
    http::header::{CONTENT_ENCODING, CONTENT_TYPE},
    service_fn, Request, Response,
};
use tokio::runtime::Handle;

pub use lambda_http;
pub use lambda_runtime;

#[async_trait]
pub trait LambdaResponseProcessor<T: Send + Sync + Clone> {
    /// # Errors
    ///
    /// * If the request fails to prepare
    fn prepare_request(&self, req: Request) -> Result<T, lambda_runtime::Error>;

    async fn to_html(&self, data: T) -> Result<String, lambda_runtime::Error>;
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
                let html = app.processor.to_html(data).await?;
                let mut gz = GzEncoder::new(vec![], Compression::default());
                gz.write_all(html.as_bytes())?;
                let gzip = gz.finish()?;

                let response = Response::builder()
                    .status(200)
                    .header(CONTENT_TYPE, "text/html")
                    .header(CONTENT_ENCODING, "gzip")
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
