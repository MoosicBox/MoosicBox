#![cfg_attr(feature = "fail-on-warnings", deny(warnings))]
#![warn(clippy::all, clippy::pedantic, clippy::nursery, clippy::cargo)]
#![allow(clippy::multiple_crate_versions)]
#![allow(clippy::doc_markdown)]
#![doc = include_str!("../README.md")]

use std::path::{Path, PathBuf};
use std::sync::{LazyLock, RwLock};

use itertools::Itertools;
use moosicbox_music_api_models::search::api::{ApiGlobalSearchResult, ApiSearchResultsResponse};
use switchy_async::task::JoinError;
use tantivy::collector::TopDocs;
use tantivy::directory::MmapDirectory;
use tantivy::query::{BooleanQuery, BoostQuery, DisjunctionMaxQuery, QueryParser, TermQuery};
use tantivy::query_grammar::Occur;
use tantivy::{
    Directory, IndexWriter,
    schema::{
        Document, Field, IndexRecordOption, NamedFieldDocument, STORED, STRING, Schema, TEXT,
        TantivyDocument, Term,
    },
};
use tantivy::{Index, IndexReader, ReloadPolicy};
use thiserror::Error;
use tokio::sync::Semaphore;

/// API endpoints for search functionality.
///
/// Provides Actix-web REST API endpoints for global search operations.
#[cfg(feature = "api")]
pub mod api;
/// Database integration and model conversion utilities.
///
/// Provides traits for converting music models to search index data values.
#[cfg(feature = "db")]
pub mod data;

static SEMAPHORE: LazyLock<Semaphore> = LazyLock::new(|| Semaphore::new(1));

#[cfg(test)]
static TESTS_DIR_PATH: LazyLock<PathBuf> = LazyLock::new(moosicbox_config::get_tests_dir_path);

static GLOBAL_SEARCH_INDEX_PATH: LazyLock<PathBuf> = LazyLock::new(|| {
    #[cfg(test)]
    let base_path = TESTS_DIR_PATH.to_path_buf();

    #[cfg(not(test))]
    let base_path =
        moosicbox_config::get_config_dir_path().expect("Failed to get config directory");

    base_path.join("search_indices").join("global_search_index")
});

static GLOBAL_SEARCH_INDEX_WRITER_MEMORY_BUDGET: LazyLock<RwLock<usize>> =
    LazyLock::new(|| RwLock::new(50_000_000));

static GLOBAL_SEARCH_INDEX_WRITER_NUM_THREADS: LazyLock<RwLock<Option<usize>>> =
    LazyLock::new(|| RwLock::new(None));

