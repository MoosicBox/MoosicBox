//! Native component construction and secret-safe projections.

use std::cell::RefCell;

use bmux_text_edit::{SelectionMode, TextEditBuffer, TextMotion};
use bmux_tui::component::{Component, Constraints, EventCx, LayoutCx, LayoutId, LayoutNode};
use bmux_tui::composition::Column;
use bmux_tui::event::{Event, EventOutcome};
use bmux_tui::paint::PaintCx;
use bmux_tui::style::Style;
use bmux_tui_components::button::ButtonComponent;
use bmux_tui_components::checkbox::CheckboxComponent;
use bmux_tui_components::select_dropdown::SelectDropdownComponent;
use bmux_tui_components::text_input::{
    EnterBehavior, TextInputComponent, TextInputControl, TextInputOutcome, TextInputPolicy,
    TextInputState,
};
use unicode_segmentation::UnicodeSegmentation;

use super::spec::Kind;
use super::{Control, NativeState};

pub(super) fn policy(kind: Kind) -> TextInputPolicy {
    let mut policy = TextInputPolicy::chat_composer();
    policy.keyboard.enter = if kind == Kind::Textarea {
        EnterBehavior::InsertNewline
    } else {
        EnterBehavior::Submit
    };
    policy.keyboard.shift_enter = None;
    policy.viewport.min_rows = if kind == Kind::Textarea { 3 } else { 1 };
    policy.viewport.max_rows = if kind == Kind::Textarea {
        None
    } else {
        Some(1)
    };
    policy
}

/// Pointer coordinates for a password address masked graphemes, never the
/// cleartext's terminal widths. Native mouse selection runs on persistent masked
/// state and its resulting byte positions are mapped to native real-buffer motions.
pub(super) fn password_mouse(
    real: &mut TextInputState,
    shadow: &mut TextInputState,
    policy: &TextInputPolicy,
    area: bmux_tui::geometry::Rect,
    event: &Event,
) -> TextInputOutcome {
    let text = real.buffer().text();
    let offsets: Vec<_> = text
        .grapheme_indices(true)
        .map(|(offset, _)| offset)
        .chain(std::iter::once(text.len()))
        .collect();
    let mask = "*".repeat(offsets.len().saturating_sub(1));
    if shadow.buffer().text() != mask {
        *shadow = TextInputState::new(TextEditBuffer::from_text(mask));
    }
    shadow.set_content_area(area, policy);
    let result = TextInputControl::new(policy).handle_event(shadow, event);
    let cursor = shadow.buffer().cursor_byte_index().min(offsets.len() - 1);
    let anchor = shadow.buffer().selection().map(|selection| {
        if cursor == selection.start {
            selection.end
        } else {
            selection.start
        }
    });
    let buffer = real.buffer_mut();
    if let Some(anchor) = anchor {
        buffer.move_cursor(TextMotion::Absolute(offsets[anchor.min(offsets.len() - 1)]));
        buffer.move_cursor_with_selection(
            TextMotion::Absolute(offsets[cursor]),
            SelectionMode::Extend,
        );
    } else {
        buffer.move_cursor(TextMotion::Absolute(offsets[cursor]));
    }
    result
}

impl Control {
    pub(super) fn component<'a>(&'a self, key: &str, focused: bool) -> Box<dyn Component + 'a> {
        match &self.state {
            NativeState::Text(state) => Box::new(Editor {
                id: LayoutId::from(key.to_owned()),
                state,
                policy: policy(self.spec.kind),
                placeholder: &self.spec.placeholder,
                focused,
                secret: self.spec.kind == Kind::Password,
            }),
            NativeState::Checkbox(state) => {
                let mut value = state.get();
                value.set_focused(focused);
                value.set_disabled(self.spec.disabled);
                state.set(value);
                Box::new(CheckboxComponent::new(
                    key.to_owned(),
                    &self.spec.label,
                    state,
                ))
            }
            NativeState::Select(state) => {
                let mut value = state.get();
                value.interaction.focused = focused;
                value.set_disabled(self.spec.disabled);
                state.set(value);
                Box::new(SelectDropdownComponent::new(
                    key.to_owned(),
                    &self.spec.options,
                    state,
                ))
            }
            NativeState::Multiple(states) => {
                let mut column = Column::new().id(key.to_owned());
                for (index, (option, state)) in self.spec.options.iter().zip(states).enumerate() {
                    let mut value = state.get();
                    // Keyboard dispatch selects exactly one native row; the host
                    // router can address a row's emitted hit identity directly.
                    value.set_focused(focused && index == self.active_option.get());
                    value.set_disabled(self.spec.disabled || option.disabled);
                    state.set(value);
                    column = column.child(CheckboxComponent::new(
                        format!("{key}/option/{index}"),
                        &option.label,
                        state,
                    ));
                }
                Box::new(column)
            }
            NativeState::Button(state) => {
                let mut value = state.get();
                value.set_focused(focused);
                value.set_disabled(self.spec.disabled);
                state.set(value);
                if self.spec.kind == Kind::Anchor {
                    return Box::new(InlineLink {
                        id: LayoutId::from(key.to_owned()),
                        label: &self.spec.label,
                    });
                }
                Box::new(ButtonComponent::new(
                    key.to_owned(),
                    &self.spec.label,
                    state,
                ))
            }
            NativeState::Details(_) | NativeState::Hidden => {
                Box::new(Empty(LayoutId::from(key.to_owned())))
            }
        }
    }
}

