use crate::AppBuilder;

#[cfg(feature = "egui")]
pub type DefaultRenderer = egui::EguiRenderer;

#[cfg(all(feature = "fltk", not(feature = "egui")))]
pub type DefaultRenderer = fltk::FltkRenderer;

#[cfg(all(
    feature = "actix",
    feature = "vanilla-js",
    not(any(feature = "egui", feature = "fltk"))
))]
pub type DefaultRenderer = html::actix::vanilla_js::HtmlVanillaJsActixRenderer;

#[cfg(all(
    feature = "lambda",
    feature = "vanilla-js",
    not(any(feature = "egui", feature = "fltk", feature = "actix"))
))]
pub type DefaultRenderer = html::lambda::vanilla_js::HtmlVanillaJsLambdaRenderer;

#[cfg(all(
    feature = "actix",
    feature = "html",
    not(any(
        feature = "egui",
        feature = "fltk",
        feature = "lambda",
        feature = "vanilla-js"
    ))
))]
pub type DefaultRenderer = html::actix::HtmlActixRenderer;

#[cfg(all(
    feature = "lambda",
    feature = "html",
    not(any(
        feature = "egui",
        feature = "fltk",
        feature = "vanilla-js",
        feature = "actix"
    ))
))]
pub type DefaultRenderer = html::lambda::HtmlLambdaRenderer;

#[cfg(all(
    feature = "html",
    feature = "vanilla-js",
    not(any(
        feature = "egui",
        feature = "fltk",
        feature = "actix",
        feature = "lambda"
    ))
))]
pub type DefaultRenderer = html::vanilla_js::HtmlVanillaJsRenderer;

#[cfg(all(
    feature = "html",
    not(any(
        feature = "egui",
        feature = "fltk",
        feature = "lambda",
        feature = "vanilla-js",
        feature = "actix"
    ))
))]
pub type DefaultRenderer = html::HtmlStubRenderer;

#[cfg(not(any(
    feature = "html",
    feature = "egui",
    feature = "fltk",
    feature = "vanilla-js"
)))]
pub type DefaultRenderer = stub::StubRenderer;

#[cfg(feature = "egui")]
mod egui {
    use std::sync::Arc;

    use async_trait::async_trait;
    use hyperchad_renderer::transformer::layout::calc::{Calculator, CalculatorDefaults};
    use hyperchad_renderer_egui::{eframe::egui, font_metrics::EguiFontMetrics};
    use hyperchad_router::{DEFAULT_CLIENT_INFO, Router};

    use crate::{App, AppBuilder, BuilderError, Cleaner, Error, Generator};

    #[derive(Clone)]
    pub struct EguiCalculator(pub Option<Arc<Calculator<EguiFontMetrics>>>);

    pub type EguiRenderer = hyperchad_renderer_egui::EguiRenderer<EguiCalculator>;

    impl hyperchad_renderer::transformer::layout::Calc for EguiCalculator {
        fn calc(&self, container: &mut hyperchad_router::Container) -> bool {
            self.0.as_ref().unwrap().calc(container)
        }
    }

    impl hyperchad_renderer_egui::layout::EguiCalc for EguiCalculator {
        fn with_context(mut self, context: egui::Context) -> Self {
            const DELTA: f32 = 14.0f32 / 16.0;
            self.0 = Some(Arc::new(Calculator::new(
                EguiFontMetrics::new(context),
                CalculatorDefaults {
                    font_size: 16.0 * DELTA,
                    font_margin_top: 0.0 * DELTA,
                    font_margin_bottom: 0.0 * DELTA,
                    h1_font_size: 32.0 * DELTA,
                    h1_font_margin_top: 21.44 * DELTA,
                    h1_font_margin_bottom: 21.44 * DELTA,
                    h2_font_size: 24.0 * DELTA,
                    h2_font_margin_top: 19.92 * DELTA,
                    h2_font_margin_bottom: 19.92 * DELTA,
                    h3_font_size: 18.72 * DELTA,
                    h3_font_margin_top: 18.72 * DELTA,
                    h3_font_margin_bottom: 18.72 * DELTA,
                    h4_font_size: 16.0 * DELTA,
                    h4_font_margin_top: 21.28 * DELTA,
                    h4_font_margin_bottom: 21.28 * DELTA,
                    h5_font_size: 13.28 * DELTA,
                    h5_font_margin_top: 22.1776 * DELTA,
                    h5_font_margin_bottom: 22.1776 * DELTA,
                    h6_font_size: 10.72 * DELTA,
                    h6_font_margin_top: 24.9776 * DELTA,
                    h6_font_margin_bottom: 24.9776 * DELTA,
                },
            )));
            self
        }
    }

