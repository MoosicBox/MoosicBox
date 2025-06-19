# MoosicBox Database

Database abstraction layer with support for multiple database backends, migrations, and connection pooling.

## Overview

The MoosicBox Database package provides:

- **Multi-Database Support**: SQLite, PostgreSQL, MySQL, and in-memory databases
- **Connection Pooling**: Efficient connection management with automatic scaling
- **Schema Migrations**: Version-controlled database schema evolution
- **Query Builder**: Type-safe query construction and execution
- **Transaction Management**: ACID transaction support with rollback capabilities
- **Performance Monitoring**: Query performance metrics and slow query logging

## Features

### Database Backends
- **SQLite**: File-based database for development and single-user deployments
- **PostgreSQL**: Production-ready with advanced features and JSON support
- **MySQL**: Widely supported relational database with clustering support
- **In-Memory**: Fast testing database that doesn't persist data

### Advanced Features
- **Connection Pooling**: Automatic connection lifecycle management
- **Read Replicas**: Load balancing across multiple database instances
- **Schema Migrations**: Incremental database schema updates
- **Query Optimization**: Automatic query analysis and optimization suggestions
- **Backup & Restore**: Automated database backup and recovery tools

## Usage

### Basic Database Setup

```rust
use moosicbox_database::{Database, DatabaseConfig, DatabaseType};

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    // Configure database
    let config = DatabaseConfig {
        database_type: DatabaseType::PostgreSQL,
        connection_string: "postgresql://user:pass@localhost/moosicbox".to_string(),
        max_connections: 20,
        min_connections: 5,
        connection_timeout_seconds: 30,
        idle_timeout_seconds: 600,
        max_lifetime_seconds: 1800,
    };

    // Initialize database
    let db = Database::new(config).await?;

    // Run migrations
    db.migrate().await?;

    println!("Database initialized successfully");

    Ok(())
}
```

### Connection Management

```rust
use moosicbox_database::{Database, ConnectionPool, PoolConfig};

async fn setup_connection_pool() -> Result<(), Box<dyn std::error::Error>> {
    let pool_config = PoolConfig {
        max_connections: 20,
        min_connections: 5,
        acquire_timeout_seconds: 10,
        idle_timeout_seconds: 600,
        max_lifetime_seconds: 1800,
        test_before_acquire: true,
        test_query: Some("SELECT 1".to_string()),
    };

    let db = Database::with_pool_config(connection_string, pool_config).await?;

    // Get connection from pool
    let mut conn = db.acquire().await?;

    // Use connection
    let result = conn.execute("SELECT COUNT(*) FROM tracks").await?;
    println!("Total tracks: {}", result.rows_affected());

    // Connection automatically returned to pool when dropped
    Ok(())
}
```

### Query Execution

```rust
use moosicbox_database::{Database, QueryBuilder, Row};
use serde::{Deserialize, Serialize};

#[derive(Debug, Serialize, Deserialize, sqlx::FromRow)]
struct Track {
    id: i64,
    title: String,
    artist: String,
    album: String,
    duration: i32,
    file_path: String,
}

async fn query_examples(db: &Database) -> Result<(), Box<dyn std::error::Error>> {
    // Simple query
    let tracks: Vec<Track> = db
        .query_as("SELECT * FROM tracks WHERE artist = ?")
        .bind("The Beatles")
        .fetch_all()
        .await?;

    // Query builder
    let tracks: Vec<Track> = QueryBuilder::new()
        .select("*")
        .from("tracks")
        .where_eq("artist", "The Beatles")
        .where_gt("duration", 180)
        .order_by("title")
        .limit(10)
        .fetch_all::<Track>(db)
        .await?;

    // Raw SQL with parameters
    let track_count: i64 = db
        .query_scalar("SELECT COUNT(*) FROM tracks WHERE album = ?")
        .bind("Abbey Road")
        .fetch_one()
        .await?;

    // Insert data
    let track_id: i64 = db
        .query_scalar("INSERT INTO tracks (title, artist, album, duration, file_path) VALUES (?, ?, ?, ?, ?) RETURNING id")
        .bind("Come Together")
        .bind("The Beatles")
        .bind("Abbey Road")
        .bind(259)
        .bind("/music/beatles/come_together.mp3")
        .fetch_one()
        .await?;

    println!("Inserted track with ID: {}", track_id);

    Ok(())
}
```

### Transactions

