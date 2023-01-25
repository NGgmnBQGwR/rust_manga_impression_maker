#![warn(
    clippy::all,
    clippy::restriction,
    clippy::pedantic,
    clippy::nursery,
    clippy::cargo
)]
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")] // hide console window on Windows in release

use anyhow::Context;
use anyhow::Result as AnyResult;
use eframe::egui::{Color32, Vec2 as EguiVec2};
use sqlx::sqlite::{SqliteConnectOptions, SqlitePoolOptions};
use sqlx::Pool;
use std::collections::HashMap;
use std::path::{Path, PathBuf};

type SqlitePool = Pool<sqlx::sqlite::Sqlite>;

#[derive(Debug, Clone)]
struct MangaGroup {
    added_on: chrono::NaiveDateTime,
    id: i64,
}
impl MangaGroup {
    fn delete_cascade(&self, runtime: &tokio::runtime::Runtime, db: &SqlitePool) {
        let group_entries = runtime
            .block_on(
                sqlx::query_as!(
                    MangaEntry,
                    r"SELECT * FROM manga_entries WHERE manga_group = ?",
                    self.id
                )
                .fetch_all(db),
            )
            .unwrap();

        for entry in group_entries {
            entry.delete_cascade(runtime, db);
        }

        runtime
            .block_on(
                sqlx::query(r"DELETE FROM manga_groups WHERE id = ?")
                    .bind(self.id)
                    .execute(db),
            )
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
    fn delete_cascade(&self, runtime: &tokio::runtime::Runtime, db: &SqlitePool) {
        let manga_images = runtime
            .block_on(
                sqlx::query_as!(Image, r"SELECT * FROM images WHERE manga = ?", self.id)
                    .fetch_all(db),
            )
            .unwrap();

        for image in manga_images {
            image.delete_cascade(runtime, db);
        }

        runtime
            .block_on(
                sqlx::query(r"DELETE FROM manga_entries WHERE id = ?")
                    .bind(self.id)
                    .execute(db),
            )
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
    fn delete_cascade(&self, runtime: &tokio::runtime::Runtime, db: &SqlitePool) {
        // TODO: delete file

        runtime
            .block_on(
                sqlx::query(r"DELETE FROM images WHERE id = ?")
                    .bind(self.id)
                    .execute(db),
            )
            .unwrap();
    }
}

struct ImageCacheEntry {
    file_contents: Vec<u8>,
    thumbnail: Vec<u8>,
}

fn main() -> AnyResult<()> {
    dotenvy::dotenv().expect(".env file not found");

    let options = eframe::NativeOptions {
        initial_window_size: Some(EguiVec2::new(1200., 800.)),
        ..Default::default()
    };

    let runtime = tokio::runtime::Builder::new_multi_thread()
        .enable_all()
        .build()
        .context("Failed to create Tokio runtime.")
        .unwrap();

    let db_pool = runtime
        .block_on(MangaUI::init_db())
        .context("Failed to initialize DB pool.")
        .unwrap();

    let mut manga_ui = MangaUI {
        manga_groups: Vec::new(),
        selected_group: Option::None,
        group_to_delete: Option::None,
        images_cache: HashMap::with_capacity(100),
        cwd: std::env::current_dir()
            .context("Unable to get CWD.")
            .unwrap(),
        db_pool: db_pool,
        async_runtime: runtime,
    };

    manga_ui.refresh_manga_groups();

    eframe::run_native(
        "Manga impression maker",
        options,
        Box::new(|_cc| Box::new(manga_ui.setup(_cc))),
    );

    Ok(())
}
struct MangaUI {
    manga_groups: Vec<MangaGroup>,
    selected_group: Option<MangaGroup>,
    group_to_delete: Option<MangaGroup>,
    images_cache: HashMap<PathBuf, ImageCacheEntry>,
    cwd: PathBuf,
    db_pool: SqlitePool,
    async_runtime: tokio::runtime::Runtime,
}

impl eframe::App for MangaUI {
    fn update(&mut self, ctx: &egui::Context, frame: &mut eframe::Frame) {
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

                // FIXME: this variable should not be here, but otherwise I get errors like
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
                                                "Group #{} ({})",
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
        self.async_runtime
            .block_on(
                sqlx::query!(r"INSERT INTO manga_groups DEFAULT VALUES").execute(&self.db_pool),
            )
            .unwrap();
        self.refresh_manga_groups();
    }

    fn refresh_manga_groups(&mut self) {
        self.manga_groups = self
            .async_runtime
            .block_on(
                sqlx::query_as!(
                    MangaGroup,
                    r"SELECT * FROM manga_groups ORDER BY added_on DESC, id DESC"
                )
                .fetch_all(&self.db_pool),
            )
            .unwrap();

        dbg!(&self.manga_groups);
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

        self
    }

    fn confirm_delete_group(&mut self) {
        if self.group_to_delete.is_none() {
            return;
        }

        if self
            .selected_group
            .as_ref()
            .map_or(false, |x| x.id == self.group_to_delete.as_ref().unwrap().id)
        {
            self.selected_group = None;
        }

        let group = self.group_to_delete.as_mut().unwrap();
        group.delete_cascade(&self.async_runtime, &self.db_pool);
        self.group_to_delete = None;
        self.refresh_manga_groups();
    }
}
