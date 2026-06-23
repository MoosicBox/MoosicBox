#![cfg_attr(feature = "fail-on-warnings", deny(warnings))]
#![warn(clippy::all, clippy::pedantic, clippy::nursery, clippy::cargo)]
#![allow(clippy::multiple_crate_versions)]

//! Reusable `HyperChad` documentation site framework.

pub mod generated;
pub mod link_map;
pub mod registry;
pub mod site;
pub mod theme;

pub use registry::{DocPage, DocsSection, NavItem, NavSection, PageKind};
pub use site::{DocsSite, DocsSiteBuilder, MarkdownScan};
pub use theme::Theme;
