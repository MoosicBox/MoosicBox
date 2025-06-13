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
                    if let ImplItem::Fn(ImplItemFn { sig, attrs, .. }) = member {
                        if sig.asyncness.is_some() && !has_inject_attr(attrs) {
                            let name = &sig.ident;
                            self.warnings.push(format!(
                                "{}: async method `{}` in impl is missing #[inject_yields]",
                                self.file, name
                            ));
                        }
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
