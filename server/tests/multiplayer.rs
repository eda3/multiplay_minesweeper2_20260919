//! ボット（WebSocket のクライアント）で、`server` を外から確かめる。
//!
//! サーバーは空きポート（0番）で立て、盤面の乱数は固定のシードで作る。
//! 期待値は、テスト側で持つ `Game` に同じ操作をして求める。

use anyhow::{Context, bail};
use futures_util::{SinkExt, StreamExt};
use game_core::{
    BoardView, CellState, ClientMessage, Game, HEIGHT, RevealedCell, ServerMessage, WIDTH,
};
use std::collections::BTreeSet;
use std::net::SocketAddr;
use std::time::Duration;
use tokio::net::{TcpListener, TcpStream};
use tokio_tungstenite::tungstenite::Message;
use tokio_tungstenite::{MaybeTlsStream, WebSocketStream, connect_async};

const SEED: u64 = 7;
/// メッセージを待つ時間の上限。届かなければテストを失敗させる。
const TIMEOUT: Duration = Duration::from_secs(5);

/// 開いたマス `(x, y, 周りの地雷の数)`。
type Opened = (usize, usize, usize);

async fn start_server(seed: u64) -> anyhow::Result<SocketAddr> {
    let listener = TcpListener::bind("127.0.0.1:0").await?;
    let addr = listener.local_addr()?;
    tokio::spawn(server::serve(listener, seed));
    Ok(addr)
}

struct Bot {
    ws: WebSocketStream<MaybeTlsStream<TcpStream>>,
    player_id: u32,
    initial_board: BoardView,
}

impl Bot {
    /// 繋いで、最初の `init` を受け取るところまで進める。
    async fn join(addr: SocketAddr) -> anyhow::Result<Self> {
        let (ws, _) = connect_async(format!("ws://{addr}/ws")).await?;
        let mut bot = Self {
            ws,
            player_id: 0,
            initial_board: BoardView::from_game(&Game::new(0))?,
        };
        match bot.recv().await? {
            ServerMessage::Init { player_id, board } => {
                bot.player_id = player_id;
                bot.initial_board = board;
                Ok(bot)
            }
            other => bail!("最初に init が届くはずが、{other:?} が届いた"),
        }
    }

    async fn send(&mut self, message: ClientMessage) -> anyhow::Result<()> {
        self.send_raw(&serde_json::to_string(&message)?).await
    }

    async fn send_raw(&mut self, text: &str) -> anyhow::Result<()> {
        self.ws.send(Message::text(text)).await?;
        Ok(())
    }

    async fn recv(&mut self) -> anyhow::Result<ServerMessage> {
        let next = tokio::time::timeout(TIMEOUT, self.ws.next())
            .await
            .context("メッセージが時間内に届かなかった")?;
        match next.context("接続が閉じた")?? {
            Message::Text(text) => Ok(serde_json::from_str(&text)?),
            other => bail!("テキスト以外が届いた: {other:?}"),
        }
    }

    /// 次に届くのが `cells_revealed` であることを確かめ、開いたマスを返す。
    async fn expect_revealed(&mut self) -> anyhow::Result<BTreeSet<Opened>> {
        match self.recv().await? {
            ServerMessage::CellsRevealed { cells } => Ok(cells.iter().map(opened).collect()),
            other => bail!("cells_revealed が届くはずが、{other:?} が届いた"),
        }
    }

    async fn expect_flag(&mut self, x: usize, y: usize, flagged: bool) -> anyhow::Result<()> {
        assert_eq!(
            self.recv().await?,
            ServerMessage::FlagToggled { x, y, flagged }
        );
        Ok(())
    }
}

const fn opened(cell: &RevealedCell) -> Opened {
    (cell.x, cell.y, cell.adjacent)
}

/// 開いているマスすべて。
fn open_cells(game: &Game) -> anyhow::Result<BTreeSet<Opened>> {
    let mut cells = BTreeSet::new();
    for y in 0..HEIGHT {
        for x in 0..WIDTH {
            if game.state(x, y)? == CellState::Open {
                cells.insert((x, y, game.adjacent_mines(x, y)?));
            }
        }
    }
    Ok(cells)
}

/// 手元の盤面で `(x, y)` を開き、新しく開いたマスを返す。サーバーが送るはずの内容の期待値になる。
fn reveal_locally(game: &mut Game, x: usize, y: usize) -> anyhow::Result<BTreeSet<Opened>> {
    let before = open_cells(game)?;
    game.open(x, y)?;
    Ok(open_cells(game)?.difference(&before).copied().collect())
}

/// まだ閉じている地雷でないマス。
fn hidden_safe_cell(game: &Game) -> anyhow::Result<(usize, usize)> {
    (0..HEIGHT)
        .flat_map(|y| (0..WIDTH).map(move |x| (x, y)))
        .find(|&(x, y)| {
            game.state(x, y) == Ok(CellState::Hidden) && game.is_mine(x, y) == Ok(false)
        })
        .context("閉じた安全マスがない")
}

/// 2人が入った状態を作る。A に届く「B が入った」は読み捨てる。
async fn two_bots(seed: u64) -> anyhow::Result<(Bot, Bot)> {
    let addr = start_server(seed).await?;
    let mut a = Bot::join(addr).await?;
    let b = Bot::join(addr).await?;
    assert_eq!(
        a.recv().await?,
        ServerMessage::PlayerJoined {
            player_id: b.player_id
        }
    );
    Ok((a, b))
}

