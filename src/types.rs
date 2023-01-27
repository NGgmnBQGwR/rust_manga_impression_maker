pub type SqlitePool = sqlx::Pool<sqlx::sqlite::Sqlite>;
pub type GuiChannelSend = crossbeam::channel::Sender<GuiCommand>;
pub type GuiChannelRecv = crossbeam::channel::Receiver<GuiCommand>;
pub type BackendChannelSend = crossbeam::channel::Sender<BackendCommand>;
pub type BackendChannelRecv = crossbeam::channel::Receiver<BackendCommand>;

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

impl std::fmt::Debug for DisplayedMangaImage {
    fn fmt(&self, fmt: &mut std::fmt::Formatter) -> std::fmt::Result {
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

impl std::fmt::Debug for DisplayedMangaEntry {
    fn fmt(&self, fmt: &mut std::fmt::Formatter) -> std::fmt::Result {
        fmt.debug_struct("DisplayedMangaEntry")
            .field("entry", &self.entry)
            .field("thumbnails", &self.thumbnails)
            .finish()
    }
}

#[derive(Debug)]
pub enum GuiCommand {
    UpdateMangaGroups,
    CreateNewMangaGroup,
    GetUpdatedMangaGroups,
    CreateNewMangaEntry(MangaGroup),
    DeleteMangaGroup(MangaGroup),
    DeleteMangaEntry(MangaEntry),
    DeleteImage(MangaImage),
    GetSelectedGroupInfo(MangaGroup),
    SaveMangaEntry(MangaEntry),
    SaveAllMangaEntries(Vec<MangaEntry>),
    Exit,
}

#[derive(Debug)]
pub enum BackendCommand {
    UpdateGroups(Vec<MangaGroup>),
    UpdateSelectedGroup(Vec<DisplayedMangaEntry>),
}
