//! Static file serving for Actix Web backend.

use std::path::PathBuf;

use actix_web::dev::HttpServiceFactory;
use switchy_web_server::StaticFiles;

/// Extension trait for adding static file serving to Actix apps.
pub trait StaticFilesExt {
    /// Registers static file serving with the application.
    fn with_static_files(self, config: &StaticFiles) -> Self;
}

/// Registers static files with an Actix App.
///
/// This function configures `actix_files::Files` based on the provided `StaticFiles`
/// configuration, including support for index files and SPA fallback.
pub fn register_static_files<T>(app: actix_web::App<T>, config: &StaticFiles) -> actix_web::App<T>
where
    T: actix_service::ServiceFactory<
            actix_web::dev::ServiceRequest,
            Config = (),
            Error = actix_web::Error,
            InitError = (),
        >,
{
    let files = create_files_service(
        config.mount_path().to_string(),
        config.directory().to_path_buf(),
        config.index_file_name().map(String::from),
        config.is_spa_fallback(),
        config.effective_index_file().map(String::from),
    );
    app.service(files)
}

/// Creates an `actix_files::Files` service from the configuration.
fn create_files_service(
    mount_path: String,
    directory: PathBuf,
    index_file: Option<String>,
    spa_fallback: bool,
    effective_index_file: Option<String>,
) -> impl HttpServiceFactory {
    let mut files = actix_files::Files::new(&mount_path, &directory);

    // Set index file if configured
    if let Some(index) = index_file {
        files = files.index_file(index);
    }

    // Set up SPA fallback if enabled
    if spa_fallback {
        let index_path =
            directory.join(effective_index_file.unwrap_or_else(|| "index.html".to_string()));

        files = files.default_handler(actix_web::web::to(move |req: actix_web::HttpRequest| {
            let index_path = index_path.clone();
            async move {
                actix_files::NamedFile::open_async(&index_path)
                    .await
                    .map(|file| file.into_response(&req))
            }
        }));
    }

    files
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    #[test]
    fn test_static_files_config() {
        let config = StaticFiles::new("/static", "./public")
            .index_file("index.html")
            .spa_fallback(true);

        assert_eq!(config.mount_path(), "/static");
        assert_eq!(config.directory(), &PathBuf::from("./public"));
        assert_eq!(config.index_file_name(), Some("index.html"));
        assert!(config.is_spa_fallback());
    }
}
