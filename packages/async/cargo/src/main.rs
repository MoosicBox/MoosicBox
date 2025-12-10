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

/// Command-line interface for the `switchy_async_cargo` linter tool.
///
/// Provides options for specifying the root directory to scan for Rust source files.
#[derive(Parser)]
#[command(
    author,
    version,
    about = "Warns about async fn missing #[inject_yields] in Rust workspace"
)]
struct Cli {
    /// The root directory to scan for Rust source files.
    ///
    /// If not provided, defaults to the `CARGO_MANIFEST_DIR` environment variable.
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

    #[test_log::test]
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

    #[test_log::test]
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

    #[test_log::test]
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

    #[test_log::test]
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

    #[test_log::test]
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

    #[test_log::test]
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

    #[test_log::test]
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

    #[test_log::test]
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

    #[test_log::test]
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

    #[test_log::test]
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

    #[test_log::test]
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

    #[test_log::test]
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

    #[test_log::test]
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

    #[test_log::test]
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

    #[test_log::test]
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

    #[test_log::test]
    fn test_has_inject_attr_empty_attrs() {
        let attrs: Vec<Attribute> = vec![];
        assert!(!has_inject_attr(&attrs));
    }

    #[test_log::test]
    fn test_has_inject_attr_similar_names_not_matched() {
        // Test that similar attribute names are NOT matched
        let code = r"
            #[inject_yield]
            async fn test_inject_yield() {}

            #[yields]
            async fn test_yields() {}

            #[inject]
            async fn test_inject() {}
        ";
        let syntax = syn::parse_file(code).unwrap();
        let mut checker = Checker {
            file: "test.rs",
            warnings: Vec::new(),
        };
        checker.visit_file(&syntax);

        // All three should be flagged - none of these are "inject_yields"
        assert_eq!(checker.warnings.len(), 3);
        assert!(
            checker
                .warnings
                .iter()
                .any(|w| w.contains("test_inject_yield"))
        );
        assert!(checker.warnings.iter().any(|w| w.contains("test_yields")));
        assert!(checker.warnings.iter().any(|w| w.contains("test_inject")));
    }

    #[test_log::test]
    fn test_checker_async_fn_with_multiple_attributes() {
        // Test that inject_yields is found among multiple attributes
        let code = r#"
            #[cfg(test)]
            #[inject_yields]
            #[allow(unused)]
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

        // Should NOT be flagged since it has inject_yields
        assert_eq!(checker.warnings.len(), 0);
    }

    #[test_log::test]
    fn test_checker_async_fn_with_multiple_attributes_without_inject_yields() {
        // Test async function with multiple OTHER attributes (but not inject_yields)
        let code = r#"
            #[cfg(test)]
            #[allow(unused)]
            #[inline]
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

        // Should be flagged - no inject_yields among the attributes
        assert_eq!(checker.warnings.len(), 1);
        assert!(checker.warnings[0].contains("test_function"));
    }

    #[test_log::test]
    fn test_checker_generic_async_function() {
        // Test that generic async functions are still detected
        let code = r"
            async fn generic_fn<T, U>() where T: std::fmt::Debug {
            }
        ";
        let syntax = syn::parse_file(code).unwrap();
        let mut checker = Checker {
            file: "test.rs",
            warnings: Vec::new(),
        };
        checker.visit_file(&syntax);

        assert_eq!(checker.warnings.len(), 1);
        assert!(checker.warnings[0].contains("generic_fn"));
    }

    #[test_log::test]
    fn test_checker_generic_async_function_with_attribute() {
        // Test that generic async functions with inject_yields are exempt
        let code = r"
            #[inject_yields]
            async fn generic_fn<T, U>() where T: std::fmt::Debug {
            }
        ";
        let syntax = syn::parse_file(code).unwrap();
        let mut checker = Checker {
            file: "test.rs",
            warnings: Vec::new(),
        };
        checker.visit_file(&syntax);

        assert_eq!(checker.warnings.len(), 0);
    }

