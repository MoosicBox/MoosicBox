CREATE TABLE IF NOT EXISTS qobuz_bundle_secrets (
    "id" INTEGER PRIMARY KEY NOT NULL,
    "qobuz_bundle_id" INTEGER NOT NULL,
    "timezone" VARCHAR(32) NOT NULL,
    "secret" VARCHAR(32) NOT NULL,
    created TIMESTAMP NOT NULL DEFAULT NOW(),
    updated TIMESTAMP NOT NULL DEFAULT NOW()
);
