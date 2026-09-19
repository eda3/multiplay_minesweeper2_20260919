//! `client` と `server` がやり取りするメッセージの型。
//!
//! 通信は WebSocket＋JSON。種類は `type` フィールドに `snake_case` で入る。

use crate::error::Error;
use crate::game::{CellState, Game, HEIGHT, Status, WIDTH};
use serde::{Deserialize, Serialize};

/// ブラウザからサーバーへ送る操作。
///
/// ```rust
/// use game_core::ClientMessage;
///
/// let message: ClientMessage = serde_json::from_str(r#"{"type":"reveal_cell","x":3,"y":4}"#)?;
/// assert_eq!(message, ClientMessage::RevealCell { x: 3, y: 4 });
/// # Ok::<(), serde_json::Error>(())
/// ```
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum ClientMessage {
    /// マス `(x, y)` を開く。
    RevealCell {
        /// 列。
        x: usize,
        /// 行。
        y: usize,
    },
    /// マス `(x, y)` の旗を立てる。立っていれば取り消す。
    ToggleFlag {
        /// 列。
        x: usize,
        /// 行。
        y: usize,
    },
    /// 盤面を新しく作り直す。
    ResetGame,
}

/// クライアントに見せてよい範囲で表した、1マスの状態。地雷かどうかは含まない。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "state", rename_all = "snake_case")]
pub enum CellView {
    /// まだ開いていない。
    Hidden,
    /// 旗が立っている。
    Flagged,
    /// 開いている。
    Open {
        /// 周り8マスにある地雷の数。
        adjacent: usize,
    },
}

/// 新しく開いたマス。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct RevealedCell {
    /// 列。
    pub x: usize,
    /// 行。
    pub y: usize,
    /// 周り8マスにある地雷の数。
    pub adjacent: usize,
}

/// クライアントに送ってよい範囲の盤面。
///
/// 地雷の位置は、勝敗がつくまでは含まれない。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct BoardView {
    /// 全マスの状態。左上から右へ、上の行から順に `WIDTH * HEIGHT` 個。
    pub cells: Vec<CellView>,
    /// 勝敗。
    pub status: Status,
    /// 地雷の位置 `(x, y)`。勝敗がつくまでは `None`（JSON では項目ごと省く）。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub mines: Option<Vec<(usize, usize)>>,
}

impl BoardView {
    /// `game` の、いま見えている盤面を作る。
    ///
    /// ```rust
    /// use game_core::{BoardView, Game};
    ///
    /// let view = BoardView::from_game(&Game::new(1))?;
    /// assert_eq!(view.mines, None);
    /// # Ok::<(), game_core::Error>(())
    /// ```
    ///
    /// # Errors
    ///
    /// 盤面の範囲内だけを調べるので、通常は失敗しない。
    pub fn from_game(game: &Game) -> Result<Self, Error> {
        let mut cells = Vec::with_capacity(WIDTH * HEIGHT);
        let mut mines = Vec::new();
        for y in 0..HEIGHT {
            for x in 0..WIDTH {
                cells.push(match game.state(x, y)? {
                    CellState::Hidden => CellView::Hidden,
                    CellState::Flagged => CellView::Flagged,
                    CellState::Open => CellView::Open {
                        adjacent: game.adjacent_mines(x, y)?,
                    },
                });
                if game.is_mine(x, y)? {
                    mines.push((x, y));
                }
            }
        }
        Ok(Self {
            cells,
            status: game.status(),
            mines: (game.status() != Status::Playing).then_some(mines),
        })
    }

    /// `before` のときには開いていなくて、いまは開いているマス。
    #[must_use]
    pub fn newly_opened_since(&self, before: &Self) -> Vec<RevealedCell> {
        self.cells
            .iter()
            .zip(&before.cells)
            .enumerate()
            .filter_map(|(i, (now, was))| match (now, was) {
                (CellView::Open { adjacent }, CellView::Hidden | CellView::Flagged) => {
                    Some(RevealedCell {
                        x: i % WIDTH,
                        y: i / WIDTH,
                        adjacent: *adjacent,
                    })
                }
                _ => None,
            })
            .collect()
    }
}

/// サーバーからブラウザへ送る知らせ。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum ServerMessage {
    /// 接続した本人に、最初に送る。自分の番号と、その時点の盤面。
    Init {
        /// 接続した本人の番号。
        player_id: u32,
        /// その時点で見えている盤面。
        board: BoardView,
    },
    /// 新しい人が入った（本人以外に送る）。
    PlayerJoined {
        /// 入った人の番号。
        player_id: u32,
    },
    /// 人が抜けた。
    PlayerLeft {
        /// 抜けた人の番号。
        player_id: u32,
    },
    /// マスが開いた。
    CellsRevealed {
        /// 新しく開いたマス。
        cells: Vec<RevealedCell>,
    },
    /// 旗が変わった。
    FlagToggled {
        /// 列。
        x: usize,
        /// 行。
        y: usize,
        /// 操作のあと、旗が立っているか。
        flagged: bool,
    },
    /// 勝敗がついた。ここで初めて地雷の位置を送る。
    GameOver {
        /// 勝ちか負けか。
        status: Status,
        /// 地雷の位置 `(x, y)`。
        mines: Vec<(usize, usize)>,
    },
    /// 盤面が新しく作り直された。全マスが閉じた状態に戻る。
    GameReset,
}

