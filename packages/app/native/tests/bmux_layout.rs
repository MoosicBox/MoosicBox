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
fn actual_albums_shell_confines_scrolling_at_140_by_56() {
    assert_album_shell(140, 56, 3);
}

#[test]
fn actual_populated_albums_scroll_reuses_committed_geometry() {
    use bmux_tui::{
        event::{Event, MouseEvent, MouseEventKind},
        geometry::Point,
    };
    use bmux_tui_runtime::{Program, RuntimeEvent};
    use hyperchad::renderer_bmux::{program::BmuxProgram, renderer::BmuxConfig};

    // Observe the normal public prepare path, not a copy of its private guard.
    // Thread-local capture prevents other parallel tests from supplying evidence.
    struct PrepareLog;
    thread_local! {
        static PREPARES: std::cell::RefCell<Vec<String>> = const {
            std::cell::RefCell::new(Vec::new())
        };
    }
    impl log::Log for PrepareLog {
        fn enabled(&self, metadata: &log::Metadata<'_>) -> bool {
            metadata.target().starts_with("hyperchad_renderer_bmux")
        }
        fn log(&self, record: &log::Record<'_>) {
            if self.enabled(record.metadata()) {
                let message = record.args().to_string();
                if message.starts_with("bmux prepare") || message.starts_with("bmux scroll") {
                    PREPARES.with(|records| records.borrow_mut().push(message));
                }
            }
        }
        fn flush(&self) {}
    }
    static LOGGER: PrepareLog = PrepareLog;
    log::set_logger(&LOGGER).expect("isolated integration-test logger");
    log::set_max_level(log::LevelFilter::Debug);
    moosicbox_app_native::register_api_sources();
    use moosicbox_app_native_ui::state::{PlaybackState, PlayerState};
    let state = State {
        connection: Some(moosicbox_app_models::Connection {
            name: "Scroll reuse fixture (no network)".into(),
            api_url: "http://example.invalid".into(),
        }),
        player: PlayerState {
            playback: Some(PlaybackState {
                session_id: 1,
                playing: false,
                position: 15,
                seek: 42.0,
                volume: 0.7,
                tracks: (0_u64..40)
                    .map(|index| moosicbox_music_models::api::ApiTrack {
                        track_id: index.into(),
                        album_id: index.into(),
                        title: format!("Long current playlist track title {index}"),
                        artist: "Production artist".into(),
                        album: "Production album".into(),
                        contains_cover: true,
                        duration: 240.0,
                        ..Default::default()
                    })
                    .collect(),
            }),
        },
        ..State::default()
    };
    let page = Page::WithTotal {
        items: (0_u64..100)
            .map(|index| ApiAlbum {
                album_id: index.into(),
                artist_id: (index / 4).into(),
                title: format!("Production album {index}: a long title with bonus tracks"),
                artist: format!("Production artist {}", index / 4),
                contains_cover: true,
                ..Default::default()
            })
            .collect(),
        offset: 0,
        limit: 100,
        total: 100,
    };
    let mut failures = Vec::new();
    for (width, height, menu_shown) in [(80, 24), (120, 40), (140, 60)]
        .into_iter()
        .flat_map(|(width, height)| [false, true].map(|shown| (width, height, shown)))
    {
        let mut root: Container = albums::albums(&state, &[], AlbumSort::NameAsc, None).into();
        fn set_menu_visibility(node: &mut Container, shown: bool) {
            if node.str_id.as_deref() == Some("albums-menu") {
                node.visibility = Some(if shown {
                    hyperchad::transformer::models::Visibility::Visible
                } else {
                    hyperchad::transformer::models::Visibility::Hidden
                });
            }
            for child in &mut node.children {
                set_menu_visibility(child, shown);
            }
        }
        set_menu_visibility(&mut root, menu_shown);
        let response: View = albums::albums_list_start(
            &state,
            &page,
            &[],
            AlbumSort::NameAsc,
            moosicbox_app_native_ui::ALBUM_ART_SIZE,
            "",
        )
        .into();
        let (actions, _actions_rx) = flume::bounded(8);
        let (resize, _resize_rx) = flume::bounded(8);
        let mut program = BmuxProgram::new(
            BmuxConfig {
                width,
                height,
                ..Default::default()
            },
            actions,
            resize,
        );
        program.apply_view(root.into(), None).unwrap();
        program
            .apply_response(response, Some("albums"), SwapStrategy::Children)
            .unwrap();
        program.prepare().unwrap();
        program.commit_prepared();
        let root = program.state.root();
        let menu_key = key_for(root, |node| node.str_id.as_deref() == Some("albums-menu"));
        assert_eq!(
            menu_key, "albums-menu",
            "keep actual albums_page_content menu"
        );
        let main_key = key_for(root, |node| node.element == Element::Main);
        let opts = LayoutOptions {
            viewport: Size::new(width, height),
            ..Default::default()
        };
        let tree = layout(root, &opts, &program.controls.measurer(root)).unwrap();
        let main = tree.get(&main_key).unwrap();
        let menu = tree
            .get("albums-menu")
            .expect("real hidden menu in authoritative geometry");
        eprintln!(
            "{width}x{height}: first production menu key={} path={:?} main_path={:?} menu_size={:?}",
            menu.key, menu.path, main.path, menu.bounds
        );
        assert!(
            menu.path.starts_with(&main.path),
            "actual menu remains in main scroll subtree"
        );
        assert!(main.content_extents.height > main.content_bounds.height);
        let pointer = Point::new(
            u16::try_from(main.content_bounds.x + 2).unwrap(),
            u16::try_from(main.content_bounds.y + 8).unwrap(),
        );
        let mut dynamic_images = Vec::new();
        fn inspect_images(node: &Container, path: &mut Vec<usize>, output: &mut Vec<String>) {
            if matches!(node.element, Element::Image { .. })
                && (!matches!(node.width, Some(hyperchad::transformer::Number::Integer(_)))
                    || !matches!(
                        node.height,
                        Some(hyperchad::transformer::Number::Integer(_))
                    ))
            {
                output.push(format!(
                    "{} width={:?} height={:?}",
                    node_key(node, path),
                    node.width,
                    node.height
                ));
            }
            for (index, child) in node.children.iter().enumerate() {
                path.push(index);
                inspect_images(child, path, output);
                path.pop();
            }
        }
        inspect_images(root, &mut Vec::new(), &mut dynamic_images);
        let mut positioned = Vec::new();
        fn inspect_positioned(node: &Container, path: &mut Vec<usize>, output: &mut Vec<String>) {
            if matches!(
                node.position,
                Some(hyperchad::transformer::models::Position::Absolute)
            ) {
                let value = format!(
                    "size={:?}/{:?} insets={:?}/{:?}/{:?}/{:?} minmax={:?}/{:?}/{:?}/{:?} margin={:?}/{:?}/{:?}/{:?} padding={:?}/{:?}/{:?}/{:?}",
                    node.width,
                    node.height,
                    node.left,
                    node.right,
                    node.top,
                    node.bottom,
                    node.min_width,
                    node.max_width,
                    node.min_height,
                    node.max_height,
                    node.margin_left,
                    node.margin_right,
                    node.margin_top,
                    node.margin_bottom,
                    node.padding_left,
                    node.padding_right,
                    node.padding_top,
                    node.padding_bottom
                );
                if !output.contains(&value) {
                    output.push(value);
                }
            }
            for (index, child) in node.children.iter().enumerate() {
                path.push(index);
                inspect_positioned(child, path, output);
                path.pop();
            }
        }
        inspect_positioned(root, &mut Vec::new(), &mut positioned);
        eprintln!("{width}x{height} actual Absolute shapes: {positioned:#?}");
        for (step, direction) in [
            MouseEventKind::ScrollDown,
            MouseEventKind::ScrollDown,
            MouseEventKind::ScrollDown,
            MouseEventKind::ScrollUp,
            MouseEventKind::ScrollDown,
            MouseEventKind::ScrollDown,
            MouseEventKind::ScrollUp,
        ]
        .into_iter()
        .enumerate()
        {
            PREPARES.with(|records| records.borrow_mut().clear());
            program
                .update(RuntimeEvent::Terminal(Event::Mouse(MouseEvent::new(
                    direction, pointer,
                ))))
                .unwrap();
            program.prepare().unwrap();
            let records = PREPARES.with(|records| records.borrow().clone());
            eprintln!(
                "{width}x{height} menu_shown={menu_shown} step={step}: diagnostic chain={records:?}"
            );
            if !records
                .iter()
                .any(|line| line.contains("scroll-reuse") && line.contains("full_layouts=0"))
            {
                failures.push(format!(
                    "{width}x{height} menu_shown={menu_shown} step={step}: {records:?}; first menu key={} path={:?}; dynamic images={dynamic_images:?}",
                    menu.key, menu.path
                ));
            }
            // A second prepare without a new scroll intent must take the fresh
            // full-reflow path, retaining exactly the same document and offsets.
            fn presentation(program: &mut BmuxProgram, width: u16, height: u16) -> String {
                use bmux_tui::{buffer::Buffer, frame::Frame, geometry::Rect, paint::PaintCx};
                let mut buffer = Buffer::empty(Rect::new(0, 0, width, height));
                let metadata = {
                    let mut frame = Frame::new(&mut buffer);
                    program.paint(&mut PaintCx::new(&mut frame));
                    format!("{:?}/{:?}", frame.hits().regions(), frame.images())
                };
                format!("{:?}/{metadata}", buffer.cells())
            }
            let reused = presentation(&mut program, width, height);
            PREPARES.with(|records| records.borrow_mut().clear());
            program.prepare().unwrap();
            let full_records = PREPARES.with(|records| records.borrow().clone());
            assert!(
                full_records.iter().any(|line| line.contains("layout_us="))
                    && !full_records
                        .iter()
                        .any(|line| line.contains("scroll-reuse")),
                "comparator must execute fresh full reflow: {full_records:?}"
            );
            assert_eq!(
                reused,
                presentation(&mut program, width, height),
                "full-frame text/style/hit/image parity {width}x{height} menu_shown={menu_shown} step={step}"
            );
            program.commit_prepared();
        }
    }
    assert!(
        failures.is_empty(),
        "production scroll reuse rejected:\n{}",
        failures.join("\n")
    );
}

