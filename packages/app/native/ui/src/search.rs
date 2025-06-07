#![allow(clippy::module_name_repetitions)]

use hyperchad::{
    actions::ActionType,
    transformer_models::{AlignItems, Position, TextAlign, Visibility},
};
use maud::{Markup, html};
use moosicbox_music_api_models::search::api::{
    ApiGlobalAlbumSearchResult, ApiGlobalArtistSearchResult, ApiGlobalSearchResult,
    ApiGlobalTrackSearchResult, ApiSearchResultsResponse,
};
use moosicbox_music_models::ApiSource;

use crate::{pre_escaped, public_img};

#[must_use]
pub fn search<'a>(
    results: impl Iterator<Item = (&'a ApiSource, &'a ApiSearchResultsResponse)>,
    open: bool,
) -> Markup {
    html! {
        div
            id="search"
            sx-visibility=(if open { Visibility::Visible } else { Visibility::Hidden })
            sx-padding=(20)
            sx-gap=(10)
            sx-position=(Position::Fixed)
            sx-top=(0)
            sx-left=(0)
            sx-right=(0)
            sx-bottom=(0)
            sx-background="#00000088"
        {
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

                    (search_results(results))
                }
            }
            button
                id="close-search-button"
                sx-border-radius=(100)
                sx-background="#fff"
                sx-border="2, #222"
                sx-padding=(10)
                sx-margin-x=(20)
                sx-margin-y=(10)
                sx-position=(Position::Fixed)
                sx-top=(0)
                sx-right=(0)
                fx-click=(ActionType::hide_str_id("search").and(ActionType::show_str_id("search-button")))
            {
                img
                    sx-width=(20)
                    sx-height=(20)
                    src=(public_img!("cross.svg"));
            }
        }
        button
            id="search-button"
            sx-visibility=(if open { Visibility::Hidden } else { Visibility::Visible })
            sx-border-radius=(100)
            sx-background="#fff"
            sx-border="2, #222"
            sx-padding=(10)
            sx-margin-x=(20)
            sx-margin-y=(10)
            sx-position=(Position::Fixed)
            sx-top=(0)
            sx-right=(0)
            fx-click=(ActionType::hide_self().and(ActionType::show_str_id("search")))
        {
            img
                sx-width=(20)
                sx-height=(20)
                src=(public_img!("magnifying-glass.svg"));
        }
    }
}

pub fn search_results<'a>(
    results: impl Iterator<Item = (&'a ApiSource, &'a ApiSearchResultsResponse)>,
) -> Markup {
    html! {
        div id="search-results" sx-gap=(10) {
            @for (source, results) in results {
                (source.to_string_display())
                (results_content(&results.results))
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
    let source = artist.api_source.clone();
    html! {
        div {
            a href={(pre_escaped!("/artists?artistId="))(artist_id)(pre_escaped!("&source="))(source)} {
                (artist.title)
            }
        }
    }
}

#[must_use]
fn album_result(album: &ApiGlobalAlbumSearchResult) -> Markup {
    let album_id = album.album_id.clone();
    let source = album.api_source.clone();
    html! {
        div {
            a href={(pre_escaped!("/albums?albumId="))(album_id)(pre_escaped!("&source="))(source)} {
                (album.title)
            }
        }
    }
}

#[must_use]
fn track_result(track: &ApiGlobalTrackSearchResult) -> Markup {
    let album_id = track.album_id.clone();
    let title = track.title.clone();
    let source = track.api_source.clone();
    html! {
        div {
            a href={(pre_escaped!("/albums?albumId="))(album_id)(pre_escaped!("&source="))(source)} {
                (title)
            }
        }
    }
}
