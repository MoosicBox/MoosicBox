-- Create the join table for API sources
CREATE TABLE api_sources (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    entity_type VARCHAR(32) NOT NULL, -- 'tracks', 'albums', or 'artists'
    entity_id INTEGER NOT NULL,
    source TEXT NOT NULL,      -- 'Tidal' or 'Qobuz'
    source_id TEXT NOT NULL,
    UNIQUE(entity_type, entity_id, source)
);

-- Migrate tracks data
INSERT INTO api_sources (entity_type, entity_id, source, source_id)
SELECT 'tracks', id, 'Tidal', tidal_id
FROM tracks
WHERE tidal_id IS NOT NULL;

INSERT INTO api_sources (entity_type, entity_id, source, source_id)
SELECT 'tracks', id, 'Qobuz', qobuz_id
FROM tracks
WHERE qobuz_id IS NOT NULL;

-- Update the tracks unique index
DROP INDEX IF EXISTS ux_tracks_file;
CREATE UNIQUE INDEX ux_tracks_file ON tracks(
    ifnull(`file`, ''),
    `album_id`,
    `title`,
    `duration`,
    `number`,
    ifnull(`format`, ''),
    `source`
);

ALTER TABLE tracks DROP COLUMN tidal_id;
ALTER TABLE tracks DROP COLUMN qobuz_id;

-- Migrate albums data
INSERT INTO api_sources (entity_type, entity_id, source, source_id)
SELECT 'albums', id, 'Tidal', tidal_id
FROM albums
WHERE tidal_id IS NOT NULL;

INSERT INTO api_sources (entity_type, entity_id, source, source_id)
SELECT 'albums', id, 'Qobuz', qobuz_id
FROM albums
WHERE qobuz_id IS NOT NULL;

ALTER TABLE albums DROP COLUMN tidal_id;
ALTER TABLE albums DROP COLUMN qobuz_id;

-- Migrate artists data
INSERT INTO api_sources (entity_type, entity_id, source, source_id)
SELECT 'artists', id, 'Tidal', tidal_id
FROM artists
WHERE tidal_id IS NOT NULL;

INSERT INTO api_sources (entity_type, entity_id, source, source_id)
SELECT 'artists', id, 'Qobuz', qobuz_id
FROM artists
WHERE qobuz_id IS NOT NULL;

ALTER TABLE artists DROP COLUMN tidal_id;
ALTER TABLE artists DROP COLUMN qobuz_id;

-- Create indexes for better query performance
CREATE INDEX idx_api_sources_entity ON api_sources(entity_type, entity_id);
CREATE INDEX idx_api_sources_source ON api_sources(source, source_id);

UPDATE session_playlist_tracks SET data = replace(data, '"apiSource":"LIBRARY"', '"apiSource":"Library"');
UPDATE session_playlist_tracks SET data = replace(data, '"apiSource":"TIDAL"', '"apiSource":"Tidal"');
UPDATE session_playlist_tracks SET data = replace(data, '"apiSource":"QOBUZ"', '"apiSource":"Qobuz"');
UPDATE session_playlist_tracks SET data = replace(data, '"apiSource":"YT"', '"apiSource":"Yt"');
UPDATE session_playlist_tracks SET data = replace(data, '"trackSource":"TIDAL"', '"trackSource":"API:Tidal"');
UPDATE session_playlist_tracks SET data = replace(data, '"trackSource":"QOBUZ"', '"trackSource":"API:Qobuz"');
UPDATE session_playlist_tracks SET data = replace(data, '"trackSource":"YT"', '"trackSource":"API:Yt"');
UPDATE session_playlist_tracks SET type = 'Library' WHERE type = 'LIBRARY';
UPDATE session_playlist_tracks SET type = 'Qobuz' WHERE type = 'QOBUZ';
UPDATE session_playlist_tracks SET type = 'Tidal' WHERE type = 'TIDAL';
UPDATE session_playlist_tracks SET type = 'Yt' WHERE type = 'YT';

UPDATE tracks SET source = 'API:Qobuz' WHERE source = 'QOBUZ';
UPDATE tracks SET source = 'API:Tidal' WHERE source = 'TIDAL';

DELETE FROM download_tasks;
