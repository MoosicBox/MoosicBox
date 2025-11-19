//! Album display and navigation components.
//!
//! This module provides UI templates for rendering album lists, album detail pages,
//! and album cover artwork.

#![allow(clippy::module_name_repetitions)]

use std::slice;

use hyperchad::{
    template::{self as hyperchad_template, Containers, container},
    transformer::models::{ImageLoading, LayoutOverflow, Visibility},
};
use moosicbox_menu_models::api::ApiAlbumVersion;
use moosicbox_music_models::{
    AlbumSort, AlbumVersionQuality, ApiSource, TrackApiSource,
    api::{ApiAlbum, ApiTrack},
    id::Id,
};
use moosicbox_paging::Page;

use crate::{
    Action, DARK_BACKGROUND,
    artists::artist_page_url,
    formatting::{
        AlbumVersionQualityFormat as _, TimeFormat as _, display_album_version_qualities,
        format_date_string,
    },
    page, public_img,
    state::State,
};

/// Constructs a URL for an album cover image.
///
/// Returns a placeholder image URL if the album does not contain cover art.
#[must_use]
pub fn album_cover_url(
    host: &str,
    album_id: &Id,
    source: &ApiSource,
    contains_cover: bool,
    width: u16,
    height: u16,
) -> String {
    if contains_cover {
        format!(
            "{host}/files/albums/{album_id}/{width}x{height}?moosicboxProfile=master&source={source}",
        )
    } else {
        public_img!("album.svg").to_string()
    }
}

/// Constructs a URL for an album cover image from an `ApiAlbum`.
#[must_use]
pub fn album_cover_url_from_album(host: &str, album: &ApiAlbum, width: u16, height: u16) -> String {
    album_cover_url(
        host,
        &album.album_id,
        &album.api_source,
        album.contains_cover,
        width,
        height,
    )
}

/// Constructs a URL for an album cover image from an `ApiTrack`.
#[must_use]
pub fn album_cover_url_from_track(host: &str, track: &ApiTrack, width: u16, height: u16) -> String {
    album_cover_url(
        host,
        &track.album_id,
        &track.api_source,
        track.contains_cover,
        width,
        height,
    )
}

/// Renders an album cover image element from an `ApiAlbum`.
///
/// Uses lazy loading and requests a higher resolution image for better display quality.
#[must_use]
pub fn album_cover_img_from_album(host: &str, album: &ApiAlbum, size: u16) -> Containers {
    #[allow(clippy::cast_sign_loss)]
    #[allow(clippy::cast_possible_truncation)]
    let request_size = (f64::from(size) * 1.33).round() as u16;

    container! {
        image loading=(ImageLoading::Lazy) src=(album_cover_url_from_album(host, album, request_size, request_size)) width=(size) height=(size);
    }
}

/// Renders an album cover image element from an `ApiTrack`.
///
/// Uses lazy loading and requests a higher resolution image for better display quality.
#[must_use]
pub fn album_cover_img_from_track(host: &str, track: &ApiTrack, size: u16) -> Containers {
    #[allow(clippy::cast_sign_loss)]
    #[allow(clippy::cast_possible_truncation)]
    let request_size = (f64::from(size) * 1.33).round() as u16;

    container! {
        image loading=(ImageLoading::Lazy) src=(album_cover_url_from_track(host, track, request_size, request_size)) width=(size) height=(size);
    }
}

/// Renders a loading placeholder for an album page.
///
/// Shows a skeleton UI that will be replaced by the actual album content via `HyperChad`.
#[must_use]
pub fn album_page_immediate(
    album_id: &str,
    source: Option<&ApiSource>,
    version_source: Option<&TrackApiSource>,
    sample_rate: Option<u32>,
    bit_depth: Option<u8>,
) -> Containers {
    let path = album_page_url(
        album_id,
        true,
        source,
        version_source,
        sample_rate,
        bit_depth,
    );
    container! {
        div
            hx-get=(path)
            hx-trigger="load"
            padding-x=60
            padding-y=20
        {
            div direction=row {
                @let size = 200;
                div width=(size) height=(size + 30) {
                    image loading=(ImageLoading::Lazy) src=(public_img!("album.svg")) width=(size) height=(size);
                }
                div {
                    h1 { "loading..." }
                    h2 { "loading..." }
                }
            }
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
                    tbody {
                        tr {
                            td { "loading..." }
                            td { "loading..." }
                            td { anchor { "loading..." } }
                            td { "loading..." }
                        }
                    }
                }
            }
        }
    }
}

