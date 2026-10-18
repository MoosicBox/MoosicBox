//! Opt-in, read-only acceptance against a real server; persistence stays in memory.
#![cfg(feature = "bmux")]

use hyperchad::{
    renderer::Content,
    router::RouteRequest,
    transformer::{Container, Element},
};
use moosicbox_app_models::{Connection, ConnectionStatus};
use moosicbox_music_models::api::{ApiAlbum, ApiTrack};
use serde_json::Value;

fn visit<'a>(node: &'a Container, predicate: &impl Fn(&Container) -> bool) -> Vec<&'a Container> {
    let mut found = Vec::new();
    if predicate(node) {
        found.push(node);
    }
    for child in &node.children {
        found.extend(visit(child, predicate));
    }
    found
}

fn href(node: &Container) -> Option<&str> {
    if let Element::Anchor { href, .. } = &node.element {
        href.as_deref()
    } else {
        None
    }
}

async fn get_json(client: &switchy::http::Client, url: &str) -> Value {
    let response = client.get(url).send().await.unwrap();
    assert!(
        response.status().is_success(),
        "GET {url}: {}",
        response.status()
    );
    response.json().await.unwrap()
}

async fn app_view(router: &hyperchad::router::Router, path: &str) -> Container {
    let content = router
        .navigate(RouteRequest::from_path(path, Default::default()))
        .await
        .unwrap()
        .unwrap();
    let Content::View(view) = content else {
        panic!("expected app view for {path}")
    };
    view.primary.unwrap()
}

#[test]
fn album_card_artist_and_album_links_preserve_provider_identity() {
    moosicbox_app_native::register_api_sources();
    let album = ApiAlbum {
        album_id: "provider-album".into(),
        artist_id: "provider-artist".into(),
        title: "Provider album".into(),
        artist: "Provider artist".into(),
        api_source: serde_json::from_value(serde_json::json!("Qobuz")).unwrap(),
        ..Default::default()
    };
    let cards = moosicbox_app_native_ui::albums::album_display(
        "http://example.invalid",
        &album,
        64,
        true,
        false,
    );
    let artist_url = format!(
        "/artists?artistId={}&source={}",
        album.artist_id, album.api_source
    );
    let album_url = moosicbox_app_native_ui::albums::album_page_url(
        &album.album_id.to_string(),
        false,
        Some(&album.api_source),
        None,
        None,
        None,
    );
    let mut links = Vec::new();
    for card in &cards {
        links.extend(visit(card, &|node| href(node).is_some()));
    }
    assert_eq!(
        links
            .iter()
            .filter(|node| href(node) == Some(artist_url.as_str()))
            .count(),
        1
    );
    assert_eq!(
        links
            .iter()
            .filter(|node| href(node) == Some(album_url.as_str()))
            .count(),
        2,
        "cover and title must navigate to the same source-qualified album"
    );
}

