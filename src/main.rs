// TODO: remove after checking most of them once
#![warn(
    clippy::all,
    clippy::restriction,
    clippy::pedantic,
    clippy::nursery,
    clippy::cargo
)]
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")] // Hide console window on Windows in release

use anyhow::Result as AnyResult;
use eframe::egui::Vec2 as EguiVec2;

use data_storage::DataStorage;
use manga_ui::{MangaUI, UiMessenger};

mod cascade_delete;
mod data_storage;
mod manga_ui;
mod types;

fn main() -> AnyResult<()> {
    dotenvy::dotenv().expect(".env file not found");

    let options = eframe::NativeOptions {
        initial_window_size: Some(EguiVec2::new(1100., 800.)),
        max_window_size: Some(EguiVec2::new(1100., 800.)),
        min_window_size: Some(EguiVec2::new(1100., 800.)),
        ..Default::default()
    };

    let (backend_send, backend_recv) = crossbeam::channel::bounded(100);
    let (gui_send, gui_recv) = crossbeam::channel::bounded(100);
    let backend_thread = std::thread::spawn(move || DataStorage::start(backend_send, gui_recv));

    let manga_ui = MangaUI {
        manga_groups: Vec::new(),
        selected_group: Option::None,
        group_to_delete: Option::None,
        entry_to_delete: Option::None,
        manga_entries: Option::None,
        messenger: UiMessenger { backend_recv, gui_send },
    };

    eframe::run_native(
        "Manga impression maker",
        options,
        Box::new(|_cc| Box::new(manga_ui.setup(_cc))),
    );
    backend_thread.join().unwrap();

    Ok(())
}
