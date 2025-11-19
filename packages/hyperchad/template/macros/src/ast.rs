//! Abstract syntax tree (AST) types for the `HyperChad` template macro.
//!
//! This module defines the AST structures used to parse and represent templates
//! in the `container!` macro. The AST supports elements, attributes, control flow, literals,
//! and dynamic expressions that are eventually transformed into `Vec<Container>` structures.

use std::fmt::{self, Display, Formatter};

use proc_macro2::TokenStream;
use proc_macro2_diagnostics::{Diagnostic, SpanDiagnosticExt};
use quote::ToTokens;
use syn::{
    Error, Expr, Ident, Lit, LitFloat, LitInt, LitStr, Local, Pat, Stmt, braced, bracketed,
    ext::IdentExt,
    parenthesized,
    parse::{Lookahead1, Parse, ParseStream, discouraged::Speculative},
    parse_quote,
    punctuated::{Pair, Punctuated},
    spanned::Spanned,
    token::{
        At, Brace, Bracket, Colon, Comma, Dot, Else, Eq, FatArrow, For, If, In, Let, Match, Minus,
        Paren, Pound, Question, Semi, Slash, While,
    },
};

/// A collection of markup nodes.
///
/// Represents the top-level structure of a template, containing zero or more
/// [`Markup`] nodes that will be rendered to various output formats.
#[derive(Debug, Clone)]
pub struct Markups<E> {
    /// The individual markup nodes in this collection.
    pub markups: Vec<Markup<E>>,
}

impl<E: MaybeElement> DiagnosticParse for Markups<E> {
    fn diagnostic_parse(
        input: ParseStream,
        diagnostics: &mut Vec<Diagnostic>,
    ) -> syn::Result<Self> {
        let mut markups = Vec::new();
        while !input.is_empty() {
            markups.push(Markup::diagnostic_parse_in_block(input, diagnostics)?);
        }
        Ok(Self { markups })
    }
}

impl<E: ToTokens> ToTokens for Markups<E> {
    fn to_tokens(&self, tokens: &mut TokenStream) {
        for markup in &self.markups {
            markup.to_tokens(tokens);
        }
    }
}

/// A single markup node in a template.
///
/// Represents different types of content that can appear in a template:
/// literals, expressions, elements, control flow, and more.
#[derive(Debug, Clone)]
pub enum Markup<E> {
    /// A block containing nested markup nodes.
    Block(Block<E>),
    /// A literal value (string, integer, or float).
    Lit(ContainerLit),
    /// A numeric literal with optional units (e.g., `100%`, `50vw`, `3.14`).
    NumericLit(NumericLit),
    /// A dynamic Rust expression wrapped in parentheses.
    Splice {
        /// The parentheses token.
        paren_token: Box<Paren>,
        /// The Rust expression to evaluate.
        expr: Box<Expr>,
    },
    /// A brace-wrapped expression or concatenation of literals and expressions.
    BraceSplice {
        /// The braces token.
        brace_token: Brace,
        /// The items to concatenate (literals and expressions).
        items: Vec<Markup<NoElement>>,
    },
    /// An element.
    Element(E),
    /// Control flow constructs (`@if`, `@for`, `@while`, `@match`, `@let`).
    ControlFlow(Box<ControlFlow<E>>),
    /// A semicolon (used for void elements).
    Semi(Semi),
}

