#![allow(clippy::module_name_repetitions)]

use gigachad_actions::{
    logic::{
        get_data_attr_value_self, get_event_value, get_visibility_self, get_visibility_str_id,
    },
    ActionType,
};
use gigachad_transformer_models::Visibility;
use maud::{html, Markup, PreEscaped};
use moosicbox_core::sqlite::models::{
    AlbumSort, AlbumVersionQuality, ApiAlbum, ApiSource, ApiTrack, Id, TrackApiSource,
};
use moosicbox_menu_models::api::ApiAlbumVersion;
use moosicbox_paging::Page;

use crate::{formatting::TimeFormat as _, page, pre_escaped, public_img, state::State, Action};

#[must_use]
pub fn album_cover_url(
    album_id: &Id,
    source: ApiSource,
    contains_cover: bool,
    width: u16,
    height: u16,
) -> String {
    if contains_cover {
        format!(
            "{}/files/albums/{}/{width}x{height}?moosicboxProfile=master&source={}",
            std::env::var("MOOSICBOX_HOST")
                .as_deref()
                .unwrap_or("http://localhost:8500"),
            album_id,
            source,
        )
    } else {
        public_img!("album.svg").to_string()
    }
}

#[must_use]
pub fn album_cover_url_from_album(album: &ApiAlbum, width: u16, height: u16) -> String {
    album_cover_url(
        &album.album_id,
        album.api_source,
        album.contains_cover,
        width,
        height,
    )
}

#[must_use]
pub fn album_cover_url_from_track(track: &ApiTrack, width: u16, height: u16) -> String {
    album_cover_url(
        &track.album_id,
        track.api_source,
        track.contains_cover,
        width,
        height,
    )
}

#[must_use]
pub fn album_cover_img_from_album(album: &ApiAlbum, size: u16) -> Markup {
    #[allow(clippy::cast_sign_loss)]
    #[allow(clippy::cast_possible_truncation)]
    let request_size = (f64::from(size) * 1.33).round() as u16;

    html! {
        img src=(PreEscaped(album_cover_url_from_album(album, request_size, request_size))) sx-width=(size) sx-height=(size);
    }
}

#[must_use]
pub fn album_cover_img_from_track(track: &ApiTrack, size: u16) -> Markup {
    #[allow(clippy::cast_sign_loss)]
    #[allow(clippy::cast_possible_truncation)]
    let request_size = (f64::from(size) * 1.33).round() as u16;

    html! {
        img src=(PreEscaped(album_cover_url_from_track(track, request_size, request_size))) sx-width=(size) sx-height=(size);
    }
}

#[must_use]
pub fn album_page_immediate(
    album_id: &str,
    source: Option<ApiSource>,
    version_source: Option<TrackApiSource>,
    sample_rate: Option<u32>,
    bit_depth: Option<u8>,
) -> Markup {
    let path = album_page_url(
        album_id,
        true,
        source,
        version_source,
        sample_rate,
        bit_depth,
    );
    html! {
        div
            hx-get=(path)
            hx-trigger="load"
            sx-padding-x=(60)
            sx-padding-y=(20)
        {
            div sx-dir="row" {
                @let size = 200;
                div sx-width=(size) sx-height=(size + 30) {
                    img src=(public_img!("album.svg")) sx-width=(size) sx-height=(size);
                }
                div {
                    h1 { "loading..." }
                    h2 { "loading..." }
                }
            }
            div {
                table {
                    thead {
                        tr{
                            th { "#" }
                            th { "Title" }
                            th { "Artist" }
                            th { "Time" }
                        }
                    }
                    tbody {
                        tr {
                            td { "loading..." }
                            td { "loading..." }
                            td { a { "loading..." } }
                            td { "loading..." }
                        }
                    }
                }
            }
        }
    }
}

