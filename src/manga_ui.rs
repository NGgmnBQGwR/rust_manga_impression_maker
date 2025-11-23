use anyhow::Context;
use anyhow::Result as AnyResult;
use eframe::egui::{Color32, Stroke, Vec2 as EguiVec2};

use crate::types::MangaEntry;
use crate::types::{
    BackendChannelRecv, BackendCommand, DisplayedMangaEntry, GuiChannelSend, GuiCommand,
    MangaGroup, MangaImage, SqlitePool,
};

pub struct UiMessenger {
    pub backend_recv: BackendChannelRecv,
    pub gui_send: GuiChannelSend,
}

impl UiMessenger {
    fn delete_image(&self, image: &MangaImage, entry: &MangaEntry) {
        self.gui_send
            .send(GuiCommand::DeleteImage(image.clone()))
            .unwrap();
        self.gui_send
            .send(GuiCommand::UpdateEntryImages(entry.clone()))
            .unwrap();
    }

    fn save_entry(&self, entry: &DisplayedMangaEntry) {
        self.gui_send
            .send(GuiCommand::SaveMangaEntry(entry.entry.clone()))
            .unwrap();
    }

    fn save_all_entries(&self, manga_entries: &[DisplayedMangaEntry], selected_group: &MangaGroup) {
        let entries = manga_entries.iter().map(|x| x.entry.clone()).collect();
        self.gui_send
            .send(GuiCommand::SaveAllMangaEntries(entries))
            .unwrap();
        self.gui_send
            .send(GuiCommand::GetSelectedGroupInfo(selected_group.clone()))
            .unwrap();
    }

    fn add_images_from_disk(&self, entry: &MangaEntry) {
        self.gui_send
            .send(GuiCommand::AddImagesFromDisk(entry.clone()))
            .unwrap();
        self.gui_send
            .send(GuiCommand::UpdateEntryImages(entry.clone()))
            .unwrap();
    }

    fn add_image_from_clipboard(&self, entry: &MangaEntry) {
        self.gui_send
            .send(GuiCommand::AddImageFromClipboard(entry.clone()))
            .unwrap();
        self.gui_send
            .send(GuiCommand::UpdateEntryImages(entry.clone()))
            .unwrap();
    }
}

pub struct MangaUI {
    pub manga_groups: Vec<MangaGroup>,
    pub selected_group: Option<MangaGroup>,
    pub group_to_delete: Option<MangaGroup>,
    pub entry_to_delete: Option<MangaEntry>,
    pub manga_entries: Option<Vec<DisplayedMangaEntry>>,
    pub messenger: UiMessenger,
}

impl eframe::App for MangaUI {
    fn on_exit(&mut self, _: Option<&eframe::glow::Context>) {
        self.messenger.gui_send.send(GuiCommand::Exit).unwrap();
    }

    fn update(&mut self, ctx: &egui::Context, _: &mut eframe::Frame) {
        self.process_backend_commands(ctx);

        egui::SidePanel::left("left_panel_manga_groups")
            .resizable(false)
            .show(ctx, |ui| {
                self.draw_manga_groups_panel(ctx, ui);
            });

        egui::CentralPanel::default().show(ctx, |ui| {
            self.draw_central_manga_entries_panel(ctx, ui);
        });

        if self.group_to_delete.is_some() {
            self.draw_group_delete_confirm(ctx);
        }

        #[cfg(debug_assertions)]
        {
            ctx.set_debug_on_hover(true);
            egui::Window::new("🔧 Settings")
                .vscroll(true)
                .default_open(false)
                .show(ctx, |ui| {
                    ctx.settings_ui(ui);
                });
            egui::Window::new("🔍 Inspection")
                .vscroll(true)
                .default_open(false)
                .show(ctx, |ui| {
                    ctx.inspection_ui(ui);
                });

            egui::Window::new("📝 Memory")
                .resizable(false)
                .default_open(false)
                .show(ctx, |ui| {
                    ctx.memory_ui(ui);
                });
        }
    }
}