    #[async_trait]
    impl Generator for EguiRenderer {
        /// # Errors
        ///
        /// * Infallible
        async fn generate(&self, _router: &Router, _output: Option<String>) -> Result<(), Error> {
            Ok(())
        }
    }

    #[async_trait]
    impl Cleaner for EguiRenderer {
        /// # Errors
        ///
        /// * Infallible
        async fn clean(&self, _output: Option<String>) -> Result<(), Error> {
            Ok(())
        }
    }

    impl AppBuilder {
        /// # Errors
        ///
        /// * If the `AppBuilder` is missing a router
        pub fn build_egui(self, renderer: EguiRenderer) -> Result<App<EguiRenderer>, BuilderError> {
            log::debug!("build_egui");

            self.build(renderer)
        }

        /// # Errors
        ///
        /// * If the `AppBuilder` is missing a router
        pub fn build_default_egui(self) -> Result<App<EguiRenderer>, BuilderError> {
            log::debug!("build_default_egui");

            let action_tx = self.listen_actions(self.action_handlers.clone());
            let resize_tx = self.listen_resize(self.resize_listeners.clone());
            let router = self.router.clone().ok_or(BuilderError::MissingRouter)?;
            let calculator = EguiCalculator(None);
            let renderer = hyperchad_renderer_egui::EguiRenderer::new(
                router.clone(),
                #[cfg(feature = "logic")]
                action_tx,
                resize_tx,
                DEFAULT_CLIENT_INFO.clone(),
                calculator,
            );

            self.runtime_handle().spawn({
                let renderer = renderer.clone();
                async move {
                    while let Some(path) = renderer.wait_for_navigation().await {
                        if let Err(e) = router
                            .navigate_send((
                                &path,
                                hyperchad_router::RequestInfo {
                                    client: DEFAULT_CLIENT_INFO.clone(),
                                },
                            ))
                            .await
                        {
                            log::error!("Failed to navigate: {e:?}");
                        }
                    }
                }
            });

            self.build(renderer)
        }
    }
}

#[cfg(feature = "fltk")]
mod fltk {
    use async_trait::async_trait;
    use hyperchad_router::{DEFAULT_CLIENT_INFO, Router};

    use crate::{App, AppBuilder, BuilderError, Cleaner, Error, Generator};

    pub type FltkRenderer = hyperchad_renderer_fltk::FltkRenderer;

    #[async_trait]
    impl Generator for FltkRenderer {
        /// # Errors
        ///
        /// * Infallible
        async fn generate(&self, _router: &Router, _output: Option<String>) -> Result<(), Error> {
            Ok(())
        }
    }

    #[async_trait]
    impl Cleaner for FltkRenderer {
        /// # Errors
        ///
        /// * Infallible
        async fn clean(&self, _output: Option<String>) -> Result<(), Error> {
            Ok(())
        }
    }

    impl AppBuilder {
        /// # Errors
        ///
        /// * If the `AppBuilder` is missing a router
        pub fn build_fltk(self, renderer: FltkRenderer) -> Result<App<FltkRenderer>, BuilderError> {
            log::debug!("build_fltk");

            self.build(renderer)
        }

        /// # Errors
        ///
        /// * If the `AppBuilder` is missing a router
        pub fn build_default_fltk(self) -> Result<App<FltkRenderer>, BuilderError> {
            log::debug!("build_default_fltk");

            let action_tx = self.listen_actions(self.action_handlers.clone());
            let router = self.router.clone().ok_or(BuilderError::MissingRouter)?;
            let renderer = hyperchad_renderer_fltk::FltkRenderer::new(action_tx);

            self.runtime_handle().spawn({
                let renderer = renderer.clone();
                async move {
                    while let Some(path) = renderer.wait_for_navigation().await {
                        if let Err(e) = router
                            .navigate_send((
                                &path,
                                hyperchad_router::RequestInfo {
                                    client: DEFAULT_CLIENT_INFO.clone(),
                                },
                            ))
                            .await
                        {
                            log::error!("Failed to navigate: {e:?}");
                        }
                    }
                }
            });

            self.build(renderer)
        }
    }
}