    #[test_log::test]
    fn test_checker_async_associated_function_without_self() {
        // Test async associated functions (no self parameter)
        let code = r"
            struct MyStruct;
            impl MyStruct {
                async fn new() -> Self {
                    MyStruct
                }

                async fn create(value: i32) -> Self {
                    MyStruct
                }
            }
        ";
        let syntax = syn::parse_file(code).unwrap();
        let mut checker = Checker {
            file: "test.rs",
            warnings: Vec::new(),
        };
        checker.visit_file(&syntax);

        // Both associated functions should be flagged
        assert_eq!(checker.warnings.len(), 2);
        assert!(checker.warnings.iter().any(|w| w.contains("new")));
        assert!(checker.warnings.iter().any(|w| w.contains("create")));
    }

    #[test_log::test]
    fn test_checker_impl_with_only_constants_and_types() {
        // Test impl block with only associated constants and types (no methods)
        let code = r"
            struct MyStruct;
            impl MyStruct {
                const VALUE: i32 = 42;
                type Alias = i32;
            }
        ";
        let syntax = syn::parse_file(code).unwrap();
        let mut checker = Checker {
            file: "test.rs",
            warnings: Vec::new(),
        };
        checker.visit_file(&syntax);

        // No warnings since there are no async methods
        assert_eq!(checker.warnings.len(), 0);
    }

    #[test_log::test]
    fn test_checker_empty_file() {
        // Test empty file produces no warnings
        let code = "";
        let syntax = syn::parse_file(code).unwrap();
        let mut checker = Checker {
            file: "empty.rs",
            warnings: Vec::new(),
        };
        checker.visit_file(&syntax);

        assert_eq!(checker.warnings.len(), 0);
    }

    #[test_log::test]
    fn test_checker_file_with_only_imports() {
        // Test file with only imports produces no warnings
        let code = r"
            use std::collections::HashMap;
            use std::fmt::Debug;
        ";
        let syntax = syn::parse_file(code).unwrap();
        let mut checker = Checker {
            file: "imports.rs",
            warnings: Vec::new(),
        };
        checker.visit_file(&syntax);

        assert_eq!(checker.warnings.len(), 0);
    }

