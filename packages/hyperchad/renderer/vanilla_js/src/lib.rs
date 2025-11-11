//! Vanilla JavaScript renderer for the `HyperChad` framework.
//!
//! This crate provides an HTML renderer that generates interactive web applications using
//! vanilla JavaScript (no framework dependencies). It produces HTML with custom attributes
//! (e.g., `v-onclick`, `hx-get`) that are processed by the embedded `HyperChad` JavaScript
//! runtime to enable reactive behavior, DOM manipulation, and HTTP requests.
//!
//! # Main Components
//!
//! * [`VanillaJsTagRenderer`] - Converts `HyperChad` containers to HTML with JavaScript event handlers
//! * [`VanillaJsRenderer`] - Extends HTML rendering with server-sent events and canvas support
//! * `SCRIPT` - The embedded JavaScript runtime (available with `script` feature)
//! * [`SCRIPT_NAME`] - Filename for the JavaScript runtime file
//!
//! # Features
//!
//! * **Interactive elements** - Click, hover, resize, and keyboard event handlers
//! * **HTTP requests** - Support for GET, POST, PUT, DELETE, PATCH with htmx-style attributes
//! * **Server-sent events** - Real-time updates via SSE (requires `plugin-sse`)
//! * **Canvas rendering** - Dynamic canvas updates (requires `plugin-canvas`)
//! * **Content hashing** - Cache-busting filenames (requires `hash` feature)
//! * **Plugin system** - Modular features for routing, forms, UUID generation, etc.
//!
//! # Example
//!
//! ```rust
//! use hyperchad_renderer_vanilla_js::VanillaJsTagRenderer;
//! use hyperchad_renderer::HtmlTagRenderer;
//!
//! let renderer = VanillaJsTagRenderer::default();
//! // Use renderer to convert HyperChad containers to HTML
//! ```

#![cfg_attr(feature = "fail-on-warnings", deny(warnings))]
#![warn(clippy::all, clippy::pedantic, clippy::nursery, clippy::cargo)]
#![allow(clippy::multiple_crate_versions)]

use std::{collections::BTreeMap, io::Write, sync::LazyLock};

use async_trait::async_trait;
use const_format::concatcp;
use hyperchad_renderer::{Color, HtmlTagRenderer, RendererEvent, View, canvas};
use hyperchad_renderer_html::{
    DefaultHtmlTagRenderer,
    extend::{ExtendHtmlRenderer, HtmlRendererEventPub},
    html::write_attr,
};
use hyperchad_transformer::{
    Container, ResponsiveTrigger,
    actions::{
        ActionEffect, ActionTrigger, ActionType, ElementTarget, InputActionType, LogLevel,
        StyleAction, Target,
        dsl::{BinaryOp, Expression, Literal, UnaryOp},
        logic::{Arithmetic, CalcValue, Condition, Value},
    },
    models::{LayoutDirection, Route, Visibility},
};
use maud::{DOCTYPE, PreEscaped, html};

static INSECURE_WARNING: LazyLock<()> = LazyLock::new(|| {
    #[cfg(all(not(debug_assertions), feature = "plugin-uuid-insecure"))]
    log::warn!(
        "The `plugin-uuid-insecure` feature is enabled. If this is a production service make sure to disable that feature and just use `plugin-uuid`"
    );
});

/// HTML tag renderer that generates vanilla JavaScript for hyperchad framework.
///
/// This renderer produces HTML with custom attributes (e.g., `v-onclick`, `hx-get`)
/// that are processed by the hyperchad JavaScript runtime to enable interactive behavior.
#[derive(Debug, Default, Clone)]
pub struct VanillaJsTagRenderer {
    default: DefaultHtmlTagRenderer,
}

const SCRIPT_NAME_STEM: &str = "hyperchad";
#[cfg(debug_assertions)]
const SCRIPT_NAME_EXTENSION: &str = "js";
#[cfg(not(debug_assertions))]
const SCRIPT_NAME_EXTENSION: &str = "min.js";

/// The filename of the hyperchad JavaScript runtime.
///
/// In debug builds, this is `"hyperchad.js"`. In release builds, this is `"hyperchad.min.js"`.
pub const SCRIPT_NAME: &str = concatcp!(SCRIPT_NAME_STEM, ".", SCRIPT_NAME_EXTENSION);

/// The embedded hyperchad JavaScript runtime source code.
///
/// This constant contains the full JavaScript source that implements the hyperchad
/// runtime for handling interactive elements, actions, and events. In debug builds,
/// this is the unminified source from `index.js`. In release builds, this is the
/// minified source from `index.min.js`.
///
/// Only available when the `script` feature is enabled.
#[cfg(all(debug_assertions, feature = "script"))]
pub const SCRIPT: &str = include_str!(concat!(
    env!("HYPERCHAD_VANILLA_JS_EMBED_SCRIPT_DIR"),
    "/index.js"
));

