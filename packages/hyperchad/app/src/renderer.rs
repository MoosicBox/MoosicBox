//! Renderer implementations and type aliases for different backends.
//!
//! This module provides the `DefaultRenderer` type alias which resolves to the appropriate
//! renderer implementation based on enabled features. It also contains backend-specific modules
//! for egui, fltk, HTML (with Actix/Lambda variants), and a stub renderer for testing.
//!
//! # Feature-based selection
//!
//! The default renderer is selected based on feature flags in the following priority order:
//!
//! * `egui` - Native GUI using egui
//! * `fltk` - Native GUI using fltk
//! * `actix` + `vanilla-js` - Web application using Actix with vanilla JavaScript
//! * `lambda` + `vanilla-js` - Serverless web application using AWS Lambda
//! * `actix` + `html` - Web application using Actix with plain HTML
//! * `lambda` + `html` - Serverless web application using Lambda with plain HTML
//! * `html` + `vanilla-js` - Standalone HTML with vanilla JavaScript (stub)
//! * `html` - Plain HTML renderer (stub)
//! * Default - Stub renderer (no-op implementation)

use crate::AppBuilder;

/// The default renderer type based on enabled features.
///
/// This type alias resolves to the appropriate renderer implementation depending on which
/// features are enabled (see module documentation for priority order).
#[cfg(feature = "egui")]
pub type DefaultRenderer = egui::EguiRenderer;

/// The default renderer type based on enabled features.
///
/// This type alias resolves to the appropriate renderer implementation depending on which
/// features are enabled (see module documentation for priority order).
#[cfg(all(feature = "fltk", not(feature = "egui")))]
pub type DefaultRenderer = fltk::FltkRenderer;

/// The default renderer type based on enabled features.
///
/// This type alias resolves to the appropriate renderer implementation depending on which
/// features are enabled (see module documentation for priority order).
#[cfg(all(
    feature = "actix",
    feature = "vanilla-js",
    not(any(feature = "egui", feature = "fltk"))
))]
pub type DefaultRenderer = html::actix::vanilla_js::HtmlVanillaJsActixRenderer;

/// The default renderer type based on enabled features.
///
/// This type alias resolves to the appropriate renderer implementation depending on which
/// features are enabled (see module documentation for priority order).
#[cfg(all(
    feature = "lambda",
    feature = "vanilla-js",
    not(any(feature = "egui", feature = "fltk", feature = "actix"))
))]
pub type DefaultRenderer = html::lambda::vanilla_js::HtmlVanillaJsLambdaRenderer;

/// The default renderer type based on enabled features.
///
/// This type alias resolves to the appropriate renderer implementation depending on which
/// features are enabled (see module documentation for priority order).
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

/// The default renderer type based on enabled features.
///
/// This type alias resolves to the appropriate renderer implementation depending on which
/// features are enabled (see module documentation for priority order).
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

/// The default renderer type based on enabled features.
///
/// This type alias resolves to the appropriate renderer implementation depending on which
/// features are enabled (see module documentation for priority order).
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

/// The default renderer type based on enabled features.
///
/// This type alias resolves to the appropriate renderer implementation depending on which
/// features are enabled (see module documentation for priority order).
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

/// The default renderer type based on enabled features.
///
/// This type alias resolves to the appropriate renderer implementation depending on which
/// features are enabled (see module documentation for priority order).
#[cfg(not(any(
    feature = "html",
    feature = "egui",
    feature = "fltk",
    feature = "vanilla-js"
)))]
pub type DefaultRenderer = stub::StubRenderer;

#[cfg(feature = "egui")]
mod egui {
    //! Egui renderer integration and builder helpers.

    use std::sync::Arc;

    use async_trait::async_trait;
    use hyperchad_renderer::transformer::layout::calc::{Calculator, CalculatorDefaults};
    use hyperchad_renderer_egui::{eframe::egui, font_metrics::EguiFontMetrics};
    use hyperchad_router::{DEFAULT_CLIENT_INFO, Router};

    use crate::{App, AppBuilder, BuilderError, Cleaner, Error, Generator};

    #[cfg(feature = "assets")]
    mod native_assets {
        use std::{
            io::{self, Read as _},
            path::{Component, Path},
            sync::Arc,
        };

        use hyperchad_renderer::{
            Content,
            assets::{AssetNotFoundBehavior, AssetPathTarget, StaticAssetRoute},
        };
        use hyperchad_router::{RouteFunc, RoutePath, RouteRequest, Router};
        use switchy::{http::models::Method, unsync::runtime::Handle};

        // Match the native image resolver's bounded-work requirement. Never read a
        // whole arbitrary file before checking the limit (including growing files).
        const MAX_ASSET_BYTES: usize = 16 * 1024 * 1024;

        /// Register the configured asset layer before handing the router to egui.
        ///
        /// Capture the already-configured application routes for explicit fallthrough;
        /// requests never publish into App's navigation channel.
        ///
        /// # Panics
        /// * If the router's route locks are poisoned.
        pub(super) fn register(
            router: &Router,
            assets: Vec<StaticAssetRoute>,
            missing: AssetNotFoundBehavior,
            runtime: Handle,
        ) {
            if assets.is_empty() {
                return;
            }
            let fallback = router.routes.read().expect("router routes lock").clone();
            #[cfg(feature = "static-routes")]
            let fallback = {
                let mut fallback_routes = fallback;
                fallback_routes.extend(
                    router
                        .static_routes
                        .read()
                        .expect("router static routes lock")
                        .clone(),
                );
                fallback_routes
            };
            let paths: Vec<RoutePath> = assets
                .iter()
                .flat_map(|asset| {
                    let exact = RoutePath::Literal(asset.route.clone());
                    if matches!(&asset.target, AssetPathTarget::Directory(_)) {
                        vec![
                            exact,
                            RoutePath::LiteralPrefix(format!(
                                "{}/",
                                asset.route.trim_end_matches('/')
                            )),
                        ]
                    } else {
                        vec![exact]
                    }
                })
                .collect();
            let handler: RouteFunc = Arc::new(Box::new(move |request: RouteRequest| {
                let assets = assets.clone();
                let fallback = fallback.clone();
                let runtime = runtime.clone();
                Box::pin(async move {
                    dispatch(&assets, &fallback, missing, &runtime, request)
                        .await
                        .map_err(|error| Box::new(error) as Box<dyn std::error::Error>)
                })
            }));
            let mut registered_routes = router.routes.write().expect("router routes lock");
            for path in paths.into_iter().rev() {
                registered_routes.insert(0, (path, handler.clone()));
            }
        }