/// Renders the full album detail page content.
///
/// Displays album information, cover art, version selector, and track listing.
#[allow(clippy::too_many_lines)]
#[must_use]
pub fn album_page_content(
    state: &State,
    album: &ApiAlbum,
    versions: &[ApiAlbumVersion],
    selected_version: Option<&ApiAlbumVersion>,
) -> Containers {
    fn same_version(a: &AlbumVersionQuality, b: &AlbumVersionQuality) -> bool {
        a.source == b.source && a.sample_rate == b.sample_rate && a.bit_depth == b.bit_depth
    }

    let Some(connection) = &state.connection else {
        return container! {};
    };
    let host = &connection.api_url;

    let selected_version = versions
        .iter()
        .find(|x| selected_version.is_some_and(|v| same_version(&(*x).into(), &v.into())))
        .or_else(|| versions.first());

    container! {
        div padding-x=60 padding-y=20 {
            div direction=row {
                @let size = 200;
                div width=(size) height=(size) padding-right=15 {
                    (album_cover_img_from_album(host, &album, size))
                }
                div {
                    h1 { (album.title) }
                    anchor href={"/artists?artistId="(album.artist_id)"&source="(album.api_source)} {
                        h2 { (album.artist) }
                    }
                    @if let Some(date_released) = &album.date_released{
                        h2 { (format_date_string(date_released, "%B %d, %Y")) }
                    }
                    div direction=row {
                        @for version in album.versions.iter().cloned() {
                            @let selected = selected_version.is_some_and(|x| same_version(&version, &x.into()));
                            anchor href=(
                                album_page_url(
                                    &album.album_id.to_string(),
                                    false,
                                    Some(&album.api_source),
                                    Some(&version.source),
                                    version.sample_rate,
                                    version.bit_depth,
                                )
                            ) {
                                h3 {
                                    (if selected { "*" } else { "" })
                                    (version.into_formatted())
                                    (if selected { "*" } else { "" })
                                }
                            }
                        }
                    }
                }
            }
            div direction=row padding-y=20 gap=8 {
                button
                    direction=row
                    width=130
                    height=40
                    background=#fff
                    border-radius=5
                    justify-content=center
                    align-items=center
                    gap=8
                    fx-click=(Action::PlayAlbum {
                        album_id: album.album_id.clone(),
                        api_source: album.api_source.clone(),
                        version_source: selected_version.map(|x| x.source.clone()),
                        sample_rate: selected_version.and_then(|x| x.sample_rate),
                        bit_depth: selected_version.and_then(|x| x.bit_depth),
                    })
                {
                    @let icon_size = 12;
                    image
                        width=(icon_size)
                        height=(icon_size)
                        src=(public_img!("play-button.svg"));
                    "Play"
                }
                button
                    direction=row
                    width=130
                    height=40
                    background=#fff
                    border-radius=5
                    justify-content=center
                    align-items=center
                    gap=8
                    fx-click=(Action::AddAlbumToQueue {
                        album_id: album.album_id.clone(),
                        api_source: album.api_source.clone(),
                        version_source: selected_version.map(|x| x.source.clone()),
                        sample_rate: selected_version.and_then(|x| x.sample_rate),
                        bit_depth: selected_version.and_then(|x| x.bit_depth),
                    })
                {
                    @let icon_size = 20;
                    image
                        width=(icon_size)
                        height=(icon_size)
                        src=(public_img!("more-options.svg"));
                    "Options"
                }
                @if let Some(selected) = selected_version {
                    @let source = &selected.source;

                    @if let TrackApiSource::Api(api_source) = source {
                        @let album_id = album.album_sources
                            .iter()
                            .find(|x| &x.source == api_source)
                            .map(|x| x.id.clone());

                        @if let Some(album_id) = album_id {
                            button
                                direction=row
                                width=130
                                height=40
                                background=#fff
                                border-radius=5
                                justify-content=center
                                align-items=center
                                gap=8
                                hx-post={
                                    "/download"
                                        "?source="(api_source)
                                        "&albumId="(album_id)
                                }
                            {
                                "Download"
                            }

                            @if album.album_sources.iter().any(|x| x.source.is_library()) {
                                button
                                    direction=row
                                    width=130
                                    height=40
                                    background=#fff
                                    border-radius=5
                                    justify-content=center
                                    align-items=center
                                    gap=8
                                    hx-delete={
                                        "/library"
                                            "?source="(api_source)
                                            "&albumId="(album_id)
                                    }
                                {
                                    "Remove from Library"
                                }
                            } @else {
                                button
                                    direction=row
                                    width=130
                                    height=40
                                    background=#fff
                                    border-radius=5
                                    justify-content=center
                                    align-items=center
                                    gap=8
                                    hx-post={
                                        "/library"
                                            "?source="(api_source)
                                            "&albumId="(album_id)
                                    }
                                {
                                    "Add to Library"
                                }
                            }
                        }
                    }
                }
            }
            @if let Some(version) = selected_version {
                div {
                    div direction=row {
                        div padding-x=10 height=50 justify-content=center { "#" }
                        div padding-x=10 height=50 justify-content=center { "Title" }
                        div padding-x=10 height=50 justify-content=center { "Artist" }
                        div padding-x=10 height=50 justify-content=center { "Time" }
                    }
                    (album_page_tracks_table_body_from_state(state, &version))
                }
            }
        }
    }
}

