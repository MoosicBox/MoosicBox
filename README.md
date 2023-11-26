# MoosicBox Server

A music server for cows

## Local Server

### Run

`cargo server 8001`

### Debug

`RUST_BACKTRACE=1 RUST_LOG="moosicbox=debug" cargo server:debug 8001`

### Deploy

`WS_HOST="wss://tunnel2.moosicbox.com/ws" TUNNEL_ACCESS_TOKEN='your access token here' ./aws-deploy.sh moosicbox_server moosicbox-server`

## Tunnel Server

### Run

`TUNNEL_ACCESS_TOKEN='your access token here' cargo tunnel-server 8005`

### Development

`TUNNEL_ACCESS_TOKEN='your access token here' RUST_BACKTRACE=1 RUST_LOG="moosicbox=debug" cargo tunnel-server:debug 8005`

### Deploy

`TUNNEL_ACCESS_TOKEN='your access token here' ./aws-deploy.sh moosicbox_tunnel_server moosicbox-tunnel-server`

## Database

### SQLite

The SQLite database stores the music library data:

- Artist metadata
- Album metadata
- Track metadata
- Local WebSocket connection metadata
- Audio Player configurations
- Playback Sessions

#### Migrations

##### Run

`diesel migration run --migration-dir migrations/sqlite --database-url library.db`

##### Revert

`diesel migration revert --migration-dir migrations/sqlite --database-url library.db`

##### New Migration

`diesel migration generate --migration-dir migrations/sqlite migration_name`

### MySQL

The MySQL database stores the tunnel server configurations:

- WebSocket connection mappings
  - Enables the tunnel server to know which WebSocket connection to tunnel data from

#### Migrations

##### Run

`diesel migration run --migration-dir migrations/mysql --database-url mysql://username:password@host/dbname`

##### Revert

`diesel migration revert --migration-dir migrations/mysql --database-url mysql://username:password@host/dbname`

##### New Migration

`diesel migration generate --migration-dir migrations/mysql migration_name`
