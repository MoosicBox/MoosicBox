//! Actual native route/host acceptance; SQLite state is memory-only.
#![cfg(feature = "bmux")]

use std::{
    collections::BTreeMap,
    io::{Read, Write},
    net::TcpListener,
    sync::{
        Arc, Mutex,
        atomic::{AtomicBool, Ordering},
    },
    thread,
    time::Duration,
};

// Same RGBA quadrants served by the executable PTY mock, encoded as PNG.
const ARTWORK: &[u8] = &[
    137, 80, 78, 71, 13, 10, 26, 10, 0, 0, 0, 13, 73, 72, 68, 82, 0, 0, 0, 2, 0, 0, 0, 2, 8, 6, 0,
    0, 0, 114, 182, 13, 36, 0, 0, 0, 18, 73, 68, 65, 84, 120, 156, 99, 248, 207, 192, 240, 31, 12,
    129, 52, 24, 0, 0, 73, 200, 9, 247, 249, 171, 182, 13, 0, 0, 0, 0, 73, 69, 78, 68, 174, 66, 96,
    130,
];

struct ResourceServer {
    endpoint: String,
    stop: Arc<AtomicBool>,
    requests: Arc<Mutex<Vec<String>>>,
    worker: Option<thread::JoinHandle<()>>,
}

impl ResourceServer {
    fn start() -> Self {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        listener.set_nonblocking(true).unwrap();
        let endpoint = format!("http://{}", listener.local_addr().unwrap());
        let stop = Arc::new(AtomicBool::new(false));
        let requests = Arc::new(Mutex::new(Vec::new()));
        let worker = thread::spawn({
            let stop = Arc::clone(&stop);
            let requests = Arc::clone(&requests);
            move || {
                while !stop.load(Ordering::Acquire) {
                    let Ok((mut socket, _)) = listener.accept() else {
                        thread::sleep(Duration::from_millis(5));
                        continue;
                    };
                    socket
                        .set_read_timeout(Some(Duration::from_secs(1)))
                        .unwrap();
                    socket
                        .set_write_timeout(Some(Duration::from_secs(1)))
                        .unwrap();
                    let mut buffer = [0; 16384];
                    let Ok(count) = socket.read(&mut buffer) else {
                        continue;
                    };
                    let request = String::from_utf8_lossy(&buffer[..count]).into_owned();
                    let artwork = request.starts_with("GET /files/albums/1/64x64?");
                    let albums = request.starts_with("GET /menu/albums?");
                    requests.lock().unwrap().push(request);
                    let (status, body): (&str, &[u8]) = if artwork {
                        ("200 OK", ARTWORK)
                    } else if albums {
                        (
                            "200 OK",
                            br#"{"items":[],"offset":0,"limit":1,"total":0,"hasMore":false}"#,
                        )
                    } else {
                        ("404 Not Found", b"")
                    };
                    let header = format!(
                        "HTTP/1.1 {status}\r\nContent-Type: image/png\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",
                        body.len()
                    );
                    let _ = socket
                        .write_all(header.as_bytes())
                        .and_then(|()| socket.write_all(body));
                }
            }
        });
        Self {
            endpoint,
            stop,
            requests,
            worker: Some(worker),
        }
    }
}

impl Drop for ResourceServer {
    fn drop(&mut self) {
        self.stop.store(true, Ordering::Release);
        self.worker.take().unwrap().join().unwrap();
    }
}

use hyperchad::{
    app::AppBuilder,
    renderer::{Content, View},
    renderer_bmux::{
        actions::{RequestBody, RequestEffect, Source},
        graphics::{GraphicsSource, GraphicsStore, SourcePolicy},
        layout::{DefaultLeafMeasure, LayoutOptions, layout},
        renderer::{HostRequest, HostResponse},
    },
    transformer::models::{Selector, SwapStrategy},
};

fn content_view(content: Option<Content>) -> View {
    match content.unwrap() {
        Content::View(view) => *view,
        _ => panic!("native route did not return a view"),
    }
}

fn text(view: &View) -> String {
    layout(
        view.primary.as_ref().unwrap(),
        &LayoutOptions::default(),
        &DefaultLeafMeasure,
    )
    .unwrap()
    .nodes
    .iter()
    .flat_map(|node| &node.text)
    .map(|fragment| fragment.text.as_str())
    .collect()
}