#[test]
fn actual_albums_shell_is_stable_at_common_terminal_sizes() {
    for (width, height, columns) in [(80, 24, 1), (120, 40, 3), (140, 60, 3)] {
        assert_album_shell(width, height, columns);
    }
}

#[test]
fn actual_album_details_keep_artwork_beside_metadata_and_tracks_below() {
    moosicbox_app_native::register_api_sources();
    let state = State {
        connection: Some(moosicbox_app_models::Connection {
            name: "Geometry fixture".into(),
            api_url: "http://example.invalid".into(),
        }),
        ..State::default()
    };
    let album = ApiAlbum {
        title: "A very long realistic album title with remastered sessions".into(),
        artist: "Artist with a long realistic display name".into(),
        ..Default::default()
    };
    let versions: Vec<_> = serde_json::from_value(serde_json::json!([{
        "tracks": [moosicbox_music_models::api::ApiTrack {
            title: "A realistic long track title".into(),
            artist: album.artist.clone(),
            duration: 240.0,
            ..Default::default()
        }],
        "format": null,
        "bitDepth": 16,
        "sampleRate": 44100,
        "channels": 2,
        "source": moosicbox_music_models::TrackApiSource::Local,
    }]))
    .unwrap();
    let content = albums::album_page_content(&state, &album, &versions, None);
    let loading: Container = moosicbox_app_native_ui::page(
        &state,
        &albums::album_page_immediate("0", None, None, None, None),
    )
    .into();
    let target = key_for(&loading, |node| node.route.is_some());
    let loading_root = loading.clone();
    let mut view_state = ViewState::from_root(loading).unwrap();
    // Same fragment swap as GET /albums?albumId=0&full=true after the
    // production details loading route fires. Keep the surrounding shell.
    view_state
        .apply_response(content.into(), Some(&target), SwapStrategy::This)
        .unwrap();
    let root = view_state.root();
    let cover_key = key_for(root, |node| {
        matches!(node.element, Element::Image { .. })
            && node.width == Some(moosicbox_app_native_ui::ALBUM_ART_SIZE.into())
    });
    let title_key = key_for(root, |node| {
        matches!(node.element, Element::Heading { .. }) && node.children.iter().any(|child| matches!(&child.element, Element::Raw { value } | Element::Text { value } if value == &album.title))
    });
    let table_key = key_for(root, |node| matches!(node.element, Element::Table));
    let mut controls = Controls::new();
    controls.reconcile(root);
    for (width, height) in [(80, 24), (120, 40), (140, 60)] {
        let opts = LayoutOptions {
            viewport: Size::new(width, height),
            ..Default::default()
        };
        let tree = layout(root, &opts, &controls.measurer(root)).unwrap();
        let mut loading_controls = Controls::new();
        loading_controls.reconcile(&loading_root);
        let before = layout(
            &loading_root,
            &opts,
            &loading_controls.measurer(&loading_root),
        )
        .unwrap();
        for element in [Element::Aside, Element::Main, Element::Footer] {
            let before_key = key_for(&loading_root, |node| node.element == element);
            let after_key = key_for(root, |node| node.element == element);
            assert_eq!(
                before.get(&before_key).unwrap().bounds,
                tree.get(&after_key).unwrap().bounds
            );
        }
        let cover = tree.get(&cover_key).unwrap();
        let title = tree.get(&title_key).unwrap();
        eprintln!(
            "details {width}x{height}: cover={:?} title={:?}",
            cover.bounds, title.bounds
        );
        assert!(
            title.bounds.x >= cover.bounds.right(),
            "metadata must be to the right of artwork"
        );
        assert!(
            title.bounds.right() <= i64::from(width),
            "metadata must fit the viewport"
        );
        assert!(title.bounds.width >= 15);
        assert!(
            tree.get(&table_key).unwrap().bounds.y
                >= cover.bounds.bottom().max(title.bounds.bottom())
        );
    }
}