#[cfg(feature = "html")]
pub mod html {
    use std::{path::PathBuf, sync::LazyLock};

    use async_trait::async_trait;
    use hyperchad_renderer::ToRenderRunner;
    use hyperchad_renderer_html::{HtmlApp, HtmlRenderer};
    use hyperchad_router::Router;

    use crate::{App, AppBuilder, BuilderError};
    use crate::{Cleaner, Error, Generator};

    pub type HtmlStubRenderer = hyperchad_renderer_html::HtmlRenderer<
        hyperchad_renderer_html::stub::StubApp<hyperchad_renderer_html::DefaultHtmlTagRenderer>,
    >;

    static DEFAULT_OUTPUT_DIR: &str = "gen";
    static CARGO_MANIFEST_DIR: LazyLock<Option<std::path::PathBuf>> =
        LazyLock::new(|| std::env::var("CARGO_MANIFEST_DIR").ok().map(Into::into));

    #[async_trait]
    impl<T: HtmlApp + ToRenderRunner + Send + Sync> Generator for HtmlRenderer<T> {
        #[cfg(not(feature = "static-routes"))]
        async fn generate(&self, _router: &Router, _output: Option<String>) -> Result<(), Error> {
            panic!("Must have `static-routes` enabled to generate");
        }

        #[allow(clippy::too_many_lines)]
        #[cfg(feature = "static-routes")]
        async fn generate(&self, router: &Router, output: Option<String>) -> Result<(), Error> {
            use std::io::Write as _;

            use hyperchad_renderer::{Color, Content, PartialView, View};
            use hyperchad_renderer_html::html::container_element_to_html_response;
            use hyperchad_router::{ClientInfo, ClientOs, RequestInfo, RoutePath, RouteRequest};

            static BACKGROUND_COLOR: LazyLock<Color> = LazyLock::new(|| Color::from_hex("#181a1b"));
            static VIEWPORT: LazyLock<String> = LazyLock::new(|| "width=device-width".to_string());

            log::debug!("generate: output={output:?}");

            let output = output.unwrap_or_else(|| {
                CARGO_MANIFEST_DIR
                    .as_ref()
                    .and_then(|x| x.join(DEFAULT_OUTPUT_DIR).to_str().map(ToString::to_string))
                    .unwrap_or_else(|| DEFAULT_OUTPUT_DIR.to_string())
            });
            let output_path: PathBuf = output.into();
            let static_routes = router.static_routes.read().unwrap().clone();

            if static_routes.is_empty() {
                log::debug!("generate: no static routes");
            }

            if output_path.is_dir() {
                std::fs::remove_dir_all(&output_path)?;
            }

            for (path, handler) in &static_routes {
                let path_str = match path {
                    RoutePath::Literal(path) => path,
                    RoutePath::Literals(paths) => {
                        if let Some(path) = paths.first() {
                            path
                        } else {
                            continue;
                        }
                    }
                    RoutePath::LiteralPrefix(..) => continue,
                };
                let path_str = path_str.strip_prefix('/').unwrap_or(path_str);
                let path_str = if path_str.is_empty() {
                    "index"
                } else {
                    path_str
                };

                let req = RouteRequest {
                    path: path_str.to_string(),
                    method: switchy::http::models::Method::Get,
                    query: std::collections::BTreeMap::new(),
                    headers: std::collections::BTreeMap::new(),
                    cookies: std::collections::BTreeMap::new(),
                    info: RequestInfo {
                        client: std::sync::Arc::new(ClientInfo {
                            os: ClientOs {
                                name: "n/a".to_string(),
                            },
                        }),
                    },
                    body: None,
                };

                match handler(req).await {
                    Ok(content) => {
                        let Some(content) = content else {
                            continue;
                        };
                        let output_path = output_path.join(format!("{path_str}.html"));
                        std::fs::create_dir_all(output_path.parent().unwrap())?;

                        log::debug!("generate: path={path_str} -> {}", output_path.display());

                        let mut file = std::fs::File::options()
                            .truncate(true)
                            .write(true)
                            .create(true)
                            .open(&output_path)?;

                        match content {
                            Content::View(View {
                                immediate: view, ..
                            })
                            | Content::PartialView(PartialView {
                                container: view, ..
                            }) => {
                                let html = container_element_to_html_response(
                                    &std::collections::BTreeMap::new(),
                                    &view,
                                    Some(&*VIEWPORT),
                                    Some(*BACKGROUND_COLOR),
                                    Some("MoosicBox"),
                                    Some("MoosicBox: A music app for cows"),
                                    self.app.tag_renderer(),
                                )?;

                                log::debug!(
                                    "generate: path={path_str} -> {}:\n{html}",
                                    output_path.display()
                                );

                                file.write_all(html.as_bytes())
                                    .expect("Failed to write file");
                            }
                            Content::Raw { data, .. } => {
                                log::debug!(
                                    "generate: path={path_str} -> {}:\n{data:?}",
                                    output_path.display()
                                );

                                file.write_all(&data).expect("Failed to write file");
                            }
                            #[cfg(feature = "json")]
                            Content::Json(value) => {
                                log::debug!(
                                    "generate: path={path_str} -> {}:\n{value}",
                                    output_path.display()
                                );

                                file.write_all(
                                    serde_json::to_string(&value)
                                        .expect("Failed to stringify JSON")
                                        .as_bytes(),
                                )
                                .expect("Failed to write file");
                            }
                        }
                    }
                    Err(e) => {
                        panic!("Failed to process route: {e:?}");
                    }
                }
            }

            #[cfg(feature = "assets")]
            {
                use hyperchad_renderer::assets::AssetPathTarget;

                use std::path::Path;

                fn copy_dir_all(src: &Path, dst: &Path) -> std::io::Result<()> {
                    std::fs::create_dir_all(dst)?;
                    let mut entries: Vec<_> =
                        std::fs::read_dir(src)?.filter_map(Result::ok).collect();

                    // Sort entries for deterministic processing
                    entries.sort_by_key(std::fs::DirEntry::file_name);

                    for entry in entries {
                        let ty = entry.file_type()?;
                        if ty.is_dir() {
                            copy_dir_all(&entry.path(), &dst.join(entry.file_name()))?;
                        } else {
                            std::fs::copy(entry.path(), dst.join(entry.file_name()))?;
                        }
                    }
                    Ok(())
                }

                for route in self.static_asset_routes() {
                    let route_path = route.route.strip_prefix('/').unwrap_or(&route.route);
                    let assets_output = output_path.join(route_path);
                    log::debug!("generate: asset {}", assets_output.display());
                    std::fs::create_dir_all(assets_output.parent().unwrap())
                        .expect("Failed to create dirs");
                    match &route.target {
                        AssetPathTarget::File(file) => {
                            std::fs::copy(file, &assets_output)?;
                        }
                        AssetPathTarget::FileContents(contents) => {
                            let mut file = std::fs::File::options()
                                .truncate(true)
                                .write(true)
                                .create(true)
                                .open(&assets_output)
                                .expect("Failed to open file");

                            file.write_all(contents).expect("Failed to write file");
                        }
                        AssetPathTarget::Directory(dir) => {
                            copy_dir_all(dir, &assets_output)?;
                        }
                    }
                }
            }

            Ok(())
        }
    }

