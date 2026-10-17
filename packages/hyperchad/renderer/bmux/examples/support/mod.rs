//! Canonical application trees shared by the executable demo and acceptance tests.
#![allow(dead_code)] // Each example/test intentionally uses a different fixture subset.

use hyperchad_renderer::Color;
use hyperchad_renderer::canvas::{CanvasAction, CanvasUpdate, Pos};
use hyperchad_transformer::models::{LayoutDirection, LayoutOverflow};
use hyperchad_transformer::{Container, Element, Input, Number};

#[derive(Debug, Clone, Copy)]
pub enum Workflow {
    Dashboard,
    Documents,
    MusicLibrary,
}

pub fn node(id: usize, key: &str, element: Element, children: Vec<Container>) -> Container {
    Container {
        id,
        str_id: Some(key.into()),
        element,
        children,
        direction: LayoutDirection::Column,
        ..Container::default()
    }
}
pub fn text(id: usize, key: &str, value: &str) -> Container {
    node(
        id,
        key,
        Element::Text {
            value: value.into(),
        },
        vec![],
    )
}
fn button(id: usize, key: &str, label: &str) -> Container {
    node(
        id,
        key,
        Element::Button {
            r#type: Some("button".into()),
        },
        vec![text(id + 1, &format!("{key}-label"), label)],
    )
}
fn link(id: usize, key: &str, label: &str, href: &str) -> Container {
    node(
        id,
        key,
        Element::Anchor {
            target: None,
            href: Some(href.into()),
        },
        vec![text(id + 1, &format!("{key}-label"), label)],
    )
}
fn input(id: usize, key: &str, value: &str) -> Container {
    let mut result = node(
        id,
        key,
        Element::Input {
            input: Input::Text {
                value: Some(value.into()),
                placeholder: Some("Search".into()),
            },
            name: Some(key.into()),
            autofocus: Some(true),
        },
        vec![],
    );
    result.width = Some(Number::Integer(256));
    result.height = Some(Number::Integer(16));
    result
}
fn cell(id: usize, key: &str, value: &str, columns: Option<i64>) -> Container {
    node(
        id,
        key,
        Element::TD {
            rows: None,
            columns: columns.map(Number::Integer),
        },
        vec![text(id + 1, &format!("{key}-text"), value)],
    )
}
fn table() -> Container {
    let rich = node(
        121,
        "track-rich",
        Element::TD {
            rows: None,
            columns: None,
        },
        vec![
            link(122, "track-link", "First Light", "/albums/first-light"),
            node(
                124,
                "track-span",
                Element::Span,
                vec![text(125, "track-note", " · lossless")],
            ),
        ],
    );
    let mut table = node(
        100,
        "library-table",
        Element::Table,
        vec![
            node(
                101,
                "table-head",
                Element::THead,
                vec![node(
                    102,
                    "head-row",
                    Element::TR,
                    vec![
                        node(
                            103,
                            "head-title",
                            Element::TH {
                                rows: None,
                                columns: Some(Number::Integer(2)),
                            },
                            vec![text(104, "head-title-text", "Album / Artist")],
                        ),
                        node(
                            105,
                            "head-time",
                            Element::TH {
                                rows: None,
                                columns: None,
                            },
                            vec![text(106, "head-time-text", "Time")],
                        ),
                    ],
                )],
            ),
            node(
                110,
                "table-body",
                Element::TBody,
                vec![
                    node(
                        120,
                        "track-row-1",
                        Element::TR,
                        vec![
                            rich,
                            cell(130, "artist-1", "Núria 星", None),
                            cell(132, "time-1", "3:42", None),
                        ],
                    ),
                    node(
                        140,
                        "track-row-2",
                        Element::TR,
                        vec![
                            cell(141, "track-2", "Night Walk", None),
                            cell(143, "artist-2", "The Ensemble", None),
                            cell(145, "time-2", "4:08", None),
                        ],
                    ),
                    node(
                        150,
                        "table-summary",
                        Element::TR,
                        vec![cell(151, "summary", "2 tracks · locally cached", Some(3))],
                    ),
                ],
            ),
        ],
    );
    table.width = Some(Number::IntegerPercent(100));
    table
}

