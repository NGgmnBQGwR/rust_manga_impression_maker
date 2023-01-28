use anyhow::Context;

use std::collections::HashMap;
use std::path::PathBuf;

use crate::cascade_delete::CascadeDelete;
use crate::manga_ui::MangaUI;
use crate::types::{
    BackendChannelSend, BackendCommand, DisplayedMangaEntry, DisplayedMangaImage, GuiChannelRecv,
    GuiCommand, MangaEntry, MangaGroup, MangaImage, SqlitePool, THUMBNAIL_IMAGE_HEIGHT,
    THUMBNAIL_IMAGE_WIDTH,
};

struct ImageCache {
    pub images_cache: HashMap<i64, Vec<u8>>,
    pub thumbnails_cache: HashMap<i64, egui::ImageData>,
    pub cwd: PathBuf,
}

impl ImageCache {
    // TODO: replace cloning Vec with &mut, if it's possible
    fn get_image(&mut self, image: &MangaImage) -> Vec<u8> {
        self.images_cache
            .entry(image.id)
            .or_insert_with(|| std::fs::read(self.cwd.join(&image.path)).unwrap())
            .clone()
    }

    fn get_thumbnail(&mut self, image: &MangaImage) -> egui::ImageData {
        let file_contents = self.get_image(image);

        self.thumbnails_cache
            .entry(image.id)
            .or_insert_with(|| {
                let original_image = image::load_from_memory(&file_contents).unwrap();
                let resized_image = original_image.resize(
                    THUMBNAIL_IMAGE_WIDTH,
                    THUMBNAIL_IMAGE_HEIGHT,
                    image::imageops::FilterType::Lanczos3,
                );
                let image_buffer = resized_image.to_rgba8();

                egui::ColorImage::from_rgba_unmultiplied(
                    [
                        usize::try_from(resized_image.width()).unwrap(),
                        usize::try_from(resized_image.height()).unwrap(),
                    ],
                    image_buffer.as_flat_samples().as_slice(),
                )
                .into()
            })
            .clone()
    }

    fn get_image_data(&mut self, image: &MangaImage) -> DisplayedMangaImage {
        DisplayedMangaImage {
            image: image.clone(),
            thumbnail: self.get_thumbnail(image),
        }
    }
}

pub struct DataStorage {
    pub manga_groups: Vec<MangaGroup>,
    pub selected_group: Option<MangaGroup>,
    pub group_to_delete: Option<MangaGroup>,
    pub cwd: PathBuf,
    image_cache: ImageCache,
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

        let cwd = std::env::current_dir()
            .context("Unable to get CWD.")
            .unwrap();

        DataStorage {
            manga_groups: Vec::new(),
            selected_group: Option::None,
            group_to_delete: Option::None,
            cwd: cwd.clone(),
            db_pool,
            backend_send,
            gui_recv,
            exiting: false,
            image_cache: ImageCache {
                images_cache: HashMap::with_capacity(100),
                thumbnails_cache: HashMap::with_capacity(100),
                cwd: cwd.clone(),
            },
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
                GuiCommand::DeleteImage(image) => {
                    image.delete_cascade(&self.db_pool).await;
                    self.send_manga_entry_images(image.manga).await;
                }
                GuiCommand::CreateNewMangaEntry(group) => self.create_new_manga_entry(group).await,
                GuiCommand::GetSelectedGroupInfo(group) => self.send_selected_group(group).await,
                GuiCommand::Exit => {
                    self.exiting = true;
                    break;
                }
                GuiCommand::SaveMangaEntry(entry) => self.save_manga_entry(entry).await,
                GuiCommand::SaveAllMangaEntries(entries) => {
                    // TODO: should this be rewritten using futures/JoinSet, since this is probably not very performant?
                    for entry in entries.into_iter() {
                        self.save_manga_entry(entry).await
                    }
                }
                GuiCommand::AddImageFromDisk(entry) => self.add_image_from_disk(entry).await,
                GuiCommand::AddImageFromClipboard(entry) => {
                    self.add_image_from_clipboard(entry).await
                }
                GuiCommand::UpdateEntryImages(entry) => {
                    self.send_manga_entry_images(entry.id).await
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

        self.send_selected_group(group).await;
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

    async fn send_selected_group(&mut self, group: MangaGroup) {
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
                MangaImage,
                r"SELECT * FROM manga_images WHERE manga = ? ORDER BY id DESC",
                entry.id
            )
            .fetch_all(&self.db_pool)
            .await
            .unwrap();

            result.push(DisplayedMangaEntry {
                entry: entry,
                thumbnails: manga_images
                    .iter()
                    .map(|manga_image| self.image_cache.get_image_data(manga_image))
                    .collect(),
                textures: vec![],
            });
        }

        self.backend_send
            .send(BackendCommand::UpdateSelectedGroup(result))
            .unwrap();
    }

