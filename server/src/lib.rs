//! マルチプレイマインスイーパーのサーバー。
//!
//! 盤面の正はここが持つ。`/ws` に WebSocket で繋いだ全員が、1つの盤面を共有する。
//! ページ（`static/`）も、同じポートから配る。

mod room;

use axum::Router;
use axum::extract::State;
use axum::extract::ws::{Message, WebSocket, WebSocketUpgrade};
use axum::response::Response;
use axum::routing::get;
use game_core::{ClientMessage, ServerMessage};
use room::Room;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use tokio::net::TcpListener;
use tokio::sync::{Mutex, mpsc};
use tower_http::services::ServeDir;

type SharedRoom = Arc<Mutex<Room>>;

/// ページ（`index.html` と、WASM のビルド結果 `pkg/`）を置いてある場所。ワークスペースの `static/`。
///
/// どこから起動しても同じ場所を指すよう、`server` クレートの場所から決める。
#[must_use]
pub fn default_static_dir() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../static")
}

/// `listener` で待ち受け、`/ws` の WebSocket と、`static/` のページを、同じポートから配る。
/// 止まるのは、待ち受けが失敗したときだけ。
///
/// 盤面の乱数は `seed` から決まる。リセットのたびに `seed` を1ずつ進めた盤面になる。
///
/// # Errors
///
/// 待ち受けが失敗したとき。
pub async fn serve(listener: TcpListener, seed: u64) -> anyhow::Result<()> {
    serve_with_static(listener, seed, default_static_dir()).await
}

/// [`serve`] と同じだが、ページを置く場所を `static_dir` で指定する。
///
/// # Errors
///
/// 待ち受けが失敗したとき。
pub async fn serve_with_static(
    listener: TcpListener,
    seed: u64,
    static_dir: impl AsRef<Path>,
) -> anyhow::Result<()> {
    let room: SharedRoom = Arc::new(Mutex::new(Room::new(seed)));
    let app = Router::new()
        .route("/ws", get(ws_handler))
        .fallback_service(ServeDir::new(static_dir))
        .with_state(room);
    axum::serve(listener, app).await?;
    Ok(())
}

async fn ws_handler(ws: WebSocketUpgrade, State(room): State<SharedRoom>) -> Response {
    ws.on_upgrade(move |socket| handle_socket(socket, room))
}

async fn handle_socket(socket: WebSocket, room: SharedRoom) {
    let (tx, rx) = mpsc::unbounded_channel();
    let Ok(player_id) = room.lock().await.join(tx) else {
        return;
    };
    // 途中で切れたときも、部屋から抜ける処理は必ず通す
    let _ = pump(socket, rx, &room, player_id).await;
    room.lock().await.leave(player_id);
}

/// 部屋からの知らせを本人に送り、本人からの操作を部屋に渡す。接続が閉じたら戻る。
async fn pump(
    mut socket: WebSocket,
    mut rx: mpsc::UnboundedReceiver<ServerMessage>,
    room: &SharedRoom,
    player_id: u32,
) -> anyhow::Result<()> {
    loop {
        tokio::select! {
            outgoing = rx.recv() => {
                let Some(message) = outgoing else { return Ok(()) };
                socket.send(Message::text(serde_json::to_string(&message)?)).await?;
            }
            incoming = socket.recv() => match incoming.transpose()? {
                Some(Message::Text(text)) => {
                    // 読めない操作と、game_core が断った操作は、サーバーを止めずに無視する
                    if let Ok(message) = serde_json::from_str::<ClientMessage>(&text) {
                        let _ = room.lock().await.handle(player_id, message);
                    }
                }
                Some(Message::Close(_)) | None => return Ok(()),
                Some(_) => {}
            },
        }
    }
}
