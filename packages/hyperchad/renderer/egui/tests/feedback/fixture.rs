//! Network-free HyperChad fixture shared by headless and native feedback runners.

use hyperchad_actions::{Action, ActionTrigger, ActionType};
use hyperchad_transformer::Container;

pub const WIDTH: f32 = 640.0;
pub const HEIGHT: f32 = 480.0;
pub const BUTTON_ID: &str = "feedback-submit";
pub const INPUT_ID: &str = "feedback-name";
pub const CHECKBOX_ID: &str = "feedback-enabled";
pub const LINK_ID: &str = "feedback-next";
pub const CLICK_ACTION: &str = "feedback-submit";
pub const CHANGE_ACTION: &str = "feedback-name-changed";
pub const CHECKBOX_ACTION: &str = "feedback-enabled-changed";
#[cfg(test)]
pub const NEXT_ROUTE: &str = "/feedback/next";

// Use production HTML parsing, not a parallel egui implementation. No remote
// fonts, images, routes with automatic loading, animations, or wall-clock text.
const HTML: &str = r##"
<div id="feedback-root" sx-dir="col" sx-width="640" sx-height="480"
     sx-padding="24" sx-gap="16" sx-background="#181a1b" sx-color="#ffffff">
    <div id="feedback-title" sx-height="32">HyperChad interaction feedback</div>
    <div sx-height="24">Name</div>
    <input id="feedback-name" type="text" value="Ada" placeholder="Name"
           sx-width="280" sx-height="32" />
    <div sx-dir="row" sx-height="32" sx-gap="12">
        <input id="feedback-enabled" type="checkbox" sx-width="24" sx-height="24" />
        <div>Enable feedback</div>
    </div>
    <button id="feedback-submit" sx-width="160" sx-height="36">Submit feedback</button>
    <a id="feedback-next" href="/feedback/next" sx-width="160" sx-height="28">Next page</a>
</div>
"##;

/// Build the actual HyperChad view, including production custom-action wiring.
///
/// Panics if this checked-in fixture cannot be parsed or loses a required ID.
#[must_use]
pub fn view() -> Container {
    let html = HTML.lines().map(str::trim).collect::<String>();
    let mut root = Container::try_from(html).expect("feedback HTML must parse");
    for (id, trigger, name) in [
        (BUTTON_ID, ActionTrigger::Click, CLICK_ACTION),
        (INPUT_ID, ActionTrigger::Change, CHANGE_ACTION),
        (CHECKBOX_ID, ActionTrigger::Change, CHECKBOX_ACTION),
    ] {
        find_mut(&mut root, id)
            .unwrap_or_else(|| panic!("missing feedback fixture element {id}"))
            .actions
            .push(Action {
                effect: if matches!(trigger, ActionTrigger::Change) {
                    ActionType::Parameterized {
                        action: Box::new(ActionType::Custom {
                            action: name.to_owned(),
                        }),
                        value: hyperchad_actions::logic::CalcValue::EventValue.into(),
                    }
                } else {
                    ActionType::Custom {
                        action: name.to_owned(),
                    }
                }
                .into(),
                trigger,
            });
    }
    root
}

/// Change the authored visible background, not the parser's transparent wrapper.
/// The wrapper is a real ancestor; painting it behind an opaque child cannot
/// produce a visible mismatch and must not override the child's authored color.
pub fn set_background(root: &mut Container, color: hyperchad_color::Color) {
    find_mut(root, "feedback-root")
        .expect("fixture background host")
        .background = Some(color);
}

fn find_mut<'a>(root: &'a mut Container, id: &str) -> Option<&'a mut Container> {
    if root.str_id.as_deref() == Some(id) {
        return Some(root);
    }
    root.children
        .iter_mut()
        .find_map(|child| find_mut(child, id))
}