/// The embedded hyperchad JavaScript runtime source code.
///
/// This constant contains the full JavaScript source that implements the hyperchad
/// runtime for handling interactive elements, actions, and events. In debug builds,
/// this is the unminified source from `index.js`. In release builds, this is the
/// minified source from `index.min.js`.
///
/// Only available when the `script` feature is enabled.
#[cfg(all(not(debug_assertions), feature = "script"))]
pub const SCRIPT: &str = include_str!(concat!(
    env!("HYPERCHAD_VANILLA_JS_EMBED_SCRIPT_DIR"),
    "/index.min.js"
));

/// Content-hashed filename for the hyperchad JavaScript runtime.
///
/// This static computes an MD5 hash of the script content (including enabled plugin features)
/// and generates a filename like `"hyperchad-a1b2c3d4e5.js"` or `"hyperchad-a1b2c3d4e5.min.js"`.
/// The hash ensures browsers invalidate their cache when the script content changes.
///
/// Only available when both `hash` and `script` features are enabled.
#[cfg(all(feature = "hash", feature = "script"))]
pub static SCRIPT_NAME_HASHED: std::sync::LazyLock<String> = std::sync::LazyLock::new(|| {
    #[allow(unused_mut)]
    let mut bytes = SCRIPT.as_bytes().to_vec();

    #[cfg(feature = "plugin-nav")]
    bytes.extend(b"nav;");
    #[cfg(feature = "plugin-idiomorph")]
    bytes.extend(b"idiomorph;");
    #[cfg(feature = "plugin-sse")]
    bytes.extend(b"sse;");
    #[cfg(feature = "plugin-tauri-event")]
    bytes.extend(b"tauri-event;");
    #[cfg(all(not(feature = "plugin-uuid-insecure"), feature = "plugin-uuid"))]
    bytes.extend(b"uuid;");
    #[cfg(feature = "plugin-uuid-insecure")]
    bytes.extend(b"uuid-insecure;");
    #[cfg(feature = "plugin-routing")]
    bytes.extend(b"routing;");
    #[cfg(feature = "plugin-event")]
    bytes.extend(b"event;");
    #[cfg(feature = "plugin-canvas")]
    bytes.extend(b"canvas;");
    #[cfg(feature = "plugin-form")]
    bytes.extend(b"form;");
    #[cfg(feature = "plugin-actions-change")]
    bytes.extend(b"actions-change");
    #[cfg(feature = "plugin-actions-click")]
    bytes.extend(b"actions-click");
    #[cfg(feature = "plugin-actions-click-outside")]
    bytes.extend(b"actions-click-outside");
    #[cfg(feature = "plugin-actions-event")]
    bytes.extend(b"actions-event");
    #[cfg(feature = "plugin-actions-event-key-down")]
    bytes.extend(b"actions-event-key-down");
    #[cfg(feature = "plugin-actions-event-key-up")]
    bytes.extend(b"actions-event-key-up");
    #[cfg(feature = "plugin-actions-immediate")]
    bytes.extend(b"actions-immediate");
    #[cfg(feature = "plugin-actions-mouse-down")]
    bytes.extend(b"actions-mouse-down");
    #[cfg(feature = "plugin-actions-mouse-over")]
    bytes.extend(b"actions-mouse-over");
    #[cfg(feature = "plugin-actions-key-down")]
    bytes.extend(b"actions-key-down");
    #[cfg(feature = "plugin-actions-key-up")]
    bytes.extend(b"actions-key-up");
    #[cfg(feature = "plugin-actions-resize")]
    bytes.extend(b"actions-resize");
    #[cfg(feature = "plugin-http-events")]
    bytes.extend(b"http-events");

    let digest = md5::compute(&bytes);
    let digest = format!("{digest:x}");
    let hash = &digest[..10];
    format!("{SCRIPT_NAME_STEM}-{hash}.{SCRIPT_NAME_EXTENSION}")
});

fn arithmetic_to_js(value: &Arithmetic) -> String {
    match value {
        Arithmetic::Plus(a, b) => {
            format!("{}+{}", value_to_js(a, false).0, value_to_js(b, false).0)
        }
        Arithmetic::Minus(a, b) => {
            format!("{}-{}", value_to_js(a, false).0, value_to_js(b, false).0)
        }
        Arithmetic::Multiply(a, b) => {
            format!("{}*{}", value_to_js(a, false).0, value_to_js(b, false).0)
        }
        Arithmetic::Divide(a, b) => {
            format!("{}/{}", value_to_js(a, false).0, value_to_js(b, false).0)
        }
        Arithmetic::Min(a, b) => format!(
            "Math.min({},{})",
            value_to_js(a, false).0,
            value_to_js(b, false).0
        ),
        Arithmetic::Max(a, b) => format!(
            "Math.max({},{})",
            value_to_js(a, false).0,
            value_to_js(b, false).0
        ),
        Arithmetic::Grouping(x) => format!("({})", arithmetic_to_js(x)),
    }
}