    #[async_trait]
    impl<T: HtmlApp + ToRenderRunner + Send + Sync> Cleaner for HtmlRenderer<T> {
        /// # Errors
        ///
        /// * If the `App` fails to clean the output directory
        async fn clean(&self, output: Option<String>) -> Result<(), Error> {
            let output = output.unwrap_or_else(|| {
                CARGO_MANIFEST_DIR
                    .as_ref()
                    .and_then(|x| x.join(DEFAULT_OUTPUT_DIR).to_str().map(ToString::to_string))
                    .unwrap_or_else(|| DEFAULT_OUTPUT_DIR.to_string())
            });
            let output_path: PathBuf = output.into();

            if output_path.is_dir() {
                std::fs::remove_dir_all(&output_path)?;
            }

            Ok(())
        }
    }

    impl AppBuilder {
        /// # Errors
        ///
        /// * If the `AppBuilder` is missing a router
        pub fn build_html(
            self,
            renderer: HtmlStubRenderer,
        ) -> Result<App<HtmlStubRenderer>, BuilderError> {
            log::debug!("build_html");

            self.build(renderer)
        }

        /// # Errors
        ///
        /// * If the `AppBuilder` is missing a router
        pub fn build_default_html(self) -> Result<App<HtmlStubRenderer>, BuilderError> {
            log::debug!("build_default_html");
            log::debug!(
                "this returns a stub app. to get a real app, use a proper backend such as build_default_html_vanilla_js_actix or build_default_html_vanilla_js_lambda, or enable the actix/lambda backend features"
            );

            let renderer = hyperchad_renderer_html::HtmlRenderer::new(
                hyperchad_renderer_html::stub::StubApp::new(
                    hyperchad_renderer_html::DefaultHtmlTagRenderer::default(),
                ),
            );

            self.build(renderer)
        }
    }