        async fn dispatch(
            assets: &[StaticAssetRoute],
            fallback: &[(RoutePath, RouteFunc)],
            missing: AssetNotFoundBehavior,
            runtime: &Handle,
            request: RouteRequest,
        ) -> io::Result<Option<Content>> {
            // Router's string conversion retains the query in `path`, whereas
            // RouteRequest::from_path separates it. Both are valid native callers;
            // query bytes must never become part of a filesystem path.
            let resource_path = request
                .path
                .split_once('?')
                .map_or(request.path.as_str(), |(path, _)| path);
            let path = decoded_path(resource_path)?;
            for asset in assets {
                let Some(relative) = asset_suffix(asset, &path) else {
                    continue;
                };
                if !matches!(request.method, Method::Get | Method::Head) {
                    return Err(io::Error::new(
                        io::ErrorKind::Unsupported,
                        "native assets require GET or HEAD",
                    ));
                }
                let target = asset.target.clone();
                let result = match target {
                    AssetPathTarget::FileContents(data) => {
                        if data.len() > MAX_ASSET_BYTES {
                            Err(too_large())
                        } else {
                            Ok(data.to_vec())
                        }
                    }
                    target => {
                        let relative = relative.to_owned();
                        runtime
                            .spawn_blocking(move || {
                                read_target(&target, &relative, MAX_ASSET_BYTES)
                            })
                            .await
                            .map_err(|_| io::Error::other("native asset read task failed"))?
                    }
                };
                match result {
                    Ok(data) => {
                        return Ok(Some(Content::Raw {
                            data: if request.method == Method::Head {
                                Vec::new().into()
                            } else {
                                data.into()
                            },
                            content_type: media_type(&path).to_owned(),
                        }));
                    }
                    Err(error) if error.kind() == io::ErrorKind::NotFound => {
                        match asset.not_found_behavior.unwrap_or(missing) {
                            AssetNotFoundBehavior::Fallthrough => {}
                            AssetNotFoundBehavior::NotFound => {
                                return Err(io::Error::new(
                                    io::ErrorKind::NotFound,
                                    "native asset not found",
                                ));
                            }
                            AssetNotFoundBehavior::InternalServerError => {
                                return Err(io::Error::other(
                                    "native asset missing (internal-error policy)",
                                ));
                            }
                        }
                    }
                    Err(error) => return Err(error),
                }
            }
            if let Some((_, handler)) = fallback
                .iter()
                .find(|(route, _)| route.matches(&request.path))
            {
                return handler(request)
                    .await
                    .map_err(|_| io::Error::other("native asset fallback handler failed"));
            }
            Err(io::Error::new(
                io::ErrorKind::NotFound,
                "native route not found",
            ))
        }

