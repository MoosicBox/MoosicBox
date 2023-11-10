CREATE TABLE IF NOT EXISTS active_players (
    id INTEGER PRIMARY KEY NOT NULL,
    session_id INTEGER NOT NULL,
    player_id INTEGER NOT NULL,
    created TEXT DEFAULT (strftime('%Y-%m-%dT%H:%M:%f', 'now')),
    updated TEXT DEFAULT (strftime('%Y-%m-%dT%H:%M:%f', 'now')),
    FOREIGN KEY (player_id) REFERENCES players(id),
    FOREIGN KEY (session_id) REFERENCES sessions(id)
);
