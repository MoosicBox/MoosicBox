#![cfg_attr(feature = "fail-on-warnings", deny(warnings))]

use std::path::{Path, PathBuf};
use std::sync::RwLock;

use once_cell::sync::Lazy;
use tantivy::collector::TopDocs;
use tantivy::directory::MmapDirectory;
use tantivy::query::QueryParser;
use tantivy::{schema::*, Directory};
use tantivy::{Index, IndexReader, IndexWriter, ReloadPolicy};
use thiserror::Error;

#[cfg(test)]
static TESTS_DIR_PATH: Lazy<PathBuf> = Lazy::new(moosicbox_config::get_tests_dir_path);

static GLOBAL_SEARCH_INDEX_PATH: Lazy<PathBuf> = Lazy::new(|| {
    #[cfg(test)]
    let base_path = TESTS_DIR_PATH.to_path_buf();

    #[cfg(not(test))]
    let base_path =
        moosicbox_config::get_config_dir_path().expect("Failed to get config directory");

    base_path.join("search_indices").join("global_search_index")
});

static GLOBAL_SEARCH_INDEX: Lazy<RwLock<Index>> = Lazy::new(|| {
    RwLock::new(create_global_search_index(false).expect("Failed to create GLOBAL_SEARCH_INDEX"))
});

static GLOBAL_SEARCH_READER: Lazy<RwLock<IndexReader>> = Lazy::new(|| {
    RwLock::new(
        get_index_reader(&GLOBAL_SEARCH_INDEX.read().unwrap()).expect("Failed to get reader"),
    )
});