    #[cfg(feature = "actix")]
    pub mod actix {
        use crate::{App, AppBuilder, BuilderError};

        pub type HtmlActixRenderer = hyperchad_renderer_html::HtmlRenderer<
            hyperchad_renderer_html::actix::ActixApp<
                hyperchad_renderer_html::actix::PreparedRequest,
                hyperchad_renderer_html::actix::HtmlActixResponseProcessor<
                    hyperchad_renderer_html::DefaultHtmlTagRenderer,
                >,
            >,
        >;

        impl AppBuilder {
            /// # Errors
            ///
            /// * If the `AppBuilder` is missing a router
            pub fn build_html_actix(
                self,
                renderer: HtmlActixRenderer,
            ) -> Result<App<HtmlActixRenderer>, BuilderError> {
                log::debug!("build_html_actix");

                self.build(renderer)
            }

            /// # Errors
            ///
            /// * If the `AppBuilder` is missing a router
            pub fn build_default_html_actix(self) -> Result<App<HtmlActixRenderer>, BuilderError> {
                log::debug!("build_default_html_actix");

                let router = self.router.clone().ok_or(BuilderError::MissingRouter)?;

                let renderer = hyperchad_renderer_html::router_to_actix(
                    hyperchad_renderer_html::DefaultHtmlTagRenderer::default(),
                    router,
                );

                #[cfg(feature = "assets")]
                let renderer = renderer.with_static_asset_routes(self.static_asset_routes.clone());

                self.build(renderer)
            }
        }

        #[cfg(feature = "vanilla-js")]
        pub mod vanilla_js {
            use crate::{App, AppBuilder, BuilderError};

            pub type HtmlVanillaJsActixRenderer = hyperchad_renderer_html::HtmlRenderer<
                hyperchad_renderer_html::actix::ActixApp<
                    hyperchad_renderer_html::actix::PreparedRequest,
                    hyperchad_renderer_html::actix::HtmlActixResponseProcessor<
                        hyperchad_renderer_vanilla_js::VanillaJsTagRenderer,
                    >,
                >,
            >;

            impl AppBuilder {
                /// # Errors
                ///
                /// * If the `AppBuilder` is missing a router
                pub fn build_html_vanilla_js_actix(
                    self,
                    renderer: HtmlVanillaJsActixRenderer,
                ) -> Result<App<HtmlVanillaJsActixRenderer>, BuilderError> {
                    log::debug!("build_html_vanilla_js_actix");

                    self.build(renderer)
                }

