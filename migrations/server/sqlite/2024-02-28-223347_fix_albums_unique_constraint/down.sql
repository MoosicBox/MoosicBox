DROP INDEX IF EXISTS ux_albums;
CREATE UNIQUE INDEX ux_albums ON albums(
    `artist_id`,
    `title`,
    ifnull(`directory`, ''),
    ifnull(`tidal_id`, 0),
    ifnull(`qobuz_id`, '')
);
