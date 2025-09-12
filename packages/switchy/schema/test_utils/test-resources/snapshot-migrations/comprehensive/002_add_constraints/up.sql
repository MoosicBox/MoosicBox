-- Add foreign key constraint (requires rebuilding table in SQLite)
CREATE TABLE posts_new (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    user_id INTEGER NOT NULL,
    title TEXT NOT NULL,
    content TEXT,
    published BOOLEAN DEFAULT FALSE,
    created_at TIMESTAMP DEFAULT CURRENT_TIMESTAMP,
    FOREIGN KEY (user_id) REFERENCES users(id)
);
INSERT INTO posts_new SELECT * FROM posts;
DROP TABLE posts;
ALTER TABLE posts_new RENAME TO posts;