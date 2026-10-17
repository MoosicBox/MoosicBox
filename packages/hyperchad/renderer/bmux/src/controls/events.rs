//! Native control-specific input dispatch; coordinates come from retained layouts.
use super::spec::Kind;
use super::{Control, ControlEvent, ControlOutcome, ControlValue, NativeState, Prepared, native};
use bmux_keyboard::KeyCode;
use bmux_tui::event::{Event, MouseEventKind};
use bmux_tui::geometry::Rect;
use bmux_tui_components::button::{Button, ButtonOutcome};
use bmux_tui_components::checkbox::{Checkbox, CheckboxOutcome};
use bmux_tui_components::select_dropdown::{SelectDropdown, SelectDropdownOutcome};
use bmux_tui_components::text_input::{TextInputControl, TextInputOutcome};
pub(super) fn dispatch(
    control: &mut Control,
    key: &str,
    event: &Event,
    local_event: &Event,
    focused: bool,
    area: Rect,
    prepared: &Prepared,
) -> (ControlOutcome, bool) {
    match &control.state {
        NativeState::Text(..) => text(control, key, local_event, area),
        NativeState::Select(..) => select(control, key, local_event, focused, area),
        NativeState::Button(..) => button(control, event, local_event, focused, area, prepared),
        NativeState::Checkbox(..) => checkbox(control, key, local_event, focused, area),
        NativeState::Multiple(..) => multiple(control, key, local_event, focused, prepared),
        _ => (ControlOutcome::default(), false),
    }
}
fn text(
    control: &mut Control,
    key: &str,
    local_event: &Event,
    area: Rect,
) -> (ControlOutcome, bool) {
    let NativeState::Text(state) = &mut control.state else {
        unreachable!("dispatch kind checked")
    };
    let mut outcome = ControlOutcome::default();
    let activate = false;

    let policy = native::policy(control.spec.kind);
    let mut state = state.borrow_mut();
    state.set_content_area(area, &policy);
    let scroll = state.vertical_scroll();
    let result = if control.spec.kind == Kind::Password && matches!(local_event, Event::Mouse(_)) {
        native::password_mouse(
            &mut state,
            &mut control.secret_pointer.borrow_mut(),
            &policy,
            area,
            local_event,
        )
    } else {
        TextInputControl::new(&policy).handle_event(&mut state, local_event)
    };
    outcome.consumed = result != TextInputOutcome::Ignored;
    outcome.redraw = outcome.consumed;
    match result {
        TextInputOutcome::Edited => {
            outcome.relayout = control.spec.kind == Kind::Textarea;
            let value = if control.spec.kind == Kind::Password {
                ControlValue::SecretChanged
            } else {
                ControlValue::Text(state.buffer().text().to_owned())
            };
            outcome.events.push(ControlEvent::Change {
                key: key.to_owned(),
                value,
            });
        }
        TextInputOutcome::Submitted => {
            if let Some(form) = &control.spec.form {
                outcome.events.push(ControlEvent::Submit {
                    form: form.clone(),
                    submitter: None,
                });
            }
        }
        TextInputOutcome::EdgeUp | TextInputOutcome::EdgeDown => {
            outcome.events.push(ControlEvent::Scroll {
                key: key.to_owned(),
                edge: if result == TextInputOutcome::EdgeUp {
                    -1
                } else {
                    1
                },
            });
        }
        _ => {}
    }
    if scroll != state.vertical_scroll() {
        outcome.events.push(ControlEvent::Scroll {
            key: key.to_owned(),
            edge: 0,
        });
    }

    (outcome, activate)
}

fn select(
    control: &mut Control,
    key: &str,
    local_event: &Event,
    focused: bool,
    area: Rect,
) -> (ControlOutcome, bool) {
    let NativeState::Select(state) = &mut control.state else {
        unreachable!("dispatch kind checked")
    };
    let mut outcome = ControlOutcome::default();
    let activate = false;

    let mut value = state.get();
    value.interaction.focused = focused;
    let result =
        SelectDropdown::new(&control.spec.options).handle_event(area, &mut value, local_event);
    state.set(value);
    outcome.consumed = result != SelectDropdownOutcome::Ignored;
    outcome.redraw = outcome.consumed;
    if matches!(
        result,
        SelectDropdownOutcome::Opened | SelectDropdownOutcome::Closed
    ) {
        outcome.relayout = true;
        outcome.events.push(ControlEvent::Popup {
            key: key.to_owned(),
            open: value.is_open(),
        });
    }
    if let SelectDropdownOutcome::Selected(index) = result {
        if let Some(option) = control.spec.options.get(index) {
            outcome.events.push(ControlEvent::Change {
                key: key.to_owned(),
                value: ControlValue::Text(option.id.clone()),
            });
        }
        outcome.relayout = true;
    }

    (outcome, activate)
}

