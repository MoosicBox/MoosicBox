CREATE TABLE IF NOT EXISTS connections (
    tunnel_ws_id VARCHAR(128) PRIMARY KEY NOT NULL,
    client_id VARCHAR(128),
    created TEXT DEFAULT (DATE_FORMAT(NOW(), '%Y-%m-%dT%H:%M:%f')),
    updated TEXT DEFAULT (DATE_FORMAT(NOW(), '%Y-%m-%dT%H:%M:%f'))
);
