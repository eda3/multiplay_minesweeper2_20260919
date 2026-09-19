use game_core::{
    BoardView, CellState, ClientMessage, Error, Game, HEIGHT, ServerMessage, Status, WIDTH,
};
use std::collections::HashMap;
use tokio::sync::mpsc::UnboundedSender;

/// 全員が共有する1つの盤面と、繋がっている人たち。
pub struct Room {
    game: Game,
    seed: u64,
    next_player_id: u32,
    players: HashMap<u32, UnboundedSender<ServerMessage>>,
}

impl Room {
    pub fn new(seed: u64) -> Self {
        Self {
            game: Game::new(seed),
            seed,
            next_player_id: 1,
            players: HashMap::new(),
        }
    }

    /// 新しい人を入れる。本人にはいまの盤面を、ほかの人には入ったことを送る。返すのは本人の番号。
    pub fn join(&mut self, tx: UnboundedSender<ServerMessage>) -> Result<u32, Error> {
        let player_id = self.next_player_id;
        self.next_player_id += 1;
        let board = BoardView::from_game(&self.game)?;
        // 本人が切れていても、部屋には影響しない
        let _ = tx.send(ServerMessage::Init { player_id, board });
        self.broadcast(&ServerMessage::PlayerJoined { player_id });
        self.players.insert(player_id, tx);
        Ok(player_id)
    }

    /// 人を抜けさせ、残った人に知らせる。
    pub fn leave(&mut self, player_id: u32) {
        self.players.remove(&player_id);
        self.broadcast(&ServerMessage::PlayerLeft { player_id });
    }

    /// `player_id` の人の操作を盤面に反映し、結果を全員に送る。
    ///
    /// # Errors
    ///
    /// `game_core` が断った操作（盤面の外・勝敗がついたあと）や、盤面の外へのカーソルの移動。
    /// 盤面は変わらず、誰にも送らない。
    pub fn handle(&mut self, player_id: u32, message: ClientMessage) -> Result<(), Error> {
        match message {
            ClientMessage::PlayerMove { x, y } => {
                if x >= WIDTH || y >= HEIGHT {
                    return Err(Error::OutOfBounds { x, y });
                }
                self.broadcast_others(player_id, &ServerMessage::PlayerMoved { player_id, x, y });
                Ok(())
            }
            ClientMessage::RevealCell { x, y } => self.reveal(x, y),
            ClientMessage::ToggleFlag { x, y } => {
                match self.game.toggle_flag(x, y)? {
                    CellState::Flagged => {
                        self.broadcast(&ServerMessage::FlagToggled {
                            x,
                            y,
                            flagged: true,
                        });
                    }
                    CellState::Hidden => {
                        self.broadcast(&ServerMessage::FlagToggled {
                            x,
                            y,
                            flagged: false,
                        });
                    }
                    // 開いているマスは旗が変わらないので、何も送らない
                    CellState::Open => {}
                }
                Ok(())
            }
            ClientMessage::ResetGame => {
                self.seed = self.seed.wrapping_add(1);
                self.game = Game::new(self.seed);
                self.broadcast(&ServerMessage::GameReset);
                Ok(())
            }
        }
    }

    fn reveal(&mut self, x: usize, y: usize) -> Result<(), Error> {
        let before = BoardView::from_game(&self.game)?;
        let status = self.game.open(x, y)?;
        let after = BoardView::from_game(&self.game)?;
        match status {
            // 負けたときに開くのは地雷1つだけ。地雷の位置は game_over でまとめて送る
            Status::Lost => {}
            Status::Playing | Status::Won => {
                let cells = after.newly_opened_since(&before);
                if !cells.is_empty() {
                    self.broadcast(&ServerMessage::CellsRevealed { cells });
                }
            }
        }
        if let (Status::Won | Status::Lost, Some(mines)) = (status, after.mines) {
            self.broadcast(&ServerMessage::GameOver { status, mines });
        }
        Ok(())
    }

    /// 繋がっている全員（操作した本人も含む）に送る。切れている人には届かないが、放っておく。
    fn broadcast(&self, message: &ServerMessage) {
        for tx in self.players.values() {
            let _ = tx.send(message.clone());
        }
    }

    /// `except` の人以外の、繋がっている全員に送る。
    fn broadcast_others(&self, except: u32, message: &ServerMessage) {
        for (_, tx) in self.players.iter().filter(|&(&id, _)| id != except) {
            let _ = tx.send(message.clone());
        }
    }
}