/// Renders the track list table body for an album version.
///
/// Includes interactive hover effects and playback controls for each track.
#[must_use]
pub fn album_page_tracks_table_body(
    version: &ApiAlbumVersion,
    track_id: Option<&Id>,
) -> Containers {
    container! {
        @for track in &version.tracks {
            @let current_track = track_id.is_some_and(|x| x == &track.track_id);
            div
                direction=row
                border-radius=5
                data-track-id=(track.track_id)
                fx-hover=fx {
                    set_background_self("#444");
                    set_visibility_child_class(Visibility::Hidden, "track-number");
                    set_visibility_child_class(Visibility::Hidden, "track-playing");
                    set_visibility_child_class(Visibility::Visible, "play-button");
                }
                fx-global-play-track=fx {
                    if get_event_value() == get_data_attr_value_self("track-id") {
                        set_background_self("#333");
                        set_visibility_child_class(Visibility::Hidden, "track-number");
                        set_visibility_child_class(Visibility::Hidden, "play-button");
                        set_visibility_child_class(Visibility::Visible, "track-playing");
                    } else {
                        remove_background_self();
                        set_visibility_child_class(Visibility::Hidden, "play-button");
                        set_visibility_child_class(Visibility::Hidden, "track-playing");
                        set_visibility_child_class(Visibility::Visible, "track-number");
                    }
                }
                background=[if current_track { Some("#333") } else { None }]
            {
                div padding-x=10 height=50 justify-content=center {
                    span
                        .track-number
                        visibility=(if current_track { Visibility::Hidden } else { Visibility::Visible })
                    {
                        (track.number)
                    }
                    span
                        .track-playing
                        visibility=(if current_track { Visibility::Visible } else { Visibility::Hidden })
                    {
                        @let icon_size = 12;
                        image
                            width=(icon_size)
                            height=(icon_size)
                            src=(public_img!("audio-white.svg"));
                    }
                    button
                        .play-button
                        visibility=hidden
                        fx-click=(Action::PlayAlbumStartingAtTrackId {
                            album_id: track.album_id.clone(),
                            start_track_id: track.track_id.clone(),
                            api_source: track.api_source.clone(),
                            version_source: Some(version.source.clone()),
                            sample_rate: version.sample_rate,
                            bit_depth: version.bit_depth,
                        })
                    {
                        @let icon_size = 12;
                        image
                            width=(icon_size)
                            height=(icon_size)
                            src=(public_img!("play-button-white.svg"));
                    }
                }
                div padding-x=10 height=50 justify-content=center {
                    (track.title)
                }
                div padding-x=10 height=50 justify-content=center {
                    anchor href={"/artists?artistId="(track.artist_id)"&source="(track.api_source)} { (track.artist) }
                }
                div padding-x=10 height=50 justify-content=center {
                    (track.duration.into_formatted())
                }
            }
        }
    }
}

/// Renders the track list table body with current playback state.
///
/// Highlights the currently playing track if it belongs to this album.
#[must_use]
pub fn album_page_tracks_table_body_from_state(
    state: &State,
    version: &ApiAlbumVersion,
) -> Containers {
    if let Some(playback) = &state.player.playback {
        let track: Option<&ApiTrack> = playback.tracks.get(playback.position as usize);

        if let Some(track) = track {
            return album_page_tracks_table_body(version, Some(&track.track_id));
        }
    }

    album_page_tracks_table_body(version, None)
}

