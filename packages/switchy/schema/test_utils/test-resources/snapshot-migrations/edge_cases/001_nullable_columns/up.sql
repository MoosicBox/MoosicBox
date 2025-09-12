CREATE TABLE optional_data (
    id INTEGER PRIMARY KEY,
    required_field TEXT NOT NULL,
    optional_field TEXT,              -- Nullable column
    nullable_with_default TEXT DEFAULT 'default_value'
);