        fn asset_suffix<'a>(asset: &StaticAssetRoute, path: &'a str) -> Option<&'a str> {
            if !asset.route.starts_with('/') || asset.route.starts_with("//") {
                return None;
            }
            match &asset.target {
                AssetPathTarget::Directory(_) => {
                    let prefix = asset.route.trim_end_matches('/');
                    if path == prefix {
                        Some("")
                    } else {
                        path.strip_prefix(prefix)?.strip_prefix('/')
                    }
                }
                AssetPathTarget::File(_) | AssetPathTarget::FileContents(_) => {
                    (path == asset.route).then_some("")
                }
            }
        }

        fn decoded_path(path: &str) -> io::Result<String> {
            let invalid =
                || io::Error::new(io::ErrorKind::PermissionDenied, "unsafe native asset path");
            if !path.starts_with('/') || path.starts_with("//") || path.contains(['?', '#', '\\']) {
                return Err(invalid());
            }
            let mut bytes = path.bytes();
            let mut decoded = Vec::new();
            while let Some(byte) = bytes.next() {
                decoded.push(if byte == b'%' {
                    let high = bytes
                        .next()
                        .and_then(|b| char::from(b).to_digit(16))
                        .ok_or_else(invalid)?;
                    let low = bytes
                        .next()
                        .and_then(|b| char::from(b).to_digit(16))
                        .ok_or_else(invalid)?;
                    let value = u8::try_from(high * 16 + low).map_err(|_| invalid())?;
                    // Reject encoded separators, double escapes and Windows prefixes.
                    if matches!(value, b'/' | b'\\' | b'%' | b':') {
                        return Err(invalid());
                    }
                    value
                } else {
                    byte
                });
            }
            let path = String::from_utf8(decoded).map_err(|_| invalid())?;
            if path.chars().any(char::is_control)
                || path.contains(['\\', ':'])
                || path.split('/').any(|segment| matches!(segment, "." | ".."))
            {
                return Err(invalid());
            }
            Ok(path)
        }

        fn read_target(
            target: &AssetPathTarget,
            relative: &str,
            limit: usize,
        ) -> io::Result<Vec<u8>> {
            let path = match target {
                AssetPathTarget::File(path) => path.canonicalize()?,
                AssetPathTarget::Directory(root) => {
                    if relative.is_empty() {
                        return Err(io::Error::new(
                            io::ErrorKind::NotFound,
                            "native asset directory is not a file",
                        ));
                    }
                    let relative = Path::new(relative);
                    if relative
                        .components()
                        .any(|part| !matches!(part, Component::Normal(_)))
                    {
                        return Err(io::Error::new(
                            io::ErrorKind::PermissionDenied,
                            "unsafe native asset relative path",
                        ));
                    }
                    let root = root.canonicalize()?;
                    let path = root.join(relative).canonicalize()?;
                    if !path.starts_with(&root) {
                        return Err(io::Error::new(
                            io::ErrorKind::PermissionDenied,
                            "native asset escapes configured directory",
                        ));
                    }
                    path
                }
                AssetPathTarget::FileContents(_) => {
                    return Err(io::Error::new(
                        io::ErrorKind::InvalidInput,
                        "memory asset is not a filesystem target",
                    ));
                }
            };
            // Reject device/FIFO/directory targets before opening; these are not
            // bounded regular-file reads. Recheck the opened handle as well.
            if !std::fs::metadata(&path)?.is_file() {
                return Err(io::Error::new(
                    io::ErrorKind::NotFound,
                    "native asset is not a regular file",
                ));
            }
            let file = std::fs::File::open(path)?;
            let metadata = file.metadata()?;
            if !metadata.is_file() {
                return Err(io::Error::new(
                    io::ErrorKind::NotFound,
                    "native asset is not a regular file",
                ));
            }
            if metadata.len() > limit as u64 {
                return Err(too_large());
            }
            let mut bytes = Vec::new();
            file.take(limit.saturating_add(1) as u64)
                .read_to_end(&mut bytes)?;
            if bytes.len() > limit {
                return Err(too_large());
            }
            Ok(bytes)
        }

        fn too_large() -> io::Error {
            io::Error::new(
                io::ErrorKind::InvalidData,
                "native asset exceeds byte limit",
            )
        }

        fn media_type(path: &str) -> &'static str {
            match path
                .rsplit('.')
                .next()
                .unwrap_or("")
                .to_ascii_lowercase()
                .as_str()
            {
                "png" => "image/png",
                "jpg" | "jpeg" => "image/jpeg",
                "gif" => "image/gif",
                "webp" => "image/webp",
                "svg" => "image/svg+xml",
                "ico" => "image/x-icon",
                "avif" => "image/avif",
                "css" => "text/css",
                "js" | "mjs" => "text/javascript",
                "html" | "htm" => "text/html",
                "txt" => "text/plain",
                "json" => "application/json",
                "woff" => "font/woff",
                "woff2" => "font/woff2",
                _ => "application/octet-stream",
            }
        }

        #[cfg(test)]
        mod tests {
            use super::*;
            use std::{
                path::PathBuf,
                sync::atomic::{AtomicU64, Ordering},
            };
            use switchy::unsync::runtime::Runtime;

            struct Fixture(PathBuf);
            impl Fixture {
                fn new() -> Self {
                    static NEXT: AtomicU64 = AtomicU64::new(0);
                    let path = std::env::temp_dir().join(format!(
                        "hyperchad-native-assets-{}-{}",
                        std::process::id(),
                        NEXT.fetch_add(1, Ordering::Relaxed)
                    ));
                    std::fs::create_dir(&path).unwrap();
                    Self(path)
                }
            }
            impl Drop for Fixture {
                fn drop(&mut self) {
                    let _ = std::fs::remove_dir_all(&self.0);
                }
            }
            fn asset(route: &str, target: AssetPathTarget) -> StaticAssetRoute {
                StaticAssetRoute {
                    route: route.into(),
                    target,
                    not_found_behavior: None,
                }
            }
            fn raw(content: Option<Content>) -> Vec<u8> {
                let Some(Content::Raw { data, .. }) = content else {
                    panic!("expected native raw asset");
                };
                data.to_vec()
            }
            fn request(path: &str) -> RouteRequest {
                RouteRequest::from_path(path, hyperchad_router::RequestInfo::default())
            }

            #[test]
            fn configured_memory_file_and_directory_are_real_router_raw_routes() {
                let fixture = Fixture::new();
                let file = fixture.0.join("photo.png");
                std::fs::write(&file, b"file-image").unwrap();
                let runtime = Runtime::new();
                let router = Router::new();
                register(
                    &router,
                    vec![
                        asset(
                            "/custom/embedded.svg",
                            AssetPathTarget::FileContents(b"<svg/>".to_vec().into()),
                        ),
                        asset("/custom/photo", AssetPathTarget::File(file)),
                        asset(
                            "/custom/tree",
                            AssetPathTarget::Directory(fixture.0.clone()),
                        ),
                    ],
                    AssetNotFoundBehavior::NotFound,
                    runtime.handle(),
                );
                runtime.handle().block_on(async {
                    assert_eq!(
                        raw(router.navigate("/custom/embedded.svg").await.unwrap()),
                        b"<svg/>"
                    );
                    assert_eq!(
                        raw(router.navigate("/custom/photo").await.unwrap()),
                        b"file-image"
                    );
                    let Some(Content::Raw { data, content_type }) = router
                        .navigate("/custom/tree/photo.png?cache=1")
                        .await
                        .unwrap()
                    else {
                        panic!("raw file");
                    };
                    assert_eq!(data.as_ref(), b"file-image");
                    assert_eq!(content_type, "image/png");
                    // Query content is not a path component, regardless of which
                    // Router request constructor the native resolver uses.
                    assert_eq!(
                        raw(router
                            .navigate("/custom/tree/photo.png?next=../secret")
                            .await
                            .unwrap()),
                        b"file-image"
                    );
                    assert_eq!(
                        raw(router
                            .navigate(request("/custom/tree/photo.png?cache=2"))
                            .await
                            .unwrap()),
                        b"file-image"
                    );
                    assert!(
                        router
                            .navigate("/custom/tree/%2e%2e/secret?cache=1")
                            .await
                            .is_err()
                    );
                    assert!(
                        router
                            .navigate("/custom/treehouse/photo.png")
                            .await
                            .is_err()
                    );
                    assert!(router.navigate("/custom/photo/child").await.is_err());
                    let mut head = request("/custom/tree/photo.png");
                    head.method = Method::Head;
                    assert_eq!(raw(router.navigate(head).await.unwrap()), [0_u8; 0]);
                });
            }

            #[test]
            fn missing_policy_override_default_and_application_fallthrough() {
                let fixture = Fixture::new();
                let runtime = Runtime::new();
                let fallback = Router::new().with_route("/assets/missing.png", |_| async {
                    Content::Raw {
                        data: b"fallback".to_vec().into(),
                        content_type: "text/plain".into(),
                    }
                });
                let routes = fallback.routes.read().unwrap().clone();
                runtime.handle().block_on(async {
                    for (policy, kind) in [
                        (AssetNotFoundBehavior::NotFound, io::ErrorKind::NotFound),
                        (
                            AssetNotFoundBehavior::InternalServerError,
                            io::ErrorKind::Other,
                        ),
                    ] {
                        let mut route =
                            asset("/assets", AssetPathTarget::Directory(fixture.0.clone()));
                        route.not_found_behavior = Some(policy);
                        let error = dispatch(
                            &[route],
                            &routes,
                            AssetNotFoundBehavior::Fallthrough,
                            &runtime.handle(),
                            request("/assets/missing.png"),
                        )
                        .await
                        .unwrap_err();
                        assert_eq!(error.kind(), kind);
                    }
                    let route = asset("/assets", AssetPathTarget::Directory(fixture.0.clone()));
                    let content = dispatch(
                        &[route],
                        &routes,
                        AssetNotFoundBehavior::Fallthrough,
                        &runtime.handle(),
                        request("/assets/missing.png"),
                    )
                    .await
                    .unwrap();
                    assert_eq!(raw(content), b"fallback");
                    let registered = Router::new().with_route("/assets/missing.png", |_| async {
                        Content::Raw {
                            data: b"registered-fallback".to_vec().into(),
                            content_type: "text/plain".into(),
                        }
                    });
                    register(
                        &registered,
                        vec![asset(
                            "/assets",
                            AssetPathTarget::Directory(fixture.0.clone()),
                        )],
                        AssetNotFoundBehavior::Fallthrough,
                        runtime.handle(),
                    );
                    assert_eq!(
                        raw(registered.navigate("/assets/missing.png").await.unwrap()),
                        b"registered-fallback"
                    );
                    let file = asset(
                        "/assets/missing.png",
                        AssetPathTarget::File(fixture.0.join("missing.png")),
                    );
                    assert_eq!(
                        dispatch(
                            &[file],
                            &[],
                            AssetNotFoundBehavior::NotFound,
                            &runtime.handle(),
                            request("/assets/missing.png")
                        )
                        .await
                        .unwrap_err()
                        .kind(),
                        io::ErrorKind::NotFound
                    );
                });
            }

            #[test]
            fn directory_traversal_symlink_escape_and_bounded_reads() {
                let fixture = Fixture::new();
                let root = fixture.0.join("root");
                std::fs::create_dir(&root).unwrap();
                std::fs::write(fixture.0.join("secret"), b"private").unwrap();
                std::fs::write(root.join("large"), b"12345").unwrap();
                for path in [
                    "/assets/../secret",
                    "/assets/%2e%2e/secret",
                    "/assets/%2fetc",
                    "/assets/%252e%252e/secret",
                    "/assets/..\\secret",
                    "/assets/%00",
                    "//host/assets",
                    "/assets/C:secret",
                ] {
                    assert!(decoded_path(path).is_err(), "{path}");
                }
                assert_eq!(
                    decoded_path("/assets/a%20b.png").unwrap(),
                    "/assets/a b.png"
                );
                let directory = AssetPathTarget::Directory(root.clone());
                assert_eq!(
                    read_target(&directory, "large", 4).unwrap_err().kind(),
                    io::ErrorKind::InvalidData
                );
                assert_eq!(
                    read_target(&directory, "../secret", 20).unwrap_err().kind(),
                    io::ErrorKind::PermissionDenied
                );
                assert_eq!(
                    read_target(&directory, "", 20).unwrap_err().kind(),
                    io::ErrorKind::NotFound
                );
                #[cfg(unix)]
                {
                    std::os::unix::fs::symlink(fixture.0.join("secret"), root.join("escape"))
                        .unwrap();
                    assert_eq!(
                        read_target(&directory, "escape", 20).unwrap_err().kind(),
                        io::ErrorKind::PermissionDenied
                    );
                }
            }
        }
    }

    /// Calculator for egui font metrics and layout calculations.
    #[derive(Clone)]
    pub struct EguiCalculator(pub Option<Arc<Calculator<EguiFontMetrics>>>);

    /// Type alias for the egui renderer with custom calculator.
    pub type EguiRenderer = hyperchad_renderer_egui::EguiRenderer<EguiCalculator>;

    impl hyperchad_renderer::transformer::layout::Calc for EguiCalculator {
        /// Calculates layout for a container using egui font metrics.
        ///
        /// # Panics
        ///
        /// * If the calculator has not been initialized with an egui context
        fn calc(&self, container: &mut hyperchad_router::Container) -> bool {
            self.0.as_ref().unwrap().calc(container)
        }

        /// Calculates checked layout without publishing failed geometry.
        ///
        /// # Errors
        ///
        /// * Returns checked-layout unsupported before context initialization.
        /// * Propagates structural, budget, and geometry errors from the calculator.
        fn try_calc(
            &self,
            container: &mut hyperchad_router::Container,
        ) -> Result<bool, hyperchad_renderer::transformer::layout::LayoutError> {
            use hyperchad_renderer::transformer::layout::{LayoutError, LayoutErrorKind};

            self.0.as_ref().map_or_else(
                || {
                    Err(LayoutError {
                        path: Vec::new(),
                        kind: LayoutErrorKind::CheckedLayoutUnsupported,
                    })
                },
                |calculator| calculator.try_calc(container),
            )
        }
    }

    impl hyperchad_renderer_egui::layout::EguiCalc for EguiCalculator {
        /// Initializes the calculator with egui context and font metrics.
        ///
        /// This sets up default font sizes and margins for various HTML elements
        /// (h1-h6, paragraphs) scaled by a factor of 14/16 relative to standard sizes.
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
        /// No-op generator for egui renderer (static generation not supported for native GUI).
        ///
        /// # Errors
        ///
        /// * This implementation never returns an error
        async fn generate(&self, _router: &Router, _output: Option<String>) -> Result<(), Error> {
            Ok(())
        }
    }

    #[async_trait]
    impl Cleaner for EguiRenderer {
        /// No-op cleaner for egui renderer (no output to clean for native GUI).
        ///
        /// # Errors
        ///
        /// * This implementation never returns an error
        async fn clean(&self, _output: Option<String>) -> Result<(), Error> {
            Ok(())
        }
    }

    impl AppBuilder {
        /// Builds an `App` with an egui renderer.
        ///
        /// # Errors
        ///
        /// * [`BuilderError::MissingRouter`] if the `AppBuilder` is missing a router
        pub fn build_egui(self, renderer: EguiRenderer) -> Result<App<EguiRenderer>, BuilderError> {
            log::debug!("build_egui");

            let router = self.router.clone().ok_or(BuilderError::MissingRouter)?;
            let responses = renderer
                .take_response_receiver()
                .map_err(BuilderError::NativeResponseConsumer)?;
            self.runtime_handle().spawn({
                let router = router.clone();
                async move {
                    while let Ok(response) = responses.recv_async().await {
                        if let Err(error) =
                            crate::apply_native_response_metadata(&router, &response).await
                        {
                            log::error!("Failed to apply native response metadata: {error}");
                        }
                    }
                }
            });
            self.runtime_handle().spawn({
                let renderer = renderer.clone();
                async move {
                    while let Some(path) = renderer.wait_for_navigation().await {
                        if let Err(error) = router
                            .navigate_send((
                                &path,
                                hyperchad_router::RequestInfo {
                                    client: DEFAULT_CLIENT_INFO.clone(),
                                },
                            ))
                            .await
                        {
                            log::error!("Failed to navigate: {error:?}");
                        }
                    }
                }
            });
            self.build(renderer)
        }

        /// Builds an `App` with a default egui renderer configuration.
        ///
        /// # Errors
        ///
        /// * [`BuilderError::MissingRouter`] if the `AppBuilder` is missing a router
        pub fn build_default_egui(self) -> Result<App<EguiRenderer>, BuilderError> {
            log::debug!("build_default_egui");

            let action_tx = self.listen_actions(self.action_handlers.clone());
            let resize_tx = self.listen_resize(self.resize_listeners.clone());
            let router = self.router.clone().ok_or(BuilderError::MissingRouter)?;
            #[cfg(feature = "assets")]
            native_assets::register(
                &router,
                self.static_asset_routes.clone(),
                self.asset_not_found_behavior,
                self.runtime_handle(),
            );
            let calculator = EguiCalculator(None);
            let renderer = hyperchad_renderer_egui::EguiRenderer::new(
                router,
                #[cfg(feature = "logic")]
                action_tx,
                resize_tx,
                DEFAULT_CLIENT_INFO.clone(),
                calculator,
            );

            self.build_egui(renderer)
        }
    }

    #[cfg(test)]
    mod checked_layout_tests {
        use hyperchad_renderer::transformer::layout::{Calc, LayoutErrorKind};

        use super::*;

        #[test]
        fn uninitialized_calculator_rejects_checked_layout() {
            let mut container = hyperchad_router::Container::default();
            let error = EguiCalculator(None).try_calc(&mut container).unwrap_err();
            assert_eq!(error.kind, LayoutErrorKind::CheckedLayoutUnsupported);
            assert_eq!(error.path, [] as [usize; 0]);
        }

        #[test]
        fn initialized_calculator_forwards_checked_layout_errors() {
            let calculator = hyperchad_renderer_egui::layout::EguiCalc::with_context(
                EguiCalculator(None),
                egui::Context::default(),
            );
            let mut container = hyperchad_router::Container {
                element: hyperchad_renderer::transformer::Element::Table,
                children: vec![hyperchad_router::Container {
                    element: hyperchad_renderer::transformer::Element::Div,
                    ..hyperchad_router::Container::default()
                }],
                ..hyperchad_router::Container::default()
            };
            let error = calculator.try_calc(&mut container).unwrap_err();
            assert_eq!(error.kind, LayoutErrorKind::InvalidTableStructure);
        }
    }
}

