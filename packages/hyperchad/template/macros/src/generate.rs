#![allow(clippy::option_if_let_else)]

use proc_macro2::{Ident, TokenStream};
use quote::{ToTokens, format_ident, quote};
use syn::{Expr, Local};

use crate::ast::{
    AttributeName, AttributeType, Block, ContainerAttribute, ContainerNameOrMarkup, ControlFlow,
    ControlFlowKind, Element, ElementBody, ElementName, ForExpr, IfCondition, IfExpr, IfOrBlock,
    Markup, Markups, MatchExpr, NoElement, WhileExpr,
};

pub fn generate(markups: Markups<Element>, output_ident: Ident) -> Result<TokenStream, String> {
    let mut build = Builder::new(output_ident.clone());
    let generator = Generator::new(output_ident);
    generator.markups(markups, &mut build)?;
    Ok(build.finish())
}

struct Generator {
    output_ident: Ident,
}

impl Generator {
    const fn new(output_ident: Ident) -> Self {
        Self { output_ident }
    }

    fn builder(&self) -> Builder {
        Builder::new(self.output_ident.clone())
    }

    fn markups<E: Into<Element>>(
        &self,
        markups: Markups<E>,
        build: &mut Builder,
    ) -> Result<(), String> {
        for markup in markups.markups {
            self.markup(markup, build)?;
        }
        Ok(())
    }

