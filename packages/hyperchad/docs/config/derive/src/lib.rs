#![cfg_attr(feature = "fail-on-warnings", deny(warnings))]
#![warn(clippy::all, clippy::pedantic, clippy::nursery, clippy::cargo)]
#![allow(clippy::multiple_crate_versions)]
//! Proc macros for generating configuration documentation schema.
//!
//! Provides two derive macros:
//!
//! - `ConfigDoc` — for config structs. Generates `ConfigDocSchema` impl that
//!   extracts field names, doc comments, types, and serializes defaults.
//!
//! - `ConfigDocEnum` — for config enums. Generates `config_doc_values()` and
//!   `config_doc_default_value()` methods using serde rename rules.

use proc_macro::TokenStream;
use quote::quote;
use syn::{Attribute, Data, DeriveInput, Fields, Lit, Meta, parse_macro_input};

#[derive(Default)]
struct NestedDocConfig {
    enabled: bool,
    map_key: Option<String>,
    list_index: Option<String>,
}

// ── Serde rename_all support ────────────────────────────────────────────────

fn apply_rename_rule(name: &str, rule: &str) -> String {
    match rule {
        "lowercase" => name.to_lowercase(),
        "UPPERCASE" => name.to_uppercase(),
        "camelCase" => to_camel_case(name, false),
        "PascalCase" => to_camel_case(name, true),
        "snake_case" => to_snake_case(name),
        "SCREAMING_SNAKE_CASE" => to_snake_case(name).to_uppercase(),
        "kebab-case" => to_snake_case(name).replace('_', "-"),
        "SCREAMING-KEBAB-CASE" => to_snake_case(name).to_uppercase().replace('_', "-"),
        _ => name.to_string(),
    }
}

fn to_snake_case(s: &str) -> String {
    let mut result = String::new();
    for (i, ch) in s.chars().enumerate() {
        if ch.is_uppercase() && i > 0 {
            result.push('_');
        }
        result.push(ch.to_lowercase().next().unwrap());
    }
    result
}

fn to_camel_case(s: &str, upper_first: bool) -> String {
    let mut result = String::new();
    let mut capitalize_next = upper_first;
    for ch in s.chars() {
        if ch == '_' {
            capitalize_next = true;
        } else if capitalize_next {
            result.push(ch.to_uppercase().next().unwrap());
            capitalize_next = false;
        } else {
            result.push(ch);
        }
    }
    result
}

// ── Attribute parsing helpers ───────────────────────────────────────────────

/// Extract `#[doc = "..."]` attributes and join them into a single string.
fn extract_doc_comment(attrs: &[Attribute]) -> String {
    let mut lines = Vec::new();
    for attr in attrs {
        if attr.path().is_ident("doc")
            && let Meta::NameValue(nv) = &attr.meta
            && let syn::Expr::Lit(expr_lit) = &nv.value
            && let Lit::Str(lit) = &expr_lit.lit
        {
            lines.push(lit.value().trim().to_string());
        }
    }
    lines.join(" ")
}

/// Extract `#[serde(rename_all = "...")]` value from attributes.
fn extract_serde_rename_all(attrs: &[Attribute]) -> Option<String> {
    for attr in attrs {
        if !attr.path().is_ident("serde") {
            continue;
        }
        let Ok(nested) = attr
            .parse_args_with(syn::punctuated::Punctuated::<Meta, syn::Token![,]>::parse_terminated)
        else {
            continue;
        };
        for meta in &nested {
            if let Meta::NameValue(nv) = meta
                && nv.path.is_ident("rename_all")
                && let syn::Expr::Lit(expr_lit) = &nv.value
                && let Lit::Str(lit) = &expr_lit.lit
            {
                return Some(lit.value());
            }
        }
    }
    None
}

/// Extract `#[config_doc(section = "...")]` value.
fn extract_section_name(attrs: &[Attribute]) -> Option<String> {
    for attr in attrs {
        if !attr.path().is_ident("config_doc") {
            continue;
        }
        let Ok(nested) = attr
            .parse_args_with(syn::punctuated::Punctuated::<Meta, syn::Token![,]>::parse_terminated)
        else {
            continue;
        };
        for meta in &nested {
            if let Meta::NameValue(nv) = meta
                && nv.path.is_ident("section")
                && let syn::Expr::Lit(expr_lit) = &nv.value
                && let Lit::Str(lit) = &expr_lit.lit
            {
                return Some(lit.value());
            }
        }
    }
    None
}