#[test]
fn terminal_album_pagination_requires_click_and_retains_filters() {
    use hyperchad::transformer::models::{Route, SwapStrategy};
    use moosicbox_music_models::{AlbumSort, TrackApiSource};
    moosicbox_app_native::register_api_sources();
    let state = moosicbox_app_native_ui::state::State {
        connection: Some(Connection {
            name: "Offline pagination".into(),
            api_url: "http://example.invalid".into(),
        }),
        ..Default::default()
    };
    for (offset, total, has_next) in [
        (0_u32, 2006_u32, true),
        (100, 2006, true),
        (2000, 2006, false),
    ] {
        let count = (total - offset).min(100);
        let page = moosicbox_paging::Page::WithTotal {
            items: (0..count)
                .map(|index| ApiAlbum {
                    album_id: u64::from(offset + index).into(),
                    ..Default::default()
                })
                .collect(),
            offset,
            limit: 100,
            total,
        };
        let root: Container = moosicbox_app_native_ui::albums::albums_list_page(
            &state,
            &page,
            &[TrackApiSource::Local],
            AlbumSort::NameDesc,
            200,
            "A & B+é",
        )
        .into();
        let requests = visit(&root, &|node| node.route.is_some());
        assert_eq!(
            requests.len(),
            usize::from(has_next),
            "never eagerly drain remaining pages"
        );
        assert_eq!(
            visit(&root, &|node| href(node)
                .is_some_and(|url| url.contains("albumId=")))
            .len(),
            usize::try_from(count).unwrap() * 2
        );
        assert!(
            visit(&root, &|node| node.str_id.is_some()).is_empty(),
            "appended batches must not duplicate IDs"
        );
        if has_next {
            assert!(matches!(requests[0].element, Element::Button { .. }));
            let Some(Route::Get {
                route,
                trigger,
                strategy,
                ..
            }) = &requests[0].route
            else {
                panic!("GET next page")
            };
            assert_eq!(trigger.as_deref(), Some("click"));
            assert_eq!(strategy, &SwapStrategy::This);
            let request = RouteRequest::from_path(route, hyperchad::router::RequestInfo::default());
            for (key, expected) in [
                ("offset", (offset + 100).to_string()),
                ("limit", "100".into()),
                ("size", "200".into()),
                ("sources", "LOCAL".into()),
                ("sort", AlbumSort::NameDesc.to_string()),
                ("search", "A & B+é".into()),
            ] {
                assert_eq!(request.query.get(key), Some(&expected), "preserve {key}");
            }
        }
    }
}

fn assert_track_table(root: &Container, expected_tracks: usize) {
    let tables = visit(root, &|node| matches!(node.element, Element::Table));
    assert_eq!(tables.len(), 1, "one shared table must align every track");
    let table = tables[0];
    assert_eq!(table.children.len(), 2);
    let header = &table.children[0];
    let body = &table.children[1];
    assert!(matches!(header.element, Element::THead));
    assert!(matches!(body.element, Element::TBody));
    assert_eq!(header.children.len(), 1);
    assert!(matches!(header.children[0].element, Element::TR));
    let headings = &header.children[0].children;
    assert_eq!(headings.len(), 4);
    for (cell, label) in headings.iter().zip(["#", "Title", "Artist", "Time"]) {
        assert!(matches!(cell.element, Element::TH { .. }));
        let mut value = String::new();
        text(cell, &mut value);
        assert_eq!(value.trim(), label);
    }
    assert_eq!(body.children.len(), expected_tracks);
    for row in &body.children {
        assert!(matches!(row.element, Element::TR));
        assert!(row.data.contains_key("track-id"));
        assert!(
            !row.actions.is_empty(),
            "track hover and playback events survive"
        );
        assert_eq!(row.children.len(), 4);
        assert!(
            row.children
                .iter()
                .all(|cell| matches!(cell.element, Element::TD { .. }))
        );
        assert!(
            !visit(&row.children[0], &|node| matches!(
                node.element,
                Element::Button { .. }
            ) && !node.actions.is_empty())
            .is_empty(),
            "track playback action survives"
        );
    }
}

