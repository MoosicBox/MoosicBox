//! Global search UI components.
//!
//! This module provides UI templates for searching across artists, albums, and tracks
//! from multiple music API sources.

#![allow(clippy::module_name_repetitions)]

#[allow(unused_imports)]
use hyperchad::template as hyperchad_template;
use hyperchad::{
    actions::Key,
    template::{Containers, container},
    transformer::models::Visibility,
};
use moosicbox_music_api_models::search::api::{
    ApiGlobalAlbumSearchResult, ApiGlobalArtistSearchResult, ApiGlobalSearchResult,
    ApiGlobalTrackSearchResult,
};
use moosicbox_music_models::ApiSource;

use crate::{
    BACKGROUND, albums::album_cover_url, artists::artist_cover_url, formatting::classify_name,
    public_img, state::State,
};

/// Renders the global search interface.
///
/// Displays a search modal with input field, results tabs for different API sources,
/// and a floating search button when closed.
#[allow(clippy::too_many_lines)]
#[must_use]
pub fn search(state: &State, api_sources: &[ApiSource], searched: bool, open: bool) -> Containers {
    container! {
        div
            #search
            visibility=(if open { Visibility::Visible } else { Visibility::Hidden })
            padding=20
            gap=10
            position=fixed
            top=0
            left=0
            right=0
            bottom=0
            background=#00000088
        {
            section
                align-items=start
                width=100%
                height=100%
            {
                div
                    align-items=end
                    gap=10
                    width=100%
                    height=100%
                {
                    form
                        hx-post="/search"
                        width=100%
                        direction=row
                        gap=5
                        padding=10
                    {
                        div flex-grow=1 {
                            input
                                #search-input
                                type=text
                                flex-grow=1
                                name="query"
                                placeholder="Search..."
                                fx-global-key-down=fx {
                                    if get_event_value() == Key::Escape {
                                        let search = element_by_id("search");
                                        if search.visibility() == visible() {
                                            search.hide();
                                            element_by_id("search-button").show();
                                        }
                                    }
                                };
                        }
                        button
                            type=submit
                            border-radius=5
                            background=#111
                            border="2, #222"
                            padding-x=10
                            padding-y=5
                        {
                            "Search"
                        }
                        button
                            #close-search-button
                            border-radius=100
                            background=#fff
                            border="2, #222"
                            padding=10
                            fx-click=fx {
                                element_by_id("search").hide();
                                element_by_id("search-button").show();
                            }
                        {
                            image
                                width=20
                                height=20
                                src=(public_img!("cross.svg"));
                        }
                    }

                    @if let Some(host) = state.connection.as_ref().map(|x| x.api_url.as_str()) {
                        (search_results(host, api_sources, None, searched))
                    }
                }
            }
        }
        button
            #search-button
            visibility=(if open { Visibility::Hidden } else { Visibility::Visible })
            border-radius=100
            background=#fff
            border="2, #222"
            padding=10
            margin-x=20
            margin-y=10
            position=fixed
            top=0
            right=0
            fx-click=fx {
                hide_self();
                element_by_id("search").show();
                let input = element_by_id("search-input");
                input.focus();
                input.select();
            }
        {
            image
                width=20
                height=20
                src=(public_img!("magnifying-glass.svg"));
        }
    }
}

/// Renders the search results container with source tabs.
///
/// Displays tabbed results organized by API source, with the first source
/// or specified source selected by default.
#[must_use]
pub fn search_results(
    host: &str,
    api_sources: &[ApiSource],
    selected: Option<&ApiSource>,
    _searched: bool,
) -> Containers {
    let selected = selected.or_else(|| api_sources.first());

    container! {
        div #search-results width=100% gap=10 overflow-y=auto {
            div {
                div direction=row gap=10 {
                    @for source in api_sources {
                        @let id = results_content_container_id(source);

                        div
                            border-top-left-radius=5
                            border-top-right-radius=5
                            padding=10
                            background=(BACKGROUND)
                            fx-click=fx {
                                no_display_class("search-results-container");
                                display_by_id(id);
                            }
                        {
                            (source.to_string_display())
                        }
                    }
                }
                div background=(BACKGROUND) {
                    @for source in api_sources {
                        @let id = results_content_container_id(source);
                        @let selected = selected.is_some_and(|x| x == source);

                        div id=(id) .search-results-container hidden=(!selected) {
                            (results_content(host, source, &[]))
                        }
                    }
                }
            }
        }
    }
}

