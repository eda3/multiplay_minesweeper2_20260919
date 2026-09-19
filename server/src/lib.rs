//! マルチプレイマインスイーパーのサーバー。
//!
//! 盤面の正はここが持つ。`/ws` に WebSocket で繋いだ全員が、1つの盤面を共有する。

mod room;

use axum::Router;
use axum::extract::State;
use axum::extract::ws::{Message, WebSocket, WebSocketUpgrade};
use axum::response::Response;
use axum::routing::get;
use game_core::{ClientMessage, ServerMessage};
use room::Room;
use std::sync::Arc;
use tokio::net::TcpListener;
use tokio::sync::{Mutex, mpsc};

type SharedRoom = Arc<Mutex<Room>>;

/// `listener` で待ち受け、`/ws` の WebSocket を配る。止まるのは、待ち受けが失敗したときだけ。
///
/// 盤面の乱数は `seed` から決まる。リセットのたびに `seed` を1ずつ進めた盤面になる。
///
/// # Errors
///
/// 待ち受けが失敗したとき。
pub async fn serve(listener: TcpListener, seed: u64) -> anyhow::Result<()> {
    let room: SharedRoom = Arc::new(Mutex::new(Room::new(seed)));
    let app = Router::new().route("/ws", get(ws_handler)).with_state(room);
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
    let _ = pump(socket, rx, &room).await;
    room.lock().await.leave(player_id);
}

/// 部屋からの知らせを本人に送り、本人からの操作を部屋に渡す。接続が閉じたら戻る。
async fn pump(
    mut socket: WebSocket,
    mut rx: mpsc::UnboundedReceiver<ServerMessage>,
    room: &SharedRoom,
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
                        let _ = room.lock().await.handle(message);
                    }
                }
                Some(Message::Close(_)) | None => return Ok(()),
                Some(_) => {}
            },
        }
    }
}