                /// # Errors
                ///
                /// * If the `AppBuilder` is missing a router
                pub fn build_default_html_vanilla_js_actix(
                    self,
                ) -> Result<App<HtmlVanillaJsActixRenderer>, BuilderError> {
                    log::debug!("build_default_html_vanilla_js_actix");

                    let router = self.router.clone().ok_or(BuilderError::MissingRouter)?;

                    #[allow(unused_mut)]
                    let mut renderer = hyperchad_renderer_html::router_to_actix(
                        hyperchad_renderer_vanilla_js::VanillaJsTagRenderer::default(),
                        router,
                    )
                    .with_extend_html_renderer(hyperchad_renderer_vanilla_js::VanillaJsRenderer {});

                    #[cfg(feature = "actions")]
                    {
                        let action_tx = self.listen_actions(self.action_handlers.clone());
                        renderer.app.set_action_tx(action_tx);
                    }

                    #[cfg(feature = "assets")]
                    let renderer =
                        renderer.with_static_asset_routes(self.static_asset_routes.clone());

                    self.build(renderer)
                }
            }
        }
    }

    #[cfg(feature = "lambda")]
    pub mod lambda {
        use crate::{App, AppBuilder, BuilderError};

        pub type HtmlLambdaRenderer = hyperchad_renderer_html::HtmlRenderer<
            hyperchad_renderer_html::lambda::LambdaApp<
                hyperchad_renderer_html::lambda::PreparedRequest,
                hyperchad_renderer_html::lambda::HtmlLambdaResponseProcessor<
                    hyperchad_renderer_html::DefaultHtmlTagRenderer,
                >,
            >,
        >;

        impl AppBuilder {
            /// # Errors
            ///
            /// * If the `AppBuilder` is missing a router
            pub fn build_html_lambda(
                self,
                renderer: HtmlLambdaRenderer,
            ) -> Result<App<HtmlLambdaRenderer>, BuilderError> {
                self.build(renderer)
            }

            /// # Errors
            ///
            /// * If the `AppBuilder` is missing a router
            pub fn build_default_html_lambda(
                self,
            ) -> Result<App<HtmlLambdaRenderer>, BuilderError> {
                log::debug!("build_default_html_lambda");

                let router = self.router.clone().ok_or(BuilderError::MissingRouter)?;

                let renderer = hyperchad_renderer_html::router_to_lambda(
                    hyperchad_renderer_html::DefaultHtmlTagRenderer::default(),
                    router,
                );

                #[cfg(feature = "assets")]
                let renderer = renderer.with_static_asset_routes(self.static_asset_routes.clone());

                self.build(renderer)
            }
        }

        #[cfg(feature = "vanilla-js")]
        pub mod vanilla_js {
            use crate::{App, AppBuilder, BuilderError};

            pub type HtmlVanillaJsLambdaRenderer = hyperchad_renderer_html::HtmlRenderer<
                hyperchad_renderer_html::lambda::LambdaApp<
                    hyperchad_renderer_html::lambda::PreparedRequest,
                    hyperchad_renderer_html::lambda::HtmlLambdaResponseProcessor<
                        hyperchad_renderer_vanilla_js::VanillaJsTagRenderer,
                    >,
                >,
            >;

            impl AppBuilder {
                /// # Errors
                ///
                /// * If the `AppBuilder` is missing a router
                pub fn build_html_vanilla_js_lambda(
                    self,
                    renderer: HtmlVanillaJsLambdaRenderer,
                ) -> Result<App<HtmlVanillaJsLambdaRenderer>, BuilderError> {
                    log::debug!("build_html_vanilla_js_lambda");

                    self.build(renderer)
                }

                /// # Errors
                ///
                /// * If the `AppBuilder` is missing a router
                pub fn build_default_html_vanilla_js_lambda(
                    self,
                ) -> Result<App<HtmlVanillaJsLambdaRenderer>, BuilderError> {
                    log::debug!("build_default_html_vanilla_js_lambda");

                    let router = self.router.clone().ok_or(BuilderError::MissingRouter)?;

                    #[allow(unused_mut)]
                    let mut renderer = hyperchad_renderer_html::router_to_lambda(
                        hyperchad_renderer_vanilla_js::VanillaJsTagRenderer::default(),
                        router,
                    )
                    .with_extend_html_renderer(hyperchad_renderer_vanilla_js::VanillaJsRenderer {});

                    #[cfg(feature = "assets")]
                    let renderer =
                        renderer.with_static_asset_routes(self.static_asset_routes.clone());

                    self.build(renderer)
                }
            }
        }
    }

    #[cfg(feature = "vanilla-js")]
    pub mod vanilla_js {
        use crate::{App, AppBuilder, BuilderError};

