use anyhow::Context;

use std::collections::HashMap;
use std::path::PathBuf;

use crate::cascade_delete::CascadeDelete;
use crate::manga_ui::MangaUI;
use crate::types::{
    BackendChannelSend, BackendCommand, DisplayedMangaEntry, DisplayedMangaImage, GuiChannelRecv,
    GuiCommand, Image, MangaEntry, MangaGroup, SqlitePool,
};

pub struct DataStorage {
    pub manga_groups: Vec<MangaGroup>,
    pub selected_group: Option<MangaGroup>,
    pub group_to_delete: Option<MangaGroup>,
    pub images_cache: HashMap<String, Vec<u8>>,
    pub thumbnails_cache: HashMap<String, egui::ImageData>,
    pub cwd: PathBuf,
    pub db_pool: SqlitePool,
    pub backend_send: BackendChannelSend,
    pub gui_recv: GuiChannelRecv,
    pub exiting: bool,
}

impl DataStorage {
    fn start_backend(self, runtime: tokio::runtime::Runtime) {
        runtime.block_on(self.run());
    }

    pub async fn run(mut self) {
        self.update_manga_groups().await;
        self.send_updated_manga_groups().await;

        loop {
            self.process_gui_commands().await;

            if self.exiting {
                break;
            }
        }
    }

    pub fn start(backend_send: BackendChannelSend, gui_recv: GuiChannelRecv) {
        let runtime = tokio::runtime::Builder::new_multi_thread()
            .enable_all()
            .build()
            .context("Failed to create Tokio runtime.")
            .unwrap();

        let db_pool = runtime
            .block_on(MangaUI::init_db())
            .context("Failed to initialize DB pool.")
            .unwrap();

        DataStorage {
            manga_groups: Vec::new(),
            selected_group: Option::None,
            group_to_delete: Option::None,
            images_cache: HashMap::with_capacity(100),
            thumbnails_cache: HashMap::with_capacity(100),
            cwd: std::env::current_dir()
                .context("Unable to get CWD.")
                .unwrap(),
            db_pool,
            backend_send,
            gui_recv,
            exiting: false,
        }
        .start_backend(runtime);
    }

    async fn process_gui_commands(&mut self) {
        while let Ok(cmd) = self
            .gui_recv
            .recv_timeout(std::time::Duration::from_millis(500))
        {
            match cmd {
                GuiCommand::UpdateMangaGroups => self.update_manga_groups().await,
                GuiCommand::CreateNewMangaGroup => self.create_new_manga_group().await,
                GuiCommand::GetUpdatedMangaGroups => self.send_updated_manga_groups().await,
                GuiCommand::DeleteMangaGroup(group) => group.delete_cascade(&self.db_pool).await,
                GuiCommand::DeleteMangaEntry(entry) => entry.delete_cascade(&self.db_pool).await,
                GuiCommand::DeleteImage(_) => todo!(),
                GuiCommand::CreateNewMangaEntry(group) => self.create_new_manga_entry(group).await,
                GuiCommand::GetSelectedGroupInfo(group) => {
                    self.prepare_and_send_selected_group(group).await
                }
                GuiCommand::Exit => {
                    self.exiting = true;
                    break;
                }
            }
        }
    }
    async fn send_updated_manga_groups(&self) {
        self.backend_send
            .send(BackendCommand::UpdateGroups(self.manga_groups.clone()))
            .unwrap();
    }

    async fn create_new_manga_entry(&mut self, group: MangaGroup) {
        sqlx::query!(
            r"INSERT INTO manga_entries(manga_group) VALUES(?)",
            group.id
        )
        .execute(&self.db_pool)
        .await
        .unwrap();

        self.prepare_and_send_selected_group(group).await;
    }

    async fn create_new_manga_group(&mut self) {
        sqlx::query!(r"INSERT INTO manga_groups DEFAULT VALUES")
            .execute(&self.db_pool)
            .await
            .unwrap();
        self.update_manga_groups().await;
    }

    async fn update_manga_groups(&mut self) {
        self.manga_groups = sqlx::query_as!(
            MangaGroup,
            r"SELECT * FROM manga_groups ORDER BY added_on DESC, id DESC"
        )
        .fetch_all(&self.db_pool)
        .await
        .unwrap();

        dbg!(&self.manga_groups);
    }

    async fn prepare_and_send_selected_group(&mut self, group: MangaGroup) {
        let mut result = Vec::<DisplayedMangaEntry>::with_capacity(50);

        let group_entries = sqlx::query_as!(
            MangaEntry,
            r"SELECT * FROM manga_entries WHERE manga_group = ? ORDER BY id DESC",
            group.id
        )
        .fetch_all(&self.db_pool)
        .await
        .unwrap();

        for entry in group_entries.into_iter() {
            let manga_images = sqlx::query_as!(
                Image,
                r"SELECT * FROM images WHERE manga = ? ORDER BY id DESC",
                entry.id
            )
            .fetch_all(&self.db_pool)
            .await
            .unwrap();

            let loaded_images = self.cache_and_prepare_images(&manga_images);
            result.push(DisplayedMangaEntry {
                entry: entry,
                thumbnails: loaded_images,
            });
        }

        self.backend_send
            .send(BackendCommand::UpdateSelectedGroup(result))
            .unwrap();
    }

    fn cache_and_prepare_images(&mut self, images: &[Image]) -> Vec<DisplayedMangaImage> {
        let mut result = Vec::<DisplayedMangaImage>::with_capacity(images.len());
        for image in images {
            if let Ok(file_contents) = std::fs::read(self.cwd.join(&image.path)) {
                let loaded_image = image::load_from_memory(&file_contents).unwrap();
                // let size = [loaded_image.width() as _, loaded_image.height() as _];
                let size = [64, 64];
                let image_buffer = loaded_image.to_rgba8();
                let pixels = image_buffer.as_flat_samples();
                // Ok(egui::ColorImage::from_rgba_unmultiplied(size, pixels.as_slice()))

                // ui.ctx().load_texture(image, data, Default::default())
                // });
                self.images_cache
                    .entry(image.path.clone())
                    .or_insert(file_contents);
                self.thumbnails_cache.entry(image.path.clone()).or_insert(
                    egui::ColorImage::from_rgba_unmultiplied(size, pixels.as_slice()).into(),
                );
            } else {
                todo!()
                // let default_image = egui::ColorImage::new([32, 32], egui::Color32::from_rgb(255, 0, 0));
                // self.thumbnails_cache.entry(image.path.clone()).or_insert_with(|| {
                // })
            }
            result.push(DisplayedMangaImage {
                image: (*image).clone(),
                thumbnail: self.thumbnails_cache.get(&image.path).unwrap().clone(),
            })
        }
        result
    }
}
