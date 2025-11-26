//! Application framework for building hyperchad applications with pluggable renderers.
//!
//! This crate provides the core [`App`] type and [`AppBuilder`] for constructing hyperchad
//! applications that can render to multiple backends (egui, fltk, HTML/web).
//!
//! # Features
//!
//! * Multiple renderer backends: egui, fltk, HTML (Actix/Lambda), vanilla JS
//! * Static site generation for HTML renderers
//! * Router-based navigation
//! * Asset management
//! * Action handling and resize listeners
//!
//! # Example
//!
//! ```rust,no_run
//! # use hyperchad_app::AppBuilder;
//! # use hyperchad_router::Router;
//! # fn main() -> Result<(), Box<dyn std::error::Error>> {
//! let router = Router::new();
//! let app = AppBuilder::new()
//!     .with_router(router)
//!     .with_size(800.0, 600.0)
//!     .build_default()?;
//! app.run()?;
//! # Ok(())
//! # }
//! ```

#![cfg_attr(feature = "fail-on-warnings", deny(warnings))]
#![warn(clippy::all, clippy::pedantic, clippy::nursery, clippy::cargo)]
#![allow(clippy::multiple_crate_versions)]

use std::sync::Arc;

use async_trait::async_trait;
use clap::{Parser, Subcommand, arg, command};
use hyperchad_renderer::{Color, RenderRunner, Renderer, ToRenderRunner};
use hyperchad_router::{Navigation, RoutePath, Router};
use switchy::unsync::{futures::channel::oneshot, runtime::Handle};
use switchy_env::var_parse_or;

/// Renderer implementations and type aliases for different backends.
pub mod renderer;