        pub type HtmlVanillaJsRenderer = hyperchad_renderer_html::HtmlRenderer<
            hyperchad_renderer_html::stub::StubApp<
                hyperchad_renderer_vanilla_js::VanillaJsTagRenderer,
            >,
        >;

        impl AppBuilder {
            /// # Errors
            ///
            /// * If the `AppBuilder` is missing a router
            pub fn build_html_vanilla_js(
                self,
                renderer: HtmlVanillaJsRenderer,
            ) -> Result<App<HtmlVanillaJsRenderer>, BuilderError> {
                log::debug!("build_html_vanilla_js");

                self.build(renderer)
            }

            /// # Errors
            ///
            /// * If the `AppBuilder` is missing a router
            pub fn build_default_html_vanilla_js(
                self,
            ) -> Result<App<HtmlVanillaJsRenderer>, BuilderError> {
                log::debug!("build_default_html_vanilla_js");
                log::debug!(
                    "this returns a stub app. to get a real app, use a proper backend such as build_default_html_vanilla_js_actix or build_default_html_vanilla_js_lambda, or enable the actix/lambda backend features"
                );

                let renderer = hyperchad_renderer_html::HtmlRenderer::new(
                    hyperchad_renderer_html::stub::StubApp::new(
                        hyperchad_renderer_vanilla_js::VanillaJsTagRenderer::default(),
                    ),
                );

                #[cfg(feature = "assets")]
                let renderer = renderer.with_static_asset_routes(self.static_asset_routes.clone());

                self.build(renderer)
            }
        }
    }
}

pub mod stub {
    use async_trait::async_trait;
    use hyperchad_renderer::{
        Color, Handle, PartialView, RenderRunner, Renderer, ToRenderRunner, View,
        transformer::ResponsiveTrigger,
    };

    use crate::{App, AppBuilder, BuilderError, Cleaner, Error, Generator};

    #[derive(Debug, Clone)]
    pub struct StubRenderer;

    #[async_trait]
    impl Renderer for StubRenderer {
        /// # Errors
        ///
        /// Will error if `Renderer` implementation app fails to start
        #[allow(clippy::too_many_arguments)]
        async fn init(
            &mut self,
            _width: f32,
            _height: f32,
            _x: Option<i32>,
            _y: Option<i32>,
            _background: Option<Color>,
            _title: Option<&str>,
            _description: Option<&str>,
            _viewport: Option<&str>,
        ) -> Result<(), Box<dyn std::error::Error + Send + 'static>> {
            Ok(())
        }

        fn add_responsive_trigger(&mut self, _name: String, _trigger: ResponsiveTrigger) {}

        /// # Errors
        ///
        /// Will error if `Renderer` implementation fails to emit the event.
        async fn emit_event(
            &self,
            _event_name: String,
            _event_value: Option<String>,
        ) -> Result<(), Box<dyn std::error::Error + Send + 'static>> {
            Ok(())
        }

        /// # Errors
        ///
        /// Will error if `Renderer` implementation fails to render the view.
        async fn render(
            &self,
            _view: View,
        ) -> Result<(), Box<dyn std::error::Error + Send + 'static>> {
            Ok(())
        }

        /// # Errors
        ///
        /// Will error if `Renderer` implementation fails to render the partial elements.
        async fn render_partial(
            &self,
            _partial: PartialView,
        ) -> Result<(), Box<dyn std::error::Error + Send + 'static>> {
            Ok(())
        }
    }

    #[async_trait]
    impl Generator for StubRenderer {
        async fn generate(
            &self,
            _router: &hyperchad_router::Router,
            _output: Option<String>,
        ) -> Result<(), Error> {
            Ok(())
        }
    }

    #[async_trait]
    impl Cleaner for StubRenderer {
        async fn clean(&self, _output: Option<String>) -> Result<(), Error> {
            Ok(())
        }
    }

    pub struct StubRunner;

    impl RenderRunner for StubRunner {
        fn run(&mut self) -> Result<(), Box<dyn std::error::Error + Send + 'static>> {
            Ok(())
        }
    }

    impl ToRenderRunner for StubRenderer {
        /// # Errors
        ///
        /// Will error if `Renderer` implementation fails to run
        fn to_runner(
            self,
            _handle: Handle,
        ) -> Result<Box<dyn hyperchad_renderer::RenderRunner>, Box<dyn std::error::Error + Send>>
        {
            Ok(Box::new(StubRunner))
        }
    }

    impl AppBuilder {
        /// # Errors
        ///
        /// * If the `AppBuilder` is missing a router
        pub fn build_stub(self, renderer: StubRenderer) -> Result<App<StubRenderer>, BuilderError> {
            log::debug!("build_stub");

            self.build(renderer)
        }

        /// # Errors
        ///
        /// * If the `AppBuilder` is missing a router
        pub fn build_default_stub(self) -> Result<App<StubRenderer>, BuilderError> {
            log::debug!("build_default_stub");

            let renderer = StubRenderer;

            self.build(renderer)
        }
    }
}

