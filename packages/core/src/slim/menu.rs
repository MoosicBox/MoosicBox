use crate::{
    app::AppState,
    sqlite::db::{get_albums, get_artists, DbError},
    ToApi,
};
use serde::{Deserialize, Serialize};
use std::str::FromStr;
use thiserror::Error;

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct FullAlbum {
    pub id: String,
    pub title: String,
    pub artist: String,
    pub year: Option<i16>,
    pub artwork: Option<String>,
    pub source: AlbumSource,
}

#[derive(Debug, Serialize, Deserialize, Clone, PartialEq, Default)]
pub struct Artist {
    pub id: i32,
    pub title: String,
    pub cover: Option<String>,
}

impl ToApi<ApiArtist> for Artist {
    fn to_api(&self) -> ApiArtist {
        ApiArtist {
            artist_id: self.id,
            title: self.title.clone(),
            contains_cover: self.cover.is_some(),
        }
    }
}

#[derive(Debug, Serialize, Deserialize, Clone, Default)]
#[serde(rename_all = "camelCase")]
pub struct ApiArtist {
    pub artist_id: i32,
    pub title: String,
    pub contains_cover: bool,
}

#[derive(Debug, Serialize, Deserialize, Clone, PartialEq, Default)]
pub struct Album {
    pub id: i32,
    pub title: String,
    pub artist: String,
    pub artist_id: i32,
    pub date_released: Option<String>,
    pub date_added: Option<String>,
    pub artwork: Option<String>,
    pub directory: Option<String>,
    pub source: AlbumSource,
    pub blur: bool,
}

impl ToApi<ApiAlbum> for Album {
    fn to_api(&self) -> ApiAlbum {
        ApiAlbum {
            album_id: self.id,
            title: self.title.clone(),
            artist: self.artist.clone(),
            artist_id: self.artist_id,
            contains_artwork: self.artwork.is_some(),
            date_released: self.date_released.clone(),
            date_added: self.date_added.clone(),
            source: self.source.clone(),
            blur: self.blur,
        }
    }
}

#[derive(Debug, Serialize, Deserialize, Clone, Default)]
#[serde(rename_all = "camelCase")]
pub struct ApiAlbum {
    pub album_id: i32,
    pub title: String,
    pub artist: String,
    pub artist_id: i32,
    pub contains_artwork: bool,
    pub date_released: Option<String>,
    pub date_added: Option<String>,
    pub source: AlbumSource,
    pub blur: bool,
}

#[derive(Debug, Serialize, Deserialize, Clone, Eq, PartialEq, Default)]
pub enum AlbumSource {
    #[default]
    Local,
    Tidal,
    Qobuz,
}

impl FromStr for AlbumSource {
    type Err = ();

    fn from_str(input: &str) -> Result<AlbumSource, Self::Err> {
        match input.to_lowercase().as_str() {
            "local" => Ok(AlbumSource::Local),
            "tidal" => Ok(AlbumSource::Tidal),
            "qobuz" => Ok(AlbumSource::Qobuz),
            _ => Err(()),
        }
    }
}

#[derive(Debug, Serialize, Deserialize, Clone, Eq, PartialEq)]
pub enum ArtistSort {
    NameAsc,
    NameDesc,
}

impl FromStr for ArtistSort {
    type Err = ();

    fn from_str(input: &str) -> Result<ArtistSort, Self::Err> {
        match input.to_lowercase().as_str() {
            "name-asc" | "name" => Ok(ArtistSort::NameAsc),
            "name-desc" => Ok(ArtistSort::NameDesc),
            _ => Err(()),
        }
    }
}

#[derive(Debug, Serialize, Deserialize, Clone, Eq, PartialEq)]
pub enum AlbumSort {
    ArtistAsc,
    ArtistDesc,
    NameAsc,
    NameDesc,
    ReleaseDateAsc,
    ReleaseDateDesc,
    DateAddedAsc,
    DateAddedDesc,
}

impl FromStr for AlbumSort {
    type Err = ();

