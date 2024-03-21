CREATE TABLE IF NOT EXISTS tracks (
    id BIGSERIAL PRIMARY KEY NOT NULL,
    album_id BIGINT NOT NULL,
    "number" BIGINT NOT NULL,
    title TEXT NOT NULL,
    duration REAL NOT NULL,
    "file" TEXT,
    FOREIGN KEY (album_id) REFERENCES albums(id)
)
