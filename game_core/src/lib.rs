//! マルチプレイマインスイーパーのルール本体。
//!
//! 同期で書かれた純粋なルールだけを持つ。通信や描画は `server` / `client` の担当。
//! 乱数は外から受け取るシードだけで決まるので、同じシードなら同じ盤面になる。

mod error;
mod game;
mod message;
mod rng;

pub use error::Error;
pub use game::{CellState, Game, HEIGHT, MINE_COUNT, Status, WIDTH};
pub use message::{BoardView, CellView, ClientMessage, RevealedCell, ServerMessage};
