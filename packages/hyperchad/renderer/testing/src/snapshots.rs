use hyperchad_renderer::{
    View,
    transformer::{Container, models::Selector},
};

use crate::{
    dom::DomState,
    transcript::{StreamFrame, Transcript},
};

/// Produces a normalized text snapshot for DOM state.
#[must_use]
pub fn dom_snapshot(dom: &DomState) -> String {
    let Some(root) = dom.root() else {
        return "<empty-dom>\n".to_string();
    };

    let mut out = String::new();
    write_container(root, 0, &mut out);
    out
}

/// Produces a normalized text snapshot for stream transcript.
#[must_use]
pub fn transcript_snapshot(transcript: &Transcript) -> String {
    let mut out = String::new();
    for frame in transcript.frames() {
        match frame {
            StreamFrame::View { seq, view } => {
                out.push_str(&format!("{seq}: view {}\n", describe_view(view)));
            }
            StreamFrame::PartialView { seq, view } => {
                out.push_str(&format!("{seq}: partial_view {}\n", describe_view(view)));
            }
            StreamFrame::Event { seq, name, value } => {
                out.push_str(&format!("{seq}: event name={name:?} value={value:?}\n"));
            }
            #[cfg(feature = "canvas")]
            StreamFrame::CanvasUpdate { seq, update } => {
                out.push_str(&format!(
                    "{seq}: canvas_update target={:?} actions={}\n",
                    update.target,
                    update.canvas_actions.len()
                ));
            }
        }
    }
    out
}

fn write_container(container: &Container, depth: usize, out: &mut String) {
    let indent = "  ".repeat(depth);
    out.push_str(&format!(
        "{indent}- id={} str_id={:?} classes={:?} hidden={:?} visibility={:?} background={:?} element={:?}\n",
        container.id,
        container.str_id,
        container.classes,
        container.hidden,
        container.visibility,
        container.background,
        container.element
    ));

    for child in &container.children {
        write_container(child, depth.saturating_add(1), out);
    }
}

fn describe_view(view: &View) -> String {
    let primary = view
        .primary
        .as_ref()
        .map_or_else(|| "none".to_string(), |x| format!("id={}", x.id));

    let fragments = view
        .fragments
        .iter()
        .map(|x| selector_to_string(&x.selector))
        .collect::<Vec<_>>()
        .join(",");

    let deletes = view
        .delete_selectors
        .iter()
        .map(selector_to_string)
        .collect::<Vec<_>>()
        .join(",");

    format!("primary={primary} fragments=[{fragments}] delete_selectors=[{deletes}]")
}

fn selector_to_string(selector: &Selector) -> String {
    match selector {
        Selector::Id(id) => format!("#{id}"),
        Selector::Class(class) => format!(".{class}"),
        Selector::ChildClass(class) => format!("> .{class}"),
        Selector::SelfTarget => "self".to_string(),
    }
}