#[test]
fn album_detail_uses_shared_table_columns_for_unequal_track_text() {
    moosicbox_app_native::register_api_sources();
    let state = moosicbox_app_native_ui::state::State {
        connection: Some(Connection {
            name: "Offline table regression".into(),
            api_url: "http://example.invalid".into(),
        }),
        ..Default::default()
    };
    let tracks = [
        ApiTrack {
            track_id: 1.into(),
            number: 1,
            title: "Short".into(),
            artist: "A very long artist name".into(),
            ..Default::default()
        },
        ApiTrack {
            track_id: 2.into(),
            number: 12,
            title: "A substantially longer second track title".into(),
            artist: "B".into(),
            ..Default::default()
        },
    ];
    let versions = [serde_json::from_value(serde_json::json!({
        "tracks": tracks, "format": null, "bitDepth": null,
        "sampleRate": null, "channels": null, "source": "LOCAL",
    }))
    .unwrap()];
    let root: Container = moosicbox_app_native_ui::albums::album_page_content(
        &state,
        &ApiAlbum::default(),
        &versions,
        None,
    )
    .into();
    assert_track_table(&root, 2);
    let rows = visit(&root, &|node| node.data.contains_key("track-id"));
    assert_eq!(rows[0].data["track-id"], "1");
    assert_eq!(rows[1].data["track-id"], "2");

    // A pointer left over the newly loaded table immediately triggers these real
    // app hover actions. Their controls live inside the first cell, not directly
    // under the row; dispatch must not fail with Missing("action target").
    use hyperchad::renderer_bmux::actions::{
        ActionEngine, EventContext, EventKind, FormValues, Source,
    };
    use hyperchad::transformer::models::Visibility;
    for row in rows {
        let mut row = row.clone();
        let source = Source::at(&row, vec![], 0).unwrap();
        let event = EventContext::new(source, EventKind::Hover, 0);
        let mut engine = ActionEngine::default();
        engine
            .dispatch(&mut row, &event, &FormValues::new())
            .unwrap();
        for (class, expected) in [
            ("track-number", Visibility::Hidden),
            ("track-playing", Visibility::Hidden),
            ("play-button", Visibility::Visible),
        ] {
            let controls = visit(&row, &|node| {
                node.classes.iter().any(|value| value == class)
            });
            assert_eq!(controls.len(), 1);
            assert_eq!(controls[0].visibility, Some(expected));
        }
        engine.release(&mut row, &event).unwrap();
        let play = visit(&row, &|node| {
            node.classes.iter().any(|value| value == "play-button")
        });
        assert_eq!(play[0].visibility, Some(Visibility::Hidden));
    }
}

