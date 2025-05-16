#![cfg_attr(feature = "fail-on-warnings", deny(warnings))]
#![warn(clippy::all, clippy::pedantic, clippy::nursery, clippy::cargo)]
#![allow(clippy::multiple_crate_versions)]

use std::sync::Arc;

use async_trait::async_trait;
use clap::{Parser, Subcommand, arg, command};
use hyperchad_renderer::{Color, RenderRunner, Renderer, ToRenderRunner};
use hyperchad_router::{Navigation, RoutePath, Router};
use moosicbox_env_utils::default_env_usize;
use switchy_async::{futures::channel::oneshot, runtime::Runtime, task};

pub mod renderer;

#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error(transparent)]
    IO(#[from] std::io::Error),
    #[error(transparent)]
    Builder(#[from] BuilderError),
    #[error(transparent)]
    OtherSend(#[from] Box<dyn std::error::Error + Send>),
    #[error(transparent)]
    Async(#[from] switchy_async::Error),
    #[error(transparent)]
    Join(#[from] switchy_async::task::JoinError),
}

#[derive(Debug, thiserror::Error)]
pub enum BuilderError {
    #[error("Missing Router")]
    MissingRouter,
    #[error("Missing Runtime")]
    MissingRuntime,
}

#[derive(Parser, Debug)]
#[command(version, about, long_about = None)]
struct Args {
    #[command(subcommand)]
    cmd: Commands,
}

#[derive(Subcommand, Debug, Clone, PartialEq, Eq)]
enum Commands {
    DynamicRoutes,
    Gen {
        #[arg(short, long)]
        output: Option<String>,
    },
    Clean {
        #[arg(short, long)]
        output: Option<String>,
    },
    Serve,
}

#[async_trait]
pub trait Generator {
    async fn generate(&self, router: &Router, output: Option<String>) -> Result<(), Error> {
        unimplemented!("generate: unimplemented router={router:?} output={output:?}")
    }

    #[cfg(feature = "assets")]
    fn assets(&self) -> impl Iterator<Item = &hyperchad_renderer::assets::StaticAssetRoute> {
        std::iter::empty()
    }
}

#[async_trait]
pub trait Cleaner {
    async fn clean(&self, output: Option<String>) -> Result<(), Error> {
        unimplemented!("clean: unimplemented output={output:?}")
    }
}

#[cfg(feature = "logic")]
type ActionHandler = Box<
    dyn Fn(
            (&str, Option<&hyperchad_actions::logic::Value>),
        ) -> Result<bool, Box<dyn std::error::Error>>
        + Send
        + Sync,
>;
type ResizeListener = Box<dyn Fn(f32, f32) -> Result<(), Box<dyn std::error::Error>> + Send + Sync>;

#[derive(Clone)]
pub struct AppBuilder {
    router: Option<Router>,
    initial_route: Option<Navigation>,
    x: Option<i32>,
    y: Option<i32>,
    background: Option<Color>,
    title: Option<String>,
    description: Option<String>,
    viewport: Option<String>,
    width: Option<f32>,
    height: Option<f32>,
    runtime: Option<Arc<switchy_async::runtime::Runtime>>,
    #[cfg(feature = "logic")]
    action_handlers: Vec<Arc<ActionHandler>>,
    resize_listeners: Vec<Arc<ResizeListener>>,
    #[cfg(feature = "assets")]
    static_asset_routes: Vec<hyperchad_renderer::assets::StaticAssetRoute>,
}

impl std::fmt::Debug for AppBuilder {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let mut builder = f.debug_struct("AppBuilder");

        builder
            .field("router", &self.router)
            .field("initial_route", &self.initial_route)
            .field("x", &self.x)
            .field("y", &self.y)
            .field("background", &self.background)
            .field("title", &self.title)
            .field("description", &self.description)
            .field("viewport", &self.viewport)
            .field("width", &self.width)
            .field("height", &self.height)
            .field("runtime", &self.runtime);

        #[cfg(feature = "assets")]
        builder.field("static_asset_routes", &self.static_asset_routes);

        builder.finish_non_exhaustive()
    }
}

impl Default for AppBuilder {
    fn default() -> Self {
        Self::new()
    }
}

impl AppBuilder {
    #[must_use]
    pub const fn new() -> Self {
        Self {
            router: None,
            initial_route: None,
            x: None,
            y: None,
            background: None,
            title: None,
            description: None,
            viewport: None,
            width: None,
            height: None,
            runtime: None,
            #[cfg(feature = "logic")]
            action_handlers: vec![],
            resize_listeners: vec![],
            #[cfg(feature = "assets")]
            static_asset_routes: vec![],
        }
    }

    #[must_use]
    pub fn with_router(mut self, router: Router) -> Self {
        self.router = Some(router);
        self
    }

    pub fn router(&mut self, router: Router) -> &mut Self {
        self.router = Some(router);
        self
    }

    #[must_use]
    pub fn with_initial_route(mut self, initial_route: impl Into<Navigation>) -> Self {
        self.initial_route = Some(initial_route.into());
        self
    }

    pub fn initial_route(&mut self, initial_route: impl Into<Navigation>) -> &mut Self {
        self.initial_route = Some(initial_route.into());
        self
    }

    #[must_use]
    pub const fn with_width(mut self, width: f32) -> Self {
        self.width.replace(width);
        self
    }

    pub const fn width(&mut self, width: f32) -> &mut Self {
        self.width = Some(width);
        self
    }

    #[must_use]
    pub const fn with_height(mut self, height: f32) -> Self {
        self.height.replace(height);
        self
    }

    pub const fn height(&mut self, height: f32) -> &mut Self {
        self.height = Some(height);
        self
    }

    #[must_use]
    pub const fn with_size(self, width: f32, height: f32) -> Self {
        self.with_width(width).with_height(height)
    }

    pub const fn size(&mut self, width: f32, height: f32) -> &mut Self {
        self.width(width).height(height);
        self
    }

    #[must_use]
    pub const fn with_x(mut self, x: i32) -> Self {
        self.x.replace(x);
        self
    }

    pub const fn x(&mut self, x: i32) -> &mut Self {
        self.x = Some(x);
        self
    }

    #[must_use]
    pub const fn with_y(mut self, y: i32) -> Self {
        self.y.replace(y);
        self
    }

    pub const fn y(&mut self, y: i32) -> &mut Self {
        self.y = Some(y);
        self
    }

    #[must_use]
    pub const fn with_position(self, x: i32, y: i32) -> Self {
        self.with_x(x).with_y(y)
    }

    pub const fn position(&mut self, x: i32, y: i32) -> &mut Self {
        self.x(x).y(y);
        self
    }

    #[must_use]
    pub fn with_viewport(mut self, content: String) -> Self {
        self.viewport.replace(content);
        self
    }

    #[must_use]
    pub const fn with_background(mut self, color: Color) -> Self {
        self.background.replace(color);
        self
    }

    #[must_use]
    pub fn with_title(mut self, title: String) -> Self {
        self.title.replace(title);
        self
    }

    #[must_use]
    pub fn with_description(mut self, description: String) -> Self {
        self.description.replace(description);
        self
    }

    #[cfg(feature = "logic")]
    #[must_use]
    pub fn with_action_handler<E: std::error::Error + 'static>(
        mut self,
        func: impl Fn(&str, Option<&hyperchad_actions::logic::Value>) -> Result<bool, E>
        + Send
        + Sync
        + 'static,
    ) -> Self {
        self.action_handlers.push(Arc::new(Box::new(move |(a, b)| {
            func(a, b).map_err(|e| Box::new(e) as Box<dyn std::error::Error>)
        })));
        self
    }

    #[must_use]
    pub fn with_runtime(self, runtime: Runtime) -> Self {
        self.with_runtime_arc(Arc::new(runtime))
    }

    #[must_use]
    pub fn with_runtime_arc(mut self, runtime: Arc<Runtime>) -> Self {
        self.runtime.replace(runtime);
        self
    }

    #[must_use]
    pub fn with_on_resize<E: std::error::Error + 'static>(
        mut self,
        func: impl Fn(f32, f32) -> Result<(), E> + Send + Sync + 'static,
    ) -> Self {
        self.resize_listeners
            .push(Arc::new(Box::new(move |width, height| {
                func(width, height).map_err(|e| Box::new(e) as Box<dyn std::error::Error>)
            })));
        self
    }

    #[cfg(feature = "logic")]
    #[allow(unused)]
    fn listen_actions(
        &self,
        action_handlers: Vec<Arc<ActionHandler>>,
    ) -> flume::Sender<(String, Option<hyperchad_actions::logic::Value>)> {
        let (action_tx, action_rx) =
            flume::unbounded::<(String, Option<hyperchad_actions::logic::Value>)>();

        self.runtime_handle().spawn(async move {
            while let Ok((action, value)) = action_rx.recv_async().await {
                log::debug!(
                    "Received action: action={action} value={value:?} for {} handler(s)",
                    action_handlers.len()
                );
                for handler in &action_handlers {
                    if let Err(e) = handler((action.as_str(), value.as_ref())) {
                        moosicbox_assert::die_or_error!(
                            "Action handler error action={action}: {e:?}"
                        );
                    }
                }
            }
        });

        action_tx
    }

    #[allow(unused)]
    fn listen_resize(
        &self,
        resize_listeners: Vec<Arc<ResizeListener>>,
    ) -> flume::Sender<(f32, f32)> {
        let (resize_tx, resize_rx) = flume::unbounded::<(f32, f32)>();

        self.runtime_handle().spawn(async move {
            while let Ok((width, height)) = resize_rx.recv_async().await {
                log::debug!(
                    "Received resize: {width}, {height} for {} listener(s)",
                    resize_listeners.len()
                );
                for listener in &resize_listeners {
                    if let Err(e) = listener(width, height) {
                        moosicbox_assert::die_or_error!(
                            "Action listener error width={width} height={height}: {e:?}"
                        );
                    }
                }
            }
        });

        resize_tx
    }

    fn runtime_handle(&self) -> switchy_async::runtime::Handle {
        self.runtime
            .clone()
            .map_or_else(switchy_async::runtime::Handle::current, |x| {
                x.handle().clone()
            })
    }

    #[cfg(feature = "assets")]
    #[must_use]
    pub fn with_static_asset_route(
        mut self,
        path: impl Into<hyperchad_renderer::assets::StaticAssetRoute>,
    ) -> Self {
        self.static_asset_routes.push(path.into());
        self
    }

    #[cfg(feature = "assets")]
    pub fn static_asset_route(
        &mut self,
        path: impl Into<hyperchad_renderer::assets::StaticAssetRoute>,
    ) -> &mut Self {
        self.static_asset_routes.push(path.into());
        self
    }

    /// # Errors
    ///
    /// * If the asset path type is a not found
    /// * If the asset path type is an invalid path type (not a file or directory)
    #[cfg(feature = "assets")]
    pub fn with_static_asset_route_result<
        Path: TryInto<hyperchad_renderer::assets::StaticAssetRoute>,
    >(
        mut self,
        path: Path,
    ) -> Result<Self, Path::Error> {
        self.static_asset_routes.push(path.try_into()?);
        Ok(self)
    }

    /// # Errors
    ///
    /// * If the asset path type is a not found
    /// * If the asset path type is an invalid path type (not a file or directory)
    #[cfg(feature = "assets")]
    pub fn static_asset_route_result<
        Path: TryInto<hyperchad_renderer::assets::StaticAssetRoute>,
    >(
        &mut self,
        path: Path,
    ) -> Result<&mut Self, Path::Error> {
        self.static_asset_routes.push(path.try_into()?);
        Ok(self)
    }

    /// # Errors
    ///
    /// * If the `AppBuilder` is missing a router
    pub fn build<R: Renderer + ToRenderRunner + Generator + Cleaner + Clone + 'static>(
        self,
        renderer: R,
    ) -> Result<App<R>, BuilderError> {
        let router = self.router.ok_or(BuilderError::MissingRouter)?;

        Ok(App {
            renderer,
            router,
            runtime: self.runtime,
            x: self.x,
            y: self.y,
            background: self.background,
            title: self.title,
            description: self.description,
            viewport: self.viewport,
            width: self.width.unwrap_or(800.0),
            height: self.height.unwrap_or(600.0),
            initial_route: self.initial_route,
        })
    }
}