    fn markup<E: Into<Element>>(
        &self,
        markup: Markup<E>,
        build: &mut Builder,
    ) -> Result<(), String> {
        match markup {
            Markup::Block(block) => {
                if block.markups.markups.iter().any(|markup| {
                    if let Markup::ControlFlow(flow) = markup {
                        if let ControlFlowKind::Let(_) = flow.kind {
                            return true;
                        }
                    }
                    false
                }) {
                    self.block(block, build)?;
                } else {
                    self.markups(block.markups, build)?;
                }
            }
            Markup::Lit(lit) => {
                // For literals, create a Raw element with the content
                let value = match &lit.lit {
                    syn::Lit::Str(lit_str) => lit_str.value(),
                    syn::Lit::Int(lit_int) => lit_int.to_string(),
                    syn::Lit::Float(lit_float) => lit_float.to_string(),
                    syn::Lit::Char(lit_char) => lit_char.value().to_string(),
                    syn::Lit::Bool(lit_bool) => lit_bool.value.to_string(),
                    _ => lit.lit.to_token_stream().to_string(),
                };
                build.push_container(quote! {
                    hyperchad_transformer::Container {
                        element: hyperchad_transformer::Element::Raw { value: #value.to_string() },
                        ..Default::default()
                    }
                });
            }
            Markup::NumericLit(numeric_lit) => {
                // For numeric literals, create a Raw element with the value
                let value = &numeric_lit.value;
                build.push_container(quote! {
                    hyperchad_transformer::Container {
                        element: hyperchad_transformer::Element::Raw { value: #value.to_string() },
                        ..Default::default()
                    }
                });
            }
            Markup::Splice { expr, .. } => {
                // For spliced expressions, use RenderContainer trait to convert to containers
                let output_ident = &self.output_ident;
                build.push_tokens(quote! {
                    {
                        use hyperchad_template::RenderContainer;
                        let mut splice_containers = Vec::new();
                        (#expr).render_to(&mut splice_containers).unwrap();
                        for container in splice_containers {
                            #output_ident.push(container);
                        }
                    }
                });
            }
            Markup::BraceSplice { items, .. } => {
                // For brace-wrapped items {item1 item2 ...}, process each item in order
                // This handles mixed content like {"Name: " (some_function())} properly
                for item in items {
                    self.markup(item, build)?;
                }
            }
            Markup::Element(element) => self.element(element.into(), build)?,
            Markup::ControlFlow(control_flow) => self.control_flow(*control_flow, build)?,
            Markup::Semi(_) => {}
        }
        Ok(())
    }

    fn block<E: Into<Element>>(&self, block: Block<E>, build: &mut Builder) -> Result<(), String> {
        let markups = {
            let mut build = self.builder();
            self.markups(block.markups, &mut build)?;
            build.finish()
        };

        build.push_tokens(quote!({ #markups }));
        Ok(())
    }

    fn element(&self, element: Element, build: &mut Builder) -> Result<(), String> {
        let element_name = element.name.clone().unwrap_or_else(|| ElementName {
            name: format_ident!("div"),
        });

        // Generate attribute assignments
        let mut attr_assignments = Vec::new();
        let (classes, id, named_attrs) = split_attrs(element.attrs);

        // Handle ID
        if let Some(id) = id {
            match id {
                ContainerNameOrMarkup::Name(name) => {
                    let id_str = name.to_string();
                    attr_assignments.push(quote! { str_id: Some(#id_str.to_string()) });
                }
                ContainerNameOrMarkup::Markup(markup) => {
                    // For dynamic IDs, use markup_to_string_tokens to handle concatenation
                    let id_tokens = Self::markup_to_string_tokens(markup);
                    attr_assignments.push(quote! { str_id: Some(#id_tokens) });
                }
            }
        }

        // Handle classes
        if !classes.is_empty() {
            let class_strings: Vec<_> = classes
                .into_iter()
                .map(|(name, _toggler)| {
                    match name {
                        ContainerNameOrMarkup::Name(name) => {
                            let class_str = name.to_string();
                            quote! { #class_str.to_string() }
                        }
                        ContainerNameOrMarkup::Markup(_) => {
                            // For dynamic classes, we'd need special handling
                            quote! { String::new() }
                        }
                    }
                })
                .collect();

            if !class_strings.is_empty() {
                attr_assignments.push(quote! { classes: vec![#(#class_strings),*] });
            }
        }

        // Handle special case: id attribute as named attribute
        let mut filtered_named_attrs = Vec::new();
        for (name, attr_type) in named_attrs {
            if name.to_string() == "id" {
                // Handle id attribute specially
                if let AttributeType::Normal { value, .. } = attr_type {
                    let id_tokens = Self::markup_to_string_tokens(value);
                    attr_assignments.push(quote! { str_id: Some(#id_tokens) });
                }
            } else {
                filtered_named_attrs.push((name, attr_type));
            }
        }

        // Extract HTMX routing attributes
        let route_assignment = Self::extract_route_from_attributes(&filtered_named_attrs);
        if let Some(route) = route_assignment {
            attr_assignments.push(route);
        }

        // Extract action attributes
        let actions_assignment = Self::extract_actions_from_attributes(&filtered_named_attrs);
        if let Some(actions) = actions_assignment {
            attr_assignments.push(actions);
        }

        // Extract data attributes
        let data_assignment = Self::extract_data_attributes(&filtered_named_attrs);
        if let Some(data) = data_assignment {
            attr_assignments.push(data);
        }

        // Separate element-specific attributes from container-level attributes
        let (element_attrs, container_attrs) =
            Self::separate_element_and_container_attributes(&element_name, filtered_named_attrs);

        // Generate the element type with element-specific attributes
        let element_type =
            Self::element_name_to_type_with_attributes(&element_name, element_attrs)?;

        // Process container-level attributes (styling, layout, etc.)
        let processed_attrs = Self::process_attributes(container_attrs)?;
        for assignment in processed_attrs {
            attr_assignments.push(assignment);
        }

        // Generate children
        let children = if let ElementBody::Block(block) = element.body {
            // Create a unique identifier for children to avoid borrowing conflicts
            let children_ident = format_ident!("__children_{}", self.output_ident);
            let child_generator = Self::new(children_ident.clone());
            let mut child_build = child_generator.builder();
            child_generator.markups(block.markups, &mut child_build)?;
            let children_tokens = child_build.finish();
            quote! { children: { let mut #children_ident = Vec::new(); #children_tokens #children_ident } }
        } else {
            quote! { children: Vec::new() }
        };

        // Generate the complete container
        build.push_container(quote! {
            hyperchad_transformer::Container {
                element: #element_type,
                #(#attr_assignments,)*
                #children,
                ..Default::default()
            }
        });
        Ok(())
    }

    #[allow(clippy::type_complexity)]
    fn separate_element_and_container_attributes(
        element_name: &ElementName,
        named_attrs: Vec<(AttributeName, AttributeType)>,
    ) -> (
        Vec<(AttributeName, AttributeType)>,
        Vec<(AttributeName, AttributeType)>,
    ) {
        let element_name_str = element_name.name.to_string();
        let mut element_attrs = Vec::new();
        let mut container_attrs = Vec::new();

        for (name, attr_type) in named_attrs {
            let name_str = name.to_string();

            // Skip routing, action, and data attributes as they're handled separately
            if matches!(
                name_str.as_str(),
                "hx-get"
                    | "hx-post"
                    | "hx-put"
                    | "hx-delete"
                    | "hx-patch"
                    | "hx-trigger"
                    | "hx-swap"
            ) || name_str.starts_with("fx-")
                || name_str.starts_with("data-")
            {
                continue;
            }

            // Determine if this attribute belongs to the element or container
            let is_element_attr = match element_name_str.as_str() {
                "input" => matches!(
                    name_str.as_str(),
                    "value"
                        | "placeholder"
                        | "name"
                        | "type"
                        | "checked"
                        | "disabled"
                        | "readonly"
                        | "multiple"
                        | "required"
                ),
                "button" => matches!(name_str.as_str(), "type" | "disabled"),
                "anchor" => matches!(name_str.as_str(), "href" | "target"),
                "image" => matches!(
                    name_str.as_str(),
                    "src" | "alt" | "srcset" | "sizes" | "loading" | "fit"
                ),
                _ => false,
            };

            if is_element_attr {
                element_attrs.push((name, attr_type));
            } else {
                container_attrs.push((name, attr_type));
            }
        }

        (element_attrs, container_attrs)
    }

    fn element_name_to_type_with_attributes(
        name: &ElementName,
        element_attrs: Vec<(AttributeName, AttributeType)>,
    ) -> Result<TokenStream, String> {
        let name_str = name.name.to_string();

        Ok(match name_str.as_str() {
            "input" => Self::generate_input_element(element_attrs)?,
            "button" => Self::generate_button_element(element_attrs),
            "anchor" => Self::generate_anchor_element(element_attrs),
            "image" => Self::generate_image_element(element_attrs),
            _ => Self::element_name_to_type(name), // Fallback to simple element generation
        })
    }

    #[allow(clippy::too_many_lines)]
    fn generate_input_element(
        element_attrs: Vec<(AttributeName, AttributeType)>,
    ) -> Result<TokenStream, String> {
        let mut input_type = None;
        let mut value = None;
        let mut placeholder = None;
        let mut name = None;
        let mut checked = None;

        for (attr_name, attr_type) in element_attrs {
            let name_str = attr_name.to_string();
            match attr_type {
                AttributeType::Normal {
                    value: attr_value, ..
                } => match name_str.as_str() {
                    "type" => {
                        // Extract compile-time type (string literal or identifier)
                        if let Some(compile_time_type) =
                            Self::extract_compile_time_input_type(&attr_value)
                        {
                            input_type = Some(compile_time_type);
                        } else {
                            return Err("Input type must be a compile-time constant (literal string or identifier). Dynamic input types are not supported. Use one of: text, tel, email, checkbox, password, hidden".to_string());
                        }
                    }
                    "value" => {
                        let value_tokens = Self::markup_to_string_tokens(attr_value);
                        value = Some(quote! { Some(#value_tokens) });
                    }
                    "placeholder" => {
                        let placeholder_tokens = Self::markup_to_string_tokens(attr_value);
                        placeholder = Some(quote! { Some(#placeholder_tokens) });
                    }
                    "name" => {
                        let name_tokens = Self::markup_to_string_tokens(attr_value);
                        name = Some(quote! { Some(#name_tokens) });
                    }
                    "checked" => {
                        checked = Some(Self::markup_to_bool_tokens(attr_value));
                    }
                    _ => {}
                },
                AttributeType::Optional { toggler, .. } => {
                    let cond = &toggler.cond;
                    match name_str.as_str() {
                        "value" => {
                            value = Some(
                                quote! { if let Some(val) = (#cond) { Some(val.to_string()) } else { None } },
                            );
                        }
                        "placeholder" => {
                            placeholder = Some(
                                quote! { if let Some(val) = (#cond) { Some(val.to_string()) } else { None } },
                            );
                        }
                        "name" => {
                            name = Some(
                                quote! { if let Some(val) = (#cond) { Some(val.to_string()) } else { None } },
                            );
                        }
                        "checked" => {
                            checked = Some(
                                quote! { if let Some(val) = (#cond) { val.into() } else { false } },
                            );
                        }
                        _ => {}
                    }
                }
                AttributeType::Empty(_) => {
                    if name_str.as_str() == "checked" {
                        checked = Some(quote! { true });
                    }
                }
            }
        }

        let input_type = input_type.ok_or("Missing input type")?;
        let name_field = name.unwrap_or_else(|| quote! { None });
        let value_field = value.unwrap_or_else(|| quote! { None });
        let placeholder_field = placeholder.unwrap_or_else(|| quote! { None });
        let checked_field = checked.unwrap_or_else(|| quote! { false });

        // Generate compile-time input type directly
        let input_variant = match input_type.as_str() {
            "text" | "tel" | "email" => quote! {
                hyperchad_transformer::Input::Text {
                    value: #value_field,
                    placeholder: #placeholder_field
                }
            },
            "checkbox" => quote! {
                hyperchad_transformer::Input::Checkbox {
                    checked: Some(#checked_field)
                }
            },
            "password" => quote! {
                hyperchad_transformer::Input::Password {
                    value: #value_field,
                    placeholder: #placeholder_field
                }
            },
            "hidden" => quote! {
                hyperchad_transformer::Input::Hidden {
                    value: #value_field
                }
            },
            _ => {
                return Err(format!(
                    "Unsupported input type '{input_type}'. Supported types are: text, tel, email, checkbox, password, hidden"
                ));
            }
        };

        Ok(quote! {
            hyperchad_transformer::Element::Input {
                input: #input_variant,
                name: #name_field
            }
        })
    }

    fn generate_button_element(element_attrs: Vec<(AttributeName, AttributeType)>) -> TokenStream {
        let mut button_type = None;

        for (attr_name, attr_type) in element_attrs {
            let name_str = attr_name.to_string();
            if let AttributeType::Normal {
                value: attr_value, ..
            } = attr_type
            {
                if name_str == "type" {
                    button_type = Some(Self::markup_to_string_tokens(attr_value));
                }
            }
        }

        let type_field = button_type.map_or_else(|| quote! { None }, |t| quote! { Some(#t) });

        quote! {
            hyperchad_transformer::Element::Button {
                r#type: #type_field
            }
        }
    }

    fn generate_anchor_element(element_attrs: Vec<(AttributeName, AttributeType)>) -> TokenStream {
        let mut href = None;
        let mut target = None;

        for (attr_name, attr_type) in element_attrs {
            let name_str = attr_name.to_string();
            if let AttributeType::Normal {
                value: attr_value, ..
            } = attr_type
            {
                match name_str.as_str() {
                    "href" => {
                        href = Some(Self::markup_to_string_tokens(attr_value));
                    }
                    "target" => {
                        target = Some(Self::markup_to_link_target_tokens(attr_value));
                    }
                    _ => {}
                }
            }
        }

        let href_field = href.map_or_else(|| quote! { None }, |h| quote! { Some(#h) });
        let target_field = target.map_or_else(|| quote! { None }, |t| quote! { Some(#t) });

        quote! {
            hyperchad_transformer::Element::Anchor {
                href: #href_field,
                target: #target_field
            }
        }
    }

    fn generate_image_element(element_attrs: Vec<(AttributeName, AttributeType)>) -> TokenStream {
        let mut src = None;
        let mut alt = None;
        let mut srcset = None;
        let mut sizes = None;
        let mut loading = None;
        let mut fit = None;

        for (attr_name, attr_type) in element_attrs {
            let name_str = attr_name.to_string();
            if let AttributeType::Normal {
                value: attr_value, ..
            } = attr_type
            {
                match name_str.as_str() {
                    "src" => {
                        src = Some(Self::markup_to_string_tokens(attr_value));
                    }
                    "alt" => {
                        alt = Some(Self::markup_to_string_tokens(attr_value));
                    }
                    "srcset" => {
                        srcset = Some(Self::markup_to_string_tokens(attr_value));
                    }
                    "sizes" => {
                        sizes = Some(Self::markup_to_number_tokens(attr_value));
                    }
                    "loading" => {
                        loading = Some(Self::markup_to_image_loading_tokens(attr_value));
                    }
                    "fit" => {
                        fit = Some(Self::markup_to_image_fit_tokens(attr_value));
                    }
                    _ => {}
                }
            }
        }

        let src_field = src.map_or_else(|| quote! { None }, |s| quote! { Some(#s) });
        let alt_field = alt.map_or_else(|| quote! { None }, |a| quote! { Some(#a) });
        let srcset_field = srcset.map_or_else(|| quote! { None }, |s| quote! { Some(#s) });
        let sizes_field = sizes.map_or_else(|| quote! { None }, |s| quote! { Some(#s) });
        let loading_field = loading.map_or_else(|| quote! { None }, |l| quote! { Some(#l) });
        let fit_field = fit.map_or_else(|| quote! { None }, |f| quote! { Some(#f) });

        quote! {
            hyperchad_transformer::Element::Image {
                source: #src_field,
                alt: #alt_field,
                source_set: #srcset_field,
                sizes: #sizes_field,
                loading: #loading_field,
                fit: #fit_field
            }
        }
    }

    fn markup_to_link_target_tokens(value: Markup<NoElement>) -> TokenStream {
        match value {
            Markup::Lit(lit) => {
                if let syn::Lit::Str(lit_str) = &lit.lit {
                    let value_str = lit_str.value();
                    match value_str.as_str() {
                        "_self" => quote! { hyperchad_transformer_models::LinkTarget::SelfTarget },
                        "_blank" => quote! { hyperchad_transformer_models::LinkTarget::Blank },
                        "_parent" => quote! { hyperchad_transformer_models::LinkTarget::Parent },
                        "_top" => quote! { hyperchad_transformer_models::LinkTarget::Top },
                        target => {
                            quote! { hyperchad_transformer_models::LinkTarget::Custom(#target.to_string()) }
                        }
                    }
                } else {
                    let lit = &lit.lit;
                    quote! { hyperchad_transformer_models::LinkTarget::Custom((#lit).to_string()) }
                }
            }
            Markup::Splice { expr, .. } => {
                quote! { (#expr).into() }
            }
            _ => quote! { hyperchad_transformer_models::LinkTarget::default() },
        }
    }

    fn markup_to_image_loading_tokens(value: Markup<NoElement>) -> TokenStream {
        match value {
            Markup::Lit(lit) => {
                if let syn::Lit::Str(lit_str) = &lit.lit {
                    let value_str = lit_str.value();
                    match value_str.as_str() {
                        "eager" => quote! { hyperchad_transformer_models::ImageLoading::Eager },
                        "lazy" => quote! { hyperchad_transformer_models::ImageLoading::Lazy },
                        _ => quote! { hyperchad_transformer_models::ImageLoading::default() },
                    }
                } else {
                    let lit = &lit.lit;
                    quote! { (#lit).into() }
                }
            }
            Markup::Splice { expr, .. } => {
                quote! { (#expr).into() }
            }
            _ => quote! { hyperchad_transformer_models::ImageLoading::default() },
        }
    }

    /// Extract a compile-time input type (string literal or identifier)
    fn extract_compile_time_input_type(value: &Markup<NoElement>) -> Option<String> {
        match value {
            Markup::Lit(lit) => {
                if let syn::Lit::Str(lit_str) = &lit.lit {
                    Some(lit_str.value())
                } else {
                    None
                }
            }
            Markup::Splice { expr, .. } => {
                // Handle raw identifiers like `type=text`
                if let syn::Expr::Path(expr_path) = &**expr {
                    if expr_path.path.segments.len() == 1 && expr_path.qself.is_none() {
                        let identifier_name = expr_path.path.segments[0].ident.to_string();
                        Some(identifier_name)
                    } else {
                        None
                    }
                } else if let syn::Expr::Lit(expr_lit) = &**expr {
                    // Handle literal expressions in splices
                    if let syn::Lit::Str(lit_str) = &expr_lit.lit {
                        Some(lit_str.value())
                    } else {
                        None
                    }
                } else {
                    None
                }
            }
            _ => None,
        }
    }

    fn markup_to_image_fit_tokens(value: Markup<NoElement>) -> TokenStream {
        match value {
            Markup::Lit(lit) => {
                if let syn::Lit::Str(lit_str) = &lit.lit {
                    let value_str = lit_str.value();
                    match value_str.as_str() {
                        "default" => quote! { hyperchad_transformer_models::ImageFit::Default },
                        "contain" => quote! { hyperchad_transformer_models::ImageFit::Contain },
                        "cover" => quote! { hyperchad_transformer_models::ImageFit::Cover },
                        "fill" => quote! { hyperchad_transformer_models::ImageFit::Fill },
                        "none" => quote! { hyperchad_transformer_models::ImageFit::None },
                        _ => quote! { hyperchad_transformer_models::ImageFit::default() },
                    }
                } else {
                    let lit = &lit.lit;
                    quote! { (#lit).into() }
                }
            }
            Markup::Splice { expr, .. } => {
                quote! { (#expr).into() }
            }
            Markup::BraceSplice { items, .. } => {
                // For brace-wrapped items, handle like single item if only one
                if items.len() == 1 {
                    Self::markup_to_image_fit_tokens(items[0].clone())
                } else {
                    let expr = Self::handle_brace_splice_expression(&items);
                    quote! {
                        {
                            let result = { #expr };
                            result.into()
                        }
                    }
                }
            }
            _ => quote! { hyperchad_transformer_models::ImageFit::default() },
        }
    }

    fn extract_route_from_attributes(
        named_attrs: &[(AttributeName, AttributeType)],
    ) -> Option<TokenStream> {
        let mut route_method = None;
        let mut route_url = None;
        let mut trigger = None;
        let mut swap = None;

        // Find HTMX attributes
        for (name, attr_type) in named_attrs {
            let name_str = name.to_string();
            if let AttributeType::Normal { value, .. } = attr_type {
                match name_str.as_str() {
                    "hx-get" => {
                        route_method = Some("Get");
                        route_url = Some(Self::markup_to_string_tokens(value.clone()));
                    }
                    "hx-post" => {
                        route_method = Some("Post");
                        route_url = Some(Self::markup_to_string_tokens(value.clone()));
                    }
                    "hx-put" => {
                        route_method = Some("Put");
                        route_url = Some(Self::markup_to_string_tokens(value.clone()));
                    }
                    "hx-delete" => {
                        route_method = Some("Delete");
                        route_url = Some(Self::markup_to_string_tokens(value.clone()));
                    }
                    "hx-patch" => {
                        route_method = Some("Patch");
                        route_url = Some(Self::markup_to_string_tokens(value.clone()));
                    }
                    "hx-trigger" => {
                        trigger = Some(Self::markup_to_string_tokens(value.clone()));
                    }
                    "hx-swap" => {
                        swap = Some(Self::markup_to_swap_target_tokens(value.clone()));
                    }
                    _ => {}
                }
            }
        }

        // If we found a route method and URL, generate the route
        if let (Some(method), Some(url)) = (route_method, route_url) {
            let method_ident = format_ident!("{}", method);
            let trigger_field = trigger.map_or_else(
                || quote! { trigger: None },
                |trigger| quote! { trigger: Some(#trigger) },
            );
            let swap_field = swap.map_or_else(
                || quote! { swap: hyperchad_transformer_models::SwapTarget::default() },
                |swap| quote! { swap: #swap },
            );

            Some(quote! {
                route: Some(hyperchad_transformer_models::Route::#method_ident {
                    route: #url,
                    #trigger_field,
                    #swap_field,
                })
            })
        } else {
            None
        }
    }

    fn extract_actions_from_attributes(
        named_attrs: &[(AttributeName, AttributeType)],
    ) -> Option<TokenStream> {
        let mut actions = Vec::new();

        // Find fx- attributes
        for (name, attr_type) in named_attrs {
            let name_str = name.to_string();
            if let Some(trigger_name) = name_str.strip_prefix("fx-") {
                if let AttributeType::Normal { value, .. } = attr_type {
                    let trigger_ident = Self::action_trigger_name_to_ident(trigger_name);
                    let action_effect = Self::markup_to_action_effect_tokens(value.clone());

                    actions.push(quote! {
                        hyperchad_actions::Action {
                            trigger: #trigger_ident,
                            effect: #action_effect,
                        }
                    });
                }
            }
        }

        if actions.is_empty() {
            None
        } else {
            Some(quote! {
                actions: vec![#(#actions),*]
            })
        }
    }

    fn extract_data_attributes(
        named_attrs: &[(AttributeName, AttributeType)],
    ) -> Option<TokenStream> {
        let mut data_entries = Vec::new();

        // Find data- attributes
        for (name, attr_type) in named_attrs {
            let name_str = name.to_string();
            if let Some(data_key) = name_str.strip_prefix("data-") {
                match attr_type {
                    AttributeType::Normal { value, .. } => {
                        let value_tokens = Self::markup_to_string_tokens(value.clone());
                        data_entries.push(quote! {
                            (#data_key.to_string(), #value_tokens)
                        });
                    }
                    AttributeType::Optional { toggler, .. } => {
                        let cond = &toggler.cond;
                        data_entries.push(quote! {
                            {
                                if let Some(val) = (#cond) {
                                    Some((#data_key.to_string(), val.to_string()))
                                } else {
                                    None
                                }
                            }
                        });
                    }
                    AttributeType::Empty(_) => {
                        // For empty data attributes, set value to empty string
                        data_entries.push(quote! {
                            (#data_key.to_string(), String::new())
                        });
                    }
                }
            }
        }

        if data_entries.is_empty() {
            None
        } else {
            // Check if we have any optional entries that need special handling
            let has_optional = named_attrs.iter().any(|(name, attr_type)| {
                name.to_string().starts_with("data-")
                    && matches!(attr_type, AttributeType::Optional { .. })
            });

            if has_optional {
                // Use a more complex construction to handle optional data attributes
                Some(quote! {
                    data: {
                        let mut data_map = std::collections::BTreeMap::new();
                        let entries: Vec<Option<(String, String)>> = vec![#(#data_entries),*];
                        for entry in entries.into_iter().flatten() {
                            data_map.insert(entry.0, entry.1);
                        }
                        data_map
                    }
                })
            } else {
                // Simple case - all data attributes are present
                Some(quote! {
                    data: std::collections::BTreeMap::from([#(#data_entries),*])
                })
            }
        }
    }

    fn action_trigger_name_to_ident(trigger_name: &str) -> TokenStream {
        match trigger_name {
            "click" => quote! { hyperchad_actions::ActionTrigger::Click },
            "click-outside" => quote! { hyperchad_actions::ActionTrigger::ClickOutside },
            "resize" => quote! { hyperchad_actions::ActionTrigger::Resize },
            "immediate" => quote! { hyperchad_actions::ActionTrigger::Immediate },
            "hover" => quote! { hyperchad_actions::ActionTrigger::Hover },
            "change" => quote! { hyperchad_actions::ActionTrigger::Change },
            "mousedown" => quote! { hyperchad_actions::ActionTrigger::MouseDown },
            // For all other triggers, use the Event variant
            _ => {
                quote! { hyperchad_actions::ActionTrigger::Event(#trigger_name.to_string()) }
            }
        }
    }

    fn markup_to_action_effect_tokens(value: Markup<NoElement>) -> TokenStream {
        // First, try to extract fx DSL content
        if let Some(dsl_tokens) = Self::extract_fx_dsl_content(&value) {
            // Check if dsl_tokens is empty - if so, return NoOp directly
            if dsl_tokens.is_empty() {
                return quote! {
                    hyperchad_actions::ActionType::NoOp.into()
                };
            }

            // Parse the DSL content and determine ActionType at compile time
            return Self::generate_compile_time_optimized_dsl_action(&dsl_tokens);
        }

        // Fallback to existing behavior for non-fx content
        match value {
            Markup::Lit(lit) => {
                // Handle literal action effects - this might be a string representation
                if let syn::Lit::Str(lit_str) = &lit.lit {
                    let value_str = lit_str.value();
                    quote! {
                        hyperchad_actions::ActionType::Custom {
                            action: #value_str.to_string()
                        }.into()
                    }
                } else {
                    let lit = &lit.lit;
                    // For backwards compatibility: convert non-string literals to custom actions
                    quote! {
                        hyperchad_actions::ActionType::Custom {
                            action: (#lit).to_string()
                        }.into()
                    }
                }
            }
            Markup::Splice { expr, .. } => {
                // For backwards compatibility: handle all non-fx() expressions directly
                quote! {
                    {
                        let val = #expr;
                        // Use the IntoActionEffect trait for conversion
                        hyperchad_template::IntoActionEffect::into_action_effect(val)
                    }
                }
            }
            Markup::BraceSplice { items, .. } => {
                // For brace-wrapped items that aren't fx DSL
                if items.len() == 1 {
                    Self::markup_to_action_effect_tokens(items[0].clone())
                } else {
                    let expr = Self::handle_brace_splice_expression(&items);

                    // For backwards compatibility: handle all brace splice expressions directly
                    quote! {
                        {
                            let val = { #expr };
                            // Use the IntoActionEffect trait for conversion
                            hyperchad_template::IntoActionEffect::into_action_effect(val)
                        }
                    }
                }
            }
            _ => quote! {
                hyperchad_actions::ActionType::NoOp.into()
            },
        }
    }

    fn markup_to_string_tokens(value: Markup<NoElement>) -> TokenStream {
        match value {
            Markup::NumericLit(numeric_lit) => {
                // For numeric literals, just use the value as a string
                let value = &numeric_lit.value;
                quote! { #value.to_string() }
            }
            Markup::Lit(lit) => {
                if let syn::Lit::Str(lit_str) = &lit.lit {
                    let value_str = lit_str.value();
                    quote! { #value_str.to_string() }
                } else {
                    let lit = &lit.lit;
                    quote! { #lit.to_string() }
                }
            }
            Markup::Splice { expr, .. } => {
                // For expressions, handle them directly - this allows any Rust expression to be evaluated
                quote! { (#expr).to_string() }
            }
            Markup::BraceSplice { items, .. } => {
                // For brace-wrapped items, handle concatenation
                if items.len() == 1 {
                    // Single item - handle like regular markup
                    Self::markup_to_string_tokens(items[0].clone())
                } else {
                    // Multiple items - concatenate as strings
                    let item_tokens: Vec<_> = items
                        .iter()
                        .map(|item| Self::markup_to_string_tokens(item.clone()))
                        .collect();
                    quote! { vec![#(#item_tokens),*].join("") }
                }
            }
            _ => quote! { String::new() },
        }
    }

    fn markup_to_swap_target_tokens(value: Markup<NoElement>) -> TokenStream {
        match value {
            Markup::Lit(lit) => {
                if let syn::Lit::Str(lit_str) = &lit.lit {
                    let value_str = lit_str.value();
                    match value_str.as_str() {
                        "this" | "self" => {
                            quote! { hyperchad_transformer_models::SwapTarget::This }
                        }
                        "children" => quote! { hyperchad_transformer_models::SwapTarget::Children },
                        value if value.starts_with('#') => {
                            let id = &value[1..];
                            quote! { hyperchad_transformer_models::SwapTarget::Id(#id.to_string()) }
                        }
                        _ => quote! { hyperchad_transformer_models::SwapTarget::default() },
                    }
                } else {
                    let lit = &lit.lit;
                    quote! { (#lit).into() }
                }
            }
            Markup::Splice { expr, .. } => {
                quote! { (#expr).into() }
            }
            Markup::BraceSplice { items, .. } => {
                // For brace-wrapped items, handle like single item if only one
                if items.len() == 1 {
                    Self::markup_to_swap_target_tokens(items[0].clone())
                } else {
                    let expr = Self::handle_brace_splice_expression(&items);
                    quote! {
                        {
                            let result = { #expr };
                            result.into()
                        }
                    }
                }
            }
            _ => quote! { hyperchad_transformer_models::SwapTarget::default() },
        }
    }

    fn process_attributes(
        named_attrs: Vec<(AttributeName, AttributeType)>,
    ) -> Result<Vec<TokenStream>, String> {
        // Use BTreeMap to track field assignments with precedence
        let mut field_assignments = std::collections::BTreeMap::new();

        // Separate shorthand and individual properties
        let mut shorthand_attrs = std::collections::BTreeMap::new();
        let mut individual_attrs = Vec::new();

        for (name, attr_type) in named_attrs {
            let name_str = name.to_string();
            match name_str.as_str() {
                // Shorthand properties
                "padding"
                | "padding-x"
                | "padding-y"
                | "margin"
                | "margin-x"
                | "margin-y"
                | "border"
                | "border-x"
                | "border-y"
                | "border-radius"
                | "border-top-radius"
                | "border-right-radius"
                | "border-bottom-radius"
                | "border-left-radius"
                | "gap"
                | "flex"
                | "flex-grow"
                | "flex-shrink"
                | "flex-basis"
                | "text-decoration" => {
                    shorthand_attrs.insert(name_str, (name, attr_type));
                }
                _ => {
                    individual_attrs.push((name, attr_type));
                }
            }
        }

        // Handle shorthand properties first (lower precedence)
        Self::handle_shorthand_properties(&shorthand_attrs, &mut field_assignments);

        // Handle individual properties (higher precedence - these override shorthand)
        for (name, attr_type) in individual_attrs {
            if let Some(assignment) = Self::attr_to_assignment(&name, attr_type) {
                // Extract field name from the assignment and store it
                let field_name = Self::extract_field_name_from_assignment(&assignment);
                field_assignments.insert(field_name, assignment);
            } else {
                let name_str = name.to_string();
                let error_msg = format!(
                    "Unknown attribute '{name_str}'. Supported attributes include: class, width, height, padding, padding-x, padding-y, padding-left, padding-right, padding-top, padding-bottom, margin, margin-x, margin-y, margin-left, margin-right, margin-top, margin-bottom, border, border-x, border-y, border-top, border-right, border-bottom, border-left, background, color, align-items, justify-content, text-align, text-decoration, direction, position, cursor, visibility, overflow-x, overflow-y, font-family, font-size, opacity, border-radius, gap, hidden, debug, flex, flex-grow, flex-shrink, flex-basis, HTMX attributes (hx-get, hx-post, hx-put, hx-delete, hx-patch, hx-trigger, hx-swap), and action attributes (fx-click, fx-click-outside, fx-resize, fx-immediate, fx-hover, fx-change, fx-mousedown, and any other fx-* event)"
                );
                return Err(error_msg);
            }
        }

        // Convert the final field assignments to a Vec
        Ok(field_assignments.into_values().collect())
    }

    #[allow(clippy::too_many_lines)]
    fn handle_shorthand_properties(
        shorthand_attrs: &std::collections::BTreeMap<String, (AttributeName, AttributeType)>,
        field_assignments: &mut std::collections::BTreeMap<String, TokenStream>,
    ) {
        // Handle padding shortcuts
        if let Some((_, AttributeType::Normal { value, .. })) = shorthand_attrs.get("padding") {
            let value_tokens = Self::markup_to_number_tokens(value.clone());
            field_assignments.insert(
                "padding_top".to_string(),
                quote! { padding_top: Some(#value_tokens.clone()) },
            );
            field_assignments.insert(
                "padding_right".to_string(),
                quote! { padding_right: Some(#value_tokens.clone()) },
            );
            field_assignments.insert(
                "padding_bottom".to_string(),
                quote! { padding_bottom: Some(#value_tokens.clone()) },
            );
            field_assignments.insert(
                "padding_left".to_string(),
                quote! { padding_left: Some(#value_tokens) },
            );
        }

        if let Some((_, AttributeType::Normal { value, .. })) = shorthand_attrs.get("padding-x") {
            let value_tokens = Self::markup_to_number_tokens(value.clone());
            field_assignments.insert(
                "padding_left".to_string(),
                quote! { padding_left: Some(#value_tokens.clone()) },
            );
            field_assignments.insert(
                "padding_right".to_string(),
                quote! { padding_right: Some(#value_tokens) },
            );
        }

        if let Some((_, AttributeType::Normal { value, .. })) = shorthand_attrs.get("padding-y") {
            let value_tokens = Self::markup_to_number_tokens(value.clone());
            field_assignments.insert(
                "padding_top".to_string(),
                quote! { padding_top: Some(#value_tokens.clone()) },
            );
            field_assignments.insert(
                "padding_bottom".to_string(),
                quote! { padding_bottom: Some(#value_tokens) },
            );
        }

        // Handle margin shortcuts
        if let Some((_, AttributeType::Normal { value, .. })) = shorthand_attrs.get("margin") {
            let value_tokens = Self::markup_to_number_tokens(value.clone());
            field_assignments.insert(
                "margin_top".to_string(),
                quote! { margin_top: Some(#value_tokens.clone()) },
            );
            field_assignments.insert(
                "margin_right".to_string(),
                quote! { margin_right: Some(#value_tokens.clone()) },
            );
            field_assignments.insert(
                "margin_bottom".to_string(),
                quote! { margin_bottom: Some(#value_tokens.clone()) },
            );
            field_assignments.insert(
                "margin_left".to_string(),
                quote! { margin_left: Some(#value_tokens) },
            );
        }

        if let Some((_, AttributeType::Normal { value, .. })) = shorthand_attrs.get("margin-x") {
            let value_tokens = Self::markup_to_number_tokens(value.clone());
            field_assignments.insert(
                "margin_left".to_string(),
                quote! { margin_left: Some(#value_tokens.clone()) },
            );
            field_assignments.insert(
                "margin_right".to_string(),
                quote! { margin_right: Some(#value_tokens) },
            );
        }

        if let Some((_, AttributeType::Normal { value, .. })) = shorthand_attrs.get("margin-y") {
            let value_tokens = Self::markup_to_number_tokens(value.clone());
            field_assignments.insert(
                "margin_top".to_string(),
                quote! { margin_top: Some(#value_tokens.clone()) },
            );
            field_assignments.insert(
                "margin_bottom".to_string(),
                quote! { margin_bottom: Some(#value_tokens) },
            );
        }

        // Handle border shortcuts
        if let Some((_, AttributeType::Normal { value, .. })) = shorthand_attrs.get("border") {
            let border_tokens = Self::markup_to_border_tokens(value.clone());
            field_assignments.insert(
                "border_top".to_string(),
                quote! { border_top: Some(#border_tokens.clone()) },
            );
            field_assignments.insert(
                "border_right".to_string(),
                quote! { border_right: Some(#border_tokens.clone()) },
            );
            field_assignments.insert(
                "border_bottom".to_string(),
                quote! { border_bottom: Some(#border_tokens.clone()) },
            );
            field_assignments.insert(
                "border_left".to_string(),
                quote! { border_left: Some(#border_tokens) },
            );
        }

        if let Some((_, AttributeType::Normal { value, .. })) = shorthand_attrs.get("border-x") {
            let border_tokens = Self::markup_to_border_tokens(value.clone());
            field_assignments.insert(
                "border_left".to_string(),
                quote! { border_left: Some(#border_tokens.clone()) },
            );
            field_assignments.insert(
                "border_right".to_string(),
                quote! { border_right: Some(#border_tokens) },
            );
        }

        if let Some((_, AttributeType::Normal { value, .. })) = shorthand_attrs.get("border-y") {
            let border_tokens = Self::markup_to_border_tokens(value.clone());
            field_assignments.insert(
                "border_top".to_string(),
                quote! { border_top: Some(#border_tokens.clone()) },
            );
            field_assignments.insert(
                "border_bottom".to_string(),
                quote! { border_bottom: Some(#border_tokens) },
            );
        }

        // Handle border-radius shortcuts
        if let Some((_, AttributeType::Normal { value, .. })) = shorthand_attrs.get("border-radius")
        {
            let radius_tokens = Self::markup_to_number_tokens(value.clone());
            field_assignments.insert(
                "border_top_left_radius".to_string(),
                quote! { border_top_left_radius: Some(#radius_tokens.clone()) },
            );
            field_assignments.insert(
                "border_top_right_radius".to_string(),
                quote! { border_top_right_radius: Some(#radius_tokens.clone()) },
            );
            field_assignments.insert(
                "border_bottom_left_radius".to_string(),
                quote! { border_bottom_left_radius: Some(#radius_tokens.clone()) },
            );
            field_assignments.insert(
                "border_bottom_right_radius".to_string(),
                quote! { border_bottom_right_radius: Some(#radius_tokens) },
            );
        }

        if let Some((_, AttributeType::Normal { value, .. })) =
            shorthand_attrs.get("border-top-radius")
        {
            let radius_tokens = Self::markup_to_number_tokens(value.clone());
            field_assignments.insert(
                "border_top_left_radius".to_string(),
                quote! { border_top_left_radius: Some(#radius_tokens.clone()) },
            );
            field_assignments.insert(
                "border_top_right_radius".to_string(),
                quote! { border_top_right_radius: Some(#radius_tokens) },
            );
        }

        if let Some((_, AttributeType::Normal { value, .. })) =
            shorthand_attrs.get("border-right-radius")
        {
            let radius_tokens = Self::markup_to_number_tokens(value.clone());
            field_assignments.insert(
                "border_top_right_radius".to_string(),
                quote! { border_top_right_radius: Some(#radius_tokens.clone()) },
            );
            field_assignments.insert(
                "border_bottom_right_radius".to_string(),
                quote! { border_bottom_right_radius: Some(#radius_tokens) },
            );
        }

        if let Some((_, AttributeType::Normal { value, .. })) =
            shorthand_attrs.get("border-bottom-radius")
        {
            let radius_tokens = Self::markup_to_number_tokens(value.clone());
            field_assignments.insert(
                "border_bottom_left_radius".to_string(),
                quote! { border_bottom_left_radius: Some(#radius_tokens.clone()) },
            );
            field_assignments.insert(
                "border_bottom_right_radius".to_string(),
                quote! { border_bottom_right_radius: Some(#radius_tokens) },
            );
        }

        if let Some((_, AttributeType::Normal { value, .. })) =
            shorthand_attrs.get("border-left-radius")
        {
            let radius_tokens = Self::markup_to_number_tokens(value.clone());
            field_assignments.insert(
                "border_top_left_radius".to_string(),
                quote! { border_top_left_radius: Some(#radius_tokens.clone()) },
            );
            field_assignments.insert(
                "border_bottom_left_radius".to_string(),
                quote! { border_bottom_left_radius: Some(#radius_tokens) },
            );
        }

        // Handle gap shortcut
        if let Some((_, AttributeType::Normal { value, .. })) = shorthand_attrs.get("gap") {
            let gap_tokens = Self::markup_to_number_tokens(value.clone());
            field_assignments.insert(
                "column_gap".to_string(),
                quote! { column_gap: Some(#gap_tokens.clone()) },
            );
            field_assignments.insert("row_gap".to_string(), quote! { row_gap: Some(#gap_tokens) });
        }

        // Handle flex shortcuts
        {
            // Handle individual flex properties
            let flex_grow = shorthand_attrs.get("flex-grow");
            let flex_shrink = shorthand_attrs.get("flex-shrink");
            let flex_basis = shorthand_attrs.get("flex-basis");

            if let Some((_, AttributeType::Normal { value, .. })) = shorthand_attrs.get("flex") {
                let flex_tokens =
                    Self::markup_to_flex_tokens(value.clone(), flex_grow, flex_shrink, flex_basis);
                field_assignments.insert("flex".to_string(), quote! { flex: Some(#flex_tokens) });
            }

            if flex_grow.is_some() || flex_shrink.is_some() || flex_basis.is_some() {
                let grow_tokens = if let Some((_, AttributeType::Normal { value, .. })) = flex_grow
                {
                    Self::markup_to_number_tokens(value.clone())
                } else {
                    quote! { hyperchad_transformer::Number::Integer(1) }
                };

                let shrink_tokens =
                    if let Some((_, AttributeType::Normal { value, .. })) = flex_shrink {
                        Self::markup_to_number_tokens(value.clone())
                    } else {
                        quote! { hyperchad_transformer::Number::Integer(1) }
                    };

                let basis_tokens =
                    if let Some((_, AttributeType::Normal { value, .. })) = flex_basis {
                        Self::markup_to_number_tokens(value.clone())
                    } else {
                        quote! { hyperchad_transformer::Number::IntegerPercent(0) }
                    };

                field_assignments.insert(
                    "flex".to_string(),
                    quote! {
                        flex: Some(hyperchad_transformer::Flex {
                            grow: #grow_tokens,
                            shrink: #shrink_tokens,
                            basis: #basis_tokens,
                        })
                    },
                );
            }
        }

        // Handle text-decoration shortcut (simple implementation for now)
        if let Some((_, AttributeType::Normal { value, .. })) =
            shorthand_attrs.get("text-decoration")
        {
            let text_decoration_tokens = Self::markup_to_text_decoration_tokens(value.clone());
            field_assignments.insert(
                "text_decoration".to_string(),
                quote! { text_decoration: Some(#text_decoration_tokens) },
            );
        }
    }

    fn element_name_to_type(name: &ElementName) -> TokenStream {
        let name_str = name.name.to_string();
        match name_str.as_str() {
            "div" => quote! { hyperchad_transformer::Element::Div },
            "section" => quote! { hyperchad_transformer::Element::Section },
            "aside" => quote! { hyperchad_transformer::Element::Aside },
            "main" => quote! { hyperchad_transformer::Element::Main },
            "header" => quote! { hyperchad_transformer::Element::Header },
            "footer" => quote! { hyperchad_transformer::Element::Footer },
            "form" => quote! { hyperchad_transformer::Element::Form },
            "span" => quote! { hyperchad_transformer::Element::Span },
            "button" => quote! { hyperchad_transformer::Element::Button { r#type: None } },
            "anchor" => {
                quote! { hyperchad_transformer::Element::Anchor { target: None, href: None } }
            }
            "image" => quote! { hyperchad_transformer::Element::Image {
                source: None,
                alt: None,
                fit: None,
                source_set: None,
                sizes: None,
                loading: None
            } },
            "input" => quote! { hyperchad_transformer::Element::Input {
                input: hyperchad_transformer::Input::Text { value: None, placeholder: None },
                name: None
            } },
            "h1" => {
                quote! { hyperchad_transformer::Element::Heading { size: hyperchad_transformer::HeaderSize::H1 } }
            }
            "h2" => {
                quote! { hyperchad_transformer::Element::Heading { size: hyperchad_transformer::HeaderSize::H2 } }
            }
            "h3" => {
                quote! { hyperchad_transformer::Element::Heading { size: hyperchad_transformer::HeaderSize::H3 } }
            }
            "h4" => {
                quote! { hyperchad_transformer::Element::Heading { size: hyperchad_transformer::HeaderSize::H4 } }
            }
            "h5" => {
                quote! { hyperchad_transformer::Element::Heading { size: hyperchad_transformer::HeaderSize::H5 } }
            }
            "h6" => {
                quote! { hyperchad_transformer::Element::Heading { size: hyperchad_transformer::HeaderSize::H6 } }
            }
            "ul" => quote! { hyperchad_transformer::Element::UnorderedList },
            "ol" => quote! { hyperchad_transformer::Element::OrderedList },
            "li" => quote! { hyperchad_transformer::Element::ListItem },
            "table" => quote! { hyperchad_transformer::Element::Table },
            "thead" => quote! { hyperchad_transformer::Element::THead },
            "th" => quote! { hyperchad_transformer::Element::TH },
            "tbody" => quote! { hyperchad_transformer::Element::TBody },
            "tr" => quote! { hyperchad_transformer::Element::TR },
            "td" => quote! { hyperchad_transformer::Element::TD },
            "canvas" => quote! { hyperchad_transformer::Element::Canvas },
            _ => {
                let error_msg = format!(
                    "Unknown element type '{name_str}'. Supported elements are: div, section, aside, main, header, footer, form, span, button, anchor, image, input, h1, h2, h3, h4, h5, h6, ul, ol, li, table, thead, th, tbody, tr, td, canvas",
                );
                quote! { compile_error!(#error_msg) }
            }
        }
    }

    #[allow(clippy::too_many_lines)]
    fn attr_to_assignment(name: &AttributeName, attr_type: AttributeType) -> Option<TokenStream> {
        let name_str = name.to_string();

        match attr_type {
            AttributeType::Normal { value, .. } => match name_str.as_str() {
                // Number properties
                "width" => Some(Self::number_attr("width", value)),
                "height" => Some(Self::number_attr("height", value)),
                "min-width" => Some(Self::number_attr("min_width", value)),
                "max-width" => Some(Self::number_attr("max_width", value)),
                "min-height" => Some(Self::number_attr("min_height", value)),
                "max-height" => Some(Self::number_attr("max_height", value)),
                "padding-left" => Some(Self::number_attr("padding_left", value)),
                "padding-right" => Some(Self::number_attr("padding_right", value)),
                "padding-top" => Some(Self::number_attr("padding_top", value)),
                "padding-bottom" => Some(Self::number_attr("padding_bottom", value)),
                "margin-left" => Some(Self::number_attr("margin_left", value)),
                "margin-right" => Some(Self::number_attr("margin_right", value)),
                "margin-top" => Some(Self::number_attr("margin_top", value)),
                "margin-bottom" => Some(Self::number_attr("margin_bottom", value)),
                "font-size" => Some(Self::number_attr("font_size", value)),
                "opacity" => Some(Self::number_attr("opacity", value)),
                "left" => Some(Self::number_attr("left", value)),
                "right" => Some(Self::number_attr("right", value)),
                "top" => Some(Self::number_attr("top", value)),
                "bottom" => Some(Self::number_attr("bottom", value)),
                "translate-x" => Some(Self::number_attr("translate_x", value)),
                "translate-y" => Some(Self::number_attr("translate_y", value)),
                "column-gap" | "col-gap" => Some(Self::number_attr("column_gap", value)),
                "row-gap" => Some(Self::number_attr("row_gap", value)),
                "grid-cell-size" => Some(Self::number_attr("grid_cell_size", value)),
                "border-top-left-radius" => {
                    Some(Self::number_attr("border_top_left_radius", value))
                }
                "border-top-right-radius" => {
                    Some(Self::number_attr("border_top_right_radius", value))
                }
                "border-bottom-left-radius" => {
                    Some(Self::number_attr("border_bottom_left_radius", value))
                }
                "border-bottom-right-radius" => {
                    Some(Self::number_attr("border_bottom_right_radius", value))
                }

                // Border properties
                "border-top" => Some(Self::border_attr("border_top", value)),
                "border-right" => Some(Self::border_attr("border_right", value)),
                "border-bottom" => Some(Self::border_attr("border_bottom", value)),
                "border-left" => Some(Self::border_attr("border_left", value)),

                // Enum properties
                "align-items" => Some(Self::enum_attr("align_items", "AlignItems", value)),
                "justify-content" => {
                    Some(Self::enum_attr("justify_content", "JustifyContent", value))
                }
                "text-align" => Some(Self::enum_attr("text_align", "TextAlign", value)),
                "text-decoration" => Some(Self::text_decoration_attr("text_decoration", value)),
                "direction" => Some(Self::direct_enum_attr(
                    "direction",
                    "LayoutDirection",
                    value,
                )),
                "position" => Some(Self::enum_attr("position", "Position", value)),
                "cursor" => Some(Self::enum_attr("cursor", "Cursor", value)),
                "visibility" => Some(Self::enum_attr("visibility", "Visibility", value)),
                "overflow-x" => Some(Self::direct_enum_attr(
                    "overflow_x",
                    "LayoutOverflow",
                    value,
                )),
                "overflow-y" => Some(Self::direct_enum_attr(
                    "overflow_y",
                    "LayoutOverflow",
                    value,
                )),

                // Color properties
                "background" => Some(Self::color_attr("background", value)),
                "color" => Some(Self::color_attr("color", value)),

                // Boolean properties
                "hidden" => Some(Self::bool_attr("hidden", value)),
                "debug" => Some(Self::bool_attr("debug", value)),

                // String properties
                "font-family" => Some(Self::string_vec_attr_opt("font_family", value)),
                "class" => Some(Self::string_vec_attr("classes", value)),

                _ => None,
            },
            AttributeType::Optional { toggler, .. } => {
                // Handle optional attributes with togglers
                let cond = &toggler.cond;
                let name_str = name.to_string();

                // Skip action attributes as they don't support optional syntax in this implementation
                if name_str.starts_with("fx-") {
                    return None;
                }

                // Generate conditional attribute assignment based on the field type
                // Skip input-specific attributes as they're handled by generate_input_element
                match name_str.as_str() {
                    // String properties - generate Option<String>
                    "id" | "href" | "src" | "alt" => {
                        let field_ident = format_ident!("{}", name_str.replace('-', "_"));
                        Some(quote! {
                            #field_ident: if let Some(val) = (#cond) { Some(val.to_string()) } else { None }
                        })
                    }
                    "srcset" => Some(quote! {
                        source_set: if let Some(val) = (#cond) { Some(val.to_string()) } else { None }
                    }),
                    // Number properties - generate Option<Number>
                    "width"
                    | "height"
                    | "min-width"
                    | "max-width"
                    | "min-height"
                    | "max-height"
                    | "padding-left"
                    | "padding-right"
                    | "padding-top"
                    | "padding-bottom"
                    | "margin-left"
                    | "margin-right"
                    | "margin-top"
                    | "margin-bottom"
                    | "font-size"
                    | "opacity"
                    | "left"
                    | "right"
                    | "top"
                    | "bottom"
                    | "translate-x"
                    | "translate-y"
                    | "column-gap"
                    | "col-gap"
                    | "row-gap"
                    | "grid-cell-size"
                    | "border-top-left-radius"
                    | "border-top-right-radius"
                    | "border-bottom-left-radius"
                    | "border-bottom-right-radius" => {
                        let field_ident = format_ident!("{}", name_str.replace('-', "_"));
                        Some(quote! {
                            #field_ident: if let Some(val) = (#cond) {
                                Some(<hyperchad_transformer::Number as std::convert::From<_>>::from(val))
                            } else { None }
                        })
                    }

                    "background" | "color" | "hidden" | "debug" | "border-top" | "border-right"
                    | "border-bottom" | "border-left" | "font-family" | "class" => {
                        let field_ident = format_ident!("{}", name_str.replace('-', "_"));
                        Some(quote! {
                            #field_ident: if let Some(val) = (#cond) {
                                Some(val.into())
                            } else {
                                None
                            }
                        })
                    }
                    _ => None,
                }
            }
            AttributeType::Empty(_) => {
                // Handle empty attributes (boolean flags)
                let name_str = name.to_string();

                // Skip action attributes as they require values
                if name_str.starts_with("fx-") {
                    return None;
                }

                match name_str.as_str() {
                    "hidden" => Some(quote! { hidden: Some(true) }),
                    "debug" => Some(quote! { debug: Some(true) }),

                    _ => None,
                }
            }
        }
    }

    fn number_attr(field: &str, value: Markup<NoElement>) -> TokenStream {
        let field_ident = format_ident!("{}", field);
        let value_tokens = Self::markup_to_number_tokens(value);
        quote! { #field_ident: Some(#value_tokens) }
    }

    fn enum_attr(field: &str, enum_name: &str, value: Markup<NoElement>) -> TokenStream {
        let field_ident = format_ident!("{}", field);
        let value_tokens = Self::markup_to_enum_tokens(enum_name, value);
        quote! { #field_ident: Some(#value_tokens) }
    }

    fn direct_enum_attr(field: &str, enum_name: &str, value: Markup<NoElement>) -> TokenStream {
        let field_ident = format_ident!("{}", field);
        let value_tokens = Self::markup_to_enum_tokens(enum_name, value);
        quote! { #field_ident: #value_tokens }
    }

    fn color_attr(field: &str, value: Markup<NoElement>) -> TokenStream {
        let field_ident = format_ident!("{}", field);
        let value_tokens = Self::markup_to_color_tokens(value);
        quote! { #field_ident: Some(#value_tokens) }
    }

    fn bool_attr(field: &str, value: Markup<NoElement>) -> TokenStream {
        let field_ident = format_ident!("{}", field);
        let value_tokens = Self::markup_to_bool_tokens(value);
        quote! { #field_ident: Some(#value_tokens) }
    }

    fn string_vec_attr(field: &str, value: Markup<NoElement>) -> TokenStream {
        let field_ident = format_ident!("{}", field);
        let value_tokens = Self::markup_to_string_vec_tokens(value);
        quote! { #field_ident: #value_tokens }
    }

    fn string_vec_attr_opt(field: &str, value: Markup<NoElement>) -> TokenStream {
        let field_ident = format_ident!("{}", field);
        let value_tokens = Self::markup_to_string_vec_tokens(value);
        quote! { #field_ident: Some(#value_tokens) }
    }

    fn border_attr(field: &str, value: Markup<NoElement>) -> TokenStream {
        let field_ident = format_ident!("{}", field);
        let border_tokens = Self::markup_to_border_tokens(value);
        quote! { #field_ident: Some(#border_tokens) }
    }

    fn text_decoration_attr(field: &str, value: Markup<NoElement>) -> TokenStream {
        let field_ident = format_ident!("{}", field);
        let text_decoration_tokens = Self::markup_to_text_decoration_tokens(value);
        quote! { #field_ident: Some(#text_decoration_tokens) }
    }

    #[allow(clippy::too_many_lines)]
    fn markup_to_number_tokens(value: Markup<NoElement>) -> TokenStream {
        match value {
            Markup::NumericLit(numeric_lit) => {
                // Handle the new NumericLit variant for compile-time numeric parsing
                use crate::ast::NumericType;
                match numeric_lit.number_type {
                    NumericType::IntegerPercent => {
                        let num_str = &numeric_lit.value[..numeric_lit.value.len() - 1];
                        let num: i64 = num_str.parse().unwrap();
                        quote! { hyperchad_transformer::Number::IntegerPercent(#num) }
                    }
                    NumericType::RealPercent => {
                        let num_str = &numeric_lit.value[..numeric_lit.value.len() - 1];
                        let num: f32 = num_str.parse().unwrap();
                        quote! { hyperchad_transformer::Number::RealPercent(#num) }
                    }
                    NumericType::IntegerVw => {
                        let num_str = &numeric_lit.value[..numeric_lit.value.len() - 2];
                        let num: i64 = num_str.parse().unwrap();
                        quote! { hyperchad_transformer::Number::IntegerVw(#num) }
                    }
                    NumericType::RealVw => {
                        let num_str = &numeric_lit.value[..numeric_lit.value.len() - 2];
                        let num: f32 = num_str.parse().unwrap();
                        quote! { hyperchad_transformer::Number::RealVw(#num) }
                    }
                    NumericType::IntegerVh => {
                        let num_str = &numeric_lit.value[..numeric_lit.value.len() - 2];
                        let num: i64 = num_str.parse().unwrap();
                        quote! { hyperchad_transformer::Number::IntegerVh(#num) }
                    }
                    NumericType::RealVh => {
                        let num_str = &numeric_lit.value[..numeric_lit.value.len() - 2];
                        let num: f32 = num_str.parse().unwrap();
                        quote! { hyperchad_transformer::Number::RealVh(#num) }
                    }
                    NumericType::IntegerDvw => {
                        let num_str = &numeric_lit.value[..numeric_lit.value.len() - 3];
                        let num: i64 = num_str.parse().unwrap();
                        quote! { hyperchad_transformer::Number::IntegerDvw(#num) }
                    }
                    NumericType::RealDvw => {
                        let num_str = &numeric_lit.value[..numeric_lit.value.len() - 3];
                        let num: f32 = num_str.parse().unwrap();
                        quote! { hyperchad_transformer::Number::RealDvw(#num) }
                    }
                    NumericType::IntegerDvh => {
                        let num_str = &numeric_lit.value[..numeric_lit.value.len() - 3];
                        let num: i64 = num_str.parse().unwrap();
                        quote! { hyperchad_transformer::Number::IntegerDvh(#num) }
                    }
                    NumericType::RealDvh => {
                        let num_str = &numeric_lit.value[..numeric_lit.value.len() - 3];
                        let num: f32 = num_str.parse().unwrap();
                        quote! { hyperchad_transformer::Number::RealDvh(#num) }
                    }
                    NumericType::Integer => {
                        let num: i64 = numeric_lit.value.parse().unwrap();
                        quote! { hyperchad_transformer::Number::Integer(#num) }
                    }
                    NumericType::Real => {
                        let num: f32 = numeric_lit.value.parse().unwrap();
                        quote! { hyperchad_transformer::Number::Real(#num) }
                    }
                }
            }
            Markup::Lit(lit) => {
                match &lit.lit {
                    syn::Lit::Str(lit_str) => {
                        let value_str = lit_str.value();

                        // Try to parse different number formats from strings
                        if value_str.ends_with('%') {
                            let num_str = &value_str[..value_str.len() - 1];
                            if let Ok(num) = num_str.parse::<f32>() {
                                quote! { hyperchad_transformer::Number::RealPercent(#num) }
                            } else if let Ok(num) = num_str.parse::<i64>() {
                                quote! { hyperchad_transformer::Number::IntegerPercent(#num) }
                            } else {
                                quote! { hyperchad_transformer::parse::parse_number(#value_str).unwrap_or_default() }
                            }
                        } else if value_str.ends_with("vw") {
                            let num_str = &value_str[..value_str.len() - 2];
                            if let Ok(num) = num_str.parse::<f32>() {
                                quote! { hyperchad_transformer::Number::RealVw(#num) }
                            } else if let Ok(num) = num_str.parse::<i64>() {
                                quote! { hyperchad_transformer::Number::IntegerVw(#num) }
                            } else {
                                quote! { hyperchad_transformer::parse::parse_number(#value_str).unwrap_or_default() }
                            }
                        } else if value_str.ends_with("vh") {
                            let num_str = &value_str[..value_str.len() - 2];
                            if let Ok(num) = num_str.parse::<f32>() {
                                quote! { hyperchad_transformer::Number::RealVh(#num) }
                            } else if let Ok(num) = num_str.parse::<i64>() {
                                quote! { hyperchad_transformer::Number::IntegerVh(#num) }
                            } else {
                                quote! { hyperchad_transformer::parse::parse_number(#value_str).unwrap_or_default() }
                            }
                        } else if let Ok(num) = value_str.parse::<f32>() {
                            quote! { hyperchad_transformer::Number::Real(#num) }
                        } else if let Ok(num) = value_str.parse::<i64>() {
                            quote! { hyperchad_transformer::Number::Integer(#num) }
                        } else {
                            quote! { hyperchad_transformer::parse::parse_number(#value_str).unwrap_or_default() }
                        }
                    }
                    syn::Lit::Int(lit_int) => {
                        // For integer literals, convert directly to Number::Integer
                        quote! { hyperchad_transformer::Number::Integer(#lit_int) }
                    }
                    syn::Lit::Float(lit_float) => {
                        // For float literals, convert directly to Number::Real
                        quote! { hyperchad_transformer::Number::Real(#lit_float) }
                    }
                    _ => {
                        // For other literal types, try to convert using .into()
                        let lit = &lit.lit;
                        quote! { (#lit).into() }
                    }
                }
            }
            Markup::Splice { expr, .. } => {
                // Check if this expression might be an IfExpression - let it be handled at runtime
                Self::handle_potential_if_expression_for_number(&expr)
            }
            Markup::BraceSplice { items, .. } => {
                // For brace-wrapped items, handle both single items and string concatenation
                if items.len() == 1 {
                    Self::markup_to_number_tokens(items[0].clone())
                } else {
                    // Check if this looks like string concatenation (contains string literals)
                    let has_string_literals = items.iter().any(|item| {
                        matches!(item, Markup::Lit(lit) if matches!(lit.lit, syn::Lit::Str(_)))
                    });

                    if has_string_literals {
                        // Handle as string concatenation, then parse as number
                        let item_tokens: Vec<_> = items
                            .iter()
                            .map(|item| Self::markup_to_string_tokens(item.clone()))
                            .collect();
                        quote! {
                            {
                                let concatenated = vec![#(#item_tokens),*].join("");
                                hyperchad_transformer::parse::parse_number(&concatenated).unwrap_or_default()
                            }
                        }
                    } else {
                        // Handle as direct expression
                        let expr = Self::handle_brace_splice_expression(&items);
                        quote! {
                            {
                                let result = { #expr };
                                <hyperchad_transformer::Number as std::convert::From<_>>::from(result)
                            }
                        }
                    }
                }
            }
            _ => quote! { hyperchad_transformer::Number::Integer(0) },
        }
    }

    #[allow(clippy::too_many_lines)]
    fn handle_potential_if_expression_for_number(expr: &syn::Expr) -> TokenStream {
        // Check if this is a calc() function call
        if let syn::Expr::Call(call_expr) = expr {
            if let syn::Expr::Path(path_expr) = &*call_expr.func {
                if path_expr.path.segments.len() == 1 {
                    let function_name = path_expr.path.segments[0].ident.to_string();

                    match function_name.as_str() {
                        "calc" => {
                            // Handle calc() expressions
                            if call_expr.args.len() == 1 {
                                let calc_expr = &call_expr.args[0];
                                return Self::handle_calc_expression(calc_expr);
                            }
                        }
                        "min" => {
                            // Handle min() expressions outside of calc()
                            if call_expr.args.len() >= 2 {
                                // For multiple arguments, chain binary min operations
                                // min(a, b, c, d) becomes min(a, min(b, min(c, d)))
                                let mut result = Self::build_calculation_ast(
                                    &call_expr.args[call_expr.args.len() - 1],
                                );
                                for i in (0..call_expr.args.len() - 1).rev() {
                                    let left = Self::build_calculation_ast(&call_expr.args[i]);
                                    result = quote! {
                                        hyperchad_transformer::Calculation::Min(
                                            Box::new(#left),
                                            Box::new(#result)
                                        )
                                    };
                                }
                                return quote! {
                                    hyperchad_transformer::Number::Calc(#result)
                                };
                            }
                        }
                        "max" => {
                            // Handle max() expressions outside of calc()
                            if call_expr.args.len() >= 2 {
                                // For multiple arguments, chain binary max operations
                                // max(a, b, c, d) becomes max(a, max(b, max(c, d)))
                                let mut result = Self::build_calculation_ast(
                                    &call_expr.args[call_expr.args.len() - 1],
                                );
                                for i in (0..call_expr.args.len() - 1).rev() {
                                    let left = Self::build_calculation_ast(&call_expr.args[i]);
                                    result = quote! {
                                        hyperchad_transformer::Calculation::Max(
                                            Box::new(#left),
                                            Box::new(#result)
                                        )
                                    };
                                }
                                return quote! {
                                    hyperchad_transformer::Number::Calc(#result)
                                };
                            }
                        }
                        "clamp" => {
                            // Handle clamp() expressions outside of calc()
                            if call_expr.args.len() == 3 {
                                // clamp(min, preferred, max) = max(min, min(preferred, max))
                                let min_arg = Self::build_calculation_ast(&call_expr.args[0]);
                                let preferred_arg = Self::build_calculation_ast(&call_expr.args[1]);
                                let max_arg = Self::build_calculation_ast(&call_expr.args[2]);
                                return quote! {
                                    hyperchad_transformer::Number::Calc(
                                        hyperchad_transformer::Calculation::Max(
                                            Box::new(#min_arg),
                                            Box::new(hyperchad_transformer::Calculation::Min(
                                                Box::new(#preferred_arg),
                                                Box::new(#max_arg)
                                            ))
                                        )
                                    )
                                };
                            }
                        }
                        "percent" => {
                            // Helper function: percent(value) -> Number::*Percent
                            if call_expr.args.len() == 1 {
                                let value_expr = &call_expr.args[0];
                                return quote! {
                                    hyperchad_template::calc::to_percent_number(#value_expr)
                                };
                            }
                        }
                        "vh" => {
                            // Helper function: vh(value) -> Number::*Vh
                            if call_expr.args.len() == 1 {
                                let value_expr = &call_expr.args[0];
                                return quote! {
                                    hyperchad_template::calc::to_vh_number(#value_expr)
                                };
                            }
                        }
                        "vw" => {
                            // Helper function: vw(value) -> Number::*Vw
                            if call_expr.args.len() == 1 {
                                let value_expr = &call_expr.args[0];
                                return quote! {
                                    hyperchad_template::calc::to_vw_number(#value_expr)
                                };
                            }
                        }
                        "dvh" => {
                            // Helper function: dvh(value) -> Number::*Dvh
                            if call_expr.args.len() == 1 {
                                let value_expr = &call_expr.args[0];
                                return quote! {
                                    hyperchad_template::calc::to_dvh_number(#value_expr)
                                };
                            }
                        }
                        "dvw" => {
                            // Helper function: dvw(value) -> Number::*Dvw
                            if call_expr.args.len() == 1 {
                                let value_expr = &call_expr.args[0];
                                return quote! {
                                    hyperchad_template::calc::to_dvw_number(#value_expr)
                                };
                            }
                        }
                        _ => {}
                    }
                }
            }
        }

        // Fallback to original behavior
        quote! {
            {
                let val = #expr;
                <hyperchad_transformer::Number as std::convert::From<_>>::from(val)
            }
        }
    }

    /// Handle `calc()` expressions by recursively parsing mathematical operations
    /// and building Calculation AST structures
    fn handle_calc_expression(expr: &syn::Expr) -> TokenStream {
        let calculation_tokens = Self::build_calculation_ast(expr);
        quote! {
            hyperchad_transformer::Number::Calc(#calculation_tokens)
        }
    }

    /// Build a Calculation AST from a mathematical expression
    #[allow(clippy::too_many_lines)]
    fn build_calculation_ast(expr: &syn::Expr) -> TokenStream {
        match expr {
            // Handle binary operations: +, -, *, /
            syn::Expr::Binary(binary_expr) => {
                let left = Self::build_calculation_ast(&binary_expr.left);
                let right = Self::build_calculation_ast(&binary_expr.right);

                match binary_expr.op {
                    syn::BinOp::Add(_) => quote! {
                        hyperchad_transformer::Calculation::Add(
                            Box::new(#left),
                            Box::new(#right)
                        )
                    },
                    syn::BinOp::Sub(_) => quote! {
                        hyperchad_transformer::Calculation::Subtract(
                            Box::new(#left),
                            Box::new(#right)
                        )
                    },
                    syn::BinOp::Mul(_) => quote! {
                        hyperchad_transformer::Calculation::Multiply(
                            Box::new(#left),
                            Box::new(#right)
                        )
                    },
                    syn::BinOp::Div(_) => quote! {
                        hyperchad_transformer::Calculation::Divide(
                            Box::new(#left),
                            Box::new(#right)
                        )
                    },
                    _ => {
                        // For unsupported operations, just wrap both as numbers
                        quote! {
                            hyperchad_transformer::Calculation::Number(
                                Box::new({
                                    let left_val = {
                                        let left = #left;
                                        match left {
                                            hyperchad_transformer::Calculation::Number(n) => *n,
                                            _ => hyperchad_transformer::Number::Integer(0)
                                        }
                                    };
                                    left_val
                                })
                            )
                        }
                    }
                }
            }

            // Handle parenthesized expressions
            syn::Expr::Paren(paren_expr) => Self::build_calculation_ast(&paren_expr.expr),

            // Handle numeric literals with units (100%, 50vw, etc.)
            syn::Expr::Lit(expr_lit) => {
                let number_tokens = match &expr_lit.lit {
                    syn::Lit::Str(lit_str) => {
                        let value_str = lit_str.value();
                        Self::parse_number_literal_string(&value_str)
                    }
                    syn::Lit::Int(lit_int) => {
                        quote! { hyperchad_transformer::Number::Integer(#lit_int) }
                    }
                    syn::Lit::Float(lit_float) => {
                        quote! { hyperchad_transformer::Number::Real(#lit_float) }
                    }
                    _ => quote! { hyperchad_transformer::Number::Integer(0) },
                };

                quote! {
                    hyperchad_transformer::Calculation::Number(Box::new(#number_tokens))
                }
            }

            // Handle variable references
            syn::Expr::Path(path_expr) => {
                quote! {
                    hyperchad_transformer::Calculation::Number(Box::new({
                        let val = #path_expr;
                        hyperchad_template::calc::to_number(val)
                    }))
                }
            }

            // Handle function calls (percent(), vh(), vw(), min(), max(), clamp(), etc.)
            syn::Expr::Call(call_expr) => {
                if let syn::Expr::Path(path_expr) = &*call_expr.func {
                    if path_expr.path.segments.len() == 1 {
                        let function_name = path_expr.path.segments[0].ident.to_string();

                        match function_name.as_str() {
                            // CSS Math functions that return Calculation variants
                            "min" => {
                                if call_expr.args.len() >= 2 {
                                    // For multiple arguments, chain binary min operations
                                    // min(a, b, c, d) becomes min(a, min(b, min(c, d)))
                                    let mut result = Self::build_calculation_ast(
                                        &call_expr.args[call_expr.args.len() - 1],
                                    );
                                    for i in (0..call_expr.args.len() - 1).rev() {
                                        let left = Self::build_calculation_ast(&call_expr.args[i]);
                                        result = quote! {
                                            hyperchad_transformer::Calculation::Min(
                                                Box::new(#left),
                                                Box::new(#result)
                                            )
                                        };
                                    }
                                    return result;
                                }

                                return quote! {
                                    hyperchad_transformer::Calculation::Number(
                                        Box::new(hyperchad_transformer::Number::Integer(0))
                                    )
                                };
                            }
                            "max" => {
                                if call_expr.args.len() >= 2 {
                                    // For multiple arguments, chain binary max operations
                                    // max(a, b, c, d) becomes max(a, max(b, max(c, d)))
                                    let mut result = Self::build_calculation_ast(
                                        &call_expr.args[call_expr.args.len() - 1],
                                    );
                                    for i in (0..call_expr.args.len() - 1).rev() {
                                        let left = Self::build_calculation_ast(&call_expr.args[i]);
                                        result = quote! {
                                            hyperchad_transformer::Calculation::Max(
                                                Box::new(#left),
                                                Box::new(#result)
                                            )
                                        };
                                    }
                                    return result;
                                }

                                return quote! {
                                    hyperchad_transformer::Calculation::Number(
                                        Box::new(hyperchad_transformer::Number::Integer(0))
                                    )
                                };
                            }
                            "clamp" => {
                                if call_expr.args.len() == 3 {
                                    // clamp(min, preferred, max) = max(min, min(preferred, max))
                                    let min_arg = Self::build_calculation_ast(&call_expr.args[0]);
                                    let preferred_arg =
                                        Self::build_calculation_ast(&call_expr.args[1]);
                                    let max_arg = Self::build_calculation_ast(&call_expr.args[2]);

                                    return quote! {
                                        hyperchad_transformer::Calculation::Max(
                                            Box::new(#min_arg),
                                            Box::new(hyperchad_transformer::Calculation::Min(
                                                Box::new(#preferred_arg),
                                                Box::new(#max_arg)
                                            ))
                                        )
                                    };
                                }

                                return quote! {
                                    hyperchad_transformer::Calculation::Number(
                                        Box::new(hyperchad_transformer::Number::Integer(0))
                                    )
                                };
                            }
                            "percent" => {
                                // Helper function: percent(value) -> Number::*Percent
                                if call_expr.args.len() == 1 {
                                    let value_expr = &call_expr.args[0];
                                    return quote! {
                                        hyperchad_transformer::Calculation::Number(Box::new(
                                            hyperchad_template::calc::to_percent_number(#value_expr)
                                        ))
                                    };
                                }
                            }
                            "vh" => {
                                // Helper function: vh(value) -> Number::*Vh
                                if call_expr.args.len() == 1 {
                                    let value_expr = &call_expr.args[0];
                                    return quote! {
                                        hyperchad_transformer::Calculation::Number(Box::new(
                                            hyperchad_template::calc::to_vh_number(#value_expr)
                                        ))
                                    };
                                }
                            }
                            "vw" => {
                                // Helper function: vw(value) -> Number::*Vw
                                if call_expr.args.len() == 1 {
                                    let value_expr = &call_expr.args[0];
                                    return quote! {
                                        hyperchad_transformer::Calculation::Number(Box::new(
                                            hyperchad_template::calc::to_vw_number(#value_expr)
                                        ))
                                    };
                                }
                            }
                            "dvh" => {
                                // Helper function: dvh(value) -> Number::*Dvh
                                if call_expr.args.len() == 1 {
                                    let value_expr = &call_expr.args[0];
                                    return quote! {
                                        hyperchad_transformer::Calculation::Number(Box::new(
                                            hyperchad_template::calc::to_dvh_number(#value_expr)
                                        ))
                                    };
                                }
                            }
                            "dvw" => {
                                // Helper function: dvw(value) -> Number::*Dvw
                                if call_expr.args.len() == 1 {
                                    let value_expr = &call_expr.args[0];
                                    return quote! {
                                        hyperchad_transformer::Calculation::Number(Box::new(
                                            hyperchad_template::calc::to_dvw_number(#value_expr)
                                        ))
                                    };
                                }
                            }
                            _ => {
                                // Fallback: treat as regular expression
                                return quote! {
                                    hyperchad_transformer::Calculation::Number(Box::new({
                                        let val = #call_expr;
                                        hyperchad_template::calc::to_number(val)
                                    }))
                                };
                            }
                        }
                    }
                }

                // Fallback: treat as regular expression
                quote! {
                    hyperchad_transformer::Calculation::Number(Box::new({
                        let val = #call_expr;
                        hyperchad_template::calc::to_number(val)
                    }))
                }
            }

            // Handle any other expression types
            _ => {
                quote! {
                    hyperchad_transformer::Calculation::Number(Box::new({
                        let val = #expr;
                        hyperchad_template::calc::to_number(val)
                    }))
                }
            }
        }
    }

    /// Parse number literal strings (e.g., "100%", "50vw", "30") into Number tokens
    fn parse_number_literal_string(value_str: &str) -> TokenStream {
        if let Some(num_str) = value_str.strip_suffix('%') {
            if let Ok(num) = num_str.parse::<f32>() {
                quote! { hyperchad_transformer::Number::RealPercent(#num) }
            } else if let Ok(num) = num_str.parse::<i64>() {
                quote! { hyperchad_transformer::Number::IntegerPercent(#num) }
            } else {
                quote! { hyperchad_transformer::parse::parse_number(#value_str).unwrap_or_default() }
            }
        } else if let Some(num_str) = value_str.strip_suffix("dvw") {
            if let Ok(num) = num_str.parse::<f32>() {
                quote! { hyperchad_transformer::Number::RealDvw(#num) }
            } else if let Ok(num) = num_str.parse::<i64>() {
                quote! { hyperchad_transformer::Number::IntegerDvw(#num) }
            } else {
                quote! { hyperchad_transformer::parse::parse_number(#value_str).unwrap_or_default() }
            }
        } else if let Some(num_str) = value_str.strip_suffix("dvh") {
            if let Ok(num) = num_str.parse::<f32>() {
                quote! { hyperchad_transformer::Number::RealDvh(#num) }
            } else if let Ok(num) = num_str.parse::<i64>() {
                quote! { hyperchad_transformer::Number::IntegerDvh(#num) }
            } else {
                quote! { hyperchad_transformer::parse::parse_number(#value_str).unwrap_or_default() }
            }
        } else if let Some(num_str) = value_str.strip_suffix("vw") {
            if let Ok(num) = num_str.parse::<f32>() {
                quote! { hyperchad_transformer::Number::RealVw(#num) }
            } else if let Ok(num) = num_str.parse::<i64>() {
                quote! { hyperchad_transformer::Number::IntegerVw(#num) }
            } else {
                quote! { hyperchad_transformer::parse::parse_number(#value_str).unwrap_or_default() }
            }
        } else if let Some(num_str) = value_str.strip_suffix("vh") {
            if let Ok(num) = num_str.parse::<f32>() {
                quote! { hyperchad_transformer::Number::RealVh(#num) }
            } else if let Ok(num) = num_str.parse::<i64>() {
                quote! { hyperchad_transformer::Number::IntegerVh(#num) }
            } else {
                quote! { hyperchad_transformer::parse::parse_number(#value_str).unwrap_or_default() }
            }
        } else if let Ok(num) = value_str.parse::<f32>() {
            quote! { hyperchad_transformer::Number::Real(#num) }
        } else if let Ok(num) = value_str.parse::<i64>() {
            quote! { hyperchad_transformer::Number::Integer(#num) }
        } else {
            quote! { hyperchad_transformer::parse::parse_number(#value_str).unwrap_or_default() }
        }
    }

    fn markup_to_enum_tokens(enum_name: &str, value: Markup<NoElement>) -> TokenStream {
        match value {
            Markup::Lit(lit) => {
                if let syn::Lit::Str(lit_str) = &lit.lit {
                    let value_str = lit_str.value();
                    let enum_ident = format_ident!("{}", enum_name);

                    // Convert kebab-case to PascalCase for enum variants
                    let variant_name = kebab_to_pascal_case(&value_str);
                    let variant_ident = format_ident!("{}", variant_name);

                    quote! { hyperchad_transformer_models::#enum_ident::#variant_ident }
                } else {
                    // For non-string literals, use the literal directly as an expression
                    let lit = &lit.lit;
                    quote! { (#lit).into() }
                }
            }
            Markup::Splice { expr, .. } => {
                // Check if this is a simple identifier that should be converted to an enum variant
                if let syn::Expr::Path(expr_path) = &*expr {
                    if expr_path.path.segments.len() == 1 && expr_path.qself.is_none() {
                        let identifier_name = expr_path.path.segments[0].ident.to_string();

                        // Only accept kebab-case identifiers (lowercase, may contain hyphens)
                        // Reject PascalCase identifiers to enforce kebab-case convention
                        if identifier_name
                            .chars()
                            .next()
                            .is_some_and(char::is_uppercase)
                        {
                            // This is PascalCase - don't convert, let it fall through to normal expression handling
                            // This will cause a compile error, enforcing kebab-case usage
                        } else {
                            // Convert kebab-case to PascalCase
                            let variant_name = kebab_to_pascal_case(&identifier_name);
                            let enum_ident = format_ident!("{}", enum_name);
                            let variant_ident = format_ident!("{}", variant_name);

                            return quote! { hyperchad_transformer_models::#enum_ident::#variant_ident };
                        }
                    }
                }

                // Check if this is a string literal that should be converted to an enum variant
                if let syn::Expr::Lit(syn::ExprLit {
                    lit: syn::Lit::Str(lit_str),
                    ..
                }) = &*expr
                {
                    let identifier_name = lit_str.value();

                    // Only accept kebab-case identifiers (lowercase, may contain hyphens)
                    // Reject PascalCase identifiers to enforce kebab-case convention
                    if identifier_name
                        .chars()
                        .next()
                        .is_some_and(char::is_uppercase)
                    {
                        // This is PascalCase - don't convert, let it fall through to normal expression handling
                        // This will cause a compile error, enforcing kebab-case usage
                    } else {
                        // Convert kebab-case to PascalCase
                        let variant_name = kebab_to_pascal_case(&identifier_name);
                        let enum_ident = format_ident!("{}", enum_name);
                        let variant_ident = format_ident!("{}", variant_name);

                        return quote! { hyperchad_transformer_models::#enum_ident::#variant_ident };
                    }
                }

                // Handle potential IfExpression for enums
                Self::handle_potential_if_expression_for_enum(enum_name, &expr)
            }
            Markup::BraceSplice { items, .. } => {
                // For brace-wrapped items, treat the entire content as a single expression
                if items.len() == 1 {
                    Self::markup_to_enum_tokens(enum_name, items[0].clone())
                } else {
                    let expr = Self::handle_brace_splice_expression(&items);
                    let enum_ident = format_ident!("{}", enum_name);
                    quote! {
                        {
                            let result = { #expr };
                            <hyperchad_transformer_models::#enum_ident as std::convert::From<_>>::from(result)
                        }
                    }
                }
            }
            _ => {
                let enum_ident = format_ident!("{}", enum_name);
                quote! { hyperchad_transformer_models::#enum_ident::default() }
            }
        }
    }

    fn handle_potential_if_expression_for_enum(enum_name: &str, expr: &syn::Expr) -> TokenStream {
        let enum_ident = format_ident!("{}", enum_name);
        quote! {
            {
                let val = #expr;
                <hyperchad_transformer_models::#enum_ident as std::convert::From<_>>::from(val)
            }
        }
    }

    #[allow(clippy::too_many_lines)]
    fn markup_to_color_tokens(value: Markup<NoElement>) -> TokenStream {
        match value {
            Markup::Lit(lit) => {
                if let syn::Lit::Str(lit_str) = &lit.lit {
                    let value_str = lit_str.value();

                    // Handle common color names
                    match value_str.to_lowercase().as_str() {
                        "black" => quote! { hyperchad_color::Color::BLACK },
                        "white" => quote! { hyperchad_color::Color::WHITE },
                        "red" => quote! { hyperchad_color::Color::from_hex("#FF0000") },
                        "green" => quote! { hyperchad_color::Color::from_hex("#00FF00") },
                        "blue" => quote! { hyperchad_color::Color::from_hex("#0000FF") },
                        "gray" => quote! { hyperchad_color::Color::from_hex("#808080") },
                        "yellow" => quote! { hyperchad_color::Color::from_hex("#FFFF00") },
                        "cyan" => quote! { hyperchad_color::Color::from_hex("#00FFFF") },
                        "magenta" => quote! { hyperchad_color::Color::from_hex("#FF00FF") },
                        "orange" => quote! { hyperchad_color::Color::from_hex("#FFA500") },
                        "purple" => quote! { hyperchad_color::Color::from_hex("#800080") },
                        "pink" => quote! { hyperchad_color::Color::from_hex("#FFC0CB") },
                        "brown" => quote! { hyperchad_color::Color::from_hex("#A52A2A") },
                        // Only parse as hex if it starts with #
                        _ if value_str.starts_with('#') => {
                            quote! { hyperchad_color::Color::from_hex(#value_str) }
                        }
                        // Default fallback
                        _ => quote! { hyperchad_color::Color::BLACK },
                    }
                } else {
                    // For non-string literals, use the literal directly as an expression
                    let lit = &lit.lit;
                    quote! { (#lit).into() }
                }
            }
            Markup::Splice { expr, .. } => {
                // Check if this is a simple identifier that could be a color name
                if let syn::Expr::Path(expr_path) = &*expr {
                    if expr_path.path.segments.len() == 1 && expr_path.qself.is_none() {
                        let identifier_name = expr_path.path.segments[0].ident.to_string();

                        // Check if it's a known color name (must be lowercase)
                        match identifier_name.as_str() {
                            "black" => return quote! { hyperchad_color::Color::BLACK },
                            "white" => return quote! { hyperchad_color::Color::WHITE },
                            "red" => return quote! { hyperchad_color::Color::from_hex("#FF0000") },
                            "green" => {
                                return quote! { hyperchad_color::Color::from_hex("#00FF00") };
                            }
                            "blue" => return quote! { hyperchad_color::Color::from_hex("#0000FF") },
                            "gray" => return quote! { hyperchad_color::Color::from_hex("#808080") },
                            "yellow" => {
                                return quote! { hyperchad_color::Color::from_hex("#FFFF00") };
                            }
                            "cyan" => return quote! { hyperchad_color::Color::from_hex("#00FFFF") },
                            "magenta" => {
                                return quote! { hyperchad_color::Color::from_hex("#FF00FF") };
                            }
                            "orange" => {
                                return quote! { hyperchad_color::Color::from_hex("#FFA500") };
                            }
                            "purple" => {
                                return quote! { hyperchad_color::Color::from_hex("#800080") };
                            }
                            "pink" => return quote! { hyperchad_color::Color::from_hex("#FFC0CB") },
                            "brown" => {
                                return quote! { hyperchad_color::Color::from_hex("#A52A2A") };
                            }
                            _ => {
                                // Check if it looks like a raw hex identifier (starts with #)
                                if let Some(hex_part) = identifier_name.strip_prefix('#') {
                                    // Remove the # and check if the rest are hex digits
                                    if (hex_part.len() == 3 || hex_part.len() == 6)
                                        && hex_part.chars().all(|c| c.is_ascii_hexdigit())
                                    {
                                        return quote! { hyperchad_color::Color::from_hex(#identifier_name) };
                                    }
                                }
                            }
                        }
                    }
                } else if let syn::Expr::Lit(expr_lit) = &*expr {
                    // Check if this is a string literal that looks like a color name
                    if let syn::Lit::Str(lit_str) = &expr_lit.lit {
                        let identifier_name = lit_str.value();

                        // Check if it's a known color name (must be lowercase)
                        match identifier_name.as_str() {
                            "black" => return quote! { hyperchad_color::Color::BLACK },
                            "white" => return quote! { hyperchad_color::Color::WHITE },
                            "red" => return quote! { hyperchad_color::Color::from_hex("#FF0000") },
                            "green" => {
                                return quote! { hyperchad_color::Color::from_hex("#00FF00") };
                            }
                            "blue" => return quote! { hyperchad_color::Color::from_hex("#0000FF") },
                            "gray" => return quote! { hyperchad_color::Color::from_hex("#808080") },
                            "yellow" => {
                                return quote! { hyperchad_color::Color::from_hex("#FFFF00") };
                            }
                            "cyan" => return quote! { hyperchad_color::Color::from_hex("#00FFFF") },
                            "magenta" => {
                                return quote! { hyperchad_color::Color::from_hex("#FF00FF") };
                            }
                            "orange" => {
                                return quote! { hyperchad_color::Color::from_hex("#FFA500") };
                            }
                            "purple" => {
                                return quote! { hyperchad_color::Color::from_hex("#800080") };
                            }
                            "pink" => return quote! { hyperchad_color::Color::from_hex("#FFC0CB") },
                            "brown" => {
                                return quote! { hyperchad_color::Color::from_hex("#A52A2A") };
                            }
                            _ => {
                                // Check if it looks like a hex color (starts with #)
                                if identifier_name.starts_with('#') {
                                    return quote! { hyperchad_color::Color::from_hex(#identifier_name) };
                                }
                            }
                        }
                    }
                }

                // Fallback to existing behavior for expressions
                Self::handle_potential_if_expression_for_color(&expr)
            }
            Markup::BraceSplice { items, .. } => {
                // For brace-wrapped items, treat the entire content as a single expression
                if items.len() == 1 {
                    Self::markup_to_color_tokens(items[0].clone())
                } else {
                    let expr = Self::handle_brace_splice_expression(&items);
                    quote! {
                        {
                            let result = { #expr };
                            <hyperchad_color::Color as std::convert::From<_>>::from(result)
                        }
                    }
                }
            }
            _ => quote! { hyperchad_color::Color::BLACK },
        }
    }

    fn handle_potential_if_expression_for_color(expr: &syn::Expr) -> TokenStream {
        quote! {
            {
                let val = #expr;
                <hyperchad_color::Color as std::convert::From<_>>::from(val)
            }
        }
    }

    fn markup_to_bool_tokens(value: Markup<NoElement>) -> TokenStream {
        match value {
            Markup::Lit(lit) => {
                match &lit.lit {
                    syn::Lit::Str(lit_str) => {
                        let value_str = lit_str.value();
                        let bool_val =
                            matches!(value_str.to_lowercase().as_str(), "true" | "1" | "yes");
                        quote! { #bool_val }
                    }
                    syn::Lit::Bool(lit_bool) => {
                        let bool_val = lit_bool.value;
                        quote! { #bool_val }
                    }
                    syn::Lit::Int(lit_int) => {
                        // Convert integer to bool (0 = false, anything else = true)
                        quote! { (#lit_int != 0) }
                    }
                    _ => {
                        // For other literal types, use the literal directly as an expression
                        let lit = &lit.lit;
                        quote! { (#lit).into() }
                    }
                }
            }
            Markup::Splice { expr, .. } => Self::handle_potential_if_expression_for_bool(&expr),
            Markup::BraceSplice { items, .. } => {
                // For brace-wrapped items, treat the entire content as a single expression
                if items.len() == 1 {
                    Self::markup_to_bool_tokens(items[0].clone())
                } else {
                    let expr = Self::handle_brace_splice_expression(&items);
                    quote! {
                        {
                            use hyperchad_template::ToBool;
                            let result = { #expr };
                            result.to_bool()
                        }
                    }
                }
            }
            _ => quote! { false },
        }
    }

    fn handle_potential_if_expression_for_bool(expr: &syn::Expr) -> TokenStream {
        quote! {
            {
                use hyperchad_template::ToBool;
                let val = #expr;
                val.to_bool()
            }
        }
    }

    /// Helper function to handle `BraceSplice` by reconstructing the expression as a cohesive unit
    /// This properly supports `if_responsive()` and other complex logic patterns
    fn handle_brace_splice_expression(items: &[Markup<NoElement>]) -> TokenStream {
        let combined_tokens: Vec<_> = items
            .iter()
            .map(|item| match item {
                Markup::Lit(lit) => quote! { #lit },
                Markup::Splice { expr, .. } => quote! { #expr },
                Markup::BraceSplice { items, .. } => {
                    let nested_tokens: Vec<_> = items
                        .iter()
                        .map(|nested_item| match nested_item {
                            Markup::Lit(lit) => quote! { #lit },
                            Markup::Splice { expr, .. } => quote! { #expr },
                            _ => quote! { () },
                        })
                        .collect();
                    quote! { #(#nested_tokens)* }
                }
                _ => quote! { () },
            })
            .collect();

        quote! { #(#combined_tokens)* }
    }

    fn control_flow<E: Into<Element>>(
        &self,
        control_flow: ControlFlow<E>,
        build: &mut Builder,
    ) -> Result<(), String> {
        match control_flow.kind {
            ControlFlowKind::If(if_) => self.control_flow_if(*if_, build)?,
            ControlFlowKind::Let(let_) => Self::control_flow_let(&let_, build),
            ControlFlowKind::For(for_) => self.control_flow_for(*for_, build)?,
            ControlFlowKind::While(while_) => self.control_flow_while(*while_, build)?,
            ControlFlowKind::Match(match_) => self.control_flow_match(*match_, build)?,
        }
        Ok(())
    }

    fn control_flow_if<E: Into<Element>>(
        &self,
        IfExpr {
            if_token: _,
            cond,
            then_branch,
            else_branch,
        }: IfExpr<E>,
        build: &mut Builder,
    ) -> Result<(), String> {
        let then_body = {
            let mut build = self.builder();
            self.markups(then_branch.markups, &mut build)?;
            build.finish()
        };

        // Generate the condition based on its type
        let condition_tokens = match &cond {
            IfCondition::Expr(expr) => quote! { (#expr) },
            IfCondition::Let {
                let_token,
                pat,
                eq_token,
                expr,
            } => {
                quote! { #let_token #pat #eq_token #expr }
            }
        };

        match else_branch {
            Some((_, _, else_branch)) => {
                let else_body = {
                    let mut build = self.builder();
                    self.control_flow_if_or_block(*else_branch, &mut build)?;
                    build.finish()
                };
                build.push_tokens(quote! {
                    if #condition_tokens {
                        #then_body
                    } else {
                        #else_body
                    }
                });
            }
            None => {
                build.push_tokens(quote! {
                    if #condition_tokens {
                        #then_body
                    }
                });
            }
        }
        Ok(())
    }

    fn control_flow_if_or_block<E: Into<Element>>(
        &self,
        if_or_block: IfOrBlock<E>,
        build: &mut Builder,
    ) -> Result<(), String> {
        match if_or_block {
            IfOrBlock::If(if_) => self.control_flow_if(if_, build)?,
            IfOrBlock::Block(block) => self.markups(block.markups, build)?,
        }
        Ok(())
    }

    fn control_flow_let(let_: &Local, build: &mut Builder) {
        build.push_tokens(quote!(#let_;));
    }

    fn control_flow_for<E: Into<Element>>(
        &self,
        ForExpr {
            for_token: _,
            pat,
            in_token: _,
            expr,
            body,
        }: ForExpr<E>,
        build: &mut Builder,
    ) -> Result<(), String> {
        let body_tokens = {
            let mut build = self.builder();
            self.markups(body.markups, &mut build)?;
            build.finish()
        };

        build.push_tokens(quote! {
            for #pat in (#expr) {
                #body_tokens
            }
        });
        Ok(())
    }

    fn control_flow_while<E: Into<Element>>(
        &self,
        WhileExpr {
            while_token: _,
            cond,
            body,
        }: WhileExpr<E>,
        build: &mut Builder,
    ) -> Result<(), String> {
        let body_tokens = {
            let mut build = self.builder();
            self.markups(body.markups, &mut build)?;
            build.finish()
        };

        build.push_tokens(quote! {
            while (#cond) {
                #body_tokens
            }
        });
        Ok(())
    }

    fn control_flow_match<E: Into<Element>>(
        &self,
        MatchExpr {
            match_token: _,
            expr,
            brace_token: _,
            arms,
        }: MatchExpr<E>,
        build: &mut Builder,
    ) -> Result<(), String> {
        let mut arm_tokens = Vec::new();
        for arm in arms {
            let pat = &arm.pat;
            let guard = arm.guard.as_ref().map(|(if_token, guard_expr)| {
                quote! { #if_token #guard_expr }
            });
            let body = {
                let mut build = self.builder();
                self.markup(arm.body, &mut build)?;
                build.finish()
            };

            arm_tokens.push(quote! {
                #pat #guard => { #body }
            });
        }

        build.push_tokens(quote! {
            match (#expr) {
                #(#arm_tokens,)*
            }
        });
        Ok(())
    }

    fn markup_to_border_tokens(value: Markup<NoElement>) -> TokenStream {
        match value {
            Markup::Lit(lit) => {
                if let syn::Lit::Str(lit_str) = &lit.lit {
                    let value_str = lit_str.value();
                    // Parse border format: "width, color" (e.g., "2, #222")
                    if let Some((width_str, color_str)) = value_str.split_once(',') {
                        let width_str = width_str.trim();
                        let color_str = color_str.trim();

                        // Parse width
                        let width_tokens = if let Ok(num) = width_str.parse::<f32>() {
                            quote! { hyperchad_transformer::Number::Real(#num) }
                        } else if let Ok(num) = width_str.parse::<i64>() {
                            quote! { hyperchad_transformer::Number::Integer(#num) }
                        } else {
                            quote! { hyperchad_transformer::parse::parse_number(#width_str).unwrap_or_default() }
                        };

                        // Parse color using the existing color parsing logic
                        let color_tokens = Self::parse_color_string(color_str);

                        quote! { (#color_tokens, #width_tokens) }
                    } else {
                        // Invalid format, return default
                        quote! { (hyperchad_color::Color::BLACK, hyperchad_transformer::Number::Integer(1)) }
                    }
                } else {
                    // For non-string literals, assume it's a border tuple expression
                    let lit = &lit.lit;
                    quote! {
                        {
                            use hyperchad_template::IntoBorder;
                            (#lit).into_border()
                        }
                    }
                }
            }
            Markup::Splice { expr, .. } => {
                // For expressions, use the IntoBorder trait for flexible conversion
                quote! {
                    {
                        use hyperchad_template::IntoBorder;
                        (#expr).into_border()
                    }
                }
            }
            Markup::BraceSplice { items, .. } => {
                // For brace-wrapped items, handle both single items and complex expressions
                if items.len() == 1 {
                    Self::markup_to_border_tokens(items[0].clone())
                } else if items.len() == 2 {
                    // Handle explicit tuple syntax: {width, color} or {(width, color)}
                    let first_tokens = Self::handle_brace_splice_item(&items[0]);
                    let second_tokens = Self::handle_brace_splice_item(&items[1]);
                    quote! {
                        {
                            use hyperchad_template::IntoBorder;
                            (#first_tokens, #second_tokens).into_border()
                        }
                    }
                } else {
                    // Complex expression - let the IntoBorder trait handle it
                    let expr = Self::handle_brace_splice_expression(&items);
                    quote! {
                        {
                            use hyperchad_template::IntoBorder;
                            let result = { #expr };
                            result.into_border()
                        }
                    }
                }
            }
            _ => {
                quote! { (hyperchad_color::Color::BLACK, hyperchad_transformer::Number::Integer(1)) }
            }
        }
    }

    fn handle_brace_splice_item(item: &Markup<NoElement>) -> TokenStream {
        match item {
            Markup::Lit(lit) => quote! { #lit },
            Markup::Splice { expr, .. } => quote! { #expr },
            _ => quote! { () },
        }
    }

    fn parse_color_string(color_str: &str) -> TokenStream {
        match color_str {
            "black" => quote! { hyperchad_color::Color::BLACK },
            "white" => quote! { hyperchad_color::Color::WHITE },
            "red" => quote! { hyperchad_color::Color::from_hex("#FF0000") },
            "green" => quote! { hyperchad_color::Color::from_hex("#00FF00") },
            "blue" => quote! { hyperchad_color::Color::from_hex("#0000FF") },
            "gray" => quote! { hyperchad_color::Color::from_hex("#808080") },
            "yellow" => quote! { hyperchad_color::Color::from_hex("#FFFF00") },
            "cyan" => quote! { hyperchad_color::Color::from_hex("#00FFFF") },
            "magenta" => quote! { hyperchad_color::Color::from_hex("#FF00FF") },
            "orange" => quote! { hyperchad_color::Color::from_hex("#FFA500") },
            "purple" => quote! { hyperchad_color::Color::from_hex("#800080") },
            "pink" => quote! { hyperchad_color::Color::from_hex("#FFC0CB") },
            "brown" => quote! { hyperchad_color::Color::from_hex("#A52A2A") },
            _ if color_str.starts_with('#') => {
                quote! { hyperchad_color::Color::from_hex(#color_str) }
            }
            _ => quote! { hyperchad_color::Color::BLACK }, // Fallback to black for unknown colors
        }
    }

    fn extract_field_name_from_assignment(assignment: &TokenStream) -> String {
        // Extract the field name from assignments like "field_name: Some(value)"
        let assignment_str = assignment.to_string();
        assignment_str.find(':').map_or_else(
            || {
                assignment_str
                    .split_whitespace()
                    .next()
                    .unwrap_or("unknown")
                    .to_string()
            },
            |colon_pos| assignment_str[..colon_pos].trim().to_string(),
        )
    }

    #[allow(clippy::too_many_lines)]
    fn markup_to_flex_tokens(
        value: Markup<NoElement>,
        _grow: Option<&(AttributeName, AttributeType)>,
        _shrink: Option<&(AttributeName, AttributeType)>,
        _basis: Option<&(AttributeName, AttributeType)>,
    ) -> TokenStream {
        match value {
            Markup::Lit(lit) => {
                match &lit.lit {
                    syn::Lit::Str(lit_str) => {
                        let value_str = lit_str.value();
                        // Parse flex format: "grow shrink basis" (e.g., "1 0 0" or "1" or "1 0")
                        let parts: Vec<&str> = value_str.split_whitespace().collect();

                        match parts.len() {
                            1 => {
                                // Only grow specified
                                let grow_str = parts[0];
                                let grow_tokens = if let Ok(num) = grow_str.parse::<f32>() {
                                    quote! { hyperchad_transformer::Number::Real(#num) }
                                } else if let Ok(num) = grow_str.parse::<i64>() {
                                    quote! { hyperchad_transformer::Number::Integer(#num) }
                                } else {
                                    quote! { hyperchad_transformer::parse::parse_number(#grow_str).unwrap_or_default() }
                                };

                                quote! { hyperchad_transformer::Flex {
                                    grow: #grow_tokens,
                                    ..Default::default()
                                } }
                            }
                            2 => {
                                // Grow and shrink specified
                                let grow_str = parts[0];
                                let shrink_str = parts[1];

                                let grow_tokens = if let Ok(num) = grow_str.parse::<f32>() {
                                    quote! { hyperchad_transformer::Number::Real(#num) }
                                } else if let Ok(num) = grow_str.parse::<i64>() {
                                    quote! { hyperchad_transformer::Number::Integer(#num) }
                                } else {
                                    quote! { hyperchad_transformer::parse::parse_number(#grow_str).unwrap_or_default() }
                                };

                                let shrink_tokens = if let Ok(num) = shrink_str.parse::<f32>() {
                                    quote! { hyperchad_transformer::Number::Real(#num) }
                                } else if let Ok(num) = shrink_str.parse::<i64>() {
                                    quote! { hyperchad_transformer::Number::Integer(#num) }
                                } else {
                                    quote! { hyperchad_transformer::parse::parse_number(#shrink_str).unwrap_or_default() }
                                };

                                quote! { hyperchad_transformer::Flex {
                                    grow: #grow_tokens,
                                    shrink: #shrink_tokens,
                                    ..Default::default()
                                } }
                            }
                            3 => {
                                // All three values specified
                                let grow_str = parts[0];
                                let shrink_str = parts[1];
                                let basis_str = parts[2];

                                let grow_tokens = if let Ok(num) = grow_str.parse::<f32>() {
                                    quote! { hyperchad_transformer::Number::Real(#num) }
                                } else if let Ok(num) = grow_str.parse::<i64>() {
                                    quote! { hyperchad_transformer::Number::Integer(#num) }
                                } else {
                                    quote! { hyperchad_transformer::parse::parse_number(#grow_str).unwrap_or_default() }
                                };

                                let shrink_tokens = if let Ok(num) = shrink_str.parse::<f32>() {
                                    quote! { hyperchad_transformer::Number::Real(#num) }
                                } else if let Ok(num) = shrink_str.parse::<i64>() {
                                    quote! { hyperchad_transformer::Number::Integer(#num) }
                                } else {
                                    quote! { hyperchad_transformer::parse::parse_number(#shrink_str).unwrap_or_default() }
                                };

                                let basis_tokens = if let Ok(num) = basis_str.parse::<f32>() {
                                    quote! { hyperchad_transformer::Number::Real(#num) }
                                } else if let Ok(num) = basis_str.parse::<i64>() {
                                    quote! { hyperchad_transformer::Number::Integer(#num) }
                                } else {
                                    quote! { hyperchad_transformer::parse::parse_number(#basis_str).unwrap_or_default() }
                                };

                                quote! { hyperchad_transformer::Flex {
                                    grow: #grow_tokens,
                                    shrink: #shrink_tokens,
                                    basis: #basis_tokens,
                                } }
                            }
                            _ => {
                                // Invalid format, return default flex
                                quote! { hyperchad_transformer::Flex::default() }
                            }
                        }
                    }
                    syn::Lit::Int(lit_int) => {
                        // For integer literals, treat as flex grow value
                        quote! { hyperchad_transformer::Flex {
                            grow: hyperchad_transformer::Number::Integer(#lit_int),
                            ..Default::default()
                        } }
                    }
                    syn::Lit::Float(lit_float) => {
                        // For float literals, treat as flex grow value
                        quote! { hyperchad_transformer::Flex {
                            grow: hyperchad_transformer::Number::Real(#lit_float),
                            ..Default::default()
                        } }
                    }
                    _ => {
                        // For other literal types, assume it's a flex struct expression
                        let lit = &lit.lit;
                        quote! { (#lit).into() }
                    }
                }
            }
            Markup::Splice { expr, .. } => {
                quote! { (#expr).into() }
            }
            Markup::BraceSplice { items, .. } => {
                // For brace-wrapped items, treat the entire content as a single expression
                if items.len() == 1 {
                    Self::markup_to_flex_tokens(items[0].clone(), None, None, None)
                } else {
                    let expr = Self::handle_brace_splice_expression(&items);
                    quote! {
                        {
                            let result = { #expr };
                            result.into()
                        }
                    }
                }
            }
            _ => {
                quote! { hyperchad_transformer::Flex::default() }
            }
        }
    }

    fn markup_to_string_vec_tokens(value: Markup<NoElement>) -> TokenStream {
        match value {
            Markup::Lit(lit) => {
                if let syn::Lit::Str(lit_str) = &lit.lit {
                    let value_str = lit_str.value();
                    // Parse comma-separated font families, matching html.rs implementation
                    let families: Vec<String> = value_str
                        .split(',')
                        .map(str::trim)
                        .filter(|x| !x.is_empty())
                        .map(ToString::to_string)
                        .collect();

                    quote! { vec![#(#families.to_string()),*] }
                } else {
                    // For non-string literals, assume it's already a Vec<String> or can be converted
                    let lit = &lit.lit;
                    quote! { (#lit).into() }
                }
            }
            Markup::Splice { expr, .. } => {
                // For expressions, assume they evaluate to either a String (comma-separated) or Vec<String>
                quote! {
                    {
                        let val = #expr;
                        // Convert to Vec<String>, handling both String and Vec<String> inputs
                        match val.to_string().contains(',') {
                            true => {
                                // If it contains commas, parse as comma-separated string
                                val.to_string()
                                    .split(',')
                                    .map(str::trim)
                                    .filter(|x| !x.is_empty())
                                    .map(ToString::to_string)
                                    .collect::<Vec<String>>()
                            }
                            false => {
                                // Single value or already Vec<String>
                                vec![val.to_string()]
                            }
                        }
                    }
                }
            }
            Markup::BraceSplice { items, .. } => {
                // For brace-wrapped items, handle like single item if only one
                if items.len() == 1 {
                    Self::markup_to_string_vec_tokens(items[0].clone())
                } else {
                    // Multiple items - concatenate as comma-separated string then parse
                    let item_tokens: Vec<_> = items
                        .iter()
                        .map(|item| Self::markup_to_string_tokens(item.clone()))
                        .collect();
                    quote! {
                        {
                            let combined = vec![#(#item_tokens),*].join(",");
                            combined.split(',')
                                .map(str::trim)
                                .filter(|x| !x.is_empty())
                                .map(ToString::to_string)
                                .collect::<Vec<String>>()
                        }
                    }
                }
            }
            _ => quote! { vec![String::new()] },
        }
    }

    fn markup_to_text_decoration_tokens(value: Markup<NoElement>) -> TokenStream {
        match value {
            Markup::Lit(lit) => {
                if let syn::Lit::Str(lit_str) = &lit.lit {
                    let value_str = lit_str.value();
                    // Simple text-decoration parsing - just check for common values
                    if value_str.contains("underline") {
                        quote! { hyperchad_transformer::TextDecoration {
                            color: None,
                            line: vec![hyperchad_transformer_models::TextDecorationLine::Underline],
                            style: None,
                            thickness: None,
                        } }
                    } else if value_str.contains("none") {
                        quote! { hyperchad_transformer::TextDecoration {
                            color: None,
                            line: vec![hyperchad_transformer_models::TextDecorationLine::None],
                            style: None,
                            thickness: None,
                        } }
                    } else {
                        quote! { hyperchad_transformer::TextDecoration::default() }
                    }
                } else {
                    let lit = &lit.lit;
                    quote! { (#lit).into() }
                }
            }
            Markup::Splice { expr, .. } => {
                quote! { (#expr) }
            }
            Markup::BraceSplice { items, .. } => {
                // For brace-wrapped items, handle like single item if only one
                if items.len() == 1 {
                    Self::markup_to_text_decoration_tokens(items[0].clone())
                } else {
                    let expr = Self::handle_brace_splice_expression(&items);
                    quote! {
                        {
                            let result = { #expr };
                            result.into()
                        }
                    }
                }
            }
            _ => {
                quote! { hyperchad_transformer::TextDecoration::default() }
            }
        }
    }

    /// Extract DSL content from fx calls, supporting only `fx { ... }` syntax
    fn extract_fx_dsl_content(markup: &Markup<NoElement>) -> Option<TokenStream> {
        match markup {
            Markup::BraceSplice { items, .. } => {
                // Check if this is fx followed by block content
                if !items.is_empty() {
                    // Check if the first item is an fx identifier
                    if let Some(Markup::Splice { expr, .. }) = items.first() {
                        if let syn::Expr::Path(path_expr) = expr.as_ref() {
                            if let Some(ident) = path_expr.path.get_ident() {
                                if ident == "fx" {
                                    // This is fx followed by block content
                                    if items.len() == 1 {
                                        // Single fx identifier without content - return empty DSL
                                        return Some(quote! {});
                                    } else if items.len() == 2 {
                                        // fx followed by one block expression
                                        if let Markup::Splice { expr, .. } = &items[1] {
                                            // Extract the block expression content
                                            if let syn::Expr::Block(block_expr) = expr.as_ref() {
                                                // Extract the statements from the block
                                                let stmts = &block_expr.block.stmts;
                                                return Some(quote! { #(#stmts)* });
                                            }

                                            // Not a block expression, return the expression directly
                                            return Some(quote! { #expr });
                                        }
                                    } else {
                                        // Multiple items - combine them
                                        let content_items = &items[1..];
                                        let content_tokens = content_items
                                            .iter()
                                            .map(|item| match item {
                                                Markup::Splice { expr, .. } => {
                                                    if let syn::Expr::Block(block_expr) =
                                                        expr.as_ref()
                                                    {
                                                        let stmts = &block_expr.block.stmts;
                                                        quote! { #(#stmts)* }
                                                    } else {
                                                        quote! { #expr }
                                                    }
                                                }
                                                Markup::Lit(lit) => {
                                                    let lit_token = &lit.lit;
                                                    quote! { #lit_token }
                                                }
                                                _ => quote! {},
                                            })
                                            .collect::<Vec<_>>();

                                        return Some(quote! { #(#content_tokens)* });
                                    }
                                }
                            }
                        }
                    }
                }

                // Not an fx pattern - check if it's a single item that might be fx content
                if items.len() == 1 {
                    Self::extract_fx_dsl_content(&items[0])
                } else {
                    None
                }
            }
            _ => None,
        }
    }

    fn generate_compile_time_optimized_dsl_action(dsl_tokens: &TokenStream) -> TokenStream {
        // Use the main optimized macro and extract the single ActionEffect from the Vec
        // The main macro now includes optimizations internally
        // (hyperchad_template_actions_dsl::actions_dsl! { #dsl_tokens }).into_iter().next().unwrap()
        quote! {
            (hyperchad_template_actions_dsl::actions_dsl! { #dsl_tokens }).into()
        }
    }
}

#[allow(clippy::type_complexity)]
fn split_attrs(
    attrs: Vec<ContainerAttribute>,
) -> (
    Vec<(ContainerNameOrMarkup, Option<Expr>)>,
    Option<ContainerNameOrMarkup>,
    Vec<(AttributeName, AttributeType)>,
) {
    let mut classes = vec![];
    let mut id = None;
    let mut named_attrs = vec![];

    for attr in attrs {
        match attr {
            ContainerAttribute::Class { name, toggler, .. } => {
                classes.push((name, toggler.map(|toggler| toggler.cond)));
            }
            ContainerAttribute::Id { name, .. } => id = Some(name),
            ContainerAttribute::Named { name, attr_type } => named_attrs.push((name, attr_type)),
        }
    }

    (classes, id, named_attrs)
}

enum BuilderItem {
    Container(TokenStream),
    Tokens(TokenStream),
}

struct Builder {
    output_ident: Ident,
    items: Vec<BuilderItem>,
}

impl Builder {
    const fn new(output_ident: Ident) -> Self {
        Self {
            output_ident,
            items: Vec::new(),
        }
    }

    fn push_container(&mut self, container: TokenStream) {
        self.items.push(BuilderItem::Container(container));
    }

    fn push_tokens(&mut self, tokens: TokenStream) {
        self.items.push(BuilderItem::Tokens(tokens));
    }

    fn finish(self) -> TokenStream {
        let output_ident = &self.output_ident;
        let mut result = TokenStream::new();

        for item in self.items {
            match item {
                BuilderItem::Container(container) => {
                    result.extend(quote! {
                        #output_ident.push(#container);
                    });
                }
                BuilderItem::Tokens(tokens) => {
                    result.extend(tokens);
                }
            }
        }

        result
    }
}

// Helper function to convert kebab-case to PascalCase
fn kebab_to_pascal_case(s: &str) -> String {
    // Handle special cases first
    match s {
        "space-between" => return "SpaceBetween".to_string(),
        "space-evenly" => return "SpaceEvenly".to_string(),
        "flex-start" => return "FlexStart".to_string(),
        "flex-end" => return "FlexEnd".to_string(),
        "line-through" => return "LineThrough".to_string(),
        _ => {}
    }

    // General case: split on hyphens and capitalize each word
    s.split('-')
        .map(|word| {
            let mut chars = word.chars();
            chars.next().map_or_else(String::new, |first| {
                first.to_uppercase().collect::<String>() + &chars.as_str().to_lowercase()
            })
        })
        .collect()
}