fn has_text(node: &hyperchad::transformer::Container, expected: &str) -> bool {
    matches!(&node.element, hyperchad::transformer::Element::Text { value } | hyperchad::transformer::Element::Raw { value } if value.contains(expected))
        || node.children.iter().any(|child| has_text(child, expected))
}

fn dump_navigation(root: &hyperchad::transformer::Container, phase: &str) {
    use hyperchad::transformer::{Container, Element};
    fn visit(
        node: &Container,
        path: &mut Vec<usize>,
        tree: &hyperchad::renderer_bmux::layout::LayoutTree,
        phase: &str,
    ) {
        if matches!(node.element, Element::Anchor { .. } | Element::Input { .. }) {
            let key = hyperchad::renderer_bmux::layout::node_key(node, path);
            if let Some(resolved) = tree.get(&key) {
                eprintln!(
                    "{phase} control={:?} key={key} bounds={:?} clip={:?} scroll={:?} visible={}",
                    node.element,
                    resolved.bounds,
                    resolved.clip,
                    resolved.scroll_offset,
                    resolved.visible
                );
            }
        }
        for (index, child) in node.children.iter().enumerate() {
            path.push(index);
            visit(child, path, tree, phase);
            path.pop();
        }
    }
    fn images(node: &Container, sources: &mut BTreeMap<String, usize>) {
        if let Element::Image {
            source: Some(source),
            ..
        } = &node.element
        {
            *sources.entry(source.clone()).or_default() += 1;
        }
        for child in &node.children {
            images(child, sources);
        }
    }
    let mut sources = BTreeMap::new();
    images(root, &mut sources);
    eprintln!(
        "{phase} image_nodes={} unique_sources={} multiplicities={:?}",
        sources.values().sum::<usize>(),
        sources.len(),
        sources.values().collect::<Vec<_>>()
    );
    let mut options = LayoutOptions::default();
    options.viewport.width = 140;
    options.viewport.height = 60;
    let mut controls = hyperchad::renderer_bmux::controls::Controls::new();
    controls.reconcile(root);
    let tree = layout(root, &options, &controls.measurer(root)).unwrap();
    assert!(
        tree.nodes.iter().any(|node| node
            .text
            .iter()
            .map(|fragment| fragment.text.as_str())
            .collect::<String>()
            .contains("Albums")),
        "actual sidebar Albums anchor has geometry but no glyph fragments ({phase})"
    );
    visit(root, &mut Vec::new(), &tree, phase);
}

