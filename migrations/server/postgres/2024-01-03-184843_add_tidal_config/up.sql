CREATE TABLE IF NOT EXISTS tidal_config (
    "id" INTEGER PRIMARY KEY NOT NULL,
    "access_token" TEXT NOT NULL,
    "refresh_token" TEXT NOT NULL,
    "client_name" VARCHAR(128) NOT NULL,
    "expires_in" INTEGER NOT NULL,
    "issued_at" INTEGER NOT NULL DEFAULT (EXTRACT(EPOCH FROM NOW()) * 1000),
    "scope" VARCHAR(128) NOT NULL,
    "token_type" VARCHAR(128) NOT NULL,
    "user" TEXT NOT NULL,
    "user_id" INTEGER NOT NULL,
    created TIMESTAMP NOT NULL DEFAULT NOW(),
    updated TIMESTAMP NOT NULL DEFAULT NOW()
);
