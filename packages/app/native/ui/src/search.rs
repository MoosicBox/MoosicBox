#![allow(clippy::module_name_repetitions)]

use hyperchad::transformer_models::{AlignItems, TextAlign};
use maud::{Markup, html};
use moosicbox_search_models::api::{
    ApiGlobalAlbumSearchResult, ApiGlobalArtistSearchResult, ApiGlobalSearchResult,
    ApiGlobalTrackSearchResult,
};

use crate::pre_escaped;

#[must_use]
pub fn search(results: &[ApiGlobalSearchResult]) -> Markup {
    html! {
        div id="search" sx-padding=(20) sx-gap=(10) {
            section sx-align-items=(AlignItems::Start) {
                div sx-align-items=(AlignItems::End) sx-gap=(10) {
                    form
                        hx-post="/search"
                        sx-width="100%"
                        sx-align-items=(AlignItems::End)
                        sx-gap=(5)
                    {
                        div { "Query: " input type="text" name="query"; }
                        button
                            type="submit"
                            sx-border-radius=(5)
                            sx-background="#111"
                            sx-border="2, #222"
                            sx-padding-x=(10)
                            sx-padding-y=(5)
                        {
                            "Search"
                        }
                    }

                    div sx-width="100%" sx-text-align=(TextAlign::Start) {
                        h2 { "Search Results" }
                    }

                    (results_content(results))
                }
            }
        }
    }
}

#[must_use]
pub fn results_content(results: &[ApiGlobalSearchResult]) -> Markup {
    html! {
        @for result in results {
            @match result {
                ApiGlobalSearchResult::Artist(artist) => {
                    (artist_result(artist))
                }
                ApiGlobalSearchResult::Album(album) => {
                    (album_result(album))
                }
                ApiGlobalSearchResult::Track(track) => {
                    (track_result(track))
                }
            }
        }
    }
}

#[must_use]
fn artist_result(artist: &ApiGlobalArtistSearchResult) -> Markup {
    let artist_id = artist.artist_id.clone();
    html! {
        div {
            a href={(pre_escaped!("/artists?artistId="))(artist_id)} {
                (artist.title)
            }
        }
    }
}

#[must_use]
fn album_result(album: &ApiGlobalAlbumSearchResult) -> Markup {
    let album_id = album.album_id.clone();
    html! {
        div {
            a href={(pre_escaped!("/albums?albumId="))(album_id)} {
                (album.title)
            }
        }
    }
}

#[must_use]
fn track_result(track: &ApiGlobalTrackSearchResult) -> Markup {
    let album_id = track.album_id.clone();
    let title = track.title.clone();
    html! {
        div {
            a href={(pre_escaped!("/albums?albumId="))(album_id)} {
                (title)
            }
        }
    }
}
