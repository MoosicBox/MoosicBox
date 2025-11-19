//! JavaScript and TypeScript bundling using SWC.
//!
//! This module provides functionality to bundle JavaScript and TypeScript files
//! using SWC as the underlying bundler. It supports minification, module loading,
//! and TypeScript compilation with configurable options.

use std::{collections::BTreeMap, fs::create_dir_all, path::Path};

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
            NodeModulesResolver::new(TargetEnv::Browser, Default::default(), true),
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

    let mut bundles = bundler.bundle(entries.into_iter().collect()).unwrap();
    println!("Bundled as {} bundles", bundles.len());

    if minify {
        bundles = bundles
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

    print_bundles(out, cm, bundles, minify);
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
fn print_bundles(out: &Path, cm: Lrc<SourceMap>, bundles: Vec<Bundle>, minify: bool) {
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

        let syntax = match path.extension().and_then(|x| x.to_str()) {
            Some("ts") => Syntax::Typescript(TsSyntax {
                tsx: false,
                decorators: true,
                dts: false,
                no_early_errors: false,
                disallow_ambiguous_jsx_like: true,
            }),
            Some("js" | "mjs" | "cjs") => Syntax::Es(EsSyntax {
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
            }),
            _ => panic!("Invalid file: {path:?}"),
        };
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
