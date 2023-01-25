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
    name TEXT NOT NULL,
    comment TEXT NOT NULL,
    score INTEGER NOT NULL,

    manga_group INTEGER NOT NULL,
    FOREIGN KEY(manga_group) REFERENCES manga_groups(id)
);
