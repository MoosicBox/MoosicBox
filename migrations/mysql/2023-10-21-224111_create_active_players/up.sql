CREATE TABLE IF NOT EXISTS active_players (
    id INTEGER PRIMARY KEY NOT NULL AUTO_INCREMENT,
    session_id INTEGER NOT NULL,
    player_id INTEGER NOT NULL,
    created TEXT DEFAULT (DATE_FORMAT(NOW(), '%Y-%m-%dT%H:%M:%f')),
    updated TEXT DEFAULT (DATE_FORMAT(NOW(), '%Y-%m-%dT%H:%M:%f'))
);
