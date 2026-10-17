//! Exhaustive native element dispatch.
use super::*;
use egui::accesskit::Role;

impl<F: WidgetAdapter, R: WidgetAdapter, C: WidgetAdapter> FramePainter for WidgetPainter<F, R, C> {
    fn reconcile(
        &mut self,
        document: &Document,
        mutations: &[MutationSummary],
    ) -> Result<(), String> {
        self.reconcile_disclosures(document);
        self.forms.reconcile(document, mutations)?;
        self.resources.reconcile(document, mutations)?;
        self.canvas.reconcile(document, mutations)?;
        self.text_styles
            .retain(|key, _| document.get(*key).is_some());
        self.native_children.clear();
        self.list_markers.clear();
        self.opacity.retain(|key, _| document.get(*key).is_some());
        for key in document.keys() {
            let mut ancestor = document.parent(key);
            while let Some(parent) = ancestor {
                if document.get(parent).is_some_and(|node| {
                    matches!(
                        node.element,
                        Element::Select { .. }
                            | Element::Option { .. }
                            | Element::Textarea { .. }
                            | Element::Input { .. }
                    )
                }) {
                    self.native_children.insert(key);
                    break;
                }
                ancestor = document.parent(parent);
            }
            let Some(node) = document.get(key) else {
                continue;
            };
            if matches!(node.element, Element::OrderedList | Element::UnorderedList) {
                let ordered = matches!(node.element, Element::OrderedList);
                let mut ordinal = 0;
                for child in document.children(key) {
                    if document
                        .get(child)
                        .is_some_and(|node| matches!(node.element, Element::ListItem))
                    {
                        ordinal += 1;
                        self.list_markers.insert(
                            child,
                            if ordered {
                                format!("{ordinal}.")
                            } else {
                                "•".to_owned()
                            },
                        );
                    }
                }
            }
        }
        Ok(())
    }

    fn apply_canvas(
        &mut self,
        document: &Document,
        command: crate::canvas::CanvasCommand,
    ) -> Result<ElementStamp, String> {
        self.canvas.apply_canvas(document, command)
    }

    fn take_changes(&mut self) -> Vec<(ElementStamp, crate::events::EventValue)> {
        let mut changes = self.forms.take_changes();
        changes.extend(self.resources.take_changes());
        changes.extend(self.canvas.take_changes());
        changes
    }

    fn pending_resources(&self) -> usize {
        self.forms.pending_resources()
            + self.resources.pending_resources()
            + self.canvas.pending_resources()
    }

    fn prepare_layout(
        &mut self,
        document: &Document,
        roots: &mut [Container],
    ) -> Result<(), String> {
        WidgetPainter::prepare_layout(self, document, roots)
    }

    fn allows_node(&self, key: ElementKey) -> bool {
        WidgetPainter::allows_node(self, key)
    }

    fn prepare_node(
        &mut self,
        stamp: ElementStamp,
        _authored: &Container,
        calculated: &Container,
        geometry: &NodeGeometry<ElementKey>,
    ) -> Result<(), String> {
        if geometry.key != stamp.key {
            return Err("widget identity does not match geometry".to_owned());
        }
        let own = calculated.calculated_opacity.unwrap_or(1.0);
        if !own.is_finite() {
            return Err("nonfinite native widget opacity".to_owned());
        }
        let parent = geometry
            .parent
            .and_then(|key| self.opacity.get(&key))
            .copied()
            .unwrap_or(1.0);
        self.opacity.insert(stamp.key, parent * own.clamp(0.0, 1.0));
        let style = text::TextStyle::resolve(
            calculated,
            geometry.parent.and_then(|key| self.text_styles.get(&key)),
        );
        self.text_styles.insert(stamp.key, style);
        Ok(())
    }

