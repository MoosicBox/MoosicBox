use std::fmt::Display;

use moosicbox_core::sqlite::models::{
    Album, Artist, AsId, AsModel, AsModelResult, Track, TrackApiSource,
};
use moosicbox_database::{DatabaseValue, Row};
use moosicbox_json_utils::{
    database::ToValue,
    serde_json::{ToNestedValue as _, ToValue as _},
    MissingValue, ParseError, ToValueType,
};
use serde::{Deserialize, Serialize};
use serde_json::Value;

#[derive(Debug, Serialize, Deserialize, Clone, PartialEq, Default)]
#[serde(rename_all = "camelCase")]
pub struct YtConfig {
    pub id: u32,
    pub client_id: String,
    pub access_token: String,
    pub refresh_token: String,
    pub client_name: String,
    pub expires_in: u32,
    pub issued_at: u64,
    pub scope: String,
    pub token_type: String,
    pub user: String,
    pub user_id: u64,
    pub created: String,
    pub updated: String,
}

impl MissingValue<YtConfig> for &moosicbox_database::Row {}
impl ToValueType<YtConfig> for &Row {
    fn to_value_type(self) -> Result<YtConfig, ParseError> {
        Ok(YtConfig {
            id: self.to_value("id")?,
            client_id: self.to_value("client_id")?,
            access_token: self.to_value("access_token")?,
            refresh_token: self.to_value("refresh_token")?,
            client_name: self.to_value("client_name")?,
            expires_in: self.to_value("expires_in")?,
            issued_at: self.to_value("issued_at")?,
            scope: self.to_value("scope")?,
            token_type: self.to_value("token_type")?,
            user: self.to_value("user")?,
            user_id: self.to_value("user_id")?,
            created: self.to_value("created")?,
            updated: self.to_value("updated")?,
        })
    }
}

impl AsModelResult<YtConfig, ParseError> for Row {
    fn as_model(&self) -> Result<YtConfig, ParseError> {
        self.to_value_type()
    }
}

impl AsModel<YtConfig> for Row {
    fn as_model(&self) -> YtConfig {
        AsModelResult::as_model(self).unwrap()
    }
}

impl AsId for YtConfig {
    fn as_id(&self) -> DatabaseValue {
        DatabaseValue::Number(self.id as i64)
    }
}

#[derive(Debug, Serialize, Deserialize, Clone, PartialEq, Default)]
#[serde(rename_all = "camelCase")]
pub struct YtArtist {
    pub id: String,
    pub picture: Option<String>,
    pub contains_cover: bool,
    pub popularity: u32,
    pub name: String,
}

impl From<YtArtist> for Artist {
    fn from(value: YtArtist) -> Self {
        Self {
            id: value.id.into(),
            title: value.name,
            cover: value.picture,
        }
    }
}

#[derive(Clone, Copy, Debug)]
pub enum YtArtistImageSize {
    Max,    // 750
    Large,  // 480
    Medium, // 320
    Small,  // 160
}

impl From<YtArtistImageSize> for u16 {
    fn from(value: YtArtistImageSize) -> Self {
        match value {
            YtArtistImageSize::Max => 750,
            YtArtistImageSize::Large => 480,
            YtArtistImageSize::Medium => 320,
            YtArtistImageSize::Small => 160,
        }
    }
}

impl From<u16> for YtArtistImageSize {
    fn from(value: u16) -> Self {
        match value {
            0..=160 => YtArtistImageSize::Small,
            161..=320 => YtArtistImageSize::Medium,
            321..=480 => YtArtistImageSize::Large,
            _ => YtArtistImageSize::Max,
        }
    }
}

impl Display for YtArtistImageSize {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_fmt(format_args!("{}", Into::<u16>::into(*self)))
    }
}

impl ToValueType<YtArtist> for &serde_json::Value {
    fn to_value_type(self) -> Result<YtArtist, ParseError> {
        self.as_model()
    }
}

impl AsModelResult<YtArtist, ParseError> for serde_json::Value {
    fn as_model(&self) -> Result<YtArtist, ParseError> {
        let picture: Option<String> = self.to_value("picture")?;

        Ok(YtArtist {
            id: self.to_value("id")?,
            contains_cover: picture.is_some(),
            picture,
            popularity: self.to_value("popularity")?,
            name: self.to_value("name")?,
        })
    }
}

#[derive(Debug, Serialize, Deserialize, Clone, PartialEq, Default)]
#[serde(rename_all = "camelCase")]
pub struct YtSearchArtist {
    pub id: u64,
    pub picture: Option<String>,
    pub contains_cover: bool,
    pub r#type: String,
    pub name: String,
}

impl YtSearchArtist {
    pub fn picture_url(&self, size: YtArtistImageSize) -> Option<String> {
        self.picture.as_ref().map(|picture| {
            let picture_path = picture.replace('-', "/");
            format!("https://resources.yt.com/images/{picture_path}/{size}x{size}.jpg")
        })
    }
}

impl ToValueType<YtSearchArtist> for &serde_json::Value {
    fn to_value_type(self) -> Result<YtSearchArtist, ParseError> {
        self.as_model()
    }
}

impl AsModelResult<YtSearchArtist, ParseError> for serde_json::Value {
    fn as_model(&self) -> Result<YtSearchArtist, ParseError> {
        let picture: Option<String> = self.to_value("picture")?;

        Ok(YtSearchArtist {
            id: self.to_value("id")?,
            contains_cover: picture.is_some(),
            picture,
            r#type: self.to_value("type")?,
            name: self.to_value("name")?,
        })
    }
}

#[derive(Debug, Serialize, Deserialize, Clone, PartialEq, Default)]
#[serde(rename_all = "camelCase")]
pub struct YtAlbum {
    pub id: String,
    pub artist: String,
    pub artist_id: String,
    pub contains_cover: bool,
    pub audio_quality: String,
    pub copyright: Option<String>,
    pub cover: Option<String>,
    pub duration: u32,
    pub explicit: bool,
    pub number_of_tracks: u32,
    pub popularity: u32,
    pub release_date: Option<String>,
    pub title: String,
    pub media_metadata_tags: Vec<String>,
}

impl From<YtAlbum> for Album {
    fn from(value: YtAlbum) -> Self {
        Self {
            id: value.id.into(),
            title: value.title,
            artist: value.artist,
            artist_id: value.artist_id.into(),
            date_released: value.release_date,
            date_added: None,
            artwork: value.cover,
            directory: None,
            blur: false,
            versions: vec![],
        }
    }
}

#[derive(Clone, Copy, Debug)]
pub enum YtAlbumImageSize {
    Max,       // 1280
    Large,     // 640
    Medium,    // 320
    Small,     // 160
    Thumbnail, // 80
}

impl From<YtAlbumImageSize> for u16 {
    fn from(value: YtAlbumImageSize) -> Self {
        match value {
            YtAlbumImageSize::Max => 1280,
            YtAlbumImageSize::Large => 640,
            YtAlbumImageSize::Medium => 320,
            YtAlbumImageSize::Small => 160,
            YtAlbumImageSize::Thumbnail => 80,
        }
    }
}

impl From<u16> for YtAlbumImageSize {
    fn from(value: u16) -> Self {
        match value {
            0..=80 => YtAlbumImageSize::Thumbnail,
            81..=160 => YtAlbumImageSize::Small,
            161..=320 => YtAlbumImageSize::Medium,
            321..=640 => YtAlbumImageSize::Large,
            _ => YtAlbumImageSize::Max,
        }
    }
}

impl Display for YtAlbumImageSize {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_fmt(format_args!("{}", Into::<u16>::into(*self)))
    }
}

#[derive(Debug, Serialize, Deserialize, Clone, PartialEq, Default)]
#[serde(rename_all = "camelCase")]
pub struct YtSearchAlbum {
    pub id: u64,
    pub artists: Vec<YtSearchArtist>,
    pub contains_cover: bool,
    pub audio_quality: String,
    pub copyright: Option<String>,
    pub cover: Option<String>,
    pub duration: u32,
    pub explicit: bool,
    pub number_of_tracks: u32,
    pub popularity: u32,
    pub release_date: Option<String>,
    pub title: String,
    pub media_metadata_tags: Vec<String>,
}