#[allow(clippy::too_many_lines)]
#[must_use]
pub fn album_page_content(
    state: &State,
    album: &ApiAlbum,
    versions: &[ApiAlbumVersion],
    selected_version: Option<&ApiAlbumVersion>,
) -> Markup {
    fn same_version(a: &AlbumVersionQuality, b: &AlbumVersionQuality) -> bool {
        a.source == b.source && a.sample_rate == b.sample_rate && a.bit_depth == b.bit_depth
    }

    let selected_version = versions
        .iter()
        .find(|x| selected_version.is_some_and(|v| same_version(&(*x).into(), &v.into())))
        .or_else(|| versions.first());

    html! {
        div
            sx-padding-x=(60)
            sx-padding-y=(20)
        {
            div sx-dir="row" {
                @let size = 200;
                div sx-width=(size) sx-height=(size + 30) {
                    (album_cover_img_from_album(&album, size))
                }
                div {
                    h1 { (album.title) }
                    h2 { (album.artist) }
                    div sx-dir="row" {
                        @for version in &album.versions {
                            @let selected = selected_version.is_some_and(|x| same_version(version, &x.into()));
                            a href=(
                                album_page_url(
                                    &album.album_id.to_string(),
                                    false,
                                    Some(album.api_source),
                                    Some(version.source),
                                    version.sample_rate,
                                    version.bit_depth,
                                )
                            ) {
                                h3 {
                                    (if selected { "*" } else { "" })
                                    (version.source)
                                    (if selected { "*" } else { "" })
                                }
                            }
                        }
                    }
                }
            }
            div sx-dir="row" {
                button
                    sx-dir="row"
                    sx-width=(130)
                    sx-height=(40)
                    sx-background="#fff"
                    sx-border-radius=(5)
                    fx-click=(Action::PlayAlbum {
                        album_id: album.album_id.clone(),
                        api_source: album.api_source,
                        version_source: selected_version.map(|x| x.source),
                        sample_rate: selected_version.and_then(|x| x.sample_rate),
                        bit_depth: selected_version.and_then(|x| x.bit_depth),
                    })
                {
                    @let icon_size = 12;
                    img
                        sx-width=(icon_size)
                        sx-height=(icon_size)
                        src=(public_img!("play-button.svg"));
                    "Play"
                }
                button
                    sx-dir="row"
                    sx-width=(130)
                    sx-height=(40)
                    sx-background="#fff"
                    sx-border-radius=(5)
                    fx-click=(Action::AddAlbumToQueue {
                        album_id: album.album_id.clone(),
                        api_source: album.api_source,
                        version_source: selected_version.map(|x| x.source),
                        sample_rate: selected_version.and_then(|x| x.sample_rate),
                        bit_depth: selected_version.and_then(|x| x.bit_depth),
                    })
                {
                    @let icon_size = 20;
                    img
                        sx-width=(icon_size)
                        sx-height=(icon_size)
                        src=(public_img!("more-options.svg"));
                    "Options"
                }
            }
            @if let Some(version) = selected_version {
                div {
                    table {
                        thead {
                            tr {
                                th { "#" }
                                th { "Title" }
                                th { "Artist" }
                                th { "Time" }
                            }
                        }
                        (album_page_tracks_table_body_from_state(state, &version))
                    }
                }
            }
        }
    }
}

#[must_use]
pub fn album_page_tracks_table_body(version: &ApiAlbumVersion, track_id: Option<&Id>) -> Markup {
    html! {
        tbody id="album-page-tracks" {
            @for track in &version.tracks {
                @let current_track = track_id.is_some_and(|x| x == &track.track_id);
                tr
                    sx-border-radius=(5)
                    data-track-id=(track.track_id)
                    fx-hover=(
                        ActionType::set_background_self("#444")
                            .and(ActionType::set_visibility_child_class(Visibility::Hidden, "track-number"))
                            .and(ActionType::set_visibility_child_class(Visibility::Visible, "play-button"))
                    )
                    fx-event=(ActionType::on_event(
                        "play-track",
                        get_event_value()
                            .eq(get_data_attr_value_self("track-id"))
                            .then(ActionType::set_background_self("#333"))
                            .or_else(ActionType::remove_background_self())
                    ))
                    sx-background=[if current_track { Some("#333") } else { None }]
                {
                    td sx-padding-x=(10) sx-padding-y=(15) sx-height=(50) {
                        span class="track-number" { (track.number) }
                        button
                            class="play-button"
                            sx-visibility=(Visibility::Hidden)
                            fx-click=(Action::PlayAlbumStartingAtTrackId {
                                album_id: track.album_id.clone(),
                                start_track_id: track.track_id.clone(),
                                api_source: track.api_source,
                                version_source: Some(version.source),
                                sample_rate: version.sample_rate,
                                bit_depth: version.bit_depth,
                            })
                        {
                            @let icon_size = 12;
                            img
                                sx-width=(icon_size)
                                sx-height=(icon_size)
                                src=(public_img!("play-button-white.svg"));
                        }
                    }
                    td sx-padding-x=(10) sx-padding-y=(15) sx-height=(50) {
                        (track.title)
                    }
                    td sx-padding-x=(10) sx-padding-y=(15) sx-height=(50) {
                        a href=(pre_escaped!("/artists?artistId={}&source={}", track.artist_id, track.api_source)) { (track.artist) }
                    }
                    td sx-padding-x=(10) sx-padding-y=(15) sx-height=(50) {
                        (track.duration.into_formatted())
                    }
                }
            }
        }
    }
}