#[test]
fn host_cell_metrics_are_sensitivity_checks_not_production_layout_metrics() {
    use hyperchad::renderer_bmux::layout::TerminalMetrics;
    moosicbox_app_native::register_api_sources();
    let state = State::default();
    let root: Container = albums::albums(&state, &[], AlbumSort::NameAsc, None).into();
    let mut controls = Controls::new();
    controls.reconcile(&root);
    // Production still uses virtual8x16 regardless of the host terminal font.
    // These alternatives only exercise the general layout API's sensitivity.
    for (width, height, cell_width, cell_height) in [
        (80, 24, 11.0, 23.0),
        (120, 40, 9.0, 18.0),
        (140, 60, 8.0, 17.0),
    ] {
        let opts = LayoutOptions {
            viewport: Size::new(width, height),
            metrics: TerminalMetrics {
                cell_width,
                cell_height,
            },
            ..Default::default()
        };
        let tree = layout(&root, &opts, &controls.measurer(&root)).unwrap();
        let main = tree
            .get(&key_for(&root, |node| {
                matches!(node.element, Element::Main)
            }))
            .unwrap();
        let footer = tree
            .get(&key_for(&root, |node| {
                matches!(node.element, Element::Footer)
            }))
            .unwrap();
        let search = tree.get("search-button").unwrap();
        assert_eq!(main.bounds.bottom(), footer.bounds.y);
        assert!(footer.bounds.bottom() <= i64::from(height));
        assert!(search.bounds.right() <= i64::from(width));
        eprintln!(
            "sensitivity only{width}x{height} cells{cell_width}x{cell_height}: main={:?} footer={:?} search={:?}",
            main.bounds, footer.bounds, search.bounds
        );
    }
}