impl YtSearchAlbum {
    pub fn cover_url(&self, size: YtAlbumImageSize) -> Option<String> {
        self.cover.as_ref().map(|cover| {
            let cover_path = cover.replace('-', "/");
            format!("https://resources.yt.com/images/{cover_path}/{size}x{size}.jpg")
        })
    }
}

impl ToValueType<YtSearchAlbum> for &serde_json::Value {
    fn to_value_type(self) -> Result<YtSearchAlbum, ParseError> {
        self.as_model()
    }
}

impl AsModelResult<YtSearchAlbum, ParseError> for serde_json::Value {
    fn as_model(&self) -> Result<YtSearchAlbum, ParseError> {
        Ok(YtSearchAlbum {
            id: self.to_value("id")?,
            artists: self.to_value("artists")?,
            contains_cover: true,
            audio_quality: self.to_value("audioQuality")?,
            copyright: self.to_value("copyright")?,
            cover: self.to_value("cover")?,
            duration: self.to_value("duration")?,
            explicit: self.to_value("explicit")?,
            number_of_tracks: self.to_value("numberOfTracks")?,
            popularity: self.to_value("popularity")?,
            release_date: self.to_value("releaseDate")?,
            title: self.to_value("title")?,
            media_metadata_tags: self.to_nested_value(&["mediaMetadata", "tags"])?,
        })
    }
}

impl YtAlbum {
    pub fn cover_url(&self, size: YtAlbumImageSize) -> Option<String> {
        self.cover.as_ref().map(|cover| {
            let cover_path = cover.replace('-', "/");
            format!("https://resources.yt.com/images/{cover_path}/{size}x{size}.jpg")
        })
    }
}

impl ToValueType<YtAlbum> for &serde_json::Value {
    fn to_value_type(self) -> Result<YtAlbum, ParseError> {
        self.as_model()
    }
}

impl AsModelResult<YtAlbum, ParseError> for serde_json::Value {
    fn as_model(&self) -> Result<YtAlbum, ParseError> {
        Ok(YtAlbum {
            id: self.to_value("id")?,
            artist: self.to_nested_value(&["artist", "name"])?,
            artist_id: self.to_nested_value(&["artist", "id"])?,
            contains_cover: true,
            audio_quality: self.to_value("audioQuality")?,
            copyright: self.to_value("copyright")?,
            cover: self.to_value("cover")?,
            duration: self.to_value("duration")?,
            explicit: self.to_value("explicit")?,
            number_of_tracks: self.to_value("numberOfTracks")?,
            popularity: self.to_value("popularity")?,
            release_date: self.to_value("releaseDate")?,
            title: self.to_value("title")?,
            media_metadata_tags: self.to_nested_value(&["mediaMetadata", "tags"])?,
        })
    }
}

#[derive(Debug, Serialize, Deserialize, Clone, PartialEq, Default)]
#[serde(rename_all = "camelCase")]
pub struct YtTrack {
    pub id: String,
    pub track_number: u32,
    pub artist_id: String,
    pub artist: String,
    pub artist_cover: Option<String>,
    pub album_id: String,
    pub album: String,
    pub album_cover: Option<String>,
    pub audio_quality: String,
    pub copyright: Option<String>,
    pub duration: u32,
    pub explicit: bool,
    pub isrc: String,
    pub popularity: u32,
    pub title: String,
    pub media_metadata_tags: Vec<String>,
}

impl From<YtTrack> for Track {
    fn from(value: YtTrack) -> Self {
        Self {
            id: value.id.into(),
            number: value.track_number as i32,
            title: value.title,
            duration: value.duration as f64,
            album: value.album,
            album_id: value.album_id.into(),
            date_released: None,
            date_added: None,
            artist: value.artist,
            artist_id: value.artist_id.into(),
            file: None,
            artwork: value.album_cover,
            blur: false,
            bytes: 0,
            format: None,
            bit_depth: None,
            audio_bitrate: None,
            overall_bitrate: None,
            sample_rate: None,
            channels: None,
            source: TrackApiSource::Yt,
        }
    }
}

impl ToValueType<YtTrack> for &serde_json::Value {
    fn to_value_type(self) -> Result<YtTrack, ParseError> {
        self.as_model()
    }
}

impl AsModelResult<YtTrack, ParseError> for serde_json::Value {
    fn as_model(&self) -> Result<YtTrack, ParseError> {
        Ok(YtTrack {
            id: self.to_value("id")?,
            track_number: self.to_value("trackNumber")?,
            artist_id: self.to_nested_value(&["artist", "id"])?,
            artist: self.to_nested_value(&["artist", "name"])?,
            artist_cover: self.to_nested_value(&["artist", "picture"])?,
            album_id: self.to_nested_value(&["album", "id"])?,
            album: self.to_nested_value(&["album", "title"])?,
            album_cover: self.to_nested_value(&["album", "cover"])?,
            audio_quality: self.to_value("audioQuality")?,
            copyright: self.to_value("copyright")?,
            duration: self.to_value("duration")?,
            explicit: self.to_value("explicit")?,
            isrc: self.to_value("isrc")?,
            popularity: self.to_value("popularity")?,
            title: self.to_value("title")?,
            media_metadata_tags: self.to_nested_value(&["mediaMetadata", "tags"])?,
        })
    }
}

#[derive(Debug, Serialize, Deserialize, Clone, PartialEq, Default)]
#[serde(rename_all = "camelCase")]
pub struct YtVideo {
    pub id: String,
    pub artist_id: u64,
    pub artist: String,
    pub artist_cover: Option<String>,
    pub album_id: u64,
    pub album: String,
    pub album_cover: Option<String>,
    pub audio_quality: String,
    pub duration: u32,
    pub explicit: bool,
    pub title: String,
}

impl ToValueType<YtVideo> for &serde_json::Value {
    fn to_value_type(self) -> Result<YtVideo, ParseError> {
        self.as_model()
    }
}

impl AsModelResult<YtVideo, ParseError> for serde_json::Value {
    fn as_model(&self) -> Result<YtVideo, ParseError> {
        Ok(YtVideo {
            id: self.to_value("id")?,
            artist_id: self.to_nested_value(&["artist", "id"])?,
            artist: self.to_nested_value(&["artist", "name"])?,
            artist_cover: self.to_nested_value(&["artist", "picture"])?,
            album_id: self.to_nested_value(&["album", "id"])?,
            album: self.to_nested_value(&["album", "title"])?,
            album_cover: self.to_nested_value(&["album", "cover"])?,
            audio_quality: self.to_value("audioQuality")?,
            duration: self.to_value("duration")?,
            explicit: self.to_value("explicit")?,
            title: self.to_value("title")?,
        })
    }
}

#[derive(Debug, Serialize, Deserialize, Clone, PartialEq, Default)]
#[serde(rename_all = "camelCase")]
pub struct YtSearchTrack {
    pub id: u64,
    pub track_number: u32,
    pub artists: Vec<YtSearchArtist>,
    pub artist_cover: Option<String>,
    pub album_id: u64,
    pub album: String,
    pub album_cover: Option<String>,
    pub audio_quality: String,
    pub copyright: Option<String>,
    pub duration: u32,
    pub explicit: bool,
    pub isrc: String,
    pub popularity: u32,
    pub title: String,
    pub media_metadata_tags: Vec<String>,
}

impl ToValueType<YtSearchTrack> for &serde_json::Value {
    fn to_value_type(self) -> Result<YtSearchTrack, ParseError> {
        self.as_model()
    }
}

impl AsModelResult<YtSearchTrack, ParseError> for serde_json::Value {
    fn as_model(&self) -> Result<YtSearchTrack, ParseError> {
        Ok(YtSearchTrack {
            id: self.to_value("id")?,
            track_number: self.to_value("trackNumber")?,
            artists: self.to_value("artists")?,
            artist_cover: self.to_nested_value(&["artist", "picture"])?,
            album_id: self.to_nested_value(&["album", "id"])?,
            album: self.to_nested_value(&["album", "title"])?,
            album_cover: self.to_nested_value(&["album", "cover"])?,
            audio_quality: self.to_value("audioQuality")?,
            copyright: self.to_value("copyright")?,
            duration: self.to_value("duration")?,
            explicit: self.to_value("explicit")?,
            isrc: self.to_value("isrc")?,
            popularity: self.to_value("popularity")?,
            title: self.to_value("title")?,
            media_metadata_tags: self.to_nested_value(&["mediaMetadata", "tags"])?,
        })
    }
}

