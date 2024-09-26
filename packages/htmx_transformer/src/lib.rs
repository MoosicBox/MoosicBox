#![cfg_attr(feature = "fail-on-warnings", deny(warnings))]

#[cfg(feature = "html")]
pub mod html;

#[derive(Copy, Clone, Debug)]
pub enum LayoutDirection {
    Row,
    Column,
}

#[derive(Clone, Debug)]
pub struct ElementList(Vec<Element>);

impl std::ops::Deref for ElementList {
    type Target = [Element];

    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

#[derive(Clone, Debug)]
pub enum Element {
    Raw {
        value: String,
    },
    Div {
        elements: Vec<Element>,
    },
    Aside {
        elements: Vec<Element>,
    },
    Main {
        elements: Vec<Element>,
    },
    Header {
        elements: Vec<Element>,
    },
    Footer {
        elements: Vec<Element>,
    },
    Section {
        elements: Vec<Element>,
    },
    Form {
        elements: Vec<Element>,
    },
    Span {
        elements: Vec<Element>,
    },
    Input(Input),
    Button {
        elements: Vec<Element>,
    },
    Image {
        source: Option<String>,
    },
    Anchor {
        elements: Vec<Element>,
    },
    Heading {
        elements: Vec<Element>,
        size: HeaderSize,
    },
}

#[derive(Copy, Clone, Debug)]
pub enum HeaderSize {
    H1,
    H2,
    H3,
    H4,
    H5,
    H6,
}

#[derive(Clone, Debug)]
pub enum Input {
    Text { value: String, placeholder: String },
    Password { value: String, placeholder: String },
}
