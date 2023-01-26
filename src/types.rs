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

#[derive(Debug)]
pub struct MangaEntry {
    pub name: String,
    pub score: i64,
    pub comment: String,
    pub manga_group: i64,
    pub id: i64,
}

#[derive(Debug)]
pub struct Image {
    pub path: String,
    pub manga: i64,
    pub id: i64,
}

pub struct ImageCacheEntry {
    pub file_contents: Vec<u8>,
    pub thumbnail: Vec<u8>,
}
#[derive(Debug)]

pub enum GuiCommand {
    UpdateMangaGroups,
    CreateNewMangaGroup,
    GetUpdatedMangaGroups,
    DeleteMangaGroup(MangaGroup),
    DeleteMangaEntry(MangaEntry),
    DeleteImage(Image),
    Exit,
}

#[derive(Debug)]
pub enum BackendCommand {
    UpdateGroups(Vec<MangaGroup>),
}
