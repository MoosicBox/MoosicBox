# Server

## Run

`cargo server 8001`

## Debug

`cargo server:debug 8001`

# Database

## SQLite

### Migrations

#### Run

`diesel migration run --migration-dir migrations/sqlite --database-url library.db`

#### Revert

`diesel migration revert --migration-dir migrations/sqlite --database-url library.db`

#### New Migration

`diesel migration generate --migration-dir migrations/sqlite migration_name`

## MySQL

### Migrations

#### Run

`diesel migration run --migration-dir migrations/mysql --database-url mysql://username:password@host/dbname`

#### Revert

`diesel migration revert --migration-dir migrations/mysql --database-url mysql://username:password@host/dbname`

#### New Migration

`diesel migration generate --migration-dir migrations/mysql migration_name`