/// Renders a complete album page within the application layout.
#[must_use]
pub fn album(
    state: &State,
    album_id: &str,
    source: Option<&ApiSource>,
    version_source: Option<&TrackApiSource>,
    sample_rate: Option<u32>,
    bit_depth: Option<u8>,
) -> Containers {
    page(
        state,
        &album_page_immediate(album_id, source, version_source, sample_rate, bit_depth),
    )
}

/// Renders the initial album list with lazy loading triggers.
///
/// Sets up `HyperChad` requests for parallel loading of additional album pages.
#[must_use]
pub fn albums_list_start(
    state: &State,
    albums: &Page<ApiAlbum>,
    filtered_sources: &[TrackApiSource],
    sort: AlbumSort,
    size: u16,
    search: &str,
) -> Containers {
    static MAX_PARALLEL_REQUESTS: u32 = 6;
    static MIN_PAGE_THRESHOLD: u32 = 30;

    let Some(connection) = &state.connection else {
        return container! {};
    };

    let filtered_sources = filtered_sources_to_string(filtered_sources);
    let sort = sort.to_string();
    let limit = albums.limit();
    let offset = albums.offset() + limit;
    let remaining = if albums.has_more() {
        albums.remaining().map_or_else(
            || {
                container! {
                    div
                        hx-get={
                            "/albums-list-start"
                            (build_query('?', &[
                                ("offset", &offset.to_string()),
                                ("limit", &limit.to_string()),
                                ("size", &size.to_string()),
                                ("sources", &filtered_sources),
                                ("sort", &sort),
                                ("search", search),
                            ]))
                        }
                        hx-trigger="load"
                        hidden=(true)
                    {}
                }
            },
            |remaining| {
                let limit = remaining / MAX_PARALLEL_REQUESTS;
                let last = limit + (remaining % MAX_PARALLEL_REQUESTS);

                container! {
                    @if limit < MIN_PAGE_THRESHOLD {
                        div
                            hx-get={
                                "/albums-list"
                                (build_query('?', &[
                                    ("offset", &offset.to_string()),
                                    ("limit", &remaining.to_string()),
                                    ("size", &size.to_string()),
                                    ("sources", &filtered_sources),
                                    ("sort", &sort),
                                    ("search", search),
                                ]))
                            }
                            hx-trigger="load"
                            hidden=(true)
                        {}
                    } @else {
                        @for i in 0..MAX_PARALLEL_REQUESTS {
                            @if i == MAX_PARALLEL_REQUESTS - 1 {
                                div
                                    hx-get={
                                        "/albums-list"
                                        (build_query('?', &[
                                            ("offset", &(offset + i * limit).to_string()),
                                            ("limit", &last.to_string()),
                                            ("size", &size.to_string()),
                                            ("sources", &filtered_sources),
                                            ("sort", &sort),
                                            ("search", search),
                                        ]))
                                    }
                                    hx-trigger="load"
                                    hidden=(true)
                                {}
                            } @else {
                                div
                                    hx-get={
                                        "/albums-list"
                                        (build_query('?', &[
                                            ("offset", &(offset + i * limit).to_string()),
                                            ("limit", &limit.to_string()),
                                            ("size", &size.to_string()),
                                            ("sources", &filtered_sources),
                                            ("sort", &sort),
                                            ("search", search),
                                        ]))
                                    }
                                    hx-trigger="load"
                                    hidden=(true)
                                {}
                            }
                        }
                    }
                }
            },
        )
    } else {
        container! {}
    };

    let host = &connection.api_url;

    container! {
        (show_albums(host, albums.iter(), size))
        (remaining)
    }
}

/// Renders a page of albums without lazy loading setup.
#[must_use]
pub fn albums_list(host: &str, albums: &Page<ApiAlbum>, size: u16) -> Containers {
    show_albums(host, albums.iter(), size)
}

