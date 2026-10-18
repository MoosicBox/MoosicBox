//! Exact geometry of the production Albums UI, not a hand-built shell.
#![cfg(feature = "bmux")]

use bmux_tui::{
    component::{Constraints, LogicalSize},
    geometry::Size,
};
use hyperchad::{
    renderer::View,
    renderer_bmux::{
        controls::Controls,
        layout::{LayoutOptions, LeafMeasure, ScrollOffset, layout, node_key},
        state::ViewState,
    },
    transformer::{Container, Element, models::SwapStrategy},
};
use moosicbox_app_native_ui::{albums, state::State};
use moosicbox_music_models::{AlbumSort, api::ApiAlbum};
use moosicbox_paging::Page;
use std::collections::BTreeSet;

fn key_for(root: &Container, predicate: impl Fn(&Container) -> bool) -> String {
    fn visit(
        node: &Container,
        path: &mut Vec<usize>,
        predicate: &impl Fn(&Container) -> bool,
    ) -> Option<String> {
        if predicate(node) {
            return Some(node_key(node, path));
        }
        for (index, child) in node.children.iter().enumerate() {
            path.push(index);
            let found = visit(child, path, predicate);
            path.pop();
            if found.is_some() {
                return found;
            }
        }
        None
    }
    visit(root, &mut Vec::new(), &predicate).expect("production UI element")
}

#[test]
fn actual_album_action_button_paints_svg_and_legible_label_once() {
    use bmux_tui::{
        buffer::Buffer, frame::Frame, geometry::Rect, image::ImageContribution, paint::PaintCx,
        style::Color,
    };
    use hyperchad::renderer_bmux::{program::BmuxProgram, renderer::BmuxConfig};
    fn find_button(node: &Container) -> Option<Container> {
        if matches!(node.element, Element::Button { .. })
            && node.background.is_some()
            && node
                .children
                .iter()
                .any(|child| matches!(child.element, Element::Image { .. }))
        {
            return Some(node.clone());
        }
        node.children.iter().find_map(find_button)
    }
    let state = State {
        connection: Some(moosicbox_app_models::Connection {
            name: "button fixture".into(),
            api_url: "http://example.invalid".into(),
        }),
        ..Default::default()
    };
    let content: Container =
        albums::album_page_content(&state, &ApiAlbum::default(), &[], None).into();
    let button = find_button(&content).expect("production album play button");
    assert!(!button.actions.is_empty(), "keep production click actions");
    let root = Container {
        color: Some(hyperchad::color::Color::from_hex("ffffff")),
        children: vec![button],
        ..Default::default()
    };
    let icon = key_for(&root, |node| matches!(node.element, Element::Image { .. }));
    let (actions, _rx) = flume::bounded(8);
    let (resize, _sizes) = flume::bounded(8);
    let mut program = BmuxProgram::new(BmuxConfig::default(), actions, resize);
    program.set_graphics_available(true);
    program.apply_view(root.into(), None).unwrap();
    program.prepare().unwrap();
    program
        .graphics
        .decode_for_display(
            &icon,
            &hyperchad::renderer_bmux::graphics::GraphicsSource::Memory,
            &hyperchad::renderer_bmux::graphics::SourcePolicy {
                allow_memory: true,
                ..Default::default()
            },
            include_bytes!("../public/img/play-button.svg"),
            Some((16, 16)),
        )
        .unwrap();
    program.prepare().unwrap();
    let mut buffer = Buffer::empty(Rect::new(0, 0, 140, 60));
    {
        let mut frame = Frame::new(&mut buffer);
        program.paint(&mut PaintCx::new(&mut frame));
        assert!(
            frame
                .images()
                .iter()
                .any(|image| matches!(image, ImageContribution::Present(_))),
            "production button SVG must reach graphics metadata"
        );
    }
    let text: String = buffer
        .cells()
        .iter()
        .map(|cell| cell.symbol.as_str())
        .collect();
    assert_eq!(
        text.matches("Play").count(),
        1,
        "child label is not painted twice"
    );
    let cell = buffer
        .cells()
        .iter()
        .find(|cell| cell.symbol == "P")
        .unwrap();
    assert_eq!(cell.style.fg, Some(Color::Black));
    assert_eq!(cell.style.bg, Some(Color::Rgb(255, 255, 255)));
}

