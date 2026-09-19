//! ブラウザ（WASM）での画面表示と操作の配線。
//!
//! 判断は `state` / `layout` / `input` に任せて、ここは WebSocket・Canvas・DOM との受け渡しだけにする。
//! 接続先は、ページを配った場所（`location.host`）から組み立てる。

use crate::input;
use crate::layout::{CELL_PIXELS, Layout, board_pixels};
use crate::state::{Appearance, ClientState};
use game_core::{ClientMessage, HEIGHT, ServerMessage, Status, WIDTH};
use std::cell::RefCell;
use std::f64::consts::TAU;
use std::rc::Rc;
use wasm_bindgen::convert::FromWasmAbi;
use wasm_bindgen::prelude::*;
use web_sys::{
    CanvasRenderingContext2d, Element, Event, EventTarget, HtmlCanvasElement, MessageEvent,
    MouseEvent, WebSocket,
};

/// 周りの地雷の数（1〜8）ごとの文字の色。
const NUMBER_COLORS: [&str; 9] = [
    "", "#0000ff", "#008000", "#ff0000", "#000080", "#800000", "#008080", "#000000", "#808080",
];

/// 盤面の座標（0〜15）を、描画用の数にする。
fn to_f64(n: usize) -> f64 {
    f64::from(u32::try_from(n).unwrap_or(0))
}

struct App {
    state: ClientState,
    /// 最後にサーバーへ送ったカーソルのマス。
    last_cursor: Option<(usize, usize)>,
    socket: WebSocket,
    ctx: CanvasRenderingContext2d,
    connection: Element,
    players: Element,
    result: Element,
}

type Shared = Rc<RefCell<App>>;

impl App {
    fn send(&self, message: &ClientMessage) {
        if let Ok(json) = message.to_json() {
            // 接続前や切断後は送れない。そのときの操作は、捨てる
            let _ = self.socket.send_with_str(&json);
        }
    }
}

/// ページが読み込まれたときに、WASM の初期化のあとで呼ばれる。
///
/// # Errors
///
/// ページに必要な部品（Canvas など）がない、または接続を作れないとき。
#[wasm_bindgen(start)]
pub fn start() -> Result<(), JsValue> {
    let window = web_sys::window().ok_or("window がない")?;
    let document = window.document().ok_or("document がない")?;
    let element = |id: &str| {
        document
            .get_element_by_id(id)
            .ok_or_else(|| JsValue::from_str(&format!("#{id} がない")))
    };

    let canvas: HtmlCanvasElement = element("board")?.dyn_into()?;
    canvas.set_width(board_pixels(WIDTH));
    canvas.set_height(board_pixels(HEIGHT));
    let ctx: CanvasRenderingContext2d = canvas
        .get_context("2d")?
        .ok_or("2d コンテキストがない")?
        .dyn_into()?;

    let location = window.location();
    let scheme = if location.protocol()? == "https:" {
        "wss"
    } else {
        "ws"
    };
    let socket = WebSocket::new(&format!("{scheme}://{}/ws", location.host()?))?;

    let app: Shared = Rc::new(RefCell::new(App {
        state: ClientState::default(),
        last_cursor: None,
        socket: socket.clone(),
        ctx,
        connection: element("connection")?,
        players: element("players")?,
        result: element("result")?,
    }));

    // サーバーとの接続
    for name in ["open", "close"] {
        let app = Rc::clone(&app);
        listen(&socket, name, move |_: Event| render(&app.borrow()))?;
    }
    let shared = Rc::clone(&app);
    listen(&socket, "message", move |event: MessageEvent| {
        if let Some(text) = event.data().as_string()
            && let Ok(message) = ServerMessage::from_json(&text)
        {
            let mut app = shared.borrow_mut();
            app.state.apply(&message);
            render(&app);
        }
    })?;

    // マウス操作
    let shared = Rc::clone(&app);
    listen(&canvas, "mousemove", move |event: MouseEvent| {
        let mut app = shared.borrow_mut();
        let (px, py) = (f64::from(event.offset_x()), f64::from(event.offset_y()));
        if let Some(message) =
            input::cursor_message(&Layout::canvas(), &mut app.last_cursor, px, py)
        {
            app.send(&message);
        }
    })?;
    let shared = Rc::clone(&app);
    listen(&canvas, "mousedown", move |event: MouseEvent| {
        let (px, py) = (f64::from(event.offset_x()), f64::from(event.offset_y()));
        if let Some(button) = input::button_from_dom(event.button())
            && let Some(message) = input::click_message(&Layout::canvas(), button, px, py)
        {
            shared.borrow().send(&message);
        }
    })?;
    // 右クリックで、ブラウザのメニューを出さない
    listen(&canvas, "contextmenu", |event: Event| {
        event.prevent_default();
    })?;

    // リセットボタン
    let reset = element("reset")?;
    let shared = Rc::clone(&app);
    listen(&reset, "click", move |_: Event| {
        shared.borrow().send(&ClientMessage::ResetGame);
    })?;

    render(&app.borrow());
    Ok(())
}

