#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")] // Hide console window on Windows in release

use anyhow::Context;
use anyhow::Result as AnyResult;
use eframe::egui::Vec2 as EguiVec2;

use data_storage::DataStorage;
use manga_ui::{MangaUI, UiMessenger};

mod manga_group_export;
mod cascade_delete;
mod data_storage;
mod manga_ui;
mod types;

fn main() -> AnyResult<()> {
    dotenvy::dotenv().context(".env file not found")?;

    let options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default()
        .with_max_inner_size(EguiVec2::new(1110., 800.))
        .with_min_inner_size(EguiVec2::new(1110., 800.))
        .with_inner_size(EguiVec2::new(1110., 800.)),
        ..Default::default()
    };

    let (backend_send, backend_recv) = crossbeam::channel::bounded(100);
    let (gui_send, gui_recv) = crossbeam::channel::bounded(100);
    let backend_thread = std::thread::spawn(move || DataStorage::start(backend_send, gui_recv));

    let manga_ui = MangaUI::new(UiMessenger {
            backend_recv,
            gui_send,
        });

    eframe::run_native(
        "Manga impression maker",
        options,
        Box::new(|cc| Ok(Box::new(manga_ui.setup(cc)))),
    ).unwrap();
    backend_thread.join().unwrap();

    Ok(())
}
