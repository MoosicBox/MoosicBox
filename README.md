# MoosicBox Server

A music server for cows

## Local Server

### Run

`cargo server 8001`

### Debug

`cargo server:debug 8001`

## Proxy Server

### Installation

`pnpm install`

### Deploy

`pnpm sst deploy --stage stage-name`

### Development

`pnpm sst dev`

## Database

### SQLite

The SQLite database stores the music library data:

-   Artist metadata
-   Album metadata
-   Track metadata
-   Local WebSocket connection metadata
-   Audio Player configurations
-   Playback Sessions

#### Migrations

##### Run

`diesel migration run --migration-dir migrations/sqlite --database-url library.db`

##### Revert

`diesel migration revert --migration-dir migrations/sqlite --database-url library.db`

##### New Migration

`diesel migration generate --migration-dir migrations/sqlite migration_name`

### MySQL

The MySQL database stores the proxy server configurations:

-   WebSocket connection mappings
    -   Enables the proxy server to know which WebSocket connection to forward proxy data from

#### Migrations

##### Run

`diesel migration run --migration-dir migrations/mysql --database-url mysql://username:password@host/dbname`

##### Revert

`diesel migration revert --migration-dir migrations/mysql --database-url mysql://username:password@host/dbname`

##### New Migration

`diesel migration generate --migration-dir migrations/mysql migration_name`