/// Extract `#[config_doc(values("a", "b", "c"))]` from field attributes.
/// Used for foreign enum types where we can't derive `ConfigDocEnum`.
fn extract_config_doc_values(attrs: &[Attribute]) -> Option<Vec<String>> {
    for attr in attrs {
        if !attr.path().is_ident("config_doc") {
            continue;
        }
        let Ok(nested) = attr
            .parse_args_with(syn::punctuated::Punctuated::<Meta, syn::Token![,]>::parse_terminated)
        else {
            continue;
        };
        for meta in &nested {
            if let Meta::List(list) = meta
                && list.path.is_ident("values")
            {
                let mut values = Vec::new();
                let Ok(inner) = list.parse_args_with(
                    syn::punctuated::Punctuated::<Lit, syn::Token![,]>::parse_terminated,
                ) else {
                    continue;
                };
                for lit in &inner {
                    if let Lit::Str(s) = lit {
                        values.push(s.value());
                    }
                }
                if !values.is_empty() {
                    return Some(values);
                }
            }
        }
    }
    None
}

/// Extract nested schema options from `#[config_doc(...)]` field attributes.
///
/// Supported forms:
/// - `#[config_doc(nested)]`
/// - `#[config_doc(nested, map_key = "<name>")]`
/// - `#[config_doc(nested, list_index = "<index>")]`
fn extract_nested_doc_config(attrs: &[Attribute]) -> syn::Result<NestedDocConfig> {
    let mut result = NestedDocConfig::default();

    for attr in attrs {
        if !attr.path().is_ident("config_doc") {
            continue;
        }

        let Ok(nested) = attr
            .parse_args_with(syn::punctuated::Punctuated::<Meta, syn::Token![,]>::parse_terminated)
        else {
            continue;
        };

        for meta in &nested {
            match meta {
                Meta::Path(path) if path.is_ident("nested") => {
                    result.enabled = true;
                }
                Meta::NameValue(nv) if nv.path.is_ident("map_key") => {
                    if let syn::Expr::Lit(expr_lit) = &nv.value
                        && let Lit::Str(lit) = &expr_lit.lit
                    {
                        result.map_key = Some(lit.value());
                    } else {
                        return Err(syn::Error::new_spanned(
                            &nv.value,
                            "config_doc map_key must be a string literal",
                        ));
                    }
                }
                Meta::NameValue(nv) if nv.path.is_ident("list_index") => {
                    if let syn::Expr::Lit(expr_lit) = &nv.value
                        && let Lit::Str(lit) = &expr_lit.lit
                    {
                        result.list_index = Some(lit.value());
                    } else {
                        return Err(syn::Error::new_spanned(
                            &nv.value,
                            "config_doc list_index must be a string literal",
                        ));
                    }
                }
                _ => {}
            }
        }
    }

    if (result.map_key.is_some() || result.list_index.is_some()) && !result.enabled {
        return Err(syn::Error::new(
            proc_macro2::Span::call_site(),
            "config_doc map_key/list_index requires config_doc(nested)",
        ));
    }

    if result.map_key.is_some() && result.list_index.is_some() {
        return Err(syn::Error::new(
            proc_macro2::Span::call_site(),
            "config_doc map_key and list_index cannot be combined on the same field",
        ));
    }

    Ok(result)
}

fn option_inner_type(ty: &syn::Type) -> Option<&syn::Type> {
    let syn::Type::Path(type_path) = ty else {
        return None;
    };

    let segment = type_path.path.segments.last()?;
    if segment.ident != "Option" {
        return None;
    }

    let syn::PathArguments::AngleBracketed(args) = &segment.arguments else {
        return None;
    };

    for arg in &args.args {
        if let syn::GenericArgument::Type(inner) = arg {
            return Some(inner);
        }
    }

    None
}

fn map_value_type(ty: &syn::Type) -> Option<&syn::Type> {
    let syn::Type::Path(type_path) = ty else {
        return None;
    };

    let segment = type_path.path.segments.last()?;
    if segment.ident != "BTreeMap" && segment.ident != "HashMap" {
        return None;
    }

    let syn::PathArguments::AngleBracketed(args) = &segment.arguments else {
        return None;
    };

    let mut type_args = args.args.iter().filter_map(|arg| {
        if let syn::GenericArgument::Type(ty) = arg {
            Some(ty)
        } else {
            None
        }
    });

    let _key_ty = type_args.next()?;
    type_args.next()
}

