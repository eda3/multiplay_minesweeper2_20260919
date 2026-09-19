//! マルチプレイマインスイーパーのクライアント。
//!
//! 画面（web-sys）に依存しない部分だけを持つ。届いたメッセージで盤面を更新する処理（[`state`]）と、
//! クリックした位置をマスに変える処理（[`layout`]）。どちらも `cargo test` で確かめられる。

pub mod layout;
pub mod state;

pub use layout::Layout;
pub use state::{Appearance, ClientState};