fn calc_value_to_js(value: &CalcValue, serializable: bool) -> String {
    let target = match value {
        CalcValue::EventValue => {
            return if serializable {
                "{String:ctx.value}".to_string()
            } else {
                "ctx.value".to_string()
            };
        }
        CalcValue::Key { key } => return format!("'{key}'"),
        CalcValue::MouseX { target: None } => return "ctx.event.clientX".to_string(),
        CalcValue::MouseY { target: None } => return "ctx.event.clientY".to_string(),
        CalcValue::Visibility { target }
        | CalcValue::Display { target }
        | CalcValue::Id { target }
        | CalcValue::DataAttrValue { target, .. }
        | CalcValue::WidthPx { target }
        | CalcValue::HeightPx { target }
        | CalcValue::PositionX { target }
        | CalcValue::PositionY { target }
        | CalcValue::MouseX {
            target: Some(target),
        }
        | CalcValue::MouseY {
            target: Some(target),
        } => Some(element_target_to_js(target)),
    };

    target.map_or_else(
        || "null".to_string(),
        |target| match value {
            CalcValue::EventValue
            | CalcValue::Key { .. }
            | CalcValue::MouseX { target: None }
            | CalcValue::MouseY { target: None } => unreachable!(),
            CalcValue::Visibility { .. } => {
                format!("{target}[0]?.style.visibility")
            }
            CalcValue::Display { .. } => {
                format!("{target}[0]?.style.display")
            }
            CalcValue::Id { .. } => {
                format!("{target}[0]?.id")
            }
            CalcValue::DataAttrValue { attr, .. } => {
                use convert_case::{Case, Casing as _};
                let camel_case_attr = attr.to_case(Case::Camel);

                format!("{target}[0]?.dataset.{camel_case_attr}")
            }
            CalcValue::WidthPx { .. } => {
                format!("{target}[0]?.clientWidth")
            }
            CalcValue::HeightPx { .. } => {
                format!("{target}[0]?.clientHeight")
            }
            CalcValue::PositionX { .. } => {
                format!("{target}[0]?.getBoundingClientRect().left")
            }
            CalcValue::PositionY { .. } => {
                format!("{target}[0]?.getBoundingClientRect().top")
            }
            CalcValue::MouseX { .. } => {
                format!("(ctx.event.clientX-{target}[0]?.getBoundingClientRect().left)")
            }
            CalcValue::MouseY { .. } => {
                format!("(ctx.event.clientY-{target}[0]?.getBoundingClientRect().top)")
            }
        },
    )
}

fn value_to_js(value: &Value, serializable: bool) -> (String, bool) {
    match value {
        Value::Calc(calc_value) => (calc_value_to_js(calc_value, serializable), true),
        Value::Arithmetic(arithmetic) => (arithmetic_to_js(arithmetic), true),
        Value::Real(x) => (x.to_string(), true),
        Value::Visibility(visibility) => (
            match visibility {
                Visibility::Visible => "'visible'".to_string(),
                Visibility::Hidden => "'hidden'".to_string(),
            },
            true,
        ),
        Value::Display(display) => ("'none'".to_string(), !*display),
        Value::LayoutDirection(layout_direction) => (
            match layout_direction {
                LayoutDirection::Row => "'row'".to_string(),
                LayoutDirection::Column => "'column'".to_string(),
            },
            true,
        ),
        Value::Key(key) => (format!("'{key}'"), true),
        Value::String(x) => (
            if serializable {
                format!("{{String:'{x}'}}")
            } else {
                format!("'{x}'")
            },
            true,
        ),
    }
}

fn action_effect_to_js(effect: &ActionEffect) -> (String, Option<String>) {
    action_to_js(&effect.action, true)
}

fn action_effect_to_js_attr(effect: &ActionEffect) -> String {
    let (action, reset) = action_effect_to_js(effect);

    let reset = if let Some(delay) = effect.delay_off {
        reset.map(|x| format!("ctx.delay(()=>{{{x}}},{delay});"))
    } else {
        reset
    };

    let action = if let Some(throttle) = effect.throttle {
        format!("ctx.throttle(()=>{{{action}}},{throttle});")
    } else {
        action
    };

    format!(
        "{action}{}",
        reset.map_or_else(String::new, |reset| format!("`{reset}`"))
    )
}

