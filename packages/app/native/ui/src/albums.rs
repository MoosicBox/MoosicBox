#![allow(clippy::module_name_repetitions)]

use hyperchad::{
    actions::{
        self as hyperchad_actions, ActionType,
        logic::{
            get_data_attr_value_self, get_event_value, get_visibility_self, get_visibility_str_id,
        },
    },
    template2::{self as hyperchad_template2, Containers, container},
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

#[must_use]
pub fn album_cover_img_from_album(host: &str, album: &ApiAlbum, size: u16) -> Containers {
    #[allow(clippy::cast_sign_loss)]
    #[allow(clippy::cast_possible_truncation)]
    let request_size = (f64::from(size) * 1.33).round() as u16;

    container! {
        Image loading=(ImageLoading::Lazy) src=(album_cover_url_from_album(host, album, request_size, request_size)) width=(size) height=(size);
    }
}

#[must_use]
pub fn album_cover_img_from_track(host: &str, track: &ApiTrack, size: u16) -> Containers {
    #[allow(clippy::cast_sign_loss)]
    #[allow(clippy::cast_possible_truncation)]
    let request_size = (f64::from(size) * 1.33).round() as u16;

    container! {
        Image loading=(ImageLoading::Lazy) src=(album_cover_url_from_track(host, track, request_size, request_size)) width=(size) height=(size);
    }
}

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
        Div
            hx-get=(path)
            hx-trigger="load"
            padding-x=60
            padding-y=20
        {
            Div direction=row {
                @let size = 200;
                Div width=(size) height=(size + 30) {
                    Image loading=(ImageLoading::Lazy) src=(public_img!("album.svg")) width=(size) height=(size);
                }
                Div {
                    H1 { "loading..." }
                    H2 { "loading..." }
                }
            }
            Div {
                Table {
                    THead {
                        TR {
                            TH { "#" }
                            TH { "Title" }
                            TH { "Artist" }
                            TH { "Time" }
                        }
                    }
                    TBody {
                        TR {
                            TD { "loading..." }
                            TD { "loading..." }
                            TD { Anchor { "loading..." } }
                            TD { "loading..." }
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
        Div padding-x=60 padding-y=20 {
            Div direction=row {
                @let size = 200;
                Div width=(size) height=(size) padding-right=15 {
                    (album_cover_img_from_album(host, &album, size))
                }
                Div {
                    H1 { (album.title) }
                    Anchor href={"/artists?artistId="(album.artist_id)"&source="(album.api_source)} {
                        H2 { (album.artist) }
                    }
                    @if let Some(date_released) = &album.date_released{
                        H2 { (format_date_string(date_released, "%B %d, %Y")) }
                    }
                    Div direction=row {
                        @for version in album.versions.iter().cloned() {
                            @let selected = selected_version.is_some_and(|x| same_version(&version, &x.into()));
                            Anchor href=(
                                album_page_url(
                                    &album.album_id.to_string(),
                                    false,
                                    Some(&album.api_source),
                                    Some(&version.source),
                                    version.sample_rate,
                                    version.bit_depth,
                                )
                            ) {
                                H3 {
                                    (if selected { "*" } else { "" })
                                    (version.into_formatted())
                                    (if selected { "*" } else { "" })
                                }
                            }
                        }
                    }
                }
            }
            Div direction=row padding-y=20 gap=8 {
                Button
                    direction=row
                    width=130
                    height=40
                    background="#fff"
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
                    Image
                        width=(icon_size)
                        height=(icon_size)
                        src=(public_img!("play-button.svg"));
                    "Play"
                }
                Button
                    direction=row
                    width=130
                    height=40
                    background="#fff"
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
                    Image
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
                            Button
                                direction=row
                                width=130
                                height=40
                                background="#fff"
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
                                Button
                                    direction=row
                                    width=130
                                    height=40
                                    background="#fff"
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
                                Button
                                    direction=row
                                    width=130
                                    height=40
                                    background="#fff"
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
                Div {
                    Div direction=row {
                        Div padding-x=10 height=50 justify-content=center { "#" }
                        Div padding-x=10 height=50 justify-content=center { "Title" }
                        Div padding-x=10 height=50 justify-content=center { "Artist" }
                        Div padding-x=10 height=50 justify-content=center { "Time" }
                    }
                    (album_page_tracks_table_body_from_state(state, &version))
                }
            }
        }
    }
}

#[must_use]
pub fn album_page_tracks_table_body(
    version: &ApiAlbumVersion,
    track_id: Option<&Id>,
) -> Containers {
    container! {
        @for track in &version.tracks {
            @let current_track = track_id.is_some_and(|x| x == &track.track_id);
            Div
                direction=row
                border-radius=5
                data-track-id=(track.track_id)
                fx-hover=(
                    ActionType::set_background_self("#444")
                        .and(ActionType::set_visibility_child_class(Visibility::Hidden, "track-number"))
                        .and(ActionType::set_visibility_child_class(Visibility::Hidden, "track-playing"))
                        .and(ActionType::set_visibility_child_class(Visibility::Visible, "play-button"))
                )
                fx-event=(ActionType::on_event(
                    "play-track",
                    get_event_value()
                        .eq(get_data_attr_value_self("track-id"))
                        .then(ActionType::Multi(vec![
                            ActionType::set_background_self("#333"),
                            ActionType::set_visibility_child_class(Visibility::Hidden, "track-number"),
                            ActionType::set_visibility_child_class(Visibility::Hidden, "play-button"),
                            ActionType::set_visibility_child_class(Visibility::Visible, "track-playing"),
                        ]))
                        .or_else(ActionType::Multi(vec![
                            ActionType::remove_background_self(),
                            ActionType::set_visibility_child_class(Visibility::Hidden, "play-button"),
                            ActionType::set_visibility_child_class(Visibility::Hidden, "track-playing"),
                            ActionType::set_visibility_child_class(Visibility::Visible, "track-number"),
                        ]))
                ))
                background=[if current_track { Some("#333") } else { None }]
            {
                Div padding-x=10 height=50 justify-content=center {
                    Span
                        .track-number
                        visibility=(if current_track { Visibility::Hidden } else { Visibility::Visible })
                    {
                        (track.number)
                    }
                    Span
                        .track-playing
                        visibility=(if current_track { Visibility::Visible } else { Visibility::Hidden })
                    {
                        @let icon_size = 12;
                        Image
                            width=(icon_size)
                            height=(icon_size)
                            src=(public_img!("audio-white.svg"));
                    }
                    Button
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
                        Image
                            width=(icon_size)
                            height=(icon_size)
                            src=(public_img!("play-button-white.svg"));
                    }
                }
                Div padding-x=10 height=50 justify-content=center {
                    (track.title)
                }
                Div padding-x=10 height=50 justify-content=center {
                    Anchor href={"/artists?artistId="(track.artist_id)"&source="(track.api_source)} { (track.artist) }
                }
                Div padding-x=10 height=50 justify-content=center {
                    (track.duration.into_formatted())
                }
            }
        }
    }
}

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
                    Div
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
                        Div
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
                                Div
                                    hx-get={
                                        "/albums-list{}"
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
                                Div
                                    hx-get={
                                        "/albums-list{}"
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

#[must_use]
pub fn albums_list(host: &str, albums: &Page<ApiAlbum>, size: u16) -> Containers {
    show_albums(host, albums.iter(), size)
}

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
            Div align-items=center {
                Div {
                    Anchor href=(album_page_url) { (album.title) }
                }
                Div {
                    Anchor href=(artist_page_url) { (album.artist) }
                }
                @if let Some(date_released) = &album.date_released {
                    Div {
                        (format_date_string(date_released, "%Y"))
                    }
                }
                Div {
                    (display_album_version_qualities(album.versions.iter().cloned(), Some(25)))
                }
            }
        }
    } else {
        container! {}
    };

    let album_cover = if show_media_controls {
        container! {
            Div
                width=(size)
                height=(size)
                position="relative"
                fx-hover=(ActionType::show_last_child())
            {
                (album_cover_img_from_album(host, album, size))
                Div
                    width=(size)
                    height=(size)
                    position="absolute"
                    visibility="hidden"
                {
                    @let button_size = size / 4;
                    @let icon_size = size / 10;
                    Button
                        direction=row
                        position="absolute"
                        bottom=5%
                        left=5%
                        width=(button_size)
                        height=(button_size)
                        justify-content="center"
                        align-items="center"
                        background="#fff"
                        border-radius=(button_size)
                        fx-click=(Action::PlayAlbum {
                            album_id: album.album_id.clone(),
                            api_source: album.api_source.clone(),
                            version_source: None,
                            sample_rate: None,
                            bit_depth: None,
                        })
                    {
                        Image
                            width=(icon_size)
                            height=(icon_size)
                            src=(public_img!("play-button.svg"));
                    }
                    @let icon_size = size / 7;
                    Button
                        direction=row
                        position="absolute"
                        bottom=5%
                        right=5%
                        width=(button_size)
                        height=(button_size)
                        justify-content="center"
                        align-items="center"
                        background="#fff"
                        border-radius=(button_size)
                        fx-click=(Action::AddAlbumToQueue {
                            album_id: album.album_id.clone(),
                            api_source: album.api_source.clone(),
                            version_source: None,
                            sample_rate: None,
                            bit_depth: None,
                        })
                    {
                        Image
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
        Div width=(size) gap=5 {
            Anchor href=(album_page_url) width=(size) {
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

#[allow(clippy::too_many_lines)]
#[must_use]
pub fn albums_page_content(filtered_sources: &[TrackApiSource], sort: AlbumSort) -> Containers {
    let size: u16 = 200;

    container! {
        Div background=(DARK_BACKGROUND) {
            Div padding-x=30 padding-y=15 {
                Div direction=row align-items=center {
                    H1 { "Albums" }
                    @let button_size = 30;
                    @let icon_size = button_size - 10;
                    Div position="relative" width=(button_size) height=(button_size) {
                        Button
                            direction=row
                            width=(button_size)
                            height=(button_size)
                            justify-content="center"
                            align-items="center"
                            fx-click=(
                                get_visibility_str_id("albums-menu")
                                    .eq(Visibility::Hidden)
                                    .then(ActionType::show_str_id("albums-menu"))
                            )
                        {
                            Image
                                width=(icon_size)
                                height=(icon_size)
                                src=(public_img!("more-options-white.svg"));
                        }
                        Div
                            #albums-menu
                            width=300
                            position="absolute"
                            top=100%
                            visibility="hidden"
                            background=(DARK_BACKGROUND)
                            border-radius=5
                            direction=row
                            fx-click-outside=(
                                get_visibility_self()
                                    .eq(Visibility::Visible)
                                    .then(ActionType::hide_self())
                            )
                        {
                            Div {
                                Div {
                                    Button
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
                                Div border-top="1, #222" {
                                    Button
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
                                Div border-top="1, #222" {
                                    Button
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
                                Div border-top="1, #222" {
                                    Button
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
                            Div {
                                @for source in TrackApiSource::all() {
                                    Div direction=row {
                                        @let checked = filtered_sources.iter().any(|x| x == source);
                                        (source.to_string())
                                        Input
                                            fx-change=(ActionType::Navigate {
                                                url: albums_page_url(&if checked {
                                                    filtered_sources.iter().filter(|x| *x != source).cloned().collect::<Vec<_>>()
                                                } else {
                                                    [filtered_sources, &[source.clone()]].concat()
                                                }, sort)
                                            })
                                            type=checkbox
                                            checked=(checked);
                                    }
                                }
                            }
                        }
                    }
                }
                Input
                    type=text
                    placeholder="Filter..."
                    fx-change=(
                        get_event_value()
                            .then_pass_to(Action::FilterAlbums {
                                filtered_sources: filtered_sources.to_vec(),
                                sort
                            })
                    );
            }
        }
        (load_albums(size, sort, filtered_sources, ""))
    }
}

#[must_use]
pub fn albums(state: &State, filtered_sources: &[TrackApiSource], sort: AlbumSort) -> Containers {
    page(state, &albums_page_content(filtered_sources, sort))
}

#[must_use]
pub fn load_albums(
    size: u16,
    sort: AlbumSort,
    filtered_sources: &[TrackApiSource],
    search: &str,
) -> Containers {
    container! {
        Div
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
            justify-content="space-evenly"
            gap=15
            padding-x=30
            padding-y=15
        {
            @for _ in 0..100 {
                Div width=(size) height=(size + 30) {
                    Image loading=(ImageLoading::Lazy) src=(public_img!("album.svg")) width=(size) height=(size);
                }
            }
        }
    }
}
