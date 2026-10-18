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
                    requests.lock().unwrap().push(request);
                    let (status, body): (&str, &[u8]) = if artwork {
                        ("200 OK", ARTWORK)
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
            if node.key == "name"
                || node.key == "api-url"
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
            tree.nodes.iter().any(|node| node.key == "api-url"
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