fn button(
    control: &mut Control,
    event: &Event,
    local_event: &Event,
    focused: bool,
    area: Rect,
    prepared: &Prepared,
) -> (ControlOutcome, bool) {
    let NativeState::Button(state) = &mut control.state else {
        unreachable!("dispatch kind checked")
    };
    let mut outcome = ControlOutcome::default();

    if control.spec.kind == Kind::Anchor
        && let Event::Mouse(mouse) = event
        && !prepared.contains(i64::from(mouse.position.x), i64::from(mouse.position.y))
    {
        if matches!(mouse.kind, MouseEventKind::Up(_)) {
            let mut value = state.get();
            value.interaction.pressed = false;
            state.set(value);
        }
        return (ControlOutcome::default(), false);
    }
    let mut value = state.get();
    value.set_focused(focused);
    let result = Button::new(&control.spec.label).handle_event(area, &mut value, local_event);
    state.set(value);
    let activate = result == ButtonOutcome::Pressed;
    outcome.consumed = result.is_handled();
    outcome.redraw = result.needs_redraw();

    (outcome, activate)
}

fn checkbox(
    control: &mut Control,
    key: &str,
    local_event: &Event,
    focused: bool,
    area: Rect,
) -> (ControlOutcome, bool) {
    let NativeState::Checkbox(state) = &mut control.state else {
        unreachable!("dispatch kind checked")
    };
    let mut outcome = ControlOutcome::default();
    let activate = false;

    let mut value = state.get();
    value.set_focused(focused);
    let result = Checkbox::new(&control.spec.label).handle_event(area, &mut value, local_event);
    state.set(value);
    outcome.consumed = result != CheckboxOutcome::Ignored;
    outcome.redraw = outcome.consumed;
    if let CheckboxOutcome::Toggled(checked) = result {
        outcome.events.push(ControlEvent::Change {
            key: key.to_owned(),
            value: ControlValue::Checked(checked),
        });
    }

    (outcome, activate)
}

fn multiple(
    control: &mut Control,
    key: &str,
    local_event: &Event,
    focused: bool,
    prepared: &Prepared,
) -> (ControlOutcome, bool) {
    let NativeState::Multiple(states) = &mut control.state else {
        unreachable!("dispatch kind checked")
    };
    let mut outcome = ControlOutcome::default();
    let activate = false;

    let enabled: Vec<_> = control
        .spec
        .options
        .iter()
        .enumerate()
        .filter(|(_, option)| !option.disabled)
        .map(|(i, _)| i)
        .collect();
    if let Event::Key(stroke) = local_event
        && matches!(
            stroke.key,
            KeyCode::Up | KeyCode::Down | KeyCode::Home | KeyCode::End
        )
        && !enabled.is_empty()
    {
        let current = enabled
            .iter()
            .position(|i| *i == control.active_option.get())
            .unwrap_or(0);
        let next = match stroke.key {
            KeyCode::Up => current.saturating_sub(1),
            KeyCode::Down => (current + 1).min(enabled.len() - 1),
            KeyCode::Home => 0,
            _ => enabled.len() - 1,
        };
        control.active_option.set(enabled[next]);
        return (ControlOutcome::handled(), false);
    }
    let mut changed = false;
    for (index, (option, state)) in control.spec.options.iter().zip(states.iter()).enumerate() {
        if option.disabled
            || matches!(local_event, Event::Key(_)) && control.active_option.get() != index
        {
            continue;
        }
        let Some(child) = prepared.layout.children.get(index) else {
            continue;
        };
        let row = Rect::new(
            u16::try_from(child.x).unwrap_or(u16::MAX),
            u16::try_from(child.y).unwrap_or(u16::MAX),
            u16::try_from(child.node.size.width).unwrap_or(u16::MAX),
            u16::try_from(child.node.size.height).unwrap_or(u16::MAX),
        );
        let mut value = state.get();
        value.set_focused(focused && control.active_option.get() == index);
        let result = Checkbox::new(&option.label).handle_event(row, &mut value, local_event);
        if matches!(result, CheckboxOutcome::Toggled(_)) {
            control.active_option.set(index);
            changed = true;
        }
        state.set(value);
        outcome.consumed |= result != CheckboxOutcome::Ignored;
        outcome.redraw |= result != CheckboxOutcome::Ignored;
    }
    if changed {
        let selected = control
            .spec
            .options
            .iter()
            .zip(states.iter())
            .filter(|(_, state)| state.get().checked())
            .map(|(option, _)| option.id.clone())
            .collect();
        outcome.events.push(ControlEvent::Change {
            key: key.to_owned(),
            value: ControlValue::Multiple(selected),
        });
    }

    (outcome, activate)
}