/// Production UI, without the native route singleton, persistence, or network.
/// Commands returned from input are inspected and deliberately never executed.
#[test]
fn production_settings_exact_mouse_targets_activate() {
    // Exact mouse geometry regression; keyboard activation is not tested here.
    use bmux_tui::{
        buffer::Buffer,
        event::{Event, MouseButton, MouseEvent, MouseEventKind},
        frame::Frame,
        geometry::{Point, Rect, Size},
        paint::PaintCx,
    };
    use bmux_tui_runtime::PresentReport;
    use hyperchad::renderer_bmux::{
        controls::{ControlEvent, Controls},
        presentation::EmbeddedSurface,
        renderer::{BmuxConfig, BmuxRenderer},
    };

    fn commit(surface: &mut EmbeddedSurface) -> Buffer {
        let ticket = surface.prepare(Size::new(140, 60)).unwrap();
        let mut buffer = Buffer::empty(Rect::new(0, 0, 140, 60));
        let mut frame = Frame::new(&mut buffer);
        surface
            .paint(ticket, &mut PaintCx::new(&mut frame))
            .unwrap();
        // Never run resource/read commands: this test owns no external effects.
        drop(surface.commit(ticket, PresentReport::default()).unwrap());
        buffer
    }

    let root: hyperchad::transformer::Container = moosicbox_app_native_ui::settings::settings(
        &moosicbox_app_native_ui::state::State::default(),
        "MoosicBox",
        &[],
        None,
        &[],
    )
    .into();
    let mut controls = Controls::new();
    controls.reconcile(&root);
    let tree = layout(
        &root,
        &LayoutOptions {
            viewport: Size::new(140, 60),
            ..LayoutOptions::default()
        },
        &controls.measurer(&root),
    )
    .unwrap();
    fn find(
        node: &hyperchad::transformer::Container,
        path: &mut Vec<usize>,
        found: &mut Vec<(String, String)>,
    ) {
        let key = hyperchad::renderer_bmux::layout::node_key(node, path);
        if matches!(&node.element, hyperchad::transformer::Element::Anchor { href: Some(href), .. } if href == "/settings")
        {
            found.push(("gear".into(), key.clone()));
        }
        if matches!(node.element, hyperchad::transformer::Element::Button { .. })
            && has_text(node, "New Connection")
        {
            found.push(("new".into(), key));
        }
        for (index, child) in node.children.iter().enumerate() {
            path.push(index);
            find(child, path, found);
            path.pop();
        }
    }
    let mut targets = Vec::new();
    find(&root, &mut Vec::new(), &mut targets);
    assert_eq!(targets.len(), 2);
    let mut outcomes = Vec::new();
    for (label, key) in targets {
        let point = if label == "gear" {
            Point::new(17, 1)
        } else {
            Point::new(80, 11)
        };
        let node = tree.get(&key).unwrap();
        eprintln!(
            "{label}: key={key} bounds={:?} content={:?} clip={:?} regions={:?} top={:?}",
            node.bounds,
            node.content_bounds,
            node.clip,
            node.hit_regions,
            tree.hit_test(i64::from(point.x), i64::from(point.y))
                .map(|hit| &hit.key)
        );
        controls.prepare(&tree);
        controls.focus(None);
        let down = controls.event(&Event::Mouse(MouseEvent::new(
            MouseEventKind::Down(MouseButton::Left),
            point,
        )));
        let up = controls.event(&Event::Mouse(MouseEvent::new(
            MouseEventKind::Up(MouseButton::Left),
            point,
        )));
        eprintln!("{label}: direct down={:?} up={:?}", down.events, up.events);
        let focus = controls.focus(Some(&key));
        eprintln!("{label}: programmatic focus={:?}", focus.events);

        let (action_tx, _action_rx) = flume::unbounded();
        let (resize_tx, _resize_rx) = flume::unbounded();
        let renderer = BmuxRenderer::new(BmuxConfig::default(), action_tx, resize_tx);
        let mut surface = renderer.into_embedded().unwrap();
        surface
            .program
            .apply_view(View::from(root.clone()), None)
            .unwrap();
        commit(&mut surface);
        let down_update = surface
            .event(Event::Mouse(MouseEvent::new(
                MouseEventKind::Down(MouseButton::Left),
                point,
            )))
            .unwrap();
        // Match the PTY harness: a frame may be committed between down and up.
        commit(&mut surface);
        let up_update = surface
            .event(Event::Mouse(MouseEvent::new(
                MouseEventKind::Up(MouseButton::Left),
                point,
            )))
            .unwrap();
        eprintln!(
            "{label}: embedded down_commands={} up_commands={} focused={:?}",
            down_update.commands.len(),
            up_update.commands.len(),
            surface.program.controls.focused()
        );
        assert!(
            !down_update.commands.is_empty(),
            "initial resource work is deferred"
        );
        assert_eq!(
            up_update.commands.len(),
            if label == "gear" { 2 } else { 1 }
        );
        assert_eq!(surface.program.controls.focused(), Some(key.as_str()));
        outcomes.push((label, key.clone(), up.events.iter().any(|event| matches!(event, ControlEvent::Activate { key: activated } if activated == &key)) || up.events.iter().any(|event| matches!(event, ControlEvent::Navigate { .. }))));
    }
    // Diagnose both production Save controls with one real connection, without
    // routing requests or changing persistent application state.
    let connection = moosicbox_app_models::Connection {
        name: "New connection".into(),
        api_url: "http://127.0.0.1:1".into(),
    };
    let connected: hyperchad::transformer::Container = moosicbox_app_native_ui::settings::settings(
        &moosicbox_app_native_ui::state::State::default(),
        "MoosicBox",
        std::slice::from_ref(&connection),
        Some(&connection),
        &[],
    )
    .into();
    let mut connected_controls = Controls::new();
    connected_controls.reconcile(&connected);
    let connected_tree = layout(
        &connected,
        &LayoutOptions {
            viewport: Size::new(140, 60),
            ..LayoutOptions::default()
        },
        &connected_controls.measurer(&connected),
    )
    .unwrap();
    fn dump_saves(
        node: &hyperchad::transformer::Container,
        path: &mut Vec<usize>,
        tree: &hyperchad::renderer_bmux::layout::LayoutTree,
        controls: &Controls,
    ) -> usize {
        let mut count = 0;
        if matches!(node.element, hyperchad::transformer::Element::Button { .. })
            && has_text(node, "Save")
        {
            count += 1;
            let key = hyperchad::renderer_bmux::layout::node_key(node, path);
            let resolved = tree.get(&key).unwrap();
            let bounds = resolved.content_bounds;
            let native = controls
                .measure(
                    &key,
                    bmux_tui::component::Constraints::new(
                        bounds.width,
                        bounds.width,
                        bounds.height,
                        Some(bounds.height),
                    ),
                )
                .unwrap();
            eprintln!(
                "SAVE {key} bounds={:?} content={:?} clip={:?} regions={:?} native={native:?} painted={:?}",
                resolved.bounds,
                bounds,
                resolved.clip,
                resolved.hit_regions,
                bounds.intersection(resolved.clip)
            );
            let mut parent = resolved.parent_key.as_deref();
            while let Some(parent_key) = parent {
                let ancestor = tree.get(parent_key).unwrap();
                eprintln!(
                    "  ancestor {parent_key} content={:?} extents={:?} clip={:?}",
                    ancestor.content_bounds, ancestor.content_extents, ancestor.clip
                );
                parent = ancestor.parent_key.as_deref();
            }
            assert_eq!(native.width, bounds.width);
            assert_eq!(native.height, bounds.height);
        }
        for (index, child) in node.children.iter().enumerate() {
            path.push(index);
            count += dump_saves(child, path, tree, controls);
            path.pop();
        }
        count
    }
    assert_eq!(
        dump_saves(
            &connected,
            &mut Vec::new(),
            &connected_tree,
            &connected_controls
        ),
        2
    );
    fn row(buffer: &Buffer, y: u16) -> String {
        (125..137)
            .map(|x| buffer.get(Point::new(x, y)).unwrap().symbol.as_str())
            .collect()
    }
    connected_controls.prepare(&connected_tree);
    let mut standalone = Buffer::empty(Rect::new(0, 0, 140, 60));
    {
        let mut frame = Frame::new(&mut standalone);
        for node in &connected_tree.nodes {
            connected_controls.paint(&node.key, &mut PaintCx::new(&mut frame));
        }
    }
    let (action_tx, _action_rx) = flume::unbounded();
    let (resize_tx, _resize_rx) = flume::unbounded();
    let mut painted_surface = BmuxRenderer::new(BmuxConfig::default(), action_tx, resize_tx)
        .into_embedded()
        .unwrap();
    painted_surface
        .program
        .apply_view(View::from(connected), None)
        .unwrap();
    let embedded = commit(&mut painted_surface);
    eprintln!(
        "SAVE paint standalone={:?} embedded={:?}",
        row(&standalone, 4),
        row(&embedded, 4)
    );
    // Standalone native controls deliberately omit later source backgrounds.
    // The shared fixed search button is painted after the top Save and covers
    // its suffix. This asserts layer ordering, not real-app Settings acceptance.
    let cover = connected_tree
        .nodes
        .iter()
        .find(|node| node.key == "search-button")
        .unwrap();
    let mut cover_source = painted_surface.program.state.root();
    for &index in &cover.path {
        cover_source = &cover_source.children[index];
    }
    assert_eq!(
        cover_source.position,
        Some(hyperchad::transformer::models::Position::Fixed)
    );
    let background = cover_source.background.unwrap();
    assert_eq!((background.r, background.g, background.b), (255, 255, 255));
    assert_eq!(
        cover.bounds,
        hyperchad::renderer_bmux::layout::CellRect {
            x: 130,
            y: 1,
            width: 7,
            height: 5
        }
    );
    let save = connected_tree.get("@0/0/1/0/0/0/0/1").unwrap();
    assert!(cover.paint_order > save.paint_order);
    assert!(row(&standalone, 4).contains("[ Save ]"));
    for x in 125..137 {
        let point = Point::new(x, 4);
        let cell = embedded.get(point).unwrap();
        if cover.bounds.contains(i64::from(x), 4) {
            assert_eq!(cell.symbol, " ");
            assert_eq!(
                cell.style.bg,
                Some(bmux_tui::style::Color::Rgb(255, 255, 255))
            );
        } else {
            assert_eq!(cell.symbol, standalone.get(point).unwrap().symbol);
        }
    }
    // Native Settings uses get_connection_name().unwrap_or_default(); a fresh
    // app has None, so also exercise the actual empty initial display value.
    let initial: hyperchad::transformer::Container = moosicbox_app_native_ui::settings::settings(
        &moosicbox_app_native_ui::state::State::default(),
        "",
        std::slice::from_ref(&connection),
        Some(&connection),
        &[],
    )
    .into();
    painted_surface
        .program
        .apply_view(View::from(initial), None)
        .unwrap();
    let actual_initial = commit(&mut painted_surface);
    eprintln!("SAVE paint empty-initial={:?}", row(&actual_initial, 4));
    assert_eq!(row(&actual_initial, 4), row(&embedded, 4));
    for (label, key, mouse) in outcomes {
        assert!(
            mouse,
            "production {label} {key}: exact painted mouse target must activate too"
        );
    }
}

