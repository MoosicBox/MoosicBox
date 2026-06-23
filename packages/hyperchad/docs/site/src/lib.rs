#![cfg_attr(feature = "fail-on-warnings", deny(warnings))]
#![warn(clippy::all, clippy::pedantic, clippy::nursery, clippy::cargo)]
#![allow(clippy::multiple_crate_versions)]

//! Reusable `HyperChad` documentation site framework.
//!
//! ```no_run
//! use hyperchad_docs_site::{DocPage, DocsSection, DocsSite};
//!
//! fn generated_reference() -> String {
//!     "# Reference".to_string()
//! }
//!
//! static SECTIONS: &[DocsSection] = &[DocsSection::new("reference", "Reference")];
//! static PAGES: &[DocPage] = &[DocPage::generated("/docs/reference", generated_reference)
//!     .title("Reference")
//!     .nav("reference", "Reference")];
//!
//! let site = DocsSite::builder("my-project")
//!     .title("my-project docs")
//!     .description("Documentation for my-project")
//!     .sections(SECTIONS)
//!     .pages(&PAGES)
//!     .build();
//! # let _ = site;
//! ```

pub mod generated;
pub mod link_map;
pub mod registry;
pub mod site;
pub mod theme;

pub use generated::{CliReference, ConfigReference, EnvOverrideDoc, SectionHeadingStyle};
pub use registry::{DocPage, DocsSection, NavItem, NavSection, PageKind};
pub use site::{DocsSite, DocsSiteBuilder, MarkdownScan};
pub use theme::Theme;
