use proc_macro2::{Ident, TokenStream};
use quote::{ToTokens, format_ident, quote};
use syn::{Expr, Local};

use crate::ast::*;

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
    fn new(output_ident: Ident) -> Generator {
        Generator { output_ident }
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
                    matches!(
                        *markup,
                        Markup::ControlFlow(ControlFlow {
                            kind: ControlFlowKind::Let(_),
                            ..
                        })
                    )
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
            Markup::Splice { expr, .. } => {
                // For spliced expressions, use RenderContainer trait to convert to containers
                let output_ident = &self.output_ident;
                build.push_tokens(quote! {
                    {
                        use hyperchad_template2::RenderContainer;
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
            Markup::ControlFlow(control_flow) => self.control_flow(control_flow, build)?,
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
            name: format_ident!("Div"),
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
        let route_assignment = self.extract_route_from_attributes(&filtered_named_attrs);
        if let Some(route) = route_assignment {
            attr_assignments.push(route);
        }

        // Extract action attributes
        let actions_assignment = self.extract_actions_from_attributes(&filtered_named_attrs);
        if let Some(actions) = actions_assignment {
            attr_assignments.push(actions);
        }

        // Extract data attributes
        let data_assignment = self.extract_data_attributes(&filtered_named_attrs);
        if let Some(data) = data_assignment {
            attr_assignments.push(data);
        }

        // Separate element-specific attributes from container-level attributes
        let (element_attrs, container_attrs) =
            self.separate_element_and_container_attributes(&element_name, filtered_named_attrs);

        // Generate the element type with element-specific attributes
        let element_type = self.element_name_to_type_with_attributes(&element_name, element_attrs);

        // Process container-level attributes (styling, layout, etc.)
        let processed_attrs = self.process_attributes(container_attrs)?;
        for assignment in processed_attrs {
            attr_assignments.push(assignment);
        }

        // Generate children
        let children = if let ElementBody::Block(block) = element.body {
            // Create a unique identifier for children to avoid borrowing conflicts
            let children_ident = format_ident!("__children_{}", self.output_ident);
            let child_generator = Generator::new(children_ident.clone());
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
        &self,
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
                "Input" => matches!(
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
                "Button" => matches!(name_str.as_str(), "type" | "disabled"),
                "Anchor" => matches!(name_str.as_str(), "href" | "target"),
                "Image" => matches!(
                    name_str.as_str(),
                    "src" | "alt" | "srcset" | "sizes" | "loading" | "fit"
                ),
                "Canvas" => matches!(name_str.as_str(), "width" | "height"),
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
        &self,
        name: &ElementName,
        element_attrs: Vec<(AttributeName, AttributeType)>,
    ) -> TokenStream {
        let name_str = name.name.to_string();

        match name_str.as_str() {
            "Input" => self.generate_input_element(element_attrs),
            "Button" => self.generate_button_element(element_attrs),
            "Anchor" => self.generate_anchor_element(element_attrs),
            "Image" => self.generate_image_element(element_attrs),
            _ => self.element_name_to_type(name), // Fallback to simple element generation
        }
    }

    fn generate_input_element(
        &self,
        element_attrs: Vec<(AttributeName, AttributeType)>,
    ) -> TokenStream {
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
                        input_type = Some(Self::markup_to_string_tokens(attr_value));
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

        let name_field = name.unwrap_or_else(|| quote! { None });
        let value_field = value.unwrap_or_else(|| quote! { None });
        let placeholder_field = placeholder.unwrap_or_else(|| quote! { None });
        let checked_field = checked.unwrap_or_else(|| quote! { false });

        // Generate runtime matching for input type
        let input_variant = if let Some(input_type_tokens) = input_type {
            quote! {
                {
                    let input_type = #input_type_tokens;
                    match input_type.as_str() {
                        "checkbox" => hyperchad_transformer::Input::Checkbox {
                            checked: Some(#checked_field)
                        },
                        "password" => hyperchad_transformer::Input::Password {
                            value: #value_field,
                            placeholder: #placeholder_field
                        },
                        "hidden" => hyperchad_transformer::Input::Hidden {
                            value: #value_field
                        },
                        _ => hyperchad_transformer::Input::Text {
                            value: #value_field,
                            placeholder: #placeholder_field
                        },
                    }
                }
            }
        } else {
            // Default to text input if no type specified
            quote! {
                hyperchad_transformer::Input::Text {
                    value: #value_field,
                    placeholder: #placeholder_field
                }
            }
        };

        quote! {
            hyperchad_transformer::Element::Input {
                input: #input_variant,
                name: #name_field
            }
        }
    }

    fn generate_button_element(
        &self,
        element_attrs: Vec<(AttributeName, AttributeType)>,
    ) -> TokenStream {
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

        let type_field = button_type
            .map(|t| quote! { Some(#t) })
            .unwrap_or_else(|| quote! { None });

        quote! {
            hyperchad_transformer::Element::Button {
                r#type: #type_field
            }
        }
    }

    fn generate_anchor_element(
        &self,
        element_attrs: Vec<(AttributeName, AttributeType)>,
    ) -> TokenStream {
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
                        target = Some(self.markup_to_link_target_tokens(attr_value));
                    }
                    _ => {}
                }
            }
        }

        let href_field = href
            .map(|h| quote! { Some(#h) })
            .unwrap_or_else(|| quote! { None });
        let target_field = target
            .map(|t| quote! { Some(#t) })
            .unwrap_or_else(|| quote! { None });

        quote! {
            hyperchad_transformer::Element::Anchor {
                href: #href_field,
                target: #target_field
            }
        }
    }

    fn generate_image_element(
        &self,
        element_attrs: Vec<(AttributeName, AttributeType)>,
    ) -> TokenStream {
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
                        loading = Some(self.markup_to_image_loading_tokens(attr_value));
                    }
                    "fit" => {
                        fit = Some(Self::markup_to_image_fit_tokens(attr_value));
                    }
                    _ => {}
                }
            }
        }

        let src_field = src
            .map(|s| quote! { Some(#s) })
            .unwrap_or_else(|| quote! { None });
        let alt_field = alt
            .map(|a| quote! { Some(#a) })
            .unwrap_or_else(|| quote! { None });
        let srcset_field = srcset
            .map(|s| quote! { Some(#s) })
            .unwrap_or_else(|| quote! { None });
        let sizes_field = sizes
            .map(|s| quote! { Some(#s) })
            .unwrap_or_else(|| quote! { None });
        let loading_field = loading
            .map(|l| quote! { Some(#l) })
            .unwrap_or_else(|| quote! { None });
        let fit_field = fit
            .map(|f| quote! { Some(#f) })
            .unwrap_or_else(|| quote! { None });

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

    fn markup_to_link_target_tokens(&self, value: Markup<NoElement>) -> TokenStream {
        match value {
            Markup::Lit(lit) => match &lit.lit {
                syn::Lit::Str(lit_str) => {
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
                }
                _ => {
                    let lit = &lit.lit;
                    quote! { hyperchad_transformer_models::LinkTarget::Custom((#lit).to_string()) }
                }
            },
            Markup::Splice { expr, .. } => {
                quote! { (#expr).into() }
            }
            _ => quote! { hyperchad_transformer_models::LinkTarget::default() },
        }
    }

    fn markup_to_image_loading_tokens(&self, value: Markup<NoElement>) -> TokenStream {
        match value {
            Markup::Lit(lit) => match &lit.lit {
                syn::Lit::Str(lit_str) => {
                    let value_str = lit_str.value();
                    match value_str.as_str() {
                        "eager" => quote! { hyperchad_transformer_models::ImageLoading::Eager },
                        "lazy" => quote! { hyperchad_transformer_models::ImageLoading::Lazy },
                        _ => quote! { hyperchad_transformer_models::ImageLoading::default() },
                    }
                }
                _ => {
                    let lit = &lit.lit;
                    quote! { (#lit).into() }
                }
            },
            Markup::Splice { expr, .. } => {
                quote! { (#expr).into() }
            }
            _ => quote! { hyperchad_transformer_models::ImageLoading::default() },
        }
    }

    fn markup_to_image_fit_tokens(value: Markup<NoElement>) -> TokenStream {
        match value {
            Markup::Lit(lit) => match &lit.lit {
                syn::Lit::Str(lit_str) => {
                    let value_str = lit_str.value();
                    match value_str.as_str() {
                        "default" => quote! { hyperchad_transformer_models::ImageFit::Default },
                        "contain" => quote! { hyperchad_transformer_models::ImageFit::Contain },
                        "cover" => quote! { hyperchad_transformer_models::ImageFit::Cover },
                        "fill" => quote! { hyperchad_transformer_models::ImageFit::Fill },
                        "none" => quote! { hyperchad_transformer_models::ImageFit::None },
                        _ => quote! { hyperchad_transformer_models::ImageFit::default() },
                    }
                }
                _ => {
                    let lit = &lit.lit;
                    quote! { (#lit).into() }
                }
            },
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
        &self,
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
            let trigger_field = if let Some(trigger) = trigger {
                quote! { trigger: Some(#trigger) }
            } else {
                quote! { trigger: None }
            };
            let swap_field = if let Some(swap) = swap {
                quote! { swap: #swap }
            } else {
                quote! { swap: hyperchad_transformer_models::SwapTarget::default() }
            };

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
        &self,
        named_attrs: &[(AttributeName, AttributeType)],
    ) -> Option<TokenStream> {
        let mut actions = Vec::new();

        // Find fx- attributes
        for (name, attr_type) in named_attrs {
            let name_str = name.to_string();
            if let Some(trigger_name) = name_str.strip_prefix("fx-") {
                if let AttributeType::Normal { value, .. } = attr_type {
                    let trigger_ident = self.action_trigger_name_to_ident(trigger_name);
                    let action_effect = Self::markup_to_action_effect_tokens(value.clone());

                    actions.push(quote! {
                        hyperchad_actions::Action {
                            trigger: #trigger_ident,
                            action: #action_effect,
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
        &self,
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

    fn action_trigger_name_to_ident(&self, trigger_name: &str) -> TokenStream {
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
        match value {
            Markup::Lit(lit) => {
                // Handle literal action effects - this might be a string representation
                match &lit.lit {
                    syn::Lit::Str(lit_str) => {
                        let value_str = lit_str.value();
                        quote! {
                            hyperchad_actions::ActionType::Custom {
                                action: #value_str.to_string()
                            }.into()
                        }
                    }
                    _ => {
                        let lit = &lit.lit;
                        quote! { hyperchad_template2::IntoActionEffect::into_action_effect(#lit) }
                    }
                }
            }
            Markup::Splice { expr, .. } => {
                // For expressions, handle them directly using our helper trait
                quote! { hyperchad_template2::IntoActionEffect::into_action_effect(#expr) }
            }
            Markup::BraceSplice { items, .. } => {
                // For brace-wrapped items, handle like single item if only one
                if items.len() == 1 {
                    Self::markup_to_action_effect_tokens(items[0].clone())
                } else {
                    let expr = Self::handle_brace_splice_expression(&items);
                    quote! {
                        hyperchad_template2::IntoActionEffect::into_action_effect({ #expr })
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
            Markup::Lit(lit) => match &lit.lit {
                syn::Lit::Str(lit_str) => {
                    let value_str = lit_str.value();
                    quote! { #value_str.to_string() }
                }
                _ => {
                    let lit = &lit.lit;
                    quote! { #lit.to_string() }
                }
            },
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
            Markup::Lit(lit) => match &lit.lit {
                syn::Lit::Str(lit_str) => {
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
                }
                _ => {
                    let lit = &lit.lit;
                    quote! { (#lit).into() }
                }
            },
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
        &self,
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
        self.handle_shorthand_properties(&shorthand_attrs, &mut field_assignments);

        // Handle individual properties (higher precedence - these override shorthand)
        for (name, attr_type) in individual_attrs {
            if let Some(assignment) = self.attr_to_assignment(name.clone(), attr_type) {
                // Extract field name from the assignment and store it
                let field_name = self.extract_field_name_from_assignment(&assignment);
                field_assignments.insert(field_name, assignment);
            } else {
                let name_str = name.to_string();
                let error_msg = format!(
                    "Unknown attribute '{}'. Supported attributes include: class, width, height, padding, padding-x, padding-y, padding-left, padding-right, padding-top, padding-bottom, margin, margin-x, margin-y, margin-left, margin-right, margin-top, margin-bottom, border, border-x, border-y, border-top, border-right, border-bottom, border-left, background, color, align-items, justify-content, text-align, text-decoration, direction, position, cursor, visibility, overflow-x, overflow-y, font-family, font-size, opacity, border-radius, gap, hidden, debug, flex, flex-grow, flex-shrink, flex-basis, HTMX attributes (hx-get, hx-post, hx-put, hx-delete, hx-patch, hx-trigger, hx-swap), and action attributes (fx-click, fx-click-outside, fx-resize, fx-immediate, fx-hover, fx-change, fx-mousedown, and any other fx-* event)",
                    name_str
                );
                return Err(error_msg);
            }
        }

        // Convert the final field assignments to a Vec
        Ok(field_assignments.into_values().collect())
    }

    fn handle_shorthand_properties(
        &self,
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

    fn element_name_to_type(&self, name: &ElementName) -> TokenStream {
        let name_str = name.name.to_string();
        match name_str.as_str() {
            "Div" => quote! { hyperchad_transformer::Element::Div },
            "Section" => quote! { hyperchad_transformer::Element::Section },
            "Aside" => quote! { hyperchad_transformer::Element::Aside },
            "Main" => quote! { hyperchad_transformer::Element::Main },
            "Header" => quote! { hyperchad_transformer::Element::Header },
            "Footer" => quote! { hyperchad_transformer::Element::Footer },
            "Form" => quote! { hyperchad_transformer::Element::Form },
            "Span" => quote! { hyperchad_transformer::Element::Span },
            "Button" => quote! { hyperchad_transformer::Element::Button { r#type: None } },
            "Anchor" => {
                quote! { hyperchad_transformer::Element::Anchor { target: None, href: None } }
            }
            "Image" => quote! { hyperchad_transformer::Element::Image {
                source: None,
                alt: None,
                fit: None,
                source_set: None,
                sizes: None,
                loading: None
            } },
            "Input" => quote! { hyperchad_transformer::Element::Input {
                input: hyperchad_transformer::Input::Text { value: None, placeholder: None },
                name: None
            } },
            "H1" => {
                quote! { hyperchad_transformer::Element::Heading { size: hyperchad_transformer::HeaderSize::H1 } }
            }
            "H2" => {
                quote! { hyperchad_transformer::Element::Heading { size: hyperchad_transformer::HeaderSize::H2 } }
            }
            "H3" => {
                quote! { hyperchad_transformer::Element::Heading { size: hyperchad_transformer::HeaderSize::H3 } }
            }
            "H4" => {
                quote! { hyperchad_transformer::Element::Heading { size: hyperchad_transformer::HeaderSize::H4 } }
            }
            "H5" => {
                quote! { hyperchad_transformer::Element::Heading { size: hyperchad_transformer::HeaderSize::H5 } }
            }
            "H6" => {
                quote! { hyperchad_transformer::Element::Heading { size: hyperchad_transformer::HeaderSize::H6 } }
            }
            "UnorderedList" | "Ul" => quote! { hyperchad_transformer::Element::UnorderedList },
            "OrderedList" | "Ol" => quote! { hyperchad_transformer::Element::OrderedList },
            "ListItem" | "Li" => quote! { hyperchad_transformer::Element::ListItem },
            "Table" => quote! { hyperchad_transformer::Element::Table },
            "THead" => quote! { hyperchad_transformer::Element::THead },
            "TH" => quote! { hyperchad_transformer::Element::TH },
            "TBody" => quote! { hyperchad_transformer::Element::TBody },
            "TR" => quote! { hyperchad_transformer::Element::TR },
            "TD" => quote! { hyperchad_transformer::Element::TD },
            "Canvas" => quote! { hyperchad_transformer::Element::Canvas },
            _ => {
                let error_msg = format!(
                    "Unknown element type '{name_str}'. Supported elements are: Div, Section, Aside, Main, Header, Footer, Form, Span, Button, Anchor, Image, Input, H1, H2, H3, H4, H5, H6, UnorderedList (Ul), OrderedList (Ol), ListItem (Li), Table, THead, TH, TBody, TR, TD, Canvas",
                );
                quote! { compile_error!(#error_msg) }
            }
        }
    }

    fn attr_to_assignment(
        &self,
        name: AttributeName,
        attr_type: AttributeType,
    ) -> Option<TokenStream> {
        let name_str = name.to_string();

        match attr_type {
            AttributeType::Normal { value, .. } => match name_str.as_str() {
                // Number properties
                "width" => Some(self.number_attr("width", value)),
                "height" => Some(self.number_attr("height", value)),
                "min-width" => Some(self.number_attr("min_width", value)),
                "max-width" => Some(self.number_attr("max_width", value)),
                "min-height" => Some(self.number_attr("min_height", value)),
                "max-height" => Some(self.number_attr("max_height", value)),
                "padding-left" => Some(self.number_attr("padding_left", value)),
                "padding-right" => Some(self.number_attr("padding_right", value)),
                "padding-top" => Some(self.number_attr("padding_top", value)),
                "padding-bottom" => Some(self.number_attr("padding_bottom", value)),
                "margin-left" => Some(self.number_attr("margin_left", value)),
                "margin-right" => Some(self.number_attr("margin_right", value)),
                "margin-top" => Some(self.number_attr("margin_top", value)),
                "margin-bottom" => Some(self.number_attr("margin_bottom", value)),
                "font-size" => Some(self.number_attr("font_size", value)),
                "opacity" => Some(self.number_attr("opacity", value)),
                "left" => Some(self.number_attr("left", value)),
                "right" => Some(self.number_attr("right", value)),
                "top" => Some(self.number_attr("top", value)),
                "bottom" => Some(self.number_attr("bottom", value)),
                "translate-x" => Some(self.number_attr("translate_x", value)),
                "translate-y" => Some(self.number_attr("translate_y", value)),
                "column-gap" | "col-gap" => Some(self.number_attr("column_gap", value)),
                "row-gap" => Some(self.number_attr("row_gap", value)),
                "grid-cell-size" => Some(self.number_attr("grid_cell_size", value)),
                "border-top-left-radius" => Some(self.number_attr("border_top_left_radius", value)),
                "border-top-right-radius" => {
                    Some(self.number_attr("border_top_right_radius", value))
                }
                "border-bottom-left-radius" => {
                    Some(self.number_attr("border_bottom_left_radius", value))
                }
                "border-bottom-right-radius" => {
                    Some(self.number_attr("border_bottom_right_radius", value))
                }

                // Border properties
                "border-top" => Some(self.border_attr("border_top", value)),
                "border-right" => Some(self.border_attr("border_right", value)),
                "border-bottom" => Some(self.border_attr("border_bottom", value)),
                "border-left" => Some(self.border_attr("border_left", value)),

                // Enum properties
                "align-items" => Some(self.enum_attr("align_items", "AlignItems", value)),
                "justify-content" => {
                    Some(self.enum_attr("justify_content", "JustifyContent", value))
                }
                "text-align" => Some(self.enum_attr("text_align", "TextAlign", value)),
                "text-decoration" => Some(self.text_decoration_attr("text_decoration", value)),
                "direction" => Some(self.direct_enum_attr("direction", "LayoutDirection", value)),
                "position" => Some(self.enum_attr("position", "Position", value)),
                "cursor" => Some(self.enum_attr("cursor", "Cursor", value)),
                "visibility" => Some(self.enum_attr("visibility", "Visibility", value)),
                "overflow-x" => Some(self.direct_enum_attr("overflow_x", "LayoutOverflow", value)),
                "overflow-y" => Some(self.direct_enum_attr("overflow_y", "LayoutOverflow", value)),

                // Color properties
                "background" => Some(self.color_attr("background", value)),
                "color" => Some(self.color_attr("color", value)),

                // Boolean properties
                "hidden" => Some(self.bool_attr("hidden", value)),
                "debug" => Some(self.bool_attr("debug", value)),

                // String properties
                "font-family" => Some(self.string_vec_attr_opt("font_family", value)),
                "class" => Some(self.string_vec_attr("classes", value)),

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
                    // Skip input-specific attributes - these are handled by generate_input_element
                    "placeholder" | "value" | "name" | "type" | "checked" => None,

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

                    // Color properties
                    "background" | "color" => {
                        let field_ident = format_ident!("{}", name_str);
                        Some(quote! {
                            #field_ident: if let Some(val) = (#cond) { Some(val.into()) } else { None }
                        })
                    }

                    // Boolean properties - generate Option<bool>
                    "hidden" | "debug" => {
                        let field_ident = format_ident!("{}", name_str);
                        Some(quote! {
                            #field_ident: if let Some(val) = (#cond) { Some(val.into()) } else { None }
                        })
                    }

                    // Border properties - generate Option<(Color, Number)>
                    "border-top" | "border-right" | "border-bottom" | "border-left" => {
                        let field_ident = format_ident!("{}", name_str.replace('-', "_"));
                        Some(quote! {
                            #field_ident: if let Some(val) = (#cond) {
                                Some(val.into())
                            } else { None }
                        })
                    }
                    "font-family" => {
                        let field_ident = format_ident!("{}", name_str.replace('-', "_"));
                        Some(quote! {
                            #field_ident: if let Some(val) = (#cond) { Some(val.into()) } else { None }
                        })
                    }
                    "class" => {
                        let field_ident = format_ident!("{}", name_str.replace('-', "_"));
                        Some(quote! {
                            #field_ident: if let Some(val) = (#cond) { Some(val.into()) } else { None }
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

    fn number_attr(&self, field: &str, value: Markup<NoElement>) -> TokenStream {
        let field_ident = format_ident!("{}", field);
        let value_tokens = Self::markup_to_number_tokens(value);
        quote! { #field_ident: Some(#value_tokens) }
    }

    fn enum_attr(&self, field: &str, enum_name: &str, value: Markup<NoElement>) -> TokenStream {
        let field_ident = format_ident!("{}", field);
        let value_tokens = Self::markup_to_enum_tokens(enum_name, value);
        quote! { #field_ident: Some(#value_tokens) }
    }

    fn direct_enum_attr(
        &self,
        field: &str,
        enum_name: &str,
        value: Markup<NoElement>,
    ) -> TokenStream {
        let field_ident = format_ident!("{}", field);
        let value_tokens = Self::markup_to_enum_tokens(enum_name, value);
        quote! { #field_ident: #value_tokens }
    }

    fn color_attr(&self, field: &str, value: Markup<NoElement>) -> TokenStream {
        let field_ident = format_ident!("{}", field);
        let value_tokens = Self::markup_to_color_tokens(value);
        quote! { #field_ident: Some(#value_tokens) }
    }

    fn bool_attr(&self, field: &str, value: Markup<NoElement>) -> TokenStream {
        let field_ident = format_ident!("{}", field);
        let value_tokens = Self::markup_to_bool_tokens(value);
        quote! { #field_ident: Some(#value_tokens) }
    }

    fn string_vec_attr(&self, field: &str, value: Markup<NoElement>) -> TokenStream {
        let field_ident = format_ident!("{}", field);
        let value_tokens = Self::markup_to_string_vec_tokens(value);
        quote! { #field_ident: #value_tokens }
    }

    fn string_vec_attr_opt(&self, field: &str, value: Markup<NoElement>) -> TokenStream {
        let field_ident = format_ident!("{}", field);
        let value_tokens = Self::markup_to_string_vec_tokens(value);
        quote! { #field_ident: Some(#value_tokens) }
    }

    fn border_attr(&self, field: &str, value: Markup<NoElement>) -> TokenStream {
        let field_ident = format_ident!("{}", field);
        let border_tokens = Self::markup_to_border_tokens(value);
        quote! { #field_ident: Some(#border_tokens) }
    }

    fn text_decoration_attr(&self, field: &str, value: Markup<NoElement>) -> TokenStream {
        let field_ident = format_ident!("{}", field);
        let text_decoration_tokens = Self::markup_to_text_decoration_tokens(value);
        quote! { #field_ident: Some(#text_decoration_tokens) }
    }

    fn markup_to_number_tokens(value: Markup<NoElement>) -> TokenStream {
        match value {
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

    fn handle_potential_if_expression_for_number(expr: &syn::Expr) -> TokenStream {
        quote! {
            {
                let val = #expr;
                <hyperchad_transformer::Number as std::convert::From<_>>::from(val)
            }
        }
    }

    fn markup_to_enum_tokens(enum_name: &str, value: Markup<NoElement>) -> TokenStream {
        match value {
            Markup::Lit(lit) => {
                match &lit.lit {
                    syn::Lit::Str(lit_str) => {
                        let value_str = lit_str.value();
                        let enum_ident = format_ident!("{}", enum_name);

                        // Convert kebab-case to PascalCase for enum variants
                        let variant_name = kebab_to_pascal_case(&value_str);
                        let variant_ident = format_ident!("{}", variant_name);

                        quote! { hyperchad_transformer_models::#enum_ident::#variant_ident }
                    }
                    _ => {
                        // For non-string literals, use the literal directly as an expression
                        let lit = &lit.lit;
                        quote! { (#lit).into() }
                    }
                }
            }
            Markup::Splice { expr, .. } => {
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

    fn markup_to_color_tokens(value: Markup<NoElement>) -> TokenStream {
        match value {
            Markup::Lit(lit) => {
                match &lit.lit {
                    syn::Lit::Str(lit_str) => {
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
                            // Try to parse as hex if it starts with # or looks like hex
                            _ if value_str.starts_with('#')
                                || value_str.chars().all(|c| c.is_ascii_hexdigit()) =>
                            {
                                quote! { hyperchad_color::Color::from_hex(#value_str) }
                            }
                            // Default fallback
                            _ => quote! { hyperchad_color::Color::BLACK },
                        }
                    }
                    _ => {
                        // For non-string literals, use the literal directly as an expression
                        let lit = &lit.lit;
                        quote! { (#lit).into() }
                    }
                }
            }
            Markup::Splice { expr, .. } => Self::handle_potential_if_expression_for_color(&expr),
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
                            use hyperchad_template2::ToBool;
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
                use hyperchad_template2::ToBool;
                let val = #expr;
                val.to_bool()
            }
        }
    }

    /// Helper function to handle BraceSplice by reconstructing the expression as a cohesive unit
    /// This properly supports if_responsive() and other complex logic patterns
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
            ControlFlowKind::If(if_) => self.control_flow_if(if_, build)?,
            ControlFlowKind::Let(let_) => self.control_flow_let(let_, build)?,
            ControlFlowKind::For(for_) => self.control_flow_for(for_, build)?,
            ControlFlowKind::While(while_) => self.control_flow_while(while_, build)?,
            ControlFlowKind::Match(match_) => self.control_flow_match(match_, build)?,
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

    fn control_flow_let(&self, let_: Local, build: &mut Builder) -> Result<(), String> {
        build.push_tokens(quote!(#let_;));
        Ok(())
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
                match &lit.lit {
                    syn::Lit::Str(lit_str) => {
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
                    }
                    _ => {
                        // For non-string literals, assume it's a border tuple expression
                        let lit = &lit.lit;
                        quote! {
                            {
                                use hyperchad_template2::IntoBorder;
                                (#lit).into_border()
                            }
                        }
                    }
                }
            }
            Markup::Splice { expr, .. } => {
                // For expressions, use the IntoBorder trait for flexible conversion
                quote! {
                    {
                        use hyperchad_template2::IntoBorder;
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
                            use hyperchad_template2::IntoBorder;
                            (#first_tokens, #second_tokens).into_border()
                        }
                    }
                } else {
                    // Complex expression - let the IntoBorder trait handle it
                    let expr = Self::handle_brace_splice_expression(&items);
                    quote! {
                        {
                            use hyperchad_template2::IntoBorder;
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
            "gray" | "grey" => quote! { hyperchad_color::Color::from_hex("#808080") },
            "yellow" => quote! { hyperchad_color::Color::from_hex("#FFFF00") },
            "cyan" => quote! { hyperchad_color::Color::from_hex("#00FFFF") },
            "magenta" => quote! { hyperchad_color::Color::from_hex("#FF00FF") },
            "orange" => quote! { hyperchad_color::Color::from_hex("#FFA500") },
            "purple" => quote! { hyperchad_color::Color::from_hex("#800080") },
            "pink" => quote! { hyperchad_color::Color::from_hex("#FFC0CB") },
            "brown" => quote! { hyperchad_color::Color::from_hex("#A52A2A") },
            _ if color_str.starts_with('#') || color_str.chars().all(|c| c.is_ascii_hexdigit()) => {
                quote! { hyperchad_color::Color::from_hex(#color_str) }
            }
            _ => quote! { hyperchad_color::Color::from_hex(#color_str) },
        }
    }

    fn extract_field_name_from_assignment(&self, assignment: &TokenStream) -> String {
        // Extract the field name from assignments like "field_name: Some(value)"
        let assignment_str = assignment.to_string();
        if let Some(colon_pos) = assignment_str.find(':') {
            assignment_str[..colon_pos].trim().to_string()
        } else {
            // Fallback - try to extract identifier from start
            assignment_str
                .split_whitespace()
                .next()
                .unwrap_or("unknown")
                .to_string()
        }
    }

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
                match &lit.lit {
                    syn::Lit::Str(lit_str) => {
                        let value_str = lit_str.value();
                        // Parse comma-separated font families, matching html.rs implementation
                        let families: Vec<String> = value_str
                            .split(',')
                            .map(str::trim)
                            .filter(|x| !x.is_empty())
                            .map(ToString::to_string)
                            .collect();

                        quote! { vec![#(#families.to_string()),*] }
                    }
                    _ => {
                        // For non-string literals, assume it's already a Vec<String> or can be converted
                        let lit = &lit.lit;
                        quote! { (#lit).into() }
                    }
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
                match &lit.lit {
                    syn::Lit::Str(lit_str) => {
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
                    }
                    _ => {
                        let lit = &lit.lit;
                        quote! { (#lit).into() }
                    }
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

    // fn markup_to_flex_tokens(value: Markup<NoElement>) -> TokenStream {
    //     match value {
    //         Markup::Lit(lit) => {
    //             match &lit.lit {
    //                 syn::Lit::Str(lit_str) => {
    //                     let value_str = lit_str.value();
    //                     // Simple flex parsing - just check for common values
    //                     if value_str.contains("underline") {
    //                         quote! { hyperchad_transformer::TextDecoration {
    //                             color: None,
    //                             line: vec![hyperchad_transformer_models::TextDecorationLine::Underline],
    //                             style: None,
    //                             thickness: None,
    //                         } }
    //                     } else if value_str.contains("none") {
    //                         quote! { hyperchad_transformer::TextDecoration {
    //                             color: None,
    //                             line: vec![hyperchad_transformer_models::TextDecorationLine::None],
    //                             style: None,
    //                             thickness: None,
    //                         } }
    //                     } else {
    //                         quote! { hyperchad_transformer::TextDecoration::default() }
    //                     }
    //                 }
    //                 _ => {
    //                     let lit = &lit.lit;
    //                     quote! { (#lit).into() }
    //                 }
    //             }
    //         }
    //         Markup::Splice { expr, .. } => {
    //             quote! { (#expr) }
    //         }
    //         Markup::BraceSplice { items, .. } => {
    //             // For brace-wrapped items, handle like single item if only one
    //             if items.len() == 1 {
    //                 Self::markup_to_text_decoration_tokens(items[0].clone())
    //             } else {
    //                 let expr = Self::handle_brace_splice_expression(&items);
    //                 quote! {
    //                     {
    //                         let result = { #expr };
    //                         result.into()
    //                     }
    //                 }
    //             }
    //         }
    //         _ => {
    //             quote! { hyperchad_transformer::TextDecoration::default() }
    //         }
    //     }
    // }

    // fn flex_attr(&self, field: &str, value: Markup<NoElement>) -> TokenStream {
    //     let field_ident = format_ident!("{}", field);
    //     let value_tokens = Self::markup_to_flex_tokens(value);
    //     quote! { #field_ident: Some(#value_tokens) }
    // }
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
                classes.push((name, toggler.map(|toggler| toggler.cond)))
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
    fn new(output_ident: Ident) -> Builder {
        Builder {
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
    s.split('-')
        .map(|word| {
            let mut chars = word.chars();
            match chars.next() {
                None => String::new(),
                Some(first) => {
                    first.to_uppercase().collect::<String>() + &chars.as_str().to_lowercase()
                }
            }
        })
        .collect()
}
