# ToDo

上から順に、`/next-todo` で進める。1項目＝1ループ＋1コミット。【えだ】の付いた項目は、えだがチェックを入れる。

## 1周目・1段目: game_core

- [x] ルールとテスト①〜⑥を作る（2d3110c）
- [x] テストを強める（点検の指摘を、壊し方で確かめる）（42ad651）
- [x] 【えだ】差分を読んで push する

## 1周目・2段目: server とボット

- [x] server を作り、⑦を満たす（メッセージの型は game_core に足す）
  - メモ: axum の `/ws`・ポートは環境変数 `PORT`（なければ 8080）・部屋は1つ（全員が同じ盤面）。メッセージの種類と名前は day1 に合わせる（ブラウザ→サーバー: reveal_cell・toggle_flag・reset_game／サーバー→ブラウザ: init・player_joined・player_left・cells_revealed・flag_toggled・game_over・game_reset）。カーソルの2種（player_move・player_moved）は2周目で作る。game_core が返すエラーは、サーバーを止めずに無視する。テストでは、固定のシードを渡せるようにし、サーバーは空きポート（0番）で立て、ボットがメッセージを待つときは時間の上限を付ける。
  - 足してよい依存: serde（derive）・serde_json（game_core・server）、axum（ws）・tokio・anyhow（server）、tokio-tungstenite・futures-util（server のテスト用）
- [x] ⑧を満たす（途中参加の人に、見えている盤面が届く）
- [x] ⑨を満たす（勝敗がつくまで、地雷の位置を送らない）
- [x] 点検: test-reviewer に点検させ、指摘を壊し方で確かめて直す（最大2回）
- [x] 【えだ】差分を読んで push する

## 2周目: 画面

ゴール: ブラウザのタブ2つで、同じ盤面を協力して遊べる（day1 でできたことが全部できる）

- [x] 【えだ】wasm-pack と wasm32-unknown-unknown ターゲットを入れる
- [x] ⑨を補強する（勝ったときにも地雷の位置が届く／遊んでいる間に届く値に、地雷の位置が混ざらない）
- [x] server がページを配る（⑩）
  - メモ: static/index.html は手書きでコミットする。ビルド結果の static/pkg/ は git に入れない（.gitignore に足す）。server は static/ を配る。
  - 足してよい依存: tower-http（fs）
- [x] カーソルを中継する（⑪）
  - メモ: メッセージは day1 の player_move・player_moved。
- [x] client の、画面に依存しない部分を作る（⑫⑬）
  - メモ: 届いたメッセージで盤面を更新する処理と、クリックした位置をマスに変える処理を、web-sys を使わないモジュールに分けて、cargo test で確かめる。
- [x] client の描画と操作を作る
  - 完了条件: wasm-pack で client をビルドでき、結果が static/pkg/ に出る
  - メモ: day1 と同じ WASM＋Canvas。盤面・ほかのプレイヤーのカーソル（丸）・接続状態・人数・リセットボタン。左クリックで開き、右クリックで旗。Canvas に描くコードは薄くして、判断は⑫⑬の側に寄せる。接続先は location.host から組み立てる。
  - 足してよい依存: wasm-bindgen・web-sys・js-sys
- [x] 点検: test-reviewer に点検させ、指摘を壊し方で確かめて直す（最大2回）
- [x] 【えだ】タブ2つで遊んで確かめ、差分を読んで push する

## 2周目の追加: 参加者数

ゴール: あとから入った人の画面でも、参加者の数が正しく出る

- [x] ⑧を補強する（init に、今いる参加者を足す）
  - メモ: init に、自分以外の参加者の番号を入れる。⑨のテストにある「init に入れてよい項目名の一覧」に、この項目を足すことは、えだが確認済み（参加者の番号は、地雷の位置と関係がないため）。止まらずに変えてよい。
- [x] client が、init の参加者を人数に数える（⑭）
- [ ] 点検: test-reviewer に点検させ、指摘を壊し方で確かめて直す（最大2回）
- [ ] 【えだ】タブ2つで参加者の数を確かめ、差分を読んで push する
