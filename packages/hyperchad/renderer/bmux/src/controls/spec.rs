//! Canonical control descriptions, deliberately separate from mutable state.

use bmux_tui_components::select_dropdown::SelectOption;
use hyperchad_transformer::{Container, Element, Input};

use crate::layout::node_key;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum Kind {
    Text,
    Password,
    Textarea,
    Checkbox,
    File,
    Hidden,
    Select,
    Multiple,
    Button,
    Anchor,
    Details,
    Summary,
}

// No Debug: `external` can contain a password.
#[derive(Clone)]
pub(super) struct Spec {
    pub kind: Kind,
    pub label: String,
    pub placeholder: String,
    pub external: Option<String>,
    pub checked: bool,
    pub checked_external: Option<bool>,
    pub options: Vec<SelectOption>,
    pub disabled: bool,
    pub autofocus: bool,
    pub name: Option<String>,
    pub form: Option<String>,
    pub details: Option<String>,
    pub button_type: String,
    pub href: Option<String>,
}

impl Spec {
    pub fn selected_index(&self) -> Option<usize> {
        self.external
            .as_ref()
            .and_then(|value| self.options.iter().position(|option| &option.id == value))
            .filter(|index| !self.options[*index].disabled)
            .or_else(|| {
                if self.external.is_none() {
                    self.options.iter().position(|option| !option.disabled)
                } else {
                    None
                }
            })
    }

    pub const fn focusable(&self) -> bool {
        !matches!(self.kind, Kind::Details | Kind::Hidden)
    }
}

pub(super) fn text(node: &Container) -> String {
    let mut result = match &node.element {
        Element::Text { value } => value.clone(),
        // Raw markup is never interpreted as terminal control sequences.
        _ => String::new(),
    };
    for child in &node.children {
        result.push_str(&text(child));
    }
    result
        .chars()
        .filter(|ch| !ch.is_control() || *ch == '\n' || *ch == '\t')
        .collect()
}

pub(super) fn collect(root: &Container) -> Vec<(String, Spec)> {
    let mut result = Vec::new();
    walk(root, &mut Vec::new(), None, None, false, &mut result);
    result
}

fn walk(
    node: &Container,
    path: &mut Vec<usize>,
    form: Option<&str>,
    details: Option<&str>,
    inherited_disabled: bool,
    out: &mut Vec<(String, Spec)>,
) {
    // Container has no generic disabled field. Honor explicit data-disabled
    // on semantic ancestors, as well as Select's native disabled flag.
    let disabled = inherited_disabled
        || node
            .data
            .get("disabled")
            .is_some_and(|value| value != "false" && value != "0")
        || matches!(
            node.element,
            Element::Select {
                disabled: Some(true),
                ..
            }
        );
    let key = node_key(node, path);
    let form = if matches!(node.element, Element::Form { .. }) {
        Some(key.as_str())
    } else {
        form
    };
    let mut spec = Spec {
        kind: Kind::Button,
        label: text(node),
        placeholder: String::new(),
        external: None,
        checked: false,
        checked_external: None,
        options: Vec::new(),
        disabled,
        autofocus: false,
        name: None,
        form: form.map(str::to_owned),
        details: details.map(str::to_owned),
        button_type: "submit".into(),
        href: None,
    };
    let supported = spec.decode(node);
    if supported {
        out.push((key.clone(), spec));
    }
    let details = if matches!(node.element, Element::Details { .. }) {
        Some(key.as_str())
    } else {
        details
    };
    for (index, child) in node.children.iter().enumerate() {
        path.push(index);
        walk(child, path, form, details, disabled, out);
        path.pop();
    }
}

impl Spec {
    fn decode(&mut self, node: &Container) -> bool {
        match &node.element {
            Element::Input { .. } => self.decode_input(node),
            Element::Textarea {
                value,
                placeholder,
                name,
                ..
            } => {
                self.kind = Kind::Textarea;
                self.external = Some(value.clone());
                self.placeholder = placeholder.clone().unwrap_or_default();
                self.name.clone_from(name);
                true
            }
            Element::Select {
                name,
                selected,
                multiple,
                disabled,
                autofocus,
            } => {
                self.kind = if multiple.unwrap_or(false) {
                    Kind::Multiple
                } else {
                    Kind::Select
                };
                self.name.clone_from(name);
                self.external.clone_from(selected);
                self.disabled |= disabled.unwrap_or(false);
                self.autofocus = autofocus.unwrap_or(false);
                self.options = node
                    .children
                    .iter()
                    .filter_map(|option| {
                        if let Element::Option { value, disabled } = &option.element {
                            let label = text(option);
                            Some(
                                SelectOption::new(
                                    value.clone().unwrap_or_else(|| label.clone()),
                                    label,
                                )
                                .disabled(disabled.unwrap_or(false)),
                            )
                        } else {
                            None
                        }
                    })
                    .collect();
                true
            }
            Element::Button { r#type } => {
                self.button_type = r#type
                    .clone()
                    .unwrap_or_else(|| "submit".into())
                    .to_ascii_lowercase();
                true
            }
            Element::Anchor { href, .. } => {
                self.kind = Kind::Anchor;
                self.href.clone_from(href);
                true
            }
            Element::Details { open } => {
                self.kind = Kind::Details;
                self.checked = open.unwrap_or(false);
                self.checked_external = *open;
                true
            }
            Element::Summary => {
                self.kind = Kind::Summary;
                true
            }
            _ => false,
        }
    }

    fn decode_input(&mut self, node: &Container) -> bool {
        let Element::Input {
            input,
            name,
            autofocus,
        } = &node.element
        else {
            return false;
        };

        self.name.clone_from(name);
        self.autofocus = autofocus.unwrap_or(false);
        match input {
            Input::Text { value, placeholder } | Input::Password { value, placeholder } => {
                self.kind = if matches!(input, Input::Password { .. }) {
                    Kind::Password
                } else {
                    Kind::Text
                };
                self.external.clone_from(value);
                self.placeholder = placeholder.clone().unwrap_or_default();
            }
            Input::Checkbox { checked } => {
                self.kind = Kind::Checkbox;
                self.checked = checked.unwrap_or(false);
                self.checked_external = *checked;
            }
            Input::File => {
                self.kind = Kind::File;
                self.label = "Choose file".into();
            }
            Input::Hidden { value } => {
                self.kind = Kind::Hidden;
                self.external.clone_from(value);
            }
        }
        true
    }
}
