use moosicbox_core::{
    app::AppState,
    sqlite::{
        db::{get_albums, DbError},
        models::{Album, AlbumSort, AlbumSource},
    },
};
use serde::{Deserialize, Serialize};
use thiserror::Error;

#[derive(Debug, Serialize, Deserialize, Clone)]
#[serde(rename_all = "camelCase")]
pub struct AlbumsRequest {
    pub sources: Option<Vec<AlbumSource>>,
    pub sort: Option<AlbumSort>,
    pub filters: AlbumFilters,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
#[serde(rename_all = "camelCase")]
pub struct AlbumFilters {
    pub name: Option<String>,
    pub artist: Option<String>,
    pub search: Option<String>,
}

pub fn filter_albums(albums: Vec<Album>, request: &AlbumsRequest) -> Vec<Album> {
    albums
        .into_iter()
        .filter(|album| {
            !request
                .sources
                .as_ref()
                .is_some_and(|s| !s.contains(&album.source))
        })
        .filter(|album| {
            !request
                .filters
                .name
                .as_ref()
                .is_some_and(|s| !album.title.to_lowercase().contains(s))
        })
        .filter(|album| {
            !request
                .filters
                .artist
                .as_ref()
                .is_some_and(|s| !&album.artist.to_lowercase().contains(s))
        })
        .filter(|album| {
            !request.filters.search.as_ref().is_some_and(|s| {
                !(album.title.to_lowercase().contains(s) || album.artist.to_lowercase().contains(s))
            })
        })
        .collect()
}

pub fn sort_albums(mut albums: Vec<Album>, request: &AlbumsRequest) -> Vec<Album> {
    match request.sort {
        Some(AlbumSort::ArtistAsc) => albums.sort_by(|a, b| a.artist.cmp(&b.artist)),
        Some(AlbumSort::NameAsc) => albums.sort_by(|a, b| a.title.cmp(&b.title)),
        Some(AlbumSort::ArtistDesc) => albums.sort_by(|a, b| b.artist.cmp(&a.artist)),
        Some(AlbumSort::NameDesc) => albums.sort_by(|a, b| b.title.cmp(&a.title)),
        _ => (),
    }
    match request.sort {
        Some(AlbumSort::ArtistAsc) => {
            albums.sort_by(|a, b| a.artist.to_lowercase().cmp(&b.artist.to_lowercase()))
        }
        Some(AlbumSort::NameAsc) => {
            albums.sort_by(|a, b| a.title.to_lowercase().cmp(&b.title.to_lowercase()))
        }
        Some(AlbumSort::ArtistDesc) => {
            albums.sort_by(|a, b| b.artist.to_lowercase().cmp(&a.artist.to_lowercase()))
        }
        Some(AlbumSort::NameDesc) => {
            albums.sort_by(|a, b| b.title.to_lowercase().cmp(&a.title.to_lowercase()))
        }
        Some(AlbumSort::ReleaseDateAsc) => {
            albums.sort_by(|a, b| a.clone().date_released.cmp(&b.clone().date_released))
        }
        Some(AlbumSort::ReleaseDateDesc) => {
            albums.sort_by(|b, a| a.clone().date_released.cmp(&b.clone().date_released))
        }
        Some(AlbumSort::DateAddedAsc) => {
            albums.sort_by(|a, b| a.clone().date_added.cmp(&b.clone().date_added))
        }
        Some(AlbumSort::DateAddedDesc) => {
            albums.sort_by(|b, a| a.clone().date_added.cmp(&b.clone().date_added))
        }
        None => (),
    }

    albums
}

#[derive(Debug, Error)]
pub enum GetAlbumsError {
    #[error(transparent)]
    DbError(#[from] DbError),
}

pub async fn get_all_albums(
    data: &AppState,
    request: &AlbumsRequest,
) -> Result<Vec<Album>, GetAlbumsError> {
    let albums = get_albums(&data.db.as_ref().unwrap().library.lock().unwrap())?;

    Ok(sort_albums(filter_albums(albums, request), request))
}

#[cfg(test)]
mod test {
    use super::*;

    #[test]
    fn filter_albums_empty_albums_returns_empty_albums() {
        let albums = vec![];
        let result = filter_albums(
            albums,
            &AlbumsRequest {
                sources: None,
                sort: None,
                filters: AlbumFilters {
                    name: None,
                    artist: None,
                    search: None,
                },
            },
        );
        assert_eq!(result, vec![]);
    }