#[cfg(test)]
mod tests {
    use super::*;

    fn json(value: &impl Serialize) -> Result<String, serde_json::Error> {
        serde_json::to_string(value)
    }

    #[test]
    fn client_messages_use_snake_case_type_names() -> Result<(), serde_json::Error> {
        assert_eq!(
            json(&ClientMessage::RevealCell { x: 1, y: 2 })?,
            r#"{"type":"reveal_cell","x":1,"y":2}"#
        );
        assert_eq!(
            json(&ClientMessage::ToggleFlag { x: 3, y: 4 })?,
            r#"{"type":"toggle_flag","x":3,"y":4}"#
        );
        assert_eq!(json(&ClientMessage::ResetGame)?, r#"{"type":"reset_game"}"#);
        Ok(())
    }

    #[test]
    fn server_messages_use_snake_case_type_names() -> Result<(), serde_json::Error> {
        assert_eq!(
            json(&ServerMessage::PlayerJoined { player_id: 5 })?,
            r#"{"type":"player_joined","player_id":5}"#
        );
        assert_eq!(
            json(&ServerMessage::PlayerLeft { player_id: 5 })?,
            r#"{"type":"player_left","player_id":5}"#
        );
        assert_eq!(
            json(&ServerMessage::FlagToggled {
                x: 1,
                y: 2,
                flagged: true
            })?,
            r#"{"type":"flag_toggled","x":1,"y":2,"flagged":true}"#
        );
        assert_eq!(
            json(&ServerMessage::CellsRevealed {
                cells: vec![RevealedCell {
                    x: 0,
                    y: 1,
                    adjacent: 2
                }]
            })?,
            r#"{"type":"cells_revealed","cells":[{"x":0,"y":1,"adjacent":2}]}"#
        );
        assert_eq!(
            json(&ServerMessage::GameOver {
                status: Status::Lost,
                mines: vec![(1, 2)]
            })?,
            r#"{"type":"game_over","status":"lost","mines":[[1,2]]}"#
        );
        assert_eq!(json(&ServerMessage::GameReset)?, r#"{"type":"game_reset"}"#);
        Ok(())
    }

    #[test]
    fn server_message_round_trips() -> Result<(), Box<dyn std::error::Error>> {
        let board = BoardView::from_game(&Game::new(1))?;
        let message = ServerMessage::Init {
            player_id: 7,
            board,
        };
        let decoded: ServerMessage = serde_json::from_str(&json(&message)?)?;
        assert_eq!(decoded, message);
        Ok(())
    }

    #[test]
    fn board_view_lists_open_and_flagged_cells() -> Result<(), Error> {
        let mut game = Game::new(3);
        game.toggle_flag(0, 0)?;
        game.open(8, 8)?;
        let view = BoardView::from_game(&game)?;
        assert_eq!(view.cells.len(), WIDTH * HEIGHT);
        assert_eq!(view.cells[0], CellView::Flagged);
        assert_eq!(
            view.cells[8 * WIDTH + 8],
            CellView::Open {
                adjacent: game.adjacent_mines(8, 8)?
            }
        );
        assert_eq!(view.status, Status::Playing);
        Ok(())
    }

    #[test]
    fn newly_opened_since_lists_only_cells_opened_after() -> Result<(), Error> {
        let mut game = Game::new(3);
        game.open(8, 8)?;
        let before = BoardView::from_game(&game)?;
        assert_eq!(before.newly_opened_since(&before), vec![]);
        // 閉じていた安全マスを1つ開く
        let (x, y) = (0..HEIGHT)
            .flat_map(|y| (0..WIDTH).map(move |x| (x, y)))
            .find(|&(x, y)| {
                game.state(x, y) == Ok(CellState::Hidden) && game.is_mine(x, y) == Ok(false)
            })
            .expect("最初の一手のあとにも、閉じた安全マスが残っている");
        game.open(x, y)?;
        let after = BoardView::from_game(&game)?;
        let opened = after.newly_opened_since(&before);
        assert!(opened.contains(&RevealedCell {
            x,
            y,
            adjacent: game.adjacent_mines(x, y)?
        }));
        assert!(
            opened
                .iter()
                .all(|c| before.cells[c.y * WIDTH + c.x] == CellView::Hidden)
        );
        Ok(())
    }
}
