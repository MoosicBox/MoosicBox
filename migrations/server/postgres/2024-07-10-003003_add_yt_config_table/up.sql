CREATE TABLE IF NOT EXISTS yt_config (
    "id" BIGSERIAL PRIMARY KEY NOT NULL,
    "access_token" VARCHAR(256) NOT NULL,
    "user_id" BIGINT NOT NULL,
    "user_email" VARCHAR(256) NOT NULL,
    "user_public_id" VARCHAR(256) NOT NULL,
    created TIMESTAMP NOT NULL DEFAULT NOW(),
    updated TIMESTAMP NOT NULL DEFAULT NOW()
);