    #[test]
    fn filter_albums_filters_albums_of_sources_that_dont_match() {
        let local = Album {
            id: 0,
            title: "".to_string(),
            artist: "".to_string(),
            artwork: None,
            source: AlbumSource::Local,
            ..Default::default()
        };
        let tidal = Album {
            id: 0,
            title: "".to_string(),
            artist: "".to_string(),
            artwork: None,
            source: AlbumSource::Tidal,
            ..Default::default()
        };
        let qobuz = Album {
            id: 0,
            title: "".to_string(),
            artist: "".to_string(),
            artwork: None,
            source: AlbumSource::Qobuz,
            ..Default::default()
        };
        let albums = vec![local.clone(), tidal, qobuz];
        let result = filter_albums(
            albums,
            &AlbumsRequest {
                sources: Some(vec![AlbumSource::Local]),
                sort: None,
                filters: AlbumFilters {
                    name: None,
                    artist: None,
                    search: None,
                },
            },
        );
        assert_eq!(result, vec![local]);
    }

    #[test]
    fn filter_albums_filters_albums_of_name_that_dont_match() {
        let bob = Album {
            id: 0,
            title: "bob".to_string(),
            artist: "".to_string(),
            artwork: None,
            source: AlbumSource::Local,
            ..Default::default()
        };
        let sally = Album {
            id: 0,
            title: "sally".to_string(),
            artist: "".to_string(),
            artwork: None,
            source: AlbumSource::Local,
            ..Default::default()
        };
        let test = Album {
            id: 0,
            title: "test".to_string(),
            artist: "".to_string(),
            artwork: None,
            source: AlbumSource::Local,
            ..Default::default()
        };
        let albums = vec![bob, sally, test.clone()];
        let result = filter_albums(
            albums,
            &AlbumsRequest {
                sources: None,
                sort: None,
                filters: AlbumFilters {
                    name: Some("test".to_string()),
                    artist: None,
                    search: None,
                },
            },
        );
        assert_eq!(result, vec![test]);
    }

    #[test]
    fn filter_albums_filters_albums_of_name_that_dont_match_and_searches_multiple_words() {
        let bob = Album {
            id: 0,
            title: "bob".to_string(),
            artist: "".to_string(),
            artwork: None,
            source: AlbumSource::Local,
            ..Default::default()
        };
        let sally = Album {
            id: 0,
            title: "sally".to_string(),
            artist: "".to_string(),
            artwork: None,
            source: AlbumSource::Local,
            ..Default::default()
        };
        let test = Album {
            id: 0,
            title: "one test two".to_string(),
            artist: "".to_string(),
            artwork: None,
            source: AlbumSource::Local,
            ..Default::default()
        };
        let albums = vec![bob, sally, test.clone()];
        let result = filter_albums(
            albums,
            &AlbumsRequest {
                sources: None,
                sort: None,
                filters: AlbumFilters {
                    name: Some("test".to_string()),
                    artist: None,
                    search: None,
                },
            },
        );
        assert_eq!(result, vec![test]);
    }

    #[test]
    fn filter_albums_filters_albums_of_artist_that_dont_match() {
        let bob = Album {
            id: 0,
            title: "".to_string(),
            artist: "bob".to_string(),
            artwork: None,
            source: AlbumSource::Local,
            ..Default::default()
        };
        let sally = Album {
            id: 0,
            title: "".to_string(),
            artist: "sally".to_string(),
            artwork: None,
            source: AlbumSource::Local,
            ..Default::default()
        };
        let test = Album {
            id: 0,
            title: "".to_string(),
            artist: "test".to_string(),
            artwork: None,
            source: AlbumSource::Local,
            ..Default::default()
        };
        let albums = vec![bob, sally, test.clone()];
        let result = filter_albums(
            albums,
            &AlbumsRequest {
                sources: None,
                sort: None,
                filters: AlbumFilters {
                    name: None,
                    artist: Some("test".to_string()),
                    search: None,
                },
            },
        );
        assert_eq!(result, vec![test]);
    }

    #[test]
    fn filter_albums_filters_albums_of_artist_that_dont_match_and_searches_multiple_words() {
        let bob = Album {
            id: 0,
            title: "".to_string(),
            artist: "bob".to_string(),
            artwork: None,
            source: AlbumSource::Local,
            ..Default::default()
        };
        let sally = Album {
            id: 0,
            title: "".to_string(),
            artist: "sally".to_string(),
            artwork: None,
            source: AlbumSource::Local,
            ..Default::default()
        };
        let test = Album {
            id: 0,
            title: "".to_string(),
            artist: "one test two".to_string(),
            artwork: None,
            source: AlbumSource::Local,
            ..Default::default()
        };
        let albums = vec![bob, sally, test.clone()];
        let result = filter_albums(
            albums,
            &AlbumsRequest {
                sources: None,
                sort: None,
                filters: AlbumFilters {
                    name: None,
                    artist: Some("test".to_string()),
                    search: None,
                },
            },
        );
        assert_eq!(result, vec![test]);
    }

