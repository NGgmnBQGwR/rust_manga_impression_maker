use shared::types::{DisplayedMangaEntry, DisplayedMangaImage, MangaEntry, MangaGroup, MangaImage};

#[derive(Debug)]
// TODO: trim down parameters from struct to a single id?
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
    AddImagesFromDisk(MangaEntry),
    UpdateEntryImages(MangaEntry),
    AddImageFromClipboard(MangaEntry),
    ExportGroup(MangaGroup),
    AddNamesFromFolder(MangaGroup),
    Exit,
}

#[derive(Debug)]
pub enum BackendCommand {
    UpdateGroups(Vec<MangaGroup>),
    UpdateSelectedGroup(Vec<DisplayedMangaEntry>),
    UpdateThumbnailsForMangaEntry((i64, Vec<DisplayedMangaImage>)),
}

pub type SqlitePool = sqlx::Pool<sqlx::sqlite::Sqlite>;
pub type GuiChannelSend = crossbeam::channel::Sender<GuiCommand>;
pub type GuiChannelRecv = crossbeam::channel::Receiver<GuiCommand>;
pub type BackendChannelSend = crossbeam::channel::Sender<BackendCommand>;
pub type BackendChannelRecv = crossbeam::channel::Receiver<BackendCommand>;
