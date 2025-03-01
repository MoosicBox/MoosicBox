#![cfg_attr(feature = "fail-on-warnings", deny(warnings))]
#![warn(clippy::all, clippy::pedantic, clippy::nursery, clippy::cargo)]

use std::{collections::HashMap, io::Write};

use async_trait::async_trait;
use const_format::concatcp;
use hyperchad_renderer::{canvas, Color, HtmlTagRenderer, PartialView, RendererEvent, View};
use hyperchad_renderer_html::{
    extend::{ExtendHtmlRenderer, HtmlRendererEventPub},
    html::write_attr,
    DefaultHtmlTagRenderer,
};
use hyperchad_transformer::{
    actions::{
        logic::{Arithmetic, CalcValue, Condition, Value},
        ActionEffect, ActionTrigger, ActionType, ElementTarget, LogLevel, StyleAction,
    },
    models::{LayoutDirection, Route, Visibility},
    Container, ResponsiveTrigger,
};
use maud::{html, PreEscaped, DOCTYPE};

#[derive(Default, Clone)]
pub struct VanillaJsTagRenderer {
    default: DefaultHtmlTagRenderer,
}

const SCRIPT_NAME_STEM: &str = "hyperchad";
#[cfg(debug_assertions)]
const SCRIPT_NAME_EXTENSION: &str = "js";
#[cfg(not(debug_assertions))]
const SCRIPT_NAME_EXTENSION: &str = "min.js";

pub const SCRIPT_NAME: &str = concatcp!(SCRIPT_NAME_STEM, ".", SCRIPT_NAME_EXTENSION);

#[cfg(all(debug_assertions, feature = "script"))]
pub const SCRIPT: &str = include_str!("../web/dist/index.js");

#[cfg(all(not(debug_assertions), feature = "script"))]
pub const SCRIPT: &str = include_str!("../web/dist/index.min.js");

#[cfg(all(feature = "hash", feature = "script"))]
pub static SCRIPT_NAME_HASHED: std::sync::LazyLock<String> = std::sync::LazyLock::new(|| {
    #[allow(unused_mut)]
    let mut bytes = SCRIPT.as_bytes().to_vec();

    #[cfg(feature = "plugin-nav")]
    bytes.extend(b"nav;");
    #[cfg(feature = "plugin-sse")]
    bytes.extend(b"sse;");
    #[cfg(feature = "plugin-routing")]
    bytes.extend(b"routing;");
    #[cfg(feature = "plugin-event")]
    bytes.extend(b"event;");
    #[cfg(feature = "plugin-canvas")]
    bytes.extend(b"canvas;");
    #[cfg(feature = "plugin-actions-change")]
    bytes.extend(b"actions-change");
    #[cfg(feature = "plugin-actions-click")]
    bytes.extend(b"actions-click");
    #[cfg(feature = "plugin-actions-click-outside")]
    bytes.extend(b"actions-click-outside");
    #[cfg(feature = "plugin-actions-event")]
    bytes.extend(b"actions-event");
    #[cfg(feature = "plugin-actions-immediate")]
    bytes.extend(b"actions-immediate");
    #[cfg(feature = "plugin-actions-mouse-down")]
    bytes.extend(b"actions-mouse-down");
    #[cfg(feature = "plugin-actions-mouse-over")]
    bytes.extend(b"actions-mouse-over");
    #[cfg(feature = "plugin-actions-resize")]
    bytes.extend(b"actions-resize");

    let digest = md5::compute(&bytes);
    let digest = format!("{digest:x}");
    let hash = &digest[..10];
    format!("{SCRIPT_NAME_STEM}-{hash}.{SCRIPT_NAME_EXTENSION}")
});