/// Renders a single album card with cover art and optional details.
///
/// Can include playback controls and album metadata based on flags.
#[allow(clippy::too_many_lines)]
#[must_use]
pub fn album_display(
    host: &str,
    album: &ApiAlbum,
    size: u16,
    show_details: bool,
    show_media_controls: bool,
) -> Containers {
    let album_page_url = album_page_url(
        &album.album_id.to_string(),
        false,
        Some(&album.api_source),
        None,
        None,
        None,
    );

    let details = if show_details {
        let artist_page_url = artist_page_url(&album.artist_id.to_string());

        container! {
            div align-items=center text-align=center {
                div {
                    anchor href=(album_page_url) { (album.title) }
                }
                div {
                    anchor href=(artist_page_url) { (album.artist) }
                }
                @if let Some(date_released) = &album.date_released {
                    div {
                        (format_date_string(date_released, "%Y"))
                    }
                }
                div {
                    (display_album_version_qualities(album.versions.iter().cloned(), Some(25)))
                }
            }
        }
    } else {
        container! {}
    };

    let album_cover = if show_media_controls {
        container! {
            div
                width=(size)
                height=(size)
                position=relative
                fx-hover=fx { show_last_child() }
            {
                (album_cover_img_from_album(host, album, size))
                div
                    width=(size)
                    height=(size)
                    position=absolute
                    visibility=hidden
                {
                    @let button_size = size / 4;
                    @let icon_size = size / 10;
                    button
                        direction=row
                        position=absolute
                        bottom=5%
                        left=5%
                        width=(button_size)
                        height=(button_size)
                        justify-content=center
                        align-items=center
                        background=#fff
                        border-radius=(button_size)
                        fx-click=(Action::PlayAlbum {
                            album_id: album.album_id.clone(),
                            api_source: album.api_source.clone(),
                            version_source: None,
                            sample_rate: None,
                            bit_depth: None,
                        })
                    {
                        image
                            width=(icon_size)
                            height=(icon_size)
                            src=(public_img!("play-button.svg"));
                    }
                    @let icon_size = size / 7;
                    button
                        direction=row
                        position=absolute
                        bottom=5%
                        right=5%
                        width=(button_size)
                        height=(button_size)
                        justify-content=center
                        align-items=center
                        background=#fff
                        border-radius=(button_size)
                        fx-click=(Action::AddAlbumToQueue {
                            album_id: album.album_id.clone(),
                            api_source: album.api_source.clone(),
                            version_source: None,
                            sample_rate: None,
                            bit_depth: None,
                        })
                    {
                        image
                            width=(icon_size)
                            height=(icon_size)
                            src=(public_img!("more-options.svg"));
                    }
                }
            }
        }
    } else {
        container! { (album_cover_img_from_album(host, album, size)) }
    };

    container! {
        div width=(size) gap=5 {
            anchor href=(album_page_url) width=(size) {
                (album_cover)
            }
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

/// Constructs a URL for the albums page with filters and sort order.
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

/// Constructs a URL for a specific album page with version parameters.
#[must_use]
pub fn album_page_url(
    album_id: &str,
    full: bool,
    api_source: Option<&ApiSource>,
    version_source: Option<&TrackApiSource>,
    sample_rate: Option<u32>,
    bit_depth: Option<u8>,
) -> String {
    format!(
        "/albums?albumId={album_id}{}{}{}{}{}",
        if full { "&full=true" } else { "" },
        api_source.map_or_else(String::new, |x| format!("&source={x}")),
        version_source.map_or_else(String::new, |x| format!("&versionSource={x}")),
        sample_rate.map_or_else(String::new, |x| format!("&sampleRate={x}")),
        bit_depth.map_or_else(String::new, |x| format!("&bitDepth={x}")),
    )
}

/// Renders a collection of album cards.
///
/// Displays albums with details and media controls enabled.
#[must_use]
pub fn show_albums<'a>(
    host: &str,
    albums: impl Iterator<Item = &'a ApiAlbum>,
    size: u16,
) -> Containers {
    container! {
        @for album in albums {
            (album_display(host, album, size, true, true))
        }
    }
}

/// Renders the albums page content with filters and sort controls.
///
/// Includes a filter menu, search box, and album grid with lazy loading.
#[allow(clippy::too_many_lines)]
#[must_use]
pub fn albums_page_content(
    filtered_sources: &[TrackApiSource],
    sort: AlbumSort,
    search: Option<&str>,
) -> Containers {
    let size: u16 = 200;

    container! {
        div background=(DARK_BACKGROUND) {
            div padding-x=30 padding-y=15 {
                div direction=row align-items=center {
                    h1 { "Albums" }
                    @let button_size = 30;
                    @let icon_size = button_size - 10;
                    div position=relative width=(button_size) height=(button_size) {
                        button
                            direction=row
                            width=(button_size)
                            height=(button_size)
                            justify-content=center
                            align-items=center
                            fx-click=fx { show("albums-menu") }
                        {
                            image
                                width=(icon_size)
                                height=(icon_size)
                                src=(public_img!("more-options-white.svg"));
                        }
                        div
                            #albums-menu
                            width=300
                            position=absolute
                            top=100%
                            visibility=hidden
                            background=(DARK_BACKGROUND)
                            border-radius=5
                            direction=row
                            fx-click-outside=fx { hide_self() }
                        {
                            div {
                                div {
                                    @let url = albums_page_url(
                                        filtered_sources,
                                        if sort == AlbumSort::ArtistDesc {
                                            AlbumSort::ArtistAsc
                                        } else {
                                            AlbumSort::ArtistDesc
                                        }
                                    );
                                    button fx-click=fx { navigate(url) } {
                                        "Album Artist"
                                    }
                                }
                                div border-top="1, #222" {
                                    @let url = albums_page_url(
                                        filtered_sources,
                                        if sort == AlbumSort::NameDesc {
                                            AlbumSort::NameAsc
                                        } else {
                                            AlbumSort::NameDesc
                                        }
                                    );
                                    button fx-click=fx { navigate(url) } {
                                        "Album Name"
                                    }
                                }
                                div border-top="1, #222" {
                                    @let url = albums_page_url(
                                        filtered_sources,
                                        if sort == AlbumSort::ReleaseDateDesc {
                                            AlbumSort::ReleaseDateAsc
                                        } else {
                                            AlbumSort::ReleaseDateDesc
                                        }
                                    );
                                    button fx-click=fx { navigate(url) } {
                                        "Album Release Date"
                                    }
                                }
                                div border-top="1, #222" {
                                    @let url = albums_page_url(
                                        filtered_sources,
                                        if sort == AlbumSort::DateAddedDesc {
                                            AlbumSort::DateAddedAsc
                                        } else {
                                            AlbumSort::DateAddedDesc
                                        }
                                    );
                                    button fx-click=fx { navigate(url) } {
                                        "Album Date Added"
                                    }
                                }
                            }
                            div {
                                @for source in TrackApiSource::all() {
                                    div direction=row {
                                        @let checked = filtered_sources.iter().any(|x| x == source);
                                        (source.to_string())
                                        input
                                            fx-change=fx {
                                                navigate(
                                                    albums_page_url(&if checked {
                                                        filtered_sources.iter().filter(|x| *x != source).cloned().collect::<Vec<_>>()
                                                    } else {
                                                        [filtered_sources, slice::from_ref(source)].concat()
                                                    }, sort)
                                                )
                                            }
                                            type=checkbox
                                            checked=(checked);
                                    }
                                }
                            }
                        }
                    }
                }
                input
                    type=text
                    placeholder="Filter..."
                    value=[search]
                    fx-change=fx {
                        invoke(Action::FilterAlbums {
                            filtered_sources: filtered_sources.to_vec(),
                            sort
                        }, get_event_value());
                    };
            }
        }
        (load_albums(size, sort, filtered_sources, search.unwrap_or("")))
    }
}

/// Renders the complete albums page within the application layout.
#[must_use]
pub fn albums(
    state: &State,
    filtered_sources: &[TrackApiSource],
    sort: AlbumSort,
    search: Option<&str>,
) -> Containers {
    page(state, &albums_page_content(filtered_sources, sort, search))
}

/// Renders the album grid container with loading placeholders.
///
/// Sets up `HyperChad` to load actual album data on page load.
#[must_use]
pub fn load_albums(
    size: u16,
    sort: AlbumSort,
    filtered_sources: &[TrackApiSource],
    search: &str,
) -> Containers {
    container! {
        div
            #albums
            hx-get={
                "/albums-list-start"
                (build_query('?', &[
                    ("limit", "100"),
                    ("size", &size.to_string()),
                    ("sources", &filtered_sources_to_string(filtered_sources)),
                    ("sort", &sort.to_string()),
                    ("search", search),
                ]))
            }
            hx-trigger="load"
            hx-swap="children"
            direction=row
            overflow-x=(LayoutOverflow::Wrap { grid: true })
            grid-cell-size=(size)
            justify-content=space-evenly
            gap=15
            padding-x=30
            padding-y=15
        {
            @for _ in 0..100 {
                div width=(size) height=(size + 30) {
                    image loading=(ImageLoading::Lazy) src=(public_img!("album.svg")) width=(size) height=(size);
                }
            }
        }
    }
}
