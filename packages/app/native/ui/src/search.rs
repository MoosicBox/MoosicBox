#![allow(clippy::module_name_repetitions)]

use hyperchad::{
    actions::{
        ActionType,
        logic::{Value, get_display_str_id},
    },
    transformer_models::{AlignItems, LayoutDirection, LayoutOverflow, Position, Visibility},
};
use maud::{Markup, html};
use moosicbox_music_api_models::search::api::{
    ApiGlobalAlbumSearchResult, ApiGlobalArtistSearchResult, ApiGlobalSearchResult,
    ApiGlobalTrackSearchResult, ApiSearchResultsResponse,
};
use moosicbox_music_models::ApiSource;

use crate::{
    DARK_BACKGROUND, albums::album_cover_url, artists::artist_cover_url, formatting::classify_name,
    pre_escaped, public_img, state::State,
};

#[must_use]
pub fn search<'a>(
    state: &State,
    results: impl Iterator<Item = (&'a ApiSource, &'a ApiSearchResultsResponse)>,
    searched: bool,
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
            section
                sx-align-items=(AlignItems::Start)
                sx-width="100%"
                sx-height="100%"
            {
                div
                    sx-align-items=(AlignItems::End)
                    sx-gap=(10)
                    sx-width="100%"
                    sx-height="100%"
                {
                    form
                        hx-post="/search"
                        sx-width="100%"
                        sx-dir=(LayoutDirection::Row)
                        sx-gap=(5)
                        sx-padding=(10)
                    {
                        div sx-flex-grow=(1) {
                            input sx-flex-grow=(1) type="text" name="query" placeholder="Search...";
                        }
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
                        button
                            id="close-search-button"
                            sx-border-radius=(100)
                            sx-background="#fff"
                            sx-border="2, #222"
                            sx-padding=(10)
                            fx-click=(ActionType::hide_str_id("search").and(ActionType::show_str_id("search-button")))
                        {
                            img
                                sx-width=(20)
                                sx-height=(20)
                                src=(public_img!("cross.svg"));
                        }
                    }

                    @if let Some(host) = state.connection.as_ref().map(|x| x.api_url.as_str()) {
                        (search_results(host, results, None, searched))
                    }
                }
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
    host: &str,
    results: impl Iterator<Item = (&'a ApiSource, &'a ApiSearchResultsResponse)>,
    selected: Option<&ApiSource>,
    searched: bool,
) -> Markup {
    let results = results.collect::<Vec<_>>();
    let selected = selected.or_else(|| results.first().map(|x| x.0));

    html! {
        div id="search-results" sx-width="100%" sx-gap=(10) sx-overflow-y=(LayoutOverflow::Auto) {
            @if searched {
                div {
                    div sx-dir=(LayoutDirection::Row) sx-gap=(10) {
                        @for (source, _) in &results {
                            @let id = format!("search-results-{}", classify_name(source));

                            div
                                sx-border-top-left-radius=(5)
                                sx-border-top-right-radius=(5)
                                sx-padding=(10)
                                sx-background=(DARK_BACKGROUND)
                                fx-click=(
                                    get_display_str_id(&id)
                                        .eq(Value::Display(true))
                                        .then(ActionType::no_display_str_id(&id))
                                        .or_else(ActionType::display_str_id(&id))
                                )
                            {
                                (source.to_string_display())
                            }
                        }
                    }
                    div sx-background=(DARK_BACKGROUND) {
                        @for (source, results) in results {
                            @let id = format!("search-results-{}", classify_name(source));

                            div
                                sx-hidden=(selected.is_none_or(|x| x != source))
                                id=(id)
                                sx-width="100%"
                                sx-gap=(10)
                                sx-overflow-y=(LayoutOverflow::Auto)
                            {
                                @if results.results.is_empty() {
                                    h2 { "No Results" }
                                } @else {
                                    (results_content(host, &results.results))
                                }
                            }
                        }
                    }
                }
            }
        }
    }
}

#[must_use]
pub fn results_content(host: &str, results: &[ApiGlobalSearchResult]) -> Markup {
    html! {
        @for result in results {
            @match result {
                ApiGlobalSearchResult::Artist(artist) => {
                    (artist_result(host, artist))
                }
                ApiGlobalSearchResult::Album(album) => {
                    (album_result(host, album))
                }
                ApiGlobalSearchResult::Track(track) => {
                    (track_result(host, track))
                }
            }
        }
    }
}

#[must_use]
fn artist_result(host: &str, artist: &ApiGlobalArtistSearchResult) -> Markup {
    let artist_id = artist.artist_id.clone();
    let source = artist.api_source.clone();
    html! {
        div sx-dir=(LayoutDirection::Row) {
            @let size = 70;
            img
                src=(artist_cover_url(host, &artist_id, &source, artist.contains_cover, size, size))
                sx-width=(size)
                sx-height=(size);
            a href={(pre_escaped!("/artists?artistId="))(artist_id)(pre_escaped!("&source="))(source)} {
                (artist.title)
            }
        }
    }
}

#[must_use]
fn album_result(host: &str, album: &ApiGlobalAlbumSearchResult) -> Markup {
    let album_id = album.album_id.clone();
    let source = album.api_source.clone();
    html! {
        div sx-dir=(LayoutDirection::Row) {
            @let size = 70;
            img
                src=(album_cover_url(host, &album_id, &source, album.contains_cover, size, size))
                sx-width=(size)
                sx-height=(size);
            a href={(pre_escaped!("/albums?albumId="))(album_id)(pre_escaped!("&source="))(source)} {
                (album.title)
            }
        }
    }
}

#[must_use]
fn track_result(host: &str, track: &ApiGlobalTrackSearchResult) -> Markup {
    let album_id = track.album_id.clone();
    let title = track.title.clone();
    let source = track.api_source.clone();
    html! {
        div sx-dir=(LayoutDirection::Row) {
            @let size = 70;
            img
                src=(album_cover_url(host, &album_id, &source, track.contains_cover, size, size))
                sx-width=(size)
                sx-height=(size);
            a href={(pre_escaped!("/albums?albumId="))(album_id)(pre_escaped!("&source="))(source)} {
                (title)
            }
        }
    }
}