fn vec_item_type(ty: &syn::Type) -> Option<&syn::Type> {
    let syn::Type::Path(type_path) = ty else {
        return None;
    };

    let segment = type_path.path.segments.last()?;
    if segment.ident != "Vec" {
        return None;
    }

    let syn::PathArguments::AngleBracketed(args) = &segment.arguments else {
        return None;
    };

    for arg in &args.args {
        if let syn::GenericArgument::Type(inner) = arg {
            return Some(inner);
        }
    }

    None
}

// ── Type display mapping ────────────────────────────────────────────────────

fn scalar_type_display(name: &str) -> Option<&'static str> {
    match name {
        "bool" => Some("bool"),
        "String" => Some("string"),
        "PathBuf" => Some("path"),
        "usize" | "u64" | "u32" | "u16" | "u8" | "i64" | "i32" | "i16" | "i8" => Some("integer"),
        "f64" | "f32" => Some("number"),
        _ => None,
    }
}

/// Map a Rust type to a human-readable display name for docs.
fn type_to_display(ty: &syn::Type) -> String {
    let s = quote!(#ty).to_string().replace(' ', "");

    match s.as_str() {
        "bool" => "bool".to_string(),
        "String" => "string".to_string(),
        "usize" | "u64" | "u32" | "u16" | "u8" | "i64" | "i32" | "i16" | "i8" => {
            "integer".to_string()
        }
        "f64" | "f32" => "number".to_string(),
        _ if s.starts_with("Option<") => {
            let inner = &s[7..s.len() - 1];
            let inner_display = scalar_type_display(inner).unwrap_or("string");
            format!("{inner_display} (optional)")
        }
        _ if s.starts_with("Vec<") => {
            let inner = &s[4..s.len() - 1];
            let inner_display = scalar_type_display(inner).unwrap_or("string");
            format!("list of {inner_display}s")
        }
        _ if s.starts_with("BTreeMap<") || s.starts_with("HashMap<") => "table".to_string(),
        _ if s == "PathBuf" => "path".to_string(),
        // For custom enum types, use "string" since they serialize as strings
        _ => "string".to_string(),
    }
}

/// Check if a type is likely a custom enum (not a primitive/std type).
fn is_enum_type(ty: &syn::Type) -> bool {
    let s = quote!(#ty).to_string().replace(' ', "");
    !matches!(
        s.as_str(),
        "bool"
            | "String"
            | "usize"
            | "u64"
            | "u32"
            | "u16"
            | "u8"
            | "i64"
            | "i32"
            | "f64"
            | "f32"
            | "PathBuf"
    ) && !s.starts_with("Option<")
        && !s.starts_with("Vec<")
        && !s.starts_with("BTreeMap<")
        && !s.starts_with("HashMap<")
}

/// Extract the type identifier for an enum type (to call `::config_doc_values()`).
fn enum_type_ident(ty: &syn::Type) -> Option<proc_macro2::Ident> {
    if let syn::Type::Path(tp) = ty {
        tp.path.get_ident().cloned()
    } else {
        None
    }
}

// ── ConfigDocEnum derive ────────────────────────────────────────────────────

/// Derive macro for config enums.
///
/// Generates:
/// - `fn config_doc_values() -> &'static [&'static str]` — TOML-facing variant names
/// - `fn config_doc_default_value() -> &'static str` — the default variant's TOML name
///
/// Respects `#[serde(rename_all = "...")]` and `#[default]` attributes.
#[proc_macro_derive(ConfigDocEnum)]
pub fn derive_config_doc_enum(input: TokenStream) -> TokenStream {
    let input = parse_macro_input!(input as DeriveInput);
    let name = &input.ident;

    let Data::Enum(data_enum) = &input.data else {
        return syn::Error::new_spanned(&input, "ConfigDocEnum can only be derived for enums")
            .to_compile_error()
            .into();
    };

    let rename_rule = extract_serde_rename_all(&input.attrs).unwrap_or_default();

    let mut variant_names = Vec::new();
    let mut default_variant: Option<String> = None;

    for variant in &data_enum.variants {
        let raw_name = variant.ident.to_string();
        let toml_name = if rename_rule.is_empty() {
            raw_name.clone()
        } else {
            apply_rename_rule(&raw_name, &rename_rule)
        };

        let is_default = variant.attrs.iter().any(|a| a.path().is_ident("default"));

        if is_default {
            default_variant = Some(toml_name.clone());
        }

        variant_names.push(toml_name);
    }

    let default_val =
        default_variant.unwrap_or_else(|| variant_names.first().cloned().unwrap_or_default());

    let variant_name_literals: Vec<_> = variant_names.iter().map(|n| quote!(#n)).collect();
    let num_variants = variant_names.len();

    let expanded = quote! {
        impl #name {
            /// Returns TOML-facing names for all variants.
            pub fn config_doc_values() -> &'static [&'static str] {
                static VALUES: [&str; #num_variants] = [#(#variant_name_literals),*];
                &VALUES
            }

            /// Returns the TOML-facing name of the default variant.
            pub fn config_doc_default_value() -> &'static str {
                #default_val
            }
        }
    };

    expanded.into()
}

// ── ConfigDoc derive ────────────────────────────────────────────────────────

/// Derive macro for config structs.
///
/// Generates `ConfigDocSchema` impl with:
/// - `section_name()` — from `#[config_doc(section = "...")]`
/// - `section_description()` — from struct doc comment
/// - `field_docs()` — field metadata extracted from doc comments and types
/// - `default_values()` — serialized from `Default::default()`
///
/// Field-level options:
/// - `#[config_doc(values("a", "b"))]` for explicit enum values.
/// - `#[config_doc(nested)]` for inline nested schema expansion.
/// - `#[config_doc(nested, map_key = "<name>")]` for map value schema expansion.
/// - `#[config_doc(nested, list_index = "<index>")]` for list item schema expansion.
///
/// **All public fields must have doc comments.** A compile error is emitted
/// for any undocumented field.
#[allow(clippy::too_many_lines)]
#[proc_macro_derive(ConfigDoc, attributes(config_doc))]
pub fn derive_config_doc(input: TokenStream) -> TokenStream {
    let input = parse_macro_input!(input as DeriveInput);
    let name = &input.ident;

    let Data::Struct(data_struct) = &input.data else {
        return syn::Error::new_spanned(&input, "ConfigDoc can only be derived for structs")
            .to_compile_error()
            .into();
    };

    let section = extract_section_name(&input.attrs).unwrap_or_else(|| {
        // Fall back to snake_case of struct name without "Config" suffix
        let s = name.to_string();
        let s = s.strip_suffix("Config").unwrap_or(&s);
        to_snake_case(s)
    });

    let struct_doc = extract_doc_comment(&input.attrs);

    let Fields::Named(fields) = &data_struct.fields else {
        return syn::Error::new_spanned(&input, "ConfigDoc requires named fields")
            .to_compile_error()
            .into();
    };

    let mut field_doc_entries = Vec::new();
    let mut default_value_entries = Vec::new();

    for field in &fields.named {
        let Some(field_ident) = &field.ident else {
            continue;
        };

        // Only process pub fields
        if !matches!(field.vis, syn::Visibility::Public(_)) {
            continue;
        }

        let field_name = field_ident.to_string();
        let doc_comment = extract_doc_comment(&field.attrs);

        // Compile error for missing doc comments
        if doc_comment.is_empty() {
            return syn::Error::new_spanned(
                field_ident,
                format!(
                    "ConfigDoc: field `{field_name}` in `{name}` is missing a doc comment. \
                     All config fields must be documented."
                ),
            )
            .to_compile_error()
            .into();
        }

        let type_display = type_to_display(&field.ty);

        let nested_doc_config = match extract_nested_doc_config(&field.attrs) {
            Ok(cfg) => cfg,
            Err(err) => return err.to_compile_error().into(),
        };

        // Check for explicit #[config_doc(values("a", "b"))] on the field first,
        // then fall back to TypeName::config_doc_values() for local enums.
        let enum_values_expr = if nested_doc_config.enabled {
            quote! { None }
        } else {
            extract_config_doc_values(&field.attrs).map_or_else(
                || {
                    if is_enum_type(&field.ty) {
                        enum_type_ident(&field.ty).map_or_else(
                            || quote! { None },
                            |enum_ident| quote! { Some(#enum_ident::config_doc_values()) },
                        )
                    } else if let Some(item_ty) = vec_item_type(&field.ty) {
                        if is_enum_type(item_ty) {
                            enum_type_ident(item_ty).map_or_else(
                                || quote! { None },
                                |enum_ident| quote! { Some(#enum_ident::config_doc_values()) },
                            )
                        } else {
                            quote! { None }
                        }
                    } else {
                        quote! { None }
                    }
                },
                |explicit_values| {
                    let num = explicit_values.len();
                    let lits: Vec<_> = explicit_values.iter().map(|v| quote!(#v)).collect();
                    quote! {{
                        static VALS: [&str; #num] = [#(#lits),*];
                        Some(&VALS[..])
                    }}
                },
            )
        };

        let nested_expr = if nested_doc_config.enabled {
            if let Some(value_ty) = map_value_type(&field.ty) {
                if nested_doc_config.list_index.is_some() {
                    return syn::Error::new_spanned(
                        &field.ty,
                        "config_doc list_index is only valid for list fields",
                    )
                    .to_compile_error()
                    .into();
                }
                let key_placeholder = nested_doc_config
                    .map_key
                    .unwrap_or_else(|| "<key>".to_string());
                quote! {
                    Some(hyperchad_docs_config::NestedFieldDoc::Map {
                        key_placeholder: #key_placeholder,
                        value_fields: <#value_ty as hyperchad_docs_config::ConfigDocSchema>::field_docs(),
                        value_defaults: <#value_ty as hyperchad_docs_config::ConfigDocSchema>::default_values(),
                    })
                }
            } else if let Some(item_ty) = vec_item_type(&field.ty) {
                if nested_doc_config.map_key.is_some() {
                    return syn::Error::new_spanned(
                        &field.ty,
                        "config_doc map_key is only valid for map fields",
                    )
                    .to_compile_error()
                    .into();
                }
                let index_placeholder = nested_doc_config
                    .list_index
                    .unwrap_or_else(|| "<index>".to_string());
                quote! {
                    Some(hyperchad_docs_config::NestedFieldDoc::List {
                        index_placeholder: #index_placeholder,
                        item_fields: <#item_ty as hyperchad_docs_config::ConfigDocSchema>::field_docs(),
                        item_defaults: <#item_ty as hyperchad_docs_config::ConfigDocSchema>::default_values(),
                    })
                }
            } else {
                if nested_doc_config.map_key.is_some() || nested_doc_config.list_index.is_some() {
                    return syn::Error::new_spanned(
                        &field.ty,
                        "config_doc map_key/list_index is only valid for map/list fields",
                    )
                    .to_compile_error()
                    .into();
                }

                let nested_ty = option_inner_type(&field.ty).unwrap_or(&field.ty);
                quote! {
                    Some(hyperchad_docs_config::NestedFieldDoc::Inline {
                        fields: <#nested_ty as hyperchad_docs_config::ConfigDocSchema>::field_docs(),
                        defaults: <#nested_ty as hyperchad_docs_config::ConfigDocSchema>::default_values(),
                    })
                }
            }
        } else {
            quote! { None }
        };

        field_doc_entries.push(quote! {
            hyperchad_docs_config::FieldDoc {
                toml_key: #field_name,
                type_display: #type_display,
                description: #doc_comment,
                enum_values: #enum_values_expr,
                nested: #nested_expr,
            }
        });

        // Generate default value serialization
        default_value_entries.push(quote! {
            m.insert(
                #field_name.to_string(),
                toml::Value::try_from(&d.#field_ident)
                    .map(|v| {
                        if let toml::Value::Table(table) = &v
                            && table.is_empty()
                        {
                            return "{}".to_string();
                        }
                        if let toml::Value::Array(array) = &v
                            && array.is_empty()
                        {
                            return "[]".to_string();
                        }
                        if v.is_table() {
                            toml::to_string_pretty(&v)
                                .unwrap_or_default()
                                .trim()
                                .to_string()
                        } else {
                            let s = v.to_string();
                            s.trim_matches('"').to_string()
                        }
                    })
                    .unwrap_or_else(|_| "—".to_string()),
            );
        });
    }

    let expanded = quote! {
        impl hyperchad_docs_config::ConfigDocSchema for #name {
            fn section_name() -> &'static str {
                #section
            }

            fn section_description() -> &'static str {
                #struct_doc
            }

            fn field_docs() -> ::std::vec::Vec<hyperchad_docs_config::FieldDoc> {
                ::std::vec![#(#field_doc_entries),*]
            }

            fn default_values() -> ::std::collections::BTreeMap<String, String> {
                let d = <Self as ::std::default::Default>::default();
                let mut m = ::std::collections::BTreeMap::new();
                #(#default_value_entries)*
                m
            }
        }
    };

    expanded.into()
}