#[cfg(feature = "fltk")]
mod fltk {
    //! FLTK renderer integration and builder helpers.

    use async_trait::async_trait;
    use hyperchad_router::{DEFAULT_CLIENT_INFO, Router};

    use crate::{App, AppBuilder, BuilderError, Cleaner, Error, Generator};

    /// Type alias for the fltk renderer.
    pub type FltkRenderer = hyperchad_renderer_fltk::FltkRenderer;

    #[async_trait]
    impl Generator for FltkRenderer {
        /// No-op generator for fltk renderer (static generation not supported for native GUI).
        ///
        /// # Errors
        ///
        /// * This implementation never returns an error
        async fn generate(&self, _router: &Router, _output: Option<String>) -> Result<(), Error> {
            Ok(())
        }
    }

    #[async_trait]
    impl Cleaner for FltkRenderer {
        /// No-op cleaner for fltk renderer (no output to clean for native GUI).
        ///
        /// # Errors
        ///
        /// * This implementation never returns an error
        async fn clean(&self, _output: Option<String>) -> Result<(), Error> {
            Ok(())
        }
    }

    impl AppBuilder {
        /// Builds an `App` with a fltk renderer.
        ///
        /// # Errors
        ///
        /// * [`BuilderError::MissingRouter`] if the `AppBuilder` is missing a router
        pub fn build_fltk(self, renderer: FltkRenderer) -> Result<App<FltkRenderer>, BuilderError> {
            log::debug!("build_fltk");

            self.build(renderer)
        }