impl MangaUI {
    fn create_new_manga_entry(&mut self) {
        if self.selected_group.is_none() {
            return;
        }

        self.messenger
            .gui_send
            .send(GuiCommand::CreateNewMangaEntry(
                self.selected_group.as_ref().unwrap().clone(),
            ))
            .unwrap();
        self.messenger
            .gui_send
            .send(GuiCommand::GetSelectedGroupInfo(
                self.selected_group.as_ref().unwrap().clone(),
            ))
            .unwrap();
    }

    fn create_new_manga_group(&mut self) {
        self.messenger
            .gui_send
            .send(GuiCommand::CreateNewMangaGroup)
            .unwrap();
        self.messenger
            .gui_send
            .send(GuiCommand::UpdateMangaGroups)
            .unwrap();
        self.messenger
            .gui_send
            .send(GuiCommand::GetUpdatedMangaGroups)
            .unwrap();
    }

    fn export_group(&mut self) {
        self.messenger
            .gui_send
            .send(GuiCommand::ExportGroup(
                self.selected_group.as_ref().unwrap().clone(),
            ))
            .unwrap();
    }

    fn add_names_from_folder(&mut self) {
        self.messenger
            .gui_send
            .send(GuiCommand::AddNamesFromFolder(
                self.selected_group.as_ref().unwrap().clone(),
            ))
            .unwrap();
    }

    fn refresh_manga_groups(&mut self) {
        self.messenger
            .gui_send
            .send(GuiCommand::GetUpdatedMangaGroups)
            .unwrap();
    }

    fn select_group(&mut self, group: MangaGroup) {
        self.selected_group = Some(group);
        self.messenger
            .gui_send
            .send(GuiCommand::GetSelectedGroupInfo(
                self.selected_group.clone().unwrap(),
            ))
            .unwrap();
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
        sqlx::migrate!("./migrations")
            .run(&pool)
            .await
            .context("Error while running migrations.")?;

        Ok(pool)
    }

    pub fn setup(self, cc: &eframe::CreationContext) -> Self {
        cc.egui_ctx.set_visuals(egui::Visuals::light());
        cc.egui_ctx.all_styles_mut(|style| {
            style.spacing.scroll = egui::style::ScrollStyle::solid();
        });
        cc.egui_ctx.all_styles_mut(|style| {
            style
                .text_styles
                .get_mut(&egui::TextStyle::Body)
                .unwrap()
                .size = 14.
        });

        {
            let backend_recv_clone = self.messenger.backend_recv.clone();
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
        }

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

        self.messenger
            .gui_send
            .send(GuiCommand::DeleteMangaGroup(
                self.group_to_delete.take().unwrap(),
            ))
            .unwrap();
        self.messenger
            .gui_send
            .send(GuiCommand::UpdateMangaGroups)
            .unwrap();
        self.messenger
            .gui_send
            .send(GuiCommand::GetUpdatedMangaGroups)
            .unwrap();
    }

    fn confirm_delete_entry(&mut self) {
        // Sanity check - we can't delete an entry if no entry was selected
        if self.entry_to_delete.is_none() {
            return;
        }

        let entry = self.entry_to_delete.take().unwrap();
        self.messenger
            .gui_send
            .send(GuiCommand::DeleteMangaEntry(entry))
            .unwrap();
        self.messenger
            .gui_send
            .send(GuiCommand::GetSelectedGroupInfo(
                self.selected_group.as_ref().unwrap().clone(),
            ))
            .unwrap();
    }

