//! マウス操作を、サーバーに送る操作に変える判断。画面（web-sys）には依存しない。

use crate::layout::Layout;
use game_core::ClientMessage;

/// 使うマウスのボタン。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Button {
    /// 左ボタン。マスを開く。
    Left,
    /// 右ボタン。旗を立てる・取り消す。
    Right,
}

/// ブラウザの `MouseEvent.button`（0 が左、2 が右）から、使うボタンを決める。それ以外は使わない。
#[must_use]
pub const fn button_from_dom(code: i16) -> Option<Button> {
    match code {
        0 => Some(Button::Left),
        2 => Some(Button::Right),
        _ => None,
    }
}

/// 画面上の位置 `(px, py)` を `button` でクリックしたときに、サーバーへ送る操作。
/// 左は「開く」、右は「旗」。盤面の外なら送らない。
#[must_use]
pub fn click_message(layout: &Layout, button: Button, px: f64, py: f64) -> Option<ClientMessage> {
    let (x, y) = layout.cell_at(px, py)?;
    Some(match button {
        Button::Left => ClientMessage::RevealCell { x, y },
        Button::Right => ClientMessage::ToggleFlag { x, y },
    })
}

/// カーソルが画面上の位置 `(px, py)` に動いたときに、サーバーへ送る操作。
///
/// `last` は、最後に送ったマス。同じマスの上を動いている間は送らず、通信を減らす。
/// 盤面の外に出たときも送らない（最後に送ったマスの表示が、ほかの人には残る）。
#[must_use]
pub fn cursor_message(
    layout: &Layout,
    last: &mut Option<(usize, usize)>,
    px: f64,
    py: f64,
) -> Option<ClientMessage> {
    let (x, y) = layout.cell_at(px, py)?;
    if *last == Some((x, y)) {
        return None;
    }
    *last = Some((x, y));
    Some(ClientMessage::PlayerMove { x, y })
}

#[cfg(test)]
mod tests {
    use super::*;

    const LAYOUT: Layout = Layout {
        origin_x: 10.0,
        origin_y: 20.0,
        cell_size: 30.0,
    };

    #[test]
    fn dom_buttons_map_to_left_and_right_only() {
        assert_eq!(button_from_dom(0), Some(Button::Left));
        assert_eq!(button_from_dom(2), Some(Button::Right));
        for code in [-1, 1, 3, 4, 100] {
            assert_eq!(button_from_dom(code), None, "{code}");
        }
    }

    #[test]
    fn left_click_reveals_and_right_click_flags_the_clicked_cell() {
        // (2, 3) のマスの中: x は 10 + 2 * 30 .. 100、y は 20 + 3 * 30 .. 140
        let (px, py) = (75.0, 125.0);
        assert_eq!(
            click_message(&LAYOUT, Button::Left, px, py),
            Some(ClientMessage::RevealCell { x: 2, y: 3 })
        );
        assert_eq!(
            click_message(&LAYOUT, Button::Right, px, py),
            Some(ClientMessage::ToggleFlag { x: 2, y: 3 })
        );
    }

    #[test]
    fn clicks_outside_the_board_send_nothing() {
        for (px, py) in [(5.0, 25.0), (25.0, 5.0), (490.0, 30.0), (30.0, 500.0)] {
            for button in [Button::Left, Button::Right] {
                assert_eq!(click_message(&LAYOUT, button, px, py), None);
            }
        }
    }

    #[test]
    fn cursor_is_sent_only_when_it_enters_a_different_cell() {
        let mut last = None;
        // 最初のマスに入ったときに送る。同じマスの中では送らない
        assert_eq!(
            cursor_message(&LAYOUT, &mut last, 15.0, 25.0),
            Some(ClientMessage::PlayerMove { x: 0, y: 0 })
        );
        assert_eq!(cursor_message(&LAYOUT, &mut last, 20.0, 30.0), None);
        assert_eq!(cursor_message(&LAYOUT, &mut last, 39.0, 49.0), None);
        // 隣のマスに入ったら送る
        assert_eq!(
            cursor_message(&LAYOUT, &mut last, 45.0, 30.0),
            Some(ClientMessage::PlayerMove { x: 1, y: 0 })
        );
        // 盤面の外に出ても送らない。同じマスに戻っても、ほかの人の表示はそのままなので送らない
        assert_eq!(cursor_message(&LAYOUT, &mut last, 5.0, 30.0), None);
        assert_eq!(cursor_message(&LAYOUT, &mut last, 45.0, 30.0), None);
        // ほかのマスに移れば、また送る
        assert_eq!(
            cursor_message(&LAYOUT, &mut last, 15.0, 30.0),
            Some(ClientMessage::PlayerMove { x: 0, y: 0 })
        );
    }
}