fn element_target_to_js(target: &ElementTarget) -> String {
    #[allow(clippy::match_wildcard_for_single_variants)]
    match target {
        ElementTarget::StrId(id) => {
            match id {
                Target::Literal(id) => format!("[document.getElementById('{id}')]"),
                Target::Ref(ref_name) => format!("[{ref_name}]"),
            }
        }
        ElementTarget::Class(class) => {
            match class {
                Target::Literal(class) => format!("Array.from(document.querySelectorAll('.{class}'))"),
                Target::Ref(ref_name) => format!("[{ref_name}]"),
            }
        }
        ElementTarget::ChildClass(class) => {
            match class {
                Target::Literal(class) => format!("Array.from(ctx.element.querySelectorAll('.{class}'))"),
                Target::Ref(ref_name) => format!("[{ref_name}]"),
            }
        }
        ElementTarget::SelfTarget => "[ctx.element]".to_string(),
        ElementTarget::LastChild => {
            "(ctx.element.children.length>0?[ctx.element.children[ctx.element.children.length-1]]:[])"
                .to_string()
        }
        ElementTarget::Id(_) => unreachable!(),
    }
}

const fn binary_op_to_js(op: &BinaryOp) -> &'static str {
    match op {
        BinaryOp::Add => "+",
        BinaryOp::Subtract => "-",
        BinaryOp::Multiply => "*",
        BinaryOp::Divide => "/",
        BinaryOp::Modulo => "%",
        BinaryOp::Equal => "==",
        BinaryOp::NotEqual => "!=",
        BinaryOp::Less => "<",
        BinaryOp::LessEqual => "<=",
        BinaryOp::Greater => ">",
        BinaryOp::GreaterEqual => ">=",
        BinaryOp::And => "&&",
        BinaryOp::Or => "||",
        BinaryOp::BitAnd => "&",
        BinaryOp::BitOr => "|",
        BinaryOp::BitXor => "^",
    }
}

const fn unary_op_to_js(op: &UnaryOp) -> &'static str {
    match op {
        UnaryOp::Not => "!",
        UnaryOp::Minus => "-",
        UnaryOp::Plus => "+",
        UnaryOp::Ref => "&",
    }
}

#[allow(clippy::too_many_lines)]
fn expression_to_js(expr: &Expression) -> String {
    match expr {
        Expression::Literal(lit) => match lit {
            Literal::String(s) => format!("'{s}'"),
            Literal::Integer(i) => format!("{i}"),
            Literal::Float(f) => format!("{f}"),
            Literal::Bool(b) => format!("{b}"),
            Literal::Unit => "null".to_string(),
        },
        Expression::Variable(name) => name.clone(),
        Expression::ElementRef(element_ref) => match &**element_ref {
            Expression::Literal(Literal::String(selector)) => {
                let selector = selector.clone();
                format!("document.querySelector('{selector}')")
            }
            Expression::Variable(selector) => {
                let selector = selector.clone();
                format!("document.querySelector({selector})")
            }
            _ => unimplemented!(),
        },
        Expression::Call { function, args } => {
            let args = args.iter().map(expression_to_js).collect::<Vec<_>>();
            format!("{function}({})", args.join(","))
        }
        Expression::MethodCall {
            receiver,
            method,
            args,
        } => {
            let receiver = expression_to_js(receiver);
            let args = args.iter().map(expression_to_js).collect::<Vec<_>>();
            format!("{receiver}.{method}({})", args.join(","))
        }
        Expression::Field { object, field } => {
            let object = expression_to_js(object);
            format!("{object}.{field}")
        }
        Expression::Binary { left, op, right } => {
            let left = expression_to_js(left);
            let right = expression_to_js(right);
            let op = binary_op_to_js(op);
            format!("({left} {op} {right})")
        }
        Expression::Unary { op, expr } => {
            let expr = expression_to_js(expr);
            let op = unary_op_to_js(op);
            format!("({op} {expr})")
        }
        Expression::If {
            condition,
            then_branch,
            else_branch,
        } => {
            let condition = expression_to_js(condition);
            let then_branch = expression_to_js(then_branch);
            let else_branch = else_branch
                .as_ref()
                .map(|else_branch| expression_to_js(else_branch));
            format!(
                "if({condition}){{{then_branch}}}{else_branch}",
                else_branch = else_branch
                    .map(|x| format!("else {{{x}}}"))
                    .as_deref()
                    .unwrap_or("")
            )
        }
        Expression::Match { .. } => {
            unimplemented!("match expression")
        }
        Expression::Block(..) => {
            unimplemented!("block expression")
        }
        Expression::Array(..) => {
            unimplemented!("array expression")
        }
        Expression::Tuple(..) => {
            unimplemented!("tuple expression")
        }
        Expression::Range {
            start,
            end,
            inclusive,
        } => {
            let start = start
                .as_ref()
                .map_or_else(|| "0".to_string(), |start| expression_to_js(start));

            let end = end
                .as_ref()
                .map_or_else(|| "0".to_string(), |end| expression_to_js(end));

            format!(
                "ctx.range({start},{end},{inclusive})",
                start = start,
                end = end,
                inclusive = if *inclusive { "true" } else { "false" }
            )
        }
        Expression::Closure { .. } => {
            unimplemented!("closure expression")
        }
        Expression::Grouping(x) => {
            format!("({})", expression_to_js(x))
        }
        Expression::RawRust(code) => code.clone(),
    }
}

