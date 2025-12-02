use axum::{
    Router,
    extract::{
        Query, State,
        ws::{Message, WebSocket, WebSocketUpgrade},
    },
    response::{Html, IntoResponse},
    routing::get,
};
use futures::{sink::SinkExt, stream::StreamExt};
use serde::{Deserialize, Serialize};
use shared::types::DisplayedMangaEntry;
use std::{
    collections::HashMap,
    sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
    },
    time::Duration,
};
use tokio::runtime::Builder;
use tokio::sync::{RwLock, mpsc};
use uuid::Uuid;

#[derive(Serialize, Clone)]
struct ClientState {
    manga_name: String,
    page_src: String,
    manga_score: i64,
    manga_comment: String,
    manga_pos: (usize, usize), // (current, total)
    page_pos: (usize, usize),  // (current, total)
}

struct User {
    tx: mpsc::UnboundedSender<Message>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Action {
    Next,
    Prev,
}

struct AppState {
    mangas: Vec<Manga>,
    current_manga: usize,
    current_page: usize,
    users: HashMap<Uuid, User>,
    actions: HashMap<Uuid, Option<Action>>,
}

pub struct Manga {
    name: String,
    score: i64,
    comment: String,
    page_paths: Vec<String>,
}

pub fn prepare_data(entries: &Vec<DisplayedMangaEntry>) -> Vec<Manga> {
    entries
        .into_iter()
        .map(|entry| Manga {
            name: entry.entry.name.clone(),
            score: entry.entry.score,
            comment: entry.entry.comment.clone(),
            page_paths: entry
                .thumbnails
                .iter()
                .map(|t| t.image.path.clone())
                .collect(),
        })
        .collect()
}

impl AppState {
    fn from_displayed(mangas: Vec<Manga>) -> Self {
        Self {
            mangas,
            current_manga: 0,
            current_page: 0,
            users: HashMap::new(),
            actions: HashMap::new(),
        }
    }

    fn get_client_state(&self) -> ClientState {
        let manga = &self.mangas[self.current_manga];
        ClientState {
            manga_name: manga.name.clone(),
            page_src: format!(
                "/image?manga={}&page={}",
                self.current_manga, self.current_page
            ),
            manga_score: manga.score,
            manga_comment: manga.comment.clone(),
            manga_pos: (self.current_manga + 1, self.mangas.len()),
            page_pos: (self.current_page + 1, manga.page_paths.len()),
        }
    }

    fn check_consensus(&self) -> Option<Action> {
        if self.actions.is_empty() || self.users.is_empty() {
            return None;
        }
        let first_action = *self.actions.values().next().unwrap();
        if first_action.is_none() {
            return None;
        }
        if self.actions.values().all(|&a| a == first_action) {
            first_action
        } else {
            None
        }
    }

    fn navigate(&mut self, action: Action) {
        match action {
            Action::Next => {
                let manga = &self.mangas[self.current_manga];
                if self.current_page + 1 < manga.page_paths.len() {
                    self.current_page += 1;
                } else if self.current_manga + 1 < self.mangas.len() {
                    self.current_manga += 1;
                    self.current_page = 0;
                }
            }
            Action::Prev => {
                if self.current_page > 0 {
                    self.current_page -= 1;
                } else if self.current_manga > 0 {
                    self.current_manga -= 1;
                    if self.mangas[self.current_manga].page_paths.len() > 0 {
                        self.current_page = self.mangas[self.current_manga].page_paths.len() - 1;
                    } else {
                        self.current_page = 0;
                    }
                }
            }
        }
        // Clear actions after successful navigation
        for action in self.actions.values_mut() {
            *action = None;
        }
    }
}

async fn home_handler(State(state): State<Arc<RwLock<AppState>>>) -> Html<String> {
    let client_state = {
        let state = state.read().await;
        serde_json::to_string(&state.get_client_state()).unwrap()
    };
    Html(format!(
        r#"
<!DOCTYPE html>
<html>
<head>
    <meta charset="utf-8">
    <style>
        body {{
            margin: 0;
            background: #1a1a1a;
            color: white;
            font-family: sans-serif;
            display: flex;
            flex-direction: column;
            height: 100vh;
            overflow: hidden;
        }}
        #debug {{
            position: absolute;
            opacity: 20%;
            top: 10px;
            left: 10px;
            font-size: 12px;
            background: rgba(0,0,0,0.7);
            padding: 5px;
            border-radius: 3px;
            z-index: 100;
        }}
        #header {{
            text-align: center;
            flex-shrink: 0;
        }}
        #header h2 {{
            margin: 5px 0;
            font-size: 28px;
        }}
        #manga-score {{
            font-size: 18px;
            margin: 0px;
        }}
        #manga-comment {{
            font-size: 14px;
            margin-bottom: 10px;
            color: #ccc;
        }}
        #image-container {{
            flex: 1;
            display: flex;
            justify-content: center;
            align-items: center;
            position: relative;
            min-height: 0;
        }}
        #manga-img {{
            padding-bottom: 10px;
            max-width: 100%;
            max-height: 100%;
            object-fit: contain;
            pointer-events: none;
        }}
        .nav-arrow {{
            position: absolute;
            top: 0;
            height: 100%;
            width: 33.33%;
            display: flex;
            align-items: center;
            cursor: pointer;
            opacity: 0;
            transition: opacity 0.3s;
            user-select: none;
            z-index: 10;
        }}
        .nav-arrow:hover {{
            opacity: 0.4;
        }}
        #prev-arrow {{
            left: 0;
            justify-content: flex-start;
            padding-left: 30px;
        }}
        #next-arrow {{
            right: 0;
            justify-content: flex-end;
            padding-right: 30px;
        }}
        .arrow-char {{
            font-size: 72px;
            color: white;
            text-shadow: 0 0 15px rgba(0,0,0,0.8);
        }}
        #counters {{
            position: absolute;
            bottom: 15px;
            right: 15px;
            font-size: 14px;
            background: rgba(0,0,0,0.5);
            padding: 8px 12px;
            border-radius: 4px;
        }}
    </style>
