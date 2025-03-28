#![cfg_attr(feature = "fail-on-warnings", deny(warnings))]
#![warn(clippy::all, clippy::pedantic, clippy::nursery, clippy::cargo)]
#![allow(clippy::multiple_crate_versions)]

use std::{collections::HashMap, io::Write};

use hyperchad_renderer::{Color, HtmlTagRenderer};
use hyperchad_renderer_html::DefaultHtmlTagRenderer;
use hyperchad_router::Container;
use hyperchad_transformer::ResponsiveTrigger;
use maud::{DOCTYPE, PreEscaped, html};

#[derive(Default, Clone)]
pub struct DatastarTagRenderer {
    default: DefaultHtmlTagRenderer,
}

impl HtmlTagRenderer for DatastarTagRenderer {
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

        Ok(())
    }

    fn partial_html(
        &self,
        _headers: &HashMap<String, String>,
        _container: &Container,
        content: String,
        _viewport: Option<&str>,
        _background: Option<Color>,
    ) -> String {
        content
    }

    fn root_html(
        &self,
        _headers: &HashMap<String, String>,
        container: &Container,
        content: String,
        viewport: Option<&str>,
        background: Option<Color>,
        title: Option<&str>,
        description: Option<&str>,
    ) -> String {
        let background = background.map(|x| format!("background:rgb({},{},{})", x.r, x.g, x.b));
        let background = background.as_deref().unwrap_or("");

        let mut responsive_css = vec![];
        self.default
            .reactive_conditions_to_css(&mut responsive_css, container)
            .unwrap();
        let responsive_css = std::str::from_utf8(&responsive_css).unwrap();

        html! {
            (DOCTYPE)
            html style="height:100%" lang="en" {
                head {
                    @if let Some(title) = title {
                        title { (title) }
                    }
                    @if let Some(description) = description {
                        meta name="description" content=(description);
                    }
                    script
                        type="module"
                        src="https://cdn.jsdelivr.net/npm/@sudodevnull/datastar@0.19.9/dist/datastar.min.js"
                        defer;
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
                    (PreEscaped(responsive_css))
                    @if let Some(content) = viewport {
                        meta name="viewport" content=(content);
                    }
                }
                body style="height:100%" {
                    (PreEscaped(content))
                }
            }
        }
        .into_string()
    }
}