fn arithmetic_to_js(value: &Arithmetic) -> String {
    match value {
        Arithmetic::Plus(a, b) => format!("{}+{}", value_to_js(a, false), value_to_js(b, false)),
        Arithmetic::Minus(a, b) => format!("{}-{}", value_to_js(a, false), value_to_js(b, false)),
        Arithmetic::Multiply(a, b) => {
            format!("{}*{}", value_to_js(a, false), value_to_js(b, false))
        }
        Arithmetic::Divide(a, b) => format!("{}/{}", value_to_js(a, false), value_to_js(b, false)),
        Arithmetic::Min(a, b) => format!(
            "Math.min({},{})",
            value_to_js(a, false),
            value_to_js(b, false)
        ),
        Arithmetic::Max(a, b) => format!(
            "Math.max({},{})",
            value_to_js(a, false),
            value_to_js(b, false)
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
        CalcValue::MouseX { target: None } => return "ctx.event.clientX".to_string(),
        CalcValue::MouseY { target: None } => return "ctx.event.clientY".to_string(),
        CalcValue::Visibility { target }
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
            | CalcValue::MouseX { target: None }
            | CalcValue::MouseY { target: None } => unreachable!(),
            CalcValue::Visibility { .. } => {
                format!("{target}[0]?.style.visibility")
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

fn value_to_js(value: &Value, serializable: bool) -> String {
    match value {
        Value::Calc(calc_value) => calc_value_to_js(calc_value, serializable),
        Value::Arithmetic(arithmetic) => arithmetic_to_js(arithmetic),
        Value::Real(x) => x.to_string(),
        Value::Visibility(visibility) => match visibility {
            Visibility::Visible => "'visible'".to_string(),
            Visibility::Hidden => "'hidden'".to_string(),
        },
        Value::LayoutDirection(layout_direction) => match layout_direction {
            LayoutDirection::Row => "'row'".to_string(),
            LayoutDirection::Column => "'column'".to_string(),
        },
        Value::String(x) => {
            if serializable {
                format!("{{String:'{x}'}}")
            } else {
                format!("'{x}'")
            }
        }
    }
}

fn action_effect_to_js(effect: &ActionEffect) -> (String, Option<String>) {
    action_to_js(&effect.action)
}

fn action_effect_to_js_attr(effect: &ActionEffect) -> String {
    let (mut action, reset) = action_effect_to_js(effect);

    if matches!(
        &effect.action,
        ActionType::Custom { .. } | ActionType::Parameterized { .. }
    ) {
        action = format!("triggerAction({action})");
    }

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
            format!("[document.getElementById('{id}')]")
        }
        ElementTarget::ChildClass(class) => {
            format!("Array.from(ctx.element.querySelectorAll('.{class}'))")
        }
        ElementTarget::SelfTarget => "[ctx.element]".to_string(),
        ElementTarget::LastChild => {
            "(ctx.element.children.length>0?[ctx.element.children[ctx.element.children.length-1]]:[])"
                .to_string()
        }
        #[allow(unreachable_patterns)]
        _ => {
            unreachable!();
        }
    }
}

#[allow(clippy::too_many_lines)]
fn action_to_js(action: &ActionType) -> (String, Option<String>) {
    match action {
        ActionType::NoOp => (String::new(), None),
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
                StyleAction::SetDisplay(display) => (
                    format!(
                        "ctx.ss({target},'display',{});",
                        if *display { "'initial'" } else { "null" }
                    ),
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
            let actions = vec.iter().map(action_to_js).collect::<Vec<_>>();
            let all_actions = actions
                .iter()
                .map(|(action, _)| action.as_str())
                .collect::<Vec<_>>()
                .join("");
            let all_reset = actions
                .iter()
                .filter_map(|(_, reset)| reset.as_ref().map(String::as_str))
                .collect::<Vec<_>>()
                .join("");

            (
                all_actions,
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
        } => action_to_js(action),
        ActionType::Logic(logic) => {
            let expr = match &logic.condition {
                Condition::Eq(a, b) => {
                    format!("{}==={}", value_to_js(a, false), value_to_js(b, false))
                }
            };
            let if_true = logic
                .actions
                .iter()
                .map(|x| &x.action)
                .map(action_effect_to_js)
                .collect::<Vec<_>>();

            let true_reset = if_true
                .iter()
                .filter_map(|(_, reset)| reset.as_ref().map(String::as_str))
                .collect::<Vec<_>>()
                .join("");

            let if_true = if_true
                .iter()
                .map(|(action, _)| action.as_str())
                .collect::<Vec<_>>()
                .join("");

            let if_false = logic
                .else_actions
                .iter()
                .map(|x| &x.action)
                .map(action_effect_to_js)
                .collect::<Vec<_>>();

            let false_reset = if_false
                .iter()
                .filter_map(|(_, reset)| reset.as_ref().map(String::as_str))
                .collect::<Vec<_>>()
                .join("");

            let if_false = if_false
                .iter()
                .map(|(action, _)| action.as_str())
                .collect::<Vec<_>>()
                .join("");

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
            let (action, reset) = action_to_js(action);

            let action = action
                .strip_prefix("{action:")
                .and_then(|x| x.strip_suffix("}"))
                .unwrap_or(action.as_str());

            let action = html_escape::encode_double_quoted_attribute(&action)
                .to_string()
                .replace('\n', "&#10;");

            (
                format!("{{action:{action},value:{}}}", value_to_js(value, true)),
                reset,
            )
        }
        ActionType::Custom { action } => {
            let action = html_escape::encode_double_quoted_attribute(&action)
                .to_string()
                .replace('\n', "&#10;");

            (format!("{{action:{action}}}"), None)
        }
        ActionType::Log { message, level } => (
            format!(
                "console.{}(`{}`)",
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
        ActionType::Navigate { url } => (format!("navigate(`{url}`)"), None),
    }
}

impl HtmlTagRenderer for VanillaJsTagRenderer {
    fn add_responsive_trigger(&mut self, name: String, trigger: ResponsiveTrigger) {
        self.default.responsive_triggers.insert(name, trigger);
    }

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
                    route,
                    trigger,
                    swap,
                } => {
                    match swap {
                        hyperchad_transformer::models::SwapTarget::This => {
                            write_attr(f, b"hx-swap", b"outerHTML")?;
                        }
                        hyperchad_transformer::models::SwapTarget::Children => {
                            write_attr(f, b"hx-swap", b"innerHTML")?;
                        }
                    }
                    write_attr(f, b"hx-get", route.as_bytes())?;
                    if let Some(trigger) = trigger {
                        write_attr(f, b"hx-trigger", trigger.as_bytes())?;
                    }
                }
                Route::Post {
                    route,
                    trigger,
                    swap,
                } => {
                    match swap {
                        hyperchad_transformer::models::SwapTarget::This => {
                            write_attr(f, b"hx-swap", b"outerHTML")?;
                        }
                        hyperchad_transformer::models::SwapTarget::Children => {
                            write_attr(f, b"hx-swap", b"innerHTML")?;
                        }
                    }
                    write_attr(f, b"hx-swap", b"outerHTML")?;
                    write_attr(f, b"hx-post", route.as_bytes())?;
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
                        action_effect_to_js_attr(&action.action).as_bytes(),
                    )?;
                }
                ActionTrigger::ClickOutside => {
                    write_attr(
                        f,
                        b"v-onclickoutside",
                        action_effect_to_js_attr(&action.action).as_bytes(),
                    )?;
                }
                ActionTrigger::MouseDown => {
                    write_attr(
                        f,
                        b"v-onmousedown",
                        action_effect_to_js_attr(&action.action).as_bytes(),
                    )?;
                }
                ActionTrigger::Hover => {
                    write_attr(
                        f,
                        b"v-onmouseover",
                        action_effect_to_js_attr(&action.action).as_bytes(),
                    )?;
                }
                ActionTrigger::Change => {
                    write_attr(
                        f,
                        b"v-onchange",
                        action_effect_to_js_attr(&action.action).as_bytes(),
                    )?;
                }
                ActionTrigger::Resize => {
                    write_attr(
                        f,
                        b"v-onresize",
                        action_effect_to_js_attr(&action.action).as_bytes(),
                    )?;
                }
                ActionTrigger::Event(name) => {
                    write_attr(
                        f,
                        b"v-onevent",
                        format!("{name}:{}", action_effect_to_js_attr(&action.action)).as_bytes(),
                    )?;
                }
                ActionTrigger::Immediate => {
                    write_attr(
                        f,
                        b"v-onload",
                        action_effect_to_js_attr(&action.action).as_bytes(),
                    )?;
                }
            }
        }

        Ok(())
    }

    fn partial_html(
        &self,
        _headers: &HashMap<String, String>,
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

    fn root_html(
        &self,
        _headers: &HashMap<String, String>,
        container: &Container,
        content: String,
        viewport: Option<&str>,
        background: Option<Color>,
        title: Option<&str>,
        description: Option<&str>,
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
                    "))}
                    (script)
                    (PreEscaped(responsive_css))
                    @if let Some(content) = viewport {
                        meta name="viewport" content=(content);
                    }
                }
                body style="height:100%" {
                    (PreEscaped(content))
                }
            }
        }
        .into_string()
    }
}