/// Inline glyphs belong to the document formatting context. This component
/// contributes only intrinsic text size, with no native button decoration.
struct InlineLink<'a> {
    id: LayoutId,
    label: &'a str,
}
impl Component for InlineLink<'_> {
    fn layout(&self, constraints: Constraints, cx: &mut LayoutCx) -> LayoutNode {
        cx.record_measurement();
        let width = unicode_width::UnicodeWidthStr::width(self.label);
        LayoutNode::leaf(
            self.id.clone(),
            constraints.constrain(bmux_tui::component::LogicalSize::new(
                u64::try_from(width).unwrap_or(u64::MAX),
                1,
            )),
        )
    }
    fn paint(&self, _: &LayoutNode, _: &mut PaintCx<'_, '_>) {}
}

struct Empty(LayoutId);
impl Component for Empty {
    fn layout(&self, _: Constraints, _: &mut LayoutCx) -> LayoutNode {
        LayoutNode::leaf(self.0.clone(), bmux_tui::component::LogicalSize::new(0, 0))
    }
    fn paint(&self, _: &LayoutNode, _: &mut PaintCx<'_, '_>) {}
}

struct Editor<'a> {
    id: LayoutId,
    state: &'a RefCell<TextInputState>,
    policy: TextInputPolicy,
    placeholder: &'a str,
    focused: bool,
    secret: bool,
}

impl Editor<'_> {
    fn native<'a>(&'a self, state: &'a RefCell<TextInputState>) -> TextInputComponent<'a, 'a> {
        TextInputComponent::new(self.id.clone(), state, &self.policy)
            .placeholder(self.placeholder, Style::new())
            .focused(self.focused)
    }

    // Map byte-indexed native selection to one-cell-per-grapheme masked text.
    // The actual edit buffer and history never leave native editing state.
    fn masked(&self) -> RefCell<TextInputState> {
        let state = self.state.borrow();
        let source = state.buffer();
        let text = source.text();
        let mut mask = TextEditBuffer::from_text("*".repeat(text.graphemes(true).count()));
        let cursor = text[..source.cursor_byte_index()].graphemes(true).count();
        if let Some(selection) = source.selection() {
            let anchor = if source.cursor_byte_index() == selection.start {
                selection.end
            } else {
                selection.start
            };
            mask.move_cursor(TextMotion::Absolute(text[..anchor].graphemes(true).count()));
            mask.move_cursor_with_selection(TextMotion::Absolute(cursor), SelectionMode::Extend);
        } else {
            mask.move_cursor(TextMotion::Absolute(cursor));
        }
        RefCell::new(TextInputState::new(mask))
    }
}

impl Component for Editor<'_> {
    fn layout(&self, constraints: Constraints, cx: &mut LayoutCx) -> LayoutNode {
        if self.secret {
            self.native(&self.masked()).layout(constraints, cx)
        } else {
            self.native(self.state).layout(constraints, cx)
        }
    }

    fn paint(&self, layout: &LayoutNode, cx: &mut PaintCx<'_, '_>) {
        if self.secret {
            self.native(&self.masked()).paint(layout, cx);
        } else {
            self.native(self.state).paint(layout, cx);
        }
    }

    fn event(&self, event: &Event, layout: &LayoutNode, cx: &mut EventCx<'_>) -> EventOutcome {
        self.native(self.state).event(event, layout, cx)
    }
}
