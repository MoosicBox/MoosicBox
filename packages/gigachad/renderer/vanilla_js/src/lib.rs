#![cfg_attr(feature = "fail-on-warnings", deny(warnings))]
#![warn(clippy::all, clippy::pedantic, clippy::nursery, clippy::cargo)]

use std::{collections::HashMap, io::Write};

use const_format::concatcp;
use gigachad_renderer::{Color, HtmlTagRenderer};
use gigachad_renderer_html::{html::write_attr, DefaultHtmlTagRenderer};
use gigachad_transformer::{models::Route, Container, ResponsiveTrigger};
use maud::{html, PreEscaped};

#[derive(Default, Clone)]
pub struct VanillaJsTagRenderer {
    default: DefaultHtmlTagRenderer,
}

const SCRIPT_NAME_STEM: &str = "gigachad";
#[cfg(debug_assertions)]
const SCRIPT_NAME_EXTENSION: &str = "js";
#[cfg(not(debug_assertions))]
const SCRIPT_NAME_EXTENSION: &str = "min.js";

pub const SCRIPT_NAME: &str = concatcp!(SCRIPT_NAME_STEM, ".", SCRIPT_NAME_EXTENSION);

#[cfg(all(debug_assertions, feature = "script"))]
pub const SCRIPT: &str = include_str!("../web/dist/index.js");

#[cfg(all(not(debug_assertions), feature = "script"))]
pub const SCRIPT: &str = include_str!("../web/dist/index.min.js");

#[cfg(all(feature = "hash", feature = "script"))]
pub static SCRIPT_NAME_HASHED: std::sync::LazyLock<String> = std::sync::LazyLock::new(|| {
    let digest = md5::compute(SCRIPT.as_bytes());
    let digest = format!("{digest:x}");
    let hash = &digest[..10];
    format!("{SCRIPT_NAME_STEM}-{hash}.{SCRIPT_NAME_EXTENSION}")
});

impl HtmlTagRenderer for VanillaJsTagRenderer {
    fn add_responsive_trigger(&mut self, name: String, trigger: ResponsiveTrigger) {
        self.default.responsive_triggers.insert(name, trigger);
    }

    fn element_attrs_to_html(
        &self,
        f: &mut dyn Write,
        container: &Container,
        is_flex_child: bool,
    ) -> Result<(), std::io::Error> {
        self.default
            .element_attrs_to_html(f, container, is_flex_child)?;

        if let Some(route) = &container.route {
            match route {
                Route::Get {
                    route,
                    trigger,
                    swap,
                } => {
                    match swap {
                        gigachad_transformer::models::SwapTarget::This => {
                            write_attr(f, b"hx-swap", b"outerHTML")?;
                        }
                        gigachad_transformer::models::SwapTarget::Children => {
                            write_attr(f, b"hx-swap", b"in 'nerHTML")?;
                        }
                    }
                    write_attr(f, b"hx-get", route.as_bytes())?;
                    if let Some(trigger) = trigger {
                        write_attr(f, b"hx-trigger", trigger.as_bytes())?;
                    }
                }
                Route::Post {
                    route,
                    trigger,
                    swap,
                } => {
                    match swap {
                        gigachad_transformer::models::SwapTarget::This => {
                            write_attr(f, b"hx-swap", b"outerHTML")?;
                        }
                        gigachad_transformer::models::SwapTarget::Children => {
                            write_attr(f, b"hx-swap", b"innerHTML")?;
                        }
                    }
                    write_attr(f, b"hx-swap", b"outerHTML")?;
                    write_attr(f, b"hx-post", route.as_bytes())?;
                    if let Some(trigger) = trigger {
                        write_attr(f, b"hx-trigger", trigger.as_bytes())?;
                    }
                }
            }
        }

        Ok(())
    }

    fn partial_html(
        &self,
        _headers: &HashMap<String, String>,
        container: &Container,
        content: String,
        _viewport: Option<&str>,
        _background: Option<Color>,
    ) -> String {
        let mut responsive_css = vec![];
        self.default
            .reactive_conditions_to_css(&mut responsive_css, container)
            .unwrap();
        let responsive_css = std::str::from_utf8(&responsive_css).unwrap();

        format!("{responsive_css}\n\n{content}")
    }

    fn root_html(
        &self,
        _headers: &HashMap<String, String>,
        container: &Container,
        content: String,
        viewport: Option<&str>,
        background: Option<Color>,
        title: Option<&str>,
    ) -> String {
        let mut responsive_css = vec![];
        self.default
            .reactive_conditions_to_css(&mut responsive_css, container)
            .unwrap();
        let responsive_css = std::str::from_utf8(&responsive_css).unwrap();

        let background = background.map(|x| format!("background:rgb({},{},{})", x.r, x.g, x.b));
        let background = background.as_deref().unwrap_or("");

        #[cfg(all(feature = "hash", feature = "script"))]
        let script = html! { script src={"/js/"(SCRIPT_NAME_HASHED.as_str())} {} };
        #[cfg(not(all(feature = "hash", feature = "script")))]
        let script = html! { script src={"/js/"(SCRIPT_NAME)} {} };

        html! {
            html lang="en" {
                head {
                    @if let Some(title) = title {
                        title { (title) }
                    }
                    style {(format!(r"
                            body {{
                                margin: 0;{background};
                                overflow: hidden;
                            }}
                            .remove-button-styles {{
                                background: none;
                                color: inherit;
                                border: none;
                                padding: 0;
                                font: inherit;
                                cursor: pointer;
                                outline: inherit;
                            }}
                        "))}
                    (script)
                    (PreEscaped(responsive_css))
                    @if let Some(content) = viewport {
                        meta name="viewport" content=(content);
                    }
                }
                body {
                    (PreEscaped(content))
                }
            }
        }
        .into_string()
    }
}