        /// Builds an `App` with a default fltk renderer configuration.
        ///
        /// # Errors
        ///
        /// * [`BuilderError::MissingRouter`] if the `AppBuilder` is missing a router
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
/// HTML renderer implementations with support for static generation and multiple backends.
///
/// This module provides HTML rendering capabilities with support for Actix web server,
/// AWS Lambda serverless deployment, and vanilla JavaScript interactivity.
pub mod html {
    use std::{path::PathBuf, sync::LazyLock};

    use async_trait::async_trait;
    use hyperchad_renderer::ToRenderRunner;
    use hyperchad_renderer_html::{HtmlApp, HtmlRenderer};
    use hyperchad_router::Router;

    use crate::{App, AppBuilder, BuilderError};
    use crate::{Cleaner, Error, Generator};

    /// Type alias for a stub HTML renderer (no backend server).
    pub type HtmlStubRenderer = hyperchad_renderer_html::HtmlRenderer<
        hyperchad_renderer_html::stub::StubApp<hyperchad_renderer_html::DefaultHtmlTagRenderer>,
    >;

    static DEFAULT_OUTPUT_DIR: &str = "gen";
    static CARGO_MANIFEST_DIR: LazyLock<Option<std::path::PathBuf>> =
        LazyLock::new(|| std::env::var("CARGO_MANIFEST_DIR").ok().map(Into::into));

    #[async_trait]
    impl<T: HtmlApp + ToRenderRunner + Send + Sync> Generator for HtmlRenderer<T> {
        /// Generates static HTML files for all registered static routes.
        ///
        /// # Errors
        ///
        /// * [`Error::IO`] if file I/O operations fail during generation
        ///
        /// # Panics
        ///
        /// * If the `static-routes` feature is not enabled (feature-gated panic)
        /// * If file write operations fail during generation
        /// * If JSON serialization fails for JSON content
        /// * If the static routes `RwLock` is poisoned
        #[cfg(not(feature = "static-routes"))]
        async fn generate(&self, _router: &Router, _output: Option<String>) -> Result<(), Error> {
            panic!("Must have `static-routes` enabled to generate");
        }

