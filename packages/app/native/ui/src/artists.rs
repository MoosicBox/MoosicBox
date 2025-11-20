//! Artist display and navigation components.
//!
//! This module provides UI templates for rendering artist lists, artist detail pages,
//! and artist cover artwork.

#![allow(clippy::module_name_repetitions)]

use hyperchad::{
    template::{self as hyperchad_template, Containers, container},
    transformer::models::{ImageLoading, LayoutOverflow},
};
use moosicbox_music_models::{
    AlbumType, ApiSource,
    api::{ApiAlbum, ApiArtist},
    id::Id,
};

use crate::{
    formatting::{AlbumTypeFormat as _, ApiSourceFormat},
    page, public_img,
    state::State,
};

/// Constructs a URL for a specific artist page.
#[must_use]
pub fn artist_page_url(artist_id: &str) -> String {
    format!("/artists?artistId={artist_id}")
}

/// Constructs a URL for an artist cover image from an `ApiArtist`.
#[must_use]
pub fn artist_cover_url_from_artist(
    host: &str,
    artist: &ApiArtist,
    width: u16,
    height: u16,
) -> String {
    artist_cover_url(
        host,
        &artist.artist_id,
        &artist.api_source,
        artist.contains_cover,
        width,
        height,
    )
}

/// Constructs a URL for an artist cover image.
///
/// Returns a placeholder image URL if the artist does not contain cover art.
#[must_use]
pub fn artist_cover_url(
    host: &str,
    artist_id: &Id,
    source: &ApiSource,
    contains_cover: bool,
    width: u16,
    height: u16,
) -> String {
    if contains_cover {
        format!(
            "{host}/files/artists/{artist_id}/{width}x{height}?moosicboxProfile=master&source={source}",
        )
    } else {
        public_img!("album.svg").to_string()
    }
}

/// Renders an artist cover image element with lazy loading.
///
/// Requests a higher resolution image (1.33x) for better display quality.
fn artist_cover_img(host: &str, artist: &ApiArtist, size: u16) -> Containers {
    #[allow(clippy::cast_sign_loss)]
    #[allow(clippy::cast_possible_truncation)]
    let request_size = (f64::from(size) * 1.33).round() as u16;

    container! {
        image loading=(ImageLoading::Lazy) src=(artist_cover_url_from_artist(host, &artist, request_size, request_size)) width=(size) height=(size);
    }
}

/// Renders the artist detail page content.
///
/// Displays artist information and albums grouped by type and source, loaded via `HyperChad`.
#[must_use]
pub fn artist_page_content(state: &State, artist: &ApiArtist) -> Containers {
    fn source_html(
        artist_id: &Id,
        source: &ApiSource,
        album_type: AlbumType,
        size: u16,
    ) -> Containers {
        container! {
            div
                hx-get={
                    "/artists/albums-list"
                    "?artistId="(artist_id)
                    "&size="(size)
                    "&source="(source)
                    "&albumType="(album_type)
                }
                hx-trigger="load"
                hidden=(true)
            {}
        }
    }

    let Some(connection) = &state.connection else {
        return container! {};
    };

    let size = 200;

    let mut sources = vec![];

    {
        let artist_id = artist.artist_id.clone();
        let source = ApiSource::library_ref();
        sources.extend(vec![
            source_html(&artist_id, source, AlbumType::Lp, size),
            source_html(&artist_id, source, AlbumType::EpsAndSingles, size),
            source_html(&artist_id, source, AlbumType::Compilations, size),
        ]);
    }

    for source in &artist.api_sources {
        let artist_id = source.id.clone();
        let source = &source.source;
        sources.extend(vec![
            source_html(&artist_id, source, AlbumType::Lp, size),
            source_html(&artist_id, source, AlbumType::EpsAndSingles, size),
            source_html(&artist_id, source, AlbumType::Compilations, size),
        ]);
    }

    container! {
        div padding-x=60 padding-y=20 {
            div padding-y=20 {
                "Back"
            }
            div direction=row {
                div width=(size) height=(size) padding-right=15 {
                    (artist_cover_img(&connection.api_url, &artist, size))
                }
                div {
                    h1 { (artist.title) }
                }
            }
            @for source in sources {
                (source)
            }
        }
    }
}

/// Renders a complete artist page within the application layout.
#[must_use]
pub fn artist(state: &State, artist: &ApiArtist) -> Containers {
    page(state, &artist_page_content(state, artist))
}

/// Renders the artists list page content.
///
/// Displays artists in a grid layout with cover images.
#[must_use]
pub fn artists_page_content(state: &State, artists: &[ApiArtist]) -> Containers {
    let Some(connection) = &state.connection else {
        return container! {};
    };

    let size: u16 = 200;
    #[allow(clippy::cast_sign_loss)]
    #[allow(clippy::cast_possible_truncation)]
    let request_size = (f64::from(size) * 1.33).round() as u16;

    container! {
        div
            direction=row
            overflow-x=(LayoutOverflow::Wrap { grid: true })
            grid-cell-size=(size)
            justify-content=space-evenly
            gap=15
            padding-x=30
            padding-y=15
        {
            @for artist in artists {
                anchor href=(artist_page_url(&artist.artist_id.to_string())) width=(size) {
                    div width=(size) {
                        image
                            loading=(ImageLoading::Lazy)
                            src=(artist_cover_url_from_artist(&connection.api_url, artist, request_size, request_size))
                            width=(size)
                            height=(size);

                        (artist.title)
                    }
                }
            }
        }
    }
}

/// Renders the complete artists page within the application layout.
#[must_use]
pub fn artists(state: &State, artists: &[ApiArtist]) -> Containers {
    page(state, &artists_page_content(state, artists))
}

/// Renders an album list section for an artist page.
///
/// Groups albums by type and source with a descriptive header.
#[must_use]
pub fn albums_list(
    host: &str,
    albums: &[ApiAlbum],
    source: ApiSource,
    album_type: AlbumType,
    size: u16,
) -> Containers {
    if albums.is_empty() {
        return container! {};
    }

    container! {
        div padding-y=20 {
            h2 {
                (album_type.into_formatted())
                @if source.is_library() {
                    " in "
                } @else {
                    " on "
                }
                (source.into_formatted())
            }
            div
                direction=row
                overflow-x=(LayoutOverflow::Wrap { grid: true })
                grid-cell-size=(size)
                justify-content=space-evenly
                gap=15
                padding-y=15
            {
                (crate::albums::show_albums(host, albums.iter(), size))
            }
        }
    }
}