```rust
use moosicbox_database::{Database, Transaction};

async fn transaction_example(db: &Database) -> Result<(), Box<dyn std::error::Error>> {
    // Begin transaction
    let mut tx = db.begin().await?;

    // Execute operations within transaction
    let user_id: i64 = tx
        .query_scalar("INSERT INTO users (username, email) VALUES (?, ?) RETURNING id")
        .bind("music_lover")
        .bind("user@example.com")
        .fetch_one()
        .await?;

    let playlist_id: i64 = tx
        .query_scalar("INSERT INTO playlists (user_id, name) VALUES (?, ?) RETURNING id")
        .bind(user_id)
        .bind("My Favorites")
        .fetch_one()
        .await?;

    // Add tracks to playlist
    for track_id in &[1, 2, 3, 4, 5] {
        tx.execute("INSERT INTO playlist_tracks (playlist_id, track_id) VALUES (?, ?)")
            .bind(playlist_id)
            .bind(track_id)
            .fetch_optional()
            .await?;
    }

    // Commit transaction
    tx.commit().await?;

    println!("Created user {} with playlist {}", user_id, playlist_id);

    Ok(())
}

async fn transaction_with_rollback(db: &Database) -> Result<(), Box<dyn std::error::Error>> {
    let mut tx = db.begin().await?;

    // This will succeed
    tx.execute("INSERT INTO artists (name) VALUES (?)")
        .bind("New Artist")
        .fetch_optional()
        .await?;

    // This might fail (e.g., duplicate key)
    let result = tx
        .execute("INSERT INTO artists (name) VALUES (?)")
        .bind("New Artist") // Same name, might violate unique constraint
        .fetch_optional()
        .await;

    match result {
        Ok(_) => {
            tx.commit().await?;
            println!("Transaction committed successfully");
        },
        Err(e) => {
            tx.rollback().await?;
            println!("Transaction rolled back due to error: {}", e);
        }
    }

    Ok(())
}
```

### Schema Migrations

```rust
use moosicbox_database::{Migration, MigrationManager, Schema};

async fn setup_migrations(db: &Database) -> Result<(), Box<dyn std::error::Error>> {
    let migration_manager = MigrationManager::new(db);

    // Define migrations
    let migration_001 = Migration::new("001_initial_schema")
        .up(r#"
            CREATE TABLE artists (
                id BIGSERIAL PRIMARY KEY,
                name VARCHAR NOT NULL UNIQUE,
                created_at TIMESTAMP DEFAULT CURRENT_TIMESTAMP
            );

            CREATE TABLE albums (
                id BIGSERIAL PRIMARY KEY,
                title VARCHAR NOT NULL,
                artist_id BIGINT REFERENCES artists(id),
                release_date DATE,
                created_at TIMESTAMP DEFAULT CURRENT_TIMESTAMP
            );

            CREATE TABLE tracks (
                id BIGSERIAL PRIMARY KEY,
                title VARCHAR NOT NULL,
                artist_id BIGINT REFERENCES artists(id),
                album_id BIGINT REFERENCES albums(id),
                track_number INTEGER,
                duration INTEGER,
                file_path VARCHAR NOT NULL,
                created_at TIMESTAMP DEFAULT CURRENT_TIMESTAMP
            );

            CREATE INDEX idx_tracks_artist ON tracks(artist_id);
            CREATE INDEX idx_tracks_album ON tracks(album_id);
            CREATE INDEX idx_tracks_title ON tracks(title);
        "#)
        .down(r#"
            DROP TABLE tracks;
            DROP TABLE albums;
            DROP TABLE artists;
        "#);

    let migration_002 = Migration::new("002_add_playlists")
        .up(r#"
            CREATE TABLE playlists (
                id BIGSERIAL PRIMARY KEY,
                user_id BIGINT NOT NULL,
                name VARCHAR NOT NULL,
                description TEXT,
                is_public BOOLEAN DEFAULT FALSE,
                created_at TIMESTAMP DEFAULT CURRENT_TIMESTAMP,
                updated_at TIMESTAMP DEFAULT CURRENT_TIMESTAMP
            );

            CREATE TABLE playlist_tracks (
                id BIGSERIAL PRIMARY KEY,
                playlist_id BIGINT REFERENCES playlists(id) ON DELETE CASCADE,
                track_id BIGINT REFERENCES tracks(id) ON DELETE CASCADE,
                position INTEGER NOT NULL,
                added_at TIMESTAMP DEFAULT CURRENT_TIMESTAMP,
                UNIQUE(playlist_id, position)
            );

            CREATE INDEX idx_playlists_user ON playlists(user_id);
            CREATE INDEX idx_playlist_tracks_playlist ON playlist_tracks(playlist_id);
        "#)
        .down(r#"
            DROP TABLE playlist_tracks;
            DROP TABLE playlists;
        "#);

    // Register and run migrations
    migration_manager.register(migration_001);
    migration_manager.register(migration_002);

    // Apply all pending migrations
    migration_manager.migrate_up().await?;

    println!("All migrations applied successfully");

    Ok(())
}
```

