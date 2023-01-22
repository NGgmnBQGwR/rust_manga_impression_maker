CREATE TABLE IF NOT EXISTS manga_groups (
    id BIGINT PRIMARY KEY,
    added_on TIMESTAMP NOT NULL DEFAULT CURRENT_TIMESTAMP
);

CREATE TABLE IF NOT EXISTS images (
    id BIGINT PRIMARY KEY,
    path TEXT NOT NULL,

    manga INTEGER,
    FOREIGN KEY(manga) REFERENCES manga_entries(id)
);

CREATE TABLE IF NOT EXISTS manga_entries (
    id BIGINT PRIMARY KEY,
    name TEXT NOT NULL,
    comment TEXT,
    score INTEGER,

    manga_group INTEGER,
    FOREIGN KEY(manga_group) REFERENCES manga_groups(id)
);
