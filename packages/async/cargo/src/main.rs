//! A command-line tool for detecting missing `#[inject_yields]` attributes on async functions.
//!
//! This tool scans Rust source files in a workspace and warns about async functions
//! and methods that are missing the `#[inject_yields]` attribute. The attribute is
//! important for simulation testing, enabling deterministic async testing with controlled
//! execution yield points.
//!
//! # Usage
//!
//! ```bash
//! # Check current workspace
//! switchy_async_cargo
//!
//! # Check specific directory
//! switchy_async_cargo --root /path/to/project
//! ```
//!
//! # Exit Codes
//!
//! * 0 - Success, no warnings found
//! * 1 - Warnings found, missing `#[inject_yields]` attributes detected
//!
//! # Exemptions
//!
//! Functions and methods are exempt from the check if they have `#[inject_yields]`
//! at the function level, or if their containing impl block or module has the attribute.

#![cfg_attr(feature = "fail-on-warnings", deny(warnings))]
#![warn(clippy::all, clippy::pedantic, clippy::nursery, clippy::cargo)]
#![allow(clippy::multiple_crate_versions)]

use clap::Parser;
use std::{env, fs, process};
use syn::{Attribute, File, ImplItem, ImplItemFn, Item, visit::Visit};
use walkdir::WalkDir;

#[derive(Parser)]
#[command(
    author,
    version,
    about = "Warns about async fn missing #[inject_yields] in Rust workspace"
)]
struct Cli {
    #[arg(long)]
    root: Option<String>,
}

fn has_inject_attr(attrs: &[Attribute]) -> bool {
    attrs.iter().any(|a| a.path().is_ident("inject_yields"))
}

struct Checker<'a> {
    file: &'a str,
    warnings: Vec<String>,
}

impl<'ast> Visit<'ast> for Checker<'_> {
    fn visit_item(&mut self, item: &'ast Item) {
        let skip = match item {
            Item::Fn(func) => has_inject_attr(&func.attrs),
            Item::Impl(item_impl) => has_inject_attr(&item_impl.attrs),
            Item::Mod(item_mod) => has_inject_attr(&item_mod.attrs),
            _ => false,
        };
        if skip {
            return;
        }

        match item {
            Item::Fn(func) if func.sig.asyncness.is_some() => {
                let name = &func.sig.ident;
                self.warnings.push(format!(
                    "{}: async fn `{}` is missing #[inject_yields]",
                    self.file, name
                ));
            }
            Item::Impl(item_impl) => {
                for member in &item_impl.items {
                    if let ImplItem::Fn(ImplItemFn { sig, attrs, .. }) = member
                        && sig.asyncness.is_some()
                        && !has_inject_attr(attrs)
                    {
                        let name = &sig.ident;
                        self.warnings.push(format!(
                            "{}: async method `{}` in impl is missing #[inject_yields]",
                            self.file, name
                        ));
                    }
                }
            }
            _ => {}
        }
        syn::visit::visit_item(self, item);
    }
}

fn main() {
    pretty_env_logger::init();

    let cli = Cli::parse();
    let root = cli
        .root
        .unwrap_or_else(|| env::var("CARGO_MANIFEST_DIR").unwrap());
    let src_dir = format!("{root}/src");

    let mut warnings = vec![];
    for entry in WalkDir::new(&src_dir)
        .into_iter()
        .filter_map(Result::ok)
        .filter(|e| e.file_type().is_file())
        .filter(|e| e.path().extension().and_then(|s| s.to_str()) == Some("rs"))
    {
        let path = entry.path();
        let content = fs::read_to_string(path).unwrap_or_default();
        let syntax = syn::parse_file(&content).unwrap_or_else(|_| File {
            attrs: vec![],
            items: vec![],
            shebang: None,
        });
        let file_str = path.display().to_string();
        let mut checker = Checker {
            file: &file_str,
            warnings: Vec::new(),
        };
        checker.visit_file(&syntax);
        warnings.extend(checker.warnings);
    }
    if !warnings.is_empty() {
        println!(
            "{} warning{}:",
            warnings.len(),
            if warnings.len() == 1 { "" } else { "s" }
        );
        for w in warnings {
            println!("warning: {w}");
        }
        process::exit(1);
    }
}