#[test]
fn actual_named_producer_containers_preserve_shell_and_untouched_siblings() {
    use moosicbox_app_native_ui::{search, settings};
    use moosicbox_music_models::ApiSource;
    moosicbox_app_native::register_api_sources();
    let source = ApiSource::library();
    let outputs = [
        moosicbox_app_native_ui::audio_zones::audio_zones(&[], &[]),
        albums::load_albums(
            moosicbox_app_native_ui::ALBUM_ART_SIZE,
            AlbumSort::NameAsc,
            &[],
            "",
        ),
        search::results_content("http://example.invalid", &source, &[]),
        settings::download_settings_content(&moosicbox_app_models::DownloadSettings {
            download_locations: vec![],
            default_download_location: None,
        }),
        settings::scan_settings_content(&moosicbox_app_models::ScanSettings { scan_paths: vec![] }),
    ];
    for output in outputs {
        assert!(!output.is_empty());
        let mut root: Container = moosicbox_app_native_ui::page(
            &State::default(),
            &hyperchad::template::container! { div #untouched { "Sibling" } },
        )
        .into();
        fn insert_slots(node: &mut Container, output: &[Container]) -> bool {
            if matches!(node.element, Element::Main) {
                for item in output {
                    let id = item
                        .str_id
                        .clone()
                        .expect("actual producer output must have a named target");
                    if id == moosicbox_app_native_ui::AUDIO_ZONES_CONTENT_ID {
                        continue;
                    }
                    node.children.push(Container {
                        str_id: Some(id),
                        ..Default::default()
                    });
                }
                return true;
            }
            node.children
                .iter_mut()
                .any(|child| insert_slots(child, output))
        }
        // These outputs belong to different routes. Model each route's existing
        // named slot, while retaining actual output IDs and unmodified shell.
        assert!(insert_slots(&mut root, &output));
        let sibling = key_for(&root, |node| node.str_id.as_deref() == Some("untouched"));
        let mut document = ViewState::from_root(root.clone()).unwrap();
        document
            .apply_response(
                View::builder().with_fragments(output).build(),
                None,
                SwapStrategy::This,
            )
            .unwrap();
        let after = document.root();
        assert_eq!(
            key_for(after, |node| node.str_id.as_deref() == Some("untouched")),
            sibling
        );
        for (width, height) in [(80, 24), (120, 40), (140, 60)] {
            let opts = LayoutOptions {
                viewport: Size::new(width, height),
                ..Default::default()
            };
            let mut controls = Controls::new();
            controls.reconcile(&root);
            let before = layout(&root, &opts, &controls.measurer(&root)).unwrap();
            controls.reconcile(after);
            let tree = layout(after, &opts, &controls.measurer(after)).unwrap();
            for element in [Element::Aside, Element::Main, Element::Footer] {
                let old_key = key_for(&root, |node| node.element == element);
                let new_key = key_for(after, |node| node.element == element);
                assert_eq!(
                    before.get(&old_key).unwrap().bounds,
                    tree.get(&new_key).unwrap().bounds
                );
            }
            assert_eq!(
                before.get(&sibling).unwrap().bounds,
                tree.get(&sibling).unwrap().bounds
            );
        }
    }
}

#[test]
fn unsolicited_modal_fragments_target_loading_slots_before_routes_settle() {
    use moosicbox_app_native_ui::{AUDIO_ZONES_CONTENT_ID, PLAYBACK_SESSIONS_CONTENT_ID};
    moosicbox_app_native::register_api_sources();
    let root: Container = albums::albums(&State::default(), &[], AlbumSort::NameAsc, None).into();
    // No route resolution or manual loader replacement occurs before callbacks.
    for id in [AUDIO_ZONES_CONTENT_ID, PLAYBACK_SESSIONS_CONTENT_ID] {
        let slot = key_for(&root, |node| {
            node.str_id.as_deref() == Some(id) && node.route.is_some()
        });
        assert_eq!(slot, id);
    }
    let mut document = ViewState::from_root(root.clone()).unwrap();
    let audio = moosicbox_app_native_ui::audio_zones::audio_zones(&[], &[]);
    document
        .apply_response(
            View::builder().with_fragments(audio).build(),
            None,
            SwapStrategy::This,
        )
        .unwrap();
    let sessions = moosicbox_app_native_ui::playback_sessions::playback_sessions(
        "http://example.invalid",
        &[],
    );
    document
        .apply_response(
            View::builder().with_fragments(sessions).build(),
            None,
            SwapStrategy::This,
        )
        .unwrap();
    let after = document.root();
    for (width, height) in [(80, 24), (120, 40), (140, 60)] {
        let opts = LayoutOptions {
            viewport: Size::new(width, height),
            ..Default::default()
        };
        let mut controls = Controls::new();
        controls.reconcile(&root);
        let before = layout(&root, &opts, &controls.measurer(&root)).unwrap();
        controls.reconcile(after);
        let tree = layout(after, &opts, &controls.measurer(after)).unwrap();
        for element in [Element::Aside, Element::Main, Element::Footer] {
            let before_key = key_for(&root, |node| node.element == element);
            let after_key = key_for(after, |node| node.element == element);
            assert_eq!(
                before.get(&before_key).unwrap().bounds,
                tree.get(&after_key).unwrap().bounds
            );
        }
    }
}

#[test]
fn populated_session_partials_preserve_production_shell() {
    use moosicbox_app_native_ui::state::{PlaybackState, PlayerState};
    use moosicbox_session_models::{ApiSession, ApiSessionPlaylist, ApiUpdateSession};
    moosicbox_app_native::register_api_sources();
    let tracks: Vec<_> = (0_u64..40)
        .map(|index| moosicbox_music_models::api::ApiTrack {
            track_id: index.into(),
            album_id: index.into(),
            title: format!(
                "Long current playlist track title {index} — Deluxe Remastered Sessions"
            ),
            artist: "An artist with a realistic long display name".into(),
            album: "A complete remastered album with bonus tracks".into(),
            duration: 240.0,
            ..Default::default()
        })
        .collect();
    let mut state = State {
        connection: Some(moosicbox_app_models::Connection {
            name: "Fixture".into(),
            api_url: "http://example.invalid".into(),
        }),
        ..Default::default()
    };
    let root: Container = albums::albums(&state, &[], AlbumSort::NameAsc, None).into();
    let mut document = ViewState::from_root(root.clone()).unwrap();
    for position in [0, 15, 39] {
        state.player = PlayerState {
            playback: Some(PlaybackState {
                session_id: 1,
                playing: false,
                position,
                seek: 42.0,
                volume: 0.7,
                tracks: tracks.clone(),
            }),
        };
        let session = ApiSession {
            session_id: 1,
            name: "Fixture session".into(),
            active: true,
            playing: false,
            position: Some(position),
            seek: Some(42.0),
            volume: Some(0.7),
            playback_target: None,
            playlist: ApiSessionPlaylist {
                session_playlist_id: 1,
                tracks: tracks.clone(),
            },
        };
        let update = ApiUpdateSession {
            session_id: 1,
            position: Some(position),
            playing: Some(false),
            seek: Some(42.0),
            volume: Some(0.7),
            ..Default::default()
        };
        // Exact events::handle_session_update boundary: unsolicited fragments,
        // not primary content or a synthetic page replacement.
        let view = View::builder()
            .with_fragments(moosicbox_app_native_ui::session_updated(
                &state, &update, &session,
            ))
            .build();
        document
            .apply_response(view, None, SwapStrategy::This)
            .unwrap();
        // Position/playlist updates also call update_playlist_sessions(), a
        // SECOND fragment-only render. Omitting this lost the actual WS-ready
        // boundary and concealed replacement of the entire application root.
        let sessions = moosicbox_app_native_ui::playback_sessions::playback_sessions(
            "http://example.invalid",
            std::slice::from_ref(&session),
        );
        document
            .apply_response(
                View::builder().with_fragments(sessions).build(),
                None,
                SwapStrategy::This,
            )
            .unwrap();
        let after = document.root();
        assert_eq!(
            after.str_id, root.str_id,
            "second fragment render must preserve application root"
        );
        let settled: Container = albums::albums(&state, &[], AlbumSort::NameAsc, None).into();
        assert_eq!(
            after.children.len(),
            root.children.len(),
            "partials must not append shell siblings"
        );
        for (width, height) in [(80, 24), (120, 40), (140, 60)] {
            let opts = LayoutOptions {
                viewport: Size::new(width, height),
                ..Default::default()
            };
            let mut controls = Controls::new();
            controls.reconcile(after);
            let tree = layout(after, &opts, &controls.measurer(after)).unwrap();
            let mut settled_controls = Controls::new();
            settled_controls.reconcile(&settled);
            let settled_tree =
                layout(&settled, &opts, &settled_controls.measurer(&settled)).unwrap();
            let measure = controls.measurer(after);
            let decoded = |node: &Container, constraints: Constraints| {
                if matches!(node.element, Element::Image { .. }) {
                    LogicalSize::new(500, 250)
                } else {
                    measure.measure(node, constraints)
                }
            };
            let decoded_tree = layout(after, &opts, &decoded).unwrap();
            for id in [
                "player-current-playing",
                "player-play-button",
                "player-current-progress",
            ] {
                if let Some(node) = tree.get(id) {
                    assert_eq!(node.bounds, settled_tree.get(id).unwrap().bounds);
                    assert_eq!(node.bounds, decoded_tree.get(id).unwrap().bounds);
                }
            }
            let mut before_controls = Controls::new();
            before_controls.reconcile(&root);
            let before = layout(&root, &opts, &before_controls.measurer(&root)).unwrap();
            for element in [Element::Aside, Element::Main, Element::Footer] {
                let key = key_for(&root, |node| node.element == element);
                let after_key = key_for(after, |node| node.element == element);
                assert_eq!(
                    before.get(&key).unwrap().bounds,
                    tree.get(&after_key).unwrap().bounds,
                    "{width}x{height}, position {position}"
                );
            }
            let footer_key = key_for(after, |node| matches!(node.element, Element::Footer));
            let footer = tree.get(&footer_key).unwrap();
            let playing = tree.get("player-current-playing").unwrap();
            eprintln!(
                "session {position} {width}x{height}: footer={:?} current={:?}",
                footer.bounds, playing.bounds
            );
            assert!(
                playing.bounds.right() <= footer.bounds.right(),
                "current track must not displace controls"
            );
            assert!(
                playing.bounds.bottom() <= footer.bounds.bottom(),
                "current track must stay in footer"
            );
            let metadata = tree
                .nodes
                .iter()
                .filter(|node| node.path.starts_with(&playing.path))
                .map(|node| node.bounds.right())
                .max()
                .unwrap();
            assert!(
                metadata <= playing.bounds.right(),
                "track metadata overflows its footer column: {metadata} > {}",
                playing.bounds.right()
            );
        }
    }
}

fn assert_album_shell(width: u16, height: u16, columns: usize) {
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
    let playlist_key = key_for(
        &root,
        |node| matches!(&node.element, Element::Image { source: Some(source), .. } if source.ends_with("playlist-white.svg")),
    );
    let mut controls = Controls::new();
    controls.reconcile(&root);
    let opts = LayoutOptions {
        viewport: Size::new(width, height),
        ..Default::default()
    };
    let search_key = key_for(&root, |node| {
        node.str_id.as_deref() == Some("search-button")
    });
    let search_tree = layout(&root, &opts, &controls.measurer(&root)).unwrap();
    let search = search_tree.get(&search_key).unwrap().bounds;
    assert!(
        search.right() <= i64::from(width),
        "search button must fit viewport: {search:?}"
    );
    let skeleton = layout(&root, &opts, &controls.measurer(&root)).unwrap();

    // Exercise the exact dynamic response boundary. In particular, do NOT
    // assign album_display cards directly to grid.children: doing so bypasses
    // the response wrapper and previously concealed the live one-column bug.
    let page = Page::WithTotal {
        items: (0_u64..100)
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
        total: 100,
    };
    // Same Containers -> View conversion returned by albums_list_start_route.
    // Preserve its envelope: the response layer, not this test, must interpret it.
    let response: View = albums::albums_list_start(
        &state,
        &page,
        &[],
        AlbumSort::NameAsc,
        moosicbox_app_native_ui::ALBUM_ART_SIZE,
        "",
    )
    .into();
    assert_eq!(response.primary.as_ref().unwrap().children.len(), 100);
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
        let playlist = tree.get(&playlist_key).unwrap();
        assert_eq!(playlist.bounds.width, 3);
        assert_eq!(playlist.bounds.height, 2);
        assert!(playlist.bounds.x >= footer.bounds.x);
        assert!(playlist.bounds.right() <= footer.bounds.right());
        assert!(playlist.bounds.y >= footer.bounds.y);
        assert!(playlist.bounds.bottom() <= footer.bounds.bottom());
        assert_eq!(sidebar.bounds.x, 0);
        assert_eq!(sidebar.bounds.width, 30);
        assert_eq!(main.bounds.x, 30);
        assert_eq!(main.bounds.width, u64::from(width) - 30);
        assert_eq!(main.bounds.bottom(), footer.bounds.y);
        assert_eq!(sidebar.bounds.height, main.bounds.height);
        assert_eq!(footer.bounds.right(), i64::from(width));
        assert!(footer.bounds.bottom() <= i64::from(height));
        assert_eq!(
            tree.get("albums").unwrap().content_bounds.width,
            u64::from(width) - 38
        );
        eprintln!(
            "{width}x{height}: sidebar={:?} main={:?} footer={:?}",
            sidebar.bounds, main.bounds, footer.bounds
        );
    }
    for tree in [&before, &after] {
        let grid = tree.get("albums").unwrap();
        let cards: Vec<_> = grid
            .children
            .iter()
            .map(|key| tree.get(key).unwrap())
            .collect();
        assert_eq!(cards.len(), 100);
        let first = cards[0];
        let label = tree
            .nodes
            .iter()
            .find(|node| {
                node.path.starts_with(&first.path)
                    && node.text.iter().any(|fragment| fragment.text.contains('A'))
            })
            .expect("first card has title text");
        assert!(label.bounds.y >= tree.get(&main_key).unwrap().bounds.y);
        if height >= 40 {
            assert!(
                label.bounds.bottom() <= tree.get(&main_key).unwrap().bounds.bottom(),
                "a labeled album must be visible without scrolling on a tall viewport"
            );
        }
        let first_y = cards[0].bounds.y;
        assert_eq!(
            cards
                .iter()
                .take_while(|card| card.bounds.y == first_y)
                .count(),
            columns
        );
        assert_eq!(
            cards
                .iter()
                .map(|card| card.bounds.x)
                .collect::<BTreeSet<_>>()
                .len(),
            columns
        );
        for (index, card) in cards.iter().enumerate() {
            assert_eq!(card.bounds.width, 25);
            assert_eq!(card.bounds.x, cards[index % columns].bounds.x);
            assert!(card.bounds.x >= grid.content_bounds.x);
            assert!(card.bounds.right() <= grid.content_bounds.right());
        }
    }
    let mut scrolled_opts = opts.clone();
    scrolled_opts
        .scroll_offsets
        .insert(main_key.clone(), ScrollOffset { x: 0, y: 2 });
    let scrolled = layout(root, &scrolled_opts, &decoded).unwrap();
    assert_eq!(scrolled.get(&main_key).unwrap().scroll_offset.y, 2);
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
        after.get("albums").unwrap().bounds.y - 2
    );
    let main = scrolled.get(&main_key).unwrap();
    assert_eq!(main.text_clip.bottom(), main.bounds.bottom());
    for key in &scrolled.get("albums").unwrap().children {
        assert!(scrolled.get(key).unwrap().clip.bottom() <= main.bounds.bottom());
    }
}