#[must_use]
pub fn album_page_tracks_table_body_from_state(state: &State, version: &ApiAlbumVersion) -> Markup {
    if let Some(playback) = &state.player.playback {
        let track: Option<&ApiTrack> = playback.tracks.get(playback.position as usize);

        if let Some(track) = track {
            return album_page_tracks_table_body(version, Some(&track.track_id));
        }
    }

    album_page_tracks_table_body(version, None)
}

#[must_use]
pub fn album(
    state: &State,
    album_id: &str,
    source: Option<ApiSource>,
    version_source: Option<TrackApiSource>,
    sample_rate: Option<u32>,
    bit_depth: Option<u8>,
) -> Markup {
    page(
        state,
        &album_page_immediate(album_id, source, version_source, sample_rate, bit_depth),
    )
}

#[must_use]
pub fn albums_list_start(
    albums: &Page<ApiAlbum>,
    filtered_sources: &[TrackApiSource],
    sort: AlbumSort,
    size: u16,
) -> Markup {
    static MAX_PARALLEL_REQUESTS: u32 = 6;
    static MIN_PAGE_THRESHOLD: u32 = 30;
    let filtered_sources = filtered_sources_to_string(filtered_sources);
    let sort = sort.to_string();
    let limit = albums.limit();
    let offset = albums.offset() + limit;
    let remaining = if albums.has_more() {
        albums.remaining().map_or_else(
            || {
                html! {
                    div
                        hx-get=(pre_escaped!(
                            "/albums-list-start{}",
                            build_query('?', &[
                                ("offset", &offset.to_string()),
                                ("limit", &limit.to_string()),
                                ("size", &size.to_string()),
                                ("sources", &filtered_sources),
                                ("sort", &sort),
                            ])
                        ))
                        hx-trigger="load"
                        sx-hidden=(true)
                    {}
                }
            },
            |remaining| {
                let limit = remaining / MAX_PARALLEL_REQUESTS;
                let last = limit + (remaining % MAX_PARALLEL_REQUESTS);

                html! {
                    @if limit < MIN_PAGE_THRESHOLD {
                        div
                            hx-get=(pre_escaped!(
                                "/albums-list{}",
                                build_query('?', &[
                                    ("offset", &offset.to_string()),
                                    ("limit", &remaining.to_string()),
                                    ("size", &size.to_string()),
                                    ("sources", &filtered_sources),
                                    ("sort", &sort),
                                ])
                            ))
                            hx-trigger="load"
                            sx-hidden=(true)
                        {}
                    } @else {
                        @for i in 0..MAX_PARALLEL_REQUESTS {
                            @if i == MAX_PARALLEL_REQUESTS - 1 {
                                div
                                    hx-get=(pre_escaped!(
                                        "/albums-list{}",
                                        build_query('?', &[
                                            ("offset", &(offset + i * limit).to_string()),
                                            ("limit", &last.to_string()),
                                            ("size", &size.to_string()),
                                            ("sources", &filtered_sources),
                                            ("sort", &sort),
                                        ])
                                    ))
                                    hx-trigger="load"
                                    sx-hidden=(true)
                                {}
                            } @else {
                                div
                                    hx-get=(pre_escaped!(
                                        "/albums-list{}",
                                        build_query('?', &[
                                            ("offset", &(offset + i * limit).to_string()),
                                            ("limit", &limit.to_string()),
                                            ("size", &size.to_string()),
                                            ("sources", &filtered_sources),
                                            ("sort", &sort),
                                        ])
                                    ))
                                    hx-trigger="load"
                                    sx-hidden=(true)
                                {}
                            }
                        }
                    }
                }
            },
        )
    } else {
        html! {}
    };

    html! {
        (show_albums(albums.iter(), size))
        (remaining)
    }
}

#[must_use]
pub fn albums_list(albums: &Page<ApiAlbum>, size: u16) -> Markup {
    show_albums(albums.iter(), size)
}