### Advanced Querying

```rust
use moosicbox_database::{QueryBuilder, JoinType, Condition, OrderDirection};

async fn advanced_queries(db: &Database) -> Result<(), Box<dyn std::error::Error>> {
    // Complex join query
    let results = QueryBuilder::new()
        .select("t.title, a.name as artist, al.title as album")
        .from("tracks t")
        .join(JoinType::Inner, "artists a", "t.artist_id = a.id")
        .join(JoinType::Left, "albums al", "t.album_id = al.id")
        .where_condition(
            Condition::and(vec![
                Condition::eq("a.name", "The Beatles"),
                Condition::gt("t.duration", 180),
            ])
        )
        .order_by_direction("t.title", OrderDirection::Asc)
        .limit(20)
        .fetch_all::<Row>(db)
        .await?;

    // Subquery
    let popular_tracks = QueryBuilder::new()
        .select("*")
        .from("tracks")
        .where_in(
            "id",
            QueryBuilder::new()
                .select("track_id")
                .from("play_counts")
                .where_gt("count", 100)
        )
        .fetch_all::<Track>(db)
        .await?;

    // Aggregate query
    let artist_stats = QueryBuilder::new()
        .select("a.name, COUNT(t.id) as track_count, AVG(t.duration) as avg_duration")
        .from("artists a")
        .join(JoinType::Left, "tracks t", "a.id = t.artist_id")
        .group_by("a.id, a.name")
        .having_gt("COUNT(t.id)", 0)
        .order_by_direction("track_count", OrderDirection::Desc)
        .fetch_all::<Row>(db)
        .await?;

    println!("Found {} popular tracks", popular_tracks.len());

    Ok(())
}
```

### Database Models

```rust
use moosicbox_database::{Model, Repository, DatabaseError};
use async_trait::async_trait;

#[derive(Debug, Clone, sqlx::FromRow)]
pub struct Artist {
    pub id: i64,
    pub name: String,
    pub created_at: chrono::DateTime<chrono::Utc>,
}

#[derive(Debug)]
pub struct CreateArtist {
    pub name: String,
}

#[derive(Debug)]
pub struct UpdateArtist {
    pub name: Option<String>,
}

// Repository pattern implementation
pub struct ArtistRepository {
    db: Database,
}

impl ArtistRepository {
    pub fn new(db: Database) -> Self {
        Self { db }
    }

    pub async fn create(&self, create_artist: CreateArtist) -> Result<Artist, DatabaseError> {
        let artist = self.db
            .query_as("INSERT INTO artists (name) VALUES (?) RETURNING *")
            .bind(&create_artist.name)
            .fetch_one::<Artist>()
            .await?;

        Ok(artist)
    }

    pub async fn find_by_id(&self, id: i64) -> Result<Option<Artist>, DatabaseError> {
        let artist = self.db
            .query_as("SELECT * FROM artists WHERE id = ?")
            .bind(id)
            .fetch_optional::<Artist>()
            .await?;

        Ok(artist)
    }

    pub async fn find_by_name(&self, name: &str) -> Result<Option<Artist>, DatabaseError> {
        let artist = self.db
            .query_as("SELECT * FROM artists WHERE name = ?")
            .bind(name)
            .fetch_optional::<Artist>()
            .await?;

        Ok(artist)
    }

    pub async fn list(&self, limit: Option<i64>, offset: Option<i64>) -> Result<Vec<Artist>, DatabaseError> {
        let mut query = QueryBuilder::new()
            .select("*")
            .from("artists")
            .order_by("name");

        if let Some(limit) = limit {
            query = query.limit(limit);
        }

        if let Some(offset) = offset {
            query = query.offset(offset);
        }

        let artists = query.fetch_all::<Artist>(&self.db).await?;
        Ok(artists)
    }

    pub async fn update(&self, id: i64, update_artist: UpdateArtist) -> Result<Option<Artist>, DatabaseError> {
        if update_artist.name.is_none() {
            return self.find_by_id(id).await;
        }

        let artist = self.db
            .query_as("UPDATE artists SET name = COALESCE(?, name) WHERE id = ? RETURNING *")
            .bind(&update_artist.name)
            .bind(id)
            .fetch_optional::<Artist>()
            .await?;

        Ok(artist)
    }

    pub async fn delete(&self, id: i64) -> Result<bool, DatabaseError> {
        let result = self.db
            .execute("DELETE FROM artists WHERE id = ?")
            .bind(id)
            .await?;

        Ok(result.rows_affected() > 0)
    }

    pub async fn count(&self) -> Result<i64, DatabaseError> {
        let count = self.db
            .query_scalar("SELECT COUNT(*) FROM artists")
            .fetch_one::<i64>()
            .await?;

        Ok(count)
    }
}
```