    async fn save_manga_entry(&self, entry: MangaEntry) {
        sqlx::query_as!(
            MangaImage,
            r"UPDATE manga_entries SET name = ?, comment = ?, score = ? WHERE id = ?",
            entry.name,
            entry.comment,
            entry.score,
            entry.id
        )
        .execute(&self.db_pool)
        .await
        .unwrap();
    }

    async fn add_image_shared(&mut self, entry: MangaEntry, image_file: image::DynamicImage) {
        // TODO: find a way to avoid making this query just to get group id
        let manga_group = sqlx::query!(
            r"SELECT manga_group FROM manga_entries WHERE manga_entries.id = ? LIMIT 1",
            entry.id
        )
        .fetch_one(&self.db_pool)
        .await
        .unwrap()
        .manga_group;

        let relative_image_path = {
            let relative_folder_path = format!("media/{}", manga_group.to_string());
            let full_folder_path = self.cwd.join(&relative_folder_path);
            if !full_folder_path.exists() {
                std::fs::create_dir_all(full_folder_path).unwrap();
            }

            format!(
                "{}/{}.jpg",
                relative_folder_path,
                uuid::Uuid::new_v4().to_string()
            )
        };
        let full_image_path = self.cwd.join(&relative_image_path);

        let new_file =
            &mut std::io::BufWriter::new(std::fs::File::create(&full_image_path).unwrap());
        let mut encoder = image::codecs::jpeg::JpegEncoder::new_with_quality(new_file, 95);

        encoder
            .encode(
                &image_file.to_rgb8(),
                image_file.width(),
                image_file.height(),
                image::ColorType::Rgb8,
            )
            .unwrap();

        sqlx::query!(
            r"INSERT INTO manga_images(path, manga) VALUES(?, ?)",
            relative_image_path,
            entry.id,
        )
        .execute(&self.db_pool)
        .await
        .unwrap();
    }

    async fn add_image_from_disk(&mut self, entry: MangaEntry) {
        let image_file_path = rfd::FileDialog::new()
            .set_title("Select image")
            .set_directory(&self.cwd)
            .add_filter("Images", &["jpg", "jpeg", "png"])
            .pick_file();
        if image_file_path.is_none() {
            return;
        }

        let file_contents = std::fs::read(image_file_path.unwrap()).unwrap();
        let loaded_image = image::load_from_memory(&file_contents).unwrap();

        self.add_image_shared(entry, loaded_image).await;
    }

    async fn add_image_from_clipboard(&mut self, entry: MangaEntry) {
        let mut clipboard = arboard::Clipboard::new().unwrap();
        let image = clipboard.get_image();
        if image.is_err() {
            return;
        }
        let image = image.unwrap();

        let buffer = image::ImageBuffer::from_raw(
            image.width as u32,
            image.height as u32,
            image.bytes.into_owned(),
        )
        .unwrap();
        let loaded_image = image::DynamicImage::ImageRgba8(buffer);
        self.add_image_shared(entry, loaded_image).await;
    }

    async fn send_manga_entry_images(&mut self, entry_id: i64) {
        let manga_images = sqlx::query_as!(
            MangaImage,
            r"SELECT * FROM manga_images WHERE manga = ? ORDER BY id DESC",
            entry_id
        )
        .fetch_all(&self.db_pool)
        .await
        .unwrap();

        let image_data = manga_images
            .iter()
            .map(|image| self.image_cache.get_image_data(image))
            .collect();

        self.backend_send
            .send(BackendCommand::UpdateThumbnailsForMangaEntry((
                entry_id, image_data,
            )))
            .unwrap();
    }
}
