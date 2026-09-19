//! ボット（WebSocket のクライアント）で、`server` を外から確かめる。
//!
//! サーバーは空きポート（0番）で立て、盤面の乱数は固定のシードで作る。
//! 期待値は、テスト側で持つ `Game` に同じ操作をして求める。

use anyhow::{Context, bail};
use futures_util::{SinkExt, StreamExt};
use game_core::{
    BoardView, CellState, CellView, ClientMessage, Game, HEIGHT, RevealedCell, ServerMessage,
    Status, WIDTH,
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
    addr: SocketAddr,
    ws: WebSocketStream<MaybeTlsStream<TcpStream>>,
    player_id: u32,
    initial_board: BoardView,
    /// これまでに受け取ったメッセージの生のJSON。
    log: Vec<String>,
}

impl Bot {
    /// 繋いで、最初の `init` を受け取るところまで進める。
    async fn join(addr: SocketAddr) -> anyhow::Result<Self> {
        let (ws, _) = connect_async(format!("ws://{addr}/ws")).await?;
        let mut bot = Self {
            addr,
            ws,
            player_id: 0,
            initial_board: BoardView::from_game(&Game::new(0))?,
            log: Vec::new(),
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
            Message::Text(text) => {
                self.log.push(text.to_string());
                Ok(serde_json::from_str(&text)?)
            }
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

/// `board` が、手元の盤面 `game` の見えている状態（開いたマスと数字・旗・勝敗）と一致することを確かめる。
/// 期待値は `BoardView::from_game` を使わず、`Game` の問い合わせから1マスずつ求める。
fn assert_board_shows(board: &BoardView, game: &Game) -> anyhow::Result<()> {
    assert_eq!(board.cells.len(), WIDTH * HEIGHT);
    for y in 0..HEIGHT {
        for x in 0..WIDTH {
            let expected = match game.state(x, y)? {
                CellState::Hidden => CellView::Hidden,
                CellState::Flagged => CellView::Flagged,
                CellState::Open => CellView::Open {
                    adjacent: game.adjacent_mines(x, y)?,
                },
            };
            assert_eq!(board.cells[y * WIDTH + x], expected, "({x}, {y})");
        }
    }
    assert_eq!(board.status, game.status());
    Ok(())
}

/// A が `(x, y)` を開く。手元の盤面にも同じ操作をし、A に `cells_revealed` が届くところまで進める。
/// 勝敗がついたときは、続く `game_over` も読み、その勝敗を返す。
async fn reveal_as(bot: &mut Bot, game: &mut Game, x: usize, y: usize) -> anyhow::Result<Status> {
    let status = game.open(x, y)?;
    bot.send(ClientMessage::RevealCell { x, y }).await?;
    if status != Status::Lost {
        bot.expect_revealed().await?;
    }
    if status != Status::Playing {
        match bot.recv().await? {
            ServerMessage::GameOver { status: told, .. } => assert_eq!(told, status),
            other => bail!("game_over が届くはずが、{other:?} が届いた"),
        }
    }
    Ok(status)
}

/// ⑧: あとから入った3人目にも、開いたマスと旗が届く
#[tokio::test]
async fn item8_late_joiner_receives_open_cells_and_flags() -> anyhow::Result<()> {
    let (mut a, mut b) = two_bots(SEED).await?;
    let mut local = Game::new(SEED);

    local.toggle_flag(3, 4)?;
    a.send(ClientMessage::ToggleFlag { x: 3, y: 4 }).await?;
    a.expect_flag(3, 4, true).await?;
    b.expect_flag(3, 4, true).await?;
    reveal_as(&mut a, &mut local, 8, 8).await?;
    b.expect_revealed().await?;
    local.toggle_flag(12, 12)?;
    b.send(ClientMessage::ToggleFlag { x: 12, y: 12 }).await?;
    a.expect_flag(12, 12, true).await?;
    b.expect_flag(12, 12, true).await?;

    let c = Bot::join(a.addr).await?;
    assert_board_shows(&c.initial_board, &local)?;
    // 盤面が空ではないことも確かめる（期待値の側が空だと、上の比べ合いが素通りするため）
    let opened = c
        .initial_board
        .cells
        .iter()
        .filter(|cell| matches!(cell, CellView::Open { .. }))
        .count();
    assert!(opened >= 9, "開いたマスが届いていない");
    assert_eq!(c.initial_board.cells[4 * WIDTH + 3], CellView::Flagged);
    assert_eq!(c.initial_board.cells[12 * WIDTH + 12], CellView::Flagged);
    assert_eq!(c.initial_board.status, Status::Playing);
    // 3人目が入ったことは、先の2人にも届く
    assert_eq!(
        a.recv().await?,
        ServerMessage::PlayerJoined {
            player_id: c.player_id
        }
    );
    Ok(())
}

/// ⑧: リセットしたあとに入った人には、新しい（何も開いていない）盤面が届く
#[tokio::test]
async fn item8_late_joiner_after_reset_receives_a_fresh_board() -> anyhow::Result<()> {
    let (mut a, mut b) = two_bots(SEED).await?;
    let mut local = Game::new(SEED);
    local.toggle_flag(3, 4)?;
    a.send(ClientMessage::ToggleFlag { x: 3, y: 4 }).await?;
    a.expect_flag(3, 4, true).await?;
    b.expect_flag(3, 4, true).await?;
    reveal_as(&mut a, &mut local, 8, 8).await?;
    b.expect_revealed().await?;

    b.send(ClientMessage::ResetGame).await?;
    assert_eq!(a.recv().await?, ServerMessage::GameReset);
    assert_eq!(b.recv().await?, ServerMessage::GameReset);

    let c = Bot::join(a.addr).await?;
    assert_board_shows(&c.initial_board, &Game::new(SEED + 1))?;
    assert!(
        c.initial_board
            .cells
            .iter()
            .all(|cell| *cell == CellView::Hidden)
    );
    Ok(())
}

/// ⑧: 負けたあとに入った人には、負けたことと、開いたマスが届く
#[tokio::test]
async fn item8_late_joiner_receives_a_lost_game() -> anyhow::Result<()> {
    let (mut a, _b) = two_bots(SEED).await?;
    let mut local = Game::new(SEED);
    reveal_as(&mut a, &mut local, 8, 8).await?;
    let (mx, my) = (0..HEIGHT)
        .flat_map(|y| (0..WIDTH).map(move |x| (x, y)))
        .find(|&(x, y)| local.is_mine(x, y) == Ok(true))
        .context("地雷がない")?;
    assert_eq!(reveal_as(&mut a, &mut local, mx, my).await?, Status::Lost);

    let c = Bot::join(a.addr).await?;
    assert_eq!(c.initial_board.status, Status::Lost);
    assert_board_shows(&c.initial_board, &local)?;
    Ok(())
}

/// ⑧: 勝ったあとに入った人には、勝ったことと、開いたマスが届く
#[tokio::test]
async fn item8_late_joiner_receives_a_won_game() -> anyhow::Result<()> {
    let (mut a, _b) = two_bots(SEED).await?;
    let mut local = Game::new(SEED);
    reveal_as(&mut a, &mut local, 8, 8).await?;
    for y in 0..HEIGHT {
        for x in 0..WIDTH {
            if local.is_mine(x, y)? || local.state(x, y)? != CellState::Hidden {
                continue;
            }
            reveal_as(&mut a, &mut local, x, y).await?;
        }
    }
    assert_eq!(local.status(), Status::Won);

    let c = Bot::join(a.addr).await?;
    assert_eq!(c.initial_board.status, Status::Won);
    assert_board_shows(&c.initial_board, &local)?;
    Ok(())
}

/// 地雷のある座標すべて。
fn mines_of(game: &Game) -> anyhow::Result<BTreeSet<(usize, usize)>> {
    let mut mines = BTreeSet::new();
    for y in 0..HEIGHT {
        for x in 0..WIDTH {
            if game.is_mine(x, y)? {
                mines.insert((x, y));
            }
        }
    }
    Ok(mines)
}

/// JSON の項目名が `expected` と過不足なく一致することを確かめる。
fn assert_keys(value: &serde_json::Value, expected: &[&str]) {
    let mut actual: Vec<&str> = value
        .as_object()
        .map(|object| object.keys().map(String::as_str).collect())
        .unwrap_or_default();
    actual.sort_unstable();
    let mut expected = expected.to_vec();
    expected.sort_unstable();
    assert_eq!(actual, expected, "{value}");
}

/// 勝敗がつくまでに届いてよいメッセージの形かを確かめ、その種類を返す。
/// 届いてよい項目名を決め打ちにして、地雷の位置を運びうる項目がどこにもないことを確かめる。
fn assert_safe_while_playing(raw: &str) -> anyhow::Result<String> {
    let message: serde_json::Value = serde_json::from_str(raw)?;
    let kind = message["type"].as_str().context("type がない")?;
    match kind {
        "init" => {
            assert_keys(&message, &["type", "player_id", "board"]);
            let board = &message["board"];
            assert_keys(board, &["cells", "status"]);
            assert_eq!(board["status"], "playing", "{raw}");
            for cell in board["cells"].as_array().context("cells がない")? {
                match cell["state"].as_str() {
                    Some("hidden" | "flagged") => assert_keys(cell, &["state"]),
                    Some("open") => assert_keys(cell, &["state", "adjacent"]),
                    other => bail!("知らないマスの状態 {other:?}: {raw}"),
                }
            }
        }
        "player_joined" | "player_left" => assert_keys(&message, &["type", "player_id"]),
        "cells_revealed" => {
            assert_keys(&message, &["type", "cells"]);
            for cell in message["cells"].as_array().context("cells がない")? {
                assert_keys(cell, &["x", "y", "adjacent"]);
            }
        }
        "flag_toggled" => assert_keys(&message, &["type", "x", "y", "flagged"]),
        "game_reset" => assert_keys(&message, &["type"]),
        other => bail!("勝敗がつく前に {other} が届いた: {raw}"),
    }
    Ok(kind.to_owned())
}

/// 最後に届いた `game_over` の勝敗と、そこに書かれた地雷の位置。
fn last_game_over(bot: &Bot) -> anyhow::Result<(Status, BTreeSet<(usize, usize)>)> {
    match serde_json::from_str(bot.log.last().context("何も届いていない")?)? {
        ServerMessage::GameOver { status, mines } => Ok((status, mines.into_iter().collect())),
        other => bail!("最後に game_over が届くはずが、{other:?} が届いた"),
    }
}

/// ⑨: 勝敗がつくまで、地雷の位置はどのクライアントにも送らない
#[tokio::test]
async fn item9_no_mine_positions_are_sent_while_playing() -> anyhow::Result<()> {
    let (mut a, mut b) = two_bots(SEED).await?;
    let mut local = Game::new(SEED);
    reveal_as(&mut a, &mut local, 8, 8).await?;
    b.expect_revealed().await?;

    // 地雷のマスにも旗は立てられる。サーバーの返事から、地雷かどうかは分からない
    let (mx, my) = *mines_of(&local)?.first().context("地雷がない")?;
    local.toggle_flag(mx, my)?;
    b.send(ClientMessage::ToggleFlag { x: mx, y: my }).await?;
    a.expect_flag(mx, my, true).await?;
    b.expect_flag(mx, my, true).await?;

    // B が安全なマスを開く
    let (safe_x, safe_y) = hidden_safe_cell(&local)?;
    reveal_as(&mut b, &mut local, safe_x, safe_y).await?;
    a.expect_revealed().await?;

    // 途中から C が入る。C の init にも、地雷の位置はない
    let mut c = Bot::join(a.addr).await?;
    let joined = ServerMessage::PlayerJoined {
        player_id: c.player_id,
    };
    assert_eq!(a.recv().await?, joined);
    assert_eq!(b.recv().await?, joined);

    // リセットして、新しい盤面でも開く
    c.send(ClientMessage::ResetGame).await?;
    for bot in [&mut a, &mut b, &mut c] {
        assert_eq!(bot.recv().await?, ServerMessage::GameReset);
    }
    a.send(ClientMessage::RevealCell { x: 8, y: 8 }).await?;
    for bot in [&mut a, &mut b, &mut c] {
        bot.expect_revealed().await?;
    }

    // 3人が受け取った全メッセージが、地雷の位置を運ばない形をしている
    let mut kinds = BTreeSet::new();
    for bot in [&a, &b, &c] {
        for raw in &bot.log {
            kinds.insert(assert_safe_while_playing(raw)?);
        }
    }
    // 上の確認が空振りしないよう、いろいろな種類のメッセージを見たことも確かめる
    for kind in [
        "init",
        "player_joined",
        "cells_revealed",
        "flag_toggled",
        "game_reset",
    ] {
        assert!(kinds.contains(kind), "{kind} を見ていない: {kinds:?}");
    }
    Ok(())
}

/// ⑨: 負けて勝敗がついたら、地雷の位置が届く（その場にいる人にも、あとから入った人にも）
#[tokio::test]
async fn item9_mine_positions_are_sent_once_the_game_is_lost() -> anyhow::Result<()> {
    let (mut a, mut b) = two_bots(SEED).await?;
    let mut local = Game::new(SEED);
    reveal_as(&mut a, &mut local, 8, 8).await?;
    b.expect_revealed().await?;
    let mines = mines_of(&local)?;
    assert_eq!(mines.len(), game_core::MINE_COUNT);

    let &(mx, my) = mines.first().context("地雷がない")?;
    assert_eq!(reveal_as(&mut a, &mut local, mx, my).await?, Status::Lost);
    b.recv().await?;

    assert_eq!(last_game_over(&a)?, (Status::Lost, mines.clone()));
    assert_eq!(last_game_over(&b)?, (Status::Lost, mines.clone()));
    let c = Bot::join(a.addr).await?;
    assert_eq!(
        c.initial_board.mines.map(|m| m.into_iter().collect()),
        Some(mines)
    );
    Ok(())
}
