//! Static asset helpers for docs sites.

/// Return the embedded `HyperChad` vanilla JavaScript runtime asset route.
#[cfg(all(feature = "assets", feature = "vanilla-js"))]
#[must_use]
pub fn vanilla_js_route() -> hyperchad::renderer::assets::StaticAssetRoute {
    hyperchad::renderer::assets::StaticAssetRoute {
        route: format!(
            "js/{}",
            hyperchad::renderer_vanilla_js::SCRIPT_NAME_HASHED.as_str()
        ),
        target: hyperchad::renderer::assets::AssetPathTarget::FileContents(
            hyperchad::renderer_vanilla_js::SCRIPT.as_bytes().into(),
        ),
        not_found_behavior: None,
    }
}

/// Return a static directory asset route.
///
/// # Panics
///
/// Panics if `path` cannot be converted into a `HyperChad` asset path target.
#[cfg(feature = "assets")]
#[must_use]
pub fn public_dir_route(
    route: impl Into<String>,
    path: impl Into<std::path::PathBuf>,
) -> hyperchad::renderer::assets::StaticAssetRoute {
    hyperchad::renderer::assets::StaticAssetRoute {
        route: route.into(),
        target: path.into().try_into().unwrap(),
        not_found_behavior: None,
    }
}