#[derive(Debug, Error)]
pub enum CreateIndexError {
    #[error(transparent)]
    Tantivy(#[from] tantivy::error::TantivyError),
    #[error(transparent)]
    OpenDirectory(#[from] tantivy::directory::error::OpenDirectoryError),
    #[error(transparent)]
    OpenRead(#[from] tantivy::directory::error::OpenReadError),
    #[error(transparent)]
    IO(#[from] std::io::Error),
}

fn create_global_search_index(recreate_if_exists: bool) -> Result<Index, CreateIndexError> {
    let path: &Path = GLOBAL_SEARCH_INDEX_PATH.as_ref();
    std::fs::create_dir_all(path)
        .unwrap_or_else(|_| panic!("Failed to create global search index directory at {path:?}"));

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
    schema_builder.add_text_field("artist_title", TEXT | STORED);
    schema_builder.add_u64_field("artist_id", STORED);
    schema_builder.add_text_field("album_title", TEXT | STORED);
    schema_builder.add_u64_field("album_id", STORED);
    schema_builder.add_text_field("track_title", TEXT | STORED);
    schema_builder.add_u64_field("track_id", STORED);

    let schema = schema_builder.build();

    // # Indexing documents
    //
    // Let's create a brand new index.
    //
    // This will actually just save a meta.json
    // with our schema in the directory.

    let mmap_directory = MmapDirectory::open(path)?;

    Ok(if recreate_if_exists {
        if Index::exists(&mmap_directory)? {
            log::debug!("Deleting existing index in dir {path:?}");
            std::fs::remove_dir_all(path)?;
            std::fs::create_dir_all(path)?;
        } else {
            log::trace!("No existing index in dir {path:?}");
        }
        log::debug!("Creating Index in dir {path:?}");
        Index::create_in_dir(path, schema.clone())?
    } else {
        let directory: Box<dyn Directory> = Box::new(mmap_directory);
        log::debug!("Opening or creating index in dir {path:?}");
        Index::open_or_create(directory, schema.clone())?
    })
}

#[derive(Debug, Error)]
pub enum RecreateIndexError {
    #[error(transparent)]
    CreateIndex(#[from] CreateIndexError),
    #[error(transparent)]
    GetIndexReader(#[from] GetIndexReaderError),
}

fn recreate_global_search_index() -> Result<(), RecreateIndexError> {
    let index = create_global_search_index(true)?;
    let reader = get_index_reader(&index)?;

    log::trace!("Resetting GLOBAL_SEARCH_INDEX value");
    *GLOBAL_SEARCH_INDEX.write().unwrap() = index;
    log::trace!("Resetting GLOBAL_SEARCH_READER value");
    *GLOBAL_SEARCH_READER.write().unwrap() = reader;

    Ok(())
}

#[derive(Debug, Error)]
pub enum GetIndexReaderError {
    #[error(transparent)]
    Tantivy(#[from] tantivy::error::TantivyError),
}

fn get_index_reader(index: &Index) -> Result<IndexReader, GetIndexReaderError> {
    Ok(index
        .reader_builder()
        .reload_policy(ReloadPolicy::OnCommit)
        .try_into()?)
}

#[derive(Debug, Error)]
pub enum PopulateIndexError {
    #[error(transparent)]
    Tantivy(#[from] tantivy::error::TantivyError),
}

#[derive(Debug, Clone)]
pub enum DataValue {
    String(String),
    Number(u64),
}

pub fn populate_global_search_index(
    data: Vec<Vec<(&str, DataValue)>>,
) -> Result<(), PopulateIndexError> {
    log::debug!("Populating global search index...");
    let index: &Index = &GLOBAL_SEARCH_INDEX.read().unwrap();
    let schema = index.schema();
    // To insert a document we will need an index writer.
    // There must be only one writer at a time.
    // This single `IndexWriter` is already
    // multithreaded.
    //
    // Here we give tantivy a budget of `50MB`.
    // Using a bigger memory_arena for the indexer may increase
    // throughput, but 50 MB is already plenty.
    let mut index_writer: IndexWriter = index.writer(50_000_000)?;

    index_writer.delete_all_documents()?;

    for entry in data {
        let mut doc = Document::default();

        for (key, value) in entry {
            let field = schema.get_field(key)?;

            match value {
                DataValue::String(value) => {
                    doc.add_text(field, value);
                }
                DataValue::Number(value) => {
                    doc.add_u64(field, value);
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
    GLOBAL_SEARCH_READER.read().unwrap().reload()?;

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

#[derive(Debug, Error)]
pub enum ReindexError {
    #[error(transparent)]
    RecreateIndex(#[from] RecreateIndexError),
    #[error(transparent)]
    PopulateIndex(#[from] PopulateIndexError),
}

pub fn reindex_global_search_index(data: Vec<Vec<(&str, DataValue)>>) -> Result<(), ReindexError> {
    recreate_global_search_index()?;
    populate_global_search_index(data)?;

    Ok(())
}

#[derive(Debug, Error)]
pub enum SearchIndexError {
    #[error(transparent)]
    Tantivy(#[from] tantivy::error::TantivyError),
    #[error(transparent)]
    QueryParser(#[from] tantivy::query::QueryParserError),
}

pub fn search_global_search_index(
    query: &str,
    offset: usize,
    limit: usize,
) -> Result<Vec<NamedFieldDocument>, SearchIndexError> {
    log::debug!("Searching global_search_index...");
    let index: &Index = &GLOBAL_SEARCH_INDEX.read().unwrap();
    let schema = index.schema();

    let artist_title = schema.get_field("artist_title").unwrap();
    let album_title = schema.get_field("album_title").unwrap();
    let track_title = schema.get_field("track_title").unwrap();

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
    let reader: &IndexReader = &GLOBAL_SEARCH_READER.read().unwrap();

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

    // ### Query
    let query_parser = QueryParser::for_index(index, vec![artist_title, album_title, track_title]);
    let query = query_parser.parse_query(query)?;

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

    // We can now perform our query.
    let top_docs = searcher.search(&query, &TopDocs::with_limit(limit).and_offset(offset))?;

    // The actual documents still need to be
    // retrieved from Tantivy's store.
    //
    // Since the body field was not configured as stored,
    // the document returned will only contain
    // a title.
    let results = top_docs
        .into_iter()
        .map(|(_score, doc_address)| {
            let retrieved_doc: Document = searcher.doc(doc_address)?;
            Ok(schema.to_named_doc(&retrieved_doc))
        })
        .collect::<Result<Vec<_>, tantivy::error::TantivyError>>()?;

    log::debug!("Searched global_search_index");

    Ok(results)
}

#[cfg(test)]
mod tests {
    use std::{cmp::Ordering, collections::BTreeMap};

    use once_cell::sync::Lazy;
    use pretty_assertions::assert_eq;
    use serial_test::serial;
    use static_init::dynamic;
    use tantivy::schema::Value;

    use crate::{DataValue, TESTS_DIR_PATH};

    #[derive(Debug)]
    struct TestSetup;

    impl TestSetup {
        pub fn new() -> Self {
            log::debug!("Initializing tests...");
            Self
        }
    }

    impl Drop for TestSetup {
        fn drop(&mut self) {
            log::debug!("Cleaning up temp directory {:?}", TESTS_DIR_PATH.as_path());
            std::fs::remove_dir_all(TESTS_DIR_PATH.as_path())
                .expect("Failed to clean up temp directory");
        }
    }

    #[dynamic(drop)]
    static mut TEST_SETUP: TestSetup = TestSetup::new();

    static ELDER_ARTIST: Lazy<Vec<(&'static str, DataValue)>> = Lazy::new(|| {
        vec![
            ("artist_title", DataValue::String("Elder".into())),
            ("artist_id", DataValue::Number(51)),
        ]
    });
    static OMENS_ALBUM: Lazy<Vec<(&'static str, DataValue)>> = Lazy::new(|| {
        vec![
            ("artist_title", DataValue::String("Elder".into())),
            ("artist_id", DataValue::Number(51)),
            ("album_title", DataValue::String("Omens".into())),
            ("album_id", DataValue::Number(163)),
        ]
    });
    static OMENS_TRACK_1: Lazy<Vec<(&'static str, DataValue)>> = Lazy::new(|| {
        vec![
            ("artist_title", DataValue::String("Elder".into())),
            ("artist_id", DataValue::Number(51)),
            ("album_title", DataValue::String("Omens".into())),
            ("album_id", DataValue::Number(163)),
            ("track_title", DataValue::String("Omens".into())),
            ("track_id", DataValue::Number(1654)),
        ]
    });
    static OMENS_TRACK_2: Lazy<Vec<(&'static str, DataValue)>> = Lazy::new(|| {
        vec![
            ("artist_title", DataValue::String("Elder".into())),
            ("artist_id", DataValue::Number(51)),
            ("album_title", DataValue::String("Omens".into())),
            ("album_id", DataValue::Number(163)),
            ("track_title", DataValue::String("In Procession 1".into())),
            ("track_id", DataValue::Number(1655)),
        ]
    });
    static OMENS_TRACK_3: Lazy<Vec<(&'static str, DataValue)>> = Lazy::new(|| {
        vec![
            ("artist_title", DataValue::String("Elder".into())),
            ("artist_id", DataValue::Number(51)),
            ("album_title", DataValue::String("Omens".into())),
            ("album_id", DataValue::Number(163)),
            ("track_title", DataValue::String("In Procession 2".into())),
            ("track_id", DataValue::Number(1659)),
        ]
    });
    static OMENS_TRACK_4: Lazy<Vec<(&'static str, DataValue)>> = Lazy::new(|| {
        vec![
            ("artist_title", DataValue::String("Elder".into())),
            ("artist_id", DataValue::Number(51)),
            ("album_title", DataValue::String("Omens".into())),
            ("album_id", DataValue::Number(163)),
            ("track_title", DataValue::String("In Procession 3".into())),
            ("track_id", DataValue::Number(1660)),
        ]
    });
    static OMENS_TRACK_5: Lazy<Vec<(&'static str, DataValue)>> = Lazy::new(|| {
        vec![
            ("artist_title", DataValue::String("Elder".into())),
            ("artist_id", DataValue::Number(51)),
            ("album_title", DataValue::String("Omens".into())),
            ("album_id", DataValue::Number(163)),
            ("track_title", DataValue::String("Halcyon".into())),
            ("track_id", DataValue::Number(1656)),
        ]
    });
    static OMENS_TRACK_6: Lazy<Vec<(&'static str, DataValue)>> = Lazy::new(|| {
        vec![
            ("artist_title", DataValue::String("Elder".into())),
            ("artist_id", DataValue::Number(51)),
            ("album_title", DataValue::String("Omens".into())),
            ("album_id", DataValue::Number(163)),
            ("track_title", DataValue::String("Embers".into())),
            ("track_id", DataValue::Number(1657)),
        ]
    });
    static OMENS_TRACK_7: Lazy<Vec<(&'static str, DataValue)>> = Lazy::new(|| {
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
    static TEST_DATA: Lazy<Vec<Vec<(&'static str, DataValue)>>> = Lazy::new(|| {
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

    fn to_btree(data: Vec<(&'static str, DataValue)>) -> BTreeMap<String, Vec<Value>> {
        let mut map = BTreeMap::new();
        for field in data {
            map.insert(
                field.0.to_string(),
                vec![match &field.1 {
                    DataValue::String(value) => Value::Str(value.to_string()),
                    DataValue::Number(value) => Value::U64(*value),
                }],
            );
        }
        map
    }

    fn to_btree_vec(
        data: Vec<Vec<(&'static str, DataValue)>>,
    ) -> Vec<BTreeMap<String, Vec<Value>>> {
        data.into_iter().map(to_btree).collect::<Vec<_>>()
    }

    fn entry_cache_key(entry: &BTreeMap<String, Vec<Value>>) -> String {
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
                            Value::Str(str) => str.to_string(),
                            Value::U64(num) => num.to_string(),
                            _ => unimplemented!(),
                        })
                        .collect::<Vec<_>>()
                        .join(",")
                )
            })
            .collect::<Vec<_>>()
            .join("|")
    }

    fn sort_entries(
        a: &BTreeMap<String, Vec<Value>>,
        b: &BTreeMap<String, Vec<Value>>,
    ) -> Ordering {
        entry_cache_key(a).cmp(&entry_cache_key(b))
    }

    #[test_log::test]
    #[serial]
    fn test_global_search() {
        crate::populate_global_search_index(TEST_DATA.clone()).unwrap();
        let results = crate::search_global_search_index("in procession", 0, 10).unwrap();

        assert_eq!(results.len(), 3);
        assert_eq!(
            results.iter().map(|r| r.0.clone()).collect::<Vec<_>>(),
            to_btree_vec(vec![
                OMENS_TRACK_2.clone(),
                OMENS_TRACK_3.clone(),
                OMENS_TRACK_4.clone(),
            ])
        );
    }

    #[test_log::test]
    #[serial]
    fn test_global_search_with_weight() {
        crate::populate_global_search_index(TEST_DATA.clone()).unwrap();
        let results =
            crate::search_global_search_index("(omens) || track_title:(omens)^10", 0, 10).unwrap();

        assert_eq!(results.len(), 8);
        assert_eq!(
            results.first().map(|r| r.0.clone()).unwrap(),
            to_btree(OMENS_TRACK_1.clone(),)
        );
        assert_eq!(
            results
                .iter()
                .map(|r| r.0.clone())
                .collect::<Vec<_>>()
                .sort_by(sort_entries),
            to_btree_vec(vec![
                OMENS_ALBUM.clone(),
                OMENS_TRACK_1.clone(),
                OMENS_TRACK_2.clone(),
                OMENS_TRACK_3.clone(),
                OMENS_TRACK_4.clone(),
                OMENS_TRACK_5.clone(),
                OMENS_TRACK_6.clone(),
                OMENS_TRACK_7.clone(),
            ])
            .sort_by(sort_entries)
        );
    }

    #[test_log::test]
    #[serial]
    fn test_global_search_with_offset() {
        crate::populate_global_search_index(TEST_DATA.clone()).unwrap();
        let results = crate::search_global_search_index("in procession", 1, 10).unwrap();

        assert_eq!(results.len(), 2);
        assert_eq!(
            results.iter().map(|r| r.0.clone()).collect::<Vec<_>>(),
            to_btree_vec(vec![OMENS_TRACK_3.clone(), OMENS_TRACK_4.clone(),])
        );
    }

    #[test_log::test]
    #[serial]
    fn test_global_search_with_limit() {
        crate::populate_global_search_index(TEST_DATA.clone()).unwrap();
        let results = crate::search_global_search_index("in procession", 0, 2).unwrap();

        assert_eq!(results.len(), 2);
        assert_eq!(
            results.iter().map(|r| r.0.clone()).collect::<Vec<_>>(),
            to_btree_vec(vec![OMENS_TRACK_2.clone(), OMENS_TRACK_3.clone(),])
        );
    }

    #[test_log::test]
    #[serial]
    fn test_global_search_with_limit_and_offset() {
        crate::populate_global_search_index(TEST_DATA.clone()).unwrap();
        let results = crate::search_global_search_index("in procession", 1, 1).unwrap();

        assert_eq!(results.len(), 1);
        assert_eq!(
            results.iter().map(|r| r.0.clone()).collect::<Vec<_>>(),
            to_btree_vec(vec![OMENS_TRACK_3.clone(),])
        );
    }

    #[test_log::test]
    #[serial]
    fn test_global_search_reindex() {
        crate::populate_global_search_index(TEST_DATA.clone()).unwrap();
        assert_eq!(
            crate::search_global_search_index("in procession", 0, 10)
                .unwrap()
                .len(),
            3
        );

        crate::recreate_global_search_index().expect("Failed to recreate_global_search_index");
        assert_eq!(
            crate::search_global_search_index("in procession", 0, 10)
                .unwrap()
                .len(),
            0
        );

        crate::populate_global_search_index(TEST_DATA.clone()).unwrap();
        assert_eq!(
            crate::search_global_search_index("in procession", 0, 10)
                .unwrap()
                .len(),
            3
        );

        crate::reindex_global_search_index(TEST_DATA.clone())
            .expect("Failed to reindex_global_search_index");
        assert_eq!(
            crate::search_global_search_index("in procession", 0, 10)
                .unwrap()
                .len(),
            3
        );
    }
}
