#![allow(clippy::module_name_repetitions)]

use hyperchad::{
    template2::{self as hyperchad_template2, Containers, container},
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

#[must_use]
pub fn artist_page_url(artist_id: &str) -> String {
    format!("/artists?artistId={artist_id}")
}

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

fn artist_cover_img(host: &str, artist: &ApiArtist, size: u16) -> Containers {
    #[allow(clippy::cast_sign_loss)]
    #[allow(clippy::cast_possible_truncation)]
    let request_size = (f64::from(size) * 1.33).round() as u16;

    container! {
        Image loading=(ImageLoading::Lazy) src=(artist_cover_url_from_artist(host, &artist, request_size, request_size)) width=(size) height=(size);
    }
}

#[must_use]
pub fn artist_page_content(state: &State, artist: &ApiArtist) -> Containers {
    fn source_html(
        artist_id: &Id,
        source: &ApiSource,
        album_type: AlbumType,
        size: u16,
    ) -> Containers {
        container! {
            Div
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
        Div padding-x=(60) padding-y=(20) {
            Div padding-y=(20) {
                "Back"
            }
            Div direction="row" {
                Div width=(size) height=(size) padding-right=(15) {
                    (artist_cover_img(&connection.api_url, &artist, size))
                }
                Div {
                    H1 { (artist.title) }
                }
            }
            @for source in sources {
                (source)
            }
        }
    }
}

#[must_use]
pub fn artist(state: &State, artist: &ApiArtist) -> Containers {
    page(state, &artist_page_content(state, artist))
}

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
        Div
            direction="row"
            overflow-x=(LayoutOverflow::Wrap { grid: true })
            grid-cell-size=(size)
            justify-content="space-evenly"
            gap=(15)
            padding-x=(30)
            padding-y=(15)
        {
            @for artist in artists {
                Anchor href=(artist_page_url(&artist.artist_id.to_string())) width=(size) {
                    Div width=(size) {
                        Image
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

#[must_use]
pub fn artists(state: &State, artists: &[ApiArtist]) -> Containers {
    page(state, &artists_page_content(state, artists))
}

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
        Div padding-y=(20) {
            H2 {
                (album_type.into_formatted())
                @if source.is_library() {
                    " in "
                } @else {
                    " on "
                }
                (source.into_formatted())
            }
            Div
                direction="row"
                overflow-x=(LayoutOverflow::Wrap { grid: true })
                grid-cell-size=(size)
                justify-content="space-evenly"
                gap=(15)
                padding-y=(15)
            {
                (crate::albums::show_albums(host, albums.iter(), size))
            }
        }
    }
}
