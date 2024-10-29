#![allow(clippy::module_name_repetitions)]

use maud::{html, Markup};

use crate::page;

#[must_use]
pub fn settings_page_content() -> Markup {
    html! {
        input type="text" value="hey";
    }
}

#[must_use]
pub fn settings() -> Markup {
    page(&settings_page_content())
}
