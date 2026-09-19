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
- [ ] ⑨を満たす（勝敗がつくまで、地雷の位置を送らない）
- [ ] 点検: test-reviewer に点検させ、指摘を壊し方で確かめて直す（最大2回）
- [ ] 【えだ】差分を読んで push する