#[allow(clippy::too_many_lines)]
fn action_to_js(action: &ActionType, trigger_action: bool) -> (String, Option<String>) {
    match action {
        ActionType::NoOp => (String::new(), None),
        ActionType::Let { name, value } => {
            (format!("let {name}={};", expression_to_js(value)), None)
        }
        ActionType::Input(action) => match action {
            InputActionType::Select { target } => {
                let target = element_target_to_js(target);
                (format!("ctx.cf({target},'select');"), None)
            }
        },
        ActionType::Style { target, action } => {
            let target = element_target_to_js(target);

            match action {
                StyleAction::SetVisibility(visibility) => (
                    format!(
                        "ctx.ss({target},'visibility',{});",
                        match visibility {
                            Visibility::Visible => "'visible'",
                            Visibility::Hidden => "'hidden'",
                        },
                    ),
                    Some(format!("ctx.rs({target},'visibility');")),
                ),
                StyleAction::SetFocus(focus) => (
                    format!(
                        "ctx.cf({target},'{}');",
                        if *focus { "focus" } else { "blur" }
                    ),
                    None,
                ),
                StyleAction::SetDisplay(display) => (
                    if *display {
                        format!("ctx.ss({target},'display','initial');")
                    } else {
                        format!("ctx.ss({target},'display','none');")
                    },
                    Some(format!("ctx.rs({target},'display');")),
                ),
                StyleAction::SetBackground(background) => (
                    format!(
                        "ctx.ss({target},'background',{});",
                        background
                            .as_ref()
                            .map_or_else(|| "null".to_string(), |color| format!("'{color}'"))
                    ),
                    Some(format!("ctx.rs({target},'background');")),
                ),
            }
        }
        ActionType::Multi(vec) => {
            let actions = vec
                .iter()
                .map(|x| action_to_js(x, true))
                .collect::<Vec<_>>();
            let all_actions = actions
                .iter()
                .map(|(action, _)| action.as_str())
                .collect::<String>();
            let all_reset = actions
                .iter()
                .filter_map(|(_, reset)| reset.as_ref().map(String::as_str))
                .collect::<String>();

            (
                all_actions,
                if all_reset.is_empty() {
                    None
                } else {
                    Some(all_reset)
                },
            )
        }
        ActionType::MultiEffect(vec) => {
            let effects = vec.iter().map(action_effect_to_js).collect::<Vec<_>>();
            let all_effects = effects
                .iter()
                .map(|(effect, _)| effect.as_str())
                .collect::<String>();
            let all_reset = effects
                .iter()
                .filter_map(|(_, reset)| reset.as_ref().map(String::as_str))
                .collect::<String>();

            (
                all_effects,
                if all_reset.is_empty() {
                    None
                } else {
                    Some(all_reset)
                },
            )
        }
        ActionType::Event {
            name: _name,
            action,
        } => action_to_js(action, true),
        ActionType::Logic(logic) => {
            let expr = match &logic.condition {
                Condition::Eq(a, b) => {
                    let (a, a_eq) = value_to_js(a, false);
                    let (b, b_eq) = value_to_js(b, false);
                    format!("{}{}{}", a, if a_eq == b_eq { "===" } else { "!==" }, b)
                }
                Condition::Bool(b) => format!("{b}"),
            };
            let if_true = logic
                .actions
                .iter()
                .map(action_effect_to_js)
                .collect::<Vec<_>>();

            let true_reset = if_true
                .iter()
                .filter_map(|(_, reset)| reset.as_ref().map(String::as_str))
                .collect::<String>();

            let if_true = if_true
                .iter()
                .map(|(action, _)| action.as_str())
                .collect::<String>();

            let if_false = logic
                .else_actions
                .iter()
                .map(action_effect_to_js)
                .collect::<Vec<_>>();

            let false_reset = if_false
                .iter()
                .filter_map(|(_, reset)| reset.as_ref().map(String::as_str))
                .collect::<String>();

            let if_false = if_false
                .iter()
                .map(|(action, _)| action.as_str())
                .collect::<String>();

            (
                format!("if({expr}){{{if_true}}}else{{{if_false}}}"),
                if true_reset.is_empty() && false_reset.is_empty() {
                    None
                } else {
                    Some(String::new())
                },
            )
        }
        ActionType::Parameterized { action, value } => {
            let (action, reset) = action_to_js(action, false);

            let action = action
                .strip_prefix("{action:")
                .and_then(|x| x.strip_suffix("}"))
                .unwrap_or(action.as_str());

            let action = html_escape::encode_double_quoted_attribute(&action)
                .to_string()
                .replace('\n', "&#10;");

            let action = format!("{{action:{action},value:{}}}", value_to_js(value, true).0);

            let action = if trigger_action {
                format!("triggerAction({action});")
            } else {
                action
            };

            (action, reset)
        }
        ActionType::Custom { action } => {
            let action = html_escape::encode_double_quoted_attribute(&action)
                .to_string()
                .replace('\n', "&#10;");

            let action = format!("{{action:{action}}}");

            let action = if trigger_action {
                format!("triggerAction({action});")
            } else {
                action
            };

            (action, None)
        }
        ActionType::Log { message, level } => (
            format!(
                "console.{}(`{}`);",
                match level {
                    LogLevel::Error => "error",
                    LogLevel::Warn => "warn",
                    LogLevel::Info => "log",
                    LogLevel::Debug => "debug",
                    LogLevel::Trace => "trace",
                },
                message.replace('"', "&quot;")
            ),
            None,
        ),
        ActionType::Navigate { url } => (format!("navigate(`{url}`);"), None),
    }
}