/// Errors that can occur in the hyperchad app.
#[derive(Debug, thiserror::Error)]
pub enum Error {
    /// I/O error occurred during app operations.
    #[error(transparent)]
    IO(#[from] std::io::Error),
    /// App builder configuration error.
    #[error(transparent)]
    Builder(#[from] BuilderError),
    /// Generic error from another component.
    #[error(transparent)]
    OtherSend(#[from] Box<dyn std::error::Error + Send>),
    /// Async runtime error.
    #[error(transparent)]
    Async(#[from] switchy::unsync::Error),
    /// Task join error when waiting for async task completion.
    #[error(transparent)]
    Join(#[from] switchy::unsync::task::JoinError),
}

/// Errors that can occur when building an [`App`].
#[derive(Debug, thiserror::Error)]
pub enum BuilderError {
    /// Router was not provided to the builder.
    #[error("Missing Router")]
    MissingRouter,
    /// Runtime handle was not provided to the builder.
    #[error("Missing Runtime")]
    MissingRuntime,
}

/// Command-line arguments for the hyperchad application.
#[derive(Parser, Debug)]
#[command(version, about, long_about = None)]
struct Args {
    #[command(subcommand)]
    cmd: Commands,
}

/// Available subcommands for the hyperchad application.
#[derive(Subcommand, Debug, Clone, PartialEq, Eq)]
enum Commands {
    /// Prints all dynamic routes registered in the router.
    DynamicRoutes,
    /// Generates static output for all routes.
    Gen {
        /// Optional output directory path.
        #[arg(short, long)]
        output: Option<String>,
    },
    /// Cleans the generated output directory.
    Clean {
        /// Optional output directory path.
        #[arg(short, long)]
        output: Option<String>,
    },
    /// Starts serving the application.
    Serve,
}

/// Trait for generating static output from a router.
#[async_trait]
pub trait Generator {
    /// Generates static output for the given router.
    ///
    /// # Errors
    ///
    /// * If the renderer fails to generate the output
    async fn generate(&self, router: &Router, output: Option<String>) -> Result<(), Error> {
        unimplemented!("generate: unimplemented router={router:?} output={output:?}")
    }
}

/// Trait for cleaning generated output.
#[async_trait]
pub trait Cleaner {
    /// Cleans the generated output directory.
    ///
    /// # Errors
    ///
    /// * If the renderer fails to clean the output
    async fn clean(&self, output: Option<String>) -> Result<(), Error> {
        unimplemented!("clean: unimplemented output={output:?}")
    }
}

#[cfg(feature = "logic")]
/// Type alias for action handler functions that process application actions.
///
/// The handler receives an action name and optional value, returning whether the action was handled.
type ActionHandler = Box<
    dyn Fn(
            (&str, Option<&hyperchad_actions::logic::Value>),
        ) -> Result<bool, Box<dyn std::error::Error>>
        + Send
        + Sync,
>;

/// Type alias for resize listener functions that handle window resize events.
///
/// The listener receives the new width and height in pixels.
type ResizeListener = Box<dyn Fn(f32, f32) -> Result<(), Box<dyn std::error::Error>> + Send + Sync>;

/// Builder for constructing an [`App`] instance.
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
    runtime_handle: Option<switchy::unsync::runtime::Handle>,
    #[cfg(feature = "logic")]
    action_handlers: Vec<Arc<ActionHandler>>,
    resize_listeners: Vec<Arc<ResizeListener>>,
    #[cfg(feature = "assets")]
    static_asset_routes: Vec<hyperchad_renderer::assets::StaticAssetRoute>,
    #[cfg(feature = "html")]
    css_urls: Vec<String>,
    #[cfg(feature = "html")]
    css_paths: Vec<String>,
    #[cfg(feature = "html")]
    inline_css: Vec<String>,
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
            .field("runtime", &self.runtime_handle);

        #[cfg(feature = "assets")]
        builder.field("static_asset_routes", &self.static_asset_routes);

        #[cfg(feature = "html")]
        {
            builder
                .field("css_urls", &self.css_urls)
                .field("css_paths", &self.css_paths)
                .field("inline_css", &self.inline_css);
        }

        builder.finish_non_exhaustive()
    }
}

impl Default for AppBuilder {
    fn default() -> Self {
        Self::new()
    }
}

impl AppBuilder {
    /// Creates a new empty `AppBuilder`.
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
            runtime_handle: None,
            #[cfg(feature = "logic")]
            action_handlers: vec![],
            resize_listeners: vec![],
            #[cfg(feature = "assets")]
            static_asset_routes: vec![],
            #[cfg(feature = "html")]
            css_urls: vec![],
            #[cfg(feature = "html")]
            css_paths: vec![],
            #[cfg(feature = "html")]
            inline_css: vec![],
        }
    }

    /// Sets the router for the application (builder pattern).
    #[must_use]
    pub fn with_router(mut self, router: Router) -> Self {
        self.router = Some(router);
        self
    }

    /// Sets the router for the application (mutable reference pattern).
    pub fn router(&mut self, router: Router) -> &mut Self {
        self.router = Some(router);
        self
    }

    /// Sets the initial route to navigate to when the application starts (builder pattern).
    #[must_use]
    pub fn with_initial_route(mut self, initial_route: impl Into<Navigation>) -> Self {
        self.initial_route = Some(initial_route.into());
        self
    }

    /// Sets the initial route to navigate to when the application starts (mutable reference pattern).
    pub fn initial_route(&mut self, initial_route: impl Into<Navigation>) -> &mut Self {
        self.initial_route = Some(initial_route.into());
        self
    }

    /// Sets the window width in pixels (builder pattern).
    #[must_use]
    pub const fn with_width(mut self, width: f32) -> Self {
        self.width.replace(width);
        self
    }

    /// Sets the window width in pixels (mutable reference pattern).
    pub const fn width(&mut self, width: f32) -> &mut Self {
        self.width = Some(width);
        self
    }

    /// Sets the window height in pixels (builder pattern).
    #[must_use]
    pub const fn with_height(mut self, height: f32) -> Self {
        self.height.replace(height);
        self
    }

    /// Sets the window height in pixels (mutable reference pattern).
    pub const fn height(&mut self, height: f32) -> &mut Self {
        self.height = Some(height);
        self
    }

    /// Sets both window width and height in pixels (builder pattern).
    #[must_use]
    pub const fn with_size(self, width: f32, height: f32) -> Self {
        self.with_width(width).with_height(height)
    }

    /// Sets both window width and height in pixels (mutable reference pattern).
    pub const fn size(&mut self, width: f32, height: f32) -> &mut Self {
        self.width(width).height(height);
        self
    }

    /// Sets the window X position in pixels (builder pattern).
    #[must_use]
    pub const fn with_x(mut self, x: i32) -> Self {
        self.x.replace(x);
        self
    }

    /// Sets the window X position in pixels (mutable reference pattern).
    pub const fn x(&mut self, x: i32) -> &mut Self {
        self.x = Some(x);
        self
    }

    /// Sets the window Y position in pixels (builder pattern).
    #[must_use]
    pub const fn with_y(mut self, y: i32) -> Self {
        self.y.replace(y);
        self
    }

    /// Sets the window Y position in pixels (mutable reference pattern).
    pub const fn y(&mut self, y: i32) -> &mut Self {
        self.y = Some(y);
        self
    }

    /// Sets both window X and Y position in pixels (builder pattern).
    #[must_use]
    pub const fn with_position(self, x: i32, y: i32) -> Self {
        self.with_x(x).with_y(y)
    }

    /// Sets both window X and Y position in pixels (mutable reference pattern).
    pub const fn position(&mut self, x: i32, y: i32) -> &mut Self {
        self.x(x).y(y);
        self
    }

    /// Sets the HTML viewport meta tag content for HTML renderers (builder pattern).
    #[must_use]
    pub fn with_viewport(mut self, content: String) -> Self {
        self.viewport.replace(content);
        self
    }

    /// Sets the background color for the application window (builder pattern).
    #[must_use]
    pub const fn with_background(mut self, color: Color) -> Self {
        self.background.replace(color);
        self
    }

    /// Sets the window or page title (builder pattern).
    #[must_use]
    pub fn with_title(mut self, title: String) -> Self {
        self.title.replace(title);
        self
    }

    /// Sets the page description meta tag for HTML renderers (builder pattern).
    #[must_use]
    pub fn with_description(mut self, description: String) -> Self {
        self.description.replace(description);
        self
    }

    /// Adds a handler function for application actions (builder pattern).
    ///
    /// The handler receives an action name and optional value, and returns whether the action was handled.
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

    /// Sets a custom async runtime handle (builder pattern).
    #[must_use]
    pub fn with_runtime_handle(mut self, handle: Handle) -> Self {
        self.runtime_handle.replace(handle);
        self
    }

    /// Adds a callback to be invoked when the window is resized (builder pattern).
    ///
    /// The callback receives the new width and height in pixels.
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

    /// Sets up a listener for application actions and spawns a handler task.
    ///
    /// Returns a sender that can be used to trigger actions.
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

    /// Sets up a listener for window resize events and spawns a handler task.
    ///
    /// Returns a sender that can be used to notify listeners of resize events.
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

    /// Returns the runtime handle, either from the builder or the current runtime.
    #[must_use]
    fn runtime_handle(&self) -> switchy::unsync::runtime::Handle {
        self.runtime_handle
            .clone()
            .unwrap_or_else(switchy::unsync::runtime::Handle::current)
    }

    /// Adds a static asset route for serving files (builder pattern).
    #[cfg(feature = "assets")]
    #[must_use]
    pub fn with_static_asset_route(
        mut self,
        path: impl Into<hyperchad_renderer::assets::StaticAssetRoute>,
    ) -> Self {
        self.static_asset_routes.push(path.into());
        self
    }

    /// Adds a static asset route for serving files (mutable reference pattern).
    #[cfg(feature = "assets")]
    pub fn static_asset_route(
        &mut self,
        path: impl Into<hyperchad_renderer::assets::StaticAssetRoute>,
    ) -> &mut Self {
        self.static_asset_routes.push(path.into());
        self
    }

    /// Adds a static asset route for serving files, returning a `Result` (builder pattern).
    ///
    /// # Errors
    ///
    /// * If the asset path is not found
    /// * If the asset path is an invalid path type (not a file or directory)
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

    /// Adds a static asset route for serving files, returning a `Result` (mutable reference pattern).
    ///
    /// # Errors
    ///
    /// * If the asset path is not found
    /// * If the asset path is an invalid path type (not a file or directory)
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

    /// Adds a CSS URL to be linked in the HTML head for HTML renderers (builder pattern).
    #[cfg(feature = "html")]
    #[must_use]
    pub fn with_css_url(mut self, url: impl Into<String>) -> Self {
        self.css_urls.push(url.into());
        self
    }

    /// Adds a CSS URL to be linked in the HTML head for HTML renderers (mutable reference pattern).
    #[cfg(feature = "html")]
    pub fn css_url(&mut self, url: impl Into<String>) -> &mut Self {
        self.css_urls.push(url.into());
        self
    }

    /// Adds multiple CSS URLs to be linked in the HTML head for HTML renderers (builder pattern).
    #[cfg(feature = "html")]
    #[must_use]
    pub fn with_css_urls(mut self, urls: impl IntoIterator<Item = impl Into<String>>) -> Self {
        self.css_urls.extend(urls.into_iter().map(Into::into));
        self
    }

    /// Adds a CSS file path to be served and linked for HTML renderers (builder pattern).
    #[cfg(feature = "html")]
    #[must_use]
    pub fn with_css_path(mut self, path: impl Into<String>) -> Self {
        self.css_paths.push(path.into());
        self
    }

    /// Adds a CSS file path to be served and linked for HTML renderers (mutable reference pattern).
    #[cfg(feature = "html")]
    pub fn css_path(&mut self, path: impl Into<String>) -> &mut Self {
        self.css_paths.push(path.into());
        self
    }

    /// Adds multiple CSS file paths to be served and linked for HTML renderers (builder pattern).
    #[cfg(feature = "html")]
    #[must_use]
    pub fn with_css_paths(mut self, paths: impl IntoIterator<Item = impl Into<String>>) -> Self {
        self.css_paths.extend(paths.into_iter().map(Into::into));
        self
    }

    /// Adds inline CSS to be included in a style tag for HTML renderers (builder pattern).
    #[cfg(feature = "html")]
    #[must_use]
    pub fn with_inline_css(mut self, css: impl Into<String>) -> Self {
        self.inline_css.push(css.into());
        self
    }

    /// Adds inline CSS to be included in a style tag for HTML renderers (mutable reference pattern).
    #[cfg(feature = "html")]
    pub fn inline_css(&mut self, css: impl Into<String>) -> &mut Self {
        self.inline_css.push(css.into());
        self
    }

    /// Adds multiple inline CSS blocks to be included in style tags for HTML renderers (builder pattern).
    #[cfg(feature = "html")]
    #[must_use]
    pub fn with_inline_css_blocks(
        mut self,
        css_blocks: impl IntoIterator<Item = impl Into<String>>,
    ) -> Self {
        self.inline_css
            .extend(css_blocks.into_iter().map(Into::into));
        self
    }

    /// Builds an `App` instance with the specified renderer.
    ///
    /// # Errors
    ///
    /// * [`BuilderError::MissingRouter`] if the `AppBuilder` is missing a router
    pub fn build<R: Renderer + ToRenderRunner + Generator + Cleaner + Clone + 'static>(
        self,
        renderer: R,
    ) -> Result<App<R>, BuilderError> {
        let router = self.router.ok_or(BuilderError::MissingRouter)?;

        Ok(App {
            renderer,
            router,
            runtime: None,
            runtime_handle: self.runtime_handle,
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

/// Represents a hyperchad application with a specific renderer.
#[derive(Debug)]
pub struct App<R: Renderer + ToRenderRunner + Generator + Cleaner + Clone + 'static> {
    /// The renderer used to display the application.
    pub renderer: R,
    /// The router managing navigation and routes.
    pub router: Router,
    runtime: Option<switchy::unsync::runtime::Runtime>,
    /// Optional handle to the async runtime.
    pub runtime_handle: Option<switchy::unsync::runtime::Handle>,
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
    /// Runs the application by parsing command-line arguments and executing the appropriate command.
    ///
    /// Supports subcommands: `serve`, `gen`, `clean`, and `dynamic-routes`.
    ///
    /// # Errors
    ///
    /// * If the `App` fails to start
    ///
    /// # Panics
    ///
    /// * If the `MAX_THREADS` environment variable is not a valid `u16` integer
    pub fn run(self) -> Result<(), Error> {
        let args = Args::parse();

        log::debug!("run: args={args:?}");

        match args.cmd {
            Commands::DynamicRoutes => self.dynamic_routes(),
            Commands::Clean { output } => self.clean_sync(output),
            Commands::Gen { output } => self.generate_sync(output),
            Commands::Serve => self.handle_serve(),
        }
    }

    /// Handles the serve command by starting the application server and running the render loop.
    ///
    /// # Errors
    ///
    /// * If the `App` fails to serve
    ///
    /// # Panics
    ///
    /// * If the one-shot channel fails to send after the runner completes
    pub fn handle_serve(mut self) -> Result<(), Error> {
        let runtime = self.runtime_handle()?;

        let mut runner = runtime.block_on(async move { self.serve().await })?;

        let (tx, rx) = oneshot::channel();

        runtime.spawn(rx);

        runner.run()?;

        tx.send(()).unwrap();

        Ok(())
    }

    /// Gets or creates the async runtime handle.
    ///
    /// If a runtime handle was provided to the builder, returns that handle.
    /// Otherwise, creates a new runtime with thread configuration from the
    /// `MAX_THREADS` environment variable (defaults to 64 threads).
    ///
    /// # Errors
    ///
    /// * If the runtime fails to initialize
    ///
    /// # Panics
    ///
    /// * If the `MAX_THREADS` environment variable exceeds `u16::MAX`
    fn runtime_handle(&mut self) -> Result<Handle, Error> {
        Ok(if let Some(handle) = self.runtime_handle.clone() {
            handle
        } else {
            let threads = var_parse_or("MAX_THREADS", 64usize);
            log::debug!("Running with {threads} max blocking threads");
            let runtime = switchy::unsync::runtime::Builder::new()
                .max_blocking_threads(u16::try_from(threads).unwrap())
                .build()?;
            let handle = runtime.handle();
            self.runtime_handle = Some(handle.clone());
            self.runtime = Some(runtime);
            handle
        })
    }

    /// Prints all dynamic routes registered in the router to stdout.
    ///
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
                    RoutePath::LiteralPrefix(..) => continue,
                }
            );
        }

