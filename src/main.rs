#![warn(
    clippy::all,
    clippy::restriction,
    clippy::pedantic,
    clippy::nursery,
    clippy::cargo
)]
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")] // Hide console window on Windows in release

use anyhow::Context;
use anyhow::Result as AnyResult;
use eframe::egui::{Color32, Vec2 as EguiVec2};
use sqlx::sqlite::{SqliteConnectOptions, SqlitePoolOptions};
use sqlx::Pool;
use std::collections::HashMap;
use std::path::{Path, PathBuf};

type SqlitePool = Pool<sqlx::sqlite::Sqlite>;
type GuiChannelSend = crossbeam::channel::Sender<GuiCommand>;
type GuiChannelRecv = crossbeam::channel::Receiver<GuiCommand>;
type BackendChannelSend = crossbeam::channel::Sender<BackendCommand>;
type BackendChannelRecv = crossbeam::channel::Receiver<BackendCommand>;

#[derive(Debug, Clone)]
struct MangaGroup {
    added_on: chrono::NaiveDateTime,
    id: i64,
}
impl MangaGroup {
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
#[derive(Debug)]
struct MangaEntry {
    name: String,
    score: i64,
    comment: String,
    manga_group: i64,
    id: i64,
}
impl MangaEntry {
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
#[derive(Debug)]
struct Image {
    path: String,
    manga: i64,
    id: i64,
}
impl Image {
    async fn delete_cascade(&self, db: &SqlitePool) {
        // TODO: Delete file from the disk as well

        sqlx::query(r"DELETE FROM images WHERE id = ?")
            .bind(self.id)
            .execute(db)
            .await
            .unwrap();
    }
}

struct ImageCacheEntry {
    file_contents: Vec<u8>,
    thumbnail: Vec<u8>,
}
#[derive(Debug)]
enum GuiCommand {
    UpdateMangaGroups,
    CreateNewMangaGroup,
    GetUpdatedMangaGroups,
    DeleteMangaGroup(MangaGroup),
    DeleteMangaEntry(MangaEntry),
    DeleteImage(Image),
    Exit,
}
#[derive(Debug)]
enum BackendCommand {
    UpdateGroups(Vec<MangaGroup>),
}

fn main() -> AnyResult<()> {
    dotenvy::dotenv().expect(".env file not found");

    let options = eframe::NativeOptions {
        initial_window_size: Some(EguiVec2::new(1200., 800.)),
        ..Default::default()
    };

    let (backend_send, backend_recv) = crossbeam::channel::bounded(100);
    let (gui_send, gui_recv) = crossbeam::channel::bounded(100);
    let backend_thread = std::thread::spawn(move || DataStorage::start(backend_send, gui_recv));

    let manga_ui = MangaUI {
        manga_groups: Vec::new(),
        selected_group: Option::None,
        group_to_delete: Option::None,
        backend_recv,
        gui_send,
    };

    eframe::run_native(
        "Manga impression maker",
        options,
        Box::new(|_cc| Box::new(manga_ui.setup(_cc))),
    );
    backend_thread.join().unwrap();

    Ok(())
}
struct MangaUI {
    manga_groups: Vec<MangaGroup>,
    selected_group: Option<MangaGroup>,
    group_to_delete: Option<MangaGroup>,
    backend_recv: BackendChannelRecv,
    gui_send: GuiChannelSend,
}
struct DataStorage {
    manga_groups: Vec<MangaGroup>,
    selected_group: Option<MangaGroup>,
    group_to_delete: Option<MangaGroup>,
    images_cache: HashMap<PathBuf, ImageCacheEntry>,
    cwd: PathBuf,
    db_pool: SqlitePool,
    backend_send: BackendChannelSend,
    gui_recv: GuiChannelRecv,
    exiting: bool,
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

    fn start(backend_send: BackendChannelSend, gui_recv: GuiChannelRecv) {
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
            cwd: std::env::current_dir()
                .context("Unable to get CWD.")
                .unwrap(),
            db_pool: db_pool,
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
}

impl eframe::App for MangaUI {
    fn on_close_event(&mut self) -> bool {
        self.gui_send.send(GuiCommand::Exit).unwrap();
        true
    }

