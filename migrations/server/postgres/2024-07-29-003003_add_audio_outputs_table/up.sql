CREATE TABLE IF NOT EXISTS audio_outputs (
    "id" VARCHAR(128) PRIMARY KEY NOT NULL,
    "name" VARCHAR(256) NOT NULL,
    "spec_rate" BIGINT NOT NULL,
    "spec_channels" BIGINT NOT NULL,
    "created" TIMESTAMP NOT NULL DEFAULT NOW(),
    "updated" TIMESTAMP NOT NULL DEFAULT NOW()
);