    #[test]
    fn filter_albums_filters_albums_of_search_that_dont_match_artist() {
        let bob = Album {
            id: 0,
            title: "".to_string(),
            artist: "bob".to_string(),
            artwork: None,
            source: AlbumSource::Local,
            ..Default::default()
        };
        let sally = Album {
            id: 0,
            title: "".to_string(),
            artist: "sally".to_string(),
            artwork: None,
            source: AlbumSource::Local,
            ..Default::default()
        };
        let test = Album {
            id: 0,
            title: "".to_string(),
            artist: "test".to_string(),
            artwork: None,
            source: AlbumSource::Local,
            ..Default::default()
        };
        let albums = vec![bob, sally, test.clone()];
        let result = filter_albums(
            albums,
            &AlbumsRequest {
                sources: None,
                sort: None,
                filters: AlbumFilters {
                    name: None,
                    artist: None,
                    search: Some("test".to_string()),
                },
            },
        );
        assert_eq!(result, vec![test]);
    }

    #[test]
    fn filter_albums_filters_albums_of_search_that_dont_match_artist_and_searches_multiple_words() {
        let bob = Album {
            id: 0,
            title: "".to_string(),
            artist: "bob".to_string(),
            artwork: None,
            source: AlbumSource::Local,
            ..Default::default()
        };
        let sally = Album {
            id: 0,
            title: "".to_string(),
            artist: "sally".to_string(),
            artwork: None,
            source: AlbumSource::Local,
            ..Default::default()
        };
        let test = Album {
            id: 0,
            title: "".to_string(),
            artist: "one test two".to_string(),
            artwork: None,
            source: AlbumSource::Local,
            ..Default::default()
        };
        let albums = vec![bob, sally, test.clone()];
        let result = filter_albums(
            albums,
            &AlbumsRequest {
                sources: None,
                sort: None,
                filters: AlbumFilters {
                    name: None,
                    artist: None,
                    search: Some("test".to_string()),
                },
            },
        );
        assert_eq!(result, vec![test]);
    }

    #[test]
    fn filter_albums_filters_albums_of_search_that_dont_match_name() {
        let bob = Album {
            id: 0,
            title: "bob".to_string(),
            artist: "".to_string(),
            artwork: None,
            source: AlbumSource::Local,
            ..Default::default()
        };
        let sally = Album {
            id: 0,
            title: "sally".to_string(),
            artist: "".to_string(),
            artwork: None,
            source: AlbumSource::Local,
            ..Default::default()
        };
        let test = Album {
            id: 0,
            title: "test".to_string(),
            artist: "".to_string(),
            artwork: None,
            source: AlbumSource::Local,
            ..Default::default()
        };
        let albums = vec![bob, sally, test.clone()];
        let result = filter_albums(
            albums,
            &AlbumsRequest {
                sources: None,
                sort: None,
                filters: AlbumFilters {
                    name: None,
                    artist: None,
                    search: Some("test".to_string()),
                },
            },
        );
        assert_eq!(result, vec![test]);
    }

    #[test]
    fn filter_albums_filters_albums_of_search_that_dont_match_name_and_searches_multiple_words() {
        let bob = Album {
            id: 0,
            title: "bob".to_string(),
            artist: "".to_string(),
            artwork: None,
            source: AlbumSource::Local,
            ..Default::default()
        };
        let sally = Album {
            id: 0,
            title: "sally".to_string(),
            artist: "".to_string(),
            artwork: None,
            source: AlbumSource::Local,
            ..Default::default()
        };
        let test = Album {
            id: 0,
            title: "one test two".to_string(),
            artist: "".to_string(),
            artwork: None,
            source: AlbumSource::Local,
            ..Default::default()
        };
        let albums = vec![bob, sally, test.clone()];
        let result = filter_albums(
            albums,
            &AlbumsRequest {
                sources: None,
                sort: None,
                filters: AlbumFilters {
                    name: None,
                    artist: None,
                    search: Some("test".to_string()),
                },
            },
        );
        assert_eq!(result, vec![test]);
    }

    #[test]
    fn filter_albums_filters_albums_of_search_that_dont_match_and_searches_across_properties() {
        let bob = Album {
            id: 0,
            title: "bob".to_string(),
            artist: "test".to_string(),
            artwork: None,
            source: AlbumSource::Local,
            ..Default::default()
        };
        let sally = Album {
            id: 0,
            title: "sally".to_string(),
            artist: "".to_string(),
            artwork: None,
            source: AlbumSource::Local,
            ..Default::default()
        };
        let test = Album {
            id: 0,
            title: "one test two".to_string(),
            artist: "".to_string(),
            artwork: None,
            source: AlbumSource::Local,
            ..Default::default()
        };
        let albums = vec![bob.clone(), sally, test.clone()];
        let result = filter_albums(
            albums,
            &AlbumsRequest {
                sources: None,
                sort: None,
                filters: AlbumFilters {
                    name: None,
                    artist: None,
                    search: Some("test".to_string()),
                },
            },
        );
        assert_eq!(result, vec![bob, test]);
    }
}
