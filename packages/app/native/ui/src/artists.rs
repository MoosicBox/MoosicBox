#![allow(clippy::module_name_repetitions)]

use maud::{html, Markup};
use moosicbox_library_models::{ApiArtist, ApiLibraryArtist};

use crate::{page, public_img, state::State};

fn artist_cover_url(artist: &ApiLibraryArtist, width: u16, height: u16) -> String {
    if artist.contains_cover {
        format!(
            "{}/files/artists/{}/{width}x{height}?moosicboxProfile=master",
            std::env::var("MOOSICBOX_HOST")
                .as_deref()
                .unwrap_or("http://localhost:8500"),
            artist.artist_id
        )
    } else {
        public_img!("album.svg").to_string()
    }
}

fn artist_cover_img(artist: &ApiLibraryArtist, size: u16) -> Markup {
    #[allow(clippy::cast_sign_loss)]
    #[allow(clippy::cast_possible_truncation)]
    let request_size = (f64::from(size) * 1.33).round() as u16;

    html! {
        img src=(artist_cover_url(&artist, request_size, request_size)) sx-width=(size) sx-height=(size);
    }
}

#[must_use]
pub fn artist_page_content(artist: ApiArtist) -> Markup {
    let ApiArtist::Library(artist) = artist;

    html! {
        div sx-dir="row" {
            @let size = 200;
            div sx-width=(size) sx-height=(size + 30) {
                (artist_cover_img(&artist, size))
            }
            div {
                h1 { (artist.title) }
            }
        }
    }
}

#[must_use]
pub fn artist(state: &State, artist: ApiArtist) -> Markup {
    page(state, &artist_page_content(artist))
}

#[must_use]
pub fn artists_page_content(artists: Vec<ApiArtist>) -> Markup {
    let artists = artists
        .into_iter()
        .map(|x| {
            let ApiArtist::Library(x) = x;
            x
        })
        .collect::<Vec<_>>();

    let size: u16 = 200;
    #[allow(clippy::cast_sign_loss)]
    #[allow(clippy::cast_possible_truncation)]
    let request_size = (f64::from(size) * 1.33).round() as u16;

    html! {
        div sx-dir="row" sx-overflow-x="wrap" sx-overflow-y="show" sx-justify-content="space-evenly" sx-gap=(15) {
            @for artist in &artists {
                a href={"/artists?artistId="(artist.artist_id)} sx-width=(size) sx-height=(size + 30) {
                    div sx-width=(size) sx-height=(size + 30) {
                        img src=(artist_cover_url(artist, request_size, request_size)) sx-width=(size) sx-height=(size);
                        (artist.title)
                    }
                }
            }
        }
    }
}

#[must_use]
pub fn artists(state: &State, artists: Vec<ApiArtist>) -> Markup {
    page(state, &artists_page_content(artists))
}