</head>
<body>
    <div id="debug">UUID: <span id="uuid">-</span><br>Last: <span id="last-msg">-</span></div>
    <div id="header">
        <h2 id="manga-name"></h2>
        <div id="manga-score"></div>
        <div id="manga-comment"></div>
    </div>
    <div id="image-container">
        <img id="manga-img" src="" alt="">
        <div class="nav-arrow" id="prev-arrow"><span class="arrow-char">‹</span></div>
        <div class="nav-arrow" id="next-arrow"><span class="arrow-char">›</span></div>
    </div>
    <div id="counters">
        <div>Manga: <span id="manga-counter"></span></div>
        <div>Page: <span id="page-counter"></span></div>
    </div>

    <script>
        const initialState = {};
        let ws = null;
        let uuid = crypto.randomUUID();
        let lastMsg = "-";

        document.getElementById('uuid').textContent = uuid;

        function updateUI(state) {{
            document.getElementById('manga-name').textContent = state.manga_name;
            document.getElementById('manga-score').textContent = `${{state.manga_score}}/10`;
            document.getElementById('manga-comment').textContent = state.manga_comment || '';
            document.getElementById('manga-img').src = state.page_src;
            document.getElementById('manga-counter').textContent = `${{state.manga_pos[0]}} / ${{state.manga_pos[1]}}`;
            document.getElementById('page-counter').textContent = `${{state.page_pos[0]}} / ${{state.page_pos[1]}}`;
        }}

        function connect() {{
            ws = new WebSocket(`ws://${{window.location.host}}/ws`);

            ws.onopen = () => {{
                lastMsg = "Connected";
                document.getElementById('last-msg').textContent = lastMsg;
                ws.send(JSON.stringify({{ type: 'hello', uuid: uuid }}));
            }};

            ws.onmessage = (event) => {{
                lastMsg = event.data.slice(0, 50) + '...';
                document.getElementById('last-msg').textContent = lastMsg;
                const state = JSON.parse(event.data);
                updateUI(state);
            }};

            ws.onclose = () => {{
                lastMsg = "Disconnected";
                document.getElementById('last-msg').textContent = lastMsg;
                setTimeout(connect, 3000); // Reconnect after 3s
            }};
        }}

        document.getElementById('prev-arrow').onclick = () => {{
            ws.send(JSON.stringify({{ type: 'prev', uuid: uuid }}));
        }};

        document.getElementById('next-arrow').onclick = () => {{
            ws.send(JSON.stringify({{ type: 'next', uuid: uuid }}));
        }};

        connect();
        updateUI(initialState);
    </script>
</body>
</html>
        "#,
        client_state
    ))
}

#[derive(Deserialize)]
struct ImageParams {
    manga: usize,
    page: usize,
}

async fn image_handler(
    State(state): State<Arc<RwLock<AppState>>>,
    Query(params): Query<ImageParams>,
) -> impl IntoResponse {
    let path = {
        let state = state.read().await;
        if params.manga >= state.mangas.len() {
            return Err("Invalid manga index");
        }
        let manga = &state.mangas[params.manga];
        if params.page >= manga.page_paths.len() {
            return Err("Invalid page index");
        }
        manga.page_paths[params.page].clone()
    };

    match tokio::fs::read(path).await {
        Ok(bytes) => Ok(([(axum::http::header::CONTENT_TYPE, "image/webp")], bytes)),
        Err(_) => Err("Failed to read image"),
    }
}