#[test]
fn production_hundred_album_document_fits_history_snapshot_budget() {
    fn count_nodes(root: &Container) -> usize {
        1 + root.children.iter().map(count_nodes).sum::<usize>()
    }
    moosicbox_app_native::register_api_sources();
    let state = State {
        connection: Some(moosicbox_app_models::Connection {
            name: "History budget fixture".into(),
            api_url: "http://example.invalid".into(),
        }),
        ..State::default()
    };
    let root: Container = albums::albums(&state, &[], AlbumSort::NameAsc, None).into();
    let page = Page::WithTotal {
        items: (0..100)
            .map(|index| ApiAlbum {
                album_id: index.into(),
                title: format!(
                    "A Very Long Album Title — Complete Deluxe Remastered Sessions {index}"
                ),
                artist: "Artist with a long realistic display name".into(),
                contains_cover: true,
                ..Default::default()
            })
            .collect(),
        offset: 0,
        limit: 100,
        total: 100,
    };
    let response: View =
        albums::albums_list_start(&state, &page, &[], AlbumSort::NameAsc, 200, "").into();
    let mut document = ViewState::from_root(root).unwrap();
    document
        .apply_response(response, Some("albums"), SwapStrategy::Children)
        .unwrap();
    let nodes = count_nodes(document.root());
    eprintln!(
        "production snapshot DOM nodes={nodes} Container inline={} total inline={}",
        std::mem::size_of::<Container>(),
        nodes * std::mem::size_of::<Container>()
    );
    let mut history = hyperchad::renderer_bmux::history::DocumentHistory::default();
    assert!(
        history.push(
            Some(1),
            "/albums?sort=name-asc",
            document.root(),
            &std::collections::BTreeMap::new(),
            &std::collections::BTreeMap::new()
        ),
        "production 100-album canonical tree must be cacheable"
    );
    assert!(history.charged_bytes() <= 8 * 1024 * 1024);
    eprintln!(
        "production 100-album snapshot charge={}",
        history.charged_bytes()
    );
    let cached = history.pop(Some(1)).unwrap().into_parts().root;
    assert_eq!(&cached, document.root());
}

