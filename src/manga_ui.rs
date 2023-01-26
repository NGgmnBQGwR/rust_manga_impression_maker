use anyhow::Context;
use anyhow::Result as AnyResult;
use eframe::egui::{Color32, Vec2 as EguiVec2};

use crate::types::{MangaGroup, GuiCommand, BackendCommand, SqlitePool, BackendChannelRecv, GuiChannelSend};

pub struct MangaUI {
    pub manga_groups: Vec<MangaGroup>,
    pub selected_group: Option<MangaGroup>,
    pub group_to_delete: Option<MangaGroup>,
    pub backend_recv: BackendChannelRecv,
    pub gui_send: GuiChannelSend,
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
                                (2., Color32::from_rgb(0xA0, 0x10, 0x10)),
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

    pub async fn init_db() -> AnyResult<SqlitePool> {
        // Initialize SQL connection
        let conn = sqlx::sqlite::SqliteConnectOptions::new()
            .create_if_missing(true)
            .filename(
                std::env::var("DATABASE_URL")
                    .unwrap()
                    .split('/')
                    .last()
                    .unwrap(),
            );

        let pool = sqlx::sqlite::SqlitePoolOptions::new()
            .max_connections(2)
            .connect_with(conn)
            .await
            .context("Failed to connect to SQLite DB.")?;

        // Run migrations, if necessary
        let migrator = sqlx::migrate::Migrator::new(std::path::Path::new("./migrations")).await?;
        migrator
            .run(&pool)
            .await
            .context("Error while running migrations.")?;

        Ok(pool)
    }

    pub fn setup(self, cc: &eframe::CreationContext) -> Self {
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