    #[test_log::test]
    fn test_checker_impl_method_with_multiple_attributes() {
        // Test impl method with multiple attributes including inject_yields
        let code = r#"
            struct MyStruct;
            impl MyStruct {
                #[cfg(test)]
                #[inject_yields]
                #[allow(unused)]
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

        // Should NOT be flagged since method has inject_yields
        assert_eq!(checker.warnings.len(), 0);
    }

    #[test_log::test]
    fn test_checker_trait_definition_with_async_method() {
        // Trait definitions with async methods should NOT be flagged
        // (only implementations need inject_yields, not trait declarations)
        let code = r"
            trait MyTrait {
                async fn trait_method(&self);
            }
        ";
        let syntax = syn::parse_file(code).unwrap();
        let mut checker = Checker {
            file: "test.rs",
            warnings: Vec::new(),
        };
        checker.visit_file(&syntax);

        // Trait definitions are not checked - only impl blocks are
        assert_eq!(checker.warnings.len(), 0);
    }

    #[test_log::test]
    fn test_checker_async_block_not_flagged() {
        // Async blocks within functions are NOT async fn declarations
        // and should NOT be flagged
        let code = r#"
            fn sync_function() {
                let _future = async {
                    println!("async block");
                };
            }
        "#;
        let syntax = syn::parse_file(code).unwrap();
        let mut checker = Checker {
            file: "test.rs",
            warnings: Vec::new(),
        };
        checker.visit_file(&syntax);

        // No async fn declarations, so no warnings
        assert_eq!(checker.warnings.len(), 0);
    }

    #[test_log::test]
    fn test_checker_qualified_attribute_path_not_matched() {
        // Qualified paths like crate::inject_yields are NOT matched
        // because is_ident() only matches simple identifiers
        let code = r"
            #[other::inject_yields]
            async fn test_function() {}
        ";
        let syntax = syn::parse_file(code).unwrap();
        let mut checker = Checker {
            file: "test.rs",
            warnings: Vec::new(),
        };
        checker.visit_file(&syntax);

        // Should be flagged because qualified path is not recognized
        assert_eq!(checker.warnings.len(), 1);
        assert!(checker.warnings[0].contains("test_function"));
    }

    #[test_log::test]
    fn test_checker_generic_impl_block() {
        // Generic impl blocks should still have their async methods checked
        let code = r"
            struct Container<T>(T);

            impl<T> Container<T> {
                async fn async_method(&self) {}

                fn sync_method(&self) {}
            }
        ";
        let syntax = syn::parse_file(code).unwrap();
        let mut checker = Checker {
            file: "test.rs",
            warnings: Vec::new(),
        };
        checker.visit_file(&syntax);

        // The async method should be flagged
        assert_eq!(checker.warnings.len(), 1);
        assert!(checker.warnings[0].contains("async_method"));
    }

    #[test_log::test]
    fn test_checker_generic_impl_block_with_inject_yields() {
        // Generic impl blocks with inject_yields attribute should be exempt
        let code = r"
            struct Container<T>(T);

            #[inject_yields]
            impl<T> Container<T> {
                async fn async_method(&self) {}
            }
        ";
        let syntax = syn::parse_file(code).unwrap();
        let mut checker = Checker {
            file: "test.rs",
            warnings: Vec::new(),
        };
        checker.visit_file(&syntax);

        assert_eq!(checker.warnings.len(), 0);
    }

    #[test_log::test]
    fn test_checker_file_module_declaration() {
        // External module declarations (mod foo;) should not cause issues
        let code = r"
            mod external_module;

            async fn local_async_fn() {}
        ";
        let syntax = syn::parse_file(code).unwrap();
        let mut checker = Checker {
            file: "test.rs",
            warnings: Vec::new(),
        };
        checker.visit_file(&syntax);

        // Only the local async fn should be flagged, not the module declaration
        assert_eq!(checker.warnings.len(), 1);
        assert!(checker.warnings[0].contains("local_async_fn"));
    }

    #[test_log::test]
    fn test_checker_async_closure_not_flagged() {
        // Async closures (async || {}) are NOT async fn declarations
        let code = r#"
            fn wrapper() {
                let _closure = async || {
                    println!("async closure");
                };
            }
        "#;
        let syntax = syn::parse_file(code).unwrap();
        let mut checker = Checker {
            file: "test.rs",
            warnings: Vec::new(),
        };
        checker.visit_file(&syntax);

        // No async fn declarations, so no warnings
        assert_eq!(checker.warnings.len(), 0);
    }

    #[test_log::test]
    fn test_checker_lifetime_impl_block() {
        // Impl blocks with lifetime parameters should be checked correctly
        let code = r"
            struct Borrowed<'a>(&'a str);

            impl<'a> Borrowed<'a> {
                async fn process(&self) -> &str {
                    self.0
                }
            }
        ";
        let syntax = syn::parse_file(code).unwrap();
        let mut checker = Checker {
            file: "test.rs",
            warnings: Vec::new(),
        };
        checker.visit_file(&syntax);

        assert_eq!(checker.warnings.len(), 1);
        assert!(checker.warnings[0].contains("process"));
    }

    #[test_log::test]
    fn test_checker_where_clause_impl_block() {
        // Impl blocks with where clauses should be checked correctly
        let code = r"
            struct Generic<T>(T);

            impl<T> Generic<T>
            where
                T: std::fmt::Debug + Send,
            {
                async fn debug_value(&self) {}
            }
        ";
        let syntax = syn::parse_file(code).unwrap();
        let mut checker = Checker {
            file: "test.rs",
            warnings: Vec::new(),
        };
        checker.visit_file(&syntax);

        assert_eq!(checker.warnings.len(), 1);
        assert!(checker.warnings[0].contains("debug_value"));
    }