async fn ws_handler(
    ws: WebSocketUpgrade,
    State(state): State<Arc<RwLock<AppState>>>,
) -> impl IntoResponse {
    ws.on_upgrade(|socket| handle_socket(socket, state))
}

async fn handle_socket(socket: WebSocket, state: Arc<RwLock<AppState>>) {
    let (mut sender, mut receiver) = socket.split();
    let user_uuid = Uuid::new_v4();
    let (tx, mut rx) = mpsc::unbounded_channel::<Message>();

    // Register user
    {
        let mut state = state.write().await;
        state.users.insert(user_uuid, User { tx: tx.clone() });
        state.actions.insert(user_uuid, None);
        tracing::info!("User {} connected ({} total)", user_uuid, state.users.len());
    }

    // Send initial state
    let initial_state = {
        let state = state.read().await;
        state.get_client_state()
    };
    let _ = sender
        .send(Message::Text(
            serde_json::to_string(&initial_state).unwrap().into(),
        ))
        .await;

    // Spawn task to send messages from channel to websocket
    let mut send_task = tokio::spawn(async move {
        while let Some(msg) = rx.recv().await {
            if sender.send(msg).await.is_err() {
                break;
            }
        }
    });

    // Process incoming messages
    let mut recv_task = {
        let state = state.clone();
        tokio::spawn(async move {
            while let Some(Ok(msg)) = receiver.next().await {
                if let Message::Text(text) = msg {
                    handle_client_msg(&state, user_uuid, &text).await;
                }
            }
        })
    };

    // Wait for disconnect
    tokio::select! {
        _ = (&mut send_task) => {},
        _ = (&mut recv_task) => {},
    }

    // Cleanup
    {
        let mut state = state.write().await;
        state.users.remove(&user_uuid);
        state.actions.remove(&user_uuid);
        // Clear all actions since user set changed
        for action in state.actions.values_mut() {
            *action = None;
        }
        tracing::info!(
            "User {} disconnected ({} remaining)",
            user_uuid,
            state.users.len()
        );
    }
}

async fn handle_client_msg(state: &Arc<RwLock<AppState>>, user_uuid: Uuid, text: &str) {
    #[derive(Deserialize)]
    struct ClientMsg {
        r#type: String,
        #[allow(dead_code)]
        uuid: String, // Can verify it matches
    }

    if let Ok(msg) = serde_json::from_str::<ClientMsg>(text) {
        match msg.r#type.as_str() {
            "hello" => {
                // Already registered, could send welcome
            }
            "next" => {
                let mut state = state.write().await;
                state.actions.insert(user_uuid, Some(Action::Next));
                if let Some(consensus) = state.check_consensus() {
                    state.navigate(consensus);
                    broadcast_state(&state).await;
                }
            }
            "prev" => {
                let mut state = state.write().await;
                state.actions.insert(user_uuid, Some(Action::Prev));
                if let Some(consensus) = state.check_consensus() {
                    state.navigate(consensus);
                    broadcast_state(&state).await;
                }
            }
            _ => {}
        }
    }
}

async fn broadcast_state(state: &AppState) {
    let client_state = state.get_client_state();
    let msg = Message::Text(serde_json::to_string(&client_state).unwrap().into());
    for user in state.users.values() {
        let _ = user.tx.send(msg.clone());
    }
}

pub fn start_web_server(shutdown_requested: Arc<AtomicBool>, manga_entries: Vec<Manga>) {
    let state = Arc::new(RwLock::new(AppState::from_displayed(manga_entries)));
    if let Err(e) = tracing_subscriber::fmt::try_init() {
        dbg!("Failed to install tracing fmt:", e);
    }

    let rt = Builder::new_multi_thread()
        .worker_threads(4)
        .thread_name("webserver")
        .enable_all()
        .build()
        .unwrap();

    // Heartbeat task: send ping every 3s
    let heartbeat_state = state.clone();
    rt.spawn(async move {
        let mut interval = tokio::time::interval(Duration::from_secs(3));
        loop {
            interval.tick().await;
            let state = heartbeat_state.read().await;
            for user in state.users.values() {
                let _ = user.tx.send(Message::Ping(vec![].into()));
            }
        }
    });

    let app = Router::new()
        .route("/", get(home_handler))
        .route("/ws", get(ws_handler))
        .route("/image", get(image_handler))
        .with_state(state);

    rt.block_on(async move {
        let listener = tokio::net::TcpListener::bind("127.0.0.1:3000")
            .await
            .unwrap();
        tracing::info!("Server listening on http://127.0.0.1:3000");

        axum::serve(listener, app)
            .with_graceful_shutdown(async move {
                loop {
                    if shutdown_requested.load(Ordering::Relaxed) {
                        tracing::info!("Shutdown requested");
                        break;
                    }
                    tokio::time::sleep(Duration::from_secs(1)).await;
                }
            })
            .await
            .unwrap();
    });
}
