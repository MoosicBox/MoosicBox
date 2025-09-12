CREATE INDEX idx_posts_user ON posts(user_id);
CREATE INDEX idx_posts_published ON posts(published);
CREATE INDEX idx_users_email ON users(email);