    #[test_log::test]
    fn test_checker_multiple_impl_blocks_for_same_type() {
        // Multiple impl blocks for the same type
        let code = r"
            struct MyStruct;

            impl MyStruct {
                async fn method_a(&self) {}
            }

            #[inject_yields]
            impl MyStruct {
                async fn method_b(&self) {}
            }

            impl MyStruct {
                async fn method_c(&self) {}
            }
        ";
        let syntax = syn::parse_file(code).unwrap();
        let mut checker = Checker {
            file: "test.rs",
            warnings: Vec::new(),
        };
        checker.visit_file(&syntax);

        // method_a and method_c should be flagged, method_b exempt due to impl attribute
        assert_eq!(checker.warnings.len(), 2);
        assert!(checker.warnings.iter().any(|w| w.contains("method_a")));
        assert!(checker.warnings.iter().any(|w| w.contains("method_c")));
        assert!(!checker.warnings.iter().any(|w| w.contains("method_b")));
    }

    #[test_log::test]
    fn test_checker_async_unsafe_fn() {
        // Async unsafe functions should still be detected
        let code = r"
            async unsafe fn async_unsafe() {}
        ";
        let syntax = syn::parse_file(code).unwrap();
        let mut checker = Checker {
            file: "test.rs",
            warnings: Vec::new(),
        };
        checker.visit_file(&syntax);

        assert_eq!(checker.warnings.len(), 1);
        assert!(checker.warnings[0].contains("async_unsafe"));
    }

    #[test_log::test]
    fn test_checker_async_unsafe_fn_with_attribute() {
        // Async unsafe functions with inject_yields should be exempt
        let code = r"
            #[inject_yields]
            async unsafe fn async_unsafe() {}
        ";
        let syntax = syn::parse_file(code).unwrap();
        let mut checker = Checker {
            file: "test.rs",
            warnings: Vec::new(),
        };
        checker.visit_file(&syntax);

        assert_eq!(checker.warnings.len(), 0);
    }

    #[test_log::test]
    fn test_checker_module_with_attribute_exempts_impl_blocks() {
        // Module-level inject_yields should exempt impl blocks inside
        let code = r"
            #[inject_yields]
            mod my_module {
                struct MyStruct;

                impl MyStruct {
                    async fn method(&self) {}
                }
            }
        ";
        let syntax = syn::parse_file(code).unwrap();
        let mut checker = Checker {
            file: "test.rs",
            warnings: Vec::new(),
        };
        checker.visit_file(&syntax);

        // The module has inject_yields, so everything inside is exempt
        assert_eq!(checker.warnings.len(), 0);
    }

    #[test_log::test]
    fn test_checker_nested_module_without_attribute() {
        // Nested module without attribute inside module without attribute
        let code = r"
            mod outer {
                mod inner {
                    async fn inner_fn() {}
                }
                async fn outer_fn() {}
            }
        ";
        let syntax = syn::parse_file(code).unwrap();
        let mut checker = Checker {
            file: "test.rs",
            warnings: Vec::new(),
        };
        checker.visit_file(&syntax);

        // Both async fns should be flagged
        assert_eq!(checker.warnings.len(), 2);
        assert!(checker.warnings.iter().any(|w| w.contains("inner_fn")));
        assert!(checker.warnings.iter().any(|w| w.contains("outer_fn")));
    }

    #[test_log::test]
    fn test_checker_outer_module_with_attribute_exempts_all_nested() {
        // Outer module with attribute exempts all nested content
        let code = r"
            #[inject_yields]
            mod outer {
                mod inner {
                    async fn inner_fn() {}

                    struct InnerStruct;
                    impl InnerStruct {
                        async fn method(&self) {}
                    }
                }
                async fn outer_fn() {}
            }
        ";
        let syntax = syn::parse_file(code).unwrap();
        let mut checker = Checker {
            file: "test.rs",
            warnings: Vec::new(),
        };
        checker.visit_file(&syntax);

        // All exempt due to outer module attribute
        assert_eq!(checker.warnings.len(), 0);
    }

