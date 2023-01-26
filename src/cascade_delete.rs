use crate::types::{MangaGroup, SqlitePool, MangaEntry, Image};
use async_trait::async_trait;

#[async_trait]
pub trait CascadeDelete {
    async fn delete_cascade(&self, db: &SqlitePool);
}

#[async_trait]
impl CascadeDelete for MangaGroup {
    async fn delete_cascade(&self, db: &SqlitePool) {
        let group_entries = sqlx::query_as!(
            MangaEntry,
            r"SELECT * FROM manga_entries WHERE manga_group = ?",
            self.id
        )
        .fetch_all(db)
        .await
        .unwrap();
        
        for entry in group_entries {
            entry.delete_cascade(db).await;
        }
        
        sqlx::query(r"DELETE FROM manga_groups WHERE id = ?")
        .bind(self.id)
        .execute(db)
        .await
        .unwrap();
    }
}

#[async_trait]
impl CascadeDelete for MangaEntry {
    async fn delete_cascade(&self, db: &SqlitePool) {
        let manga_images = sqlx::query_as!(Image, r"SELECT * FROM images WHERE manga = ?", self.id)
        .fetch_all(db)
        .await
        .unwrap();
        
        for image in manga_images {
            image.delete_cascade(db).await;
        }
        
        sqlx::query(r"DELETE FROM manga_entries WHERE id = ?")
        .bind(self.id)
        .execute(db)
        .await
        .unwrap();
    }
}

#[async_trait]
impl CascadeDelete for Image {
    async fn delete_cascade(&self, db: &SqlitePool) {
        // TODO: Delete file from the disk as well

        sqlx::query(r"DELETE FROM images WHERE id = ?")
            .bind(self.id)
            .execute(db)
            .await
            .unwrap();
    }
}