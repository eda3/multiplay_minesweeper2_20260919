//! マルチプレイマインスイーパーのクライアント。
//!
//! 判断は、画面（web-sys）に依存しない部分に置く。届いたメッセージで盤面を更新する処理（[`state`]）、
//! クリックした位置をマスに変える処理（[`layout`]）、マウス操作を送る操作に変える処理（[`input`]）。
//! どれも `cargo test` で確かめられる。ブラウザ（WASM）での描画と配線は、薄い `app` に任せる。

#[cfg(target_arch = "wasm32")]
mod app;
pub mod input;
pub mod layout;
pub mod state;

pub use layout::Layout;
pub use state::{Appearance, ClientState};
