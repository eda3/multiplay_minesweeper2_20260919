use std::fmt;

/// `game_core` の操作が失敗した理由。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Error {
    /// 盤面の外の座標が指定された。
    OutOfBounds {
        /// 指定された列。
        x: usize,
        /// 指定された行。
        y: usize,
    },
    /// 勝敗がついたあとに操作された。
    GameOver,
    /// メッセージの JSON が、読めない・作れない。
    InvalidMessage,
}

impl fmt::Display for Error {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::OutOfBounds { x, y } => write!(f, "座標 ({x}, {y}) は盤面の外です"),
            Self::GameOver => write!(f, "ゲームはすでに終了しています"),
            Self::InvalidMessage => write!(f, "メッセージの JSON が正しくありません"),
        }
    }
}

impl std::error::Error for Error {}
