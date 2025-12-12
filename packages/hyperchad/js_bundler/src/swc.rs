//! JavaScript and TypeScript bundling using SWC.
//!
//! This module provides functionality to bundle JavaScript and TypeScript files
//! using SWC as the underlying bundler. It supports minification, module loading,
//! and TypeScript compilation with configurable options.

use std::{collections::BTreeMap, collections::HashMap, fs::create_dir_all, path::Path};

use anyhow::Error;
use swc_bundler::{Bundle, Bundler, Load, ModuleData, ModuleRecord, ModuleType};
use swc_common::{FileName, FilePathMapping, GLOBALS, Mark, SourceMap, Span, sync::Lrc};
use swc_ecma_ast::{
    Bool, EsVersion, Expr, IdentName, KeyValueProp, Lit, MemberExpr, MemberProp, MetaPropExpr,
    MetaPropKind, PropName, Str,
};
use swc_ecma_codegen::{
    Emitter,
    text_writer::{JsWriter, WriteJs, omit_trailing_semi},
};
use swc_ecma_loader::{
    TargetEnv,
    resolvers::{lru::CachingResolver, node::NodeModulesResolver},
};
use swc_ecma_minifier::option::{
    CompressOptions, ExtraOptions, MangleOptions, MinifyOptions, TopLevelOptions,
};
use swc_ecma_parser::{EsSyntax, Syntax, TsSyntax, parse_file_as_module, parse_file_as_program};
use swc_ecma_transforms_base::{fixer::fixer, helpers::Helpers};
use swc_ecma_transforms_typescript::strip;
use swc_ecma_visit::VisitMutWith as _;

/// Determines the syntax configuration for a file based on its extension.
///
/// Returns the appropriate parser syntax for the given file extension.
///
/// # Returns
///
/// * `Some(Syntax::Typescript(..))` for `.ts` files with decorators enabled
/// * `Some(Syntax::Es(..))` for `.js`, `.mjs`, or `.cjs` files
/// * `None` for unsupported file extensions
#[must_use]
pub fn syntax_for_extension(extension: Option<&str>) -> Option<Syntax> {
    match extension {
        Some("ts") => Some(Syntax::Typescript(TsSyntax {
            tsx: false,
            decorators: true,
            dts: false,
            no_early_errors: false,
            disallow_ambiguous_jsx_like: true,
        })),
        Some("js" | "mjs" | "cjs") => Some(Syntax::Es(EsSyntax {
            jsx: false,
            fn_bind: false,
            decorators: true,
            decorators_before_export: false,
            export_default_from: false,
            import_attributes: false,
            allow_super_outside_method: false,
            allow_return_outside_function: false,
            auto_accessors: false,
            explicit_resource_management: false,
        })),
        _ => None,
    }
}

/// Bundles a JavaScript or TypeScript file using SWC.
///
/// This function uses the SWC bundler to process the target file and its dependencies,
/// producing a single bundled output file. Optionally minifies the output.
///
/// # Panics
///
/// * Panics if the bundler fails to bundle the modules.
/// * Panics if emitting the bundled module to code fails.
/// * Panics if file I/O operations fail (creating directories or writing output).
pub fn bundle(target: &Path, out: &Path, minify: bool) {
    let globals = Box::leak(Box::default());
    let cm = Lrc::new(SourceMap::new(FilePathMapping::empty()));
    let mut bundler = Bundler::new(
        globals,
        cm.clone(),
        Loader { cm: cm.clone() },
        CachingResolver::new(
            4096,
            NodeModulesResolver::new(TargetEnv::Browser, HashMap::default(), true),
        ),
        swc_bundler::Config {
            require: false,
            disable_inliner: false,
            external_modules: vec![],
            disable_fixer: minify,
            disable_hygiene: minify,
            disable_dce: false,
            module: ModuleType::Es,
        },
        Box::new(Hook),
    );

    let mut entries = BTreeMap::new();
    entries.insert("main".to_string(), FileName::Real(target.to_path_buf()));

    let mut output_bundles = bundler.bundle(entries.into_iter().collect()).unwrap();
    println!("Bundled as {} bundles", output_bundles.len());

    if minify {
        output_bundles = output_bundles
            .into_iter()
            .map(|mut bundle| {
                GLOBALS.set(globals, || {
                    bundle.module = swc_ecma_minifier::optimize(
                        bundle.module.into(),
                        cm.clone(),
                        None,
                        None,
                        &MinifyOptions {
                            compress: Some(CompressOptions {
                                top_level: Some(TopLevelOptions { functions: true }),
                                ..Default::default()
                            }),
                            mangle: Some(MangleOptions {
                                top_level: Some(true),
                                eval: true,
                                ..Default::default()
                            }),
                            ..Default::default()
                        },
                        &ExtraOptions {
                            unresolved_mark: Mark::new(),
                            top_level_mark: Mark::new(),
                            mangle_name_cache: None,
                        },
                    )
                    .expect_module();
                    bundle.module.visit_mut_with(&mut fixer(None));
                    bundle
                })
            })
            .collect();
    }

    print_bundles(out, &cm, output_bundles, minify);
}

