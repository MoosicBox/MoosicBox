CREATE TABLE IF NOT EXISTS tracks (
    id INTEGER PRIMARY KEY NOT NULL,
    album_id INTEGER NOT NULL,
    "number" INTEGER NOT NULL,
    title TEXT NOT NULL,
    duration REAL NOT NULL,
    "file" TEXT,
    FOREIGN KEY (album_id) REFERENCES albums(id)
)
