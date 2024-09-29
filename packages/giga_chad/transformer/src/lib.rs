#![cfg_attr(feature = "fail-on-warnings", deny(warnings))]

#[cfg(feature = "html")]
pub mod html;

#[derive(Copy, Clone, Debug)]
pub enum Number {
    Real(f32),
    Integer(u64),
    RealPercent(f32),
    IntegerPercent(u64),
}

impl Default for Number {
    fn default() -> Self {
        Number::Integer(0)
    }
}

#[derive(Copy, Clone, Debug)]
pub enum LayoutDirection {
    Row,
    Column,
}

#[derive(Clone, Debug, Default)]
pub struct ElementList(Vec<Element>);

impl std::ops::Deref for ElementList {
    type Target = [Element];

    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

#[derive(Clone, Debug)]
pub struct ContainerElement {
    pub elements: Vec<Element>,
    pub direction: LayoutDirection,
    pub width: Option<Number>,
    pub height: Option<Number>,
}

#[derive(Clone, Debug)]
pub enum Element {
    Raw {
        value: String,
    },
    Div {
        element: ContainerElement,
    },
    Aside {
        element: ContainerElement,
    },
    Main {
        element: ContainerElement,
    },
    Header {
        element: ContainerElement,
    },
    Footer {
        element: ContainerElement,
    },
    Section {
        element: ContainerElement,
    },
    Form {
        element: ContainerElement,
    },
    Span {
        element: ContainerElement,
    },
    Input(Input),
    Button {
        element: ContainerElement,
    },
    Image {
        source: Option<String>,
        width: Option<Number>,
        height: Option<Number>,
    },
    Anchor {
        element: ContainerElement,
        href: Option<String>,
    },
    Heading {
        element: ContainerElement,
        size: HeaderSize,
    },
    Ul {
        element: ContainerElement,
    },
    Ol {
        element: ContainerElement,
    },
    Li {
        element: ContainerElement,
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