        /// Generates static HTML files for all registered static routes.
        ///
        /// # Errors
        ///
        /// * [`Error::IO`] if file I/O operations fail during generation
        ///
        /// # Panics
        ///
        /// * If file write operations fail during generation
        /// * If JSON serialization fails for JSON content
        /// * If the static routes `RwLock` is poisoned
        /// * If route processing fails
        #[allow(clippy::too_many_lines)]
        #[cfg(feature = "static-routes")]
        async fn generate(&self, router: &Router, output: Option<String>) -> Result<(), Error> {
            use std::io::Write as _;

            use hyperchad_renderer::{Color, Content};
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
                            Content::View(boxed_view)
                                if boxed_view.response.navigation.is_some() =>
                            {
                                return Err(Error::StaticNavigation(path_str.to_string()));
                            }
                            Content::View(boxed_view) if boxed_view.primary.is_some() => {
                                let view = boxed_view.primary.as_ref().unwrap();
                                let html = container_element_to_html_response(
                                    &std::collections::BTreeMap::new(),
                                    view,
                                    Some(&*VIEWPORT),
                                    Some(*BACKGROUND_COLOR),
                                    Some("MoosicBox"),
                                    Some("MoosicBox: A music app for cows"),
                                    self.app.tag_renderer(),
                                    self.app.css_urls(),
                                    self.app.css_paths(),
                                    self.app.inline_css_blocks(),
                                )?;

                                log::debug!(
                                    "generate: path={path_str} -> {}:\n{html}",
                                    output_path.display()
                                );

                                file.write_all(html.as_bytes())
                                    .expect("Failed to write file");
                            }
                            Content::View(_) => {
                                // Fragments-only view - skip for static generation
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

                /// Recursively copies a directory and all its contents to a destination.
                ///
                /// Entries are sorted alphabetically for deterministic processing.
                ///
                /// # Errors
                ///
                /// * If directory creation fails
                /// * If reading directory entries fails
                /// * If copying files fails
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
        /// Removes the generated output directory and all its contents.
        ///
        /// # Errors
        ///
        /// * [`Error::IO`] if directory removal fails
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
        /// Builds an `App` with an HTML stub renderer.
        ///
        /// # Errors
        ///
        /// * [`BuilderError::MissingRouter`] if the `AppBuilder` is missing a router
        pub fn build_html(
            self,
            renderer: HtmlStubRenderer,
        ) -> Result<App<HtmlStubRenderer>, BuilderError> {
            log::debug!("build_html");

            self.build(renderer)
        }

        /// Builds an `App` with a default HTML stub renderer configuration.
        ///
        /// # Errors
        ///
        /// * [`BuilderError::MissingRouter`] if the `AppBuilder` is missing a router
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
        //! Actix-backed HTML renderer types and builder helpers.

        use crate::{App, AppBuilder, BuilderError};

        /// Type alias for an HTML renderer with Actix web server backend.
        pub type HtmlActixRenderer = hyperchad_renderer_html::HtmlRenderer<
            hyperchad_renderer_html::actix::ActixApp<
                hyperchad_renderer_html::actix::PreparedRequest,
                hyperchad_renderer_html::actix::HtmlActixResponseProcessor<
                    hyperchad_renderer_html::DefaultHtmlTagRenderer,
                >,
            >,
        >;

        impl AppBuilder {
            /// Builds an `App` with an HTML Actix renderer.
            ///
            /// # Errors
            ///
            /// * [`BuilderError::MissingRouter`] if the `AppBuilder` is missing a router
            pub fn build_html_actix(
                self,
                renderer: HtmlActixRenderer,
            ) -> Result<App<HtmlActixRenderer>, BuilderError> {
                log::debug!("build_html_actix");

                self.build(renderer)
            }

            /// Builds an `App` with a default HTML Actix renderer configuration.
            ///
            /// # Errors
            ///
            /// * [`BuilderError::MissingRouter`] if the `AppBuilder` is missing a router
            pub fn build_default_html_actix(self) -> Result<App<HtmlActixRenderer>, BuilderError> {
                log::debug!("build_default_html_actix");

                let router = self.router.clone().ok_or(BuilderError::MissingRouter)?;

                #[allow(unused_mut)]
                let mut renderer = hyperchad_renderer_html::router_to_actix(
                    hyperchad_renderer_html::DefaultHtmlTagRenderer::default(),
                    router,
                );

                if let Some(address) = self.actix_bind_address.as_deref() {
                    renderer.app.set_bind_address(address);
                }
                if let Some(port) = self.actix_port {
                    renderer.app.set_port(port);
                }
                if let Some(callback) = self.actix_on_bound.clone() {
                    renderer.app.set_on_bound(callback);
                }

                #[cfg(feature = "assets")]
                #[allow(unused_mut)]
                let mut renderer = renderer
                    .with_static_asset_routes(self.static_asset_routes.clone())
                    .with_asset_not_found_behavior(self.asset_not_found_behavior);

                #[cfg(feature = "html")]
                #[allow(unused_mut)]
                let mut renderer = renderer
                    .with_css_urls(&self.css_urls)
                    .with_css_paths(&self.css_paths)
                    .with_inline_css_blocks(&self.inline_css);

                self.build(renderer)
            }
        }

        #[cfg(feature = "vanilla-js")]
        pub mod vanilla_js {
            //! Actix + vanilla JavaScript HTML renderer types and builder helpers.

            use crate::{App, AppBuilder, BuilderError};

            /// Type alias for an HTML renderer with vanilla JavaScript and Actix web server backend.
            pub type HtmlVanillaJsActixRenderer = hyperchad_renderer_html::HtmlRenderer<
                hyperchad_renderer_html::actix::ActixApp<
                    hyperchad_renderer_html::actix::PreparedRequest,
                    hyperchad_renderer_html::actix::HtmlActixResponseProcessor<
                        hyperchad_renderer_vanilla_js::VanillaJsTagRenderer,
                    >,
                >,
            >;

            impl AppBuilder {
                /// Builds an `App` with an HTML vanilla JavaScript Actix renderer.
                ///
                /// # Errors
                ///
                /// * [`BuilderError::MissingRouter`] if the `AppBuilder` is missing a router
                pub fn build_html_vanilla_js_actix(
                    self,
                    renderer: HtmlVanillaJsActixRenderer,
                ) -> Result<App<HtmlVanillaJsActixRenderer>, BuilderError> {
                    log::debug!("build_html_vanilla_js_actix");

                    self.build(renderer)
                }

                /// Builds an `App` with a default HTML vanilla JavaScript Actix renderer configuration.
                ///
                /// # Errors
                ///
                /// * [`BuilderError::MissingRouter`] if the `AppBuilder` is missing a router
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

                    if let Some(address) = self.actix_bind_address.as_deref() {
                        renderer.app.set_bind_address(address);
                    }
                    if let Some(port) = self.actix_port {
                        renderer.app.set_port(port);
                    }
                    if let Some(callback) = self.actix_on_bound.clone() {
                        renderer.app.set_on_bound(callback);
                    }

                    #[cfg(feature = "assets")]
                    #[allow(unused_mut)]
                    let mut renderer = renderer
                        .with_static_asset_routes(self.static_asset_routes.clone())
                        .with_asset_not_found_behavior(self.asset_not_found_behavior);

                    #[cfg(feature = "html")]
                    #[allow(unused_mut)]
                    let mut renderer = renderer
                        .with_css_urls(&self.css_urls)
                        .with_css_paths(&self.css_paths)
                        .with_inline_css_blocks(&self.inline_css);

                    #[cfg(all(feature = "logic", feature = "actions"))]
                    {
                        let action_tx = self.listen_actions(self.action_handlers.clone());
                        renderer.app.set_action_tx(action_tx);
                    }

                    #[cfg(all(
                        feature = "logic",
                        feature = "actions",
                        feature = "shared-state-bridge"
                    ))]
                    if let Some(shared_state_bridge) = self.actix_shared_state_bridge.clone() {
                        let command_input_resolver = shared_state_bridge.command_input_resolver;

                        renderer.app.set_shared_state_bridge(
                            shared_state_bridge.command_tx,
                            shared_state_bridge.route_resolver,
                            std::sync::Arc::new(move |action, value| {
                                command_input_resolver((action, value))
                            }),
                        );
                    }

                    self.build(renderer)
                }
            }
        }
    }

