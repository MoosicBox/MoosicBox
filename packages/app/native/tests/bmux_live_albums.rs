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

// Exercise real app routes against the same read-only server query. This checks
// fragments and their successor contract, not renderer viewport dispatch.
async fn assert_live_album_page(
    router: &hyperchad::router::Router,
    client: &switchy::http::Client,
    endpoint: &str,
    path: &str,
) -> (moosicbox_paging::Page<ApiAlbum>, Option<String>) {
    use hyperchad::transformer::models::{Route, SwapStrategy};
    let request = RouteRequest::from_path(path, Default::default());
    let response = client
        .get(&format!("{endpoint}/menu/albums"))
        .query_param("moosicboxProfile", "master")
        .query_param(
            "offset",
            request.query.get("offset").map_or("0", String::as_str),
        )
        .query_param("limit", &request.query["limit"])
        .query_param("sort", &request.query["sort"])
        .query_param_opt("sources", request.query.get("sources").map(String::as_str))
        .query_param_opt("search", request.query.get("search").map(String::as_str))
        .send()
        .await
        .unwrap();
    assert!(response.status().is_success(), "real page query: {path}");
    let page: moosicbox_paging::Page<ApiAlbum> = response.json().await.unwrap();
    let root = app_view(router, path).await;
    let links = visit(&root, &|node| {
        href(node).is_some_and(|url| url.contains("albumId="))
    });
    let expected = page
        .iter()
        .flat_map(|album| {
            let url = moosicbox_app_native_ui::albums::album_page_url(
                &album.album_id.to_string(),
                false,
                Some(&album.api_source),
                None,
                None,
                None,
            );
            [url.clone(), url]
        })
        .collect::<Vec<_>>();
    assert_eq!(
        links
            .iter()
            .map(|node| href(node).unwrap())
            .collect::<Vec<_>>(),
        expected,
        "complete ordered page identities, including source: {path}"
    );
    let routes = visit(&root, &|node| node.route.is_some());
    assert_eq!(
        routes.len(),
        usize::from(page.has_more()),
        "one sentinel, none at end: {path}"
    );
    let next = routes.first().map(|sentinel| {
        assert!(matches!(sentinel.element, Element::Button { .. }));
        assert_eq!(sentinel.height, Some(30.into()));
        let Some(Route::Get {
            route,
            trigger,
            strategy,
            ..
        }) = &sentinel.route
        else {
            panic!("expected a next-page GET: {path}")
        };
        assert_eq!(trigger.as_deref(), Some("revealed"));
        assert_eq!(strategy, &SwapStrategy::This);
        let next_request = RouteRequest::from_path(route, Default::default());
        for key in ["limit", "size", "sort", "sources", "search"] {
            assert_eq!(
                next_request
                    .query
                    .get(key)
                    .map(String::as_str)
                    .unwrap_or(""),
                request.query.get(key).map(String::as_str).unwrap_or(""),
                "retain {key}: {path}"
            );
        }
        assert_eq!(
            next_request.query["offset"],
            (page.offset() + page.limit()).to_string()
        );
        route.clone()
    });
    (page, next)
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
fn terminal_album_pagination_reveals_one_successor_and_retains_filters() {
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
    let sources = TrackApiSource::all();
    let source_filter = sources
        .iter()
        .map(ToString::to_string)
        .collect::<Vec<_>>()
        .join(",");
    for (offset, total, limit, has_next) in [
        (0_u32, 0_u32, 100_u32, false),
        (0, 100, 100, false),
        (0, 2006, 100, true),
        (100, 2006, 100, true),
        (2000, 2006, 100, false),
        (0, 7, 3, true),
        (3, 7, 3, true),
        (6, 7, 3, false),
    ] {
        let count = (total - offset).min(limit);
        let page = moosicbox_paging::Page::WithTotal {
            items: (0..count)
                .map(|index| ApiAlbum {
                    album_id: u64::from(offset + index).into(),
                    ..Default::default()
                })
                .collect(),
            offset,
            limit,
            total,
        };
        let root: Container = moosicbox_app_native_ui::albums::albums_list_page(
            &state,
            &page,
            sources,
            AlbumSort::NameDesc,
            200,
            "A & B+é?=/#",
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
            assert_eq!(requests[0].width, Some(200.into()));
            assert_eq!(requests[0].height, Some(30.into()));
            let Some(Route::Get {
                route,
                trigger,
                strategy,
                ..
            }) = &requests[0].route
            else {
                panic!("GET next page")
            };
            assert_eq!(trigger.as_deref(), Some("revealed"));
            assert_eq!(strategy, &SwapStrategy::This);
            let request = RouteRequest::from_path(route, hyperchad::router::RequestInfo::default());
            for (key, expected) in [
                ("offset", (offset + limit).to_string()),
                ("limit", limit.to_string()),
                ("size", "200".into()),
                ("sources", source_filter.clone()),
                ("sort", AlbumSort::NameDesc.to_string()),
                ("search", "A & B+é?=/#".into()),
            ] {
                assert_eq!(request.query.get(key), Some(&expected), "preserve {key}");
            }
        }
    }
}

#[test]
fn terminal_album_sentinel_handles_unknown_totals_and_initial_search_encoding() {
    use hyperchad::transformer::models::{Route, SwapStrategy};
    use moosicbox_music_models::AlbumSort;
    let state = moosicbox_app_native_ui::state::State {
        connection: Some(Connection {
            name: "Offline pagination".into(),
            api_url: "http://example.invalid".into(),
        }),
        ..Default::default()
    };
    for has_more in [true, false] {
        let page = moosicbox_paging::Page::WithHasMore {
            items: vec![ApiAlbum::default()],
            offset: 7,
            limit: 1,
            has_more,
        };
        let root: Container = moosicbox_app_native_ui::albums::albums_list_page(
            &state,
            &page,
            &[],
            AlbumSort::NameAsc,
            0,
            "",
        )
        .into();
        let routes = visit(&root, &|node| node.route.is_some());
        assert_eq!(routes.len(), usize::from(has_more));
        if has_more {
            assert_eq!(
                routes[0].width,
                Some(1.into()),
                "sentinel always has positive area"
            );
            assert_eq!(routes[0].height, Some(30.into()));
            let Some(Route::Get {
                route,
                trigger,
                strategy,
                ..
            }) = &routes[0].route
            else {
                panic!("next-page GET")
            };
            assert_eq!(trigger.as_deref(), Some("revealed"));
            assert_eq!(strategy, &SwapStrategy::This);
            let request = RouteRequest::from_path(route, Default::default());
            assert_eq!(request.query["offset"], "8");
            assert_eq!(request.query["limit"], "1");
            assert!(!request.query.contains_key("sources"));
            assert!(!request.query.contains_key("search"));
        }
    }
    let search = "A & B+é?=/#";
    let root: Container =
        moosicbox_app_native_ui::albums::load_albums(64, AlbumSort::NameAsc, &[], search).into();
    let routes = visit(&root, &|node| node.route.is_some());
    assert_eq!(
        routes.len(),
        1,
        "only initial grid load, no eager successor"
    );
    let Some(Route::Get { route, trigger, .. }) = &routes[0].route else {
        panic!("initial grid GET")
    };
    assert_eq!(trigger.as_deref(), Some("load"));
    let request = RouteRequest::from_path(route, Default::default());
    assert_eq!(request.query["search"], search);
}

#[test]
fn terminal_empty_album_pages_advance_without_discarding_continuations() {
    use hyperchad::transformer::models::{Route, SwapStrategy};
    use moosicbox_music_models::AlbumSort;
    let state = moosicbox_app_native_ui::state::State {
        connection: Some(Connection {
            name: "Empty pagination".into(),
            api_url: "http://example.invalid".into(),
        }),
        ..Default::default()
    };
    // Empty pages can be valid (for example after server-side filtering).
    // Never retry the same URL forever, nor silently discard has_more.
    for (offset, limit, has_more, expected) in [
        (0, 0, true, Some(1)),
        (0, 7, true, Some(7)),
        (7, 0, true, Some(8)),
        (7, 7, true, Some(14)),
        (0, 0, false, None),
        (0, 7, false, None),
        (u32::MAX - 1, 7, true, Some(u32::MAX)),
        (u32::MAX, 7, true, None),
    ] {
        let page = moosicbox_paging::Page::WithHasMore {
            items: vec![],
            offset,
            limit,
            has_more,
        };
        let root: Container = moosicbox_app_native_ui::albums::albums_list_page(
            &state,
            &page,
            &[],
            AlbumSort::NameAsc,
            64,
            "",
        )
        .into();
        let routes = visit(&root, &|node| node.route.is_some());
        assert_eq!(routes.len(), usize::from(expected.is_some()));
        if let Some(expected) = expected {
            assert!(matches!(routes[0].element, Element::Button { .. }));
            let mut label = String::new();
            text(routes[0], &mut label);
            assert!(
                label.contains("retry"),
                "manual retry must have a visible label"
            );
            let Some(Route::Get {
                route,
                trigger,
                strategy,
                ..
            }) = &routes[0].route
            else {
                panic!("empty-page continuation GET")
            };
            assert_eq!(trigger.as_deref(), Some("revealed"));
            assert_eq!(strategy, &SwapStrategy::This);
            let request = RouteRequest::from_path(route, Default::default());
            assert_eq!(request.query["offset"], expected.to_string());
            assert_eq!(request.query["limit"], limit.clamp(1, 100).to_string());
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
                    "/albums-list-start?limit=2000&size=64&sort=name",
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
                    if trigger.as_deref() == Some("revealed") && route.contains("offset=100") && route.contains("limit=100")
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
        // Follow real successor fragments, without eagerly draining the library.
        // Use canonical Display spelling: the accepted alias name-asc normalizes to name.
        let first_path = "/albums-list-start?limit=2&size=64&sort=name";
        let (first, mut next) = assert_live_album_page(&router, &client, &endpoint, first_path).await;
        let total = first.total().expect("real library reports a total");
        assert!(total > 6, "live fixture must support at least three pages");
        let mut identities = std::collections::BTreeSet::new();
        for album in first.iter() {
            assert!(identities.insert((album.api_source.to_string(), album.album_id.to_string())));
        }
        for _ in 0..2 {
            let path = next.expect("fixture has another real page");
            let (page, successor) = assert_live_album_page(&router, &client, &endpoint, &path).await;
            for album in page.iter() {
                assert!(identities.insert((album.api_source.to_string(), album.album_id.to_string())),
                    "successive pages must not repeat album identities");
            }
            next = successor;
        }
        assert_eq!(identities.len(), 6);
        let (last, successor) = assert_live_album_page(&router, &client, &endpoint,
            &format!("{first_path}&offset={}", total - 1)).await;
        assert_eq!(last.iter().count(), 1);
        assert!(successor.is_none());
        let (empty, successor) = assert_live_album_page(&router, &client, &endpoint,
            &format!("{first_path}&offset={total}")).await;
        assert_eq!(empty.iter().count(), 0);
        assert!(successor.is_none());

        // Restart from a filtered grid using the production initial-load URL.
        let search = &first.iter().next().unwrap().title;
        for sources in [vec![], vec![moosicbox_music_models::TrackApiSource::Local]] {
            let skeleton: Container = moosicbox_app_native_ui::albums::load_albums(
                64, moosicbox_music_models::AlbumSort::NameDesc, &sources, search,
            ).into();
            let loads = visit(&skeleton, &|node| node.route.is_some());
            assert_eq!(loads.len(), 1);
            let Some(hyperchad::transformer::models::Route::Get { route, .. }) = &loads[0].route else {
                panic!("initial album load")
            };
            let path = route.replace("limit=100", "limit=2");
            let request = RouteRequest::from_path(&path, Default::default());
            assert_eq!(&request.query["search"], search);
            let (filtered, next) = assert_live_album_page(&router, &client, &endpoint, &path).await;
            if sources.is_empty() {
                assert!(filtered.iter().next().is_some(), "title search must find its real album");
            }
            if let Some(next) = next {
                let _ = assert_live_album_page(&router, &client, &endpoint, &next).await;
            }
            let total = filtered.total().expect("filtered library reports a total");
            let (empty, next) = assert_live_album_page(&router, &client, &endpoint,
                &format!("{path}&offset={total}")).await;
            assert_eq!(empty.iter().count(), 0);
            assert!(next.is_none(), "filtered end must not load unfiltered albums");
        }

        eprintln!("Live pagination assertions passed: real successor pages, ordered identities, end-of-results, search/source filters. Detail assertions follow; overall test is not yet complete.");

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
                    // Match the image helper's rounded 1.33x request resolution,
                    // using the public layout size (terminal and GUI differ).
                    let size = u32::from(moosicbox_app_native_ui::ALBUM_ART_SIZE);
                    let request_size = u16::try_from((size * 133 + 50) / 100).unwrap();
                    let cover = moosicbox_app_native_ui::albums::album_cover_url_from_album(
                        &endpoint, &album, request_size, request_size,
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
