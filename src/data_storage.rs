use anyhow::Context;

use std::collections::HashMap;
use std::path::PathBuf;

use crate::cascade_delete::CascadeDelete;
use crate::manga_ui::MangaUI;
use crate::types::{
    BackendChannelSend, BackendCommand, GuiChannelRecv, GuiCommand, ImageCacheEntry, MangaGroup,
    SqlitePool,
};

pub struct DataStorage {
    pub manga_groups: Vec<MangaGroup>,
    pub selected_group: Option<MangaGroup>,
    pub group_to_delete: Option<MangaGroup>,
    pub images_cache: HashMap<PathBuf, ImageCacheEntry>,
    pub cwd: PathBuf,
    pub db_pool: SqlitePool,
    pub backend_send: BackendChannelSend,
    pub gui_recv: GuiChannelRecv,
    pub exiting: bool,
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

    pub fn start(backend_send: BackendChannelSend, gui_recv: GuiChannelRecv) {
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
            db_pool,
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
                GuiCommand::CreateNewMangaEntry => todo!(),
                GuiCommand::GetSelectedGroupInfo(group) => {
                    self.prepare_and_send_selected_group(group).await
                }
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

    async fn prepare_and_send_selected_group(&self, group: MangaGroup) {
        self.backend_send
            .send(BackendCommand::UpdateGroups(self.manga_groups.clone()))
            .unwrap();
    }
}