    #[cfg(feature = "lambda")]
    pub mod lambda {
        //! Lambda-backed HTML renderer types and builder helpers.

        use crate::{App, AppBuilder, BuilderError};

        /// Type alias for an HTML renderer with AWS Lambda backend.
        pub type HtmlLambdaRenderer = hyperchad_renderer_html::HtmlRenderer<
            hyperchad_renderer_html::lambda::LambdaApp<
                hyperchad_renderer_html::lambda::PreparedRequest,
                hyperchad_renderer_html::lambda::HtmlLambdaResponseProcessor<
                    hyperchad_renderer_html::DefaultHtmlTagRenderer,
                >,
            >,
        >;

        impl AppBuilder {
            /// Builds an `App` with an HTML Lambda renderer.
            ///
            /// # Errors
            ///
            /// * [`BuilderError::MissingRouter`] if the `AppBuilder` is missing a router
            pub fn build_html_lambda(
                self,
                renderer: HtmlLambdaRenderer,
            ) -> Result<App<HtmlLambdaRenderer>, BuilderError> {
                self.build(renderer)
            }

            /// Builds an `App` with a default HTML Lambda renderer configuration.
            ///
            /// # Errors
            ///
            /// * [`BuilderError::MissingRouter`] if the `AppBuilder` is missing a router
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
                let renderer = renderer
                    .with_static_asset_routes(self.static_asset_routes.clone())
                    .with_asset_not_found_behavior(self.asset_not_found_behavior);

                #[cfg(feature = "html")]
                let renderer = renderer
                    .with_css_urls(&self.css_urls)
                    .with_css_paths(&self.css_paths)
                    .with_inline_css_blocks(&self.inline_css);

                self.build(renderer)
            }
        }

        #[cfg(feature = "vanilla-js")]
        pub mod vanilla_js {
            //! Lambda + vanilla JavaScript HTML renderer types and builder helpers.

            use crate::{App, AppBuilder, BuilderError};

            /// Type alias for an HTML renderer with vanilla JavaScript and AWS Lambda backend.
            pub type HtmlVanillaJsLambdaRenderer = hyperchad_renderer_html::HtmlRenderer<
                hyperchad_renderer_html::lambda::LambdaApp<
                    hyperchad_renderer_html::lambda::PreparedRequest,
                    hyperchad_renderer_html::lambda::HtmlLambdaResponseProcessor<
                        hyperchad_renderer_vanilla_js::VanillaJsTagRenderer,
                    >,
                >,
            >;

            impl AppBuilder {
                /// Builds an `App` with an HTML vanilla JavaScript Lambda renderer.
                ///
                /// # Errors
                ///
                /// * [`BuilderError::MissingRouter`] if the `AppBuilder` is missing a router
                pub fn build_html_vanilla_js_lambda(
                    self,
                    renderer: HtmlVanillaJsLambdaRenderer,
                ) -> Result<App<HtmlVanillaJsLambdaRenderer>, BuilderError> {
                    log::debug!("build_html_vanilla_js_lambda");

                    self.build(renderer)
                }

                /// Builds an `App` with a default HTML vanilla JavaScript Lambda renderer configuration.
                ///
                /// # Errors
                ///
                /// * [`BuilderError::MissingRouter`] if the `AppBuilder` is missing a router
                pub fn build_default_html_vanilla_js_lambda(
                    self,
                ) -> Result<App<HtmlVanillaJsLambdaRenderer>, BuilderError> {
                    log::debug!("build_default_html_vanilla_js_lambda");

                    let router = self.router.clone().ok_or(BuilderError::MissingRouter)?;

                    let renderer = hyperchad_renderer_html::router_to_lambda(
                        hyperchad_renderer_vanilla_js::VanillaJsTagRenderer::default(),
                        router,
                    )
                    .with_extend_html_renderer(hyperchad_renderer_vanilla_js::VanillaJsRenderer {});

                    #[cfg(feature = "assets")]
                    let renderer = renderer
                        .with_static_asset_routes(self.static_asset_routes.clone())
                        .with_asset_not_found_behavior(self.asset_not_found_behavior);

                    #[cfg(feature = "html")]
                    let renderer = renderer
                        .with_css_urls(&self.css_urls)
                        .with_css_paths(&self.css_paths)
                        .with_inline_css_blocks(&self.inline_css);

                    self.build(renderer)
                }
            }
        }
    }

    #[cfg(feature = "vanilla-js")]
    pub mod vanilla_js {
        //! Standalone vanilla JavaScript HTML renderer types and builder helpers.

        use crate::{App, AppBuilder, BuilderError};

        /// Type alias for an HTML renderer with vanilla JavaScript (no backend server).
        pub type HtmlVanillaJsRenderer = hyperchad_renderer_html::HtmlRenderer<
            hyperchad_renderer_html::stub::StubApp<
                hyperchad_renderer_vanilla_js::VanillaJsTagRenderer,
            >,
        >;

        impl AppBuilder {
            /// Builds an `App` with an HTML vanilla JavaScript renderer.
            ///
            /// # Errors
            ///
            /// * [`BuilderError::MissingRouter`] if the `AppBuilder` is missing a router
            pub fn build_html_vanilla_js(
                self,
                renderer: HtmlVanillaJsRenderer,
            ) -> Result<App<HtmlVanillaJsRenderer>, BuilderError> {
                log::debug!("build_html_vanilla_js");

                self.build(renderer)
            }

            /// Builds an `App` with a default HTML vanilla JavaScript renderer configuration.
            ///
            /// # Errors
            ///
            /// * [`BuilderError::MissingRouter`] if the `AppBuilder` is missing a router
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
                let renderer = renderer
                    .with_static_asset_routes(self.static_asset_routes.clone())
                    .with_asset_not_found_behavior(self.asset_not_found_behavior);

                self.build(renderer)
            }
        }
    }
}

