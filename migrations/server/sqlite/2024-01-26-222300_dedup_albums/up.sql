UPDATE OR IGNORE tracks SET album_id=target.keep_id
FROM (
  SELECT tracks.id, del_albums.id AS del_id, keep_albums.id AS keep_id
  FROM tracks
  INNER JOIN
    (SELECT albums.* FROM albums
    INNER JOIN (SELECT title, artist_id, count(*) AS col_count from albums group by title, artist_id) AS cnt ON cnt.title = albums.title AND cnt.artist_id = albums.artist_id
    INNER JOIN (SELECT title, artist_id, min(rowid) AS min_row_id from albums group by title, artist_id) AS min_row ON min_row.title = albums.title AND min_row.artist_id = albums.artist_id
    WHERE cnt.col_count > 1 AND albums.rowid != min_row.min_row_id) del_albums ON del_albums.id = tracks.album_id
  INNER JOIN
    (SELECT albums.* FROM albums
    INNER JOIN (SELECT title, artist_id, count(*) AS col_count from albums group by title, artist_id) AS cnt ON cnt.title = albums.title AND cnt.artist_id = albums.artist_id
    INNER JOIN (SELECT title, artist_id, min(rowid) AS min_row_id from albums group by title, artist_id) AS min_row ON min_row.title = albums.title AND min_row.artist_id = albums.artist_id
    WHERE cnt.col_count > 1 AND albums.rowid = min_row.min_row_id) keep_albums ON keep_albums.title = del_albums.title AND keep_albums.artist_id=del_albums.artist_id) AS target
WHERE tracks.id = target.id;

DELETE FROM tracks
WHERE id in (
    SELECT tracks.id FROM tracks
    INNER JOIN albums on tracks.album_id = albums.id
    INNER JOIN (SELECT title, artist_id, count(*) AS col_count from albums group by title, artist_id) AS cnt ON cnt.title = albums.title AND cnt.artist_id = albums.artist_id
    INNER JOIN (SELECT title, artist_id, min(rowid) AS min_row_id from albums group by title, artist_id) AS min_row ON min_row.title = albums.title AND min_row.artist_id = albums.artist_id
    WHERE cnt.col_count > 1 AND albums.rowid != min_row.min_row_id);

DELETE FROM albums
  WHERE id in (SELECT albums.id FROM albums
    INNER JOIN (SELECT title, artist_id, count(*) AS col_count from albums group by title, artist_id) AS cnt ON cnt.title = albums.title AND cnt.artist_id = albums.artist_id
    INNER JOIN (SELECT title, artist_id, min(rowid) AS min_row_id from albums group by title, artist_id) AS min_row ON min_row.title = albums.title AND min_row.artist_id = albums.artist_id
    WHERE cnt.col_count > 1 AND albums.rowid != min_row.min_row_id);