#[derive(Serialize, Deserialize)]
pub struct YtSearchResultList<T> {
    pub items: Vec<T>,
    pub offset: usize,
    pub limit: usize,
    pub total: usize,
}

impl<'a, T> ToValueType<YtSearchResultList<T>> for &'a Value
where
    Value: AsModelResult<YtSearchResultList<T>, ParseError>,
{
    fn to_value_type(self) -> Result<YtSearchResultList<T>, ParseError> {
        self.as_model()
    }
}

impl<T> AsModelResult<YtSearchResultList<T>, ParseError> for Value
where
    for<'a> &'a Value: ToValueType<T>,
    for<'a> &'a Value: ToValueType<usize>,
{
    fn as_model(&self) -> Result<YtSearchResultList<T>, ParseError> {
        Ok(YtSearchResultList {
            items: self.to_value("items")?,
            offset: self.to_value("offset")?,
            limit: self.to_value("limit")?,
            total: self.to_value("totalNumberOfItems")?,
        })
    }
}

#[derive(Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct YtSearchResultsContentsListItemRenderer {
    tracking_params: String,
    thumbnail: YtSearchResultsContentsListItemRendererThumbnail,
    flex_columns: Vec<YtSearchResultsContentsListItemRendererFlexColumns>,
    menu: YtSearchResultsContentsListItemRendererMenu,
    flex_column_display_style: String,
    navigation_endpoint: YtSearchResultsContentsSearchRendererRunNavigationEndpoint,
}

impl ToValueType<YtSearchResultsContentsListItemRenderer> for &Value {
    fn to_value_type(self) -> Result<YtSearchResultsContentsListItemRenderer, ParseError> {
        self.as_model()
    }
}

impl AsModelResult<YtSearchResultsContentsListItemRenderer, ParseError> for Value {
    fn as_model(&self) -> Result<YtSearchResultsContentsListItemRenderer, ParseError> {
        Ok(YtSearchResultsContentsListItemRenderer {
            tracking_params: self.to_value("trackingParams")?,
            thumbnail: self.to_value("thumbnail")?,
            flex_columns: self.to_value("flexColumns")?,
            menu: self.to_value("menu")?,
            flex_column_display_style: self.to_value("flexColumnDisplayStyle")?,
            navigation_endpoint: self.to_value("navigationEndpoint")?,
        })
    }
}

#[derive(Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct YtSearchResultsContentsListItemRendererFlexColumns {
    music_responsive_list_item_flex_column_renderer:
        YtSearchResultsContentsListItemRendererFlexColumnsRenderer,
}

impl ToValueType<YtSearchResultsContentsListItemRendererFlexColumns> for &Value {
    fn to_value_type(
        self,
    ) -> Result<YtSearchResultsContentsListItemRendererFlexColumns, ParseError> {
        self.as_model()
    }
}

impl AsModelResult<YtSearchResultsContentsListItemRendererFlexColumns, ParseError> for Value {
    fn as_model(&self) -> Result<YtSearchResultsContentsListItemRendererFlexColumns, ParseError> {
        Ok(YtSearchResultsContentsListItemRendererFlexColumns {
            music_responsive_list_item_flex_column_renderer: self
                .to_value("musicResponsiveListItemFlexColumnRenderer")?,
        })
    }
}

#[derive(Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct YtSearchResultsContentsListItemRendererFlexColumnsRenderer {
    text: YtSearchResultsContentsSearchRendererRuns,
    display_priority: String,
}

impl ToValueType<YtSearchResultsContentsListItemRendererFlexColumnsRenderer> for &Value {
    fn to_value_type(
        self,
    ) -> Result<YtSearchResultsContentsListItemRendererFlexColumnsRenderer, ParseError> {
        self.as_model()
    }
}

impl AsModelResult<YtSearchResultsContentsListItemRendererFlexColumnsRenderer, ParseError>
    for Value
{
    fn as_model(
        &self,
    ) -> Result<YtSearchResultsContentsListItemRendererFlexColumnsRenderer, ParseError> {
        Ok(YtSearchResultsContentsListItemRendererFlexColumnsRenderer {
            text: self.to_value("text")?,
            display_priority: self.to_value("displayPriority")?,
        })
    }
}

#[derive(Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct YtSearchResultsContentsListItemRendererMenu {
    menu_renderer: YtSearchResultsContentsListItemRendererMenuRenderer,
}

impl ToValueType<YtSearchResultsContentsListItemRendererMenu> for &Value {
    fn to_value_type(self) -> Result<YtSearchResultsContentsListItemRendererMenu, ParseError> {
        self.as_model()
    }
}

impl AsModelResult<YtSearchResultsContentsListItemRendererMenu, ParseError> for Value {
    fn as_model(&self) -> Result<YtSearchResultsContentsListItemRendererMenu, ParseError> {
        Ok(YtSearchResultsContentsListItemRendererMenu {
            menu_renderer: self.to_value("menuRenderer")?,
        })
    }
}

#[derive(Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct YtSearchResultsContentsListItemRendererMenuRenderer {
    items: Vec<YtSearchResultsContentsListItemRendererMenuRendererItem>,
}

impl ToValueType<YtSearchResultsContentsListItemRendererMenuRenderer> for &Value {
    fn to_value_type(
        self,
    ) -> Result<YtSearchResultsContentsListItemRendererMenuRenderer, ParseError> {
        self.as_model()
    }
}

impl AsModelResult<YtSearchResultsContentsListItemRendererMenuRenderer, ParseError> for Value {
    fn as_model(&self) -> Result<YtSearchResultsContentsListItemRendererMenuRenderer, ParseError> {
        Ok(YtSearchResultsContentsListItemRendererMenuRenderer {
            items: self.to_value("items")?,
        })
    }
}

#[derive(Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct YtSearchResultsContentsListItemRendererMenuRendererItem {
    #[serde(skip_serializing_if = "Option::is_none")]
    menu_navigation_item_renderer:
        Option<YtSearchResultsContentsListItemRendererMenuRendererItemNavigationItemRenderer>,
    #[serde(skip_serializing_if = "Option::is_none")]
    menu_service_item_renderer:
        Option<YtSearchResultsContentsListItemRendererMenuRendererItemServiceItemRenderer>,
}

impl ToValueType<YtSearchResultsContentsListItemRendererMenuRendererItem> for &Value {
    fn to_value_type(
        self,
    ) -> Result<YtSearchResultsContentsListItemRendererMenuRendererItem, ParseError> {
        self.as_model()
    }
}

impl AsModelResult<YtSearchResultsContentsListItemRendererMenuRendererItem, ParseError> for Value {
    fn as_model(
        &self,
    ) -> Result<YtSearchResultsContentsListItemRendererMenuRendererItem, ParseError> {
        Ok(YtSearchResultsContentsListItemRendererMenuRendererItem {
            menu_navigation_item_renderer: self.to_value("menuNavigationItemRenderer")?,
            menu_service_item_renderer: self.to_value("menuServiceItemRenderer")?,
        })
    }
}

#[derive(Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct YtSearchResultsContentsListItemRendererMenuRendererItemNavigationItemRenderer {
    text: YtSearchResultsContentsSearchRendererRuns,
    tracking_params: String,
    icon: YtSearchResultsContentsSearchRendererIcon,
    navigation_endpoint: YtSearchResultsContentsSearchRendererRunNavigationEndpoint,
}

impl ToValueType<YtSearchResultsContentsListItemRendererMenuRendererItemNavigationItemRenderer>
    for &Value
{
    fn to_value_type(
        self,
    ) -> Result<
        YtSearchResultsContentsListItemRendererMenuRendererItemNavigationItemRenderer,
        ParseError,
    > {
        self.as_model()
    }
}

