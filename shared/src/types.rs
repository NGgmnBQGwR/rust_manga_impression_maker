pub const THUMBNAIL_IMAGE_WIDTH: u32 = 128;
pub const THUMBNAIL_IMAGE_HEIGHT: u32 = 72;

#[derive(Debug, Clone)]
pub struct MangaGroup {
    pub added_on: chrono::NaiveDateTime,
    pub id: i64,
}

#[derive(Debug, Clone)]
pub struct MangaEntry {
    pub name: String,
    pub score: i64,
    pub comment: String,
    pub manga_group: i64,
    pub id: i64,
}

#[derive(Debug, Clone)]
pub struct MangaImage {
    pub path: String,
    pub manga: i64,
    pub id: i64,
}

pub struct DisplayedMangaImage {
    pub image: MangaImage,
    pub thumbnail: egui::ImageData,
}

impl core::fmt::Debug for DisplayedMangaImage {
    fn fmt(&self, fmt: &mut core::fmt::Formatter) -> core::fmt::Result {
        fmt.debug_struct("DisplayedMangaImage")
            .field("image", &self.image)
            .finish()
    }
}

pub struct DisplayedMangaEntry {
    pub entry: MangaEntry,
    pub thumbnails: Vec<DisplayedMangaImage>,
    pub textures: Vec<egui::TextureHandle>,
}

impl core::fmt::Debug for DisplayedMangaEntry {
    fn fmt(&self, fmt: &mut core::fmt::Formatter) -> core::fmt::Result {
        fmt.debug_struct("DisplayedMangaEntry")
            .field("entry", &self.entry)
            .field("thumbnails", &self.thumbnails)
            .finish()
    }
}