/// `target` の `event` に `handler` をつなぐ。
fn listen<E>(
    target: &EventTarget,
    event: &str,
    handler: impl FnMut(E) + 'static,
) -> Result<(), JsValue>
where
    E: FromWasmAbi + 'static,
{
    let closure = Closure::<dyn FnMut(E)>::new(handler);
    target.add_event_listener_with_callback(event, closure.as_ref().unchecked_ref())?;
    // ページが開いている間ずっと使うので、解放せずに残す
    closure.forget();
    Ok(())
}

/// 盤面・カーソルと、接続状態・人数・勝敗の文字を描き直す。
fn render(app: &App) {
    draw_board(&app.ctx, &app.state);
    app.connection
        .set_text_content(Some(match app.socket.ready_state() {
            WebSocket::CONNECTING => "接続しています…",
            WebSocket::OPEN => "接続中",
            _ => "切断されました",
        }));
    let players = if app.state.player_id().is_some() {
        format!("参加者: {}人", app.state.player_count())
    } else {
        String::new()
    };
    app.players.set_text_content(Some(&players));
    app.result.set_text_content(Some(match app.state.status() {
        Status::Playing => "",
        Status::Won => "勝ち！",
        Status::Lost => "負け…",
    }));
}

fn draw_board(ctx: &CanvasRenderingContext2d, state: &ClientState) {
    let size = f64::from(CELL_PIXELS);
    ctx.set_font("20px sans-serif");
    ctx.set_text_align("center");
    ctx.set_text_baseline("middle");
    for y in 0..HEIGHT {
        for x in 0..WIDTH {
            let (left, top) = (to_f64(x) * size, to_f64(y) * size);
            let appearance = state.appearance(x, y).unwrap_or(Appearance::Hidden);
            ctx.set_fill_style_str(match appearance {
                Appearance::Open(_) => "#e8e8e8",
                Appearance::Mine => "#f4b0b0",
                Appearance::Hidden | Appearance::Flagged => "#b8b8b8",
            });
            ctx.fill_rect(left, top, size, size);
            ctx.set_stroke_style_str("#888888");
            ctx.stroke_rect(left, top, size, size);

            let (text, color) = match appearance {
                Appearance::Flagged => ("🚩".to_owned(), "#000000"),
                Appearance::Mine => ("💣".to_owned(), "#000000"),
                Appearance::Open(count) if count > 0 => {
                    (count.to_string(), NUMBER_COLORS[count.min(8)])
                }
                Appearance::Open(_) | Appearance::Hidden => continue,
            };
            ctx.set_fill_style_str(color);
            let _ = ctx.fill_text(&text, left + size / 2.0, top + size / 2.0);
        }
    }

    // ほかのプレイヤーのカーソル（番号ごとに色を変えた丸）
    for (player_id, (x, y)) in state.cursors() {
        ctx.begin_path();
        let _ = ctx.arc(
            (to_f64(x) + 0.5) * size,
            (to_f64(y) + 0.5) * size,
            size * 0.3,
            0.0,
            TAU,
        );
        ctx.set_fill_style_str(&format!(
            "hsla({}, 80%, 50%, 0.6)",
            player_id.wrapping_mul(67) % 360
        ));
        ctx.fill();
    }
}