impl
    AsModelResult<
        YtSearchResultsContentsListItemRendererMenuRendererItemNavigationItemRenderer,
        ParseError,
    > for Value
{
    fn as_model(
        &self,
    ) -> Result<
        YtSearchResultsContentsListItemRendererMenuRendererItemNavigationItemRenderer,
        ParseError,
    > {
        Ok(
            YtSearchResultsContentsListItemRendererMenuRendererItemNavigationItemRenderer {
                text: self.to_value("text")?,
                tracking_params: self.to_value("trackingParams")?,
                icon: self.to_value("icon")?,
                navigation_endpoint: self.to_value("navigationEndpoint")?,
            },
        )
    }
}

#[derive(Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct YtSearchResultsContentsListItemRendererMenuRendererItemServiceItemRenderer {
    text: YtSearchResultsContentsSearchRendererRuns,
    tracking_params: String,
    icon: YtSearchResultsContentsSearchRendererIcon,
    service_endpoint: YtSearchResultsContentsSearchRendererRunServiceEndpoint,
}

impl ToValueType<YtSearchResultsContentsListItemRendererMenuRendererItemServiceItemRenderer>
    for &Value
{
    fn to_value_type(
        self,
    ) -> Result<
        YtSearchResultsContentsListItemRendererMenuRendererItemServiceItemRenderer,
        ParseError,
    > {
        self.as_model()
    }
}

impl
    AsModelResult<
        YtSearchResultsContentsListItemRendererMenuRendererItemServiceItemRenderer,
        ParseError,
    > for Value
{
    fn as_model(
        &self,
    ) -> Result<
        YtSearchResultsContentsListItemRendererMenuRendererItemServiceItemRenderer,
        ParseError,
    > {
        Ok(
            YtSearchResultsContentsListItemRendererMenuRendererItemServiceItemRenderer {
                text: self.to_value("text")?,
                tracking_params: self.to_value("trackingParams")?,
                icon: self.to_value("icon")?,
                service_endpoint: self.to_value("serviceEndpoint")?,
            },
        )
    }
}

#[derive(Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct YtSearchResultsContentsListItemRendererThumbnail {
    music_thumbnail_renderer: YtSearchResultsContentsListItemRendererThumbnailRenderer,
}

impl ToValueType<YtSearchResultsContentsListItemRendererThumbnail> for &Value {
    fn to_value_type(self) -> Result<YtSearchResultsContentsListItemRendererThumbnail, ParseError> {
        self.as_model()
    }
}

impl AsModelResult<YtSearchResultsContentsListItemRendererThumbnail, ParseError> for Value {
    fn as_model(&self) -> Result<YtSearchResultsContentsListItemRendererThumbnail, ParseError> {
        Ok(YtSearchResultsContentsListItemRendererThumbnail {
            music_thumbnail_renderer: self.to_value("musicThumbnailRenderer")?,
        })
    }
}

#[derive(Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct YtSearchResultsContentsListItemRendererThumbnailRenderer {
    thumbnail: YtSearchResultsContentsListItemRendererThumbnailRendererThumbnail,
    thumbnail_crop: String,
    thumbnail_scale: String,
    tracking_params: String,
}

impl ToValueType<YtSearchResultsContentsListItemRendererThumbnailRenderer> for &Value {
    fn to_value_type(
        self,
    ) -> Result<YtSearchResultsContentsListItemRendererThumbnailRenderer, ParseError> {
        self.as_model()
    }
}

impl AsModelResult<YtSearchResultsContentsListItemRendererThumbnailRenderer, ParseError> for Value {
    fn as_model(
        &self,
    ) -> Result<YtSearchResultsContentsListItemRendererThumbnailRenderer, ParseError> {
        Ok(YtSearchResultsContentsListItemRendererThumbnailRenderer {
            thumbnail: self.to_value("thumbnail")?,
            thumbnail_crop: self.to_value("thumbnailCrop")?,
            thumbnail_scale: self.to_value("thumbnailScale")?,
            tracking_params: self.to_value("trackingParams")?,
        })
    }
}

#[derive(Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct YtSearchResultsContentsListItemRendererThumbnailRendererThumbnail {
    thumbnails: Vec<YtSearchResultsContentsListItemRendererThumbnailRendererThumbnailData>,
}

impl ToValueType<YtSearchResultsContentsListItemRendererThumbnailRendererThumbnail> for &Value {
    fn to_value_type(
        self,
    ) -> Result<YtSearchResultsContentsListItemRendererThumbnailRendererThumbnail, ParseError> {
        self.as_model()
    }
}

impl AsModelResult<YtSearchResultsContentsListItemRendererThumbnailRendererThumbnail, ParseError>
    for Value
{
    fn as_model(
        &self,
    ) -> Result<YtSearchResultsContentsListItemRendererThumbnailRendererThumbnail, ParseError> {
        Ok(
            YtSearchResultsContentsListItemRendererThumbnailRendererThumbnail {
                thumbnails: self.to_value("thumbnails")?,
            },
        )
    }
}

#[derive(Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct YtSearchResultsContentsListItemRendererThumbnailRendererThumbnailData {
    url: String,
    width: u16,
    height: u16,
}

impl ToValueType<YtSearchResultsContentsListItemRendererThumbnailRendererThumbnailData> for &Value {
    fn to_value_type(
        self,
    ) -> Result<YtSearchResultsContentsListItemRendererThumbnailRendererThumbnailData, ParseError>
    {
        self.as_model()
    }
}

impl
    AsModelResult<YtSearchResultsContentsListItemRendererThumbnailRendererThumbnailData, ParseError>
    for Value
{
    fn as_model(
        &self,
    ) -> Result<YtSearchResultsContentsListItemRendererThumbnailRendererThumbnailData, ParseError>
    {
        Ok(
            YtSearchResultsContentsListItemRendererThumbnailRendererThumbnailData {
                url: self.to_value("url")?,
                width: self.to_value("width")?,
                height: self.to_value("height")?,
            },
        )
    }
}

#[derive(Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct YtSearchResultsContentsSearchSuggestionRenderer {
    suggestion: YtSearchResultsContentsSearchRendererRuns,
}

impl ToValueType<YtSearchResultsContentsSearchSuggestionRenderer> for &Value {
    fn to_value_type(self) -> Result<YtSearchResultsContentsSearchSuggestionRenderer, ParseError> {
        self.as_model()
    }
}

impl AsModelResult<YtSearchResultsContentsSearchSuggestionRenderer, ParseError> for Value {
    fn as_model(&self) -> Result<YtSearchResultsContentsSearchSuggestionRenderer, ParseError> {
        Ok(YtSearchResultsContentsSearchSuggestionRenderer {
            suggestion: self.to_value("suggestion")?,
        })
    }
}

#[derive(Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct YtSearchResultsContentsSearchRendererRuns {
    #[serde(skip_serializing_if = "Option::is_none")]
    runs: Option<Vec<YtSearchResultsContentsSearchRendererRun>>,
}

impl ToValueType<YtSearchResultsContentsSearchRendererRuns> for &Value {
    fn to_value_type(self) -> Result<YtSearchResultsContentsSearchRendererRuns, ParseError> {
        self.as_model()
    }
}

impl AsModelResult<YtSearchResultsContentsSearchRendererRuns, ParseError> for Value {
    fn as_model(&self) -> Result<YtSearchResultsContentsSearchRendererRuns, ParseError> {
        Ok(YtSearchResultsContentsSearchRendererRuns {
            runs: self.to_value("runs")?,
        })
    }
}

#[derive(Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct YtSearchResultsContentsSearchRendererRun {
    text: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    bold: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    navigation_endpoint: Option<YtSearchResultsContentsSearchRendererRunNavigationEndpoint>,
}

impl ToValueType<YtSearchResultsContentsSearchRendererRun> for &Value {
    fn to_value_type(self) -> Result<YtSearchResultsContentsSearchRendererRun, ParseError> {
        self.as_model()
    }
}

impl AsModelResult<YtSearchResultsContentsSearchRendererRun, ParseError> for Value {
    fn as_model(&self) -> Result<YtSearchResultsContentsSearchRendererRun, ParseError> {
        Ok(YtSearchResultsContentsSearchRendererRun {
            text: self.to_value("text")?,
            bold: self.to_value("bold")?,
            navigation_endpoint: self.to_value("navigationEndpoint")?,
        })
    }
}

