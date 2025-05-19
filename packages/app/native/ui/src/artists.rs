#![allow(clippy::module_name_repetitions)]

use hyperchad::transformer_models::{ImageLoading, LayoutOverflow};
use maud::{Markup, PreEscaped, html};
use moosicbox_music_models::{
    AlbumType, ApiSource,
    api::{ApiAlbum, ApiArtist},
    id::Id,
};

use crate::{
    formatting::{AlbumTypeFormat as _, ApiSourceFormat},
    page, pre_escaped, public_img,
    state::State,
};

#[must_use]
pub fn artist_page_url(artist_id: &str) -> PreEscaped<String> {
    pre_escaped!("/artists?artistId={artist_id}")
}

fn artist_cover_url(artist: &ApiArtist, width: u16, height: u16) -> String {
    if artist.contains_cover {
        format!(
            "{}/files/artists/{}/{width}x{height}?moosicboxProfile=master",
            std::env::var("MOOSICBOX_HOST")
                .as_deref()
                .unwrap_or("http://localhost:8016"),
            artist.artist_id
        )
    } else {
        public_img!("album.svg").to_string()
    }
}

fn artist_cover_img(artist: &ApiArtist, size: u16) -> Markup {
    #[allow(clippy::cast_sign_loss)]
    #[allow(clippy::cast_possible_truncation)]
    let request_size = (f64::from(size) * 1.33).round() as u16;

    html! {
        img loading=(ImageLoading::Lazy) src=(artist_cover_url(&artist, request_size, request_size)) sx-width=(size) sx-height=(size);
    }
}

#[must_use]
pub fn artist_page_content(artist: &ApiArtist) -> Markup {
    fn source_html(artist_id: &Id, source: ApiSource, album_type: AlbumType, size: u16) -> Markup {
        html! {
            div
                hx-get=(pre_escaped!("/artists/albums-list?artistId={artist_id}&size={size}&source={source}&albumType={album_type}"))
                hx-trigger="load"
                sx-hidden=(true)
            {}
        }
    }

    let size = 200;

    let mut sources = vec![];

    {
        let artist_id = artist.artist_id.clone();
        let source = ApiSource::Library;
        sources.extend(vec![
            source_html(&artist_id, source, AlbumType::Lp, size),
            source_html(&artist_id, source, AlbumType::EpsAndSingles, size),
            source_html(&artist_id, source, AlbumType::Compilations, size),
        ]);
    }

    for source in &*artist.api_sources {
        let artist_id = source.id.clone();
        let source = source.source;
        sources.extend(vec![
            source_html(&artist_id, source, AlbumType::Lp, size),
            source_html(&artist_id, source, AlbumType::EpsAndSingles, size),
            source_html(&artist_id, source, AlbumType::Compilations, size),
        ]);
    }

    html! {
        div sx-padding-x=(60) sx-padding-y=(20) {
            div sx-padding-y=(20) {
                "Back"
            }
            div sx-dir="row" {
                div sx-width=(size) sx-height=(size) sx-padding-right=(15) {
                    (artist_cover_img(&artist, size))
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

#[must_use]
pub fn artist(state: &State, artist: &ApiArtist) -> Markup {
    page(state, &artist_page_content(artist))
}

#[must_use]
pub fn artists_page_content(artists: &[ApiArtist]) -> Markup {
    let size: u16 = 200;
    #[allow(clippy::cast_sign_loss)]
    #[allow(clippy::cast_possible_truncation)]
    let request_size = (f64::from(size) * 1.33).round() as u16;

    html! {
        div
            sx-dir="row"
            sx-overflow-x=(LayoutOverflow::Wrap { grid: true })
            sx-grid-cell-size=(size)
            sx-justify-content="space-evenly"
            sx-gap=(15)
            sx-padding-x=(30)
            sx-padding-y=(15)
        {
            @for artist in artists {
                a href=(artist_page_url(&artist.artist_id.to_string())) sx-width=(size) {
                    div sx-width=(size) {
                        img loading=(ImageLoading::Lazy) src=(artist_cover_url(artist, request_size, request_size)) sx-width=(size) sx-height=(size);
                        (artist.title)
                    }
                }
            }
        }
    }
}

#[must_use]
pub fn artists(state: &State, artists: &[ApiArtist]) -> Markup {
    page(state, &artists_page_content(artists))
}

#[must_use]
pub fn albums_list(
    albums: &[ApiAlbum],
    source: ApiSource,
    album_type: AlbumType,
    size: u16,
) -> Markup {
    if albums.is_empty() {
        return html! {};
    }

    html! {
        div sx-padding-y=(20) {
            h2 {
                (album_type.into_formatted())
                @if source == ApiSource::Library {
                    " in "
                } @else {
                    " on "
                }
                (source.into_formatted())
            }
            div
                sx-dir="row"
                sx-overflow-x=(LayoutOverflow::Wrap { grid: true })
                sx-grid-cell-size=(size)
                sx-justify-content="space-evenly"
                sx-gap=(15)
                sx-padding-y=(15)
            {
                (crate::albums::show_albums(albums.iter(), size))
            }
        }
    }
}