/// Every tree contains links, native controls, image, canvas and rich table cells.
pub fn document(workflow: Workflow) -> Container {
    let (title, description) = match workflow {
        Workflow::Dashboard => (
            "Dashboard",
            "Service health · 2 active streams · 98% cache hits",
        ),
        Workflow::Documents => (
            "Documents",
            "Terminal-native documents: select text, follow links, edit notes.",
        ),
        Workflow::MusicLibrary => (
            "Music library",
            "Browse albums, filter tracks, and maintain your playback queue.",
        ),
    };
    let mut image = node(
        30,
        "cover",
        Element::Image {
            source: Some("memory:demo-cover".into()),
            alt: Some("Demo album artwork".into()),
            fit: None,
            source_set: None,
            sizes: None,
            loading: None,
        },
        vec![],
    );
    image.width = Some(Number::Integer(96));
    image.height = Some(Number::Integer(48));
    let mut canvas = node(31, "activity", Element::Canvas, vec![]);
    canvas.width = Some(Number::Integer(128));
    canvas.height = Some(Number::Integer(32));
    let mut notes = node(
        40,
        "notes",
        Element::Textarea {
            value: "Draft notes".into(),
            placeholder: None,
            name: Some("notes".into()),
            rows: Some(Number::Integer(2)),
            cols: Some(Number::Integer(32)),
        },
        vec![],
    );
    notes.width = Some(Number::Integer(256));
    notes.height = Some(Number::Integer(48));
    let mut root = node(
        1,
        "app",
        Element::Main,
        vec![
            text(2, "title", title),
            text(3, "description", description),
            node(
                4,
                "navigation",
                Element::Div,
                vec![
                    link(5, "nav-dashboard", "Dashboard", "/dashboard"),
                    link(7, "nav-documents", "Documents", "/documents"),
                    link(9, "nav-library", "Music library", "/music"),
                ],
            ),
            input(20, "search", ""),
            node(
                21,
                "offline",
                Element::Input {
                    input: Input::Checkbox {
                        checked: Some(false),
                    },
                    name: Some("offline".into()),
                    autofocus: None,
                },
                vec![],
            ),
            node(
                22,
                "sort",
                Element::Select {
                    name: Some("sort".into()),
                    selected: Some("title".into()),
                    multiple: Some(false),
                    disabled: Some(false),
                    autofocus: None,
                },
                vec![
                    node(
                        23,
                        "sort-title",
                        Element::Option {
                            value: Some("title".into()),
                            disabled: None,
                        },
                        vec![text(24, "sort-title-label", "Title")],
                    ),
                    node(
                        25,
                        "sort-artist",
                        Element::Option {
                            value: Some("artist".into()),
                            disabled: None,
                        },
                        vec![text(26, "sort-artist-label", "Artist")],
                    ),
                ],
            ),
            button(27, "refresh", "Refresh"),
            image,
            canvas,
            notes,
            text(50, "status", "Ready"),
            table(),
        ],
    );
    root.overflow_y = LayoutOverflow::Auto;
    root.width = Some(Number::IntegerPercent(100));
    root.height = Some(Number::IntegerPercent(100));
    root
}

pub fn canvas_update() -> CanvasUpdate {
    CanvasUpdate {
        target: "activity".into(),
        canvas_actions: vec![
            CanvasAction::Clear,
            CanvasAction::StrokeColor(Color {
                r: 80,
                g: 200,
                b: 120,
                a: None,
            }),
            CanvasAction::StrokeSize(2.0),
            CanvasAction::FillRect(Pos(0.0, 24.0), Pos(128.0, 32.0)),
            CanvasAction::Line(Pos(0.0, 24.0), Pos(40.0, 8.0)),
            CanvasAction::Line(Pos(40.0, 8.0), Pos(128.0, 16.0)),
        ],
    }
}

pub fn cover_rgba() -> Vec<u8> {
    (0..16u8)
        .flat_map(|y| (0..16u8).flat_map(move |x| [x * 16, y * 16, 160, 255]))
        .collect()
}