impl HtmlTagRenderer for VanillaJsTagRenderer {
    /// Registers a responsive trigger for conditional CSS rendering.
    ///
    /// Adds a named responsive trigger (e.g., for media queries or container queries) that can
    /// be referenced by containers to apply conditional styling based on viewport or element size.
    fn add_responsive_trigger(&mut self, name: String, trigger: ResponsiveTrigger) {
        self.default.responsive_triggers.insert(name, trigger);
    }

    /// Renders HTML attributes for a container element.
    ///
    /// Generates HTML attributes including standard styling, interactive event handlers
    /// (e.g., `v-onclick`, `v-onmouseover`), and htmx HTTP attributes (e.g., `hx-get`,
    /// `hx-post`). This method extends the default HTML renderer with vanilla JavaScript
    /// interactivity features.
    ///
    /// # Errors
    ///
    /// Returns an error if writing to the output stream fails.
    #[allow(clippy::too_many_lines)]
    fn element_attrs_to_html(
        &self,
        f: &mut dyn Write,
        container: &Container,
        is_flex_child: bool,
    ) -> Result<(), std::io::Error> {
        self.default
            .element_attrs_to_html(f, container, is_flex_child)?;

        if let Some(route) = &container.route {
            match route {
                Route::Get {
                    route: path,
                    trigger,
                    target,
                    strategy,
                }
                | Route::Post {
                    route: path,
                    trigger,
                    target,
                    strategy,
                }
                | Route::Put {
                    route: path,
                    trigger,
                    target,
                    strategy,
                }
                | Route::Delete {
                    route: path,
                    trigger,
                    target,
                    strategy,
                }
                | Route::Patch {
                    route: path,
                    trigger,
                    target,
                    strategy,
                } => {
                    // Output hx-target (WHERE) if not SelfTarget
                    match target {
                        hyperchad_transformer::models::Selector::SelfTarget => {
                            // No hx-target attribute needed
                        }
                        target => {
                            write_attr(f, b"hx-target", target.to_string().as_bytes())?;
                        }
                    }

                    // Always output hx-swap (HOW)
                    write_attr(f, b"hx-swap", strategy.to_string().as_bytes())?;

                    // Output HTTP method
                    match route {
                        Route::Get { .. } => {
                            write_attr(f, b"hx-get", path.as_bytes())?;
                        }
                        Route::Post { .. } => {
                            write_attr(f, b"hx-post", path.as_bytes())?;
                        }
                        Route::Put { .. } => {
                            write_attr(f, b"hx-put", path.as_bytes())?;
                        }
                        Route::Delete { .. } => {
                            write_attr(f, b"hx-delete", path.as_bytes())?;
                        }
                        Route::Patch { .. } => {
                            write_attr(f, b"hx-patch", path.as_bytes())?;
                        }
                    }
                    if let Some(trigger) = trigger {
                        write_attr(f, b"hx-trigger", trigger.as_bytes())?;
                    }
                }
            }
        }

        for action in &container.actions {
            match &action.trigger {
                ActionTrigger::Click => {
                    write_attr(
                        f,
                        b"v-onclick",
                        action_effect_to_js_attr(&action.effect).as_bytes(),
                    )?;
                }
                ActionTrigger::ClickOutside => {
                    write_attr(
                        f,
                        b"v-onclickoutside",
                        action_effect_to_js_attr(&action.effect).as_bytes(),
                    )?;
                }
                ActionTrigger::MouseDown => {
                    write_attr(
                        f,
                        b"v-onmousedown",
                        action_effect_to_js_attr(&action.effect).as_bytes(),
                    )?;
                }
                ActionTrigger::Hover => {
                    write_attr(
                        f,
                        b"v-onmouseover",
                        action_effect_to_js_attr(&action.effect).as_bytes(),
                    )?;
                }
                ActionTrigger::Change => {
                    write_attr(
                        f,
                        b"v-onchange",
                        action_effect_to_js_attr(&action.effect).as_bytes(),
                    )?;
                }
                ActionTrigger::Resize => {
                    write_attr(
                        f,
                        b"v-onresize",
                        action_effect_to_js_attr(&action.effect).as_bytes(),
                    )?;
                }
                ActionTrigger::Event(name) => {
                    write_attr(
                        f,
                        b"v-onevent",
                        format!("{name}:{}", action_effect_to_js_attr(&action.effect)).as_bytes(),
                    )?;
                }
                ActionTrigger::KeyDown => {
                    write_attr(
                        f,
                        b"v-onkeydown",
                        action_effect_to_js_attr(&action.effect).as_bytes(),
                    )?;
                }
                ActionTrigger::Immediate => {
                    write_attr(
                        f,
                        b"v-onload",
                        action_effect_to_js_attr(&action.effect).as_bytes(),
                    )?;
                }
                ActionTrigger::HttpBeforeRequest => {
                    write_attr(
                        f,
                        b"v-http-before-request",
                        action_effect_to_js_attr(&action.effect).as_bytes(),
                    )?;
                }
                ActionTrigger::HttpAfterRequest => {
                    write_attr(
                        f,
                        b"v-http-after-request",
                        action_effect_to_js_attr(&action.effect).as_bytes(),
                    )?;
                }
                ActionTrigger::HttpRequestSuccess => {
                    write_attr(
                        f,
                        b"v-http-success",
                        action_effect_to_js_attr(&action.effect).as_bytes(),
                    )?;
                }
                ActionTrigger::HttpRequestError => {
                    write_attr(
                        f,
                        b"v-http-error",
                        action_effect_to_js_attr(&action.effect).as_bytes(),
                    )?;
                }
                ActionTrigger::HttpRequestAbort => {
                    write_attr(
                        f,
                        b"v-http-abort",
                        action_effect_to_js_attr(&action.effect).as_bytes(),
                    )?;
                }
                ActionTrigger::HttpRequestTimeout => {
                    write_attr(
                        f,
                        b"v-http-timeout",
                        action_effect_to_js_attr(&action.effect).as_bytes(),
                    )?;
                }
            }
        }

        Ok(())
    }

