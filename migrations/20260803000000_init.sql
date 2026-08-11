CREATE TABLE settings (
    key TEXT PRIMARY KEY,
    value TEXT NOT NULL
);

CREATE TABLE session_store (
    id TEXT PRIMARY KEY,
    data TEXT NOT NULL,
    expiry_date TEXT NOT NULL
);
CREATE INDEX idx_session_store_expiry ON session_store(expiry_date);
