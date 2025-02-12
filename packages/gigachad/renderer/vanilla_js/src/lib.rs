#![cfg_attr(feature = "fail-on-warnings", deny(warnings))]
#![warn(clippy::all, clippy::pedantic, clippy::nursery, clippy::cargo)]

use std::{collections::HashMap, io::Write};

use gigachad_renderer::{Color, HtmlTagRenderer};
use gigachad_renderer_html::{html::write_attr, DefaultHtmlTagRenderer};
use gigachad_transformer::{models::Route, Container, ResponsiveTrigger};
use maud::{html, PreEscaped};

#[derive(Default, Clone)]
pub struct VanillaJsTagRenderer {
    default: DefaultHtmlTagRenderer,
}

#[cfg(debug_assertions)]
pub static SCRIPT_NAME: &str = "gigachad.js";
#[cfg(all(debug_assertions, feature = "script"))]
pub static SCRIPT: &str = include_str!("../web/dist/index.js");

#[cfg(not(debug_assertions))]
pub static SCRIPT_NAME: &str = "gigachad.min.js";
#[cfg(all(not(debug_assertions), feature = "script"))]
pub static SCRIPT: &str = include_str!("../web/dist/index.min.js");

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
                            write_attr(f, b"hx-swap", b"innerHTML")?;
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

    fn root_html(
        &self,
        headers: &HashMap<String, String>,
        container: &Container,
        content: String,
        viewport: Option<&str>,
        background: Option<Color>,
    ) -> String {
        if headers.get("hx-request").is_some() {
            content
        } else {
            let background = background.map(|x| format!("background:rgb({},{},{})", x.r, x.g, x.b));
            let background = background.as_deref().unwrap_or("");

            let mut responsive_css = vec![];
            self.default
                .reactive_conditions_to_css(&mut responsive_css, container)
                .unwrap();
            let responsive_css = std::str::from_utf8(&responsive_css).unwrap();

            html! {
                html {
                    head {
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
                        script src={"/js/"(SCRIPT_NAME)} {}
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
}