    fn update(&mut self, ctx: &egui::Context, frame: &mut eframe::Frame) {
        self.process_backend_commands(ctx);

        egui::SidePanel::left("left_panel_manga_groups")
            .resizable(false)
            .exact_width(260.)
            .show(ctx, |ui| {
                ui.heading("Manga groups:");
                ui.separator();

                ui.horizontal(|ui| {
                    if ui.button("Refresh").clicked() {
                        self.refresh_manga_groups();
                    }
                    if ui.button("Add new group").clicked() {
                        self.create_new_manga_group();
                    }
                });
                ui.separator();

                if self.group_to_delete.is_some() {
                    let group = self.group_to_delete.clone().unwrap();
                    egui::Window::new(format!("Delete group #{} ({})", group.id, group.added_on))
                        .collapsible(false)
                        .resizable(false)
                        .show(ctx, |ui| {
                            ui.horizontal(|ui| {
                                if ui.button("Cancel").clicked() {
                                    self.group_to_delete = None;
                                }

                                if ui.button("Yes!").clicked() {
                                    self.confirm_delete_group();
                                }
                            });
                        });
                }

                // FIXME: This variable should not be here, but otherwise I get errors like
                // "cannot borrow mutably twice" or "cannot borrow immutable as mutable",
                // because we borrow '&self' for loop, then in the closure we need to borrow
                // '&mut self' for select_group() call.
                let mut new_selected_group: Option<MangaGroup> = None;
                egui::ScrollArea::vertical().show(ui, |ui| {
                    for group in self.manga_groups.iter() {
                        let (stroke, fill) = if self
                            .selected_group
                            .as_ref()
                            .map_or(false, |x| x.id == group.id)
                        {
                            (
                                (2f32, Color32::from_rgb(0xA0, 0x10, 0x10)),
                                Color32::LIGHT_GRAY,
                            )
                        } else {
                            ((2f32, Color32::from_rgb(0x10, 0x10, 0x10)), Color32::WHITE)
                        };

                        egui::Frame::none()
                            .inner_margin(5f32)
                            .outer_margin(EguiVec2::new(0f32, 2f32))
                            .stroke(stroke.into())
                            .fill(fill)
                            .rounding(5f32)
                            .show(ui, |ui| {
                                ui.horizontal(|ui| {
                                    let label = ui
                                        .add(
                                            egui::Label::new(format!(
                                                "Group #{:03} ({})",
                                                group.id, group.added_on
                                            ))
                                            .sense(egui::Sense::click()),
                                        )
                                        .on_hover_cursor(egui::CursorIcon::PointingHand);
                                    if label.clicked() {
                                        new_selected_group = Some((*group).clone());
                                    }

                                    let button = egui::Button::new("🗑").fill(Color32::LIGHT_RED);
                                    if ui.add(button).clicked() {
                                        self.group_to_delete = Some((*group).clone());
                                    }
                                })
                            });
                    }
                });

                if new_selected_group.is_some() {
                    self.select_group(new_selected_group.unwrap());
                }
            });

        egui::Window::new("🔧 Settings")
            .vscroll(true)
            .show(ctx, |ui| {
                ctx.settings_ui(ui);
            });
    }
}

impl MangaUI {
    fn create_new_manga_group(&mut self) {
        self.gui_send.send(GuiCommand::CreateNewMangaGroup).unwrap();
        self.gui_send.send(GuiCommand::UpdateMangaGroups).unwrap();
        self.gui_send
            .send(GuiCommand::GetUpdatedMangaGroups)
            .unwrap();
    }

    fn refresh_manga_groups(&mut self) {
        self.gui_send
            .send(GuiCommand::GetUpdatedMangaGroups)
            .unwrap();
    }

    fn select_group(&mut self, group: MangaGroup) {
        self.selected_group = Some(group);
        dbg!(&self.selected_group);
    }

    async fn init_db() -> AnyResult<Pool<sqlx::sqlite::Sqlite>> {
        // Initialize SQL connection
        let conn = SqliteConnectOptions::new()
            .create_if_missing(true)
            .filename(
                std::env::var("DATABASE_URL")
                    .unwrap()
                    .split('/')
                    .last()
                    .unwrap(),
            );

        let pool = SqlitePoolOptions::new()
            .max_connections(2)
            .connect_with(conn)
            .await
            .context("Failed to connect to SQLite DB.")?;

        // Run migrations, if necessary
        let migrator = sqlx::migrate::Migrator::new(Path::new("./migrations")).await?;
        migrator
            .run(&pool)
            .await
            .context("Error while running migrations.")?;

        Ok(pool)
    }

    fn setup(self, cc: &eframe::CreationContext) -> Self {
        let mut style: egui::Style = (*cc.egui_ctx.style()).clone();
        style
            .text_styles
            .get_mut(&egui::TextStyle::Body)
            .unwrap()
            .size = 14f32;
        cc.egui_ctx.set_style(style);
        cc.egui_ctx.set_visuals(egui::Visuals::light());

        let backend_recv_clone = self.backend_recv.clone();
        let ctx_clone = cc.egui_ctx.clone();
        // Since egui only calls update() when something has changed,
        // and we do message processing there, no messages will be processed if
        // there's no interaction from the user.
        // To counter this, we use a clone of receiver and every 16ms check if
        // there are messages from the backend, in a separate thread.
        // TODO: is it possible to replace "every 16ms" with "every frame"?
        std::thread::spawn(move || loop {
            std::thread::sleep(std::time::Duration::from_millis(16));
            if !backend_recv_clone.is_empty() {
                ctx_clone.request_repaint();
            }
        });

        self
    }

    fn confirm_delete_group(&mut self) {
        // Sanity check - we can't delete a group if no group was selected
        if self.group_to_delete.is_none() {
            return;
        }

        // Unselect current group if we're deleting it
        if self
            .selected_group
            .as_ref()
            .map_or(false, |x| x.id == self.group_to_delete.as_ref().unwrap().id)
        {
            self.selected_group = None;
        }

        self.gui_send
            .send(GuiCommand::DeleteMangaGroup(
                std::mem::replace(&mut self.group_to_delete, None).unwrap(),
            ))
            .unwrap();
        self.gui_send.send(GuiCommand::UpdateMangaGroups).unwrap();
        self.gui_send
            .send(GuiCommand::GetUpdatedMangaGroups)
            .unwrap();
    }

    fn process_backend_commands(&mut self, ctx: &egui::Context) {
        println!("CHECKING");
        while let Ok(cmd) = self.backend_recv.try_recv() {
            dbg!(&cmd);
            match cmd {
                BackendCommand::UpdateGroups(groups) => self.manga_groups = groups,
            }
            println!("REPAINT");
            ctx.request_repaint();
        }
    }
}