    /// Generates partial HTML for a container with responsive CSS.
    ///
    /// Converts the container's reactive conditions to CSS and prepends them to the content.
    /// This is typically used for AJAX/htmx responses that update part of the page.
    ///
    /// # Panics
    ///
    /// Panics if:
    /// * Converting reactive conditions to CSS fails (should never occur with valid containers)
    /// * The generated CSS bytes are not valid UTF-8 (should never occur as CSS is ASCII)
    fn partial_html(
        &self,
        _headers: &BTreeMap<String, String>,
        container: &Container,
        content: String,
        _viewport: Option<&str>,
        _background: Option<Color>,
    ) -> String {
        let mut responsive_css = vec![];
        self.default
            .reactive_conditions_to_css(&mut responsive_css, container)
            .unwrap();
        let responsive_css = std::str::from_utf8(&responsive_css).unwrap();

        format!("{responsive_css}\n\n{content}")
    }

    /// Generates complete HTML document with head, body, and hyperchad JavaScript runtime.
    ///
    /// Creates a full HTML page including the document structure, metadata (title, description),
    /// stylesheets, the hyperchad JavaScript runtime, and responsive CSS. This is used for
    /// initial page loads or standalone HTML responses.
    ///
    /// # Panics
    ///
    /// Panics if:
    /// * Converting reactive conditions to CSS fails (should never occur with valid containers)
    /// * The generated CSS bytes are not valid UTF-8 (should never occur as CSS is ASCII)
    fn root_html(
        &self,
        _headers: &BTreeMap<String, String>,
        container: &Container,
        content: String,
        viewport: Option<&str>,
        background: Option<Color>,
        title: Option<&str>,
        description: Option<&str>,
        css_urls: &[String],
        css_paths: &[String],
        inline_css: &[String],
    ) -> String {
        let mut responsive_css = vec![];
        self.default
            .reactive_conditions_to_css(&mut responsive_css, container)
            .unwrap();
        let responsive_css = std::str::from_utf8(&responsive_css).unwrap();

        let background = background.map(|x| format!("background:rgb({},{},{})", x.r, x.g, x.b));
        let background = background.as_deref().unwrap_or("");

        #[cfg(all(feature = "hash", feature = "script"))]
        let script = html! { script src={"/js/"(SCRIPT_NAME_HASHED.as_str())} {} };
        #[cfg(not(all(feature = "hash", feature = "script")))]
        let script = html! { script src={"/js/"(SCRIPT_NAME)} {} };

        html! {
            (DOCTYPE)
            html style="height:100%" lang="en" {
                head {
                    @if let Some(title) = title {
                        title { (title) }
                    }
                    @if let Some(description) = description {
                        meta name="description" content=(description);
                    }
                    @for url in css_urls {
                        link rel="stylesheet" href=(url);
                    }
                    @for path in css_paths {
                        link rel="stylesheet" href=(path);
                    }
                    style {(format!(r"
                        body {{
                            margin: 0;{background};
                            overflow: hidden;
                        }}
                        .remove-button-styles {{
                            background: none;
                            color: inherit;
                            border: none;
                            padding: 0;
                            font: inherit;
                            cursor: pointer;
                            outline: inherit;
                        }}
                        table.remove-table-styles {{
                            border-collapse: collapse;
                        }}
                        table.remove-table-styles td {{
                            padding: 0;
                        }}
                    "))}
                    (script)
                    (PreEscaped(responsive_css))
                    @for css in inline_css {
                        style {(PreEscaped(css))}
                    }
                    @if let Some(content) = viewport {
                        meta name="viewport" content=(content);
                    }
                }
                body style="height:100%;overflow:auto;" {
                    (PreEscaped(content))
                }
            }
        }
        .into_string()
    }
}

/// Renderer implementation that extends HTML rendering with vanilla JavaScript capabilities.
///
/// This renderer implements the `ExtendHtmlRenderer` trait to enable server-sent events,
/// view updates, and canvas rendering in hyperchad applications using vanilla JavaScript.
pub struct VanillaJsRenderer {}

#[async_trait]
impl ExtendHtmlRenderer for VanillaJsRenderer {
    /// Emits a custom event through the renderer's event publisher.
    ///
    /// Publishes a [`RendererEvent::Event`] with the specified event name and optional value
    /// to all subscribed listeners via server-sent events or other transport mechanisms.
    ///
    /// # Errors
    ///
    /// Returns an error if publishing the event fails (e.g., channel closed or disconnected).
    async fn emit_event(
        &self,
        publisher: HtmlRendererEventPub,
        event_name: String,
        event_value: Option<String>,
    ) -> Result<(), Box<dyn std::error::Error + Send + 'static>> {
        let () = *INSECURE_WARNING;

        publisher
            .publish(RendererEvent::Event {
                name: event_name,
                value: event_value,
            })
            .map_err(|e| Box::new(e) as Box<dyn std::error::Error + Send>)?;
        Ok(())
    }

    /// Renders and publishes a view update through the renderer's event publisher.
    ///
    /// Converts the provided [`View`] into a [`RendererEvent::View`] and publishes it to all
    /// subscribed clients, enabling dynamic UI updates without full page reloads.
    ///
    /// # Errors
    ///
    /// Returns an error if publishing the view fails (e.g., channel closed or disconnected).
    async fn render(
        &self,
        publisher: HtmlRendererEventPub,
        view: View,
    ) -> Result<(), Box<dyn std::error::Error + Send + 'static>> {
        let () = *INSECURE_WARNING;

        publisher
            .publish(RendererEvent::View(Box::new(view)))
            .map_err(|e| Box::new(e) as Box<dyn std::error::Error + Send>)?;
        Ok(())
    }

    /// Renders and publishes a canvas update through the renderer's event publisher.
    ///
    /// Converts the provided [`canvas::CanvasUpdate`] into a [`RendererEvent::CanvasUpdate`]
    /// and publishes it to subscribed clients, enabling real-time canvas rendering updates.
    ///
    /// # Errors
    ///
    /// Returns an error if publishing the canvas update fails (e.g., channel closed or disconnected).
    async fn render_canvas(
        &self,
        publisher: HtmlRendererEventPub,
        update: canvas::CanvasUpdate,
    ) -> Result<(), Box<dyn std::error::Error + Send + 'static>> {
        let () = *INSECURE_WARNING;

        publisher
            .publish(RendererEvent::CanvasUpdate(update))
            .map_err(|e| Box::new(e) as Box<dyn std::error::Error + Send>)?;
        Ok(())
    }
}