/// ⑦: ボット2人が同じ盤面を受け取る
#[tokio::test]
async fn item7_both_bots_receive_the_same_board() -> anyhow::Result<()> {
    let addr = start_server(SEED).await?;
    let mut a = Bot::join(addr).await?;
    let b = Bot::join(addr).await?;
    assert_ne!(a.player_id, b.player_id);
    assert_eq!(a.initial_board, b.initial_board);
    assert_eq!(a.initial_board, BoardView::from_game(&Game::new(SEED))?);
    assert_eq!(
        a.recv().await?,
        ServerMessage::PlayerJoined {
            player_id: b.player_id
        }
    );
    Ok(())
}

/// ⑦: 片方の「開く」が、もう片方にも届く（2人は同じ盤面を開いている）
#[tokio::test]
async fn item7_reveal_reaches_the_other_bot() -> anyhow::Result<()> {
    let (mut a, mut b) = two_bots(SEED).await?;
    let mut local = Game::new(SEED);

    let first = reveal_locally(&mut local, 8, 8)?;
    assert!(first.len() >= 9, "最初の一手は周り8マスも開く");
    a.send(ClientMessage::RevealCell { x: 8, y: 8 }).await?;
    assert_eq!(a.expect_revealed().await?, first);
    assert_eq!(b.expect_revealed().await?, first);

    // 今度は B が別のマスを開く。A にも、同じ盤面での結果が届く
    let (x, y) = hidden_safe_cell(&local)?;
    let second = reveal_locally(&mut local, x, y)?;
    b.send(ClientMessage::RevealCell { x, y }).await?;
    assert_eq!(a.expect_revealed().await?, second);
    assert_eq!(b.expect_revealed().await?, second);
    Ok(())
}

/// ⑦: 片方の「旗」が、もう片方にも届く
#[tokio::test]
async fn item7_flag_reaches_the_other_bot() -> anyhow::Result<()> {
    let (mut a, mut b) = two_bots(SEED).await?;

    a.send(ClientMessage::ToggleFlag { x: 3, y: 4 }).await?;
    a.expect_flag(3, 4, true).await?;
    b.expect_flag(3, 4, true).await?;

    // 取り消しも届く。今度は B が操作する
    b.send(ClientMessage::ToggleFlag { x: 3, y: 4 }).await?;
    a.expect_flag(3, 4, false).await?;
    b.expect_flag(3, 4, false).await?;
    Ok(())
}

/// ⑦: 片方の「リセット」が、もう片方にも届き、盤面が新しくなる
#[tokio::test]
async fn item7_reset_reaches_the_other_bot() -> anyhow::Result<()> {
    let (mut a, mut b) = two_bots(SEED).await?;
    // サーバーと同じ順に、旗を立ててから開く（旗のマスは開かない）
    let mut first_game = Game::new(SEED);
    first_game.toggle_flag(3, 4)?;
    let first = reveal_locally(&mut first_game, 8, 8)?;

    a.send(ClientMessage::ToggleFlag { x: 3, y: 4 }).await?;
    a.expect_flag(3, 4, true).await?;
    b.expect_flag(3, 4, true).await?;
    a.send(ClientMessage::RevealCell { x: 8, y: 8 }).await?;
    assert_eq!(a.expect_revealed().await?, first);
    assert_eq!(b.expect_revealed().await?, first);

    b.send(ClientMessage::ResetGame).await?;
    assert_eq!(a.recv().await?, ServerMessage::GameReset);
    assert_eq!(b.recv().await?, ServerMessage::GameReset);

    // 旗は消えている（もう一度立てると「立った」になる）
    a.send(ClientMessage::ToggleFlag { x: 3, y: 4 }).await?;
    a.expect_flag(3, 4, true).await?;
    b.expect_flag(3, 4, true).await?;

    // 盤面は、シードを1進めた新しいもの。同じ場所を開いても、結果が変わる
    let mut second_game = Game::new(SEED + 1);
    second_game.toggle_flag(3, 4)?;
    let second = reveal_locally(&mut second_game, 8, 8)?;
    assert_ne!(first, second, "リセット後も同じ盤面のまま");
    a.send(ClientMessage::RevealCell { x: 8, y: 8 }).await?;
    assert_eq!(a.expect_revealed().await?, second);
    assert_eq!(b.expect_revealed().await?, second);
    Ok(())
}

#[tokio::test]
async fn player_left_reaches_the_other_bot() -> anyhow::Result<()> {
    let (mut a, mut b) = two_bots(SEED).await?;
    let left = b.player_id;
    b.ws.close(None).await?;
    assert_eq!(
        a.recv().await?,
        ServerMessage::PlayerLeft { player_id: left }
    );
    Ok(())
}

#[tokio::test]
async fn invalid_operations_are_ignored_without_stopping_the_server() -> anyhow::Result<()> {
    let addr = start_server(SEED).await?;
    let mut a = Bot::join(addr).await?;

    // 盤面の外・読めないJSON・知らない種類。どれにも返事はなく、接続も切れない
    a.send(ClientMessage::RevealCell { x: WIDTH, y: 0 }).await?;
    a.send(ClientMessage::ToggleFlag { x: 0, y: HEIGHT })
        .await?;
    a.send_raw("これはJSONではない").await?;
    a.send_raw(r#"{"type":"unknown_operation"}"#).await?;

    // 続けて出した正しい操作の結果が、最初に届く
    a.send(ClientMessage::ToggleFlag { x: 1, y: 1 }).await?;
    a.expect_flag(1, 1, true).await?;

    // ほかの人も、同じサーバーに入れる
    let b = Bot::join(addr).await?;
    assert_ne!(a.player_id, b.player_id);
    Ok(())
}