/// Writes bundled modules to the output file.
///
/// Emits each bundle as JavaScript code to the specified output path, creating
/// parent directories as needed.
///
/// # Panics
///
/// * Panics if emitting the module to code fails.
/// * Panics if creating parent directories fails.
/// * Panics if writing the output file fails.
fn print_bundles(out: &Path, cm: &Lrc<SourceMap>, bundles: Vec<Bundle>, minify: bool) {
    for bundled in bundles {
        let code = {
            let mut buf = vec![];

            {
                let wr = JsWriter::new(cm.clone(), "\n", &mut buf, None);
                let mut emitter = Emitter {
                    cfg: swc_ecma_codegen::Config::default().with_minify(minify),
                    cm: cm.clone(),
                    comments: None,
                    wr: if minify {
                        Box::new(omit_trailing_semi(wr)) as Box<dyn WriteJs>
                    } else {
                        Box::new(wr) as Box<dyn WriteJs>
                    },
                };

                emitter.emit_module(&bundled.module).unwrap();
            }

            String::from_utf8_lossy(&buf).to_string()
        };

        if let Some(parent) = out.parent() {
            create_dir_all(parent).unwrap();
        }
        std::fs::write(out, &code).unwrap();
        println!("Created {} ({}KiB)", out.display(), code.len() / 1024);
    }
}

/// Hook implementation for the SWC bundler.
///
/// Provides custom behavior for handling import.meta properties during bundling.
struct Hook;

impl swc_bundler::Hook for Hook {
    /// Returns import.meta properties for a module.
    ///
    /// Provides the `url` property with the module's file name and the `main` property
    /// indicating whether the module is an entry point.
    ///
    /// # Errors
    ///
    /// Returns an error if property generation fails (currently always succeeds).
    fn get_import_meta_props(
        &self,
        span: Span,
        module_record: &ModuleRecord,
    ) -> Result<Vec<KeyValueProp>, Error> {
        let file_name = module_record.file_name.to_string();

        println!("get_import_meta_props: file_name={file_name}");

        Ok(vec![
            KeyValueProp {
                key: PropName::Ident(IdentName::new("url".into(), span)),
                value: Box::new(Expr::Lit(Lit::Str(Str {
                    span,
                    raw: None,
                    value: file_name.into(),
                }))),
            },
            KeyValueProp {
                key: PropName::Ident(IdentName::new("main".into(), span)),
                value: Box::new(if module_record.is_entry {
                    Expr::Member(MemberExpr {
                        span,
                        obj: Box::new(Expr::MetaProp(MetaPropExpr {
                            span,
                            kind: MetaPropKind::ImportMeta,
                        })),
                        prop: MemberProp::Ident(IdentName::new("main".into(), span)),
                    })
                } else {
                    Expr::Lit(Lit::Bool(Bool { span, value: false }))
                }),
            },
        ])
    }
}

/// Custom loader for the SWC bundler.
///
/// Loads JavaScript and TypeScript modules for bundling.
pub struct Loader {
    /// The source map used for loading files.
    pub cm: Lrc<SourceMap>,
}