/// Stub renderer for testing and no-op scenarios.
///
/// This module provides a `StubRenderer` that implements all renderer traits but performs
/// no actual rendering. Useful for testing, documentation examples, or as a placeholder.
pub mod stub {
    use async_trait::async_trait;
    use hyperchad_renderer::{
        Color, Handle, RenderRunner, Renderer, ToRenderRunner, View, transformer::ResponsiveTrigger,
    };

    use crate::{App, AppBuilder, BuilderError, Cleaner, Error, Generator};

    /// No-op stub renderer for testing and documentation.
    #[derive(Debug, Clone)]
    pub struct StubRenderer;

    #[async_trait]
    impl Renderer for StubRenderer {
        /// # Errors
        ///
        /// * If the underlying renderer implementation fails to initialize
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

        /// No-op method for adding responsive triggers (stub implementation).
        fn add_responsive_trigger(&mut self, _name: String, _trigger: ResponsiveTrigger) {}

        /// # Errors
        ///
        /// * If the underlying renderer implementation fails to emit the event
        async fn emit_event(
            &self,
            _event_name: String,
            _event_value: Option<String>,
        ) -> Result<(), Box<dyn std::error::Error + Send + 'static>> {
            Ok(())
        }

        /// # Errors
        ///
        /// * If the underlying renderer implementation fails to render the view
        async fn render(
            &self,
            _view: View,
        ) -> Result<(), Box<dyn std::error::Error + Send + 'static>> {
            Ok(())
        }
    }

    #[async_trait]
    impl Generator for StubRenderer {
        /// No-op generator for stub renderer (used for testing).
        ///
        /// # Errors
        ///
        /// * This implementation never returns an error
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
        /// No-op cleaner for stub renderer (used for testing).
        ///
        /// # Errors
        ///
        /// * This implementation never returns an error
        async fn clean(&self, _output: Option<String>) -> Result<(), Error> {
            Ok(())
        }
    }

    /// No-op stub runner for testing.
    pub struct StubRunner;

    impl RenderRunner for StubRunner {
        /// No-op run method that immediately returns success.
        ///
        /// # Errors
        ///
        /// * This implementation never returns an error
        fn run(&mut self) -> Result<(), Box<dyn std::error::Error + Send + 'static>> {
            Ok(())
        }
    }

    impl ToRenderRunner for StubRenderer {
        /// Converts the stub renderer to a runner.
        ///
        /// # Errors
        ///
        /// * Infallible (always succeeds for stub renderer)
        fn to_runner(
            self,
            _handle: Handle,
        ) -> Result<Box<dyn hyperchad_renderer::RenderRunner>, Box<dyn std::error::Error + Send>>
        {
            Ok(Box::new(StubRunner))
        }
    }

    impl AppBuilder {
        /// Builds an `App` with a stub renderer.
        ///
        /// # Errors
        ///
        /// * [`BuilderError::MissingRouter`] if the `AppBuilder` is missing a router
        pub fn build_stub(self, renderer: StubRenderer) -> Result<App<StubRenderer>, BuilderError> {
            log::debug!("build_stub");

            self.build(renderer)
        }

        /// Builds an `App` with a default stub renderer configuration.
        ///
        /// # Errors
        ///
        /// * [`BuilderError::MissingRouter`] if the `AppBuilder` is missing a router
        pub fn build_default_stub(self) -> Result<App<StubRenderer>, BuilderError> {
            log::debug!("build_default_stub");

            let renderer = StubRenderer;

            self.build(renderer)
        }
    }
}

impl AppBuilder {
    /// Builds an `App` with the default renderer based on enabled features.
    ///
    /// # Errors
    ///
    /// * [`crate::BuilderError::MissingRouter`] if the `AppBuilder` is missing a router
    #[cfg(feature = "egui")]
    pub fn build_default(self) -> Result<crate::App<DefaultRenderer>, crate::BuilderError> {
        self.build_default_egui()
    }

    /// Builds an `App` with the default renderer based on enabled features.
    ///
    /// # Errors
    ///
    /// * [`crate::BuilderError::MissingRouter`] if the `AppBuilder` is missing a router
    #[cfg(all(feature = "fltk", not(feature = "egui")))]
    pub fn build_default(self) -> Result<crate::App<DefaultRenderer>, crate::BuilderError> {
        self.build_default_fltk()
    }

    /// Builds an `App` with the default renderer based on enabled features.
    ///
    /// # Errors
    ///
    /// * [`crate::BuilderError::MissingRouter`] if the `AppBuilder` is missing a router
    #[cfg(all(
        feature = "actix",
        feature = "vanilla-js",
        not(any(feature = "egui", feature = "fltk"))
    ))]
    pub fn build_default(self) -> Result<crate::App<DefaultRenderer>, crate::BuilderError> {
        self.build_default_html_vanilla_js_actix()
    }

    /// Builds an `App` with the default renderer based on enabled features.
    ///
    /// # Errors
    ///
    /// * [`crate::BuilderError::MissingRouter`] if the `AppBuilder` is missing a router
    #[cfg(all(
        feature = "lambda",
        feature = "vanilla-js",
        not(any(feature = "egui", feature = "fltk", feature = "actix"))
    ))]
    pub fn build_default(self) -> Result<crate::App<DefaultRenderer>, crate::BuilderError> {
        self.build_default_html_vanilla_js_lambda()
    }

    /// Builds an `App` with the default renderer based on enabled features.
    ///
    /// # Errors
    ///
    /// * [`crate::BuilderError::MissingRouter`] if the `AppBuilder` is missing a router
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

    /// Builds an `App` with the default renderer based on enabled features.
    ///
    /// # Errors
    ///
    /// * [`crate::BuilderError::MissingRouter`] if the `AppBuilder` is missing a router
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

    /// Builds an `App` with the default renderer based on enabled features.
    ///
    /// # Errors
    ///
    /// * [`crate::BuilderError::MissingRouter`] if the `AppBuilder` is missing a router
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

    /// Builds an `App` with the default renderer based on enabled features.
    ///
    /// # Errors
    ///
    /// * [`crate::BuilderError::MissingRouter`] if the `AppBuilder` is missing a router
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

    /// Builds an `App` with the default renderer based on enabled features.
    ///
    /// # Errors
    ///
    /// * [`crate::BuilderError::MissingRouter`] if the `AppBuilder` is missing a router
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
