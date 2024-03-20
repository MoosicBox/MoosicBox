CREATE TABLE IF NOT EXISTS qobuz_config (
    "id" INTEGER PRIMARY KEY NOT NULL,
    "access_token" VARCHAR(256) NOT NULL,
    "user_id" INTEGER NOT NULL,
    "user_email" VARCHAR(256) NOT NULL,
    "user_public_id" VARCHAR(256) NOT NULL,
    created TIMESTAMP NOT NULL DEFAULT NOW(),
    updated TIMESTAMP NOT NULL DEFAULT NOW()
);