#[derive(Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct YtSearchResultsContentsSearchRendererIcon {
    icon_type: String,
}

impl ToValueType<YtSearchResultsContentsSearchRendererIcon> for &Value {
    fn to_value_type(self) -> Result<YtSearchResultsContentsSearchRendererIcon, ParseError> {
        self.as_model()
    }
}

impl AsModelResult<YtSearchResultsContentsSearchRendererIcon, ParseError> for Value {
    fn as_model(&self) -> Result<YtSearchResultsContentsSearchRendererIcon, ParseError> {
        Ok(YtSearchResultsContentsSearchRendererIcon {
            icon_type: self.to_value("iconType")?,
        })
    }
}

#[derive(Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct YtSearchResultsContentsSearchRendererRunNavigationEndpoint {
    click_tracking_params: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    browse_endpoint:
        Option<YtSearchResultsContentsSearchRendererRunNavigationEndpointBrowseEndpoint>,
    #[serde(skip_serializing_if = "Option::is_none")]
    watch_endpoint: Option<YtSearchResultsContentsSearchRendererRunNavigationEndpointWatchEndpoint>,
    #[serde(skip_serializing_if = "Option::is_none")]
    watch_playlist_endpoint:
        Option<YtSearchResultsContentsSearchRendererRunNavigationEndpointWatchPlaylistEndpoint>,
}

impl ToValueType<YtSearchResultsContentsSearchRendererRunNavigationEndpoint> for &Value {
    fn to_value_type(
        self,
    ) -> Result<YtSearchResultsContentsSearchRendererRunNavigationEndpoint, ParseError> {
        self.as_model()
    }
}

impl AsModelResult<YtSearchResultsContentsSearchRendererRunNavigationEndpoint, ParseError>
    for Value
{
    fn as_model(
        &self,
    ) -> Result<YtSearchResultsContentsSearchRendererRunNavigationEndpoint, ParseError> {
        Ok(YtSearchResultsContentsSearchRendererRunNavigationEndpoint {
            click_tracking_params: self.to_value("clickTrackingParams")?,
            browse_endpoint: self.to_value("browseEndpoint")?,
            watch_endpoint: self.to_value("watchEndpoint")?,
            watch_playlist_endpoint: self.to_value("watchPlaylistEndpoint")?,
        })
    }
}

#[derive(Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct YtSearchResultsContentsSearchRendererRunServiceEndpoint {
    click_tracking_params: String,
    queue_add_endpoint: YtSearchResultsContentsSearchRendererRunServiceEndpointQueueAddEndpoint,
}

impl ToValueType<YtSearchResultsContentsSearchRendererRunServiceEndpoint> for &Value {
    fn to_value_type(
        self,
    ) -> Result<YtSearchResultsContentsSearchRendererRunServiceEndpoint, ParseError> {
        self.as_model()
    }
}

impl AsModelResult<YtSearchResultsContentsSearchRendererRunServiceEndpoint, ParseError> for Value {
    fn as_model(
        &self,
    ) -> Result<YtSearchResultsContentsSearchRendererRunServiceEndpoint, ParseError> {
        Ok(YtSearchResultsContentsSearchRendererRunServiceEndpoint {
            click_tracking_params: self.to_value("clickTrackingParams")?,
            queue_add_endpoint: self.to_value("queueAddEndpoint")?,
        })
    }
}

#[derive(Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct YtSearchResultsContentsSearchRendererRunServiceEndpointQueueAddEndpoint {
    queue_target: YtSearchResultsContentsSearchRendererRunServiceEndpointQueueTarget,
    queue_insert_position: String,
    commands: Vec<YtSearchResultsContentsSearchRendererRunServiceEndpointQueueCommand>,
}

impl ToValueType<YtSearchResultsContentsSearchRendererRunServiceEndpointQueueAddEndpoint>
    for &Value
{
    fn to_value_type(
        self,
    ) -> Result<YtSearchResultsContentsSearchRendererRunServiceEndpointQueueAddEndpoint, ParseError>
    {
        self.as_model()
    }
}

impl
    AsModelResult<
        YtSearchResultsContentsSearchRendererRunServiceEndpointQueueAddEndpoint,
        ParseError,
    > for Value
{
    fn as_model(
        &self,
    ) -> Result<YtSearchResultsContentsSearchRendererRunServiceEndpointQueueAddEndpoint, ParseError>
    {
        Ok(
            YtSearchResultsContentsSearchRendererRunServiceEndpointQueueAddEndpoint {
                queue_target: self.to_value("queueTarget")?,
                queue_insert_position: self.to_value("queueInsertPosition")?,
                commands: self.to_value("commands")?,
            },
        )
    }
}

#[derive(Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct YtSearchResultsContentsSearchRendererRunServiceEndpointQueueTarget {
    #[serde(skip_serializing_if = "Option::is_none")]
    playlist_id: Option<String>,
    on_empty_queue: YtSearchResultsContentsSearchRendererRunNavigationEndpointWatchEndpoint,
}

impl ToValueType<YtSearchResultsContentsSearchRendererRunServiceEndpointQueueTarget> for &Value {
    fn to_value_type(
        self,
    ) -> Result<YtSearchResultsContentsSearchRendererRunServiceEndpointQueueTarget, ParseError>
    {
        self.as_model()
    }
}

impl AsModelResult<YtSearchResultsContentsSearchRendererRunServiceEndpointQueueTarget, ParseError>
    for Value
{
    fn as_model(
        &self,
    ) -> Result<YtSearchResultsContentsSearchRendererRunServiceEndpointQueueTarget, ParseError>
    {
        Ok(
            YtSearchResultsContentsSearchRendererRunServiceEndpointQueueTarget {
                playlist_id: self.to_value("playlistId")?,
                on_empty_queue: self.to_value("onEmptyQueue")?,
            },
        )
    }
}

#[derive(Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct YtSearchResultsContentsSearchRendererRunServiceEndpointQueueCommand {
    click_tracking_params: String,
    add_to_toast_action:
        YtSearchResultsContentsSearchRendererRunServiceEndpointQueueAddToToastAction,
}

impl ToValueType<YtSearchResultsContentsSearchRendererRunServiceEndpointQueueCommand> for &Value {
    fn to_value_type(
        self,
    ) -> Result<YtSearchResultsContentsSearchRendererRunServiceEndpointQueueCommand, ParseError>
    {
        self.as_model()
    }
}

impl AsModelResult<YtSearchResultsContentsSearchRendererRunServiceEndpointQueueCommand, ParseError>
    for Value
{
    fn as_model(
        &self,
    ) -> Result<YtSearchResultsContentsSearchRendererRunServiceEndpointQueueCommand, ParseError>
    {
        Ok(
            YtSearchResultsContentsSearchRendererRunServiceEndpointQueueCommand {
                click_tracking_params: self.to_value("clickTrackingParams")?,
                add_to_toast_action: self.to_value("addToToastAction")?,
            },
        )
    }
}

#[derive(Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct YtSearchResultsContentsSearchRendererRunServiceEndpointQueueAddToToastAction {
    item: YtSearchResultsContentsSearchRendererRunServiceEndpointQueueAddToToastActionItem,
}

impl ToValueType<YtSearchResultsContentsSearchRendererRunServiceEndpointQueueAddToToastAction>
    for &Value
{
    fn to_value_type(
        self,
    ) -> Result<
        YtSearchResultsContentsSearchRendererRunServiceEndpointQueueAddToToastAction,
        ParseError,
    > {
        self.as_model()
    }
}

impl
    AsModelResult<
        YtSearchResultsContentsSearchRendererRunServiceEndpointQueueAddToToastAction,
        ParseError,
    > for Value
{
    fn as_model(
        &self,
    ) -> Result<
        YtSearchResultsContentsSearchRendererRunServiceEndpointQueueAddToToastAction,
        ParseError,
    > {
        Ok(
            YtSearchResultsContentsSearchRendererRunServiceEndpointQueueAddToToastAction {
                item: self.to_value("item")?,
            },
        )
    }
}

