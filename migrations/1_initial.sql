CREATE TABLE IF NOT EXISTS manga_groups (
    id INTEGER PRIMARY KEY NOT NULL,
    added_on TIMESTAMP NOT NULL DEFAULT CURRENT_TIMESTAMP
);

CREATE TABLE IF NOT EXISTS images (
    id INTEGER PRIMARY KEY NOT NULL,
    path TEXT NOT NULL,

    manga INTEGER NOT NULL,
    FOREIGN KEY(manga) REFERENCES manga_entries(id)
);

CREATE TABLE IF NOT EXISTS manga_entries (
    id INTEGER PRIMARY KEY NOT NULL,
    name TEXT NOT NULL DEFAULT '',
    comment TEXT NOT NULL DEFAULT '',
    score INTEGER NOT NULL DEFAULT 0,

    manga_group INTEGER NOT NULL,
    FOREIGN KEY(manga_group) REFERENCES manga_groups(id)
);
