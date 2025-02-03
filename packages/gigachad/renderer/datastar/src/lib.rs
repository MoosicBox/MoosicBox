#![cfg_attr(feature = "fail-on-warnings", deny(warnings))]
#![warn(clippy::all, clippy::pedantic, clippy::nursery, clippy::cargo)]

use std::{collections::HashMap, io::Write};

use gigachad_renderer::{Color, HtmlTagRenderer};
use gigachad_renderer_html::html::{element_classes_to_html, element_style_to_html};
use gigachad_router::Container;

pub struct DatastarTagRenderer;

impl HtmlTagRenderer for DatastarTagRenderer {
    fn element_attrs_to_html(
        &self,
        f: &mut dyn Write,
        container: &Container,
        is_flex_child: bool,
    ) -> Result<(), std::io::Error> {
        if let Some(id) = &container.str_id {
            f.write_all(b" id=\"")?;
            f.write_all(id.as_bytes())?;
            f.write_all(b"\"")?;
        }

        element_style_to_html(f, container, is_flex_child)?;
        element_classes_to_html(f, container)?;

        Ok(())
    }

    fn root_html(
        &self,
        _headers: &HashMap<String, String>,
        content: String,
        background: Option<Color>,
    ) -> String {
        if false {
            content
        } else {
            format!(
                r#"
                <html>
                    <head>
                        <script
                            type="module"
                            src="https://cdn.jsdelivr.net/npm/@sudodevnull/datastar@0.19.9/dist/datastar.min.js"
                            defer
                        ></script>
                        <style>
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
                        </style>
                    </head>
                    <body>{content}</body>
                </html>
                "#,
                background = background
                    .map(|x| format!("background:rgb({},{},{})", x.r, x.g, x.b))
                    .as_deref()
                    .unwrap_or("")
            )
        }
    }
}
