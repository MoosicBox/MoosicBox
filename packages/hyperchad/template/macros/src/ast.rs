use std::fmt::{self, Display, Formatter};

use proc_macro2::TokenStream;
use proc_macro2_diagnostics::{Diagnostic, SpanDiagnosticExt};
use quote::ToTokens;
use syn::{
    Error, Expr, Ident, Lit, LitBool, LitInt, LitStr, Local, Pat, Stmt, braced, bracketed,
    ext::IdentExt,
    parenthesized,
    parse::{Lookahead1, Parse, ParseStream},
    parse_quote,
    punctuated::{Pair, Punctuated},
    spanned::Spanned,
    token::{
        At, Brace, Bracket, Colon, Comma, Dot, Else, Eq, FatArrow, For, If, In, Let, Match, Minus,
        Paren, Pound, Question, Semi, Slash, While,
    },
};

#[derive(Debug, Clone)]
pub struct Markups<E> {
    pub markups: Vec<Markup<E>>,
}

impl<E: MaybeElement> DiagnosticParse for Markups<E> {
    fn diagnostic_parse(
        input: ParseStream,
        diagnostics: &mut Vec<Diagnostic>,
    ) -> syn::Result<Self> {
        let mut markups = Vec::new();
        while !input.is_empty() {
            markups.push(Markup::diagnostic_parse_in_block(input, diagnostics)?)
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

#[derive(Debug, Clone)]
pub enum Markup<E> {
    Block(Block<E>),
    Lit(ContainerLit),
    NumericLit(NumericLit),
    Splice {
        paren_token: Paren,
        expr: Expr,
    },
    BraceSplice {
        brace_token: Brace,
        items: Vec<Markup<NoElement>>,
    },
    Element(E),
    ControlFlow(ControlFlow<E>),
    Semi(Semi),
}

impl<E: MaybeElement> Markup<E> {
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
            if let Ok(expr) = content.parse::<Expr>() {
                if content.is_empty() {
                    // Successfully parsed as a single expression
                    // Check if it's a function call pattern like "literal"(expression)
                    if let syn::Expr::Call(call) = &expr {
                        if let syn::Expr::Lit(syn::ExprLit {
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
                                        paren_token: Paren::default(),
                                        expr: arg,
                                    });
                                }

                                return Ok(Self::BraceSplice { brace_token, items });
                            }
                        }
                    }

                    // Regular single expression - wrap as BraceSplice with one item
                    let content;
                    let brace_token = braced!(content in input);
                    let expr = content.parse::<Expr>()?;

                    return Ok(Self::BraceSplice {
                        brace_token,
                        items: vec![Markup::Splice {
                            paren_token: Paren::default(),
                            expr,
                        }],
                    });
                }
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
                        paren_token: Paren::default(),
                        expr,
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
                    paren_token: Paren::default(),
                    expr: percent_expr,
                })
            } else {
                Ok(Self::Splice { paren_token, expr })
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
                                format!("Unexpected identifier '{}' after number", unit),
                            ));
                        }
                    }
                }
            }

            // No unit suffix, treat as plain number
            if let Some(numeric_lit) = NumericLit::try_parse(&result) {
                Ok(Self::NumericLit(numeric_lit))
            } else {
                // Fallback to regular literal
                let lit = if result.contains('.') {
                    syn::Lit::Float(syn::LitFloat::new(&result, input.span()))
                } else {
                    syn::Lit::Int(syn::LitInt::new(&result, input.span()))
                };
                Ok(Self::Lit(ContainerLit { lit }))
            }
        } else if lookahead.peek(syn::Token![#]) {
            // Handle hex colors like #fff, #ffffff, #123, #000
            let _pound: syn::Token![#] = input.parse()?;

            // Try to parse as identifier first (for values like #fff, #abc)
            if let Ok(hex_ident) = input.call(Ident::parse_any) {
                let hex_str = hex_ident.to_string();

                // Validate that it's a valid hex color (3, 6, or 8 hex digits)
                if (hex_str.len() == 3 || hex_str.len() == 6 || hex_str.len() == 8)
                    && hex_str.chars().all(|c| c.is_ascii_hexdigit())
                {
                    // Create a string literal with the full hex color
                    let full_hex = format!("#{}", hex_str);
                    let expr: Expr = parse_quote!(#full_hex);
                    return Ok(Self::Splice {
                        paren_token: Paren::default(),
                        expr,
                    });
                } else {
                    return Err(Error::new(
                        hex_ident.span(),
                        format!(
                            "Invalid hex color '{}'. Hex colors must be 3, 6, or 8 hexadecimal digits (0-9, a-f)",
                            hex_str
                        ),
                    ));
                }
            }
            // Try to parse as integer literal (for values like #123, #000)
            else if let Ok(hex_int) = input.parse::<syn::LitInt>() {
                let hex_str = hex_int.to_string();

                // Validate that it's a valid hex color (3, 6, or 8 hex digits)
                if (hex_str.len() == 3 || hex_str.len() == 6 || hex_str.len() == 8)
                    && hex_str.chars().all(|c| c.is_ascii_hexdigit())
                {
                    // Create a string literal with the full hex color
                    let full_hex = format!("#{}", hex_str);
                    let expr: Expr = parse_quote!(#full_hex);
                    return Ok(Self::Splice {
                        paren_token: Paren::default(),
                        expr,
                    });
                } else {
                    return Err(Error::new(
                        hex_int.span(),
                        format!(
                            "Invalid hex color '{}'. Hex colors must be 3, 6, or 8 hexadecimal digits (0-9, a-f)",
                            hex_str
                        ),
                    ));
                }
            } else {
                return Err(Error::new(
                    input.span(),
                    "Expected hex digits after '#' for hex color",
                ));
            }
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
                    } else {
                        // For unit functions like vw, vh, etc., parse a single expression and use unit_functions module
                        let expr: Expr = content.parse()?;
                        parse_quote!(hyperchad_template::unit_functions::#unit_ident(#expr))
                    };

                    return Ok(Self::Splice {
                        paren_token: Paren::default(),
                        expr: call_expr,
                    });
                }
            }

            // Check for identifier followed by % first (approach 2): variable%
            let fork2 = input.fork();
            if let Ok(_ident) = fork2.call(Ident::parse_any) {
                if fork2.peek(syn::Token![%]) {
                    // This is a variable followed by %, like: value%
                    let var_ident: Ident = input.call(Ident::parse_any)?;
                    let _percent: syn::Token![%] = input.parse()?;

                    // Create a splice expression that converts the variable to percentage
                    let expr: Expr =
                        parse_quote!(hyperchad_template::calc::to_percent_number(#var_ident));
                    return Ok(Self::Splice {
                        paren_token: Paren::default(),
                        expr,
                    });
                }
            }

            // Try to parse as AttributeName first (supports kebab-case like space-evenly)
            if let Ok(attr_name) = input.parse::<AttributeName>() {
                let name_str = attr_name.to_string();

                // Check for identifier-based unit patterns (approach 3): vw50, px16, em1
                if let Some(numeric_lit) = NumericLit::try_parse_identifier_unit(&name_str) {
                    return Ok(Self::NumericLit(numeric_lit));
                }

                // Handle as regular string identifier for enums
                let expr: Expr = parse_quote!(#name_str);
                Ok(Self::Splice {
                    paren_token: Paren::default(),
                    expr,
                })
            } else {
                // Fallback to simple identifier
                let ident: Ident = input.call(Ident::parse_any)?;
                let ident_str = ident.to_string();

                // Check for identifier-based unit patterns (approach 3): vw50, px16, em1
                if let Some(numeric_lit) = NumericLit::try_parse_identifier_unit(&ident_str) {
                    return Ok(Self::NumericLit(numeric_lit));
                }

                // Handle as regular identifier
                let expr: Expr = parse_quote!(#ident);
                Ok(Self::Splice {
                    paren_token: Paren::default(),
                    expr,
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

        if let Self::ControlFlow(ControlFlow {
            kind: ControlFlowKind::Let(_),
            ..
        }) = &markup
        {
            diagnostics.push(
                markup
                    .span()
                    .error("`@let` bindings are only allowed inside blocks"),
            )
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
        match *self {}
    }
}

#[derive(Debug, Clone)]
pub struct ContainerElement {
    pub name: Option<ElementName>,
    pub attrs: Vec<ContainerAttribute>,
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
            Some(ContainerElement::diagnostic_parse)
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

// Re-export as Element for compatibility
pub type Element = ContainerElement;

#[derive(Debug, Clone)]
pub enum ElementBody {
    Void(Semi),
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

#[derive(Debug, Clone)]
pub struct Block<E> {
    pub brace_token: Brace,
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

#[derive(Debug, Clone)]
pub enum ContainerAttribute {
    Class {
        dot_token: Dot,
        name: ContainerNameOrMarkup,
        toggler: Option<Toggler>,
    },
    Id {
        pound_token: Pound,
        name: ContainerNameOrMarkup,
    },
    Named {
        name: AttributeName,
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

            let fork = input.fork();

            let attr = Self::Named {
                name: name.clone(),
                attr_type: input.diagnostic_parse(diagnostics)?,
            };

            if fork.peek(Eq) && fork.peek2(LitBool) {
                diagnostics.push(
                    attr.span()
                        .error("attribute value must be a string")
                        .help(format!("to declare an empty attribute, omit the equals sign: `{name}`"))
                        .help(format!("to toggle the attribute, use square brackets: `{name}[some_boolean_flag]`"))
                );
            }

            Ok(attr)
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

#[derive(Debug, Clone)]
pub enum ContainerNameOrMarkup {
    Name(AttributeName),
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

#[derive(Debug, Clone)]
pub enum AttributeType {
    Normal {
        eq_token: Eq,
        value: Markup<NoElement>,
    },
    Optional {
        eq_token: Eq,
        toggler: Toggler,
    },
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

#[derive(Debug, Clone)]
pub struct ElementName {
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

#[derive(Debug, Clone)]
pub struct AttributeName {
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

#[derive(Debug, Clone)]
pub enum AttributeNameFragment {
    Ident(Ident),
    LitInt(LitInt),
    LitStr(LitStr),
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

#[derive(Debug, Clone)]
pub struct ContainerLit {
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
                Lit::Str(_) => Ok(Self { lit }),
                Lit::Int(_) => Ok(Self { lit }),
                Lit::Float(_) => Ok(Self { lit }),
                Lit::Char(lit_char) => {
                    diagnostics.push(lit_char.span().error(format!(
                        r#"literal must be double-quoted: `"{}"`"#,
                        lit_char.value()
                    )));
                    Ok(Self {
                        lit: Lit::Str(LitStr::new("", lit_char.span())),
                    })
                }
                Lit::Bool(_) => {
                    // diagnostic handled earlier with more information
                    Ok(Self {
                        lit: Lit::Str(LitStr::new("", lit.span())),
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

#[derive(Debug, Clone)]
pub struct NumericLit {
    pub value: String,
    pub number_type: NumericType,
}

#[derive(Debug, Clone)]
pub enum NumericType {
    Integer,
    Real,
    IntegerPercent,
    RealPercent,
    IntegerVw,
    RealVw,
    IntegerVh,
    RealVh,
    IntegerDvw,
    RealDvw,
    IntegerDvh,
    RealDvh,
}

impl NumericLit {
    fn try_parse(input: &str) -> Option<Self> {
        // Check for percentage values
        if let Some(num_str) = input.strip_suffix('%') {
            if num_str.parse::<f32>().is_ok() {
                return Some(NumericLit {
                    value: input.to_string(),
                    number_type: if num_str.contains('.') {
                        NumericType::RealPercent
                    } else {
                        NumericType::IntegerPercent
                    },
                });
            }
        }

        // Check for viewport units (vw, vh, dvw, dvh)
        if let Some(num_str) = input.strip_suffix("vw") {
            if num_str.parse::<f32>().is_ok() {
                return Some(NumericLit {
                    value: input.to_string(),
                    number_type: if num_str.contains('.') {
                        NumericType::RealVw
                    } else {
                        NumericType::IntegerVw
                    },
                });
            }
        }

        if let Some(num_str) = input.strip_suffix("vh") {
            if num_str.parse::<f32>().is_ok() {
                return Some(NumericLit {
                    value: input.to_string(),
                    number_type: if num_str.contains('.') {
                        NumericType::RealVh
                    } else {
                        NumericType::IntegerVh
                    },
                });
            }
        }

        if let Some(num_str) = input.strip_suffix("dvw") {
            if num_str.parse::<f32>().is_ok() {
                return Some(NumericLit {
                    value: input.to_string(),
                    number_type: if num_str.contains('.') {
                        NumericType::RealDvw
                    } else {
                        NumericType::IntegerDvw
                    },
                });
            }
        }

        if let Some(num_str) = input.strip_suffix("dvh") {
            if num_str.parse::<f32>().is_ok() {
                return Some(NumericLit {
                    value: input.to_string(),
                    number_type: if num_str.contains('.') {
                        NumericType::RealDvh
                    } else {
                        NumericType::IntegerDvh
                    },
                });
            }
        }

        // Check for plain numbers (integer or float)
        if input.parse::<i64>().is_ok() {
            return Some(NumericLit {
                value: input.to_string(),
                number_type: NumericType::Integer,
            });
        }

        if input.parse::<f64>().is_ok() {
            return Some(NumericLit {
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
            if let Some(number_part) = input.strip_prefix(unit) {
                if !number_part.is_empty()
                    && number_part.chars().all(|c| c.is_ascii_digit() || c == '.')
                {
                    // Construct the unit+number string
                    let full_unit_string = format!("{}{}", number_part, unit);

                    // Try to parse using the existing logic
                    if let Some(numeric_lit) = Self::try_parse(&full_unit_string) {
                        return Some(numeric_lit);
                    }
                }
            }
        }

        None
    }

    /// Check if an identifier is a CSS unit that supports function syntax
    fn is_unit_identifier(input: &str) -> bool {
        matches!(
            input,
            "vw" | "vh" | "dvw" | "dvh" | "calc" | "min" | "max" | "clamp" | "percent" // Viewport units, calc function, CSS math functions, and helper functions
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

#[derive(Debug, Clone)]
pub enum AttributeNamePunct {
    Colon(Colon),
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

#[derive(Debug, Clone)]
pub struct Toggler {
    pub bracket_token: Bracket,
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

#[derive(Debug, Clone)]
pub struct ControlFlow<E> {
    pub at_token: At,
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

                    ControlFlowKind::Let(local)
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

#[derive(Debug, Clone)]
pub enum ControlFlowKind<E> {
    Let(Local),
    If(IfExpr<E>),
    For(ForExpr<E>),
    While(WhileExpr<E>),
    Match(MatchExpr<E>),
}

#[derive(Debug, Clone)]
pub struct IfExpr<E> {
    pub if_token: If,
    pub cond: IfCondition,
    pub then_branch: Block<E>,
    pub else_branch: Option<(At, Else, Box<IfOrBlock<E>>)>,
}

#[derive(Debug, Clone)]
pub enum IfCondition {
    Expr(Expr),
    Let {
        let_token: Let,
        pat: Pat,
        eq_token: syn::token::Eq,
        expr: Expr,
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
                let_token,
                pat,
                eq_token,
                expr,
            }
        } else {
            // Regular if condition
            IfCondition::Expr(input.call(Expr::parse_without_eager_brace)?)
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

#[derive(Debug, Clone)]
pub enum IfOrBlock<E> {
    If(IfExpr<E>),
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

#[derive(Debug, Clone)]
pub struct ForExpr<E> {
    pub for_token: For,
    pub pat: Pat,
    pub in_token: In,
    pub expr: Expr,
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

#[derive(Debug, Clone)]
pub struct WhileExpr<E> {
    pub while_token: While,
    pub cond: Expr,
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

#[derive(Debug, Clone)]
pub struct MatchExpr<E> {
    pub match_token: Match,
    pub expr: Expr,
    pub brace_token: Brace,
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

#[derive(Debug, Clone)]
pub struct MatchArm<E> {
    pub pat: Pat,
    pub guard: Option<(If, Expr)>,
    pub fat_arrow_token: FatArrow,
    pub body: Markup<E>,
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

pub trait DiagnosticParse: Sized {
    fn diagnostic_parse(input: ParseStream, diagnostics: &mut Vec<Diagnostic>)
    -> syn::Result<Self>;
}

impl<T: DiagnosticParse> DiagnosticParse for Box<T> {
    fn diagnostic_parse(
        input: ParseStream,
        diagnostics: &mut Vec<Diagnostic>,
    ) -> syn::Result<Self> {
        Ok(Box::new(input.diagnostic_parse(diagnostics)?))
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
