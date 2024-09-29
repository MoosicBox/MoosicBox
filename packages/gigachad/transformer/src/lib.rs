#![cfg_attr(feature = "fail-on-warnings", deny(warnings))]
#![warn(clippy::all, clippy::pedantic, clippy::nursery, clippy::cargo)]

#[cfg(feature = "calc")]
pub mod calc;
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
        Self::Integer(0)
    }
}

#[derive(Copy, Clone, Debug, Default)]
pub enum LayoutDirection {
    Row,
    #[default]
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

impl std::ops::DerefMut for ElementList {
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.0
    }
}

#[derive(Clone, Debug, Default)]
pub struct ContainerElement {
    pub elements: Vec<Element>,
    pub direction: LayoutDirection,
    pub width: Option<Number>,
    pub height: Option<Number>,
    #[cfg(feature = "calc")]
    pub calculated_width: Option<f32>,
    #[cfg(feature = "calc")]
    pub calculated_height: Option<f32>,
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

impl Element {
    #[must_use]
    pub const fn container_element(&self) -> Option<&ContainerElement> {
        match self {
            Self::Div { element }
            | Self::Aside { element }
            | Self::Main { element }
            | Self::Header { element }
            | Self::Footer { element }
            | Self::Section { element }
            | Self::Form { element }
            | Self::Span { element }
            | Self::Button { element }
            | Self::Anchor { element, .. }
            | Self::Heading { element, .. }
            | Self::Ul { element }
            | Self::Ol { element }
            | Self::Li { element } => Some(element),
            Self::Raw { .. } | Self::Image { .. } | Self::Input(_) => None,
        }
    }

    pub fn container_element_mut(&mut self) -> Option<&mut ContainerElement> {
        match self {
            Self::Div { element }
            | Self::Aside { element }
            | Self::Main { element }
            | Self::Header { element }
            | Self::Footer { element }
            | Self::Section { element }
            | Self::Form { element }
            | Self::Span { element }
            | Self::Button { element }
            | Self::Anchor { element, .. }
            | Self::Heading { element, .. }
            | Self::Ul { element }
            | Self::Ol { element }
            | Self::Li { element } => Some(element),
            Self::Raw { .. } | Self::Image { .. } | Self::Input(_) => None,
        }
    }
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