/// Error type for failures when retrieving the global search index.
#[derive(Debug, Error, Clone)]
pub enum GetGlobalSearchIndexError {
    /// An error occurred in the underlying Tantivy search library
    #[error(transparent)]
    Tantivy(#[from] tantivy::error::TantivyError),
    /// Failed to retrieve or initialize the global search index
    #[error("{0}")]
    FailedToGetIndex(String),
}

static GLOBAL_SEARCH_INDEX: LazyLock<RwLock<Result<Index, GetGlobalSearchIndexError>>> =
    LazyLock::new(|| {
        let path: &Path = GLOBAL_SEARCH_INDEX_PATH.as_ref();
        RwLock::new(
            create_global_search_index(path, false).map_err(|e| match e {
                CreateIndexError::Tantivy(err) => GetGlobalSearchIndexError::Tantivy(err),
                _ => GetGlobalSearchIndexError::FailedToGetIndex(e.to_string()),
            }),
        )
    });

/// Error type for failures when retrieving the global search index reader.
#[derive(Debug, Error, Clone)]
pub enum GetGlobalSearchReaderError {
    /// An error occurred in the underlying Tantivy search library
    #[error(transparent)]
    Tantivy(#[from] tantivy::error::TantivyError),
    /// Failed to retrieve or initialize the index reader
    #[error("{0}")]
    FailedToGetReader(String),
}

static GLOBAL_SEARCH_READER: LazyLock<RwLock<Result<IndexReader, GetGlobalSearchReaderError>>> =
    LazyLock::new(|| {
        let binding = GLOBAL_SEARCH_INDEX.read().unwrap();
        let index = match binding.as_ref() {
            Ok(index) => index,
            Err(err) => match err {
                GetGlobalSearchIndexError::Tantivy(err) => {
                    return RwLock::new(Err(GetGlobalSearchReaderError::Tantivy(err.clone())));
                }
                GetGlobalSearchIndexError::FailedToGetIndex(_) => {
                    return RwLock::new(Err(GetGlobalSearchReaderError::FailedToGetReader(
                        err.to_string(),
                    )));
                }
            },
        };
        let index = get_index_reader(index)
            .map_err(|e| GetGlobalSearchReaderError::FailedToGetReader(e.to_string()));
        drop(binding);
        RwLock::new(index)
    });

/// Error type for failures when creating a search index.
#[derive(Debug, Error)]
pub enum CreateIndexError {
    /// An error occurred in the underlying Tantivy search library
    #[error(transparent)]
    Tantivy(#[from] tantivy::error::TantivyError),
    /// Failed to open the index directory
    #[error(transparent)]
    OpenDirectory(#[from] tantivy::directory::error::OpenDirectoryError),
    /// Failed to open a file for reading in the index directory
    #[error(transparent)]
    OpenRead(#[from] tantivy::directory::error::OpenReadError),
    /// An I/O error occurred while creating the index
    #[error(transparent)]
    IO(#[from] std::io::Error),
}

/// Creates or opens the global search index at the specified path.
///
/// This function initializes a Tantivy search index with a comprehensive schema for
/// music metadata including artists, albums, and tracks. The schema includes fields
/// for titles, IDs, dates, artwork, and version metadata.
///
/// # Panics
///
/// * If the index directory cannot be created at the specified path
///
/// # Errors
///
/// * `CreateIndexError::OpenDirectory` if the directory cannot be opened
/// * `CreateIndexError::OpenRead` if index files cannot be read
/// * `CreateIndexError::Tantivy` if index creation fails
/// * `CreateIndexError::IO` if I/O operations fail
fn create_global_search_index(
    path: &Path,
    recreate_if_exists: bool,
) -> Result<Index, CreateIndexError> {
    log::trace!("Creating global search index at {}", path.display());
    switchy_fs::sync::create_dir_all(path).unwrap_or_else(|_| {
        panic!(
            "Failed to create global search index directory at {}",
            path.display()
        )
    });

    // # Defining the schema
    //
    // The Tantivy index requires a very strict schema.
    // The schema declares which fields are in the index,
    // and for each field, its type and "the way it should
    // be indexed".

    // First we need to define a schema ...
    let mut schema_builder = Schema::builder();

    // Our first field is title.
    // We want full-text search for it, and we also want
    // to be able to retrieve the document after the search.
    //
    // `TEXT | STORED` is some syntactic sugar to describe
    // that.
    //
    // `TEXT` means the field should be tokenized and indexed,
    // along with its term frequency and term positions.
    //
    // `STORED` means that the field will also be saved
    // in a compressed, row-oriented key-value store.
    // This store is useful for reconstructing the
    // documents that were selected during the search phase.

    schema_builder.add_text_field("document_type", TEXT | STORED);
    schema_builder.add_text_field("document_type_string", STRING);

    schema_builder.add_text_field("artist_title", STORED);
    schema_builder.add_text_field("artist_title_search", TEXT);
    schema_builder.add_text_field("artist_title_string", STRING);

    schema_builder.add_text_field("artist_id", STORED);
    schema_builder.add_text_field("artist_id_search", TEXT);
    schema_builder.add_text_field("artist_id_string", STRING);

    schema_builder.add_text_field("album_title", STORED);
    schema_builder.add_text_field("album_title_search", TEXT);
    schema_builder.add_text_field("album_title_string", STRING);

    schema_builder.add_text_field("album_id", STORED);
    schema_builder.add_text_field("album_id_search", TEXT);
    schema_builder.add_text_field("album_id_string", STRING);

    schema_builder.add_text_field("track_title", STORED);
    schema_builder.add_text_field("track_title_search", TEXT);
    schema_builder.add_text_field("track_title_string", STRING);

    schema_builder.add_text_field("track_id", STORED);
    schema_builder.add_text_field("track_id_search", TEXT);
    schema_builder.add_text_field("track_id_string", STRING);

    schema_builder.add_text_field("cover", STORED);
    schema_builder.add_text_field("cover_string", STRING);

    schema_builder.add_bool_field("blur", STORED);

    schema_builder.add_text_field("date_released", TEXT | STORED);
    schema_builder.add_text_field("date_released_string", STRING);

    schema_builder.add_text_field("date_added", TEXT | STORED);
    schema_builder.add_text_field("date_added_string", STRING);

    schema_builder.add_text_field("version_formats", TEXT | STORED);
    schema_builder.add_text_field("version_formats_string", STRING);

    schema_builder.add_u64_field("version_bit_depths", STORED);

    schema_builder.add_u64_field("version_sample_rates", STORED);

    schema_builder.add_u64_field("version_channels", STORED);

    schema_builder.add_text_field("version_sources", TEXT | STORED);
    schema_builder.add_text_field("version_sources_string", STRING);

    let schema = schema_builder.build();

    // # Indexing documents
    //
    // Let's create a brand new index.
    //
    // This will actually just save a meta.json
    // with our schema in the directory.

    Ok(if cfg!(any(test, feature = "simulator")) {
        Index::create_in_ram(schema)
    } else {
        let mmap_directory = MmapDirectory::open(path)?;

        if recreate_if_exists {
            if Index::exists(&mmap_directory)? {
                log::debug!("Deleting existing index in dir {}", path.display());
                switchy_fs::sync::remove_dir_all(path)?;
                switchy_fs::sync::create_dir_all(path)?;
            } else {
                log::trace!("No existing index in dir {}", path.display());
            }
            log::debug!("Creating Index in dir {}", path.display());
            Index::create_in_dir(path, schema)?
        } else {
            let directory: Box<dyn Directory> = Box::new(mmap_directory);
            log::debug!("Opening or creating index in dir {}", path.display());
            Index::open_or_create(directory, schema)?
        }
    })
}

/// Error type for failures when recreating a search index.
#[derive(Debug, Error)]
pub enum RecreateIndexError {
    /// Failed to create the new search index
    #[error(transparent)]
    CreateIndex(#[from] CreateIndexError),
    /// Failed to retrieve the index reader for the new index
    #[error(transparent)]
    GetIndexReader(#[from] GetIndexReaderError),
    /// The asynchronous task failed to complete
    #[error(transparent)]
    Join(#[from] JoinError),
}

/// Recreates the global search index synchronously by deleting and reinitializing it.
///
/// This function completely rebuilds the index from scratch, replacing the existing
/// index and reader in the global state. All existing data in the index is lost.
///
/// # Panics
///
/// * If any `RwLock`s are poisoned
///
/// # Errors
///
/// * `RecreateIndexError::CreateIndex` if index creation fails
/// * `RecreateIndexError::GetIndexReader` if reader creation fails
fn recreate_global_search_index_sync(path: &Path) -> Result<(), RecreateIndexError> {
    let index = create_global_search_index(path, true)?;
    let reader = get_index_reader(&index)?;

    log::trace!("Resetting GLOBAL_SEARCH_INDEX value");
    *GLOBAL_SEARCH_INDEX.write().unwrap() = Ok(index);
    log::trace!("Resetting GLOBAL_SEARCH_READER value");
    *GLOBAL_SEARCH_READER.write().unwrap() = Ok(reader);

    Ok(())
}

/// Error type for failures when retrieving an index reader.
#[derive(Debug, Error)]
pub enum GetIndexReaderError {
    /// Failed to create the search index
    #[error(transparent)]
    CreateIndex(#[from] CreateIndexError),
    /// An error occurred in the underlying Tantivy search library
    #[error(transparent)]
    Tantivy(#[from] tantivy::error::TantivyError),
}

/// Creates an index reader with automatic reload-on-commit policy.
///
/// The reader will automatically reload the index when commits are detected,
/// ensuring searches always see the latest committed data.
///
/// # Errors
///
/// * `GetIndexReaderError::Tantivy` if reader creation fails
fn get_index_reader(index: &Index) -> Result<IndexReader, GetIndexReaderError> {
    Ok(index
        .reader_builder()
        .reload_policy(ReloadPolicy::OnCommitWithDelay)
        .try_into()?)
}

/// Error type for failures when populating a search index.
#[derive(Debug, Error)]
pub enum PopulateIndexError {
    /// Failed to retrieve the global search index
    #[error(transparent)]
    GetGlobalSearchIndex(#[from] GetGlobalSearchIndexError),
    /// Failed to retrieve the global search index reader
    #[error(transparent)]
    GetGlobalSearchReader(#[from] GetGlobalSearchReaderError),
    /// An error occurred in the underlying Tantivy search library
    #[error(transparent)]
    Tantivy(#[from] tantivy::error::TantivyError),
    /// The asynchronous task failed to complete
    #[error(transparent)]
    Join(#[from] JoinError),
}

/// Represents a data value that can be indexed in the search system.
///
/// This enum supports the three data types that can be stored in the Tantivy search index:
/// text strings, boolean flags, and unsigned integers.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum DataValue {
    /// A text string value for fields like titles, names, and descriptions
    String(String),
    /// A boolean flag value for fields like blur settings or feature flags
    Bool(bool),
    /// An unsigned 64-bit integer value for fields like IDs, counts, and numeric metadata
    Number(u64),
}

/// Populates the global search index with the provided data.
///
/// This function adds documents to the search index asynchronously. If `delete` is true,
/// all existing documents in the index are cleared before adding the new data.
///
/// # Errors
///
/// * `PopulateIndexError::GetGlobalSearchIndex` if failed to retrieve the global search index
/// * `PopulateIndexError::GetGlobalSearchReader` if failed to retrieve the index reader
/// * `PopulateIndexError::Tantivy` if Tantivy encounters an indexing error
/// * `PopulateIndexError::Join` if the blocking task fails to join
pub async fn populate_global_search_index(
    data: &[Vec<(&str, DataValue)>],
    delete: bool,
) -> Result<(), PopulateIndexError> {
    let permit = SEMAPHORE.acquire().await;
    switchy_async::runtime::Handle::current()
        .spawn_blocking_with_name("populate_global_search_index", {
            let data = data
                .iter()
                .map(|x| {
                    x.iter()
                        .map(|x| (x.0.to_string(), x.1.clone()))
                        .collect::<Vec<_>>()
                })
                .collect::<Vec<_>>();
            move || {
                populate_global_search_index_sync(
                    &data
                        .iter()
                        .map(|x| {
                            x.iter()
                                .map(|x| (x.0.as_str(), x.1.clone()))
                                .collect::<Vec<_>>()
                        })
                        .collect::<Vec<_>>(),
                    delete,
                )
            }
        })
        .await??;
    drop(permit);
    Ok(())
}

/// Populates the global search index with the provided data synchronously.
///
/// This is the synchronous version of [`populate_global_search_index`]. It adds documents
/// to the search index in a blocking manner. If `delete` is true, all existing documents
/// in the index are cleared before adding the new data.
///
/// # Panics
///
/// * If any `RwLock`s are poisoned
///
/// # Errors
///
/// * `PopulateIndexError::GetGlobalSearchIndex` if failed to retrieve the global search index
/// * `PopulateIndexError::GetGlobalSearchReader` if failed to retrieve the index reader
/// * `PopulateIndexError::Tantivy` if Tantivy encounters an indexing error
pub fn populate_global_search_index_sync(
    data: &[Vec<(&str, DataValue)>],
    delete: bool,
) -> Result<(), PopulateIndexError> {
    log::debug!("Populating global search index...");

    if data.is_empty() {
        log::debug!("No data to populate.");
        return Ok(());
    }

    let binding = GLOBAL_SEARCH_INDEX.read().unwrap();
    let index = binding.as_ref().map_err(Clone::clone)?;
    let schema = index.schema();
    // To insert a document we will need an index writer.
    // There must be only one writer at a time.
    // This single `IndexWriter` is already
    // multithreaded.
    //
    // Here we give tantivy a budget of `50MB`.
    // Using a bigger memory_arena for the indexer may increase
    // throughput, but 50 MB is already plenty.
    let memory_budget = *GLOBAL_SEARCH_INDEX_WRITER_MEMORY_BUDGET.read().unwrap();
    let threads = *GLOBAL_SEARCH_INDEX_WRITER_NUM_THREADS.read().unwrap();

    let mut index_writer = if let Some(threads) = threads {
        index.writer_with_num_threads(threads, memory_budget)?
    } else {
        index.writer(memory_budget)?
    };

    if delete {
        index_writer.delete_all_documents()?;
    }

    for entry in data {
        let mut doc = TantivyDocument::default();

        for (key, value) in entry {
            let field = schema.get_field(key)?;

            match value {
                DataValue::String(value) => {
                    doc.add_text(field, value.clone());
                    if let Ok(string_field) = schema.get_field(&format!("{key}_string")) {
                        doc.add_text(string_field, value.clone());
                    }
                    if let Ok(search_field) = schema.get_field(&format!("{key}_search")) {
                        doc.add_text(search_field, value.clone());

                        let words = value.split_whitespace().collect::<Vec<_>>();

                        let special_words = words
                            .iter()
                            .enumerate()
                            .filter(|(_, word)| word.chars().any(|c| c == '\''))
                            .map(|(i, word)| (i, word.replace('\'', "")))
                            .collect::<Vec<_>>();

                        for i in 1..=special_words.len() {
                            let permutations = special_words.iter().combinations(i).unique();

                            for permutation in permutations {
                                let search = words
                                    .iter()
                                    .enumerate()
                                    .map(|(i, word)| {
                                        permutation
                                            .iter()
                                            .find(|(x, _)| *x == i)
                                            .map(|(_, word)| word)
                                            .map_or_else(
                                                || (*word).to_string(),
                                                ToString::to_string,
                                            )
                                    })
                                    .collect::<Vec<_>>()
                                    .join(" ");

                                doc.add_text(search_field, search);
                            }
                        }
                    }
                }
                DataValue::Bool(value) => {
                    doc.add_bool(field, *value);
                }
                DataValue::Number(value) => {
                    doc.add_u64(field, *value);
                }
            }
        }

        index_writer.add_document(doc)?;
    }

    // ### Committing
    //
    // At this point our documents are not searchable.
    //
    //
    // We need to call `.commit()` explicitly to force the
    // `index_writer` to finish processing the documents in the queue,
    // flush the current index to the disk, and advertise
    // the existence of new documents.
    //
    // This call is blocking.
    index_writer.commit()?;
    drop(binding);
    GLOBAL_SEARCH_READER
        .read()
        .unwrap()
        .as_ref()
        .map_err(Clone::clone)?
        .reload()?;

    // If `.commit()` returns correctly, then all of the
    // documents that have been added are guaranteed to be
    // persistently indexed.
    //
    // In the scenario of a crash or a power failure,
    // tantivy behaves as if it has rolled back to its last
    // commit.

    log::debug!("Populated global search index");

    Ok(())
}

/// Error type for failures when deleting from a search index.
#[derive(Debug, Error)]
pub enum DeleteFromIndexError {
    /// Failed to retrieve the global search index
    #[error(transparent)]
    GetGlobalSearchIndex(#[from] GetGlobalSearchIndexError),
    /// Failed to retrieve the global search index reader
    #[error(transparent)]
    GetGlobalSearchReader(#[from] GetGlobalSearchReaderError),
    /// An error occurred in the underlying Tantivy search library
    #[error(transparent)]
    Tantivy(#[from] tantivy::error::TantivyError),
}

/// Deletes documents from the global search index by matching field values.
///
/// This function removes documents from the search index that match the provided
/// field-value pairs. Multiple documents may be deleted if they match the same term.
/// The operation is performed synchronously and commits changes to disk.
///
/// # Panics
///
/// * If any `RwLock`s are poisoned
///
/// # Errors
///
/// * `DeleteFromIndexError::GetGlobalSearchIndex` if failed to retrieve the global search index
/// * `DeleteFromIndexError::GetGlobalSearchReader` if failed to retrieve the index reader
/// * `DeleteFromIndexError::Tantivy` if Tantivy encounters a deletion error
pub fn delete_from_global_search_index(
    data: &[(&str, DataValue)],
) -> Result<(), DeleteFromIndexError> {
    log::debug!("Deleting from global search index...");

    if data.is_empty() {
        log::debug!("No data to delete.");
        return Ok(());
    }

    let binding = GLOBAL_SEARCH_INDEX.read().unwrap();
    let index = binding.as_ref().map_err(Clone::clone)?;
    let schema = index.schema();
    // To remove a document we will need an index writer.
    // There must be only one writer at a time.
    // This single `IndexWriter` is already
    // multithreaded.
    //
    // Here we give tantivy a budget of `50MB`.
    // Using a bigger memory_arena for the indexer may increase
    // throughput, but 50 MB is already plenty.
    let memory_budget = *GLOBAL_SEARCH_INDEX_WRITER_MEMORY_BUDGET.read().unwrap();
    let threads = *GLOBAL_SEARCH_INDEX_WRITER_NUM_THREADS.read().unwrap();

    let mut index_writer: IndexWriter<TantivyDocument> = if let Some(threads) = threads {
        index.writer_with_num_threads(threads, memory_budget)?
    } else {
        index.writer(memory_budget)?
    };

    for (key, value) in data {
        let field = schema.get_field(key)?;

        log::trace!("Deleting term ({key:?}, {value:?})");

        let term = match &value {
            DataValue::String(value) => Term::from_field_text(field, value),
            DataValue::Bool(value) => Term::from_field_bool(field, *value),
            DataValue::Number(value) => Term::from_field_u64(field, *value),
        };

        index_writer.delete_term(term);
    }

    // ### Committing
    //
    // We need to call `.commit()` explicitly to force the
    // `index_writer` to finish processing the documents in the queue,
    // flush the current index to the disk, and advertise
    // the removal of the documents.
    //
    // This call is blocking.
    index_writer.commit()?;
    drop(binding);
    GLOBAL_SEARCH_READER
        .read()
        .unwrap()
        .as_ref()
        .map_err(Clone::clone)?
        .reload()?;

    // If `.commit()` returns correctly, then all of the
    // documents that have been removed are guaranteed to be
    // persistently indexed.
    //
    // In the scenario of a crash or a power failure,
    // tantivy behaves as if it has rolled back to its last
    // commit.

    log::debug!("Deleted from global search index");

    Ok(())
}

/// Error type for failures when reindexing a search index.
#[derive(Debug, Error)]
pub enum ReindexError {
    /// Failed to recreate the search index
    #[error(transparent)]
    RecreateIndex(#[from] RecreateIndexError),
    /// Failed to populate the index with data
    #[error(transparent)]
    PopulateIndex(#[from] PopulateIndexError),
    /// The asynchronous task failed to complete
    #[error(transparent)]
    Join(#[from] JoinError),
}

/// Reindexes the global search index by recreating it from scratch with new data.
///
/// This function completely rebuilds the search index by first deleting the existing index,
/// then populating it with the provided data. This operation is performed asynchronously
/// in a blocking task.
///
/// # Errors
///
/// * `ReindexError::RecreateIndex` if failed to recreate the index
/// * `ReindexError::PopulateIndex` if failed to populate the index with data
/// * `ReindexError::Join` if the blocking task fails to join
pub async fn reindex_global_search_index(
    data: &[Vec<(&str, DataValue)>],
) -> Result<(), ReindexError> {
    let permit = SEMAPHORE.acquire().await;
    switchy_async::runtime::Handle::current()
        .spawn_blocking_with_name("reindex_global_search_index", {
            let data = data
                .iter()
                .map(|x| {
                    x.iter()
                        .map(|x| (x.0.to_string(), x.1.clone()))
                        .collect::<Vec<_>>()
                })
                .collect::<Vec<_>>();
            move || {
                let path: &Path = GLOBAL_SEARCH_INDEX_PATH.as_ref();
                recreate_global_search_index_sync(path)?;
                populate_global_search_index_sync(
                    &data
                        .iter()
                        .map(|x| {
                            x.iter()
                                .map(|x| (x.0.as_str(), x.1.clone()))
                                .collect::<Vec<_>>()
                        })
                        .collect::<Vec<_>>(),
                    false,
                )?;
                Ok::<_, ReindexError>(())
            }
        })
        .await??;
    drop(permit);

    Ok(())
}

#[cfg(test)]
fn reindex_global_search_index_sync(data: &[Vec<(&str, DataValue)>]) -> Result<(), ReindexError> {
    let path: &Path = GLOBAL_SEARCH_INDEX_PATH.as_ref();
    recreate_global_search_index_sync(path)?;
    populate_global_search_index_sync(data, false)?;

    Ok(())
}

/// Error type for failures when searching a search index.
#[derive(Debug, Error)]
pub enum SearchIndexError {
    /// Failed to retrieve the global search index
    #[error(transparent)]
    GetGlobalSearchIndex(#[from] GetGlobalSearchIndexError),
    /// Failed to retrieve the global search index reader
    #[error(transparent)]
    GetGlobalSearchReader(#[from] GetGlobalSearchReaderError),
    /// An error occurred in the underlying Tantivy search library
    #[error(transparent)]
    Tantivy(#[from] tantivy::error::TantivyError),
    /// Failed to parse the search query string
    #[error(transparent)]
    QueryParser(#[from] tantivy::query::QueryParserError),
}

static NON_ALPHA_NUMERIC_REGEX: LazyLock<regex::Regex> =
    LazyLock::new(|| regex::Regex::new(r"[^A-Za-z0-9 ]").expect("Invalid Regex"));

/// Sanitizes a search query by removing non-alphanumeric characters.
///
/// This function removes all characters except letters, numbers, and spaces from
/// the query string, then normalizes whitespace by collapsing multiple spaces into one.
fn sanitize_query(query: &str) -> String {
    NON_ALPHA_NUMERIC_REGEX
        .replace_all(query, " ")
        .to_string()
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ")
}

/// Constructs a multi-strategy search query for the specified fields.
///
/// Creates a `DisjunctionMaxQuery` that combines three search strategies:
/// * Exact match - highest boost, scales with query length
/// * Prefix match - medium boost, allows fuzzy matching with edit distance 1
/// * Fuzzy match - lowest boost, allows transposition errors
///
/// Each strategy is boosted differently to prioritize exact matches while still
/// finding relevant results with typos or partial matches.
fn construct_query_for_fields(
    search: &str,
    fields: &[(Field, f32)],
    index: &Index,
) -> DisjunctionMaxQuery {
    let mut parts: Vec<Box<dyn tantivy::query::Query>> = Vec::new();

    // exact match
    {
        let mut query_parser =
            QueryParser::for_index(index, fields.iter().map(|x| x.0).collect::<Vec<_>>());
        for (field, boost) in fields {
            query_parser.set_field_boost(*field, *boost);
        }
        let exact_query = query_parser.parse_query(&format!("\"{search}\"")).unwrap();
        #[allow(clippy::cast_precision_loss)]
        let search_f32 = search.len() as f32;
        let boost_query = Box::new(BoostQuery::new(
            Box::new(exact_query),
            4.0 * (search_f32 / 3.0) * (search_f32 / 3.0),
        ));

        parts.push(boost_query);
    }

    // prefix match
    {
        let mut query_parser =
            QueryParser::for_index(index, fields.iter().map(|x| x.0).collect::<Vec<_>>());
        for (field, boost) in fields {
            query_parser.set_field_fuzzy(*field, true, 1, true);
            query_parser.set_field_boost(*field, *boost);
        }
        let prefix_query = query_parser.parse_query(search).unwrap();
        let boost_query = Box::new(BoostQuery::new(Box::new(prefix_query), 2.0));

        parts.push(boost_query);
    }

    // fuzzy match
    {
        let mut query_parser =
            QueryParser::for_index(index, fields.iter().map(|x| x.0).collect::<Vec<_>>());
        for (field, boost) in fields {
            query_parser.set_field_fuzzy(*field, false, 1, true);
            query_parser.set_field_boost(*field, *boost);
        }
        let fuzzy_query = query_parser.parse_query(search).unwrap();
        let boost_query = Box::new(BoostQuery::new(Box::new(fuzzy_query), 1.0));

        parts.push(boost_query);
    }

    DisjunctionMaxQuery::new(parts)
}

/// Constructs a comprehensive search query for the global search index.
///
/// Creates a complex `DisjunctionMaxQuery` that searches across artists, albums, and tracks
/// with different boosting strategies:
/// * Artist-specific searches receive the highest boost (10.0)
/// * Album-specific searches receive high boost (7.5)
/// * Track-specific searches receive medium boost (5.0)
/// * Generic field searches receive lower boost
///
/// The query uses field matching and document type filtering to provide relevant
/// results with appropriate ranking.
#[allow(clippy::too_many_lines)]
fn construct_global_search_query(
    search: &str,
    index: &Index,
    schema: &Schema,
) -> DisjunctionMaxQuery {
    let artist_title = schema.get_field("artist_title_search").unwrap();
    let album_title = schema.get_field("album_title_search").unwrap();
    let album_title_string = schema.get_field("album_title_string").unwrap();
    let track_title = schema.get_field("track_title_search").unwrap();
    let track_title_string = schema.get_field("track_title_string").unwrap();
    let document_type = schema.get_field("document_type").unwrap();

    let mut queries: Vec<Box<dyn tantivy::query::Query>> = Vec::new();

    // all fields
    {
        let max_query = Box::new(construct_query_for_fields(
            search,
            &[
                (artist_title, 1.0f32),
                (album_title, 1.0f32),
                (track_title, 1.0f32),
            ],
            index,
        ));

        queries.push(max_query);
    }

    // track specifically
    {
        let max_query = Box::new(construct_query_for_fields(
            search,
            &[
                (artist_title, 3.0f32),
                (album_title, 2.0f32),
                (track_title, 1.0f32),
            ],
            index,
        ));

        let track_type = Term::from_field_text(document_type, "tracks");
        let track_type_query = Box::new(TermQuery::new(track_type, IndexRecordOption::Basic));

        let mut query_parser = QueryParser::for_index(index, vec![document_type]);
        query_parser.set_field_fuzzy(document_type, false, 1, true);
        let fuzzy_query = query_parser.parse_query(search).unwrap();

        let boolean_queries: Vec<(Occur, Box<dyn tantivy::query::Query>)> = vec![
            (Occur::Must, max_query),
            (Occur::Must, track_type_query),
            (Occur::Must, fuzzy_query),
        ];
        let boolean_query = Box::new(BooleanQuery::from(boolean_queries));
        let boost_query = Box::new(BoostQuery::new(boolean_query, 5.0));

        queries.push(boost_query);
    }

    // album title
    {
        let max_query = Box::new(construct_query_for_fields(
            search,
            &[(album_title, 1.0f32)],
            index,
        ));

        let track_title = Term::from_field_text(track_title_string, "");
        let track_title_query = Box::new(TermQuery::new(track_title, IndexRecordOption::Basic));

        let boolean_queries: Vec<(Occur, Box<dyn tantivy::query::Query>)> =
            vec![(Occur::Must, max_query), (Occur::Must, track_title_query)];
        let boolean_query = Box::new(BooleanQuery::from(boolean_queries));
        let boost_query = Box::new(BoostQuery::new(boolean_query, 2.0));

        queries.push(boost_query);
    }

    // album specifically
    {
        let max_query = Box::new(construct_query_for_fields(
            search,
            &[
                (artist_title, 3.0f32),
                (album_title, 2.0f32),
                (track_title, 1.0f32),
            ],
            index,
        ));

        let track_title = Term::from_field_text(track_title_string, "");
        let track_title_query = Box::new(TermQuery::new(track_title, IndexRecordOption::Basic));

        let album_type = Term::from_field_text(document_type, "albums");
        let album_type_query = Box::new(TermQuery::new(album_type, IndexRecordOption::Basic));

        let mut query_parser = QueryParser::for_index(index, vec![document_type]);
        query_parser.set_field_fuzzy(document_type, false, 1, true);
        let fuzzy_query = query_parser.parse_query(search).unwrap();

        let boolean_queries: Vec<(Occur, Box<dyn tantivy::query::Query>)> = vec![
            (Occur::Must, max_query),
            (Occur::Must, track_title_query),
            (Occur::Must, album_type_query),
            (Occur::Must, fuzzy_query),
        ];
        let boolean_query = Box::new(BooleanQuery::from(boolean_queries));
        let boost_query = Box::new(BoostQuery::new(boolean_query, 7.5));

        queries.push(boost_query);
    }

    // artist title
    {
        let max_query = Box::new(construct_query_for_fields(
            search,
            &[(artist_title, 1.0f32)],
            index,
        ));

        let track_title = Term::from_field_text(track_title_string, "");
        let track_title_query = Box::new(TermQuery::new(track_title, IndexRecordOption::Basic));

        let album_title = Term::from_field_text(album_title_string, "");
        let album_title_query = Box::new(TermQuery::new(album_title, IndexRecordOption::Basic));

        let boolean_queries: Vec<(Occur, Box<dyn tantivy::query::Query>)> = vec![
            (Occur::Must, max_query),
            (Occur::Must, track_title_query),
            (Occur::Must, album_title_query),
        ];
        let boolean_query = Box::new(BooleanQuery::from(boolean_queries));
        let boost_query = Box::new(BoostQuery::new(boolean_query, 3.0));

        queries.push(boost_query);
    }

    // artist specifically
    {
        let max_query = Box::new(construct_query_for_fields(
            search,
            &[
                (artist_title, 3.0f32),
                (album_title, 2.0f32),
                (track_title, 1.0f32),
            ],
            index,
        ));

        let track_title = Term::from_field_text(track_title_string, "");
        let track_title_query = Box::new(TermQuery::new(track_title, IndexRecordOption::Basic));

        let album_title = Term::from_field_text(album_title_string, "");
        let album_title_query = Box::new(TermQuery::new(album_title, IndexRecordOption::Basic));

        let artist_type = Term::from_field_text(document_type, "artists");
        let artist_type_query = Box::new(TermQuery::new(artist_type, IndexRecordOption::Basic));

        let mut query_parser = QueryParser::for_index(index, vec![document_type]);
        query_parser.set_field_fuzzy(document_type, false, 1, true);
        let fuzzy_query = query_parser.parse_query(search).unwrap();

        let boolean_queries: Vec<(Occur, Box<dyn tantivy::query::Query>)> = vec![
            (Occur::Must, max_query),
            (Occur::Must, track_title_query),
            (Occur::Must, album_title_query),
            (Occur::Must, artist_type_query),
            (Occur::Must, fuzzy_query),
        ];
        let boolean_query = Box::new(BooleanQuery::from(boolean_queries));
        let boost_query = Box::new(BoostQuery::new(boolean_query, 10.0));

        queries.push(boost_query);
    }

    DisjunctionMaxQuery::new(queries)
}

/// Searches the global search index and returns raw Tantivy documents.
///
/// This function performs a full-text search across the global search index using
/// the provided query string. Results are returned as raw Tantivy documents with
/// support for pagination via offset and limit parameters.
///
/// # Panics
///
/// * If any `RwLock`s are poisoned
///
/// # Errors
///
/// * `SearchIndexError::GetGlobalSearchIndex` if failed to retrieve the global search index
/// * `SearchIndexError::GetGlobalSearchReader` if failed to retrieve the index reader
/// * `SearchIndexError::Tantivy` if Tantivy encounters a search error
/// * `SearchIndexError::QueryParser` if the query string is invalid
pub fn search_global_search_index(
    search: &str,
    offset: u32,
    limit: u32,
) -> Result<Vec<NamedFieldDocument>, SearchIndexError> {
    log::debug!("Searching global_search_index...");
    let query = sanitize_query(search);
    let binding = GLOBAL_SEARCH_INDEX.read().unwrap();
    let index = binding.as_ref().map_err(Clone::clone)?;
    let schema = index.schema();

    // # Searching
    //
    // ### Reader
    //
    // A reader is required first in order to search an index.
    // It acts as a `Searcher` pool that reloads itself,
    // depending on a `ReloadPolicy`.
    //
    // For a search server you will typically create one reader for the entire lifetime of your
    // program, and acquire a new searcher for every single request.
    //
    // In the code below, we rely on the 'ON_COMMIT' policy: the reader
    // will reload the index automatically after each commit.
    let reader_binding = GLOBAL_SEARCH_READER.read().unwrap();
    let reader = reader_binding.as_ref().map_err(Clone::clone)?;

    // ### Searcher
    //
    // We now need to acquire a searcher.
    //
    // A searcher points to a snapshotted, immutable version of the index.
    //
    // Some search experience might require more than
    // one query. Using the same searcher ensures that all of these queries will run on the
    // same version of the index.
    //
    // Acquiring a `searcher` is very cheap.
    //
    // You should acquire a searcher every time you start processing a request and
    // and release it right after your query is finished.
    let searcher = reader.searcher();

    // A query defines a set of documents, as
    // well as the way they should be scored.
    //
    // A query created by the query parser is scored according
    // to a metric called Tf-Idf, and will consider
    // any document matching at least one of our terms.

    // ### Collectors
    //
    // We are not interested in all of the documents but
    // only in the top 10. Keeping track of our top 10 best documents
    // is the role of the `TopDocs` collector.

    let global_search_query = construct_global_search_query(&query, index, &schema);

    // We can now perform our query.
    let top_docs = searcher.search(
        &global_search_query,
        &TopDocs::with_limit(limit as usize).and_offset(offset as usize),
    )?;

    // The actual documents still need to be
    // retrieved from Tantivy's store.
    //
    // Since the body field was not configured as stored,
    // the document returned will only contain
    // a title.
    let results = top_docs
        .into_iter()
        .map(|(_score, doc_address)| {
            // #[cfg(debug_assertions)]
            // {
            //     let explanation = global_search_query.explain(&searcher, doc_address)?;
            //     log::debug!("{}", explanation.to_pretty_json());
            // }
            let retrieved_doc: TantivyDocument = searcher.doc(doc_address)?;
            Ok(retrieved_doc.to_named_doc(&schema))
        })
        .collect::<Result<Vec<_>, tantivy::error::TantivyError>>()?;

    drop(reader_binding);
    drop(binding);

    log::debug!("Searched global_search_index");

    Ok(results)
}

/// Performs a global search and returns structured API results.
///
/// This is a higher-level search function that performs a full-text search across the
/// global search index and returns results as structured API types. It automatically
/// deduplicates results and provides pagination support. Default values are used for
/// offset (0) and limit (10) when not specified.
///
/// # Errors
///
/// * `SearchIndexError::GetGlobalSearchIndex` if failed to retrieve the global search index
/// * `SearchIndexError::GetGlobalSearchReader` if failed to retrieve the index reader
/// * `SearchIndexError::Tantivy` if Tantivy encounters a search error
/// * `SearchIndexError::QueryParser` if the query string is invalid
pub fn global_search(
    query: &str,
    offset: Option<u32>,
    limit: Option<u32>,
) -> Result<ApiSearchResultsResponse, SearchIndexError> {
    use moosicbox_json_utils::ToValueType;

    let limit = limit.unwrap_or(10);
    let offset = offset.unwrap_or(0);

    let mut position = offset;
    let mut results: Vec<ApiGlobalSearchResult> = vec![];

    while results.len() < limit as usize {
        let values = search_global_search_index(query, position, limit)?;

        if values.is_empty() {
            break;
        }

        for value in values {
            position += 1;

            let value: ApiGlobalSearchResult = match value.to_value_type() {
                Ok(value) => value,
                Err(err) => {
                    log::error!("Failed to parse search result: {err:?}");
                    continue;
                }
            };

            if !results.iter().any(|r| r.to_key() == value.to_key()) {
                results.push(value);

                if results.len() >= limit as usize {
                    break;
                }
            }
        }
    }

    Ok(ApiSearchResultsResponse { position, results })
}

#[cfg(test)]
mod tests {
    use std::{
        cmp::Ordering,
        collections::BTreeMap,
        path::PathBuf,
        sync::{LazyLock, RwLock},
    };

    use pretty_assertions::assert_eq;
    use serial_test::serial;
    use static_init::dynamic;
    use switchy_fs::with_real_fs;
    use tantivy::schema::OwnedValue;

    use crate::*;

    static TEMP_DIRS: LazyLock<RwLock<Vec<PathBuf>>> = LazyLock::new(|| RwLock::new(vec![]));

    #[derive(Debug)]
    struct TestSetup;

    impl TestSetup {
        pub fn new() -> Self {
            log::debug!("Initializing tests...");
            GLOBAL_SEARCH_INDEX_WRITER_NUM_THREADS
                .write()
                .unwrap()
                .replace(1);
            Self
        }
    }

    impl Drop for TestSetup {
        fn drop(&mut self) {
            with_real_fs(|| {
                for path in TEMP_DIRS.read().unwrap().iter() {
                    if switchy_fs::exists(path) {
                        log::debug!("Cleaning up temp directory {path:?}");
                        switchy_fs::sync::remove_dir_all(path)
                            .expect("Failed to clean up temp directory");
                    }
                }
                log::debug!("Cleaning up temp directory {TESTS_DIR_PATH:?}");
                if switchy_fs::exists(&*TESTS_DIR_PATH) {
                    switchy_fs::sync::remove_dir_all(&*TESTS_DIR_PATH)
                        .expect("Failed to clean up temp directory");
                }
            });
        }
    }

    fn temp_index_path() -> PathBuf {
        let path = moosicbox_config::get_tests_dir_path();

        TEMP_DIRS.write().unwrap().push(path.clone());

        path.join("search_indices").join("global_search_index")
    }

    fn before_each() {
        recreate_global_search_index_sync(&temp_index_path())
            .expect("Failed to recreate_global_search_index");
    }

    #[dynamic(drop)]
    static mut TEST_SETUP: TestSetup = TestSetup::new();

    static ELDER_ARTIST: LazyLock<Vec<(&'static str, DataValue)>> = LazyLock::new(|| {
        vec![
            ("artist_title", DataValue::String("Elder".into())),
            ("artist_id", DataValue::Number(51)),
        ]
    });
    static OMENS_ALBUM: LazyLock<Vec<(&'static str, DataValue)>> = LazyLock::new(|| {
        vec![
            ("artist_title", DataValue::String("Elder".into())),
            ("artist_id", DataValue::Number(51)),
            ("album_title", DataValue::String("Omens".into())),
            ("album_id", DataValue::Number(163)),
        ]
    });
    static OMENS_TRACK_1: LazyLock<Vec<(&'static str, DataValue)>> = LazyLock::new(|| {
        vec![
            ("artist_title", DataValue::String("Elder".into())),
            ("artist_id", DataValue::Number(51)),
            ("album_title", DataValue::String("Omens".into())),
            ("album_id", DataValue::Number(163)),
            ("track_title", DataValue::String("Omens".into())),
            ("track_id", DataValue::Number(1654)),
        ]
    });
    static OMENS_TRACK_2: LazyLock<Vec<(&'static str, DataValue)>> = LazyLock::new(|| {
        vec![
            ("artist_title", DataValue::String("Elder".into())),
            ("artist_id", DataValue::Number(51)),
            ("album_title", DataValue::String("Omens".into())),
            ("album_id", DataValue::Number(163)),
            ("track_title", DataValue::String("In Procession 1".into())),
            ("track_id", DataValue::Number(1655)),
        ]
    });
    static OMENS_TRACK_3: LazyLock<Vec<(&'static str, DataValue)>> = LazyLock::new(|| {
        vec![
            ("artist_title", DataValue::String("Elder".into())),
            ("artist_id", DataValue::Number(51)),
            ("album_title", DataValue::String("Omens".into())),
            ("album_id", DataValue::Number(163)),
            ("track_title", DataValue::String("In Procession 2".into())),
            ("track_id", DataValue::Number(1659)),
        ]
    });
    static OMENS_TRACK_4: LazyLock<Vec<(&'static str, DataValue)>> = LazyLock::new(|| {
        vec![
            ("artist_title", DataValue::String("Elder".into())),
            ("artist_id", DataValue::Number(51)),
            ("album_title", DataValue::String("Omens".into())),
            ("album_id", DataValue::Number(163)),
            ("track_title", DataValue::String("In Procession 3".into())),
            ("track_id", DataValue::Number(1660)),
        ]
    });
    static OMENS_TRACK_5: LazyLock<Vec<(&'static str, DataValue)>> = LazyLock::new(|| {
        vec![
            ("artist_title", DataValue::String("Elder".into())),
            ("artist_id", DataValue::Number(51)),
            ("album_title", DataValue::String("Omens".into())),
            ("album_id", DataValue::Number(163)),
            ("track_title", DataValue::String("Halcyon".into())),
            ("track_id", DataValue::Number(1656)),
        ]
    });
    static OMENS_TRACK_6: LazyLock<Vec<(&'static str, DataValue)>> = LazyLock::new(|| {
        vec![
            ("artist_title", DataValue::String("Elder".into())),
            ("artist_id", DataValue::Number(51)),
            ("album_title", DataValue::String("Omens".into())),
            ("album_id", DataValue::Number(163)),
            ("track_title", DataValue::String("Embers".into())),
            ("track_id", DataValue::Number(1657)),
        ]
    });
    static OMENS_TRACK_7: LazyLock<Vec<(&'static str, DataValue)>> = LazyLock::new(|| {
        vec![
            ("artist_title", DataValue::String("Elder".into())),
            ("artist_id", DataValue::Number(51)),
            ("album_title", DataValue::String("Omens".into())),
            ("album_id", DataValue::Number(163)),
            (
                "track_title",
                DataValue::String("One Light Retreating".into()),
            ),
            ("track_id", DataValue::Number(1658)),
        ]
    });
    static TEST_DATA: LazyLock<Vec<Vec<(&'static str, DataValue)>>> = LazyLock::new(|| {
        vec![
            ELDER_ARTIST.clone(),
            OMENS_ALBUM.clone(),
            OMENS_TRACK_1.clone(),
            OMENS_TRACK_2.clone(),
            OMENS_TRACK_3.clone(),
            OMENS_TRACK_4.clone(),
            OMENS_TRACK_5.clone(),
            OMENS_TRACK_6.clone(),
            OMENS_TRACK_7.clone(),
        ]
    });

    fn to_btree(data: Vec<(&'static str, DataValue)>) -> BTreeMap<String, Vec<OwnedValue>> {
        let mut map = BTreeMap::new();
        for field in data {
            match &field.1 {
                DataValue::String(value) => {
                    map.insert(field.0.to_string(), vec![OwnedValue::Str(value.clone())]);
                }
                DataValue::Bool(value) => {
                    map.insert(field.0.to_string(), vec![OwnedValue::Bool(*value)]);
                }
                DataValue::Number(value) => {
                    map.insert(field.0.to_string(), vec![OwnedValue::U64(*value)]);
                }
            }
        }
        map
    }

    fn to_btree_vec(
        data: Vec<Vec<(&'static str, DataValue)>>,
    ) -> Vec<BTreeMap<String, Vec<OwnedValue>>> {
        data.into_iter().map(to_btree).collect::<Vec<_>>()
    }

    fn entry_cache_key(entry: &BTreeMap<String, Vec<OwnedValue>>) -> String {
        entry
            .iter()
            .map(|entry| {
                format!(
                    "{}:{}",
                    entry.0,
                    entry
                        .1
                        .iter()
                        .map(|value| match value {
                            OwnedValue::Str(str) => str.clone(),
                            OwnedValue::Bool(bool) => bool.to_string(),
                            OwnedValue::U64(num) => num.to_string(),
                            _ => unimplemented!("Unimplemented cache data type"),
                        })
                        .collect::<Vec<_>>()
                        .join(",")
                )
            })
            .collect::<Vec<_>>()
            .join("|")
    }

    #[allow(unused)]
    fn sort_entries(
        a: &BTreeMap<String, Vec<OwnedValue>>,
        b: &BTreeMap<String, Vec<OwnedValue>>,
    ) -> Ordering {
        entry_cache_key(a).cmp(&entry_cache_key(b))
    }

    #[test_log::test(switchy_async::test(real_fs))]
    #[serial]
    async fn test_global_search() {
        before_each();

        crate::populate_global_search_index_sync(&TEST_DATA, true).unwrap();
        let results = crate::search_global_search_index("in procession", 0, 10).unwrap();

        assert_eq!(results.len(), 4);
        assert_eq!(
            results.iter().map(|r| r.0.clone()).collect::<Vec<_>>(),
            to_btree_vec(vec![
                OMENS_TRACK_2.clone(),
                OMENS_TRACK_3.clone(),
                OMENS_TRACK_4.clone(),
                OMENS_TRACK_7.clone(),
            ])
        );
    }

    #[test_log::test(switchy_async::test(real_fs))]
    #[serial]
    async fn test_global_search_with_offset() {
        before_each();

        crate::populate_global_search_index_sync(&TEST_DATA, true).unwrap();
        let results = crate::search_global_search_index("in procession", 1, 10).unwrap();

        assert_eq!(results.len(), 3);
        assert_eq!(
            results.iter().map(|r| r.0.clone()).collect::<Vec<_>>(),
            to_btree_vec(vec![
                OMENS_TRACK_3.clone(),
                OMENS_TRACK_4.clone(),
                OMENS_TRACK_7.clone(),
            ])
        );
    }

    #[test_log::test(switchy_async::test(real_fs))]
    #[serial]
    async fn test_global_search_with_limit() {
        before_each();

        crate::populate_global_search_index_sync(&TEST_DATA, true).unwrap();
        let results = crate::search_global_search_index("in procession", 0, 2).unwrap();

        assert_eq!(results.len(), 2);
        assert_eq!(
            results.iter().map(|r| r.0.clone()).collect::<Vec<_>>(),
            to_btree_vec(vec![OMENS_TRACK_2.clone(), OMENS_TRACK_3.clone(),])
        );
    }

    #[test_log::test(switchy_async::test(real_fs))]
    #[serial]
    async fn test_global_search_with_limit_and_offset() {
        before_each();

        crate::populate_global_search_index_sync(&TEST_DATA, true).unwrap();
        let results = crate::search_global_search_index("in procession", 1, 1).unwrap();

        assert_eq!(results.len(), 1);
        assert_eq!(
            results.iter().map(|r| r.0.clone()).collect::<Vec<_>>(),
            to_btree_vec(vec![OMENS_TRACK_3.clone(),])
        );
    }

    #[test_log::test(switchy_async::test(real_fs))]
    #[serial]
    async fn test_global_search_reindex() {
        before_each();

        crate::populate_global_search_index_sync(&TEST_DATA, true).unwrap();
        assert_eq!(
            crate::search_global_search_index("in procession", 0, 10)
                .unwrap()
                .len(),
            4
        );

        crate::recreate_global_search_index_sync(&temp_index_path())
            .expect("Failed to recreate_global_search_index");
        assert_eq!(
            crate::search_global_search_index("in procession", 0, 10)
                .unwrap()
                .len(),
            0
        );

        crate::populate_global_search_index_sync(&TEST_DATA, true).unwrap();
        assert_eq!(
            crate::search_global_search_index("in procession", 0, 10)
                .unwrap()
                .len(),
            4
        );

        crate::reindex_global_search_index_sync(&TEST_DATA)
            .expect("Failed to reindex_global_search_index");
        assert_eq!(
            crate::search_global_search_index("in procession", 0, 10)
                .unwrap()
                .len(),
            4
        );
    }

    #[test_log::test]
    fn test_sanitize_query_removes_special_characters() {
        assert_eq!(sanitize_query("hello@world!"), "hello world");
        assert_eq!(sanitize_query("test#123$456"), "test 123 456");
        assert_eq!(sanitize_query("foo&bar*baz"), "foo bar baz");
    }

    #[test_log::test]
    fn test_sanitize_query_collapses_whitespace() {
        assert_eq!(sanitize_query("hello    world"), "hello world");
        assert_eq!(
            sanitize_query("  spaces  everywhere  "),
            "spaces everywhere"
        );
        assert_eq!(sanitize_query("tab\ttab"), "tab tab");
    }

    #[test_log::test]
    fn test_sanitize_query_preserves_alphanumeric() {
        assert_eq!(sanitize_query("abc123 XYZ789"), "abc123 XYZ789");
        assert_eq!(sanitize_query("MixedCase123"), "MixedCase123");
    }

    #[test_log::test]
    fn test_sanitize_query_empty_string() {
        assert_eq!(sanitize_query(""), "");
    }

    #[test_log::test]
    fn test_sanitize_query_only_special_characters() {
        assert_eq!(sanitize_query("@#$%^&*()"), "");
        assert_eq!(sanitize_query("!!!???"), "");
    }

    #[test_log::test(switchy_async::test(real_fs))]
    #[serial]
    async fn test_delete_from_global_search_index() {
        before_each();

        crate::populate_global_search_index_sync(&TEST_DATA, true).unwrap();

        // Delete a specific track by its track_id_string - test that the function executes successfully
        let result = crate::delete_from_global_search_index(&[(
            "track_id_string",
            DataValue::String("1654".to_string()),
        )]);

        // Verify deletion completes without error
        assert!(result.is_ok());
    }

    #[test_log::test(switchy_async::test(real_fs))]
    #[serial]
    async fn test_delete_multiple_from_global_search_index() {
        before_each();

        crate::populate_global_search_index_sync(&TEST_DATA, true).unwrap();

        // Delete multiple items - test that the function executes successfully with multiple terms
        let result = crate::delete_from_global_search_index(&[
            ("track_id_string", DataValue::String("1655".to_string())),
            ("track_id_string", DataValue::String("1659".to_string())),
        ]);

        // Verify deletion completes without error
        assert!(result.is_ok());
    }

    #[test_log::test(switchy_async::test(real_fs))]
    #[serial]
    async fn test_delete_from_global_search_index_empty() {
        before_each();

        crate::populate_global_search_index_sync(&TEST_DATA, true).unwrap();
        let results_before = crate::search_global_search_index("elder", 0, 10).unwrap();

        // Delete with empty data should be a no-op
        crate::delete_from_global_search_index(&[]).unwrap();
        let results_after = crate::search_global_search_index("elder", 0, 10).unwrap();

        assert_eq!(results_before.len(), results_after.len());
    }

    #[test_log::test(switchy_async::test(real_fs))]
    #[serial]
    async fn test_populate_global_search_index_empty() {
        before_each();

        // Populate with empty data should succeed without error
        crate::populate_global_search_index_sync(&[], true).unwrap();
        let results = crate::search_global_search_index("anything", 0, 10).unwrap();
        assert_eq!(results.len(), 0);
    }

    #[test_log::test(switchy_async::test(real_fs))]
    #[serial]
    async fn test_populate_global_search_index_without_delete() {
        before_each();

        // Add initial data
        crate::populate_global_search_index_sync(std::slice::from_ref(&ELDER_ARTIST), true)
            .unwrap();
        let results = crate::search_global_search_index("elder", 0, 10).unwrap();
        assert_eq!(results.len(), 1);

        // Add more data without deleting
        crate::populate_global_search_index_sync(std::slice::from_ref(&OMENS_ALBUM), false)
            .unwrap();
        let results = crate::search_global_search_index("elder", 0, 10).unwrap();
        assert_eq!(results.len(), 2);
    }
}