#[derive(Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct YtSearchResultsContentsSearchRendererRunServiceEndpointQueueAddToToastActionItem {
    notification_text_renderer: YtSearchResultsContentsSearchNotificationTextRenderer,
}

impl ToValueType<YtSearchResultsContentsSearchRendererRunServiceEndpointQueueAddToToastActionItem>
    for &Value
{
    fn to_value_type(
        self,
    ) -> Result<
        YtSearchResultsContentsSearchRendererRunServiceEndpointQueueAddToToastActionItem,
        ParseError,
    > {
        self.as_model()
    }
}

impl
    AsModelResult<
        YtSearchResultsContentsSearchRendererRunServiceEndpointQueueAddToToastActionItem,
        ParseError,
    > for Value
{
    fn as_model(
        &self,
    ) -> Result<
        YtSearchResultsContentsSearchRendererRunServiceEndpointQueueAddToToastActionItem,
        ParseError,
    > {
        Ok(
            YtSearchResultsContentsSearchRendererRunServiceEndpointQueueAddToToastActionItem {
                notification_text_renderer: self.to_value("notificationTextRenderer")?,
            },
        )
    }
}

#[derive(Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct YtSearchResultsContentsSearchNotificationTextRenderer {
    success_response_text: YtSearchResultsContentsSearchRendererRuns,
    tracking_params: String,
}

impl ToValueType<YtSearchResultsContentsSearchNotificationTextRenderer> for &Value {
    fn to_value_type(
        self,
    ) -> Result<YtSearchResultsContentsSearchNotificationTextRenderer, ParseError> {
        self.as_model()
    }
}

impl AsModelResult<YtSearchResultsContentsSearchNotificationTextRenderer, ParseError> for Value {
    fn as_model(
        &self,
    ) -> Result<YtSearchResultsContentsSearchNotificationTextRenderer, ParseError> {
        Ok(YtSearchResultsContentsSearchNotificationTextRenderer {
            success_response_text: self.to_value("successResponseText")?,
            tracking_params: self.to_value("trackingParams")?,
        })
    }
}

#[derive(Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct YtSearchResultsContentsSearchRendererRunNavigationEndpointBrowseEndpoint {
    browse_id: String,
    browse_endpoint_context_supported_configs:
        YtSearchResultsContentsSearchRendererRunNavigationEndpointBrowseEndpointConfigs,
}

impl ToValueType<YtSearchResultsContentsSearchRendererRunNavigationEndpointBrowseEndpoint>
    for &Value
{
    fn to_value_type(
        self,
    ) -> Result<YtSearchResultsContentsSearchRendererRunNavigationEndpointBrowseEndpoint, ParseError>
    {
        self.as_model()
    }
}

impl
    AsModelResult<
        YtSearchResultsContentsSearchRendererRunNavigationEndpointBrowseEndpoint,
        ParseError,
    > for Value
{
    fn as_model(
        &self,
    ) -> Result<YtSearchResultsContentsSearchRendererRunNavigationEndpointBrowseEndpoint, ParseError>
    {
        Ok(
            YtSearchResultsContentsSearchRendererRunNavigationEndpointBrowseEndpoint {
                browse_id: self.to_value("browseId")?,
                browse_endpoint_context_supported_configs: self
                    .to_value("browseEndpointContextSupportedConfigs")?,
            },
        )
    }
}

#[derive(Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct YtSearchResultsContentsSearchRendererRunNavigationEndpointWatchEndpoint {
    #[serde(skip_serializing_if = "Option::is_none")]
    video_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    playlist_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    params: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    watch_endpoint_music_supported_configs:
        Option<YtSearchResultsContentsSearchRendererRunNavigationEndpointWatchEndpointConfigs>,
}

impl ToValueType<YtSearchResultsContentsSearchRendererRunNavigationEndpointWatchEndpoint>
    for &Value
{
    fn to_value_type(
        self,
    ) -> Result<YtSearchResultsContentsSearchRendererRunNavigationEndpointWatchEndpoint, ParseError>
    {
        self.as_model()
    }
}

impl
    AsModelResult<
        YtSearchResultsContentsSearchRendererRunNavigationEndpointWatchEndpoint,
        ParseError,
    > for Value
{
    fn as_model(
        &self,
    ) -> Result<YtSearchResultsContentsSearchRendererRunNavigationEndpointWatchEndpoint, ParseError>
    {
        Ok(
            YtSearchResultsContentsSearchRendererRunNavigationEndpointWatchEndpoint {
                video_id: self.to_value("videoId")?,
                playlist_id: self.to_value("playlistId")?,
                params: self.to_value("params")?,
                watch_endpoint_music_supported_configs: self
                    .to_value("watchEndpointMusicSupportedConfigs")?,
            },
        )
    }
}

#[derive(Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct YtSearchResultsContentsSearchRendererRunNavigationEndpointWatchPlaylistEndpoint {
    playlist_id: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    params: Option<String>,
}

impl ToValueType<YtSearchResultsContentsSearchRendererRunNavigationEndpointWatchPlaylistEndpoint>
    for &Value
{
    fn to_value_type(
        self,
    ) -> Result<
        YtSearchResultsContentsSearchRendererRunNavigationEndpointWatchPlaylistEndpoint,
        ParseError,
    > {
        self.as_model()
    }
}

impl
    AsModelResult<
        YtSearchResultsContentsSearchRendererRunNavigationEndpointWatchPlaylistEndpoint,
        ParseError,
    > for Value
{
    fn as_model(
        &self,
    ) -> Result<
        YtSearchResultsContentsSearchRendererRunNavigationEndpointWatchPlaylistEndpoint,
        ParseError,
    > {
        Ok(
            YtSearchResultsContentsSearchRendererRunNavigationEndpointWatchPlaylistEndpoint {
                playlist_id: self.to_value("playlistId")?,
                params: self.to_value("params")?,
            },
        )
    }
}

#[derive(Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct YtSearchResultsContentsSearchRendererRunNavigationEndpointWatchEndpointConfigs {
    watch_endpoint_music_config:
        YtSearchResultsContentsSearchRendererRunNavigationEndpointWatchEndpointConfig,
}

impl ToValueType<YtSearchResultsContentsSearchRendererRunNavigationEndpointWatchEndpointConfigs>
    for &Value
{
    fn to_value_type(
        self,
    ) -> Result<
        YtSearchResultsContentsSearchRendererRunNavigationEndpointWatchEndpointConfigs,
        ParseError,
    > {
        self.as_model()
    }
}

impl
    AsModelResult<
        YtSearchResultsContentsSearchRendererRunNavigationEndpointWatchEndpointConfigs,
        ParseError,
    > for Value
{
    fn as_model(
        &self,
    ) -> Result<
        YtSearchResultsContentsSearchRendererRunNavigationEndpointWatchEndpointConfigs,
        ParseError,
    > {
        Ok(
            YtSearchResultsContentsSearchRendererRunNavigationEndpointWatchEndpointConfigs {
                watch_endpoint_music_config: self.to_value("watchEndpointMusicConfig")?,
            },
        )
    }
}

#[derive(Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct YtSearchResultsContentsSearchRendererRunNavigationEndpointWatchEndpointConfig {
    music_video_type: String,
}

impl ToValueType<YtSearchResultsContentsSearchRendererRunNavigationEndpointWatchEndpointConfig>
    for &Value
{
    fn to_value_type(
        self,
    ) -> Result<
        YtSearchResultsContentsSearchRendererRunNavigationEndpointWatchEndpointConfig,
        ParseError,
    > {
        self.as_model()
    }
}

impl
    AsModelResult<
        YtSearchResultsContentsSearchRendererRunNavigationEndpointWatchEndpointConfig,
        ParseError,
    > for Value
{
    fn as_model(
        &self,
    ) -> Result<
        YtSearchResultsContentsSearchRendererRunNavigationEndpointWatchEndpointConfig,
        ParseError,
    > {
        Ok(
            YtSearchResultsContentsSearchRendererRunNavigationEndpointWatchEndpointConfig {
                music_video_type: self.to_value("musicVideoType")?,
            },
        )
    }
}

#[derive(Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct YtSearchResultsContentsSearchRendererRunNavigationEndpointBrowseEndpointConfigs {
    browse_endpoint_context_music_config:
        YtSearchResultsContentsSearchRendererRunNavigationEndpointBrowseEndpointConfig,
}