#[test]
fn actual_albums_shell_has_three_columns_and_confines_scrolling_at_140_by_56() {
    moosicbox_app_native::register_api_sources();
    let state = State {
        connection: Some(moosicbox_app_models::Connection {
            name: "Geometry fixture (no network)".into(),
            api_url: "http://example.invalid".into(),
        }),
        ..State::default()
    };
    let root: Container = albums::albums(&state, &[], AlbumSort::NameAsc, None).into();
    let sidebar_key = key_for(&root, |node| matches!(node.element, Element::Aside));
    let main_key = key_for(&root, |node| matches!(node.element, Element::Main));
    let footer_key = key_for(&root, |node| matches!(node.element, Element::Footer));
    let mut controls = Controls::new();
    controls.reconcile(&root);
    let opts = LayoutOptions {
        viewport: Size::new(140, 56),
        ..Default::default()
    };
    let skeleton = layout(&root, &opts, &controls.measurer(&root)).unwrap();

    // Exercise the exact dynamic response boundary. In particular, do NOT
    // assign album_display cards directly to grid.children: doing so bypasses
    // the response wrapper and previously concealed the live one-column bug.
    let page = Page::WithTotal {
        items: (0_u64..11)
            .map(|index| ApiAlbum {
                album_id: index.into(),
                artist_id: index.into(),
                title: if index % 3 == 0 {
                    "A Very Long Album Title — Complete Deluxe Remastered Sessions".into()
                } else {
                    format!("Album {index}")
                },
                artist: "Artist with a long realistic display name".into(),
                ..Default::default()
            })
            .collect(),
        offset: 0,
        limit: 100,
        total: 11,
    };
    // Same Containers -> View conversion returned by albums_list_start_route.
    // Preserve its envelope: the response layer, not this test, must interpret it.
    let response: View =
        albums::albums_list_start(&state, &page, &[], AlbumSort::NameAsc, 200, "").into();
    assert_eq!(response.primary.as_ref().unwrap().children.len(), 11);
    let mut view_state = ViewState::from_root(root).unwrap();
    view_state
        .apply_response(response, Some("albums"), SwapStrategy::Children)
        .unwrap();
    let root = view_state.root();
    controls.reconcile(root);
    let base = controls.measurer(root);
    let decoded = |node: &Container, constraints: Constraints| {
        if matches!(node.element, Element::Image { .. }) {
            LogicalSize::new(150, 75)
        } else {
            base.measure(node, constraints)
        }
    };
    let before = layout(root, &opts, &base).unwrap();
    let after = layout(root, &opts, &decoded).unwrap();
    for tree in [&skeleton, &before, &after] {
        let sidebar = tree.get(&sidebar_key).unwrap();
        let main = tree.get(&main_key).unwrap();
        let footer = tree.get(&footer_key).unwrap();
        assert_eq!(sidebar.bounds.x, 0);
        assert_eq!(sidebar.bounds.width, 30);
        assert_eq!(main.bounds.x, 30);
        assert_eq!(main.bounds.width, 110);
        assert_eq!(main.bounds.y, 0);
        assert_eq!(main.bounds.bottom(), footer.bounds.y);
        assert_eq!(sidebar.bounds.height, main.bounds.height);
        assert_eq!(footer.bounds.right(), 140);
        assert!(footer.bounds.bottom() <= 56);
        assert_eq!(tree.get("albums").unwrap().content_bounds.width, 102);
    }
    for tree in [&before, &after] {
        let grid = tree.get("albums").unwrap();
        let cards: Vec<_> = grid
            .children
            .iter()
            .map(|key| tree.get(key).unwrap())
            .collect();
        assert_eq!(cards.len(), 11);
        let first_y = cards[0].bounds.y;
        assert_eq!(
            cards
                .iter()
                .take_while(|card| card.bounds.y == first_y)
                .count(),
            3
        );
        assert_eq!(
            cards
                .iter()
                .map(|card| card.bounds.x)
                .collect::<BTreeSet<_>>()
                .len(),
            3
        );
        for (index, card) in cards.iter().enumerate() {
            assert_eq!(card.bounds.width, 25);
            assert_eq!(card.bounds.x, cards[index % 3].bounds.x);
            assert!(card.bounds.x >= grid.content_bounds.x);
            assert!(card.bounds.right() <= grid.content_bounds.right());
        }
    }
    let mut scrolled_opts = opts.clone();
    scrolled_opts
        .scroll_offsets
        .insert(main_key.clone(), ScrollOffset { x: 0, y: 16 });
    let scrolled = layout(root, &scrolled_opts, &decoded).unwrap();
    assert_eq!(scrolled.get(&main_key).unwrap().scroll_offset.y, 16);
    for key in [&sidebar_key, &main_key, &footer_key] {
        assert_eq!(
            before.get(key).unwrap().bounds,
            after.get(key).unwrap().bounds
        );
        assert_eq!(
            after.get(key).unwrap().bounds,
            scrolled.get(key).unwrap().bounds
        );
    }
    assert_eq!(
        scrolled.get("albums").unwrap().bounds.y,
        after.get("albums").unwrap().bounds.y - 16
    );
    let main = scrolled.get(&main_key).unwrap();
    assert_eq!(main.text_clip.bottom(), main.bounds.bottom());
    for key in &scrolled.get("albums").unwrap().children {
        assert!(scrolled.get(key).unwrap().clip.bottom() <= main.bounds.bottom());
    }
}