    fn process_backend_commands(&mut self, ctx: &egui::Context) {
        while let Ok(cmd) = self.messenger.backend_recv.try_recv() {
            match cmd {
                BackendCommand::UpdateGroups(groups) => self.manga_groups = groups,
                BackendCommand::UpdateSelectedGroup(entries) => {
                    self.manga_entries = Some(
                        entries
                            .into_iter()
                            .map(|mut x| {
                                for image in &x.thumbnails {
                                    x.textures.push(ctx.load_texture(
                                        format!("manga_image_{}", image.image.id),
                                        image.thumbnail.clone(),
                                        egui::TextureOptions::default(),
                                    ));
                                }
                                x
                            })
                            .collect(),
                    );
                }
                BackendCommand::UpdateThumbnailsForMangaEntry((entry_id, images)) => {
                    if self.manga_entries.is_none() {
                        return;
                    }
                    if let Some(entry) = self
                        .manga_entries
                        .as_mut()
                        .unwrap()
                        .iter_mut()
                        .find(|x| x.entry.id == entry_id)
                    {
                        entry.thumbnails = images;
                        entry.textures.clear();
                        for image in &entry.thumbnails {
                            entry.textures.push(ctx.load_texture(
                                format!("manga_image_{}", image.image.id),
                                image.thumbnail.clone(),
                                egui::TextureOptions::default(),
                            ));
                        }
                    }
                }
            }
            ctx.request_repaint();
        }
    }

