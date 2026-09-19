use game_core::{CellView, HEIGHT, ServerMessage, Status, WIDTH};
use std::collections::{BTreeMap, BTreeSet};

/// 1マスの、プレイヤーに見える姿。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Appearance {
    /// まだ開いていない。
    Hidden,
    /// 旗が立っている。
    Flagged,
    /// 開いている。周り8マスの地雷の数つき。
    Open(usize),
    /// 勝敗がついたあとに見える、旗のない地雷。爆発したマスも、これで表す。
    Mine,
}

/// クライアントが持つ盤面と、ほかのプレイヤーの様子。
///
/// サーバーから届いたメッセージを [`apply`](Self::apply) で順に当てはめると、
/// サーバーの見えている盤面と同じになる。
///
/// ```rust
/// use client::{Appearance, ClientState};
/// use game_core::ServerMessage;
///
/// let mut state = ClientState::default();
/// state.apply(&ServerMessage::FlagToggled { x: 3, y: 4, flagged: true });
/// assert_eq!(state.appearance(3, 4), Some(Appearance::Flagged));
/// ```
#[derive(Debug, Clone)]
pub struct ClientState {
    cells: Vec<CellView>,
    status: Status,
    /// 地雷の位置。勝敗がつくまでは空。
    mines: BTreeSet<(usize, usize)>,
    player_id: Option<u32>,
    players: BTreeSet<u32>,
    cursors: BTreeMap<u32, (usize, usize)>,
}

impl Default for ClientState {
    /// サーバーから何も届いていない状態。全マスが閉じていて、勝敗はついていない。
    fn default() -> Self {
        Self {
            cells: vec![CellView::Hidden; WIDTH * HEIGHT],
            status: Status::Playing,
            mines: BTreeSet::new(),
            player_id: None,
            players: BTreeSet::new(),
            cursors: BTreeMap::new(),
        }
    }
}

impl ClientState {
    /// サーバーから届いたメッセージを当てはめる。
    ///
    /// 盤面の外のマスを指す知らせは無視する。
    pub fn apply(&mut self, message: &ServerMessage) {
        match message {
            ServerMessage::Init {
                player_id,
                board,
                players,
            } => {
                self.cells.clone_from(&board.cells);
                self.status = board.status;
                self.mines = board.mines.iter().flatten().copied().collect();
                self.player_id = Some(*player_id);
                // 先にいた人（players）と、自分
                self.players = players.iter().copied().chain([*player_id]).collect();
                self.cursors.clear();
            }
            ServerMessage::PlayerJoined { player_id } => {
                self.players.insert(*player_id);
            }
            ServerMessage::PlayerLeft { player_id } => {
                self.players.remove(player_id);
                self.cursors.remove(player_id);
            }
            ServerMessage::CellsRevealed { cells } => {
                for revealed in cells {
                    if let Some(cell) = self.cell_mut(revealed.x, revealed.y) {
                        *cell = CellView::Open {
                            adjacent: revealed.adjacent,
                        };
                    }
                }
            }
            ServerMessage::FlagToggled { x, y, flagged } => {
                if let Some(cell) = self.cell_mut(*x, *y)
                    && !matches!(cell, CellView::Open { .. })
                {
                    *cell = if *flagged {
                        CellView::Flagged
                    } else {
                        CellView::Hidden
                    };
                }
            }
            ServerMessage::GameOver { status, mines } => {
                self.status = *status;
                self.mines = mines.iter().copied().collect();
            }
            ServerMessage::GameReset => {
                self.cells = vec![CellView::Hidden; WIDTH * HEIGHT];
                self.status = Status::Playing;
                self.mines.clear();
            }
            ServerMessage::PlayerMoved { player_id, x, y } => {
                self.cursors.insert(*player_id, (*x, *y));
            }
        }
    }

    /// マス `(x, y)` の見た目。盤面の外なら `None`。
    #[must_use]
    pub fn appearance(&self, x: usize, y: usize) -> Option<Appearance> {
        if x >= WIDTH || y >= HEIGHT {
            return None;
        }
        Some(match self.cells.get(y * WIDTH + x)? {
            CellView::Flagged => Appearance::Flagged,
            _ if self.mines.contains(&(x, y)) => Appearance::Mine,
            CellView::Open { adjacent } => Appearance::Open(*adjacent),
            CellView::Hidden => Appearance::Hidden,
        })
    }

    /// 勝敗。
    #[must_use]
    pub const fn status(&self) -> Status {
        self.status
    }