pub struct VanillaJsRenderer {}

#[async_trait]
impl ExtendHtmlRenderer for VanillaJsRenderer {
    /// # Errors
    ///
    /// Will error if `VanillaJsRenderer` fails to emit the event.
    async fn emit_event(
        &self,
        publisher: HtmlRendererEventPub,
        event_name: String,
        event_value: Option<String>,
    ) -> Result<(), Box<dyn std::error::Error + Send + 'static>> {
        publisher
            .publish(RendererEvent::Event {
                name: event_name,
                value: event_value,
            })
            .map_err(|e| Box::new(e) as Box<dyn std::error::Error + Send>)?;
        Ok(())
    }

    /// # Errors
    ///
    /// Will error if `VanillaJsRenderer` fails to render the view.
    async fn render(
        &self,
        publisher: HtmlRendererEventPub,
        view: View,
    ) -> Result<(), Box<dyn std::error::Error + Send + 'static>> {
        publisher
            .publish(RendererEvent::View(view))
            .map_err(|e| Box::new(e) as Box<dyn std::error::Error + Send>)?;
        Ok(())
    }

    /// # Errors
    ///
    /// Will error if `VanillaJsRenderer` fails to render the partial elements.
    async fn render_partial(
        &self,
        publisher: HtmlRendererEventPub,
        partial: PartialView,
    ) -> Result<(), Box<dyn std::error::Error + Send + 'static>> {
        publisher
            .publish(RendererEvent::Partial(partial))
            .map_err(|e| Box::new(e) as Box<dyn std::error::Error + Send>)?;
        Ok(())
    }

    /// # Errors
    ///
    /// Will error if `VanillaJsRenderer` fails to render the canvas update.
    async fn render_canvas(
        &self,
        publisher: HtmlRendererEventPub,
        update: canvas::CanvasUpdate,
    ) -> Result<(), Box<dyn std::error::Error + Send + 'static>> {
        publisher
            .publish(RendererEvent::CanvasUpdate(update))
            .map_err(|e| Box::new(e) as Box<dyn std::error::Error + Send>)?;
        Ok(())
    }
}