impl Load for Loader {
    /// Loads a JavaScript or TypeScript module from a file.
    ///
    /// This method reads the file, determines the appropriate syntax based on the
    /// file extension, parses the module, and applies TypeScript stripping if needed.
    ///
    /// # Errors
    ///
    /// * Returns an error if the file cannot be loaded from the source map.
    ///
    /// # Panics
    ///
    /// * Panics if the filename is not a real file path.
    /// * Panics if the file extension is not one of: ts, js, mjs, cjs.
    /// * Panics if parsing the module fails.
    fn load(&self, f: &FileName) -> Result<ModuleData, Error> {
        let FileName::Real(path) = f else {
            unreachable!()
        };

        println!("load: loading file {}", path.display());

        let extension = path.extension().and_then(|x| x.to_str());
        let syntax = syntax_for_extension(extension)
            .unwrap_or_else(|| panic!("Invalid file: {}", path.display()));
        let fm = self.cm.load_file(path)?;

        let module = if matches!(syntax, Syntax::Typescript(..)) {
            let program =
                parse_file_as_program(&fm, syntax, EsVersion::Es2020, None, &mut Vec::new())
                    .unwrap();

            let unresolved_mark = Mark::new();
            let top_level_mark = Mark::new();

            let module = program.apply(&mut strip(unresolved_mark, top_level_mark));

            module.module()
        } else {
            None
        };

        let module = module.unwrap_or_else(|| {
            println!("load: module was None");
            parse_file_as_module(&fm, syntax, EsVersion::Es2020, None, &mut Vec::new()).unwrap()
        });

        Ok(ModuleData {
            fm,
            module,
            helpers: Helpers::new(false),
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write as _;

    #[test_log::test]
    fn test_loader_loads_javascript_file() {
        let globals = Box::leak(Box::default());
        GLOBALS.set(globals, || {
            let temp_dir = tempfile::tempdir().unwrap();
            let js_path = temp_dir.path().join("test.js");

            {
                let mut file = std::fs::File::create(&js_path).unwrap();
                writeln!(file, "export const greeting = 'hello';").unwrap();
                writeln!(file, "export function add(a, b) {{ return a + b; }}").unwrap();
            }

            let cm = Lrc::new(SourceMap::new(FilePathMapping::empty()));
            let loader = Loader { cm };
            let filename = FileName::Real(js_path);

            let result = loader.load(&filename);
            assert!(result.is_ok(), "Should successfully load JavaScript file");

            let module_data = result.unwrap();
            // The module should have exports (body items)
            assert!(
                !module_data.module.body.is_empty(),
                "Module should have body items"
            );
        });
    }

    #[test_log::test]
    fn test_loader_loads_typescript_file_and_strips_types() {
        let globals = Box::leak(Box::default());
        GLOBALS.set(globals, || {
            let temp_dir = tempfile::tempdir().unwrap();
            let ts_path = temp_dir.path().join("test.ts");

            {
                let mut file = std::fs::File::create(&ts_path).unwrap();
                writeln!(file, "interface Person {{ name: string; age: number; }}").unwrap();
                writeln!(file, "export const greet = (person: Person): string => {{").unwrap();
                writeln!(file, "  return `Hello, ${{person.name}}`;").unwrap();
                writeln!(file, "}};").unwrap();
            }

            let cm = Lrc::new(SourceMap::new(FilePathMapping::empty()));
            let loader = Loader { cm };
            let filename = FileName::Real(ts_path);

            let result = loader.load(&filename);
            assert!(result.is_ok(), "Should successfully load TypeScript file");

            let module_data = result.unwrap();
            // After stripping, the module should still have exports but no interface declarations
            assert!(
                !module_data.module.body.is_empty(),
                "Module should have body items after type stripping"
            );
        });
    }

    #[test_log::test]
    fn test_loader_loads_mjs_file() {
        let globals = Box::leak(Box::default());
        GLOBALS.set(globals, || {
            let temp_dir = tempfile::tempdir().unwrap();
            let mjs_path = temp_dir.path().join("test.mjs");

            {
                let mut file = std::fs::File::create(&mjs_path).unwrap();
                writeln!(file, "export const value = 42;").unwrap();
            }

            let cm = Lrc::new(SourceMap::new(FilePathMapping::empty()));
            let loader = Loader { cm };
            let filename = FileName::Real(mjs_path);

            let result = loader.load(&filename);
            assert!(result.is_ok(), "Should successfully load .mjs file");
        });
    }

    #[test_log::test]
    fn test_loader_loads_cjs_file() {
        let globals = Box::leak(Box::default());
        GLOBALS.set(globals, || {
            let temp_dir = tempfile::tempdir().unwrap();
            let cjs_path = temp_dir.path().join("test.cjs");

            {
                let mut file = std::fs::File::create(&cjs_path).unwrap();
                writeln!(file, "const value = 42;").unwrap();
                writeln!(file, "export {{ value }};").unwrap();
            }

            let cm = Lrc::new(SourceMap::new(FilePathMapping::empty()));
            let loader = Loader { cm };
            let filename = FileName::Real(cjs_path);

            let result = loader.load(&filename);
            assert!(result.is_ok(), "Should successfully load .cjs file");
        });
    }

    #[test_log::test]
    fn test_loader_returns_error_for_nonexistent_file() {
        let globals = Box::leak(Box::default());
        GLOBALS.set(globals, || {
            let cm = Lrc::new(SourceMap::new(FilePathMapping::empty()));
            let loader = Loader { cm };
            let filename = FileName::Real(std::path::PathBuf::from("/nonexistent/path/test.js"));

            let result = loader.load(&filename);
            assert!(result.is_err(), "Should return error for nonexistent file");
        });
    }

    #[test_log::test]
    fn test_loader_typescript_with_complex_types() {
        let globals = Box::leak(Box::default());
        GLOBALS.set(globals, || {
            let temp_dir = tempfile::tempdir().unwrap();
            let ts_path = temp_dir.path().join("complex.ts");

            {
                let mut file = std::fs::File::create(&ts_path).unwrap();
                // Write TypeScript with generics and type parameters
                writeln!(
                    file,
                    "type Result<T, E> = {{ ok: true; value: T }} | {{ ok: false; error: E }};"
                )
                .unwrap();
                writeln!(
                    file,
                    "export function process<T>(items: T[]): T | undefined {{"
                )
                .unwrap();
                writeln!(file, "  return items[0];").unwrap();
                writeln!(file, "}}").unwrap();
            }

            let cm = Lrc::new(SourceMap::new(FilePathMapping::empty()));
            let loader = Loader { cm };
            let filename = FileName::Real(ts_path);

            let result = loader.load(&filename);
            assert!(
                result.is_ok(),
                "Should successfully load TypeScript with complex types"
            );

            let module_data = result.unwrap();
            assert!(
                !module_data.module.body.is_empty(),
                "Module should have body items"
            );
        });
    }

    #[test_log::test]
    fn test_syntax_for_extension_typescript() {
        let syntax = syntax_for_extension(Some("ts"));
        assert!(syntax.is_some());
        let syntax = syntax.unwrap();
        assert!(matches!(syntax, Syntax::Typescript(_)));
        if let Syntax::Typescript(ts) = syntax {
            assert!(!ts.tsx, "tsx should be disabled");
            assert!(ts.decorators, "decorators should be enabled");
            assert!(!ts.dts, "dts should be disabled");
        }
    }

    #[test_log::test]
    fn test_syntax_for_extension_javascript() {
        let syntax = syntax_for_extension(Some("js"));
        assert!(syntax.is_some());
        let syntax = syntax.unwrap();
        assert!(matches!(syntax, Syntax::Es(_)));
        if let Syntax::Es(es) = syntax {
            assert!(!es.jsx, "jsx should be disabled");
            assert!(es.decorators, "decorators should be enabled");
        }
    }

    #[test_log::test]
    fn test_syntax_for_extension_mjs() {
        let syntax = syntax_for_extension(Some("mjs"));
        assert!(syntax.is_some());
        assert!(matches!(syntax.unwrap(), Syntax::Es(_)));
    }

    #[test_log::test]
    fn test_syntax_for_extension_cjs() {
        let syntax = syntax_for_extension(Some("cjs"));
        assert!(syntax.is_some());
        assert!(matches!(syntax.unwrap(), Syntax::Es(_)));
    }

    #[test_log::test]
    fn test_syntax_for_extension_unsupported() {
        // Unsupported extensions should return None
        assert!(syntax_for_extension(Some("tsx")).is_none());
        assert!(syntax_for_extension(Some("jsx")).is_none());
        assert!(syntax_for_extension(Some("py")).is_none());
        assert!(syntax_for_extension(Some("rs")).is_none());
        assert!(syntax_for_extension(Some("")).is_none());
    }

    #[test_log::test]
    fn test_syntax_for_extension_none() {
        // No extension should return None
        assert!(syntax_for_extension(None).is_none());
    }

    #[test_log::test]
    fn test_syntax_for_extension_typescript_has_correct_config() {
        let syntax = syntax_for_extension(Some("ts")).unwrap();
        if let Syntax::Typescript(ts) = syntax {
            // Verify the TypeScript configuration is suitable for bundling
            assert!(!ts.tsx, "tsx should be disabled for .ts files");
            assert!(ts.decorators, "decorators should be enabled for bundling");
            assert!(!ts.dts, "dts should be disabled - not type definitions");
            assert!(!ts.no_early_errors, "early errors should be caught");
            assert!(
                ts.disallow_ambiguous_jsx_like,
                "ambiguous JSX-like syntax should be disallowed"
            );
        } else {
            panic!("Expected Typescript syntax");
        }
    }

    #[test_log::test]
    fn test_syntax_for_extension_javascript_has_correct_config() {
        let syntax = syntax_for_extension(Some("js")).unwrap();
        if let Syntax::Es(es) = syntax {
            // Verify the ES configuration is suitable for bundling
            assert!(!es.jsx, "jsx should be disabled for .js files");
            assert!(es.decorators, "decorators should be enabled for bundling");
            assert!(!es.fn_bind, "function bind syntax should be disabled");
            assert!(
                !es.decorators_before_export,
                "decorators should come after export"
            );
            assert!(
                !es.export_default_from,
                "export default from should be disabled"
            );
            assert!(
                !es.import_attributes,
                "import attributes should be disabled"
            );
            assert!(
                !es.allow_super_outside_method,
                "super outside method should not be allowed"
            );
            assert!(
                !es.allow_return_outside_function,
                "return outside function should not be allowed"
            );
            assert!(!es.auto_accessors, "auto accessors should be disabled");
            assert!(
                !es.explicit_resource_management,
                "explicit resource management should be disabled"
            );
        } else {
            panic!("Expected Es syntax");
        }
    }

    #[test_log::test]
    fn test_syntax_for_extension_all_js_variants_produce_es_syntax() {
        // All JavaScript variants should produce ES syntax
        for ext in ["js", "mjs", "cjs"] {
            let syntax = syntax_for_extension(Some(ext));
            assert!(syntax.is_some(), "Extension '{ext}' should be supported");
            assert!(
                matches!(syntax.unwrap(), Syntax::Es(_)),
                "Extension '{ext}' should produce Es syntax"
            );
        }
    }

    #[test_log::test]
    fn test_syntax_for_extension_consistency_across_js_variants() {
        // All JavaScript variants should produce identical configuration
        let js_syntax = syntax_for_extension(Some("js")).unwrap();
        let mjs_syntax = syntax_for_extension(Some("mjs")).unwrap();
        let cjs_syntax = syntax_for_extension(Some("cjs")).unwrap();

        if let (Syntax::Es(js), Syntax::Es(mjs), Syntax::Es(cjs)) =
            (js_syntax, mjs_syntax, cjs_syntax)
        {
            // All ES variants should have identical configuration
            assert_eq!(js.jsx, mjs.jsx, "jsx should match across variants");
            assert_eq!(js.jsx, cjs.jsx, "jsx should match across variants");
            assert_eq!(
                js.decorators, mjs.decorators,
                "decorators should match across variants"
            );
            assert_eq!(
                js.decorators, cjs.decorators,
                "decorators should match across variants"
            );
            assert_eq!(
                js.fn_bind, mjs.fn_bind,
                "fn_bind should match across variants"
            );
            assert_eq!(
                js.fn_bind, cjs.fn_bind,
                "fn_bind should match across variants"
            );
        } else {
            panic!("Expected all variants to produce Es syntax");
        }
    }
}