impl<E: MaybeElement> Markup<E> {
    #[allow(clippy::too_many_lines, clippy::cognitive_complexity)]
    pub fn diagnostic_parse_in_block(
        input: ParseStream,
        diagnostics: &mut Vec<Diagnostic>,
    ) -> syn::Result<Self> {
        if input.peek(Let)
            || input.peek(If)
            || input.peek(Else)
            || input.peek(For)
            || input.peek(While)
            || input.peek(Match)
        {
            let kw = input.call(Ident::parse_any)?;
            diagnostics.push(
                kw.span()
                    .error(format!("found keyword `{kw}`"))
                    .help(format!("should this be `@{kw}`?")),
            );
        }

        let lookahead = input.lookahead1();

        if lookahead.peek(Brace) {
            // Check if this is a brace-wrapped expression {expr} or a regular block
            let fork = input.fork();
            let content;
            let _brace_token = braced!(content in fork);

            // Handle empty braces {} as blocks
            if content.is_empty() {
                return input.diagnostic_parse(diagnostics).map(Self::Block);
            }

            // First check if this looks like a block with control flow, multiple statements, or elements
            // by looking for @ symbols, semicolons, or elements
            let mut is_block = false;
            let temp_fork = content.fork();
            while !temp_fork.is_empty() {
                if temp_fork.peek(At) || temp_fork.peek(Semi) {
                    is_block = true;
                    break;
                }
                // Also check if this looks like an element
                let lookahead = temp_fork.lookahead1();
                if E::should_parse(&lookahead).is_some() {
                    is_block = true;
                    break;
                }
                // Try to advance past one token to continue checking
                if temp_fork.parse::<proc_macro2::TokenTree>().is_err() {
                    break;
                }
            }

            if is_block {
                // This is a regular block (contains control flow or multiple statements)
                return input.diagnostic_parse(diagnostics).map(Self::Block);
            }

            // Try to parse as a single expression first
            if let Ok(expr) = content.parse::<Expr>()
                && content.is_empty()
            {
                // Successfully parsed as a single expression
                // Check if it's a function call pattern like "literal"(expression)
                if let syn::Expr::Call(call) = &expr
                    && let syn::Expr::Lit(syn::ExprLit {
                        lit: syn::Lit::Str(_),
                        ..
                    }) = call.func.as_ref()
                {
                    // This is a "literal"(args...) pattern - break it down for concatenation
                    let content;
                    let brace_token = braced!(content in input);
                    let expr = content.parse::<Expr>()?;

                    if let syn::Expr::Call(call) = expr {
                        let mut items = vec![];

                        // Add the function (string literal) as a literal item
                        if let syn::Expr::Lit(lit_expr) = call.func.as_ref() {
                            items.push(Markup::Lit(ContainerLit {
                                lit: lit_expr.lit.clone(),
                            }));
                        }

                        // Add each argument as an expression item
                        for arg in call.args {
                            items.push(Markup::Splice {
                                paren_token: Box::new(Paren::default()),
                                expr: Box::new(arg),
                            });
                        }

                        return Ok(Self::BraceSplice { brace_token, items });
                    }
                }

                // Regular single expression - wrap as BraceSplice with one item
                let content;
                let brace_token = braced!(content in input);
                let expr = content.parse::<Expr>()?;

                return Ok(Self::BraceSplice {
                    brace_token,
                    items: vec![Markup::Splice {
                        paren_token: Box::new(Paren::default()),
                        expr: Box::new(expr),
                    }],
                });
            }

            // If single expression parsing failed, try to parse as a sequence of literals and expressions
            // This handles patterns like "literal1"(expr1)"literal2"(expr2)
            let content;
            let brace_token = braced!(content in input);
            let mut items = vec![];

            while !content.is_empty() {
                if content.peek(syn::Lit) {
                    // Parse a literal
                    let lit: syn::Lit = content.parse()?;
                    items.push(Markup::Lit(ContainerLit { lit }));
                } else if content.peek(Paren) {
                    // Parse a parenthesized expression
                    let inner_content;
                    let _paren_token = parenthesized!(inner_content in content);
                    let expr = inner_content.parse()?;
                    items.push(Markup::Splice {
                        paren_token: Box::new(Paren::default()),
                        expr: Box::new(expr),
                    });
                } else {
                    // If we can't parse as literal or parenthesized expression,
                    // this might be a complex block - fall back to block parsing
                    return input.diagnostic_parse(diagnostics).map(Self::Block);
                }
            }

            if !items.is_empty() {
                return Ok(Self::BraceSplice { brace_token, items });
            }

            // This is a regular block (multiple statements, control flow, etc.)
            input.diagnostic_parse(diagnostics).map(Self::Block)
        } else if lookahead.peek(Lit) {
            input.diagnostic_parse(diagnostics).map(Self::Lit)
        } else if lookahead.peek(Paren) {
            let content;
            let paren_token = parenthesized!(content in input);
            let expr: Expr = content.parse()?;

            // Check if the parenthesized expression is followed by %
            if input.peek(syn::Token![%]) {
                let _percent: syn::Token![%] = input.parse()?;
                // Create a splice expression that converts the expression to percentage
                let percent_expr: Expr =
                    parse_quote!(hyperchad_template::calc::to_percent_number(#expr));
                Ok(Self::Splice {
                    paren_token: Box::new(Paren::default()),
                    expr: Box::new(percent_expr),
                })
            } else {
                Ok(Self::Splice {
                    paren_token: Box::new(paren_token),
                    expr: Box::new(expr),
                })
            }
        } else if let Some(parse_element) = E::should_parse(&lookahead) {
            parse_element(input, diagnostics).map(Self::Element)
        } else if lookahead.peek(At) {
            input.diagnostic_parse(diagnostics).map(Self::ControlFlow)
        } else if lookahead.peek(Semi) {
            input.parse().map(Self::Semi)
        } else if lookahead.peek(syn::LitInt) || lookahead.peek(syn::LitFloat) {
            // Handle numeric literals that might be followed by unit tokens
            let mut result = String::new();

            // Parse the numeric part
            if lookahead.peek(syn::LitInt) {
                let lit_int: syn::LitInt = input.parse()?;
                result.push_str(&lit_int.to_string());
            } else {
                let lit_float: syn::LitFloat = input.parse()?;
                result.push_str(&lit_float.to_string());
            }

            // Now check what follows the number
            let lookahead2 = input.lookahead1();

            // Check for percentage token (%)
            if lookahead2.peek(syn::Token![%]) {
                let _percent: syn::Token![%] = input.parse()?;
                result.push('%');
                if let Some(numeric_lit) = NumericLit::try_parse(&result) {
                    return Ok(Self::NumericLit(numeric_lit));
                }
            }
            // Check for identifier units (vw, vh, dvw, dvh, etc.)
            else if lookahead2.peek(Ident) {
                let ident: syn::Ident = input.parse()?;
                let unit = ident.to_string();
                match unit.as_str() {
                    "vw" | "vh" | "dvw" | "dvh" | "px" | "em" | "rem" | "ch" | "ex" => {
                        result.push_str(&unit);
                        if let Some(numeric_lit) = NumericLit::try_parse(&result) {
                            return Ok(Self::NumericLit(numeric_lit));
                        }
                    }
                    _ => {
                        // This is not a unit we recognize, so we need to "put back" the identifier
                        // Since we already consumed it, we'll create a new parsing context
                        // For now, let's just treat as plain number and ignore the identifier
                        // The identifier will be parsed in the next iteration
                        if NumericLit::try_parse(&result).is_some() {
                            // We consumed an identifier that we shouldn't have
                            // This is a parsing challenge - for now, error out
                            return Err(Error::new(
                                ident.span(),
                                format!("Unexpected identifier '{unit}' after number"),
                            ));
                        }
                    }
                }
            }

            // No unit suffix, treat as plain number
            NumericLit::try_parse(&result).map_or_else(
                || {
                    // Fallback to regular literal
                    let lit = if result.contains('.') {
                        syn::Lit::Float(syn::LitFloat::new(&result, input.span()))
                    } else {
                        syn::Lit::Int(syn::LitInt::new(&result, input.span()))
                    };
                    Ok(Self::Lit(ContainerLit { lit }))
                },
                |numeric_lit| Ok(Self::NumericLit(numeric_lit)),
            )
        } else if lookahead.peek(syn::Token![#]) {
            // Parse hex colors like #fff, #123456, #1e293b
            //
            // IMPORTANT: Hex colors are challenging to parse because Rust's lexer interprets
            // certain patterns as scientific notation:
            // - #1e2 is tokenized as: # + LitFloat(1e2) where 1e2 = 100.0
            // - #1e293b is tokenized as: # + LitFloat(1e293, suffix="b")
            // - #12e CANNOT be tokenized (invalid: missing exponent digits)
            //
            // For the last case (#12e), users must use string syntax: color="#12e"
            //
            // This parser handles scientific notation by:
            // 1. Parsing LitFloat tokens and extracting their base digits
            // 2. Including hex-valid suffixes as part of the color (e.g., "b" in "1e293b")
            // 3. Combining multiple tokens when needed (though usually it's one token)
            let pound: syn::Token![#] = input.parse()?;

            let mut hex_str = String::new();
            let successful_fork = input.fork();
            let mut last_span = pound.span();

            loop {
                if hex_str.len() >= 8 {
                    break;
                }

                let mut found_token = false;

                // Try Ident - catches things like "abc", "def", "fff"
                if !found_token {
                    let fork_ident = successful_fork.fork();
                    if let Ok(ident) = fork_ident.call(Ident::parse_any) {
                        let ident_str = ident.to_string();
                        // STRICT: Token must be ENTIRELY hex-valid
                        if !ident_str.is_empty() && ident_str.chars().all(|c| c.is_ascii_hexdigit())
                        {
                            hex_str.push_str(&ident_str);
                            last_span = ident.span();
                            successful_fork.advance_to(&fork_ident);
                            found_token = true;
                        }
                    }
                }

                // Try LitInt for purely numeric tokens like "123", "000", hex literals like "1a2"
                if !found_token {
                    let fork_int = successful_fork.fork();
                    if let Ok(lit_int) = fork_int.parse::<LitInt>() {
                        let int_str = lit_int.to_string();
                        // STRICT: Token must be ENTIRELY hex-valid (no suffixes, no underscores in output)
                        if !int_str.is_empty() && int_str.chars().all(|c| c.is_ascii_hexdigit()) {
                            hex_str.push_str(&int_str);
                            last_span = lit_int.span();
                            successful_fork.advance_to(&fork_int);
                            found_token = true;
                        }
                    }
                }

                // Try LitFloat for scientific notation like "1e2" which gets tokenized as float
                // This handles patterns like #1e2, #3e8, #1e293b
                // Note: "1e293b" is tokenized as float 1e293 with suffix "b"
                if !found_token {
                    let fork_float = successful_fork.fork();
                    if let Ok(lit_float) = fork_float.parse::<LitFloat>() {
                        // For hex colors, the "suffix" might actually be part of the hex digits
                        // e.g., "1e293b" is tokenized as float "1e293" with suffix "b"
                        // We need to check if suffix is hex-valid and include it
                        let base_digits = lit_float.base10_digits();
                        let suffix = lit_float.suffix();

                        // Check if base digits are hex-valid
                        if !base_digits.is_empty()
                            && base_digits.chars().all(|c| c.is_ascii_hexdigit())
                        {
                            // If there's a suffix, check if it's also hex-valid
                            let suffix_hex_valid = suffix.chars().all(|c| c.is_ascii_hexdigit());

                            if suffix.is_empty() || suffix_hex_valid {
                                // Add base digits
                                hex_str.push_str(base_digits);
                                // Add suffix if it's hex-valid (e.g., "b", "ff", "a1")
                                if !suffix.is_empty() && suffix_hex_valid {
                                    hex_str.push_str(suffix);
                                }
                                last_span = lit_float.span();
                                successful_fork.advance_to(&fork_float);
                                found_token = true;
                            }
                        }
                    }
                }

                if !found_token {
                    break;
                }
            }

            if (hex_str.len() == 3 || hex_str.len() == 6 || hex_str.len() == 8)
                && hex_str.chars().all(|c| c.is_ascii_hexdigit())
            {
                input.advance_to(&successful_fork);
                let full_hex = format!("#{hex_str}");
                let expr: Expr = parse_quote!(#full_hex);
                return Ok(Self::Splice {
                    paren_token: Box::new(Paren::default()),
                    expr: Box::new(expr),
                });
            }

            let error_span = pound.span().join(last_span).unwrap_or_else(|| pound.span());

            if hex_str.is_empty() {
                diagnostics.push(
                    pound
                        .span()
                        .error("Expected hex digits after '#' for hex color")
                        .help("Note: Hex colors ending in 'e' without digits after (like #12e) must use string syntax: color=\"#12e\"")
                        .help("This is due to Rust treating 'e' as scientific notation (e.g., 1e2 = 100.0)"),
                );
                return Ok(Self::Splice {
                    paren_token: Box::new(Paren::default()),
                    expr: Box::new(parse_quote!("#000")),
                });
            }

            let mut error = error_span.error(format!(
                "Invalid hex color '#{hex_str}'. Hex colors must be 3, 6, or 8 hexadecimal digits (0-9, a-f)"
            ));

            // Detect if this might be a scientific notation issue (ends with 'e' but incomplete)
            if hex_str.ends_with('e') || hex_str.ends_with('E') {
                error = error.help("If you intended a hex color ending in 'e', use string syntax: color=\"#XXXe\"")
                    .help("Rust's lexer treats patterns like '12e' as incomplete scientific notation");
            }

            diagnostics.push(error);
            input.advance_to(&successful_fork);
            Ok(Self::Splice {
                paren_token: Box::new(Paren::default()),
                expr: Box::new(parse_quote!("#000")),
            })
        } else if lookahead.peek(Ident::peek_any) {
            // Handle bare identifiers (including kebab-case) as splice expressions for attribute values
            // This enables syntax like: visibility=hidden, align-items=center, justify-content=space-between
            // PLUS unit+number patterns like: vw50, px16, em1
            // PLUS variable with % suffix: value%

            // Check for function call patterns first (approach 1): vw(expr), vh(expr), etc.
            let fork = input.fork();
            if let Ok(ident) = fork.call(Ident::parse_any) {
                let ident_str = ident.to_string();
                if fork.peek(Paren) && NumericLit::is_unit_identifier(&ident_str) {
                    // This is a function-style unit call like vw(50), vh(expr), calc(expr), min(a,b), etc.
                    let unit_ident: Ident = input.call(Ident::parse_any)?;
                    let content;
                    let _paren_token = parenthesized!(content in input);

                    // Handle calc and CSS math functions specially - pass the expression directly
                    let call_expr: Expr = if matches!(ident_str.as_str(), "calc") {
                        // For calc, parse a single expression
                        let expr: Expr = content.parse()?;
                        parse_quote!(#unit_ident(#expr))
                    } else if matches!(ident_str.as_str(), "min" | "max" | "clamp") {
                        // For CSS math functions, parse comma-separated expressions
                        let args =
                            syn::punctuated::Punctuated::<Expr, syn::Token![,]>::parse_terminated(
                                &content,
                            )?;
                        let args_vec: Vec<Expr> = args.into_iter().collect();
                        parse_quote!(#unit_ident(#(#args_vec),*))
                    } else if matches!(ident_str.as_str(), "percent") {
                        // For percent helper function, use calc module
                        let expr: Expr = content.parse()?;
                        parse_quote!(hyperchad_template::calc::to_percent_number(#expr))
                    } else if ident_str == "rgb" {
                        // For rgb function, parse comma-separated expressions
                        let args =
                            syn::punctuated::Punctuated::<Expr, syn::Token![,]>::parse_terminated(
                                &content,
                            )?;
                        let args_vec: Vec<Expr> = args.into_iter().collect();

                        // Route to appropriate function based on argument count
                        match args_vec.len() {
                            3 => {
                                parse_quote!(hyperchad_template::color_functions::rgb(#(#args_vec),*))
                            }
                            4 => {
                                // For 4 arguments, use the rgb_alpha function
                                parse_quote!(hyperchad_template::color_functions::rgb_alpha(#(#args_vec),*))
                            }
                            _ => {
                                return Err(syn::Error::new(
                                    content.span(),
                                    format!(
                                        "rgb() function expects 3 or 4 arguments, got {}",
                                        args_vec.len()
                                    ),
                                ));
                            }
                        }
                    } else if ident_str == "rgba" {
                        // For rgba function, parse comma-separated expressions (legacy support)
                        let args =
                            syn::punctuated::Punctuated::<Expr, syn::Token![,]>::parse_terminated(
                                &content,
                            )?;
                        let args_vec: Vec<Expr> = args.into_iter().collect();

                        if args_vec.len() != 4 {
                            return Err(syn::Error::new(
                                content.span(),
                                format!(
                                    "rgba() function expects 4 arguments, got {}",
                                    args_vec.len()
                                ),
                            ));
                        }

                        parse_quote!(hyperchad_template::color_functions::rgba(#(#args_vec),*))
                    } else {
                        // For unit functions like vw, vh, etc., parse a single expression and use unit_functions module
                        let expr: Expr = content.parse()?;
                        parse_quote!(hyperchad_template::unit_functions::#unit_ident(#expr))
                    };

                    return Ok(Self::Splice {
                        paren_token: Box::new(Paren::default()),
                        expr: Box::new(call_expr),
                    });
                }
            }

            // Check for identifier followed by % first (approach 2): variable%
            let fork2 = input.fork();
            if let Ok(_ident) = fork2.call(Ident::parse_any)
                && fork2.peek(syn::Token![%])
            {
                // This is a variable followed by %, like: value%
                let var_ident: Ident = input.call(Ident::parse_any)?;
                let _percent: syn::Token![%] = input.parse()?;

                // Create a splice expression that converts the variable to percentage
                let expr: Expr =
                    parse_quote!(hyperchad_template::calc::to_percent_number(#var_ident));
                return Ok(Self::Splice {
                    paren_token: Box::new(Paren::default()),
                    expr: Box::new(expr),
                });
            }

            // Try to parse as AttributeName first (supports kebab-case like space-evenly)
            if let Ok(attr_name) = input.parse::<AttributeName>() {
                let name_str = attr_name.to_string();

                // Check for identifier-based unit patterns (approach 3): vw50, px16, em1
                if let Some(numeric_lit) = NumericLit::try_parse_identifier_unit(&name_str) {
                    return Ok(Self::NumericLit(numeric_lit));
                }

                // Note: boolean literals are handled by lookahead.peek(Lit) earlier in this function

                // Handle as regular string identifier for enums
                let expr: Expr = parse_quote!(#name_str);
                Ok(Self::Splice {
                    paren_token: Box::new(Paren::default()),
                    expr: Box::new(expr),
                })
            } else {
                // Fallback to simple identifier
                let ident: Ident = input.call(Ident::parse_any)?;
                let ident_str = ident.to_string();

                // Check for identifier-based unit patterns (approach 3): vw50, px16, em1
                if let Some(numeric_lit) = NumericLit::try_parse_identifier_unit(&ident_str) {
                    return Ok(Self::NumericLit(numeric_lit));
                }

                // Note: boolean literals are handled by lookahead.peek(Lit) earlier in this function

                // Handle as regular identifier
                let expr: Expr = parse_quote!(#ident);
                Ok(Self::Splice {
                    paren_token: Box::new(Paren::default()),
                    expr: Box::new(expr),
                })
            }
        } else {
            Err(lookahead.error())
        }
    }
}

impl<E: MaybeElement> DiagnosticParse for Markup<E> {
    fn diagnostic_parse(
        input: ParseStream,
        diagnostics: &mut Vec<Diagnostic>,
    ) -> syn::Result<Self> {
        let markup = Self::diagnostic_parse_in_block(input, diagnostics)?;

        if let Self::ControlFlow(flow) = &markup
            && let ControlFlowKind::Let(_) = flow.kind
        {
            diagnostics.push(
                markup
                    .span()
                    .error("`@let` bindings are only allowed inside blocks"),
            );
        }

        Ok(markup)
    }
}

impl<E: ToTokens> ToTokens for Markup<E> {
    fn to_tokens(&self, tokens: &mut TokenStream) {
        match self {
            Self::Block(block) => block.to_tokens(tokens),
            Self::Lit(lit) => lit.to_tokens(tokens),
            Self::NumericLit(numeric_lit) => numeric_lit.to_tokens(tokens),
            Self::Splice { paren_token, expr } => {
                paren_token.surround(tokens, |tokens| {
                    expr.to_tokens(tokens);
                });
            }
            Self::BraceSplice { brace_token, items } => {
                brace_token.surround(tokens, |tokens| {
                    for item in items {
                        item.to_tokens(tokens);
                    }
                });
            }
            Self::Element(element) => element.to_tokens(tokens),
            Self::ControlFlow(control_flow) => control_flow.to_tokens(tokens),
            Self::Semi(semi) => semi.to_tokens(tokens),
        }
    }
}

/// Represents a context that may or may not allow elements.
///
/// An attribute accepts almost the same syntax as an element body, except child elements aren't
/// allowed. To enable code reuse, introduce a trait that abstracts over whether an element is
/// allowed or not.
pub trait MaybeElement: Sized + ToTokens {
    /// If an element can be parsed here, returns `Some` with a parser for the rest of the element.
    fn should_parse(lookahead: &Lookahead1<'_>) -> Option<DiagnosticParseFn<Self>>;
}

/// An implementation of `DiagnosticParse::diagnostic_parse`.
pub type DiagnosticParseFn<T> = fn(ParseStream, &mut Vec<Diagnostic>) -> syn::Result<T>;

/// Represents an attribute context, where elements are disallowed.
#[derive(Debug, Clone)]
pub enum NoElement {}

impl MaybeElement for NoElement {
    fn should_parse(
        _lookahead: &Lookahead1<'_>,
    ) -> Option<fn(ParseStream, &mut Vec<Diagnostic>) -> syn::Result<Self>> {
        None
    }
}

impl ToTokens for NoElement {
    fn to_tokens(&self, _tokens: &mut TokenStream) {
        #[allow(clippy::uninhabited_references)]
        match *self {}
    }
}

/// An element with optional name, attributes, and body.
///
/// Represents elements like `div.container { ... }` or `button hx-post="/submit" { "Click" }`.
/// Anonymous containers (e.g., `.wrapper #main { ... }`) are supported and default to `div` elements.
#[derive(Debug, Clone)]
pub struct ContainerElement {
    /// The element's tag name (e.g., `div`, `button`), or `None` for anonymous containers
    /// (which default to `div` during code generation).
    pub name: Option<ElementName>,
    /// The element's attributes (classes, IDs, and named attributes).
    pub attrs: Vec<ContainerAttribute>,
    /// The element's body (void with `;` or block with `{ ... }`).
    pub body: ElementBody,
}

impl From<NoElement> for ContainerElement {
    fn from(value: NoElement) -> Self {
        match value {}
    }
}

impl MaybeElement for ContainerElement {
    fn should_parse(
        lookahead: &Lookahead1<'_>,
    ) -> Option<fn(ParseStream, &mut Vec<Diagnostic>) -> syn::Result<Self>> {
        if lookahead.peek(Ident::peek_any) || lookahead.peek(Dot) || lookahead.peek(Pound) {
            Some(Self::diagnostic_parse)
        } else {
            None
        }
    }
}

impl DiagnosticParse for ContainerElement {
    fn diagnostic_parse(
        input: ParseStream,
        diagnostics: &mut Vec<Diagnostic>,
    ) -> syn::Result<Self> {
        Ok(Self {
            name: if input.peek(Ident::peek_any) {
                Some(input.diagnostic_parse(diagnostics)?)
            } else {
                None
            },
            attrs: {
                let mut id_pushed = false;
                let mut attrs = Vec::new();

                while input.peek(Ident::peek_any)
                    || input.peek(Lit)
                    || input.peek(Dot)
                    || input.peek(Pound)
                {
                    let attr = input.diagnostic_parse(diagnostics)?;

                    if let ContainerAttribute::Id { .. } = attr {
                        if id_pushed {
                            return Err(Error::new_spanned(
                                attr,
                                "duplicate id (`#`) attribute specified",
                            ));
                        }
                        id_pushed = true;
                    }

                    attrs.push(attr);
                }

                if !(input.peek(Brace) || input.peek(Semi) || input.peek(Slash)) {
                    let lookahead = input.lookahead1();

                    lookahead.peek(Ident::peek_any);
                    lookahead.peek(Lit);
                    lookahead.peek(Dot);
                    lookahead.peek(Pound);

                    lookahead.peek(Brace);
                    lookahead.peek(Semi);

                    return Err(lookahead.error());
                }

                attrs
            },
            body: input.diagnostic_parse(diagnostics)?,
        })
    }
}

impl ToTokens for ContainerElement {
    fn to_tokens(&self, tokens: &mut TokenStream) {
        if let Some(name) = &self.name {
            name.to_tokens(tokens);
        }
        for attr in &self.attrs {
            attr.to_tokens(tokens);
        }
        self.body.to_tokens(tokens);
    }
}

/// Type alias for [`ContainerElement`] for compatibility.
pub type Element = ContainerElement;

/// The body of an element.
///
/// Elements can either be void (self-closing with `;`) or have a block body
/// containing child markup nodes.
#[derive(Debug, Clone)]
pub enum ElementBody {
    /// A void element terminated with `;` (e.g., `input type="text";`).
    Void(Semi),
    /// A block element containing child nodes (e.g., `div { "content" }`).
    Block(Block<ContainerElement>),
}

impl DiagnosticParse for ElementBody {
    fn diagnostic_parse(
        input: ParseStream,
        diagnostics: &mut Vec<Diagnostic>,
    ) -> syn::Result<Self> {
        let lookahead = input.lookahead1();

        if lookahead.peek(Semi) {
            input.parse().map(Self::Void)
        } else if lookahead.peek(Brace) {
            input.diagnostic_parse(diagnostics).map(Self::Block)
        } else if lookahead.peek(Slash) {
            diagnostics.push(
                input
                    .parse::<Slash>()?
                    .span()
                    .error("void elements must use `;`, not `/`")
                    .help("change this to `;`")
                    .help("see https://github.com/lambda-fairy/maud/pull/315 for details"),
            );

            Ok(Self::Void(<Semi>::default()))
        } else {
            Err(lookahead.error())
        }
    }
}

impl ToTokens for ElementBody {
    fn to_tokens(&self, tokens: &mut TokenStream) {
        match self {
            Self::Void(semi) => semi.to_tokens(tokens),
            Self::Block(block) => block.to_tokens(tokens),
        }
    }
}

/// A block of markup nodes enclosed in braces.
///
/// Represents `{ ... }` containing zero or more [`Markup`] nodes.
#[derive(Debug, Clone)]
pub struct Block<E> {
    /// The braces token.
    pub brace_token: Brace,
    /// The markup nodes inside the block.
    pub markups: Markups<E>,
}

impl<E: MaybeElement> DiagnosticParse for Block<E> {
    fn diagnostic_parse(
        input: ParseStream,
        diagnostics: &mut Vec<Diagnostic>,
    ) -> syn::Result<Self> {
        let content;
        Ok(Self {
            brace_token: braced!(content in input),
            markups: content.diagnostic_parse(diagnostics)?,
        })
    }
}

impl<E: ToTokens> ToTokens for Block<E> {
    fn to_tokens(&self, tokens: &mut TokenStream) {
        self.brace_token.surround(tokens, |tokens| {
            self.markups.to_tokens(tokens);
        });
    }
}

/// An element attribute.
///
/// Represents CSS classes (`.class`), IDs (`#id`), or named attributes
/// (`attr="value"`, `attr`, `attr=[condition]`).
#[derive(Debug, Clone)]
pub enum ContainerAttribute {
    /// A CSS class attribute (e.g., `.container`, `.active[condition]`).
    Class {
        /// The dot token.
        dot_token: Dot,
        /// The class name.
        name: ContainerNameOrMarkup,
        /// Optional condition to toggle the class.
        toggler: Option<Toggler>,
    },
    /// An ID attribute (e.g., `#main`, `#(dynamic_id)`).
    Id {
        /// The pound token.
        pound_token: Pound,
        /// The ID value.
        name: ContainerNameOrMarkup,
    },
    /// A named attribute (e.g., `type="text"`, `disabled`, `hidden=[condition]`).
    Named {
        /// The attribute name.
        name: AttributeName,
        /// The attribute type (value, optional, or empty).
        attr_type: AttributeType,
    },
}

impl DiagnosticParse for ContainerAttribute {
    fn diagnostic_parse(
        input: ParseStream,
        diagnostics: &mut Vec<Diagnostic>,
    ) -> syn::Result<Self> {
        let lookahead = input.lookahead1();

        if lookahead.peek(Dot) {
            Ok(Self::Class {
                dot_token: input.parse()?,
                name: input.diagnostic_parse(diagnostics)?,
                toggler: {
                    let lookahead = input.lookahead1();

                    if lookahead.peek(Bracket) {
                        Some(input.diagnostic_parse(diagnostics)?)
                    } else {
                        None
                    }
                },
            })
        } else if lookahead.peek(Pound) {
            Ok(Self::Id {
                pound_token: input.parse()?,
                name: input.diagnostic_parse(diagnostics)?,
            })
        } else {
            let name = input.diagnostic_parse::<AttributeName>(diagnostics)?;

            if input.peek(Question) {
                input.parse::<Question>()?;
            }

            Ok(Self::Named {
                name,
                attr_type: input.diagnostic_parse(diagnostics)?,
            })
        }
    }
}

impl ToTokens for ContainerAttribute {
    fn to_tokens(&self, tokens: &mut TokenStream) {
        match self {
            Self::Class {
                dot_token,
                name,
                toggler,
            } => {
                dot_token.to_tokens(tokens);
                name.to_tokens(tokens);
                if let Some(toggler) = toggler {
                    toggler.to_tokens(tokens);
                }
            }
            Self::Id { pound_token, name } => {
                pound_token.to_tokens(tokens);
                name.to_tokens(tokens);
            }
            Self::Named { name, attr_type } => {
                name.to_tokens(tokens);
                attr_type.to_tokens(tokens);
            }
        }
    }
}

/// A name or dynamic markup for class/ID attributes.
///
/// Supports both static names (e.g., `container`) and dynamic expressions
/// (e.g., `(compute_class())`).
#[derive(Debug, Clone)]
pub enum ContainerNameOrMarkup {
    /// A static attribute name.
    Name(AttributeName),
    /// A dynamic markup expression.
    Markup(Markup<NoElement>),
}

impl DiagnosticParse for ContainerNameOrMarkup {
    fn diagnostic_parse(
        input: ParseStream,
        diagnostics: &mut Vec<Diagnostic>,
    ) -> syn::Result<Self> {
        if input.peek(Ident::peek_any) || input.peek(Lit) {
            input.diagnostic_parse(diagnostics).map(Self::Name)
        } else {
            input.diagnostic_parse(diagnostics).map(Self::Markup)
        }
    }
}

impl Parse for ContainerNameOrMarkup {
    fn parse(input: ParseStream) -> syn::Result<Self> {
        Self::diagnostic_parse(input, &mut Vec::new())
    }
}

impl ToTokens for ContainerNameOrMarkup {
    fn to_tokens(&self, tokens: &mut TokenStream) {
        match self {
            Self::Name(name) => name.to_tokens(tokens),
            Self::Markup(markup) => markup.to_tokens(tokens),
        }
    }
}

impl Display for ContainerNameOrMarkup {
    fn fmt(&self, f: &mut Formatter) -> fmt::Result {
        match self {
            Self::Name(name) => name.fmt(f),
            Self::Markup(markup) => markup.to_token_stream().fmt(f),
        }
    }
}

/// The type of value for a named attribute.
///
/// There are three distinct syntaxes:
///
/// 1. **Normal attributes** (with `=` and a value): `attr="value"`
/// 2. **Optional attributes** (with `=` and brackets): `attr=[condition]`
///    - The `=` is required. The value is conditionally rendered based on the condition.
/// 3. **Empty/boolean attributes** (no `=`, optional brackets):
///    - `disabled` - attribute always present (no condition)
///    - `checked[is_checked]` - attribute conditionally present/absent (note: no `=` sign!)
///
/// The key distinction between Optional and Empty:
/// - `disabled=[condition]` (Optional): Attribute has a value that is conditionally rendered
/// - `disabled[condition]` (Empty): Entire attribute is conditionally present or absent
///
/// For boolean HTML attributes like `disabled`, `checked`, `autofocus`, use the Empty syntax
/// because these attributes don't have values—they're either present or absent.
#[derive(Debug, Clone)]
pub enum AttributeType {
    /// Normal attribute with a value (e.g., `type="text"`).
    Normal {
        /// The equals token.
        eq_token: Eq,
        /// The attribute value.
        value: Markup<NoElement>,
    },
    /// Optional attribute with a condition (e.g., `disabled=[is_disabled]`).
    ///
    /// Note: The equals sign is **required** for Optional attributes.
    /// The value is conditionally rendered based on the toggler condition.
    Optional {
        /// The equals token.
        eq_token: Eq,
        /// The condition that controls whether the attribute is present.
        toggler: Toggler,
    },
    /// Empty/boolean attribute, optionally conditional.
    ///
    /// Examples:
    /// - `disabled` - attribute always present (unconditional)
    /// - `checked[is_checked]` - attribute conditionally present (no `=` sign!)
    ///
    /// Note: There is **no equals sign** for Empty attributes, even when conditional.
    /// For boolean HTML attributes, the entire attribute is either present or absent.
    Empty(Option<Toggler>),
}

impl DiagnosticParse for AttributeType {
    fn diagnostic_parse(
        input: ParseStream,
        diagnostics: &mut Vec<Diagnostic>,
    ) -> syn::Result<Self> {
        let lookahead = input.lookahead1();

        if lookahead.peek(Eq) {
            let eq_token = input.parse()?;

            if input.peek(Bracket) {
                Ok(Self::Optional {
                    eq_token,
                    toggler: input.diagnostic_parse(diagnostics)?,
                })
            } else {
                // Check for special fx { ... } syntax without parentheses
                if input.peek(Ident::peek_any) {
                    let fork = input.fork();
                    if let Ok(ident) = fork.call(Ident::parse_any)
                        && ident == "fx"
                        && fork.peek(Brace)
                    {
                        // This is the fx { ... } syntax - parse it specially
                        let fx_ident: Ident = input.call(Ident::parse_any)?;
                        let content;
                        let brace_token = braced!(content in input);

                        // Create a BraceSplice with fx as first item and brace content as second
                        let mut items = vec![Markup::Splice {
                            paren_token: Box::new(Paren::default()),
                            expr: Box::new(syn::Expr::Path(syn::ExprPath {
                                attrs: vec![],
                                qself: None,
                                path: syn::Path::from(fx_ident),
                            })),
                        }];

                        if !content.is_empty() {
                            // Parse the content as a block
                            let tokens: proc_macro2::TokenStream = content.parse()?;
                            items.push(Markup::Splice {
                                paren_token: Box::new(Paren::default()),
                                expr: Box::new(syn::parse2(quote::quote! { { #tokens } })?),
                            });
                        }

                        return Ok(Self::Normal {
                            eq_token,
                            value: Markup::BraceSplice { brace_token, items },
                        });
                    }
                }

                Ok(Self::Normal {
                    eq_token,
                    value: input.diagnostic_parse(diagnostics)?,
                })
            }
        } else if lookahead.peek(Bracket) {
            Ok(Self::Empty(Some(input.diagnostic_parse(diagnostics)?)))
        } else {
            Ok(Self::Empty(None))
        }
    }
}

impl ToTokens for AttributeType {
    fn to_tokens(&self, tokens: &mut TokenStream) {
        match self {
            Self::Normal { eq_token, value } => {
                eq_token.to_tokens(tokens);
                value.to_tokens(tokens);
            }
            Self::Optional { eq_token, toggler } => {
                eq_token.to_tokens(tokens);
                toggler.to_tokens(tokens);
            }
            Self::Empty(toggler) => {
                if let Some(toggler) = toggler {
                    toggler.to_tokens(tokens);
                }
            }
        }
    }
}

/// An element tag name.
///
/// Represents the name of an element (e.g., `div`, `button`, `input`).
#[derive(Debug, Clone)]
pub struct ElementName {
    /// The element's tag name identifier.
    pub name: Ident,
}

impl DiagnosticParse for ElementName {
    fn diagnostic_parse(
        input: ParseStream,
        _diagnostics: &mut Vec<Diagnostic>,
    ) -> syn::Result<Self> {
        Ok(Self {
            name: input.call(Ident::parse_any)?,
        })
    }
}

impl Parse for ElementName {
    fn parse(input: ParseStream) -> syn::Result<Self> {
        Self::diagnostic_parse(input, &mut Vec::new())
    }
}

impl ToTokens for ElementName {
    fn to_tokens(&self, tokens: &mut TokenStream) {
        self.name.to_tokens(tokens);
    }
}

impl Display for ElementName {
    fn fmt(&self, f: &mut Formatter) -> fmt::Result {
        self.name.fmt(f)
    }
}

/// An attribute name supporting kebab-case and colon-separated names.
///
/// Supports attribute names like `type`, `hx-post`, `data-value`, `aria:label`, etc.
#[derive(Debug, Clone)]
pub struct AttributeName {
    /// The name fragments separated by hyphens or colons.
    pub name: Punctuated<AttributeNameFragment, AttributeNamePunct>,
}

impl DiagnosticParse for AttributeName {
    fn diagnostic_parse(
        input: ParseStream,
        diagnostics: &mut Vec<Diagnostic>,
    ) -> syn::Result<Self> {
        Ok(Self {
            name: {
                let mut punctuated = Punctuated::new();

                loop {
                    punctuated.push_value(input.diagnostic_parse(diagnostics)?);

                    if !(input.peek(Minus) || input.peek(Colon)) {
                        break;
                    }

                    let punct = input.diagnostic_parse(diagnostics)?;
                    punctuated.push_punct(punct);
                }

                punctuated
            },
        })
    }
}

impl Parse for AttributeName {
    fn parse(input: ParseStream) -> syn::Result<Self> {
        Self::diagnostic_parse(input, &mut Vec::new())
    }
}

impl ToTokens for AttributeName {
    fn to_tokens(&self, tokens: &mut TokenStream) {
        self.name.to_tokens(tokens);
    }
}

impl Display for AttributeName {
    fn fmt(&self, f: &mut Formatter) -> fmt::Result {
        for pair in self.name.pairs() {
            match pair {
                Pair::Punctuated(fragment, punct) => {
                    fragment.fmt(f)?;
                    punct.fmt(f)?;
                }
                Pair::End(fragment) => {
                    fragment.fmt(f)?;
                }
            }
        }

        Ok(())
    }
}

/// A fragment of an attribute name.
///
/// Attribute names can contain identifiers, integers, string literals, or be empty
/// (for leading/trailing hyphens or colons).
#[derive(Debug, Clone)]
pub enum AttributeNameFragment {
    /// An identifier fragment (e.g., `hx`, `post`, `data`).
    Ident(Ident),
    /// An integer literal fragment (e.g., `2` in `h2`).
    LitInt(LitInt),
    /// A string literal fragment.
    LitStr(LitStr),
    /// An empty fragment (for leading/trailing separators).
    Empty,
}

impl DiagnosticParse for AttributeNameFragment {
    fn diagnostic_parse(
        input: ParseStream,
        _diagnostics: &mut Vec<Diagnostic>,
    ) -> syn::Result<Self> {
        let lookahead = input.lookahead1();

        if lookahead.peek(Ident::peek_any) {
            input.call(Ident::parse_any).map(Self::Ident)
        } else if lookahead.peek(LitInt) {
            input.parse().map(Self::LitInt)
        } else if lookahead.peek(LitStr) {
            input.parse().map(Self::LitStr)
        } else if lookahead.peek(Minus) || lookahead.peek(Colon) {
            Ok(Self::Empty)
        } else {
            Err(lookahead.error())
        }
    }
}

impl ToTokens for AttributeNameFragment {
    fn to_tokens(&self, tokens: &mut TokenStream) {
        match self {
            Self::Ident(ident) => ident.to_tokens(tokens),
            Self::LitInt(lit) => lit.to_tokens(tokens),
            Self::LitStr(lit) => lit.to_tokens(tokens),
            Self::Empty => {}
        }
    }
}

impl Display for AttributeNameFragment {
    fn fmt(&self, f: &mut Formatter) -> fmt::Result {
        match self {
            Self::Ident(ident) => ident.fmt(f),
            Self::LitInt(lit) => lit.fmt(f),
            Self::LitStr(lit) => lit.value().fmt(f),
            Self::Empty => Ok(()),
        }
    }
}

/// A literal value in the template.
///
/// Represents string, integer, or float literals that appear directly in the template.
#[derive(Debug, Clone)]
pub struct ContainerLit {
    /// The underlying Rust literal.
    pub lit: syn::Lit,
}

impl DiagnosticParse for ContainerLit {
    fn diagnostic_parse(
        input: ParseStream,
        diagnostics: &mut Vec<Diagnostic>,
    ) -> syn::Result<Self> {
        let lookahead = input.lookahead1();

        if lookahead.peek(Lit) {
            let lit = input.parse()?;
            match &lit {
                Lit::Str(_) | Lit::Int(_) | Lit::Float(_) => Ok(Self { lit }),
                Lit::Char(lit_char) => {
                    diagnostics.push(lit_char.span().error(format!(
                        r#"literal must be double-quoted: `"{}"`"#,
                        lit_char.value()
                    )));
                    Ok(Self {
                        lit: Lit::Str(LitStr::new("", lit_char.span())),
                    })
                }
                Lit::Bool(lit_bool) => {
                    // Convert boolean to string representation for markup_to_bool_tokens
                    let bool_str = if lit_bool.value { "true" } else { "false" };
                    Ok(Self {
                        lit: Lit::Str(LitStr::new(bool_str, lit.span())),
                    })
                }
                _ => {
                    diagnostics.push(lit.span().error("expected string, integer, or float"));
                    Ok(Self {
                        lit: Lit::Str(LitStr::new("", lit.span())),
                    })
                }
            }
        } else {
            Err(lookahead.error())
        }
    }
}

impl ToTokens for ContainerLit {
    fn to_tokens(&self, tokens: &mut TokenStream) {
        self.lit.to_tokens(tokens);
    }
}

impl Display for ContainerLit {
    fn fmt(&self, f: &mut Formatter) -> fmt::Result {
        match &self.lit {
            Lit::Str(lit) => lit.value().fmt(f),
            Lit::Int(lit) => lit.fmt(f),
            Lit::Float(lit) => lit.fmt(f),
            _ => self.lit.to_token_stream().fmt(f),
        }
    }
}

/// A numeric literal with optional CSS units.
///
/// Supports plain numbers and numbers with percentage or viewport units
/// (e.g., `100`, `3.14`, `50%`, `100vw`, `80vh`, `90dvw`, `75dvh`).
#[derive(Debug, Clone)]
pub struct NumericLit {
    /// The string representation of the numeric value with units.
    pub value: String,
    /// The type of numeric literal (integer, float, with or without units).
    pub number_type: NumericType,
}

/// The type of a numeric literal.
///
/// Distinguishes between integers and reals, and tracks CSS units
/// (percentages, viewport width/height, dynamic viewport width/height).
#[derive(Debug, Clone)]
pub enum NumericType {
    /// Integer literal (e.g., `42`).
    Integer,
    /// Real/float literal (e.g., `3.14`).
    Real,
    /// Integer percentage (e.g., `100%`).
    IntegerPercent,
    /// Real percentage (e.g., `33.3%`).
    RealPercent,
    /// Integer viewport width (e.g., `50vw`).
    IntegerVw,
    /// Real viewport width (e.g., `33.3vw`).
    RealVw,
    /// Integer viewport height (e.g., `100vh`).
    IntegerVh,
    /// Real viewport height (e.g., `80.5vh`).
    RealVh,
    /// Integer dynamic viewport width (e.g., `50dvw`).
    IntegerDvw,
    /// Real dynamic viewport width (e.g., `33.3dvw`).
    RealDvw,
    /// Integer dynamic viewport height (e.g., `100dvh`).
    IntegerDvh,
    /// Real dynamic viewport height (e.g., `80.5dvh`).
    RealDvh,
}

impl NumericLit {
    fn try_parse(input: &str) -> Option<Self> {
        // Check for percentage values
        if let Some(num_str) = input.strip_suffix('%')
            && num_str.parse::<f32>().is_ok()
        {
            return Some(Self {
                value: input.to_string(),
                number_type: if num_str.contains('.') {
                    NumericType::RealPercent
                } else {
                    NumericType::IntegerPercent
                },
            });
        }

        // Check for viewport units (vw, vh, dvw, dvh)
        if let Some(num_str) = input.strip_suffix("vw")
            && num_str.parse::<f32>().is_ok()
        {
            return Some(Self {
                value: input.to_string(),
                number_type: if num_str.contains('.') {
                    NumericType::RealVw
                } else {
                    NumericType::IntegerVw
                },
            });
        }

        if let Some(num_str) = input.strip_suffix("vh")
            && num_str.parse::<f32>().is_ok()
        {
            return Some(Self {
                value: input.to_string(),
                number_type: if num_str.contains('.') {
                    NumericType::RealVh
                } else {
                    NumericType::IntegerVh
                },
            });
        }

        if let Some(num_str) = input.strip_suffix("dvw")
            && num_str.parse::<f32>().is_ok()
        {
            return Some(Self {
                value: input.to_string(),
                number_type: if num_str.contains('.') {
                    NumericType::RealDvw
                } else {
                    NumericType::IntegerDvw
                },
            });
        }

        if let Some(num_str) = input.strip_suffix("dvh")
            && num_str.parse::<f32>().is_ok()
        {
            return Some(Self {
                value: input.to_string(),
                number_type: if num_str.contains('.') {
                    NumericType::RealDvh
                } else {
                    NumericType::IntegerDvh
                },
            });
        }

        // Check for plain numbers (integer or float)
        if input.parse::<i64>().is_ok() {
            return Some(Self {
                value: input.to_string(),
                number_type: NumericType::Integer,
            });
        }

        if input.parse::<f64>().is_ok() {
            return Some(Self {
                value: input.to_string(),
                number_type: NumericType::Real,
            });
        }

        None
    }

    /// Try to parse identifier-based unit patterns like vw50, vh100, dvw90, dvh60, etc.
    fn try_parse_identifier_unit(input: &str) -> Option<Self> {
        // Define supported units (only viewport units that are actually supported)
        let units = [
            "vw", "vh", "dvw", "dvh", // Viewport units only
        ];

        for unit in &units {
            if let Some(number_part) = input.strip_prefix(unit)
                && !number_part.is_empty()
                && number_part.chars().all(|c| c.is_ascii_digit() || c == '.')
            {
                // Construct the unit+number string
                let full_unit_string = format!("{number_part}{unit}");

                // Try to parse using the existing logic
                if let Some(numeric_lit) = Self::try_parse(&full_unit_string) {
                    return Some(numeric_lit);
                }
            }
        }

        None
    }

    /// Check if an identifier is a CSS unit that supports function syntax
    fn is_unit_identifier(input: &str) -> bool {
        matches!(
            input,
            "vw" | "vh"
                | "dvw"
                | "dvh"
                | "calc"
                | "min"
                | "max"
                | "clamp"
                | "percent"
                | "rgb"
                | "rgba" // Viewport units, calc function, CSS math functions, helper functions, and color functions
        )
    }
}

impl ToTokens for NumericLit {
    fn to_tokens(&self, tokens: &mut TokenStream) {
        let value_str = &self.value;
        tokens.extend(quote::quote! { #value_str });
    }
}

impl Display for NumericLit {
    fn fmt(&self, f: &mut Formatter) -> fmt::Result {
        self.value.fmt(f)
    }
}

impl DiagnosticParse for NumericLit {
    fn diagnostic_parse(
        input: ParseStream,
        _diagnostics: &mut Vec<Diagnostic>,
    ) -> syn::Result<Self> {
        // This should not be called directly - NumericLit is created in the Markup parsing logic
        Err(Error::new(
            input.span(),
            "NumericLit should not be parsed directly",
        ))
    }
}

/// Punctuation separating fragments in an attribute name.
///
/// Supports hyphens (`-`) for kebab-case names like `hx-post` and
/// colons (`:`) for namespaced attributes like `aria:label`.
#[derive(Debug, Clone)]
pub enum AttributeNamePunct {
    /// A colon separator (`:`) for namespaced attributes.
    Colon(Colon),
    /// A hyphen separator (`-`) for kebab-case names.
    Hyphen(Minus),
}

impl DiagnosticParse for AttributeNamePunct {
    fn diagnostic_parse(input: ParseStream, _: &mut Vec<Diagnostic>) -> syn::Result<Self> {
        let lookahead = input.lookahead1();

        if lookahead.peek(Colon) {
            input.parse().map(Self::Colon)
        } else if lookahead.peek(Minus) {
            input.parse().map(Self::Hyphen)
        } else {
            Err(lookahead.error())
        }
    }
}

impl ToTokens for AttributeNamePunct {
    fn to_tokens(&self, tokens: &mut TokenStream) {
        match self {
            Self::Colon(token) => token.to_tokens(tokens),
            Self::Hyphen(token) => token.to_tokens(tokens),
        }
    }
}

impl Display for AttributeNamePunct {
    fn fmt(&self, f: &mut Formatter) -> fmt::Result {
        match self {
            Self::Colon(_) => f.write_str(":"),
            Self::Hyphen(_) => f.write_str("-"),
        }
    }
}

/// A conditional expression for toggling attributes or classes.
///
/// Represented as `[condition]`, where `condition` is a Rust expression
/// that determines whether the attribute/class should be present.
#[derive(Debug, Clone)]
pub struct Toggler {
    /// The bracket tokens.
    pub bracket_token: Bracket,
    /// The boolean condition expression.
    pub cond: Expr,
}

impl DiagnosticParse for Toggler {
    fn diagnostic_parse(input: ParseStream, _: &mut Vec<Diagnostic>) -> syn::Result<Self> {
        let content;
        Ok(Self {
            bracket_token: bracketed!(content in input),
            cond: content.parse()?,
        })
    }
}

impl ToTokens for Toggler {
    fn to_tokens(&self, tokens: &mut TokenStream) {
        self.bracket_token.surround(tokens, |tokens| {
            self.cond.to_tokens(tokens);
        });
    }
}

/// A control flow construct in the template.
///
/// Represents `@if`, `@for`, `@while`, `@match`, and `@let` constructs
/// that enable conditional rendering, loops, and pattern matching.
#[derive(Debug, Clone)]
pub struct ControlFlow<E> {
    /// The `@` token prefix.
    pub at_token: At,
    /// The specific control flow kind.
    pub kind: ControlFlowKind<E>,
}

impl<E: MaybeElement> DiagnosticParse for ControlFlow<E> {
    fn diagnostic_parse(
        input: ParseStream,
        diagnostics: &mut Vec<Diagnostic>,
    ) -> syn::Result<Self> {
        Ok(Self {
            at_token: input.parse()?,
            kind: {
                let lookahead = input.lookahead1();

                if lookahead.peek(If) {
                    ControlFlowKind::If(input.diagnostic_parse(diagnostics)?)
                } else if lookahead.peek(For) {
                    ControlFlowKind::For(input.diagnostic_parse(diagnostics)?)
                } else if lookahead.peek(While) {
                    ControlFlowKind::While(input.diagnostic_parse(diagnostics)?)
                } else if lookahead.peek(Match) {
                    ControlFlowKind::Match(input.diagnostic_parse(diagnostics)?)
                } else if lookahead.peek(Let) {
                    let Stmt::Local(local) = input.parse()? else {
                        unreachable!()
                    };

                    ControlFlowKind::Let(Box::new(local))
                } else {
                    return Err(lookahead.error());
                }
            },
        })
    }
}

impl<E: ToTokens> ToTokens for ControlFlow<E> {
    fn to_tokens(&self, tokens: &mut TokenStream) {
        self.at_token.to_tokens(tokens);
        match &self.kind {
            ControlFlowKind::Let(local) => local.to_tokens(tokens),
            ControlFlowKind::If(if_) => if_.to_tokens(tokens),
            ControlFlowKind::For(for_) => for_.to_tokens(tokens),
            ControlFlowKind::While(while_) => while_.to_tokens(tokens),
            ControlFlowKind::Match(match_) => match_.to_tokens(tokens),
        }
    }
}

/// The specific kind of control flow construct.
///
/// Supports `@let` bindings, `@if` conditionals, `@for` loops, `@while` loops,
/// and `@match` expressions.
#[derive(Debug, Clone)]
pub enum ControlFlowKind<E> {
    /// A `@let` binding (e.g., `@let x = 42;`).
    Let(Box<Local>),
    /// An `@if` conditional (e.g., `@if condition { ... } @else { ... }`).
    If(Box<IfExpr<E>>),
    /// A `@for` loop (e.g., `@for item in items { ... }`).
    For(Box<ForExpr<E>>),
    /// A `@while` loop (e.g., `@while condition { ... }`).
    While(Box<WhileExpr<E>>),
    /// A `@match` expression (e.g., `@match value { pattern => { ... } }`).
    Match(Box<MatchExpr<E>>),
}

/// An `@if` conditional expression.
///
/// Supports regular conditionals (`@if expr { ... }`) and pattern matching
/// (`@if let pattern = expr { ... }`), optionally followed by `@else`.
#[derive(Debug, Clone)]
pub struct IfExpr<E> {
    /// The `if` keyword token.
    pub if_token: If,
    /// The condition (expression or pattern match).
    pub cond: IfCondition,
    /// The block to execute when the condition is true.
    pub then_branch: Block<E>,
    /// Optional `@else` branch.
    pub else_branch: Option<(At, Else, Box<IfOrBlock<E>>)>,
}

/// The condition in an `@if` expression.
///
/// Can be either a regular boolean expression or a `let` pattern match.
#[derive(Debug, Clone)]
pub enum IfCondition {
    /// A regular boolean expression.
    Expr(Box<Expr>),
    /// A pattern match (`if let pattern = expr`).
    Let {
        /// The `let` keyword token.
        let_token: Box<Let>,
        /// The pattern to match against.
        pat: Box<Pat>,
        /// The equals token.
        eq_token: Box<syn::token::Eq>,
        /// The expression to match.
        expr: Box<Expr>,
    },
}

impl ToTokens for IfCondition {
    fn to_tokens(&self, tokens: &mut TokenStream) {
        match self {
            Self::Expr(expr) => expr.to_tokens(tokens),
            Self::Let {
                let_token,
                pat,
                eq_token,
                expr,
            } => {
                let_token.to_tokens(tokens);
                pat.to_tokens(tokens);
                eq_token.to_tokens(tokens);
                expr.to_tokens(tokens);
            }
        }
    }
}

impl<E: MaybeElement> DiagnosticParse for IfExpr<E> {
    fn diagnostic_parse(
        input: ParseStream,
        diagnostics: &mut Vec<Diagnostic>,
    ) -> syn::Result<Self> {
        let if_token: If = input.parse()?;

        // Parse the condition - this could be a regular expression or a let pattern
        let cond = if input.peek(Let) {
            // Handle "if let" patterns
            let let_token: Let = input.parse()?;
            let pat: Pat = input.call(Pat::parse_multi_with_leading_vert)?;
            let eq_token: syn::token::Eq = input.parse()?;
            let expr: Expr = input.call(Expr::parse_without_eager_brace)?;

            IfCondition::Let {
                let_token: Box::new(let_token),
                pat: Box::new(pat),
                eq_token: Box::new(eq_token),
                expr: Box::new(expr),
            }
        } else {
            // Regular if condition
            IfCondition::Expr(Box::new(input.call(Expr::parse_without_eager_brace)?))
        };

        Ok(Self {
            if_token,
            cond,
            then_branch: input.diagnostic_parse(diagnostics)?,
            else_branch: {
                if input.peek(At) && input.peek2(Else) {
                    Some((
                        input.parse()?,
                        input.parse()?,
                        input.diagnostic_parse(diagnostics)?,
                    ))
                } else {
                    None
                }
            },
        })
    }
}

impl<E: ToTokens> ToTokens for IfExpr<E> {
    fn to_tokens(&self, tokens: &mut TokenStream) {
        self.if_token.to_tokens(tokens);
        self.cond.to_tokens(tokens);
        self.then_branch.to_tokens(tokens);
        if let Some((at_token, else_token, else_branch)) = &self.else_branch {
            at_token.to_tokens(tokens);
            else_token.to_tokens(tokens);
            else_branch.to_tokens(tokens);
        }
    }
}

/// The body of an `@else` branch.
///
/// Can be either another `@if` (for `@else if` chains) or a block.
#[derive(Debug, Clone)]
pub enum IfOrBlock<E> {
    /// Another `@if` expression (for `@else if` chains).
    If(IfExpr<E>),
    /// A block (for final `@else { ... }`).
    Block(Block<E>),
}

impl<E: MaybeElement> DiagnosticParse for IfOrBlock<E> {
    fn diagnostic_parse(
        input: ParseStream,
        diagnostics: &mut Vec<Diagnostic>,
    ) -> syn::Result<Self> {
        let lookahead = input.lookahead1();

        if lookahead.peek(If) {
            input.diagnostic_parse(diagnostics).map(Self::If)
        } else if lookahead.peek(Brace) {
            input.diagnostic_parse(diagnostics).map(Self::Block)
        } else {
            Err(lookahead.error())
        }
    }
}

impl<E: ToTokens> ToTokens for IfOrBlock<E> {
    fn to_tokens(&self, tokens: &mut TokenStream) {
        match self {
            Self::If(if_) => if_.to_tokens(tokens),
            Self::Block(block) => block.to_tokens(tokens),
        }
    }
}

/// A `@for` loop expression.
///
/// Iterates over an iterable expression, binding each element to a pattern
/// and rendering the loop body for each element.
#[derive(Debug, Clone)]
pub struct ForExpr<E> {
    /// The `for` keyword token.
    pub for_token: For,
    /// The pattern to bind each element to.
    pub pat: Pat,
    /// The `in` keyword token.
    pub in_token: In,
    /// The expression to iterate over.
    pub expr: Expr,
    /// The loop body to render for each element.
    pub body: Block<E>,
}

impl<E: MaybeElement> DiagnosticParse for ForExpr<E> {
    fn diagnostic_parse(
        input: ParseStream,
        diagnostics: &mut Vec<Diagnostic>,
    ) -> syn::Result<Self> {
        Ok(Self {
            for_token: input.parse()?,
            pat: input.call(Pat::parse_multi_with_leading_vert)?,
            in_token: input.parse()?,
            expr: input.call(Expr::parse_without_eager_brace)?,
            body: input.diagnostic_parse(diagnostics)?,
        })
    }
}

impl<E: ToTokens> ToTokens for ForExpr<E> {
    fn to_tokens(&self, tokens: &mut TokenStream) {
        self.for_token.to_tokens(tokens);
        self.pat.to_tokens(tokens);
        self.in_token.to_tokens(tokens);
        self.expr.to_tokens(tokens);
        self.body.to_tokens(tokens);
    }
}

/// A `@while` loop expression.
///
/// Repeatedly renders the loop body while the condition evaluates to true.
#[derive(Debug, Clone)]
pub struct WhileExpr<E> {
    /// The `while` keyword token.
    pub while_token: While,
    /// The loop condition expression.
    pub cond: Expr,
    /// The loop body to render while the condition is true.
    pub body: Block<E>,
}

impl<E: MaybeElement> DiagnosticParse for WhileExpr<E> {
    fn diagnostic_parse(
        input: ParseStream,
        diagnostics: &mut Vec<Diagnostic>,
    ) -> syn::Result<Self> {
        Ok(Self {
            while_token: input.parse()?,
            cond: input.call(Expr::parse_without_eager_brace)?,
            body: input.diagnostic_parse(diagnostics)?,
        })
    }
}

impl<E: ToTokens> ToTokens for WhileExpr<E> {
    fn to_tokens(&self, tokens: &mut TokenStream) {
        self.while_token.to_tokens(tokens);
        self.cond.to_tokens(tokens);
        self.body.to_tokens(tokens);
    }
}

/// A `@match` expression.
///
/// Pattern matches against an expression and renders the corresponding arm's body
/// for the first matching pattern.
#[derive(Debug, Clone)]
pub struct MatchExpr<E> {
    /// The `match` keyword token.
    pub match_token: Match,
    /// The expression to match against.
    pub expr: Expr,
    /// The braces surrounding the match arms.
    pub brace_token: Brace,
    /// The match arms.
    pub arms: Vec<MatchArm<E>>,
}

impl<E: MaybeElement> DiagnosticParse for MatchExpr<E> {
    fn diagnostic_parse(
        input: ParseStream,
        diagnostics: &mut Vec<Diagnostic>,
    ) -> syn::Result<Self> {
        let match_token = input.parse()?;
        let expr = input.call(Expr::parse_without_eager_brace)?;

        let content;
        let brace_token = braced!(content in input);

        let mut arms = Vec::new();
        while !content.is_empty() {
            arms.push(content.diagnostic_parse(diagnostics)?);
        }

        Ok(Self {
            match_token,
            expr,
            brace_token,
            arms,
        })
    }
}

impl<E: ToTokens> ToTokens for MatchExpr<E> {
    fn to_tokens(&self, tokens: &mut TokenStream) {
        self.match_token.to_tokens(tokens);
        self.expr.to_tokens(tokens);
        self.brace_token.surround(tokens, |tokens| {
            for arm in &self.arms {
                arm.to_tokens(tokens);
            }
        });
    }
}

/// A single arm in a `@match` expression.
///
/// Consists of a pattern, optional guard, and a body to render if the pattern matches.
#[derive(Debug, Clone)]
pub struct MatchArm<E> {
    /// The pattern to match against.
    pub pat: Pat,
    /// Optional guard condition (`if guard_expr`).
    pub guard: Option<(If, Expr)>,
    /// The fat arrow token (`=>`).
    pub fat_arrow_token: FatArrow,
    /// The markup to render if this arm matches.
    pub body: Markup<E>,
    /// Optional trailing comma.
    pub comma_token: Option<Comma>,
}

impl<E: MaybeElement> DiagnosticParse for MatchArm<E> {
    fn diagnostic_parse(
        input: ParseStream,
        diagnostics: &mut Vec<Diagnostic>,
    ) -> syn::Result<Self> {
        Ok(Self {
            pat: Pat::parse_multi_with_leading_vert(input)?,
            guard: {
                if input.peek(If) {
                    Some((input.parse()?, input.parse()?))
                } else {
                    None
                }
            },
            fat_arrow_token: input.parse()?,
            body: Markup::diagnostic_parse_in_block(input, diagnostics)?,
            comma_token: if input.peek(Comma) {
                Some(input.parse()?)
            } else {
                None
            },
        })
    }
}

impl<E: ToTokens> ToTokens for MatchArm<E> {
    fn to_tokens(&self, tokens: &mut TokenStream) {
        self.pat.to_tokens(tokens);
        if let Some((if_token, guard)) = &self.guard {
            if_token.to_tokens(tokens);
            guard.to_tokens(tokens);
        }
        self.fat_arrow_token.to_tokens(tokens);
        self.body.to_tokens(tokens);
        if let Some(comma_token) = &self.comma_token {
            comma_token.to_tokens(tokens);
        }
    }
}

/// Trait for parsing AST nodes with diagnostic support.
///
/// Similar to `syn::parse::Parse`, but accumulates non-fatal diagnostics
/// (warnings and errors) during parsing instead of immediately failing.
pub trait DiagnosticParse: Sized {
    /// Parses `Self` from the input stream, accumulating diagnostics.
    ///
    /// # Errors
    ///
    /// Returns an error if parsing fails fatally and cannot continue.
    fn diagnostic_parse(input: ParseStream, diagnostics: &mut Vec<Diagnostic>)
    -> syn::Result<Self>;
}

impl<T: DiagnosticParse> DiagnosticParse for Box<T> {
    fn diagnostic_parse(
        input: ParseStream,
        diagnostics: &mut Vec<Diagnostic>,
    ) -> syn::Result<Self> {
        Ok(Self::new(input.diagnostic_parse(diagnostics)?))
    }
}

trait DiagonsticParseExt: Sized {
    fn diagnostic_parse<T: DiagnosticParse>(
        self,
        diagnostics: &mut Vec<Diagnostic>,
    ) -> syn::Result<T>;
}

impl DiagonsticParseExt for ParseStream<'_> {
    fn diagnostic_parse<T>(self, diagnostics: &mut Vec<Diagnostic>) -> syn::Result<T>
    where
        T: DiagnosticParse,
    {
        T::diagnostic_parse(self, diagnostics)
    }
}