impl ToValueType<YtSearchResultsContentsSearchRendererRunNavigationEndpointBrowseEndpointConfigs>
    for &Value
{
    fn to_value_type(
        self,
    ) -> Result<
        YtSearchResultsContentsSearchRendererRunNavigationEndpointBrowseEndpointConfigs,
        ParseError,
    > {
        self.as_model()
    }
}

impl
    AsModelResult<
        YtSearchResultsContentsSearchRendererRunNavigationEndpointBrowseEndpointConfigs,
        ParseError,
    > for Value
{
    fn as_model(
        &self,
    ) -> Result<
        YtSearchResultsContentsSearchRendererRunNavigationEndpointBrowseEndpointConfigs,
        ParseError,
    > {
        Ok(
            YtSearchResultsContentsSearchRendererRunNavigationEndpointBrowseEndpointConfigs {
                browse_endpoint_context_music_config: self
                    .to_value("browseEndpointContextMusicConfig")?,
            },
        )
    }
}

#[derive(Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct YtSearchResultsContentsSearchRendererRunNavigationEndpointBrowseEndpointConfig {
    page_type: String,
}

impl ToValueType<YtSearchResultsContentsSearchRendererRunNavigationEndpointBrowseEndpointConfig>
    for &Value
{
    fn to_value_type(
        self,
    ) -> Result<
        YtSearchResultsContentsSearchRendererRunNavigationEndpointBrowseEndpointConfig,
        ParseError,
    > {
        self.as_model()
    }
}

impl
    AsModelResult<
        YtSearchResultsContentsSearchRendererRunNavigationEndpointBrowseEndpointConfig,
        ParseError,
    > for Value
{
    fn as_model(
        &self,
    ) -> Result<
        YtSearchResultsContentsSearchRendererRunNavigationEndpointBrowseEndpointConfig,
        ParseError,
    > {
        Ok(
            YtSearchResultsContentsSearchRendererRunNavigationEndpointBrowseEndpointConfig {
                page_type: self.to_value("pageType")?,
            },
        )
    }
}

#[derive(Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct YtSearchResultsContentsSectionRenderer {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub music_responsive_list_item_renderer: Option<YtSearchResultsContentsListItemRenderer>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub search_suggestion_renderer: Option<YtSearchResultsContentsSearchSuggestionRenderer>,
}

impl ToValueType<YtSearchResultsContentsSectionRenderer> for &Value {
    fn to_value_type(self) -> Result<YtSearchResultsContentsSectionRenderer, ParseError> {
        self.as_model()
    }
}

impl AsModelResult<YtSearchResultsContentsSectionRenderer, ParseError> for Value {
    fn as_model(&self) -> Result<YtSearchResultsContentsSectionRenderer, ParseError> {
        Ok(YtSearchResultsContentsSectionRenderer {
            music_responsive_list_item_renderer: self
                .to_value("musicResponsiveListItemRenderer")?,
            search_suggestion_renderer: self.to_value("searchSuggestionRenderer")?,
        })
    }
}

#[derive(Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct YtSearchResultsContentsSection {
    pub contents: Vec<YtSearchResultsContentsSectionRenderer>,
}

impl ToValueType<YtSearchResultsContentsSection> for &Value {
    fn to_value_type(self) -> Result<YtSearchResultsContentsSection, ParseError> {
        self.as_model()
    }
}

impl AsModelResult<YtSearchResultsContentsSection, ParseError> for Value {
    fn as_model(&self) -> Result<YtSearchResultsContentsSection, ParseError> {
        Ok(YtSearchResultsContentsSection {
            contents: self.to_value("contents")?,
        })
    }
}

#[derive(Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct YtSearchResultsContents {
    pub search_suggestions_section_renderer: YtSearchResultsContentsSection,
}

impl ToValueType<YtSearchResultsContents> for &Value {
    fn to_value_type(self) -> Result<YtSearchResultsContents, ParseError> {
        self.as_model()
    }
}

impl AsModelResult<YtSearchResultsContents, ParseError> for Value {
    fn as_model(&self) -> Result<YtSearchResultsContents, ParseError> {
        Ok(YtSearchResultsContents {
            search_suggestions_section_renderer: self
                .to_value("searchSuggestionsSectionRenderer")?,
        })
    }
}

#[derive(Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct YtSearchResults {
    pub contents: Vec<YtSearchResultsContents>,
}

impl ToValueType<YtSearchResults> for &Value {
    fn to_value_type(self) -> Result<YtSearchResults, ParseError> {
        self.as_model()
    }
}

impl AsModelResult<YtSearchResults, ParseError> for Value {
    fn as_model(&self) -> Result<YtSearchResults, ParseError> {
        Ok(YtSearchResults {
            contents: self.to_value("contents")?,
        })
    }
}

fn track_from_search_result(value: &YtSearchResultsContentsListItemRenderer) -> Option<YtTrack> {
    value
        .navigation_endpoint
        .watch_endpoint
        .as_ref()
        .and_then(|endpoint| {
            endpoint
                .watch_endpoint_music_supported_configs
                .as_ref()
                .and_then(|configs| {
                    if configs.watch_endpoint_music_config.music_video_type
                        == "MUSIC_VIDEO_TYPE_ATV"
                    {
                        let album = track_album_from_search_result(value);
                        Some(YtTrack {
                            id: endpoint.video_id.as_deref().unwrap_or("N/A").to_string(),
                            artist: album
                                .as_ref()
                                .map(|x| x.artist.as_str())
                                .unwrap_or("N/A")
                                .to_string(),
                            artist_id: album
                                .as_ref()
                                .map(|x| x.artist_id.as_str())
                                .unwrap_or("N/A")
                                .to_string(),
                            album: album
                                .as_ref()
                                .map(|x| x.title.as_str())
                                .unwrap_or("N/A")
                                .to_string(),
                            album_id: album
                                .as_ref()
                                .map(|x| x.id.as_str())
                                .unwrap_or("N/A")
                                .to_string(),
                            album_cover: value
                                .thumbnail
                                .music_thumbnail_renderer
                                .thumbnail
                                .thumbnails
                                .iter()
                                .max_by(|a, b| a.width.cmp(&b.width))
                                .map(|x| x.url.clone()),
                            title: value
                                .flex_columns
                                .first()
                                .and_then(|x| {
                                    x.music_responsive_list_item_flex_column_renderer
                                        .text
                                        .runs
                                        .as_ref()
                                        .and_then(|x| x.first().map(|x| x.text.as_str()))
                                })
                                .unwrap_or("N/A")
                                .to_string(),
                            ..Default::default()
                        })
                    } else {
                        None
                    }
                })
        })
}

fn video_from_search_result(value: &YtSearchResultsContentsListItemRenderer) -> Option<YtVideo> {
    value
        .navigation_endpoint
        .watch_endpoint
        .as_ref()
        .and_then(|endpoint| {
            endpoint
                .watch_endpoint_music_supported_configs
                .as_ref()
                .and_then(|configs| {
                    if configs.watch_endpoint_music_config.music_video_type
                        == "MUSIC_VIDEO_TYPE_UGC"
                    {
                        Some(YtVideo {
                            id: endpoint.video_id.as_deref().unwrap_or("N/A").to_string(),
                            album_cover: value
                                .thumbnail
                                .music_thumbnail_renderer
                                .thumbnail
                                .thumbnails
                                .iter()
                                .max_by(|a, b| a.width.cmp(&b.width))
                                .map(|x| x.url.clone()),
                            title: value
                                .flex_columns
                                .first()
                                .and_then(|x| {
                                    x.music_responsive_list_item_flex_column_renderer
                                        .text
                                        .runs
                                        .as_ref()
                                        .and_then(|x| x.first().map(|x| x.text.as_str()))
                                })
                                .unwrap_or("N/A")
                                .to_string(),
                            ..Default::default()
                        })
                    } else {
                        None
                    }
                })
        })
}