## Configuration

### Environment Variables

| Variable | Description | Default |
|----------|-------------|---------|
| `DATABASE_URL` | Database connection string | Required |
| `DATABASE_MAX_CONNECTIONS` | Maximum connections in pool | `20` |
| `DATABASE_MIN_CONNECTIONS` | Minimum connections in pool | `5` |
| `DATABASE_CONNECTION_TIMEOUT` | Connection timeout in seconds | `30` |
| `DATABASE_IDLE_TIMEOUT` | Idle connection timeout | `600` |
| `DATABASE_MAX_LIFETIME` | Maximum connection lifetime | `1800` |
| `DATABASE_ENABLE_LOGGING` | Enable query logging | `false` |
| `DATABASE_LOG_SLOW_QUERIES` | Log slow queries above threshold | `true` |

### Database-Specific Configuration

```rust
use moosicbox_database::{DatabaseConfig, PostgreSQLConfig, SQLiteConfig, MySQLConfig};

// PostgreSQL configuration
let postgres_config = DatabaseConfig::PostgreSQL(PostgreSQLConfig {
    host: "localhost".to_string(),
    port: 5432,
    database: "moosicbox".to_string(),
    username: "postgres".to_string(),
    password: Some("password".to_string()),
    ssl_mode: SSLMode::Prefer,
    application_name: Some("MoosicBox".to_string()),
    connection_pool: PoolConfig {
        max_connections: 20,
        min_connections: 5,
        acquire_timeout_seconds: 10,
        idle_timeout_seconds: 600,
        max_lifetime_seconds: 1800,
        test_before_acquire: true,
        test_query: Some("SELECT 1".to_string()),
    },
});

// SQLite configuration
let sqlite_config = DatabaseConfig::SQLite(SQLiteConfig {
    database_path: "./music.db".to_string(),
    create_if_missing: true,
    journal_mode: JournalMode::WAL,
    synchronous: SynchronousMode::Normal,
    foreign_keys: true,
    busy_timeout_ms: 30000,
    cache_size: 64000, // 64MB cache
    temp_store: TempStore::Memory,
});

// MySQL configuration
let mysql_config = DatabaseConfig::MySQL(MySQLConfig {
    host: "localhost".to_string(),
    port: 3306,
    database: "moosicbox".to_string(),
    username: "root".to_string(),
    password: Some("password".to_string()),
    charset: "utf8mb4".to_string(),
    collation: "utf8mb4_unicode_ci".to_string(),
    timezone: "+00:00".to_string(),
    connection_pool: PoolConfig::default(),
});
```

## Feature Flags

- `database` - Core database functionality
- `database-sqlite` - SQLite backend support
- `database-postgres` - PostgreSQL backend support
- `database-mysql` - MySQL backend support
- `database-migrations` - Schema migration support
- `database-pool` - Connection pooling
- `database-json` - JSON column type support
- `database-uuid` - UUID type support
- `database-chrono` - Date/time type support

## Integration with MoosicBox

### Server Integration

```toml
[dependencies]
moosicbox-database = { path = "../database", features = ["database-postgres", "database-migrations"] }
```

```rust
use moosicbox_database::{Database, DatabaseConfig};
use moosicbox_server::Server;

async fn setup_server_with_database() -> Result<(), Box<dyn std::error::Error>> {
    // Initialize database
    let db_config = DatabaseConfig::from_env()?;
    let db = Database::new(db_config).await?;

    // Run migrations
    db.migrate().await?;

    // Create server with database
    let server = Server::new()
        .with_database(db.clone())
        .build()
        .await?;

    // Start server
    server.start().await?;

    Ok(())
}
```

### Repository Pattern