#[must_use]
pub fn album_display(
    album: &ApiAlbum,
    size: u16,
    show_details: bool,
    show_media_controls: bool,
) -> Markup {
    let details = if show_details {
        html! { (album.title) }
    } else {
        html! {}
    };

    let album_cover = if show_media_controls {
        html! {
            div
                sx-width=(size)
                sx-height=(size)
                sx-position="relative"
                fx-hover=(ActionType::show_last_child())
            {
                (album_cover_img_from_album(album, size))
                div
                    sx-width=(size)
                    sx-height=(size)
                    sx-position="absolute"
                    sx-visibility="hidden"
                {
                    @let button_size = size / 4;
                    @let icon_size = size / 10;
                    button
                        sx-dir="row"
                        sx-position="absolute"
                        sx-bottom="5%"
                        sx-left="5%"
                        sx-width=(button_size)
                        sx-height=(button_size)
                        sx-justify-content="center"
                        sx-align-items="center"
                        sx-background="#fff"
                        sx-border-radius="100%"
                        fx-click=(Action::PlayAlbum {
                            album_id: album.album_id.clone(),
                            api_source: album.api_source,
                            version_source: None,
                            sample_rate: None,
                            bit_depth: None,
                        })
                    {
                        img
                            sx-width=(icon_size)
                            sx-height=(icon_size)
                            src=(public_img!("play-button.svg"));
                    }
                    @let icon_size = size / 7;
                    button
                        sx-dir="row"
                        sx-position="absolute"
                        sx-bottom="5%"
                        sx-right="5%"
                        sx-width=(button_size)
                        sx-height=(button_size)
                        sx-justify-content="center"
                        sx-align-items="center"
                        sx-background="#fff"
                        sx-border-radius="100%"
                        fx-click=(Action::AddAlbumToQueue {
                            album_id: album.album_id.clone(),
                            api_source: album.api_source,
                            version_source: None,
                            sample_rate: None,
                            bit_depth: None,
                        })
                    {
                        img
                            sx-width=(icon_size)
                            sx-height=(icon_size)
                            src=(public_img!("more-options.svg"));
                    }
                }
            }
        }
    } else {
        html! { (album_cover_img_from_album(album, size)) }
    };

    html! {
        div sx-width=(size) sx-height=(size + if show_details { 30 } else { 0 }) {
            (album_cover)
            (details)
        }
    }
}

fn filtered_sources_to_string(filtered_sources: &[TrackApiSource]) -> String {
    filtered_sources
        .iter()
        .map(ToString::to_string)
        .collect::<Vec<_>>()
        .join(",")
}

fn build_query(start: char, values: &[(&str, &str)]) -> String {
    let mut query = String::new();

    for (key, value) in values {
        if value.is_empty() {
            continue;
        }
        if query.is_empty() {
            query.push(start);
        } else {
            query.push('&');
        }

        query.push_str(key);
        query.push('=');
        query.push_str(value);
    }

    query
}

#[must_use]
pub fn albums_page_url(filtered_sources: &[TrackApiSource], sort: AlbumSort) -> String {
    format!(
        "/albums{}",
        build_query(
            '?',
            &[
                ("sort", &sort.to_string()),
                ("sources", &filtered_sources_to_string(filtered_sources)),
            ]
        )
    )
}

#[must_use]
pub fn album_page_url(
    album_id: &str,
    full: bool,
    api_source: Option<ApiSource>,
    version_source: Option<TrackApiSource>,
    sample_rate: Option<u32>,
    bit_depth: Option<u8>,
) -> PreEscaped<String> {
    pre_escaped!(
        "/albums?albumId={album_id}{}{}{}{}{}",
        if full { "&full=true" } else { "" },
        api_source.map_or_else(String::new, |x| format!("&source={x}")),
        version_source.map_or_else(String::new, |x| format!("&versionSource={x}")),
        sample_rate.map_or_else(String::new, |x| format!("&sampleRate={x}")),
        bit_depth.map_or_else(String::new, |x| format!("&bitDepth={x}")),
    )
}

pub fn show_albums<'a>(albums: impl Iterator<Item = &'a ApiAlbum>, size: u16) -> Markup {
    html! {
        @for album in albums {
            a
                href=(album_page_url(&album.album_id.to_string(), false, None, None, None, None))
                sx-width=(size)
                sx-height=(size + 30)
            {
                (album_display(album, size, true, true))
            }
        }
    }
}