fn artist_from_search_result(value: &YtSearchResultsContentsListItemRenderer) -> Option<YtArtist> {
    value
        .navigation_endpoint
        .browse_endpoint
        .as_ref()
        .and_then(|endpoint| {
            if endpoint
                .browse_endpoint_context_supported_configs
                .browse_endpoint_context_music_config
                .page_type
                == "MUSIC_PAGE_TYPE_ARTIST"
            {
                Some(YtArtist {
                    id: endpoint.browse_id.clone(),
                    picture: value
                        .thumbnail
                        .music_thumbnail_renderer
                        .thumbnail
                        .thumbnails
                        .iter()
                        .max_by(|a, b| a.width.cmp(&b.width))
                        .map(|x| x.url.clone()),
                    contains_cover: !value
                        .thumbnail
                        .music_thumbnail_renderer
                        .thumbnail
                        .thumbnails
                        .is_empty(),
                    name: value
                        .flex_columns
                        .first()
                        .and_then(|x| {
                            x.music_responsive_list_item_flex_column_renderer
                                .text
                                .runs
                                .as_ref()
                                .and_then(|x| x.first().map(|x| x.text.as_str()))
                        })
                        .unwrap_or("N/A")
                        .to_string(),
                    ..Default::default()
                })
            } else {
                None
            }
        })
}

fn album_artist_from_search_result(
    value: &YtSearchResultsContentsListItemRenderer,
) -> Option<YtArtist> {
    value.flex_columns.iter().find_map(|col| {
        col.music_responsive_list_item_flex_column_renderer
            .text
            .runs
            .as_ref()
            .and_then(|runs| {
                runs.iter().find_map(|run| {
                    run.navigation_endpoint.as_ref().and_then(|nav| {
                        nav.browse_endpoint.as_ref().and_then(|browse| {
                            if browse
                                .browse_endpoint_context_supported_configs
                                .browse_endpoint_context_music_config
                                .page_type
                                == "MUSIC_PAGE_TYPE_ARTIST"
                            {
                                Some(YtArtist {
                                    id: browse.browse_id.clone(),
                                    picture: value
                                        .thumbnail
                                        .music_thumbnail_renderer
                                        .thumbnail
                                        .thumbnails
                                        .iter()
                                        .max_by(|a, b| a.width.cmp(&b.width))
                                        .map(|x| x.url.clone()),
                                    contains_cover: !value
                                        .thumbnail
                                        .music_thumbnail_renderer
                                        .thumbnail
                                        .thumbnails
                                        .is_empty(),
                                    name: run.text.clone(),
                                    ..Default::default()
                                })
                            } else {
                                None
                            }
                        })
                    })
                })
            })
    })
}

fn track_album_from_search_result(
    value: &YtSearchResultsContentsListItemRenderer,
) -> Option<YtAlbum> {
    value.flex_columns.iter().find_map(|col| {
        col.music_responsive_list_item_flex_column_renderer
            .text
            .runs
            .as_ref()
            .and_then(|runs| {
                runs.iter().find_map(|run| {
                    run.navigation_endpoint.as_ref().and_then(|nav| {
                        nav.browse_endpoint.as_ref().and_then(|browse| {
                            if browse
                                .browse_endpoint_context_supported_configs
                                .browse_endpoint_context_music_config
                                .page_type
                                == "MUSIC_PAGE_TYPE_ALBUM"
                            {
                                let artist = album_artist_from_search_result(value);
                                Some(YtAlbum {
                                    id: browse.browse_id.clone(),
                                    artist: artist
                                        .as_ref()
                                        .map(|x| x.name.as_str())
                                        .unwrap_or("N/A")
                                        .to_string(),
                                    artist_id: artist
                                        .as_ref()
                                        .map(|x| x.id.as_str())
                                        .unwrap_or("N/A")
                                        .to_string(),
                                    contains_cover: artist
                                        .as_ref()
                                        .is_some_and(|x| x.picture.is_some()),
                                    cover: artist
                                        .as_ref()
                                        .and_then(|x| x.picture.as_ref())
                                        .cloned(),
                                    title: value
                                        .flex_columns
                                        .first()
                                        .and_then(|x| {
                                            x.music_responsive_list_item_flex_column_renderer
                                                .text
                                                .runs
                                                .as_ref()
                                                .and_then(|x| x.first().map(|x| x.text.as_str()))
                                        })
                                        .unwrap_or("N/A")
                                        .to_string(),
                                    ..Default::default()
                                })
                            } else {
                                None
                            }
                        })
                    })
                })
            })
    })
}

fn album_from_search_result(value: &YtSearchResultsContentsListItemRenderer) -> Option<YtAlbum> {
    value
        .navigation_endpoint
        .browse_endpoint
        .as_ref()
        .and_then(|endpoint| {
            if endpoint
                .browse_endpoint_context_supported_configs
                .browse_endpoint_context_music_config
                .page_type
                == "MUSIC_PAGE_TYPE_ALBUM"
            {
                let artist = album_artist_from_search_result(value);
                Some(YtAlbum {
                    id: endpoint.browse_id.clone(),
                    artist: artist
                        .as_ref()
                        .map(|x| x.name.as_str())
                        .unwrap_or("N/A")
                        .to_string(),
                    artist_id: artist
                        .as_ref()
                        .map(|x| x.id.as_str())
                        .unwrap_or("N/A")
                        .to_string(),
                    contains_cover: artist.as_ref().is_some_and(|x| x.picture.is_some()),
                    cover: artist.as_ref().and_then(|x| x.picture.as_ref()).cloned(),
                    title: value
                        .flex_columns
                        .first()
                        .and_then(|x| {
                            x.music_responsive_list_item_flex_column_renderer
                                .text
                                .runs
                                .as_ref()
                                .and_then(|x| x.first().map(|x| x.text.as_str()))
                        })
                        .unwrap_or("N/A")
                        .to_string(),
                    ..Default::default()
                })
            } else {
                None
            }
        })
}

impl From<&YtSearchResults> for Vec<YtArtist> {
    fn from(value: &YtSearchResults) -> Self {
        value
            .contents
            .iter()
            .flat_map(|contents| {
                contents
                    .search_suggestions_section_renderer
                    .contents
                    .iter()
                    .flat_map(|section| {
                        section
                            .music_responsive_list_item_renderer
                            .as_ref()
                            .and_then(artist_from_search_result)
                    })
            })
            .collect::<Vec<_>>()
    }
}

impl From<&YtSearchResults> for Vec<YtAlbum> {
    fn from(value: &YtSearchResults) -> Self {
        value
            .contents
            .iter()
            .flat_map(|contents| {
                contents
                    .search_suggestions_section_renderer
                    .contents
                    .iter()
                    .flat_map(|section| {
                        section
                            .music_responsive_list_item_renderer
                            .as_ref()
                            .and_then(album_from_search_result)
                    })
            })
            .collect::<Vec<_>>()
    }
}

impl From<&YtSearchResults> for Vec<YtVideo> {
    fn from(value: &YtSearchResults) -> Self {
        value
            .contents
            .iter()
            .flat_map(|contents| {
                contents
                    .search_suggestions_section_renderer
                    .contents
                    .iter()
                    .flat_map(|section| {
                        section
                            .music_responsive_list_item_renderer
                            .as_ref()
                            .and_then(video_from_search_result)
                    })
            })
            .collect::<Vec<_>>()
    }
}

impl From<&YtSearchResults> for Vec<YtTrack> {
    fn from(value: &YtSearchResults) -> Self {
        value
            .contents
            .iter()
            .flat_map(|contents| {
                contents
                    .search_suggestions_section_renderer
                    .contents
                    .iter()
                    .flat_map(|section| {
                        section
                            .music_responsive_list_item_renderer
                            .as_ref()
                            .and_then(track_from_search_result)
                    })
            })
            .collect::<Vec<_>>()
    }
}

#[derive(Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct YtSearchResultsFormatted {
    pub albums: Vec<YtAlbum>,
    pub artists: Vec<YtArtist>,
    pub videos: Vec<YtVideo>,
    pub tracks: Vec<YtTrack>,
}

impl From<YtSearchResults> for YtSearchResultsFormatted {
    fn from(value: YtSearchResults) -> Self {
        Self {
            albums: (&value).into(),
            artists: (&value).into(),
            videos: (&value).into(),
            tracks: (&value).into(),
        }
    }
}