```rust
use moosicbox_database::{Database, Repository};

pub struct MusicRepository {
    artists: ArtistRepository,
    albums: AlbumRepository,
    tracks: TrackRepository,
    playlists: PlaylistRepository,
}

impl MusicRepository {
    pub fn new(db: Database) -> Self {
        Self {
            artists: ArtistRepository::new(db.clone()),
            albums: AlbumRepository::new(db.clone()),
            tracks: TrackRepository::new(db.clone()),
            playlists: PlaylistRepository::new(db),
        }
    }

    pub fn artists(&self) -> &ArtistRepository {
        &self.artists
    }

    pub fn albums(&self) -> &AlbumRepository {
        &self.albums
    }

    pub fn tracks(&self) -> &TrackRepository {
        &self.tracks
    }

    pub fn playlists(&self) -> &PlaylistRepository {
        &self.playlists
    }
}
```

## Performance Optimization

### Query Optimization

```rust
use moosicbox_database::{QueryAnalyzer, QueryPlan, IndexSuggestion};

async fn optimize_queries(db: &Database) -> Result<(), Box<dyn std::error::Error>> {
    let analyzer = QueryAnalyzer::new(db);

    // Analyze slow queries
    let slow_queries = analyzer.get_slow_queries(
        chrono::Duration::seconds(1), // Queries slower than 1 second
        100, // Limit to 100 results
    ).await?;

    for query in slow_queries {
        println!("Slow query: {} ({}ms)", query.sql, query.duration_ms);

        // Get query plan
        if let Ok(plan) = analyzer.explain_query(&query.sql).await {
            println!("Query plan: {:?}", plan);
        }

        // Get index suggestions
        if let Ok(suggestions) = analyzer.suggest_indexes(&query.sql).await {
            for suggestion in suggestions {
                println!("Index suggestion: CREATE INDEX {} ON {} ({})",
                         suggestion.name, suggestion.table, suggestion.columns.join(", "));
            }
        }
    }

    Ok(())
}
```

### Connection Pool Monitoring

```rust
use moosicbox_database::{PoolMetrics, ConnectionPool};

async fn monitor_connection_pool(db: &Database) -> Result<(), Box<dyn std::error::Error>> {
    let metrics = db.pool_metrics().await?;

    println!("Connection Pool Metrics:");
    println!("  Active connections: {}", metrics.active_connections);
    println!("  Idle connections: {}", metrics.idle_connections);
    println!("  Total connections: {}", metrics.total_connections);
    println!("  Pending acquisitions: {}", metrics.pending_acquisitions);
    println!("  Average acquire time: {}ms", metrics.avg_acquire_time_ms);

    // Alert if pool is under pressure
    if metrics.pending_acquisitions > 5 {
        println!("WARNING: High connection pool pressure detected");
    }

    if metrics.avg_acquire_time_ms > 100 {
        println!("WARNING: Slow connection acquisition times");
    }

    Ok(())
}
```

## Error Handling

```rust
use moosicbox_database::error::DatabaseError;

match db.query_as::<Track>("SELECT * FROM tracks WHERE id = ?").bind(track_id).fetch_one().await {
    Ok(track) => println!("Found track: {}", track.title),
    Err(DatabaseError::NotFound) => {
        println!("Track not found");
    },
    Err(DatabaseError::ConnectionError(e)) => {
        eprintln!("Database connection error: {}", e);
    },
    Err(DatabaseError::QueryError { query, error }) => {
        eprintln!("Query failed: {} - {}", query, error);
    },
    Err(DatabaseError::MigrationError { migration, error }) => {
        eprintln!("Migration {} failed: {}", migration, error);
    },
    Err(DatabaseError::PoolError(e)) => {
        eprintln!("Connection pool error: {}", e);
    },
    Err(e) => {
        eprintln!("Database error: {}", e);
    }
}
```

## Testing

### In-Memory Database for Tests

```rust
use moosicbox_database::{Database, DatabaseConfig, DatabaseType};

#[tokio::test]
async fn test_artist_repository() -> Result<(), Box<dyn std::error::Error>> {
    // Use in-memory database for testing
    let config = DatabaseConfig {
        database_type: DatabaseType::InMemory,
        connection_string: ":memory:".to_string(),
        ..Default::default()
    };

    let db = Database::new(config).await?;
    db.migrate().await?;

    let repo = ArtistRepository::new(db);

    // Test create artist
    let create_artist = CreateArtist {
        name: "Test Artist".to_string(),
    };

    let artist = repo.create(create_artist).await?;
    assert_eq!(artist.name, "Test Artist");

    // Test find by ID
    let found_artist = repo.find_by_id(artist.id).await?;
    assert!(found_artist.is_some());
    assert_eq!(found_artist.unwrap().name, "Test Artist");

    Ok(())
}
```

## See Also

- [MoosicBox Config](../config/README.md) - Database configuration management
- [MoosicBox Server](../server/README.md) - Server with database integration
- [MoosicBox Logging](../logging/README.md) - Database query logging