    fn paint_node(
        &mut self,
        ui: &mut egui::Ui,
        stamp: ElementStamp,
        authored: &Container,
        calculated: &Container,
        geometry: &NodeGeometry<ElementKey>,
    ) -> Result<Option<NodeOutput>, String> {
        if !self.allows_node(stamp.key) {
            return Ok(None);
        }
        if geometry.key != stamp.key {
            return Err("widget identity does not match geometry".to_owned());
        }
        let parent_opacity = geometry
            .parent
            .and_then(|key| self.opacity.get(&key))
            .copied()
            .unwrap_or(1.0);
        let own_opacity = calculated.calculated_opacity.unwrap_or(1.0);
        if !own_opacity.is_finite() {
            return Err("nonfinite native widget opacity".to_owned());
        }
        let opacity = parent_opacity * own_opacity.clamp(0.0, 1.0);
        self.opacity.insert(stamp.key, opacity);
        let text_style = text::TextStyle::resolve(
            calculated,
            geometry.parent.and_then(|key| self.text_styles.get(&key)),
        );
        self.text_styles.insert(stamp.key, text_style.clone());
        // Even fully clipped ancestors prepare inherited state for escaping children.
        if geometry.visible_bounds().is_none() {
            return Ok(None);
        }
        ui.multiply_opacity(opacity);
        let painter = ui
            .painter()
            .with_clip_rect(geometry.paint_clip.intersect(ui.clip_rect()));
        paint::box_paint(
            &painter,
            calculated,
            geometry.padding_box,
            geometry.transform.scale,
        );
        let (role, interactive) = match &authored.element {
            Element::Input { .. }
            | Element::Textarea { .. }
            | Element::Select { .. }
            | Element::Option { .. } => {
                return self
                    .forms
                    .paint_node(ui, stamp, authored, calculated, geometry)
                    .map(Some);
            }
            Element::Image { .. } => {
                return self
                    .resources
                    .paint_node(ui, stamp, authored, calculated, geometry)
                    .map(Some);
            }
            Element::Canvas => {
                return self
                    .canvas
                    .paint_node(ui, stamp, authored, calculated, geometry)
                    .map(Some);
            }
            // Raw is also the production HTML parser's ordinary text-run node.
            // Native policy is literal text for BOTH variants, not HTML execution
            // or reparsing: provenance is absent, so angle/entity heuristics would
            // corrupt legitimate text. HTML-only unescaped markup has no native
            // interpretation; its source is deliberately displayed verbatim.
            Element::Raw { value } | Element::Text { value } => {
                text::paint_text(
                    &painter,
                    calculated,
                    &text_style,
                    value,
                    geometry.content_box,
                    geometry.transform.scale,
                )?;
                (Role::Label, false)
            }
            Element::Button { .. } => (Role::Button, true),
            Element::Anchor { href, .. } => (Role::Link, href.is_some()),
            Element::Summary => {
                if let Some(owner) = self.summary_owners.get(&stamp.key) {
                    let open = self.disclosure.get(owner).is_some_and(|state| state.open);
                    let size = 5.0 * geometry.transform.scale;
                    let center = egui::pos2(
                        geometry.content_box.left() - 8.0 * geometry.transform.scale,
                        geometry.content_box.top()
                            + calculated.calculated_font_size.unwrap_or(14.0)
                                * geometry.transform.scale
                                * 0.5,
                    );
                    let points = if open {
                        vec![
                            center + egui::vec2(-size, -size * 0.5),
                            center + egui::vec2(size, -size * 0.5),
                            center + egui::vec2(0.0, size * 0.5),
                        ]
                    } else {
                        vec![
                            center + egui::vec2(-size * 0.5, -size),
                            center + egui::vec2(size * 0.5, 0.0),
                            center + egui::vec2(-size * 0.5, size),
                        ]
                    };
                    painter.add(egui::Shape::convex_polygon(
                        points,
                        calculated.color.map_or(egui::Color32::BLACK, paint::color),
                        egui::Stroke::NONE,
                    ));
                    (Role::Button, true)
                } else {
                    (Role::GenericContainer, false)
                }
            }
            Element::Heading { .. } => (Role::Heading, false),
            Element::OrderedList | Element::UnorderedList => (Role::List, false),
            Element::ListItem => {
                if let Some(marker) = self.list_markers.get(&stamp.key) {
                    let size =
                        calculated.calculated_font_size.unwrap_or(14.0) * geometry.transform.scale;
                    painter.text(
                        geometry.content_box.left_top() - egui::vec2(size * 0.4, 0.0),
                        egui::Align2::RIGHT_TOP,
                        marker,
                        egui::FontId::proportional(size),
                        calculated.color.map_or(egui::Color32::BLACK, paint::color),
                    );
                }
                (Role::ListItem, false)
            }
            Element::Table => (Role::Table, false),
            Element::THead | Element::TBody => (Role::RowGroup, false),
            Element::TR => (Role::Row, false),
            Element::TH { .. } => (Role::ColumnHeader, false),
            Element::TD { .. } => (Role::Cell, false),
            Element::Aside => (Role::Complementary, false),
            Element::Main => (Role::Main, false),
            Element::Header => (Role::Header, false),
            Element::Footer => (Role::Footer, false),
            Element::Form { .. } => (Role::Form, false),
            Element::Details { .. } => {
                if self
                    .summary_owners
                    .values()
                    .any(|owner| *owner == stamp.key)
                {
                    (Role::Group, false)
                } else {
                    let open = self
                        .disclosure
                        .get(&stamp.key)
                        .is_some_and(|state| state.open);
                    let summary_rect = egui::Rect::from_min_size(
                        geometry.padding_box.min,
                        egui::vec2(
                            geometry.padding_box.width(),
                            24.0 * geometry.transform.scale,
                        ),
                    );
                    painter.text(
                        summary_rect.left_center(),
                        egui::Align2::LEFT_CENTER,
                        if open { "▾ Details" } else { "▸ Details" },
                        egui::FontId::proportional(
                            calculated.calculated_font_size.unwrap_or(14.0)
                                * geometry.transform.scale,
                        ),
                        calculated.color.map_or(egui::Color32::BLACK, paint::color),
                    );
                    (Role::Button, true)
                }
            }
            Element::Div | Element::Span | Element::Section => (Role::GenericContainer, false),
        };
        if authored
            .data
            .get("disabled")
            .is_some_and(|value| value != "false")
            && matches!(authored.element, Element::Button { .. })
        {
            ui.disable();
        }
        let bounds = geometry.visible_bounds().expect("visibility checked above");
        let bounds = if matches!(authored.element, Element::Details { .. }) && interactive {
            bounds.intersect(egui::Rect::from_min_size(
                geometry.padding_box.min,
                egui::vec2(
                    geometry.padding_box.width(),
                    24.0 * geometry.transform.scale,
                ),
            ))
        } else {
            bounds
        };
        // Hover-only descendants cannot steal the parent's click/focus response.
        let mut response = ui.interact(
            bounds,
            egui::Id::new(stamp.key),
            if interactive {
                egui::Sense::click()
            } else {
                egui::Sense::hover()
            },
        );
        let disclosure_owner = self.summary_owners.get(&stamp.key).copied().or_else(|| {
            (matches!(authored.element, Element::Details { .. }) && interactive)
                .then_some(stamp.key)
        });
        if let Some(owner) = disclosure_owner {
            if response.clicked() {
                if let Some(state) = self.disclosure.get_mut(&owner) {
                    state.open = !state.open;
                    response.mark_changed();
                    ui.ctx().request_repaint();
                }
            }
        }
        let name = if matches!(authored.element, Element::Details { .. }) && interactive {
            "Details".to_owned()
        } else {
            accessible_name(authored)
        };
        response.widget_info(|| {
            egui::WidgetInfo::labeled(
                if matches!(authored.element, Element::Anchor { .. }) {
                    egui::WidgetType::Link
                } else if interactive {
                    egui::WidgetType::Button
                } else {
                    egui::WidgetType::Label
                },
                ui.is_enabled(),
                &name,
            )
        });
        ui.ctx().accesskit_node_builder(response.id, |node| {
            node.set_role(role);
            node.set_label(name.clone());
            if let Some(owner) = disclosure_owner {
                if let Some(state) = self.disclosure.get(&owner) {
                    node.set_expanded(state.open);
                }
            }
            node.set_bounds(egui::accesskit::Rect {
                x0: f64::from(bounds.left()),
                y0: f64::from(bounds.top()),
                x1: f64::from(bounds.right()),
                y1: f64::from(bounds.bottom()),
            });
            if let Element::Heading { size } = &authored.element {
                use hyperchad_transformer::HeaderSize;
                node.set_level(match size {
                    HeaderSize::H1 => 1,
                    HeaderSize::H2 => 2,
                    HeaderSize::H3 => 3,
                    HeaderSize::H4 => 4,
                    HeaderSize::H5 => 5,
                    HeaderSize::H6 => 6,
                });
            }
        });
        if interactive {
            if response.hovered() {
                ui.ctx().set_cursor_icon(egui::CursorIcon::PointingHand);
            }
            if response.has_focus() {
                painter.rect_stroke(
                    bounds.shrink(1.0),
                    0.0,
                    ui.visuals().selection.stroke,
                    egui::StrokeKind::Inside,
                );
            }
        }
        Ok(Some(NodeOutput {
            response,
            value: None,
        }))
    }
}

fn accessible_name(node: &Container) -> String {
    fn append(node: &Container, output: &mut String) {
        match &node.element {
            Element::Text { value } | Element::Raw { value } => output.push_str(value),
            Element::Image { alt: Some(alt), .. } => output.push_str(alt),
            _ => {} // Native control values are excluded from names.
        }
        for child in &node.children {
            append(child, output);
        }
    }
    let mut result = String::new();
    append(node, &mut result);
    result.split_whitespace().collect::<Vec<_>>().join(" ")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn nested_button_name_includes_literal_raw_text() {
        let node = Container {
            element: Element::Button { r#type: None },
            children: vec![
                Container {
                    element: Element::Text {
                        value: " Play ".to_owned(),
                    },
                    ..Container::default()
                },
                Container {
                    element: Element::Span,
                    children: vec![Container {
                        element: Element::Text {
                            value: " album".to_owned(),
                        },
                        ..Container::default()
                    }],
                    ..Container::default()
                },
                Container {
                    element: Element::Raw {
                        value: "<b>not rendered</b>".to_owned(),
                    },
                    ..Container::default()
                },
            ],
            ..Container::default()
        };
        assert_eq!(accessible_name(&node), "Play album<b>not rendered</b>");
    }
}
