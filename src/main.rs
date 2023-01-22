#![warn(
    clippy::all,
    clippy::restriction,
    clippy::pedantic,
    clippy::nursery,
    clippy::cargo
)]

use anyhow::Context;
use anyhow::Result as AnyResult;
use sqlx::sqlite::{SqliteConnectOptions, SqlitePoolOptions};
use sqlx::ConnectOptions;
use std::path::Path;

struct MangaGroup {
    name: String,
    id: i64,
}
struct MangaEntry {
    name: String,
    score: u8,
    comment: String,
    manga_group: i64,
    id: i64,
}
struct Image {
    path: String,
    manga: i64,
    id: i64,
}

const SQLITE_DATABASE_FILENAME: &str = "manga.sqlite3";

#[tokio::main]
async fn main() -> AnyResult<()> {
    let conn = SqliteConnectOptions::new()
        .create_if_missing(true)
        .filename(SQLITE_DATABASE_FILENAME);
    let pool = SqlitePoolOptions::new()
        .max_connections(2)
        .connect_with(conn)
        .await
        .context("Failed to connect to SQLite DB.")?;

    let migrator = sqlx::migrate::Migrator::new(Path::new("./migrations")).await?;
    migrator
        .run(&pool)
        .await
        .context("Error while running migrations.")?;

    // Make a simple query to return the given parameter (use a question mark `?` instead of `$1` for MySQL)
    let row: (i64,) = sqlx::query_as("SELECT $1")
        .bind(150_i64)
        .fetch_one(&pool)
        .await?;

    assert_eq!(row.0, 150);

    Ok(())
}
