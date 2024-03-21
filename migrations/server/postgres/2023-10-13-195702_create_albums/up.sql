CREATE TABLE IF NOT EXISTS albums (
    id BIGSERIAL PRIMARY KEY NOT NULL,
    artist_id BIGINT NOT NULL,
    title TEXT NOT NULL,
    date_released TEXT,
    date_added TIMESTAMP NOT NULL DEFAULT NOW(),
    artwork TEXT,
    directory TEXT,
    blur BIGINT NOT NULL DEFAULT 0,
    FOREIGN KEY (artist_id) REFERENCES artists(id)
)
