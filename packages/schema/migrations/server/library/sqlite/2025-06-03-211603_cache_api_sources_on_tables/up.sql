ALTER TABLE tracks ADD COLUMN api_sources VARCHAR(256) NOT NULL DEFAULT '[]';
ALTER TABLE albums ADD COLUMN api_sources VARCHAR(256) NOT NULL DEFAULT '[]';
ALTER TABLE artists ADD COLUMN api_sources VARCHAR(256) NOT NULL DEFAULT '[]';

UPDATE tracks SET api_sources =
    (
        SELECT json_group_array(
            json_object(
               'id', api_sources.source_id,
               'source', api_sources.source
            )
        )
        FROM api_sources
        WHERE api_sources.entity_type='tracks' AND api_sources.entity_id = tracks.id
    );

UPDATE albums SET api_sources =
    (
        SELECT json_group_array(
            json_object(
               'id', api_sources.source_id,
               'source', api_sources.source
            )
        )
        FROM api_sources
        WHERE api_sources.entity_type='albums' AND api_sources.entity_id = albums.id
    );

UPDATE artists SET api_sources =
    (
        SELECT json_group_array(
            json_object(
               'id', api_sources.source_id,
               'source', api_sources.source
            )
        )
        FROM api_sources
        WHERE api_sources.entity_type='artists' AND api_sources.entity_id = artists.id
    );