#[test]
fn actual_home_settings_connection_forms_and_registered_icons() {
    let runtime = switchy::unsync::runtime::Builder::new().build().unwrap();
    runtime.block_on(async {
        let state = moosicbox_app_state::AppState::new()
            .with_persistence_in_memory()
            .await
            .unwrap();
        moosicbox_app_native::STATE_LOCK.set(state).unwrap();
        let router = moosicbox_app_native::init();
        let mut builder = AppBuilder::new()
            .with_router(router.clone())
            .with_runtime_handle(runtime.handle());
        for asset in moosicbox_app_native::assets::ASSETS.iter().cloned() {
            builder = builder.with_static_asset_route_result(asset).unwrap();
        }
        // Exercise the production builder, not a hand-built renderer/host callback.
        let app = builder.build_default_bmux().unwrap();
        assert!(
            moosicbox_app_native::RENDERER
                .set(Box::new(app.renderer.clone()))
                .is_ok()
        );
        let mut surface = app.renderer.clone().into_embedded().unwrap();
        let request = surface
            .program
            .config()
            .host
            .request
            .as_ref()
            .unwrap()
            .clone();
        let home = content_view(router.navigate("/").await.unwrap());
        assert!(text(&home).contains("Configure a server connection in Settings"));
        dump_navigation(home.primary.as_ref().unwrap(), "home-before-focus");
        let settings = content_view(router.navigate("/settings").await.unwrap());
        dump_navigation(settings.primary.as_ref().unwrap(), "settings-before-focus");
        assert!(text(&settings).contains("Connections"));
        assert!(
            has_text(settings.primary.as_ref().unwrap(), "New Connection"),
            "actual Settings route missing creation control"
        );

        let submit = |url: &str, method: &str, body: &str| {
            HostRequest::Http(Box::new(RequestEffect {
                source: Source {
                    key: "settings-connections".into(),
                    path: Vec::new(),
                    generation: 0,
                },
                url: url.into(),
                base_url: "/settings".into(),
                method: method.into(),
                body: RequestBody::UrlEncoded(body.into()),
                headers: BTreeMap::from([(
                    "content-type".into(),
                    "application/x-www-form-urlencoded".into(),
                )]),
                target: Selector::Id("settings-connections".into()),
                strategy: SwapStrategy::This,
                request_id: None,
                navigation: false,
            }))
        };
        let created = request(submit("/settings/new-connection", "POST", ""))
            .await
            .unwrap();
        let HostResponse::Route {
            status: 200,
            view: Some(created_view),
            ..
        } = created
        else {
            panic!("creation did not return a native view")
        };
        surface.program.apply_view(settings, None).unwrap();
        surface
            .program
            .apply_response(
                created_view,
                Some("settings-connections"),
                SwapStrategy::This,
            )
            .unwrap();
        let tree = layout(
            surface.program.state.root(),
            &LayoutOptions::default(),
            &surface
                .program
                .controls
                .measurer(surface.program.state.root()),
        )
        .unwrap();
        for node in &tree.nodes {
            if node.key.starts_with("connection-name-")
                || node.key.starts_with("connection-api-url-")
                || node.text.iter().any(|text| text.text.contains("API URL"))
            {
                eprintln!(
                    "connection layout key={} bounds={:?} clip={:?} visible={}",
                    node.key, node.bounds, node.clip, node.visible
                );
            }
        }
        assert!(
            has_text(surface.program.state.root(), "API URL:"),
            "successful response lost API URL canonical label"
        );
        assert!(
            tree.nodes
                .iter()
                .any(|node| node.key.starts_with("connection-api-url-")
                    && node.visible
                    && node.bounds.intersection(node.clip).width > 0),
            "API URL input not visible after actual response"
        );
        dump_navigation(surface.program.state.root(), "settings-after-response");
        let server = ResourceServer::start();
        let body = format!("name=Acceptance&api-url={}", server.endpoint);
        request(submit(
            "/settings/connections?name=New%20connection",
            "PATCH",
            &body,
        ))
        .await
        .unwrap();
        let connections = moosicbox_app_native::STATE.get_connections().await.unwrap();
        assert_eq!(connections.len(), 1);
        assert_eq!(connections[0].name, "Acceptance");
        assert_eq!(connections[0].api_url, server.endpoint);
        request(submit(
            "/settings/select-connection?name=Acceptance",
            "POST",
            "",
        ))
        .await
        .unwrap();
        // Exercise HTTP fragments independently of WebSocket readiness; a slow or
        // reconnecting socket must not strand the one-shot load placeholder.
        for status in [
            moosicbox_app_models::ConnectionStatus::Connecting,
            moosicbox_app_models::ConnectionStatus::Reconnecting { attempt: 1 },
        ] {
            *moosicbox_app_native::STATE.connection_status.write().await = status;
            let response = router
                .navigate(hyperchad::router::RouteRequest::from_path(
                    "/albums-list-start?limit=1&size=64&sort=name-asc",
                    Default::default(),
                ))
                .await
                .unwrap();
            let view = content_view(response);
            assert!(view.primary.is_some());
        }
        let albums = content_view(router.navigate("/albums").await.unwrap());
        dump_navigation(albums.primary.as_ref().unwrap(), "albums-route");
        let artwork_request = moosicbox_app_native::renderer_bmux::config()
            .host
            .request
            .unwrap();
        let source = format!("{}/files/albums/1/64x64?source=LIBRARY", server.endpoint);
        let HostResponse::Bytes(artwork) = artwork_request(HostRequest::Resource {
            source,
            max_bytes: 1024,
        })
        .await
        .unwrap() else {
            panic!("selected connection artwork did not return bytes")
        };
        assert_eq!(artwork, ARTWORK);
        assert!(
            server
                .requests
                .lock()
                .unwrap()
                .iter()
                .any(|request| request.starts_with("GET /files/albums/1/64x64?")
                    && request.contains("moosicboxProfile=master"))
        );

        let mut graphics = GraphicsStore::new(Default::default());
        let policy = SourcePolicy {
            allow_memory: true,
            ..SourcePolicy::default()
        };
        graphics
            .decode("actual-artwork", &GraphicsSource::Memory, &policy, &artwork)
            .unwrap();
        assert_eq!(
            graphics.rgba("actual-artwork").unwrap(),
            (
                2,
                2,
                &[
                    255, 0, 0, 255, 0, 255, 0, 255, 0, 0, 255, 255, 255, 255, 255, 255
                ][..]
            )
        );
        for path in [
            "/public/img/icon128.png",
            "/public/img/settings-gear-white.svg",
            "/public/img/play-button-white.svg",
        ] {
            let HostResponse::Bytes(bytes) = request(HostRequest::Resource {
                source: path.into(),
                max_bytes: 1024 * 1024,
            })
            .await
            .unwrap() else {
                panic!("registered asset must return bytes")
            };
            graphics
                .decode(path, &GraphicsSource::Memory, &policy, &bytes)
                .unwrap();
            let (width, height, rgba) = graphics.rgba(path).unwrap();
            assert!(width > 0 && height > 0);
            assert!(
                rgba.as_chunks::<4>().0.iter().any(|pixel| pixel[3] > 0),
                "blank decoded asset: {path}"
            );
            if path.ends_with("white.svg") {
                assert!(
                    rgba.as_chunks::<4>().0.iter().any(|pixel| pixel[0] > 200
                        && pixel[1] > 200
                        && pixel[2] > 200
                        && pixel[3] > 200),
                    "white icon lost against dark background: {path}"
                );
            }
        }
    });
}