/// Generates the DOM element ID for a search results container.
///
/// Creates a unique ID for the container holding search results from a specific API source.
#[must_use]
pub fn results_content_container_id(api_source: &ApiSource) -> String {
    format!("search-results-container-{}", classify_name(api_source))
}

/// Generates the DOM element ID for search results content.
///
/// Creates a unique ID for the search results list from a specific API source.
#[must_use]
pub fn results_content_id(api_source: &ApiSource) -> String {
    format!("search-results-{}", classify_name(api_source))
}

/// Renders search results for a specific API source.
///
/// Displays artists, albums, and tracks matching the search query with clickable links.
#[must_use]
pub fn results_content(
    host: &str,
    api_source: &ApiSource,
    results: &[ApiGlobalSearchResult],
) -> Containers {
    container! {
        @let id = results_content_id(api_source);

        div
            id=(id)
            width=100%
            gap=10
            overflow-y=auto
        {
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
}

/// Renders a search result item for an artist.
///
/// Displays artist cover image and name with a link to the artist page.
#[must_use]
fn artist_result(host: &str, artist: &ApiGlobalArtistSearchResult) -> Containers {
    let artist_id = artist.artist_id.clone();
    let source = artist.api_source.clone();
    container! {
        div direction=row {
            @let size = 70;
            anchor href={"/artists?artistId="(artist_id)"&source="(source)} {
                image
                    src=(artist_cover_url(host, &artist_id, &source, artist.contains_cover, size, size))
                    width=(size)
                    height=(size);
            }
            anchor href={"/artists?artistId="(artist_id)"&source="(source)} {
                (artist.title)
            }
        }
    }
}

/// Renders a search result item for an album.
///
/// Displays album cover image and title with a link to the album page.
#[must_use]
fn album_result(host: &str, album: &ApiGlobalAlbumSearchResult) -> Containers {
    let album_id = album.album_id.clone();
    let source = album.api_source.clone();
    container! {
        div direction=row {
            @let size = 70;
            anchor href={"/albums?albumId="(album_id)"&source="(source)} {
                image
                    src=(album_cover_url(host, &album_id, &source, album.contains_cover, size, size))
                    width=(size)
                    height=(size);
            }
            anchor href={"/albums?albumId="(album_id)"&source="(source)} {
                (album.title)
            }
        }
    }
}

/// Renders a search result item for a track.
///
/// Displays album cover image and track title with a link to the album page.
#[must_use]
fn track_result(host: &str, track: &ApiGlobalTrackSearchResult) -> Containers {
    let album_id = track.album_id.clone();
    let title = track.title.clone();
    let source = track.api_source.clone();
    container! {
        div direction=row {
            @let size = 70;
            anchor href={"/albums?albumId="(album_id)"&source="(source)} {
                image
                    src=(album_cover_url(host, &album_id, &source, track.contains_cover, size, size))
                    width=(size)
                    height=(size);
            }
            anchor href={"/albums?albumId="(album_id)"&source="(source)} {
                (title)
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    mod results_content_container_id_tests {
        use super::*;

        #[test_log::test]
        fn test_generates_id_with_library_source() {
            let result = results_content_container_id(&ApiSource::library());
            assert_eq!(result, "search-results-container-library");
        }

        #[test_log::test]
        fn test_generates_lowercase_hyphenated_id() {
            // The classify_name function converts to lowercase and replaces non-alphanumeric with hyphens
            let result = results_content_container_id(&ApiSource::library());
            assert!(result.starts_with("search-results-container-"));
            // The id should contain only lowercase alphanumeric and hyphens
            assert!(
                result
                    .chars()
                    .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '-')
            );
        }
    }

    mod results_content_id_tests {
        use super::*;

        #[test_log::test]
        fn test_generates_id_with_library_source() {
            let result = results_content_id(&ApiSource::library());
            assert_eq!(result, "search-results-library");
        }

        #[test_log::test]
        fn test_generates_different_id_than_container() {
            let container_id = results_content_container_id(&ApiSource::library());
            let content_id = results_content_id(&ApiSource::library());
            assert_ne!(container_id, content_id);
            assert!(container_id.contains("container"));
            assert!(!content_id.contains("container"));
        }
    }
}