    #[test_log::test]
    fn test_checker_macro_rules_in_file() {
        // Macro definitions should not cause issues or false positives
        let code = r"
            macro_rules! my_macro {
                () => {
                    async fn generated() {}
                };
            }

            async fn real_async_fn() {}
        ";
        let syntax = syn::parse_file(code).unwrap();
        let mut checker = Checker {
            file: "test.rs",
            warnings: Vec::new(),
        };
        checker.visit_file(&syntax);

        // Only the real async fn should be flagged (macro body is not expanded)
        assert_eq!(checker.warnings.len(), 1);
        assert!(checker.warnings[0].contains("real_async_fn"));
    }

    #[test_log::test]
    fn test_checker_extern_block_no_false_positives() {
        // Extern blocks should not cause any warnings
        let code = r#"
            extern "C" {
                fn external_function();
            }

            async fn local_async_fn() {}
        "#;
        let syntax = syn::parse_file(code).unwrap();
        let mut checker = Checker {
            file: "test.rs",
            warnings: Vec::new(),
        };
        checker.visit_file(&syntax);

        // Only the local async fn should be flagged
        assert_eq!(checker.warnings.len(), 1);
        assert!(checker.warnings[0].contains("local_async_fn"));
    }

    #[test_log::test]
    fn test_checker_pub_async_fn() {
        // Public async functions should be detected the same as private ones
        let code = r"
            pub async fn public_async() {}
            pub(crate) async fn crate_async() {}
            pub(super) async fn super_async() {}
        ";
        let syntax = syn::parse_file(code).unwrap();
        let mut checker = Checker {
            file: "test.rs",
            warnings: Vec::new(),
        };
        checker.visit_file(&syntax);

        // All three should be flagged
        assert_eq!(checker.warnings.len(), 3);
        assert!(checker.warnings.iter().any(|w| w.contains("public_async")));
        assert!(checker.warnings.iter().any(|w| w.contains("crate_async")));
        assert!(checker.warnings.iter().any(|w| w.contains("super_async")));
    }

    #[test_log::test]
    fn test_checker_async_fn_with_explicit_return_type() {
        // Async functions with explicit return types should be detected
        let code = r"
            async fn returns_result() -> Result<i32, String> {
                Ok(42)
            }

            async fn returns_option() -> Option<i32> {
                Some(42)
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
                .any(|w| w.contains("returns_result"))
        );
        assert!(
            checker
                .warnings
                .iter()
                .any(|w| w.contains("returns_option"))
        );
    }

    #[test_log::test]
    fn test_checker_impl_for_generic_with_trait_bounds() {
        // Impl block for a type with complex trait bounds
        let code = r"
            struct Wrapper<T>(T);

            impl<T: Clone + Send + 'static> Wrapper<T> {
                async fn bounded_method(&self) {}
            }
        ";
        let syntax = syn::parse_file(code).unwrap();
        let mut checker = Checker {
            file: "test.rs",
            warnings: Vec::new(),
        };
        checker.visit_file(&syntax);

        assert_eq!(checker.warnings.len(), 1);
        assert!(checker.warnings[0].contains("bounded_method"));
    }

    #[test_log::test]
    fn test_checker_async_method_receiver_variations() {
        // Different receiver types (&self, &mut self, self, etc.)
        let code = r"
            struct MyStruct;

            impl MyStruct {
                async fn by_ref(&self) {}
                async fn by_mut_ref(&mut self) {}
                async fn by_value(self) {}
                async fn by_box(self: Box<Self>) {}
            }
        ";
        let syntax = syn::parse_file(code).unwrap();
        let mut checker = Checker {
            file: "test.rs",
            warnings: Vec::new(),
        };
        checker.visit_file(&syntax);

        // All four methods should be flagged
        assert_eq!(checker.warnings.len(), 4);
        assert!(checker.warnings.iter().any(|w| w.contains("by_ref")));
        assert!(checker.warnings.iter().any(|w| w.contains("by_mut_ref")));
        assert!(checker.warnings.iter().any(|w| w.contains("by_value")));
        assert!(checker.warnings.iter().any(|w| w.contains("by_box")));
    }
}