    /// 自分の番号。サーバーから `init` が届くまでは `None`。
    #[must_use]
    pub const fn player_id(&self) -> Option<u32> {
        self.player_id
    }

    /// 繋がっている人数（自分を含む）。自分が入る前からいた人も、`init` で知らされて数える。
    #[must_use]
    pub fn player_count(&self) -> usize {
        self.players.len()
    }

    /// ほかのプレイヤーのカーソル `(プレイヤーの番号, (列, 行))`。番号の小さい順。
    pub fn cursors(&self) -> impl Iterator<Item = (u32, (usize, usize))> {
        self.cursors.iter().map(|(&id, &position)| (id, position))
    }

    fn cell_mut(&mut self, x: usize, y: usize) -> Option<&mut CellView> {
        if x >= WIDTH || y >= HEIGHT {
            return None;
        }
        self.cells.get_mut(y * WIDTH + x)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use game_core::{BoardView, CellState, Error, Game};

    type TestResult = Result<(), Box<dyn std::error::Error>>;

    /// サーバー（`server/src/room.rs`）が送る知らせを、手元の `Game` から同じ規則で作る。
    /// サーバーの見えている盤面は、この `game` そのもの。
    struct FakeServer {
        game: Game,
        seed: u64,
    }

    impl FakeServer {
        fn new(seed: u64) -> Self {
            Self {
                game: Game::new(seed),
                seed,
            }
        }

        fn init(&self, player_id: u32) -> Result<ServerMessage, Error> {
            Ok(ServerMessage::Init {
                player_id,
                board: BoardView::from_game(&self.game)?,
                players: Vec::new(),
            })
        }

        /// 開く。`game_core` が断った操作は、サーバーと同じく、誰にも送らない。
        fn reveal(&mut self, x: usize, y: usize) -> Result<Vec<ServerMessage>, Error> {
            let before = BoardView::from_game(&self.game)?;
            let Ok(status) = self.game.open(x, y) else {
                return Ok(Vec::new());
            };
            let after = BoardView::from_game(&self.game)?;
            let mut messages = Vec::new();
            // 負けたときに開くのは地雷1つだけ。地雷の位置は game_over でまとめて送る
            if status != Status::Lost {
                let cells = after.newly_opened_since(&before);
                if !cells.is_empty() {
                    messages.push(ServerMessage::CellsRevealed { cells });
                }
            }
            if let Some(mines) = after.mines {
                messages.push(ServerMessage::GameOver { status, mines });
            }
            Ok(messages)
        }

        fn toggle_flag(&mut self, x: usize, y: usize) -> Vec<ServerMessage> {
            match self.game.toggle_flag(x, y) {
                Ok(CellState::Flagged) => vec![ServerMessage::FlagToggled {
                    x,
                    y,
                    flagged: true,
                }],
                Ok(CellState::Hidden) => vec![ServerMessage::FlagToggled {
                    x,
                    y,
                    flagged: false,
                }],
                _ => Vec::new(),
            }
        }

        fn reset(&mut self) -> Vec<ServerMessage> {
            self.seed = self.seed.wrapping_add(1);
            self.game = Game::new(self.seed);
            vec![ServerMessage::GameReset]
        }
    }

    /// サーバーの見えている盤面を、`Game` の問い合わせから1マスずつ求める。
    /// 勝敗がついたあとは、旗のない地雷が見える。
    fn expected_grid(game: &Game) -> Result<Vec<Appearance>, Error> {
        let decided = game.status() != Status::Playing;
        let mut grid = Vec::new();
        for y in 0..HEIGHT {
            for x in 0..WIDTH {
                grid.push(match game.state(x, y)? {
                    CellState::Flagged => Appearance::Flagged,
                    _ if decided && game.is_mine(x, y)? => Appearance::Mine,
                    CellState::Open => Appearance::Open(game.adjacent_mines(x, y)?),
                    CellState::Hidden => Appearance::Hidden,
                });
            }
        }
        Ok(grid)
    }

    fn client_grid(client: &ClientState) -> Vec<Appearance> {
        (0..HEIGHT)
            .flat_map(|y| (0..WIDTH).map(move |x| (x, y)))
            .filter_map(|(x, y)| client.appearance(x, y))
            .collect()
    }

    /// クライアントの盤面が、サーバーの見えている盤面と一致することを確かめる。
    fn assert_same_board(client: &ClientState, server: &FakeServer, at: &str) -> TestResult {
        assert_eq!(client_grid(client), expected_grid(&server.game)?, "{at}");
        assert_eq!(client.status(), server.game.status(), "{at}");
        Ok(())
    }

    fn apply_all(client: &mut ClientState, messages: &[ServerMessage]) {
        for message in messages {
            client.apply(message);
        }
    }

    /// 決まった列を返す乱数（テスト用）。
    struct Rng(u64);

    impl Rng {
        fn below(&mut self, bound: u64) -> u64 {
            self.0 ^= self.0 << 13;
            self.0 ^= self.0 >> 7;
            self.0 ^= self.0 << 17;
            self.0 % bound
        }
    }

    /// 乱数で選んだ操作を1つ行い、サーバーが送る知らせを返す。
    fn random_step(
        server: &mut FakeServer,
        rng: &mut Rng,
    ) -> Result<Vec<ServerMessage>, Box<dyn std::error::Error>> {
        let x = usize::try_from(rng.below(16))?;
        let y = usize::try_from(rng.below(16))?;
        Ok(match rng.below(20) {
            0 => server.reset(),
            1..=5 => server.toggle_flag(x, y),
            _ => server.reveal(x, y)?,
        })
    }

    /// ⑫: 届いたメッセージを順に当てはめると、クライアントの盤面がサーバーの見えている盤面と一致する
    /// （乱数で開く・旗・リセットを繰り返し、1手ごとに比べる）
    #[test]
    fn item12_client_board_matches_the_server_board_after_every_step() -> TestResult {
        let (mut lost_seeds, mut reset_seeds, mut flagged_seeds) = (0, 0, 0);
        for seed in 0..40 {
            let mut server = FakeServer::new(seed);
            let mut client = ClientState::default();
            client.apply(&server.init(1)?);
            assert_same_board(&client, &server, &format!("seed={seed} init"))?;

            let mut rng = Rng(seed + 1);
            let (mut lost, mut reset, mut flagged) = (false, false, false);
            for step in 0..300 {
                let messages = random_step(&mut server, &mut rng)?;
                reset |= messages.contains(&ServerMessage::GameReset);
                flagged |= client_grid(&client).contains(&Appearance::Flagged);
                apply_all(&mut client, &messages);
                lost |= server.game.status() == Status::Lost;
                assert_same_board(&client, &server, &format!("seed={seed} step={step}"))?;
            }
            lost_seeds += usize::from(lost);
            reset_seeds += usize::from(reset);
            flagged_seeds += usize::from(flagged);
        }
        // 比べる場面が偏っていないことも確かめる（負け・リセット・旗が、それぞれ何度も起きている）
        assert!(lost_seeds >= 10, "負けた盤面が少ない: {lost_seeds}");
        assert!(reset_seeds >= 10, "リセットした盤面が少ない: {reset_seeds}");
        assert!(
            flagged_seeds >= 10,
            "旗を立てた盤面が少ない: {flagged_seeds}"
        );
        Ok(())
    }

    /// ⑫: 勝ったときも、クライアントの盤面がサーバーの見えている盤面と一致する
    #[test]
    fn item12_client_board_matches_the_server_board_after_winning() -> TestResult {
        let mut server = FakeServer::new(3);
        let mut client = ClientState::default();
        client.apply(&server.init(1)?);
        apply_all(&mut client, &server.reveal(8, 8)?);
        for y in 0..HEIGHT {
            for x in 0..WIDTH {
                if server.game.is_mine(x, y)? || server.game.state(x, y)? != CellState::Hidden {
                    continue;
                }
                apply_all(&mut client, &server.reveal(x, y)?);
                assert_same_board(&client, &server, &format!("({x}, {y})"))?;
            }
        }
        assert_eq!(client.status(), Status::Won);
        // 勝ったあとは、地雷が見える
        let mines = (0..HEIGHT)
            .flat_map(|y| (0..WIDTH).map(move |x| (x, y)))
            .filter(|&(x, y)| client.appearance(x, y) == Some(Appearance::Mine))
            .count();
        assert_eq!(mines, game_core::MINE_COUNT);
        Ok(())
    }

    /// ⑫: 途中から入ったクライアントも、`init` のあとの知らせを当てはめれば、最初からいたクライアントと同じ盤面になる
    #[test]
    fn item12_client_joining_midway_matches_the_server_board() -> TestResult {
        for seed in 0..20 {
            let mut server = FakeServer::new(seed);
            let mut rng = Rng(seed + 100);
            let mut early = ClientState::default();
            early.apply(&server.init(1)?);
            for _ in 0..40 {
                apply_all(&mut early, &random_step(&mut server, &mut rng)?);
            }
            let mut late = ClientState::default();
            late.apply(&server.init(2)?);
            assert_same_board(&late, &server, &format!("seed={seed} 入った直後"))?;
            for step in 0..40 {
                let messages = random_step(&mut server, &mut rng)?;
                apply_all(&mut early, &messages);
                apply_all(&mut late, &messages);
                assert_same_board(&early, &server, &format!("seed={seed} step={step} early"))?;
                assert_same_board(&late, &server, &format!("seed={seed} step={step} late"))?;
            }
        }
        Ok(())
    }

    /// ⑫: 負けたあとは、爆発したマスも含めて、地雷がすべて見える
    #[test]
    fn item12_after_losing_every_mine_is_visible() -> TestResult {
        let mut server = FakeServer::new(5);
        let mut client = ClientState::default();
        client.apply(&server.init(1)?);
        apply_all(&mut client, &server.reveal(8, 8)?);
        let mine = (0..HEIGHT)
            .flat_map(|y| (0..WIDTH).map(move |x| (x, y)))
            .find(|&(x, y)| server.game.is_mine(x, y) == Ok(true))
            .ok_or("地雷がない")?;
        server.toggle_flag(mine.0, mine.1);
        // 旗を立てた地雷は旗のまま。別の地雷を開いて負ける
        let other = (0..HEIGHT)
            .flat_map(|y| (0..WIDTH).map(move |x| (x, y)))
            .find(|&p| p != mine && server.game.is_mine(p.0, p.1) == Ok(true))
            .ok_or("地雷が1つしかない")?;
        apply_all(
            &mut client,
            &[ServerMessage::FlagToggled {
                x: mine.0,
                y: mine.1,
                flagged: true,
            }],
        );
        apply_all(&mut client, &server.reveal(other.0, other.1)?);
        assert_eq!(client.status(), Status::Lost);
        assert_eq!(client.appearance(other.0, other.1), Some(Appearance::Mine));
        assert_eq!(client.appearance(mine.0, mine.1), Some(Appearance::Flagged));
        assert_same_board(&client, &server, "負けたあと")?;
        Ok(())
    }

    #[test]
    fn a_fresh_client_shows_a_closed_board_and_ignores_cells_outside() {
        let mut client = ClientState::default();
        assert_eq!(client.status(), Status::Playing);
        assert_eq!(client.player_id(), None);
        assert_eq!(
            client_grid(&client),
            vec![Appearance::Hidden; WIDTH * HEIGHT]
        );
        assert_eq!(client.appearance(WIDTH, 0), None);
        assert_eq!(client.appearance(0, HEIGHT), None);
        // 盤面の外を指す知らせは、何も変えない
        client.apply(&ServerMessage::FlagToggled {
            x: WIDTH,
            y: 0,
            flagged: true,
        });
        client.apply(&ServerMessage::CellsRevealed {
            cells: vec![game_core::RevealedCell {
                x: 0,
                y: HEIGHT,
                adjacent: 1,
            }],
        });
        assert_eq!(
            client_grid(&client),
            vec![Appearance::Hidden; WIDTH * HEIGHT]
        );
    }

    #[test]
    fn players_and_cursors_follow_the_messages() -> TestResult {
        let server = FakeServer::new(1);
        let mut client = ClientState::default();
        client.apply(&server.init(7)?);
        assert_eq!(client.player_id(), Some(7));
        assert_eq!(client.player_count(), 1);

        client.apply(&ServerMessage::PlayerJoined { player_id: 8 });
        client.apply(&ServerMessage::PlayerJoined { player_id: 9 });
        assert_eq!(client.player_count(), 3);

        client.apply(&ServerMessage::PlayerMoved {
            player_id: 9,
            x: 2,
            y: 3,
        });
        client.apply(&ServerMessage::PlayerMoved {
            player_id: 8,
            x: 5,
            y: 5,
        });
        client.apply(&ServerMessage::PlayerMoved {
            player_id: 9,
            x: 4,
            y: 3,
        });
        assert_eq!(
            client.cursors().collect::<Vec<_>>(),
            vec![(8, (5, 5)), (9, (4, 3))]
        );

        // 抜けた人は、人数からもカーソルからも消える
        client.apply(&ServerMessage::PlayerLeft { player_id: 9 });
        assert_eq!(client.player_count(), 2);
        assert_eq!(client.cursors().collect::<Vec<_>>(), vec![(8, (5, 5))]);
        Ok(())
    }

    /// サーバーの部屋と、そこに繋がっているクライアントたち（テスト用）。
    /// 参加・退出のたびに、サーバー（`server/src/room.rs`）と同じ規則で、`init` と知らせを作って届ける。
    struct FakeRoom {
        game: Game,
        next_id: u32,
        /// 部屋にいる人（番号 → その人の画面）。
        clients: BTreeMap<u32, ClientState>,
    }

    impl FakeRoom {
        fn new(seed: u64) -> Self {
            Self {
                game: Game::new(seed),
                next_id: 1,
                clients: BTreeMap::new(),
            }
        }

        /// 新しい人が入る。本人には、今いる人（自分以外）つきの `init` を、ほかの人には `player_joined` を届ける。
        fn join(&mut self) -> Result<u32, Error> {
            let player_id = self.next_id;
            self.next_id += 1;
            let init = ServerMessage::Init {
                player_id,
                board: BoardView::from_game(&self.game)?,
                players: self.clients.keys().copied().collect(),
            };
            for client in self.clients.values_mut() {
                client.apply(&ServerMessage::PlayerJoined { player_id });
            }
            let mut client = ClientState::default();
            client.apply(&init);
            self.clients.insert(player_id, client);
            Ok(player_id)
        }

        /// 人が抜ける。残った人に `player_left` を届ける。
        fn leave(&mut self, player_id: u32) {
            self.clients.remove(&player_id);
            for client in self.clients.values_mut() {
                client.apply(&ServerMessage::PlayerLeft { player_id });
            }
        }

        /// どの人の画面でも、参加者の数が、部屋にいる人数と一致することを確かめる。
        fn assert_counts_match(&self, at: &str) {
            for (id, client) in &self.clients {
                assert_eq!(client.player_id(), Some(*id), "{at}");
                assert_eq!(
                    client.player_count(),
                    self.clients.len(),
                    "{at}: 番号 {id} の画面の人数"
                );
            }
        }
    }

    /// ⑭: 参加者の数が、先にいた人の画面でも、あとから入った人の画面でも、サーバーにいる人数と一致する
    #[test]
    fn item14_participant_count_matches_for_early_and_late_joiners() -> TestResult {
        let mut room = FakeRoom::new(1);
        // 1人目は自分だけ。2人目・3人目が入るたびに、先にいた人の数も、入った人の数も増える
        for expected in 1..=3 {
            room.join()?;
            assert_eq!(room.clients.len(), expected);
            room.assert_counts_match(&format!("{expected}人目が入った"));
        }
        // 3人目（あとから入った人）の画面は、最初から3人になっている
        let latest = *room.clients.keys().next_back().ok_or("誰もいない")?;
        assert_eq!(room.clients[&latest].player_count(), 3);

        // 真ん中の人が抜けると、残った全員の画面が2人になる
        room.leave(2);
        room.assert_counts_match("2人目が抜けた");
        assert_eq!(room.clients.len(), 2);

        // 抜けたあとに入った人の画面には、抜けた人は数えられない（3人）
        let late = room.join()?;
        room.assert_counts_match("抜けたあとに入った");
        assert_eq!(room.clients[&late].player_count(), 3);

        // 最後の1人になっても合う
        for id in [1, 3] {
            room.leave(id);
            room.assert_counts_match("さらに抜けた");
        }
        assert_eq!(room.clients[&late].player_count(), 1);
        Ok(())
    }

    /// ⑭: 参加と退出を乱数で繰り返しても、1手ごとに、どの人の画面でも人数が合う
    #[test]
    fn item14_participant_count_matches_after_random_joins_and_leaves() -> TestResult {
        let (mut joins_with_others, mut leaves) = (0, 0);
        for seed in 0..30 {
            let mut room = FakeRoom::new(seed);
            let mut rng = Rng(seed + 1);
            for step in 0..80 {
                let crowd = room.clients.len();
                if crowd == 0 || (crowd < 8 && rng.below(3) != 0) {
                    joins_with_others += usize::from(crowd > 0);
                    room.join()?;
                } else {
                    let ids: Vec<u32> = room.clients.keys().copied().collect();
                    room.leave(ids[usize::try_from(rng.below(u64::try_from(ids.len())?))?]);
                    leaves += 1;
                }
                room.assert_counts_match(&format!("seed={seed} step={step}"));
            }
        }
        // 比べる場面が偏っていないことも確かめる（先にいる人がいる中での参加と、退出が、何度も起きている）
        assert!(
            joins_with_others >= 300,
            "参加が少ない: {joins_with_others}"
        );
        assert!(leaves >= 300, "退出が少ない: {leaves}");
        Ok(())
    }
}