fn text(node: &Container, output: &mut String) {
    if let Element::Raw { value } | Element::Text { value } = &node.element {
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
        // Match production's cached remote adapter registry, but do not activate
        // connection services (player registration / WebSocket setup are writes).
        // SourceToRemoteLibrary cannot be constructed here: its fields are private
        // and the helper is gated on UPnP, which terminal clients need not enable.
        let apis = moosicbox_music_models::ApiSource::all()
            .map(|source| {
                let api: std::sync::Arc<Box<dyn moosicbox_music_api::MusicApi>> =
                    std::sync::Arc::new(Box::new(moosicbox_music_api::CachedMusicApi::new(
                        moosicbox_remote_library::RemoteLibraryMusicApi::new(
                            endpoint.clone(), source.clone(), "master".into(),
                        ),
                    )));
                (source, api)
            })
            .collect::<std::collections::BTreeMap<_, _>>();
        moosicbox_music_api::profiles::PROFILES.upsert("master".into(), std::sync::Arc::new(apis));
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
                    "/albums-list-start?limit=2000&size=64&sort=name-asc",
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
            let detail_links = visit(root, &|node| href(node).is_some_and(|url| url.contains("albumId=")));
            assert!(detail_links.len() <= 200, "oversized requested limit must still render at most 100 cards");
            let pagination = visit(root, &|node| node.route.is_some());
            assert_eq!(pagination.len(), 1, "real first page exposes only one demand-driven successor");
            assert!(matches!(
                &pagination[0].route,
                Some(hyperchad::transformer::models::Route::Get { trigger, route, .. })
                    if trigger.as_deref() == Some("click") && route.contains("offset=100") && route.contains("limit=100")
            ));
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

        // Exercise the application's real two-stage detail route, not a template
        // rendered directly from the oracle response. Never activate the connection:
        // that would register players / open a WebSocket on the real server.
        let client = switchy::http::Client::builder().build().unwrap();
        let albums = get_json(
            &client,
            &format!("{endpoint}/menu/albums?moosicboxProfile=master&offset=0&limit=5&sort=name"),
        ).await;
        let albums = albums["items"].as_array().expect("album page items");
        assert!(!albums.is_empty(), "live fixture must contain albums");
        for value in albums {
            let album: ApiAlbum = serde_json::from_value(value.clone()).unwrap();
            let path = moosicbox_app_native_ui::albums::album_page_url(
                &album.album_id.to_string(), false, Some(&album.api_source), None, None, None,
            );
            let skeleton = app_view(&router, &path).await;
            let loads = visit(&skeleton, &|node| matches!(
                &node.route,
                Some(hyperchad::transformer::models::Route::Get { route, trigger, .. })
                    if route.contains("full=true") && trigger.as_deref() == Some("load")
            ));
            assert_eq!(loads.len(), 1, "detail must schedule exactly one full-content load");
            let Some(hyperchad::transformer::models::Route::Get { route, .. }) = &loads[0].route else {
                unreachable!()
            };
            let expected_full_path = moosicbox_app_native_ui::albums::album_page_url(
                &album.album_id.to_string(), true, Some(&album.api_source), None, None, None,
            );
            assert_eq!(route, &expected_full_path,
                "skeleton must retain album source and identity");
            // Also follow the skeleton's exact load URL to verify the default
            // selection, independently of the explicit version selector below.
            let default_detail = app_view(&router, route).await;
            let versions = get_json(&client, &format!(
                "{endpoint}/menu/album/versions?moosicboxProfile=master&albumId={}&source={}",
                album.album_id, album.api_source,
            )).await;
            let versions = versions.as_array().expect("versions endpoint returns a plain array");
            assert!(!versions.is_empty(), "live album must have a testable version");
            let default_rows = visit(&default_detail, &|node| node.data.contains_key("track-id"));
            let default_tracks: Vec<ApiTrack> = serde_json::from_value(versions[0]["tracks"].clone()).unwrap();
            assert_eq!(
                default_rows.iter().map(|row| row.data["track-id"].clone()).collect::<Vec<_>>(),
                default_tracks.iter().map(|track| track.track_id.to_string()).collect::<Vec<_>>(),
                "default detail must render the entire first version in order",
            );
            for version in versions {
                let version_source = serde_json::from_value(version["source"].clone()).unwrap();
                let sample_rate = version["sampleRate"].as_u64().map(|x| u32::try_from(x).unwrap());
                let bit_depth = version["bitDepth"].as_u64().map(|x| u8::try_from(x).unwrap());
                let full_path = moosicbox_app_native_ui::albums::album_page_url(
                    &album.album_id.to_string(), true, Some(&album.api_source),
                    Some(&version_source), sample_rate, bit_depth,
                );
                let detail = app_view(&router, &full_path).await;
                let mut rendered = String::new();
                text(&detail, &mut rendered);
                assert!(rendered.lines().any(|line| line == album.title));
                let artist_url = format!("/artists?artistId={}&source={}", album.artist_id, album.api_source);
                assert!(!visit(&detail, &|node| href(node) == Some(artist_url.as_str())).is_empty());
                if album.contains_cover {
                    let cover = moosicbox_app_native_ui::albums::album_cover_url_from_album(
                        &endpoint, &album, 266, 266,
                    );
                    assert!(!visit(&detail, &|node| matches!(
                        &node.element, Element::Image { source: Some(source), .. } if source == &cover
                    )).is_empty(), "detail artwork must retain album identity/source");
                }
                let expected: Vec<ApiTrack> = serde_json::from_value(version["tracks"].clone()).unwrap();
                assert!(!expected.is_empty(), "live version must contain tracks");
                assert_track_table(&detail, expected.len());
                let rows = visit(&detail, &|node| node.data.contains_key("track-id"));
                assert_eq!(rows.len(), expected.len(), "complete selected-version track count: {full_path}");
                for (row, track) in rows.iter().zip(&expected) {
                    assert_eq!(row.data["track-id"], track.track_id.to_string(),
                        "ordered selected-version identity: {full_path}");
                    let mut row_text = String::new();
                    text(row, &mut row_text);
                    for value in [&track.title, &track.artist, &track.number.to_string()] {
                        assert!(row_text.lines().any(|line| line == value), "missing {value:?}: {full_path}");
                    }
                    let track_artist_url = format!("/artists?artistId={}&source={}", track.artist_id, track.api_source);
                    assert!(!visit(row, &|node| href(node) == Some(track_artist_url.as_str())).is_empty(),
                        "track artist must use track API identity, not version source");
                }
                eprintln!("Live detail {} source={} version={} tracks={}",
                    album.album_id, album.api_source, version_source, rows.len());
            }
        }
    });
}