impl AppBuilder {
    /// # Errors
    ///
    /// * If fails to build the default renderer
    #[cfg(feature = "egui")]
    pub fn build_default(self) -> Result<crate::App<DefaultRenderer>, crate::BuilderError> {
        self.build_default_egui()
    }

    /// # Errors
    ///
    /// * If fails to build the default renderer
    #[cfg(all(feature = "fltk", not(feature = "egui")))]
    pub fn build_default(self) -> Result<crate::App<DefaultRenderer>, crate::BuilderError> {
        self.build_default_fltk()
    }

    /// # Errors
    ///
    /// * If fails to build the default renderer
    #[cfg(all(
        feature = "actix",
        feature = "vanilla-js",
        not(any(feature = "egui", feature = "fltk"))
    ))]
    pub fn build_default(self) -> Result<crate::App<DefaultRenderer>, crate::BuilderError> {
        self.build_default_html_vanilla_js_actix()
    }

    /// # Errors
    ///
    /// * If fails to build the default renderer
    #[cfg(all(
        feature = "lambda",
        feature = "vanilla-js",
        not(any(feature = "egui", feature = "fltk", feature = "actix"))
    ))]
    pub fn build_default(self) -> Result<crate::App<DefaultRenderer>, crate::BuilderError> {
        self.build_default_html_vanilla_js_lambda()
    }

    /// # Errors
    ///
    /// * If fails to build the default renderer
    #[cfg(all(
        feature = "actix",
        feature = "html",
        not(any(
            feature = "egui",
            feature = "fltk",
            feature = "lambda",
            feature = "vanilla-js"
        ))
    ))]
    pub fn build_default(self) -> Result<crate::App<DefaultRenderer>, crate::BuilderError> {
        self.build_default_html_actix()
    }

    /// # Errors
    ///
    /// * If fails to build the default renderer
    #[cfg(all(
        feature = "lambda",
        feature = "html",
        not(any(
            feature = "egui",
            feature = "fltk",
            feature = "vanilla-js",
            feature = "actix"
        ))
    ))]
    pub fn build_default(self) -> Result<crate::App<DefaultRenderer>, crate::BuilderError> {
        self.build_default_html_lambda()
    }

    /// # Errors
    ///
    /// * If fails to build the default renderer
    #[cfg(all(
        feature = "html",
        feature = "vanilla-js",
        not(any(
            feature = "egui",
            feature = "fltk",
            feature = "actix",
            feature = "lambda"
        ))
    ))]
    pub fn build_default(self) -> Result<crate::App<DefaultRenderer>, crate::BuilderError> {
        self.build_default_html_vanilla_js()
    }

    /// # Errors
    ///
    /// * If fails to build the default renderer
    #[cfg(all(
        feature = "html",
        not(any(
            feature = "egui",
            feature = "fltk",
            feature = "vanilla-js",
            feature = "actix",
            feature = "lambda"
        ))
    ))]
    pub fn build_default(self) -> Result<crate::App<DefaultRenderer>, crate::BuilderError> {
        self.build_default_html()
    }

    /// # Errors
    ///
    /// * If fails to build the default renderer
    #[cfg(not(any(
        feature = "html",
        feature = "egui",
        feature = "fltk",
        feature = "vanilla-js"
    )))]
    pub fn build_default(self) -> Result<crate::App<DefaultRenderer>, crate::BuilderError> {
        self.build_default_stub()
    }
}