    fn draw_group_delete_confirm(&mut self, ctx: &egui::Context) {
        if self.group_to_delete.is_some() {
            let group = self.group_to_delete.clone().unwrap();
            egui::Window::new(format!("Delete group #{} ({})", group.id, group.added_on))
                .collapsible(false)
                .resizable(false)
                .default_pos((0., 150.))
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
    }

    fn draw_entry_delete_confirm(&mut self, ctx: &egui::Context) {
        if self.entry_to_delete.is_some() {
            let entry = self.entry_to_delete.clone().unwrap();
            egui::Window::new(format!("Delete entry #{} ({})", entry.id, entry.name))
                .collapsible(false)
                .resizable(false)
                .default_pos((0., 150.))
                .show(ctx, |ui| {
                    ui.horizontal(|ui| {
                        if ui.button("Cancel").clicked() {
                            self.entry_to_delete = None;
                        }

                        if ui.button("Yes!").clicked() {
                            self.confirm_delete_entry();
                        }
                    });
                });
        }
    }

    fn draw_manga_groups_panel(&mut self, _: &egui::Context, ui: &mut egui::Ui) {
        ui.heading(format!("Manga groups ({} total):", self.manga_groups.len()));
        ui.separator();

        ui.horizontal(|ui| {
            if ui.button("🔄 Refresh").clicked() {
                self.refresh_manga_groups();
            }
            if ui.button("➕ Add new group").clicked() {
                self.create_new_manga_group();
            }
            if ui.button("📥 Export").clicked() {
                self.export_group();
            }
        });
        ui.separator();

        // TODO: This variable should not be here, but otherwise I get errors like
        // "cannot borrow mutably twice" or "cannot borrow immutable as mutable",
        // because we borrow '&self' for loop, then in the closure we need to borrow
        // '&mut self' for select_group() call.
        let mut new_selected_group: Option<MangaGroup> = None;
        egui::ScrollArea::vertical().show(ui, |ui| {
            for group in &self.manga_groups {
                let (stroke, fill) = if self
                    .selected_group
                    .as_ref()
                    .map_or(false, |x| x.id == group.id)
                {
                    (
                        (2.0f32, Color32::from_rgb(0xA0, 0x10, 0x10)),
                        Color32::LIGHT_GRAY,
                    )
                } else {
                    (
                        (2.0f32, Color32::from_rgb(0x10, 0x10, 0x10)),
                        Color32::WHITE,
                    )
                };

                egui::Frame::new()
                    .inner_margin(5.)
                    .outer_margin(EguiVec2::new(0., 2.))
                    .stroke(Stroke::from(stroke))
                    .fill(fill)
                    .corner_radius(5.)
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

        if let Some(new_group) = new_selected_group {
            self.select_group(new_group);
        }
    }

    fn draw_central_manga_entries_panel(&mut self, ctx: &egui::Context, ui: &mut egui::Ui) {
        if self.selected_group.is_none() {
            ui.label("No manga group selected.");
            return;
        }

        ui.heading(format!(
            "Manga entries ({} total):",
            self.manga_entries.as_ref().map_or(0, std::vec::Vec::len)
        ));
        ui.separator();

        ui.horizontal(|ui| {
            if ui.button("🔄 Refresh").clicked() {
                self.select_group(self.selected_group.as_ref().unwrap().clone());
            }
            if ui.button("➕ Add new entry").clicked() {
                self.create_new_manga_entry();
            }
            if ui.button("🖴 Save all").clicked() && self.manga_entries.is_some() {
                self.messenger.save_all_entries(
                    self.manga_entries.as_ref().unwrap(),
                    self.selected_group.as_ref().unwrap(),
                );
            }
            if ui.button("🗄 Add names from folder").clicked() && self.manga_entries.is_some() {
                self.add_names_from_folder();
            }
        });
        ui.separator();

        if self.manga_entries.is_none() {
            ui.label("No entries.");
            return;
        }

        if self.entry_to_delete.is_some() {
            self.draw_entry_delete_confirm(ctx);
        }

        egui::ScrollArea::vertical().show(ui, |ui| {
            for entry in self.manga_entries.as_mut().unwrap().iter_mut() {
                let stroke = (2.0f32, Color32::from_rgb(0x10, 0x10, 0x10));
                let fill = Color32::LIGHT_GRAY;

                egui::Frame::new()
                    .inner_margin(5.)
                    .outer_margin(EguiVec2::new(0., 2.))
                    .stroke(Stroke::from(stroke))
                    .fill(fill)
                    .corner_radius(5.)
                    .show(ui, |ui| {
                        ui.horizontal(|ui| {
                            ui.vertical_centered_justified(|ui| {
                                ui.horizontal(|ui| {
                                    ui.label(format!("#{:03}", entry.entry.id));
                                    ui.label("Name: ");
                                    ui.add(egui::TextEdit::singleline(&mut entry.entry.name));
                                });
                                ui.horizontal(|ui| {
                                    ui.label("Score: ");
                                    ui.spacing_mut().slider_width = 280.;
                                    ui.add(egui::Slider::new(&mut entry.entry.score, 1..=10));
                                });
                            });

                            ui.horizontal_top(|ui| {
                                ui.label("Comment: ");
                                ui.add(
                                    egui::TextEdit::multiline(&mut entry.entry.comment)
                                        .desired_rows(3),
                                );
                            });
                            ui.vertical(|ui| {
                                let delete_button = egui::Button::new("🗑").fill(Color32::LIGHT_RED);
                                if ui.add(delete_button).clicked() {
                                    self.entry_to_delete = Some(entry.entry.clone());
                                }
                                let save_button = egui::Button::new("🖴").fill(Color32::LIGHT_GREEN);
                                if ui.add(save_button).clicked() {
                                    self.messenger.save_entry(entry);
                                }
                            });
                        });

                        ui.horizontal_top(|ui| {
                            ui.label("Images:");
                            let add_images_button = egui::Button::new("🗀 Add from disk");
                            if ui.add(add_images_button).clicked() {
                                self.messenger.add_images_from_disk(&entry.entry);
                            }
                            let paste_image_button = egui::Button::new("📋 Paste from clipboard");
                            if ui.add(paste_image_button).clicked() {
                                self.messenger.add_image_from_clipboard(&entry.entry);
                            }
                        });
                        egui::ScrollArea::horizontal()
                            .id_salt(format!("images_scroll_area_{}", entry.entry.id))
                            .show(ui, |ui| {
                                egui::Grid::new(format!("grid_{}", entry.entry.id)).show(
                                    ui,
                                    |ui| {
                                        for (texture, image_data) in core::iter::zip(
                                            entry.textures.iter(),
                                            entry.thumbnails.iter(),
                                        ) {
                                            let image = egui::Button::image(texture);
                                            let added_image = ui.add(image).on_hover_ui(|ui| {
                                                ui.label("Click to delete");
                                            });
                                            if added_image.clicked() {
                                                self.messenger
                                                    .delete_image(&image_data.image, &entry.entry);
                                            }
                                        }
                                    },
                                );
                            });
                    });
            }
        });
    }
}