        Ok(())
    }

    /// Generates static output for all routes (synchronous version).
    ///
    /// # Errors
    ///
    /// * If the `Renderer` fails to generate the output
    pub fn generate_sync(mut self, output: Option<String>) -> Result<(), Error> {
        self.runtime_handle()?
            .block_on(async move { self.generate(output).await })
    }

    /// Generates static output for all routes (async version).
    ///
    /// # Errors
    ///
    /// * [`Error::Builder`] if the renderer fails to generate the output
    pub async fn generate(&self, output: Option<String>) -> Result<(), Error> {
        self.renderer.generate(&self.router, output).await?;
        Ok(())
    }

    /// Cleans the generated output directory (synchronous version).
    ///
    /// # Errors
    ///
    /// * If the `Renderer` fails to clean the output
    pub fn clean_sync(mut self, output: Option<String>) -> Result<(), Error> {
        self.runtime_handle()?
            .block_on(async move { self.clean(output).await })
    }

    /// Cleans the generated output directory (async version).
    ///
    /// # Errors
    ///
    /// * [`Error::IO`] if the renderer fails to clean the output directory
    pub async fn clean(&self, output: Option<String>) -> Result<(), Error> {
        self.renderer.clean(output).await?;
        Ok(())
    }

    /// Starts serving the application and returns a runner (synchronous version).
    ///
    /// # Errors
    ///
    /// * If the `App` fails to serve
    ///
    /// # Panics
    ///
    /// * If the `MAX_THREADS` environment variable is not a valid `u16` integer
    pub fn serve_sync(mut self) -> Result<Box<dyn RenderRunner>, Error> {
        self.runtime_handle()?
            .block_on(async move { self.serve().await })
    }

    /// Starts serving the application and returns a runner (async version).
    ///
    /// # Errors
    ///
    /// * [`Error::Builder`] if the app fails to initialize the runtime
    /// * [`Error::OtherSend`] if the renderer fails to initialize
    #[allow(clippy::unused_async)]
    pub async fn serve(&mut self) -> Result<Box<dyn RenderRunner>, Error> {
        let router = self.router.clone();
        let initial_route = self.initial_route.clone();

        log::debug!("app: starting app");
        if let Some(initial_route) = initial_route {
            log::debug!("app: navigating to home");
            let _handle = router.navigate_spawn(initial_route);
        }

        let handle = self.runtime_handle()?;
        let mut renderer = self.renderer.clone();

        let width = self.width;
        let height = self.height;
        let x = self.x;
        let y = self.y;
        let background = self.background;
        let title = self.title.clone();
        let description = self.description.clone();
        let viewport = self.viewport.clone();

        handle.spawn({
            let renderer = renderer.clone();
            async move {
                log::debug!("app_native_lib::start: router listening");
                #[allow(unused_variables, clippy::never_loop)]
                while let Some(content) = router.wait_for_navigation().await {
                    log::debug!("app_native_lib::start: router received content");
                    match content {
                        hyperchad_renderer::Content::View(boxed_view) => {
                            renderer.render(*boxed_view).await?;
                        }
                        hyperchad_renderer::Content::Raw { .. } => {
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

        log::debug!("app: initialing renderer");

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

        log::debug!("app: to_runner");

        Ok(renderer.to_runner(handle)?)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use hyperchad_renderer::Color;
    use hyperchad_router::Router;
    use pretty_assertions::assert_eq;

    #[test_log::test]
    fn test_app_builder_new() {
        let builder = AppBuilder::new();
        assert!(builder.router.is_none());
        assert!(builder.initial_route.is_none());
        assert!(builder.x.is_none());
        assert!(builder.y.is_none());
        assert!(builder.background.is_none());
        assert!(builder.title.is_none());
        assert!(builder.description.is_none());
        assert!(builder.viewport.is_none());
        assert!(builder.width.is_none());
        assert!(builder.height.is_none());
        assert!(builder.runtime_handle.is_none());
    }

    #[test_log::test]
    fn test_app_builder_default() {
        let builder = AppBuilder::default();
        assert!(builder.router.is_none());
    }

    #[test_log::test]
    fn test_app_builder_with_router() {
        let router = Router::new();
        let builder = AppBuilder::new().with_router(router);
        assert!(builder.router.is_some());
    }

    #[test_log::test]
    fn test_app_builder_router_method() {
        let router = Router::new();
        let mut builder = AppBuilder::new();
        builder.router(router);
        assert!(builder.router.is_some());
    }

    #[test_log::test]
    fn test_app_builder_with_initial_route() {
        let builder = AppBuilder::new().with_initial_route("/home");
        assert!(builder.initial_route.is_some());
    }

    #[test_log::test]
    fn test_app_builder_initial_route_method() {
        let mut builder = AppBuilder::new();
        builder.initial_route("/home");
        assert!(builder.initial_route.is_some());
    }

    #[test_log::test]
    fn test_app_builder_with_width() {
        let builder = AppBuilder::new().with_width(1024.0);
        assert_eq!(builder.width, Some(1024.0));
    }

    #[test_log::test]
    fn test_app_builder_width_method() {
        let mut builder = AppBuilder::new();
        builder.width(1024.0);
        assert_eq!(builder.width, Some(1024.0));
    }

    #[test_log::test]
    fn test_app_builder_with_height() {
        let builder = AppBuilder::new().with_height(768.0);
        assert_eq!(builder.height, Some(768.0));
    }

    #[test_log::test]
    fn test_app_builder_height_method() {
        let mut builder = AppBuilder::new();
        builder.height(768.0);
        assert_eq!(builder.height, Some(768.0));
    }

    #[test_log::test]
    fn test_app_builder_with_size() {
        let builder = AppBuilder::new().with_size(1920.0, 1080.0);
        assert_eq!(builder.width, Some(1920.0));
        assert_eq!(builder.height, Some(1080.0));
    }

    #[test_log::test]
    fn test_app_builder_size_method() {
        let mut builder = AppBuilder::new();
        builder.size(1920.0, 1080.0);
        assert_eq!(builder.width, Some(1920.0));
        assert_eq!(builder.height, Some(1080.0));
    }

    #[test_log::test]
    fn test_app_builder_with_x() {
        let builder = AppBuilder::new().with_x(100);
        assert_eq!(builder.x, Some(100));
    }

    #[test_log::test]
    fn test_app_builder_x_method() {
        let mut builder = AppBuilder::new();
        builder.x(100);
        assert_eq!(builder.x, Some(100));
    }

    #[test_log::test]
    fn test_app_builder_with_y() {
        let builder = AppBuilder::new().with_y(200);
        assert_eq!(builder.y, Some(200));
    }

    #[test_log::test]
    fn test_app_builder_y_method() {
        let mut builder = AppBuilder::new();
        builder.y(200);
        assert_eq!(builder.y, Some(200));
    }

    #[test_log::test]
    fn test_app_builder_with_position() {
        let builder = AppBuilder::new().with_position(300, 400);
        assert_eq!(builder.x, Some(300));
        assert_eq!(builder.y, Some(400));
    }

    #[test_log::test]
    fn test_app_builder_position_method() {
        let mut builder = AppBuilder::new();
        builder.position(300, 400);
        assert_eq!(builder.x, Some(300));
        assert_eq!(builder.y, Some(400));
    }

    #[test_log::test]
    fn test_app_builder_with_viewport() {
        let viewport = "width=device-width, initial-scale=1.0".to_string();
        let builder = AppBuilder::new().with_viewport(viewport.clone());
        assert_eq!(builder.viewport, Some(viewport));
    }

    #[test_log::test]
    fn test_app_builder_with_background() {
        let color = Color::from_hex("#ffffff");
        let builder = AppBuilder::new().with_background(color);
        assert_eq!(builder.background, Some(color));
    }

    #[test_log::test]
    fn test_app_builder_with_title() {
        let title = "Test App".to_string();
        let builder = AppBuilder::new().with_title(title.clone());
        assert_eq!(builder.title, Some(title));
    }

    #[test_log::test]
    fn test_app_builder_with_description() {
        let description = "A test application".to_string();
        let builder = AppBuilder::new().with_description(description.clone());
        assert_eq!(builder.description, Some(description));
    }

    #[test_log::test]
    fn test_app_builder_chaining() {
        let router = Router::new();
        let builder = AppBuilder::new()
            .with_router(router)
            .with_size(1024.0, 768.0)
            .with_position(100, 200)
            .with_title("Test".to_string())
            .with_description("Test app".to_string());

        assert!(builder.router.is_some());
        assert_eq!(builder.width, Some(1024.0));
        assert_eq!(builder.height, Some(768.0));
        assert_eq!(builder.x, Some(100));
        assert_eq!(builder.y, Some(200));
        assert_eq!(builder.title, Some("Test".to_string()));
        assert_eq!(builder.description, Some("Test app".to_string()));
    }

    #[test_log::test]
    fn test_app_builder_build_missing_router() {
        use crate::renderer::stub::StubRenderer;

        let builder = AppBuilder::new();
        let result = builder.build(StubRenderer);

        assert!(result.is_err());
        match result {
            Err(BuilderError::MissingRouter) => (),
            _ => panic!("Expected BuilderError::MissingRouter"),
        }
    }

    #[test_log::test]
    #[allow(clippy::float_cmp)]
    fn test_app_builder_build_success() {
        use crate::renderer::stub::StubRenderer;

        let router = Router::new();
        let builder = AppBuilder::new()
            .with_router(router)
            .with_size(800.0, 600.0)
            .with_position(50, 100);

        let result = builder.build(StubRenderer);
        assert!(result.is_ok());

        let app = result.unwrap();
        assert_eq!(app.width, 800.0);
        assert_eq!(app.height, 600.0);
        assert_eq!(app.x, Some(50));
        assert_eq!(app.y, Some(100));
    }

    #[test_log::test]
    #[allow(clippy::float_cmp)]
    fn test_app_builder_build_default_dimensions() {
        use crate::renderer::stub::StubRenderer;

        let router = Router::new();
        let builder = AppBuilder::new().with_router(router);

        let result = builder.build(StubRenderer);
        assert!(result.is_ok());

        let app = result.unwrap();
        assert_eq!(app.width, 800.0);
        assert_eq!(app.height, 600.0);
    }

    #[test_log::test]
    fn test_commands_equality() {
        assert_eq!(Commands::DynamicRoutes, Commands::DynamicRoutes);
        assert_eq!(
            Commands::Gen {
                output: Some("output".to_string())
            },
            Commands::Gen {
                output: Some("output".to_string())
            }
        );
        assert_eq!(
            Commands::Clean {
                output: Some("output".to_string())
            },
            Commands::Clean {
                output: Some("output".to_string())
            }
        );
        assert_eq!(Commands::Serve, Commands::Serve);
    }

    #[test_log::test]
    fn test_commands_inequality() {
        assert_ne!(Commands::DynamicRoutes, Commands::Serve);
        assert_ne!(
            Commands::Gen {
                output: Some("output1".to_string())
            },
            Commands::Gen {
                output: Some("output2".to_string())
            }
        );
        assert_ne!(
            Commands::Clean {
                output: Some("output1".to_string())
            },
            Commands::Clean { output: None }
        );
    }

    #[test_log::test]
    fn test_commands_clone() {
        let cmd = Commands::Gen {
            output: Some("test".to_string()),
        };
        let cloned = cmd.clone();
        assert_eq!(cmd, cloned);
    }

    #[test_log::test]
    fn test_builder_error_display() {
        let error = BuilderError::MissingRouter;
        assert_eq!(error.to_string(), "Missing Router");

        let error = BuilderError::MissingRuntime;
        assert_eq!(error.to_string(), "Missing Runtime");
    }

    #[test_log::test]
    fn test_error_from_builder_error() {
        let builder_error = BuilderError::MissingRouter;
        let error: Error = builder_error.into();

        match error {
            Error::Builder(BuilderError::MissingRouter) => (),
            _ => panic!("Expected Error::Builder(MissingRouter)"),
        }
    }

    #[test_log::test]
    fn test_error_from_io_error() {
        let io_error = std::io::Error::new(std::io::ErrorKind::NotFound, "File not found");
        let error: Error = io_error.into();

        match error {
            Error::IO(_) => (),
            _ => panic!("Expected Error::IO"),
        }
    }

    #[cfg(feature = "html")]
    #[test_log::test]
    fn test_app_builder_with_css_url() {
        let url = "https://example.com/style.css".to_string();
        let builder = AppBuilder::new().with_css_url(url.clone());
        assert_eq!(builder.css_urls, vec![url]);
    }

    #[cfg(feature = "html")]
    #[test_log::test]
    fn test_app_builder_css_url_method() {
        let url = "https://example.com/style.css".to_string();
        let mut builder = AppBuilder::new();
        builder.css_url(url.clone());
        assert_eq!(builder.css_urls, vec![url]);
    }

    #[cfg(feature = "html")]
    #[test_log::test]
    fn test_app_builder_with_css_urls() {
        let urls = vec![
            "https://example.com/style1.css".to_string(),
            "https://example.com/style2.css".to_string(),
        ];
        let builder = AppBuilder::new().with_css_urls(urls.clone());
        assert_eq!(builder.css_urls, urls);
    }

    #[cfg(feature = "html")]
    #[test_log::test]
    fn test_app_builder_with_css_path() {
        let path = "/assets/style.css".to_string();
        let builder = AppBuilder::new().with_css_path(path.clone());
        assert_eq!(builder.css_paths, vec![path]);
    }

    #[cfg(feature = "html")]
    #[test_log::test]
    fn test_app_builder_css_path_method() {
        let path = "/assets/style.css".to_string();
        let mut builder = AppBuilder::new();
        builder.css_path(path.clone());
        assert_eq!(builder.css_paths, vec![path]);
    }

    #[cfg(feature = "html")]
    #[test_log::test]
    fn test_app_builder_with_css_paths() {
        let paths = vec![
            "/assets/style1.css".to_string(),
            "/assets/style2.css".to_string(),
        ];
        let builder = AppBuilder::new().with_css_paths(paths.clone());
        assert_eq!(builder.css_paths, paths);
    }

    #[cfg(feature = "html")]
    #[test_log::test]
    fn test_app_builder_with_inline_css() {
        let css = "body { margin: 0; }".to_string();
        let builder = AppBuilder::new().with_inline_css(css.clone());
        assert_eq!(builder.inline_css, vec![css]);
    }

    #[cfg(feature = "html")]
    #[test_log::test]
    fn test_app_builder_inline_css_method() {
        let css = "body { margin: 0; }".to_string();
        let mut builder = AppBuilder::new();
        builder.inline_css(css.clone());
        assert_eq!(builder.inline_css, vec![css]);
    }

    #[cfg(feature = "html")]
    #[test_log::test]
    fn test_app_builder_with_inline_css_blocks() {
        let blocks = vec![
            "body { margin: 0; }".to_string(),
            "h1 { color: red; }".to_string(),
        ];
        let builder = AppBuilder::new().with_inline_css_blocks(blocks.clone());
        assert_eq!(builder.inline_css, blocks);
    }

    #[test_log::test]
    fn test_app_builder_debug_format() {
        let router = Router::new();
        let builder = AppBuilder::new()
            .with_router(router)
            .with_size(800.0, 600.0)
            .with_title("Test".to_string());

        let debug_str = format!("{builder:?}");
        assert!(debug_str.contains("AppBuilder"));
        assert!(debug_str.contains("router"));
        assert!(debug_str.contains("width"));
        assert!(debug_str.contains("height"));
        assert!(debug_str.contains("title"));
    }

    #[test_log::test]
    fn test_app_builder_clone() {
        let router = Router::new();
        let builder = AppBuilder::new()
            .with_router(router)
            .with_size(1024.0, 768.0);

        let cloned = builder.clone();
        assert_eq!(builder.width, cloned.width);
        assert_eq!(builder.height, cloned.height);
    }

    #[test_log::test]
    fn test_app_builder_build_default_stub() {
        use crate::renderer::stub::StubRenderer;

        let router = Router::new();
        let result = AppBuilder::new().with_router(router).build_default_stub();
        assert!(result.is_ok());

        let app = result.unwrap();
        assert!(matches!(app.renderer, StubRenderer));
    }

    #[test_log::test]
    fn test_app_builder_build_stub() {
        use crate::renderer::stub::StubRenderer;

        let router = Router::new();
        let result = AppBuilder::new()
            .with_router(router)
            .build_stub(StubRenderer);
        assert!(result.is_ok());
    }

    #[test_log::test]
    fn test_stub_runner_run() {
        use crate::renderer::stub::StubRunner;
        use hyperchad_renderer::RenderRunner;

        let mut runner = StubRunner;
        let result = runner.run();
        assert!(result.is_ok());
    }

    #[test_log::test]
    fn test_app_builder_with_runtime_handle() {
        let router = Router::new();
        let runtime = switchy::unsync::runtime::Builder::new()
            .max_blocking_threads(1)
            .build()
            .expect("Failed to build runtime");
        let handle = runtime.handle();

        let builder = AppBuilder::new()
            .with_router(router)
            .with_runtime_handle(handle);

        assert!(builder.runtime_handle.is_some());
    }
}
