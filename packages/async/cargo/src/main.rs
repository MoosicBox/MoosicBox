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

/// Checks if the given attributes contain `#[inject_yields]`.
///
/// # Returns
///
/// Returns `true` if any attribute has the path identifier "`inject_yields`", `false` otherwise.
#[must_use]
fn has_inject_attr(attrs: &[Attribute]) -> bool {
    attrs.iter().any(|a| a.path().is_ident("inject_yields"))
}

/// Visitor that walks the AST checking for async functions missing `#[inject_yields]`.
///
/// This visitor traverses items in a Rust source file and collects warnings
/// for any async function or method that lacks the `#[inject_yields]` attribute.
struct Checker<'a> {
    /// The file path being checked, used in warning messages
    file: &'a str,
    /// Accumulated warnings about missing attributes
    warnings: Vec<String>,
}

impl<'ast> Visit<'ast> for Checker<'_> {
    /// Visits each item in the AST, checking async functions for the `#[inject_yields]` attribute.
    ///
    /// Skips items that already have `#[inject_yields]` at the item, impl, or module level.
    /// Records warnings for async functions and methods that are missing the attribute.
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

/// Main entry point for the `switchy_async_cargo` tool.
///
/// Scans Rust source files in the workspace (or specified root directory) and
/// warns about async functions missing the `#[inject_yields]` attribute.
///
/// # Panics
///
/// Panics if:
/// * The `CARGO_MANIFEST_DIR` environment variable is not set when no `--root` is provided
/// * File system operations fail unexpectedly (e.g., permissions issues)
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

#[cfg(test)]
mod tests {
    use super::*;
    use syn::parse_quote;

