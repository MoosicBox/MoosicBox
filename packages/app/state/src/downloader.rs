use std::sync::Arc;

use crate::{AppState, AppStateError};

impl AppState {
    /// # Errors
    ///
    /// * If the download request fails
    pub async fn local_download(
        &self,
        request: moosicbox_downloader::DownloadRequest,
    ) -> Result<(), AppStateError> {
        use moosicbox_music_api::{CachedMusicApi, MusicApis};
        use moosicbox_music_models::ApiSource;
        use moosicbox_remote_library::RemoteLibraryMusicApi;

        static PROFILE: &str = "master";

        let Some(api_url) = self.api_url.read().await.clone() else {
            return Err(AppStateError::unknown("API_URL not set"));
        };

        let mut music_apis = MusicApis::new();

        for api_source in ApiSource::all() {
            music_apis.add_source(Arc::new(Box::new(CachedMusicApi::new(
                RemoteLibraryMusicApi::new(api_url.clone(), api_source, PROFILE.to_string()),
            ))));
        }

        moosicbox_downloader::download(request, self.library_db.clone(), music_apis).await?;

        Ok(())
    }
}
