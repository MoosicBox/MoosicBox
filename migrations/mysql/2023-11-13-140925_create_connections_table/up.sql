CREATE TABLE IF NOT EXISTS connections (
    client_id VARCHAR(64) PRIMARY KEY NOT NULL,
    tunnel_ws_id VARCHAR(64) NOT NULL,
    created TEXT DEFAULT (DATE_FORMAT(NOW(), '%Y-%m-%dT%H:%i:%f')),
    updated TEXT DEFAULT (DATE_FORMAT(NOW(), '%Y-%m-%dT%H:%i:%f'))
);
