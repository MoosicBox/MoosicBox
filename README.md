# MoosicBox Server

A music server for cows

![MoosicBox](https://github.com/MoosicBox/Files/blob/master/animation.gif?raw=true)

## Features

Implemented:

- Audio playback controls
  - Next/previous track, seek track, queue tracks, adjust volume, etc
- Control playback across applications (web and desktop)
  - Supports multi simultaneous audio outputs
- Audio encoding on the fly
  - AAC (m4a, mp4), mp3, Opus
- Hi-Fi audio player
- Automatic image optimization for requested size on demand
- Tunnel server reverse proxy - allows access to local server from internet without any firewall configuration
- Tidal and Qobuz integration
- No internet connection required, ever.
- Global search functionality
- Postgres, MySQL, and SQLite database support
- Android app
- Audio file visualization on seek bar

In progress:

- UPnP/DLNA support
- Opus decoder
- YouTube Music integration

To-do (in priority order):

### Beta

- Better session & audio zone handling
- Introductory screen to walk through initial setup
- Pre-load next playback track
- Save tracks hosted on server locally on clients
  - Source quality and/or encoded lossy
- iOS app

### v1.0

- Stream audio file visualization as it processes
- Control streaming audio quality
- Auto scan file changes (optionally)
- Image optimization cache
- Audio encoding cache
- Run as a service in the background, optionally at startup
- Spotify integration
- Enable switching between different bitrates within encodings
- Enable on the fly switching audio quality during playback
- Current playback screen
- Support gapless playback
- Playlists
- Shuffle playback

### Beyond v1.0

- Peer-to-peer secure tunneling
- Turso database support
- End-to-end encryption option
- Listen-only connections
- Schedule playback
- Shareable playlists via an authenticated link
- Support HLS protocol
- Support MPEG-DASH encoding

## Local Server

### Dependencies

- pkg-config (optional for OPUS)
- libtool (optional for OPUS)
- [vips](https://www.libvips.org/install.html) (optional for libvips image optimization)

### Run

`cargo server 8001`

### Debug

`RUST_BACKTRACE=1 RUST_LOG="moosicbox=debug" cargo server:debug 8001`

### Deploy

`WS_HOST="wss://tunnel2.moosicbox.com/ws" TUNNEL_ACCESS_TOKEN='your access token here' STATIC_TOKEN='your static token here' ./do-deploy.sh moosicbox-tunnel-server`

## Tunnel Server

### Run

`TUNNEL_ACCESS_TOKEN='your access token here' cargo tunnel-server 8005`

### Development

`TUNNEL_ACCESS_TOKEN='your access token here' RUST_BACKTRACE=1 RUST_LOG="moosicbox=debug" cargo tunnel-server:debug 8005`

### Deploy

`TUNNEL_ACCESS_TOKEN='your access token here' AWS_ACCESS_KEY_ID='token here' AWS_SECRET_ACCESS_KEY='key here' ./do-deploy.sh moosicbox-tunnel-server`

## Database

### Server

The SQLite database stores the music library data:

- Artist metadata
- Album metadata
- Track metadata
- Local WebSocket connection metadata
- Audio Player configurations
- Playback Sessions

#### Migrations

##### SQLite

###### Run

`diesel migration run --migration-dir migrations/server/sqlite --database-url library.db`

###### Revert

`diesel migration revert --migration-dir migrations/server/sqlite --database-url library.db`

###### New Migration

`diesel migration generate --migration-dir migrations/server/sqlite migration_name`

##### Postgres

###### Run

`diesel migration run --migration-dir migrations/server/postgres --database-url postgres://username:password@host/dbname`

###### Revert

`diesel migration revert --migration-dir migrations/server/postgres --database-url postgres://username:password@host/dbname`

###### New Migration

`diesel migration generate --migration-dir migrations/server/postgres migration_name`

### Tunnel

#### Postgres

The Postgres database stores the tunnel server configurations:

- WebSocket connection mappings
  - Enables the tunnel server to know which WebSocket connection to tunnel data from

##### Migrations

###### Run

`diesel migration run --migration-dir migrations/tunnel/postgres --database-url postgres://username:password@host/dbname`

###### Revert

`diesel migration revert --migration-dir migrations/tunnel/postgres --database-url postgres://username:password@host/dbname`

###### New Migration

`diesel migration generate --migration-dir migrations/tunnel/postgres migration_name`

#### MySQL

The MySQL database stores the tunnel server configurations:

- WebSocket connection mappings
  - Enables the tunnel server to know which WebSocket connection to tunnel data from

##### Migrations

###### Run

`diesel migration run --migration-dir migrations/tunnel/mysql --database-url mysql://username:password@host/dbname`

###### Revert

`diesel migration revert --migration-dir migrations/tunnel/mysql --database-url mysql://username:password@host/dbname`

###### New Migration

`diesel migration generate --migration-dir migrations/tunnel/mysql migration_name`
