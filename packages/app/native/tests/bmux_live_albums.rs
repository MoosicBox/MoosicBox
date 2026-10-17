//! Opt-in, read-only acceptance against a real server; persistence stays in memory.
#![cfg(feature = "bmux")]

use hyperchad::{
    renderer::Content,
    router::RouteRequest,
    transformer::{Container, Element},
};
use moosicbox_app_models::{Connection, ConnectionStatus};

fn text(node: &Container, output: &mut String) {
    if let Element::Raw { value } = &node.element {
        output.push_str(value);
        output.push('\n');
    }
    for child in &node.children {
        text(child, output);
    }
}

#[test]
#[ignore = "requires explicit MOOSICBOX_TEST_SERVER; issues read-only library GET requests"]
fn live_albums_load_while_websocket_is_connecting_or_reconnecting() {
    let endpoint = std::env::var("MOOSICBOX_TEST_SERVER").expect("explicit test server");
    moosicbox_app_native::register_api_sources();
    let runtime = switchy::unsync::runtime::Builder::new().build().unwrap();
    runtime.block_on(async {
        let state = moosicbox_app_state::AppState::new()
            .with_persistence_in_memory()
            .await
            .unwrap();
        // Do not activate background players or WebSocket registration on the real server.
        state
            .set_current_connection(&Connection {
                name: "Read-only acceptance".into(),
                api_url: endpoint.clone(),
            })
            .await
            .unwrap();
        *state.api_url.write().await = Some(endpoint.clone());
        *state.connection_config.write().await = Some(moosicbox_app_state::ConnectionConfig::new(&endpoint, "master", "read-only-test").unwrap());
        *state.connection_id.write().await = Some("read-only-test".into());
        *state.profile.write().await = Some("master".into());
        moosicbox_app_native::STATE_LOCK.set(state.clone()).unwrap();
        let router = moosicbox_app_native::init();
        fn has_album_link(node: &Container) -> bool {
            matches!(&node.element, Element::Anchor { href: Some(href), .. } if href.contains("albumId="))
                || node.children.iter().any(has_album_link)
        }
        for status in [
            ConnectionStatus::Connecting,
            ConnectionStatus::Reconnecting { attempt: 2 },
        ] {
            *state.connection_status.write().await = status;
            let start = std::time::Instant::now();
            let content = router
                .navigate(RouteRequest::from_path(
                    "/albums-list-start?limit=100&size=64&sort=name-asc",
                    Default::default(),
                ))
                .await
                .unwrap()
                .unwrap();
            let Content::View(view) = content else {
                panic!("expected album view")
            };
            let root = view.primary.as_ref().unwrap();
            let mut rendered = String::new();
            text(root, &mut rendered);
            assert!(!rendered.contains("Connecting to server"));
            assert!(!rendered.contains("Reconnecting to server"));
            assert!(has_album_link(root), "real album must contain a detail link");
            eprintln!("Live album fragment loaded in {:?}", start.elapsed());
            fn artwork(node: &Container) -> Option<String> {
                if let Element::Image { source: Some(source), .. } = &node.element
                    && source.contains("/files/albums/") { return Some(source.clone()); }
                node.children.iter().find_map(artwork)
            }
            let source = artwork(root).expect("first page has real artwork");
            let callback = moosicbox_app_native::renderer_bmux::config().host.request.unwrap();
            let started = std::time::Instant::now();
            let hyperchad::renderer_bmux::renderer::HostResponse::Bytes(bytes) = callback(
                hyperchad::renderer_bmux::renderer::HostRequest::Resource { source, max_bytes: 1024*1024 },
            ).await.unwrap() else { panic!("artwork bytes") };
            assert!(bytes.len() > 100);
            eprintln!("Live artwork: {} bytes in {:?}", bytes.len(), started.elapsed());
        }
    });
}