#[allow(clippy::too_many_lines)]
#[must_use]
pub fn albums_page_content(filtered_sources: &[TrackApiSource], sort: AlbumSort) -> Markup {
    let size: u16 = 200;

    html! {
        div
            sx-height=(36 + 40)
            sx-background="#080a0b"
        {
            div
                sx-padding-x=(30)
                sx-padding-top=(15)
            {
                div sx-dir="row" sx-justify-content="start" {
                    h1 sx-width=(50) sx-height=(36) { "Albums" }
                    @let button_size = 30;
                    @let icon_size = button_size - 10;
                    div sx-position="relative" sx-width=(button_size) sx-height=(button_size) {
                        button
                            sx-dir="row"
                            sx-width=(button_size)
                            sx-height=(button_size)
                            sx-justify-content="center"
                            sx-align-items="center"
                            fx-click=(
                                get_visibility_str_id("albums-menu")
                                    .eq(Visibility::Hidden)
                                    .then(ActionType::show_str_id("albums-menu"))
                            )
                        {
                            img
                                sx-width=(icon_size)
                                sx-height=(icon_size)
                                src=(public_img!("more-options-white.svg"));
                        }
                        div
                            id="albums-menu"
                            sx-width=(300)
                            sx-position="absolute"
                            sx-top="100%"
                            sx-visibility="hidden"
                            sx-background="#080a0b"
                            sx-border-radius=(5)
                            sx-dir="row"
                            fx-click-outside=(
                                get_visibility_self()
                                    .eq(Visibility::Visible)
                                    .then(ActionType::hide_self())
                            )
                        {
                            div {
                                div {
                                    button
                                        fx-click=(ActionType::Navigate {
                                            url: albums_page_url(
                                                filtered_sources,
                                                if sort == AlbumSort::ArtistAsc {
                                                    AlbumSort::ArtistDesc
                                                } else {
                                                    AlbumSort::ArtistAsc
                                                }
                                            )
                                        })
                                    {
                                        "Album Artist"
                                    }
                                }
                                div sx-border-top="1, #222" {
                                    button
                                        fx-click=(ActionType::Navigate {
                                            url: albums_page_url(
                                                filtered_sources,
                                                if sort == AlbumSort::NameAsc {
                                                    AlbumSort::NameDesc
                                                } else {
                                                    AlbumSort::NameAsc
                                                }
                                            )
                                        })
                                    {
                                        "Album Name"
                                    }
                                }
                                div sx-border-top="1, #222" {
                                    button
                                        fx-click=(ActionType::Navigate {
                                            url: albums_page_url(
                                                filtered_sources,
                                                if sort == AlbumSort::ReleaseDateDesc {
                                                    AlbumSort::ReleaseDateAsc
                                                } else {
                                                    AlbumSort::ReleaseDateDesc
                                                }
                                            )
                                        })
                                    {
                                        "Album Release Date"
                                    }
                                }
                                div sx-border-top="1, #222" {
                                    button
                                        fx-click=(ActionType::Navigate {
                                            url: albums_page_url(
                                                filtered_sources,
                                                if sort == AlbumSort::DateAddedDesc {
                                                    AlbumSort::DateAddedAsc
                                                } else {
                                                    AlbumSort::DateAddedDesc
                                                }
                                            )
                                        })
                                    {
                                        "Album Date Added"
                                    }
                                }
                            }
                            div {
                                @for source in TrackApiSource::all() {
                                    div sx-dir="row" {
                                        @let checked = filtered_sources.iter().any(|x| x == source);
                                        (source.to_string())
                                        input
                                            fx-change=(ActionType::Navigate {
                                                url: albums_page_url(&if checked {
                                                    filtered_sources.iter().filter(|x| *x != source).copied().collect::<Vec<_>>()
                                                } else {
                                                    [filtered_sources, &[*source]].concat()
                                                }, sort)
                                            })
                                            type="checkbox"
                                            checked=(checked);
                                    }
                                }
                            }
                        }
                    }
                }
                input type="text" placeholder="Filter...";
            }
        }
        div
            hx-get=(pre_escaped!(
                "/albums-list-start{}",
                build_query('?', &[
                    ("limit", "100"),
                    ("size", &size.to_string()),
                    ("sources", &filtered_sources_to_string(filtered_sources)),
                    ("sort", &sort.to_string()),
                ])
            ))
            hx-trigger="load"
            hx-swap="children"
            sx-dir="row"
            sx-overflow-x="wrap"
            sx-overflow-y="show"
            sx-justify-content="space-evenly"
            sx-gap=(15)
            sx-padding-x=(30)
            sx-padding-y=(15)
        {
            @for _ in 0..100 {
                div sx-width=(size) sx-height=(size + 30) {
                    img src=(public_img!("album.svg")) sx-width=(size) sx-height=(size);
                }
            }
        }
    }
}

#[must_use]
pub fn albums(state: &State, filtered_sources: &[TrackApiSource], sort: AlbumSort) -> Markup {
    page(state, &albums_page_content(filtered_sources, sort))
}