    #[test]
    fn test_has_inject_attr_with_inject_yields() {
        let attrs: Vec<Attribute> = vec![parse_quote!(#[inject_yields])];
        assert!(has_inject_attr(&attrs));
    }

    #[test]
    fn test_has_inject_attr_without_inject_yields() {
        let attrs: Vec<Attribute> = vec![parse_quote!(#[allow(dead_code)])];
        assert!(!has_inject_attr(&attrs));
    }

    #[test]
    fn test_has_inject_attr_empty_attributes() {
        let attrs: Vec<Attribute> = vec![];
        assert!(!has_inject_attr(&attrs));
    }

    #[test]
    fn test_has_inject_attr_multiple_attributes_with_inject_yields() {
        let attrs: Vec<Attribute> = vec![
            parse_quote!(#[allow(dead_code)]),
            parse_quote!(#[inject_yields]),
            parse_quote!(#[inline]),
        ];
        assert!(has_inject_attr(&attrs));
    }

    #[test]
    fn test_has_inject_attr_similar_named_attributes() {
        let attrs: Vec<Attribute> =
            vec![parse_quote!(#[inject_something]), parse_quote!(#[yields])];
        assert!(!has_inject_attr(&attrs));
    }

    #[test]
    fn test_checker_detects_async_fn_without_attribute() {
        let code = r#"
            async fn test_function() {
                println!("test");
            }
        "#;
        let syntax = syn::parse_file(code).unwrap();
        let mut checker = Checker {
            file: "test.rs",
            warnings: Vec::new(),
        };
        checker.visit_file(&syntax);

        assert_eq!(checker.warnings.len(), 1);
        assert!(checker.warnings[0].contains("test_function"));
        assert!(checker.warnings[0].contains("missing #[inject_yields]"));
    }

    #[test]
    fn test_checker_allows_async_fn_with_attribute() {
        let code = r#"
            #[inject_yields]
            async fn test_function() {
                println!("test");
            }
        "#;
        let syntax = syn::parse_file(code).unwrap();
        let mut checker = Checker {
            file: "test.rs",
            warnings: Vec::new(),
        };
        checker.visit_file(&syntax);

        assert_eq!(checker.warnings.len(), 0);
    }

    #[test]
    fn test_checker_ignores_sync_fn() {
        let code = r#"
            fn test_function() {
                println!("test");
            }
        "#;
        let syntax = syn::parse_file(code).unwrap();
        let mut checker = Checker {
            file: "test.rs",
            warnings: Vec::new(),
        };
        checker.visit_file(&syntax);

        assert_eq!(checker.warnings.len(), 0);
    }

    #[test]
    fn test_checker_detects_async_method_in_impl_without_attribute() {
        let code = r#"
            struct MyStruct;
            impl MyStruct {
                async fn my_method(&self) {
                    println!("test");
                }
            }
        "#;
        let syntax = syn::parse_file(code).unwrap();
        let mut checker = Checker {
            file: "test.rs",
            warnings: Vec::new(),
        };
        checker.visit_file(&syntax);

        assert_eq!(checker.warnings.len(), 1);
        assert!(checker.warnings[0].contains("my_method"));
        assert!(checker.warnings[0].contains("async method"));
        assert!(checker.warnings[0].contains("missing #[inject_yields]"));
    }

    #[test]
    fn test_checker_allows_async_method_with_method_level_attribute() {
        let code = r#"
            struct MyStruct;
            impl MyStruct {
                #[inject_yields]
                async fn my_method(&self) {
                    println!("test");
                }
            }
        "#;
        let syntax = syn::parse_file(code).unwrap();
        let mut checker = Checker {
            file: "test.rs",
            warnings: Vec::new(),
        };
        checker.visit_file(&syntax);

        assert_eq!(checker.warnings.len(), 0);
    }

    #[test]
    fn test_checker_allows_async_method_with_impl_level_attribute() {
        let code = r#"
            struct MyStruct;
            #[inject_yields]
            impl MyStruct {
                async fn my_method(&self) {
                    println!("test");
                }
            }
        "#;
        let syntax = syn::parse_file(code).unwrap();
        let mut checker = Checker {
            file: "test.rs",
            warnings: Vec::new(),
        };
        checker.visit_file(&syntax);

        assert_eq!(checker.warnings.len(), 0);
    }

    #[test]
    fn test_checker_allows_functions_in_module_with_attribute() {
        let code = r#"
            #[inject_yields]
            mod my_module {
                async fn test_function() {
                    println!("test");
                }
            }
        "#;
        let syntax = syn::parse_file(code).unwrap();
        let mut checker = Checker {
            file: "test.rs",
            warnings: Vec::new(),
        };
        checker.visit_file(&syntax);

        assert_eq!(checker.warnings.len(), 0);
    }

    #[test]
    fn test_checker_detects_multiple_async_functions() {
        let code = r#"
            async fn first_function() {
                println!("first");
            }

            async fn second_function() {
                println!("second");
            }
        "#;
        let syntax = syn::parse_file(code).unwrap();
        let mut checker = Checker {
            file: "test.rs",
            warnings: Vec::new(),
        };
        checker.visit_file(&syntax);

        assert_eq!(checker.warnings.len(), 2);
        assert!(checker.warnings[0].contains("first_function"));
        assert!(checker.warnings[1].contains("second_function"));
    }

    #[test]
    fn test_checker_mixed_async_and_sync_functions() {
        let code = r#"
            async fn async_function() {
                println!("async");
            }

            fn sync_function() {
                println!("sync");
            }

            #[inject_yields]
            async fn attributed_async_function() {
                println!("attributed");
            }
        "#;
        let syntax = syn::parse_file(code).unwrap();
        let mut checker = Checker {
            file: "test.rs",
            warnings: Vec::new(),
        };
        checker.visit_file(&syntax);

        assert_eq!(checker.warnings.len(), 1);
        assert!(checker.warnings[0].contains("async_function"));
    }

    #[test]
    fn test_checker_impl_with_multiple_methods() {
        let code = r"
            struct MyStruct;
            impl MyStruct {
                async fn method1(&self) {}

                fn sync_method(&self) {}

                #[inject_yields]
                async fn method2(&self) {}

                async fn method3(&self) {}
            }
        ";
        let syntax = syn::parse_file(code).unwrap();
        let mut checker = Checker {
            file: "test.rs",
            warnings: Vec::new(),
        };
        checker.visit_file(&syntax);

        assert_eq!(checker.warnings.len(), 2);
        assert!(checker.warnings.iter().any(|w| w.contains("method1")));
        assert!(checker.warnings.iter().any(|w| w.contains("method3")));
        assert!(!checker.warnings.iter().any(|w| w.contains("method2")));
        assert!(!checker.warnings.iter().any(|w| w.contains("sync_method")));
    }

    #[test]
    fn test_checker_trait_impl_methods() {
        let code = r#"
            struct MyStruct;
            impl MyTrait for MyStruct {
                async fn trait_method(&self) {
                    println!("implementation");
                }
            }
        "#;
        let syntax = syn::parse_file(code).unwrap();
        let mut checker = Checker {
            file: "test.rs",
            warnings: Vec::new(),
        };
        checker.visit_file(&syntax);

        // The trait impl's async method should be flagged
        assert_eq!(checker.warnings.len(), 1);
        assert!(checker.warnings[0].contains("trait_method"));
    }

    #[test]
    fn test_checker_exempts_trait_impl_with_attribute() {
        let code = r#"
            struct MyStruct;
            #[inject_yields]
            impl MyTrait for MyStruct {
                async fn trait_method(&self) {
                    println!("implementation");
                }
            }
        "#;
        let syntax = syn::parse_file(code).unwrap();
        let mut checker = Checker {
            file: "test.rs",
            warnings: Vec::new(),
        };
        checker.visit_file(&syntax);

        // The impl has inject_yields attribute, so no warnings
        assert_eq!(checker.warnings.len(), 0);
    }

    #[test]
    fn test_checker_nested_items() {
        let code = r"
            mod outer {
                async fn outer_function() {}

                mod inner {
                    async fn inner_function() {}
                }
            }
        ";
        let syntax = syn::parse_file(code).unwrap();
        let mut checker = Checker {
            file: "test.rs",
            warnings: Vec::new(),
        };
        checker.visit_file(&syntax);

        assert_eq!(checker.warnings.len(), 2);
        assert!(
            checker
                .warnings
                .iter()
                .any(|w| w.contains("outer_function"))
        );
        assert!(
            checker
                .warnings
                .iter()
                .any(|w| w.contains("inner_function"))
        );
    }

    #[test]
    fn test_checker_attribute_with_arguments() {
        let code = r#"
            #[inject_yields(some_arg)]
            async fn test_function() {
                println!("test");
            }
        "#;
        let syntax = syn::parse_file(code).unwrap();
        let mut checker = Checker {
            file: "test.rs",
            warnings: Vec::new(),
        };
        checker.visit_file(&syntax);

        // Should still recognize inject_yields even with arguments
        assert_eq!(checker.warnings.len(), 0);
    }

    #[test]
    fn test_checker_warning_format_includes_filename() {
        let code = r#"
            async fn test_function() {
                println!("test");
            }
        "#;
        let syntax = syn::parse_file(code).unwrap();
        let mut checker = Checker {
            file: "path/to/file.rs",
            warnings: Vec::new(),
        };
        checker.visit_file(&syntax);

        assert_eq!(checker.warnings.len(), 1);
        assert!(checker.warnings[0].starts_with("path/to/file.rs:"));
    }
}