    fn from_str(input: &str) -> Result<AlbumSort, Self::Err> {
        match input.to_lowercase().as_str() {
            "artist-asc" | "artist" => Ok(AlbumSort::ArtistAsc),
            "artist-desc" => Ok(AlbumSort::ArtistDesc),
            "name-asc" | "name" => Ok(AlbumSort::NameAsc),
            "name-desc" => Ok(AlbumSort::NameDesc),
            "release-date-asc" | "release-date" => Ok(AlbumSort::ReleaseDateAsc),
            "release-date-desc" => Ok(AlbumSort::ReleaseDateDesc),
            "date-added-asc" | "date-added" => Ok(AlbumSort::DateAddedAsc),
            "date-added-desc" => Ok(AlbumSort::DateAddedDesc),
            _ => Err(()),
        }
    }
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct AlbumResponseParams {
    #[serde(rename = "isContextMenu")]
    is_context_menu: i32,

    item_id: String,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct AlbumResponseActionsGoParams {
    item_id: String,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct AlbumResponseActionsGo {
    params: AlbumResponseActionsGoParams,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct AlbumResponseActions {
    go: AlbumResponseActionsGo,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct GetAlbumResponse {
    pub result: GetAlbumResponseResult,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct GetAlbumResponseResultTitle {
    pub title: String,
    pub duration: String,
    pub disc: String,
    pub compilation: String,
    pub genre: String,
    pub artist_id: String,
    #[serde(rename = "tracknum")]
    pub track_num: String,
    pub url: String,
    #[serde(rename = "albumartist")]
    pub album_artist: String,
    #[serde(rename = "trackartist")]
    pub track_artist: String,
    #[serde(rename = "albumartist_ids")]
    pub album_artist_ids: String,
    #[serde(rename = "trackartist_ids")]
    pub track_artist_ids: String,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct GetAlbumResponseResult {
    pub count: i32,
    pub titles_loop: Vec<GetAlbumResponseResultTitle>,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct AlbumResponse {
    pub text: String,
    pub icon: Option<String>,
    pub params: Option<AlbumResponseParams>,
    pub actions: Option<AlbumResponseActions>,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct GetAlbumsResponseResult {
    pub item_loop: Vec<AlbumResponse>,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
#[serde(rename_all = "camelCase")]
pub struct GetAlbumsResponse {
    pub method: String,
    pub result: GetAlbumsResponseResult,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct LocalAlbumResponse {
    pub id: i32,
    pub artists: Option<String>,
    pub artist: String,
    pub album: String,
    pub artwork_track_id: Option<String>,
    pub extid: Option<String>,
    pub year: i16,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct GetLocalAlbumsResponseResult {
    pub albums_loop: Vec<LocalAlbumResponse>,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
#[serde(rename_all = "camelCase")]
pub struct GetLocalAlbumsResponse {
    pub method: String,
    pub result: GetLocalAlbumsResponseResult,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
#[serde(rename_all = "camelCase")]
pub struct ArtistsRequest {
    pub sources: Option<Vec<AlbumSource>>,
    pub sort: Option<ArtistSort>,
    pub filters: ArtistFilters,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
#[serde(rename_all = "camelCase")]
pub struct ArtistFilters {
    pub name: Option<String>,
    pub search: Option<String>,
}

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

pub fn filter_artists(artists: Vec<Artist>, request: &ArtistsRequest) -> Vec<Artist> {
    artists
        .into_iter()
        .filter(|artist| {
            !request
                .filters
                .name
                .as_ref()
                .is_some_and(|s| !artist.title.to_lowercase().contains(s))
        })
        .filter(|artist| {
            !request.filters.search.as_ref().is_some_and(|s| {
                !(artist.title.to_lowercase().contains(s)
                    || artist.title.to_lowercase().contains(s))
            })
        })
        .collect()
}

pub fn sort_artists(mut artists: Vec<Artist>, request: &ArtistsRequest) -> Vec<Artist> {
    match request.sort {
        Some(ArtistSort::NameAsc) => artists.sort_by(|a, b| a.title.cmp(&b.title)),
        Some(ArtistSort::NameDesc) => artists.sort_by(|a, b| b.title.cmp(&a.title)),
        _ => (),
    }
    match request.sort {
        Some(ArtistSort::NameAsc) => {
            artists.sort_by(|a, b| a.title.to_lowercase().cmp(&b.title.to_lowercase()))
        }
        Some(ArtistSort::NameDesc) => {
            artists.sort_by(|a, b| b.title.to_lowercase().cmp(&a.title.to_lowercase()))
        }
        None => (),
    }

    artists
}

#[derive(Debug, Error)]
pub enum GetArtistsError {
    #[error(transparent)]
    DbError(#[from] DbError),
}

pub async fn get_all_artists(
    data: &AppState,
    request: &ArtistsRequest,
) -> Result<Vec<Artist>, GetArtistsError> {
    #[allow(clippy::eq_op)]
    let artists = get_artists(&data.db.as_ref().unwrap().library.lock().unwrap())?;

    Ok(sort_artists(filter_artists(artists, request), request))
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