#[derive(Debug)]
pub struct App<R: Renderer + ToRenderRunner + Generator + Cleaner + Clone + 'static> {
    pub renderer: R,
    pub router: Router,
    pub runtime: Option<Arc<switchy_async::runtime::Runtime>>,
    x: Option<i32>,
    y: Option<i32>,
    background: Option<Color>,
    title: Option<String>,
    description: Option<String>,
    viewport: Option<String>,
    width: f32,
    height: f32,
    initial_route: Option<Navigation>,
}

impl<R: Renderer + ToRenderRunner + Generator + Cleaner + Clone + 'static> App<R> {
    /// # Errors
    ///
    /// * If the `App` fails to start
    ///
    /// # Panics
    ///
    /// * If the `MAX_THREADS` environment variable is not a valid `u16` integer
    pub fn run(self) -> Result<(), Error> {
        let args = Args::parse();

        match args.cmd {
            Commands::DynamicRoutes => self.dynamic_routes(),
            Commands::Clean { output } => self.clean_sync(output),
            Commands::Gen { output } => self.generate_sync(output),
            Commands::Serve => self.handle_serve(),
        }
    }

    fn handle_serve(mut self) -> Result<(), Error> {
        let runtime = self.runtime()?;

        let mut runner = runtime.block_on(async move { self.serve().await })?;

        let (tx, rx) = oneshot::channel();

        runtime.spawn(rx);

        runner.run()?;

        tx.send(()).unwrap();

        Ok(())
    }

    fn runtime(&mut self) -> Result<Arc<Runtime>, Error> {
        Ok(if let Some(runtime) = self.runtime.clone() {
            runtime
        } else {
            let threads = default_env_usize("MAX_THREADS", 64).unwrap_or(64);
            log::debug!("Running with {threads} max blocking threads");
            let runtime = Arc::new(
                switchy_async::runtime::Builder::new()
                    .max_blocking_threads(u16::try_from(threads).unwrap())
                    .build()?,
            );
            self.runtime = Some(runtime.clone());
            runtime
        })
    }

    /// # Errors
    ///
    /// * If the `App` fails to generate the dynamic routes
    ///
    /// # Panics
    ///
    /// * If the `Router` `routes` `RwLock` is poisoned
    pub fn dynamic_routes(&self) -> Result<(), Error> {
        let dynamic_routes = self.router.routes.read().unwrap().clone();

        for (path, _) in &dynamic_routes {
            println!(
                "{}",
                match path {
                    RoutePath::Literal(path) => path,
                    RoutePath::Literals(paths) => {
                        if let Some(path) = paths.first() {
                            path
                        } else {
                            continue;
                        }
                    }
                }
            );
        }

        Ok(())
    }

    /// # Errors
    ///
    /// * If the `Renderer` fails to generate the output
    pub fn generate_sync(mut self, output: Option<String>) -> Result<(), Error> {
        self.runtime()?
            .block_on(async move { self.generate(output).await })
    }

    /// # Errors
    ///
    /// * If the `Renderer` fails to generate the output
    pub async fn generate(&self, output: Option<String>) -> Result<(), Error> {
        self.renderer.generate(&self.router, output).await?;
        Ok(())
    }

    /// # Errors
    ///
    /// * If the `Renderer` fails to clean the output
    pub fn clean_sync(mut self, output: Option<String>) -> Result<(), Error> {
        self.runtime()?
            .block_on(async move { self.clean(output).await })
    }

    /// # Errors
    ///
    /// * If the `Renderer` fails to clean the output
    pub async fn clean(&self, output: Option<String>) -> Result<(), Error> {
        self.renderer.clean(output).await?;
        Ok(())
    }

    /// # Errors
    ///
    /// * If the `App` fails to serve
    ///
    /// # Panics
    ///
    /// * If the `MAX_THREADS` environment variable is not a valid `u16` integer
    pub fn serve_sync(mut self) -> Result<Box<dyn RenderRunner>, Error> {
        self.runtime()?.block_on(async move { self.serve().await })
    }

    /// # Errors
    ///
    /// * If the `App` fails to serve
    #[allow(clippy::unused_async)]
    pub async fn serve(&mut self) -> Result<Box<dyn RenderRunner>, Error> {
        let router = self.router.clone();
        let initial_route = self.initial_route.clone();

        log::debug!("app: starting app");
        if let Some(initial_route) = initial_route {
            log::debug!("app: navigating to home");
            let _handle = router.navigate_spawn(initial_route);
        }

        let runtime = self.runtime()?;
        let handle = runtime.handle().clone();
        let mut renderer = self.renderer.clone();

        let width = self.width;
        let height = self.height;
        let x = self.x;
        let y = self.y;
        let background = self.background;
        let title = self.title.clone();
        let description = self.description.clone();
        let viewport = self.viewport.clone();

        task::spawn({
            let renderer = renderer.clone();
            async move {
                log::debug!("app_native_lib::start: router listening");
                #[allow(unused_variables, clippy::never_loop)]
                while let Some(content) = router.wait_for_navigation().await {
                    log::debug!("app_native_lib::start: router received content");
                    match content {
                        hyperchad_renderer::Content::View(view) => {
                            renderer.render(view).await?;
                        }
                        hyperchad_renderer::Content::PartialView(..) => {
                            moosicbox_assert::die_or_warn!("Received invalid content type");
                        }
                        #[cfg(feature = "json")]
                        hyperchad_renderer::Content::Json(..) => {
                            moosicbox_assert::die_or_warn!("Received invalid content type");
                        }
                    }
                }
                Ok::<_, Error>(())
            }
        });

        renderer
            .init(
                width,
                height,
                x,
                y,
                background,
                title.as_deref(),
                description.as_deref(),
                viewport.as_deref(),
            )
            .await?;

        Ok(renderer.to_runner(handle)?)
    }
}
