use moosicbox_core::sqlite::{
    db::SqliteValue,
    models::{AsId, AsModel, ToApi},
};
use rusqlite::Row;
use serde::{Deserialize, Serialize};
use serde_json::Value;

#[derive(Debug, Serialize, Deserialize, Clone, PartialEq, Default)]
#[serde(rename_all = "camelCase")]
pub struct TidalConfig {
    pub id: u32,
    pub access_token: String,
    pub refresh_token: String,
    pub client_name: String,
    pub expires_in: u32,
    pub issued_at: u64,
    pub scope: String,
    pub token_type: String,
    pub user: String,
    pub user_id: u32,
    pub created: String,
    pub updated: String,
}

impl AsModel<TidalConfig> for Row<'_> {
    fn as_model(&self) -> TidalConfig {
        TidalConfig {
            id: self.get("id").unwrap(),
            access_token: self.get("access_token").unwrap(),
            refresh_token: self.get("refresh_token").unwrap(),
            client_name: self.get("client_name").unwrap(),
            expires_in: self.get("expires_in").unwrap(),
            issued_at: self.get("issued_at").unwrap(),
            scope: self.get("scope").unwrap(),
            token_type: self.get("token_type").unwrap(),
            user: self.get("user").unwrap(),
            user_id: self.get("user_id").unwrap(),
            created: self.get("created").unwrap(),
            updated: self.get("updated").unwrap(),
        }
    }
}

impl AsId for TidalConfig {
    fn as_id(&self) -> SqliteValue {
        SqliteValue::Number(self.id as i64)
    }
}

#[derive(Debug, Serialize, Deserialize, Clone, PartialEq, Default)]
#[serde(rename_all = "camelCase")]
pub struct TidalAlbum {
    pub id: u32,
    pub artist_id: u32,
    pub audio_quality: String,
    pub copyright: String,
    pub cover: String,
    pub duration: u32,
    pub explicit: bool,
    pub number_of_tracks: u32,
    pub popularity: u32,
    pub release_date: String,
    pub title: String,
    pub media_metadata_tags: Vec<String>,
}

impl AsModel<TidalAlbum> for Value {
    fn as_model(&self) -> TidalAlbum {
        TidalAlbum {
            id: self.get("id").unwrap().as_u64().unwrap() as u32,
            artist_id: self
                .get("artist")
                .unwrap()
                .get("id")
                .unwrap()
                .as_u64()
                .unwrap() as u32,
            audio_quality: self
                .get("audioQuality")
                .unwrap()
                .as_str()
                .unwrap()
                .to_string(),
            copyright: self.get("copyright").unwrap().as_str().unwrap().to_string(),
            cover: self.get("cover").unwrap().as_str().unwrap().to_string(),
            duration: self.get("duration").unwrap().as_u64().unwrap() as u32,
            explicit: self.get("explicit").unwrap().as_bool().unwrap(),
            number_of_tracks: self.get("numberOfTracks").unwrap().as_u64().unwrap() as u32,
            popularity: self.get("popularity").unwrap().as_u64().unwrap() as u32,
            release_date: self
                .get("releaseDate")
                .unwrap()
                .as_str()
                .unwrap()
                .to_string(),
            title: self.get("title").unwrap().as_str().unwrap().to_string(),
            media_metadata_tags: self
                .get("mediaMetadata")
                .unwrap()
                .get("tags")
                .unwrap()
                .as_array()
                .unwrap()
                .iter()
                .map(|v| v.as_str().unwrap().to_string())
                .collect::<Vec<_>>(),
        }
    }
}

impl ToApi<ApiTidalAlbum> for TidalAlbum {
    fn to_api(&self) -> ApiTidalAlbum {
        ApiTidalAlbum {
            id: self.id,
            artist_id: self.artist_id,
            audio_quality: self.audio_quality.clone(),
            copyright: self.copyright.clone(),
            cover: self.cover.clone(),
            duration: self.duration,
            explicit: self.explicit,
            number_of_tracks: self.number_of_tracks,
            popularity: self.popularity,
            release_date: self.release_date.clone(),
            title: self.title.clone(),
            media_metadata_tags: self.media_metadata_tags.clone(),
        }
    }
}

#[derive(Debug, Serialize, Deserialize, Clone, PartialEq, Default)]
#[serde(rename_all = "camelCase")]
pub struct ApiTidalAlbum {
    pub id: u32,
    pub artist_id: u32,
    pub audio_quality: String,
    pub copyright: String,
    pub cover: String,
    pub duration: u32,
    pub explicit: bool,
    pub number_of_tracks: u32,
    pub popularity: u32,
    pub release_date: String,
    pub title: String,
    pub media_metadata_tags: Vec<String>,
}
