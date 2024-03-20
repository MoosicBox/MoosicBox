CREATE TABLE IF NOT EXISTS download_tasks (
    "id" INTEGER PRIMARY KEY NOT NULL,
    "state" VARCHAR(32) NOT NULL DEFAULT 'PENDING',
    "type" VARCHAR(32) NOT NULL,
    "track_id" INTEGER DEFAULT NULL,
    "album_id" INTEGER DEFAULT NULL,
    "source" VARCHAR(32) DEFAULT NULL,
    "quality" VARCHAR(32) DEFAULT NULL,
    "file_path" VARCHAR(1024) NOT NULL,
    "total_bytes" INTEGER DEFAULT NULL,
    created TIMESTAMP NOT NULL DEFAULT NOW(),
    updated TIMESTAMP NOT NULL DEFAULT NOW()
);

CREATE UNIQUE INDEX ux_download_tasks ON download_tasks(
    "type",
    coalesce("track_id", 0),
    coalesce("album_id", 0),
    coalesce("source", ''),
    coalesce("quality", ''),
    "file_path",
    coalesce("total_bytes", 0)
);
