use crate::error::Error;
use crate::rng::Rng;
use serde::{Deserialize, Serialize};

/// 盤面の列数。
pub const WIDTH: usize = 16;
/// 盤面の行数。
pub const HEIGHT: usize = 16;
/// 盤面に置く地雷の数。
pub const MINE_COUNT: usize = 40;

/// マスの見た目上の状態。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CellState {
    /// まだ開いていない。
    Hidden,
    /// 旗が立っている。旗のマスは開かない。
    Flagged,
    /// 開いている。
    Open,
}

/// 勝敗の状態。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Status {
    /// 進行中。
    Playing,
    /// 地雷以外をすべて開いた。
    Won,
    /// 地雷を開いた。
    Lost,
}

#[derive(Debug, Clone)]
struct Cell {
    mine: bool,
    state: CellState,
}

/// 1枚の盤面と、その進行状況。
///
/// 地雷は最初にマスを開いたときに置かれる。置き方はシードだけで決まる。
#[derive(Debug, Clone)]
pub struct Game {
    seed: u64,
    cells: Vec<Cell>,
    mines_placed: bool,
    status: Status,
}

impl Game {
    /// 地雷がまだ置かれていない新しい盤面を作る。
    ///
    /// 同じ `seed` なら、同じマスを最初に開いたときに同じ盤面になる。
    ///
    /// ```rust
    /// use game_core::{Game, Status};
    ///
    /// let game = Game::new(42);
    /// assert_eq!(game.status(), Status::Playing);
    /// ```
    #[must_use]
    pub fn new(seed: u64) -> Self {
        Self {
            seed,
            cells: vec![
                Cell {
                    mine: false,
                    state: CellState::Hidden,
                };
                WIDTH * HEIGHT
            ],
            mines_placed: false,
            status: Status::Playing,
        }
    }

    /// 現在の勝敗を返す。
    #[must_use]
    pub const fn status(&self) -> Status {
        self.status
    }

    /// マス `(x, y)` の状態を返す。
    ///
    /// # Errors
    ///
    /// 座標が盤面の外なら [`Error::OutOfBounds`]。
    pub fn state(&self, x: usize, y: usize) -> Result<CellState, Error> {
        Ok(self.cells[Self::index(x, y)?].state)
    }

    /// マス `(x, y)` に地雷があるかを返す。最初のマスを開くまでは常に `false`。
    ///
    /// # Errors
    ///
    /// 座標が盤面の外なら [`Error::OutOfBounds`]。
    pub fn is_mine(&self, x: usize, y: usize) -> Result<bool, Error> {
        Ok(self.cells[Self::index(x, y)?].mine)
    }

    /// マス `(x, y)` の周り8マスにある地雷の数を返す。マス自身は数えない。
    ///
    /// # Errors
    ///
    /// 座標が盤面の外なら [`Error::OutOfBounds`]。
    pub fn adjacent_mines(&self, x: usize, y: usize) -> Result<usize, Error> {
        Self::index(x, y)?;
        Ok(self.count_adjacent(x, y))
    }

    /// マス `(x, y)` の旗を立てる。立っていれば取り消す。新しい状態を返す。
    /// 開いているマスには何もしない。
    ///
    /// ```rust
    /// use game_core::{CellState, Game};
    ///
    /// let mut game = Game::new(1);
    /// assert_eq!(game.toggle_flag(3, 4)?, CellState::Flagged);
    /// assert_eq!(game.toggle_flag(3, 4)?, CellState::Hidden);
    /// # Ok::<(), game_core::Error>(())
    /// ```
    ///
    /// # Errors
    ///
    /// 座標が盤面の外なら [`Error::OutOfBounds`]、勝敗がついたあとなら [`Error::GameOver`]。
    pub fn toggle_flag(&mut self, x: usize, y: usize) -> Result<CellState, Error> {
        let i = Self::index(x, y)?;
        self.ensure_playing()?;
        let cell = &mut self.cells[i];
        cell.state = match cell.state {
            CellState::Hidden => CellState::Flagged,
            CellState::Flagged => CellState::Hidden,
            CellState::Open => CellState::Open,
        };
        Ok(cell.state)
    }

    /// マス `(x, y)` を開き、開いたあとの勝敗を返す。
    ///
    /// - 最初に開くときに地雷が置かれる。そのマスと周り8マスには置かれない。
    /// - 開いたマスが0なら、つながった0とその縁の数字まで続けて開く。
    /// - 旗のマスと、すでに開いているマスは何もしない。
    /// - 地雷を開くと負け、地雷以外をすべて開くと勝ち。
    ///
    /// ```rust
    /// use game_core::{Game, Status};
    ///
    /// let mut game = Game::new(42);
    /// assert_eq!(game.open(8, 8)?, Status::Playing);
    /// assert_eq!(game.adjacent_mines(8, 8)?, 0);
    /// # Ok::<(), game_core::Error>(())
    /// ```
    ///
    /// # Errors
    ///
    /// 座標が盤面の外なら [`Error::OutOfBounds`]、勝敗がついたあとなら [`Error::GameOver`]。
    pub fn open(&mut self, x: usize, y: usize) -> Result<Status, Error> {
        let start = Self::index(x, y)?;
        self.ensure_playing()?;
        if self.cells[start].state != CellState::Hidden {
            return Ok(self.status);
        }
        if !self.mines_placed {
            self.place_mines(x, y);
        }
        if self.cells[start].mine {
            self.cells[start].state = CellState::Open;
            self.status = Status::Lost;
        } else {
            self.open_area(x, y);
            if self
                .cells
                .iter()
                .all(|c| c.mine || c.state == CellState::Open)
            {
                self.status = Status::Won;
            }
        }
        Ok(self.status)
    }

    const fn at(x: usize, y: usize) -> usize {
        y * WIDTH + x
    }

    const fn index(x: usize, y: usize) -> Result<usize, Error> {
        if x < WIDTH && y < HEIGHT {
            Ok(Self::at(x, y))
        } else {
            Err(Error::OutOfBounds { x, y })
        }
    }

    fn ensure_playing(&self) -> Result<(), Error> {
        if self.status == Status::Playing {
            Ok(())
        } else {
            Err(Error::GameOver)
        }
    }

    fn count_adjacent(&self, x: usize, y: usize) -> usize {
        neighbors(x, y)
            .filter(|&(nx, ny)| self.cells[Self::at(nx, ny)].mine)
            .count()
    }

    /// `(start_x, start_y)` とその周り8マスを避けて、地雷を `MINE_COUNT` 個置く。
    fn place_mines(&mut self, start_x: usize, start_y: usize) {
        let mut candidates: Vec<usize> = (0..WIDTH * HEIGHT)
            .filter(|&i| (i % WIDTH).abs_diff(start_x) > 1 || (i / WIDTH).abs_diff(start_y) > 1)
            .collect();
        let mut rng = Rng::new(self.seed);
        for i in 0..MINE_COUNT {
            let j = i + rng.below(candidates.len() - i);
            candidates.swap(i, j);
            self.cells[candidates[i]].mine = true;
        }
        self.mines_placed = true;
    }

    /// `(x, y)` を開く。0なら、つながった0とその縁の数字まで開く。
    /// 旗のマスは開かない。0のマスの周りに地雷はないので、地雷は開かれない。
    fn open_area(&mut self, x: usize, y: usize) {
        let mut pending = vec![(x, y)];
        while let Some((cx, cy)) = pending.pop() {
            let i = Self::at(cx, cy);
            if self.cells[i].state != CellState::Hidden {
                continue;
            }
            self.cells[i].state = CellState::Open;
            if self.count_adjacent(cx, cy) == 0 {
                pending.extend(neighbors(cx, cy));
            }
        }
    }
}

/// `(x, y)` の周り8マス（盤面の内側にあるものだけ）。
fn neighbors(x: usize, y: usize) -> impl Iterator<Item = (usize, usize)> {
    (y.saturating_sub(1)..=(y + 1).min(HEIGHT - 1))
        .flat_map(move |ny| (x.saturating_sub(1)..=(x + 1).min(WIDTH - 1)).map(move |nx| (nx, ny)))
        .filter(move |&p| p != (x, y))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashSet;

    /// 地雷の位置を指定した、地雷配置済みの盤面を作る。
    fn game_with_mines(mines: &[(usize, usize)]) -> Game {
        let mut game = Game::new(0);
        for &(x, y) in mines {
            game.cells[Game::at(x, y)].mine = true;
        }
        game.mines_placed = true;
        game
    }

    fn mine_count(game: &Game) -> usize {
        game.cells.iter().filter(|c| c.mine).count()
    }

    fn all_coordinates() -> impl Iterator<Item = (usize, usize)> {
        (0..HEIGHT).flat_map(|y| (0..WIDTH).map(move |x| (x, y)))
    }

    /// 地雷のある座標の一覧。
    fn mine_positions(game: &Game) -> Vec<(usize, usize)> {
        all_coordinates()
            .filter(|&(x, y)| game.cells[Game::at(x, y)].mine)
            .collect()
    }

    /// `(x, y)` の周り8マスの地雷の数。期待値用なので、実装の `neighbors` は使わず、
    /// 「縦横とも距離が1以内で、自分ではない」という条件で数える。
    fn count_mines_around(mines: &[(usize, usize)], x: usize, y: usize) -> usize {
        mines
            .iter()
            .filter(|&&(mx, my)| (mx, my) != (x, y) && mx.abs_diff(x) <= 1 && my.abs_diff(y) <= 1)
            .count()
    }

    /// 地雷ではない `start` を開いたときに開くはずのマス。期待値用なので、実装の `open_area` は使わず、
    /// 「数字が0のマスなら周り8マスも開く」を素直に繰り返して求める。
    fn expected_open(mines: &[(usize, usize)], start: (usize, usize)) -> HashSet<(usize, usize)> {
        let mut opened = HashSet::new();
        let mut pending = vec![start];
        while let Some((x, y)) = pending.pop() {
            if !opened.insert((x, y)) {
                continue;
            }
            if count_mines_around(mines, x, y) == 0 {
                pending.extend(all_coordinates().filter(|&(nx, ny)| {
                    (nx, ny) != (x, y) && nx.abs_diff(x) <= 1 && ny.abs_diff(y) <= 1
                }));
            }
        }
        opened
    }

    /// 地雷配置済みの盤面で `start` を開き、開いたマスが独立に求めた期待値と一致することを確かめる。
    fn assert_open_matches_expected(
        mines: &[(usize, usize)],
        start: (usize, usize),
    ) -> Result<(), Error> {
        let mut game = game_with_mines(mines);
        game.open(start.0, start.1)?;
        let expected = expected_open(mines, start);
        for (x, y) in all_coordinates() {
            let is_open = game.state(x, y)? == CellState::Open;
            assert_eq!(
                is_open,
                expected.contains(&(x, y)),
                "start={start:?} ({x}, {y})"
            );
            assert!(
                !(is_open && mines.contains(&(x, y))),
                "地雷が開いた ({x}, {y})"
            );
        }
        let all_safe_open = expected.len() == WIDTH * HEIGHT - mines.len();
        let status = if all_safe_open {
            Status::Won
        } else {
            Status::Playing
        };
        assert_eq!(game.status(), status, "start={start:?}");
        Ok(())
    }

    /// ①: 地雷がちょうど40個
    #[test]
    fn item1_mine_count_is_exactly_40() -> Result<(), Error> {
        for seed in (0..50).chain([u64::MAX]) {
            let mut game = Game::new(seed);
            game.open(8, 8)?;
            assert_eq!(mine_count(&game), MINE_COUNT, "seed={seed}");
            // 実装と同じ定数だけで比べると、定数ごと変わっても気づけない。仕様の数字そのものでも比べる
            assert_eq!(mine_count(&game), 40, "seed={seed}");
        }
        Ok(())
    }

    /// ②: 最初に開いたマスとその周り8マスに地雷がない（角・辺を含む全マスで確かめる）
    #[test]
    fn item2_first_open_and_its_neighbors_have_no_mines() -> Result<(), Error> {
        for seed in 0..20 {
            for (sx, sy) in all_coordinates() {
                let mut game = Game::new(seed);
                let status = game.open(sx, sy)?;
                assert_eq!(status, Status::Playing, "seed={seed} start=({sx}, {sy})");
                assert_eq!(
                    mine_count(&game),
                    MINE_COUNT,
                    "seed={seed} start=({sx}, {sy})"
                );
                for (x, y) in all_coordinates() {
                    if x.abs_diff(sx) <= 1 && y.abs_diff(sy) <= 1 {
                        assert!(
                            !game.is_mine(x, y)?,
                            "seed={seed} start=({sx}, {sy}) mine=({x}, {y})"
                        );
                    }
                }
            }
        }
        Ok(())
    }

    /// ③: 開いたマスの数字が、周りの地雷の数と一致する
    #[test]
    fn item3_opened_number_matches_adjacent_mines() -> Result<(), Error> {
        //   x: 0 1 2 3
        // y=1:   * *
        // y=2:   *
        let mut game = game_with_mines(&[(1, 1), (2, 1), (1, 2)]);
        let expected = [
            ((0, 0), 1), // 角。周りの地雷は (1,1) だけ
            ((0, 1), 2), // 辺。(1,1) と (1,2)
            ((2, 2), 3), // 内側。(1,1) (2,1) (1,2)
            ((1, 1), 2), // 地雷のマス自身は数えない。(2,1) と (1,2)
            ((15, 15), 0),
        ];
        for ((x, y), number) in expected {
            assert_eq!(game.adjacent_mines(x, y)?, number, "({x}, {y})");
        }
        // 数字のマスを開くと、そのマスだけが開く
        game.open(2, 2)?;
        assert_eq!(game.state(2, 2)?, CellState::Open);
        let opened = all_coordinates()
            .filter(|&(x, y)| game.cells[Game::at(x, y)].state == CellState::Open)
            .count();
        assert_eq!(opened, 1);

        // 周り8マスのうち、どの向きの隣に1個だけ置いても数える
        for (mx, my) in [
            (4, 4),
            (5, 4),
            (6, 4),
            (4, 5),
            (6, 5),
            (4, 6),
            (5, 6),
            (6, 6),
        ] {
            let game = game_with_mines(&[(mx, my)]);
            assert_eq!(game.adjacent_mines(5, 5)?, 1, "隣の地雷 ({mx}, {my})");
        }

        // 自分以外がすべて地雷。3×3 のうち盤の内側にある数から自分を引いた数になる
        // （角は3、辺は5、内側は8。盤の四辺のどこでも数え落とさない）
        for (x, y) in all_coordinates() {
            let others: Vec<(usize, usize)> = all_coordinates().filter(|&p| p != (x, y)).collect();
            let game = game_with_mines(&others);
            let columns = if x == 0 || x == WIDTH - 1 { 2 } else { 3 };
            let rows = if y == 0 || y == HEIGHT - 1 { 2 } else { 3 };
            assert_eq!(game.adjacent_mines(x, y)?, columns * rows - 1, "({x}, {y})");
        }

        // 複数シードの盤面で、全マスの数字を独立に数えた値と比べる
        for seed in 0..30 {
            let mut game = Game::new(seed);
            game.open(8, 8)?;
            let mines = mine_positions(&game);
            for (x, y) in all_coordinates() {
                let expected = count_mines_around(&mines, x, y);
                assert_eq!(
                    game.adjacent_mines(x, y)?,
                    expected,
                    "seed={seed} ({x}, {y})"
                );
            }
        }
        Ok(())
    }

    /// ④: 0を開くと、つながった0とその縁の数字まで開き、地雷は開かない
    #[test]
    fn item4_opening_zero_spreads_to_zeros_and_border_numbers() -> Result<(), Error> {
        // x=8 の縦一列がすべて地雷。左半分だけが開けるはず
        let wall: Vec<(usize, usize)> = (0..HEIGHT).map(|y| (8, y)).collect();
        let mut game = game_with_mines(&wall);
        assert_eq!(game.open(0, 0)?, Status::Playing);
        for (x, y) in all_coordinates() {
            let state = game.state(x, y)?;
            match x {
                0..=6 => {
                    assert_eq!(state, CellState::Open, "0 のマス ({x}, {y})");
                    assert_eq!(game.adjacent_mines(x, y)?, 0);
                }
                7 => {
                    assert_eq!(state, CellState::Open, "縁の数字 ({x}, {y})");
                    // 壁の隣。上下の端は地雷が2個、それ以外は3個
                    let number = if y == 0 || y == HEIGHT - 1 { 2 } else { 3 };
                    assert_eq!(game.adjacent_mines(x, y)?, number, "縁の数字 ({x}, {y})");
                }
                _ => assert_eq!(state, CellState::Hidden, "地雷の壁の向こう ({x}, {y})"),
            }
            if game.is_mine(x, y)? {
                assert_ne!(state, CellState::Open, "地雷が開いた ({x}, {y})");
            }
        }

        // 斜めにしかつながらない縁の数字 (6, 6) も開く。(5, 5) は0で、
        // (6, 6) の上下左右は (6, 5) (5, 6) (6, 7) が数字、(7, 6) が地雷になっている
        assert_open_matches_expected(&[(7, 6), (4, 7)], (5, 5))?;

        // 複数シードの盤面で、開いたマスの集合を独立に求めた期待値と比べる
        for seed in 0..30 {
            let mut game = Game::new(seed);
            game.open(8, 8)?;
            let mines = mine_positions(&game);
            for start in [(8, 8), (0, 0), (15, 0), (0, 15), (15, 15)] {
                if !mines.contains(&start) {
                    assert_open_matches_expected(&mines, start)?;
                }
            }
        }
        Ok(())
    }

    /// ⑤: 旗のマスは開かない
    #[test]
    fn item5_flagged_cell_is_not_opened() -> Result<(), Error> {
        // 旗を立てて、直接開いても開かない。地雷の旗を開いても負けない
        let mut game = game_with_mines(&[(5, 5)]);
        assert_eq!(game.toggle_flag(5, 5)?, CellState::Flagged);
        assert_eq!(game.open(5, 5)?, Status::Playing);
        assert_eq!(game.state(5, 5)?, CellState::Flagged);

        // 旗は取り消せて、取り消せば開ける
        assert_eq!(game.toggle_flag(5, 5)?, CellState::Hidden);
        assert_eq!(game.open(5, 5)?, Status::Lost);

        // 0 の広がりも、旗のマスは開かない
        let mut game = game_with_mines(&[(15, 15)]);
        game.toggle_flag(3, 3)?;
        game.open(0, 0)?;
        assert_eq!(game.state(3, 3)?, CellState::Flagged);
        assert_eq!(game.state(4, 3)?, CellState::Open);
        Ok(())
    }

    /// ⑥: 地雷を開いたら負け
    #[test]
    fn item6_opening_a_mine_loses() -> Result<(), Error> {
        // 複数シード・2通りの最初の一手で、どの地雷を開いても負ける
        for seed in 0..30 {
            for (sx, sy) in [(8, 8), (0, 0)] {
                let mut board = Game::new(seed);
                board.open(sx, sy)?;
                for (mx, my) in mine_positions(&board) {
                    let mut game = board.clone();
                    let at = format!("seed={seed} start=({sx}, {sy}) mine=({mx}, {my})");
                    assert_eq!(game.open(mx, my)?, Status::Lost, "{at}");
                    assert_eq!(game.status(), Status::Lost, "{at}");
                    assert_eq!(game.state(mx, my)?, CellState::Open, "{at}");
                    // 負けたあとは操作できない
                    assert_eq!(game.open(0, 0), Err(Error::GameOver), "{at}");
                    assert_eq!(game.toggle_flag(0, 0), Err(Error::GameOver), "{at}");
                }
            }
        }
        Ok(())
    }

    /// ⑥: 地雷以外をすべて開いたら勝ち（1マス残っている間は勝ちにならない）
    #[test]
    fn item6_opening_all_safe_cells_wins() -> Result<(), Error> {
        for seed in 0..30 {
            let mut game = Game::new(seed);
            game.open(8, 8)?;
            let mines = mine_positions(&game);
            let safe: Vec<(usize, usize)> =
                all_coordinates().filter(|p| !mines.contains(p)).collect();
            assert_eq!(safe.len(), WIDTH * HEIGHT - MINE_COUNT);
            for &(x, y) in &safe {
                // 0の広がりで先に開いたマスは飛ばす（最後の安全マスが先に開いていてもエラーにしない）
                if game.state(x, y)? == CellState::Open {
                    continue;
                }
                let status = game.open(x, y)?;
                let all_open = safe
                    .iter()
                    .all(|&(sx, sy)| game.state(sx, sy) == Ok(CellState::Open));
                assert_eq!(status == Status::Won, all_open, "seed={seed} ({x}, {y})");
            }
            assert_eq!(game.status(), Status::Won, "seed={seed}");
            // 勝ったあとは操作できない
            assert_eq!(game.open(0, 0), Err(Error::GameOver), "seed={seed}");
        }
        Ok(())
    }

    /// ⑥: 旗を立てた安全マスが残っている間は、ほかをすべて開いても勝ちにならない
    #[test]
    fn item6_flagged_safe_cell_blocks_the_win() -> Result<(), Error> {
        for seed in 0..30 {
            let mut game = Game::new(seed);
            game.open(8, 8)?;
            let mines = mine_positions(&game);
            let safe: Vec<(usize, usize)> =
                all_coordinates().filter(|p| !mines.contains(p)).collect();
            // まだ閉じている安全マスのうち1つに旗を立てる
            let (fx, fy) = *safe
                .iter()
                .rev()
                .find(|&&(x, y)| game.state(x, y) == Ok(CellState::Hidden))
                .expect("最初の一手のあとにも、閉じた安全マスが残っている");
            game.toggle_flag(fx, fy)?;
            for &(x, y) in &safe {
                if game.state(x, y)? != CellState::Hidden {
                    continue;
                }
                let status = game.open(x, y)?;
                assert_eq!(status, Status::Playing, "seed={seed} ({x}, {y})");
            }
            assert_eq!(game.state(fx, fy)?, CellState::Flagged, "seed={seed}");
            let opened = safe
                .iter()
                .filter(|&&(x, y)| game.state(x, y) == Ok(CellState::Open));
            assert_eq!(opened.count(), safe.len() - 1, "seed={seed}");
            // 旗を取り消して最後の1マスを開くと勝つ
            assert_eq!(game.toggle_flag(fx, fy)?, CellState::Hidden);
            assert_eq!(game.open(fx, fy)?, Status::Won, "seed={seed}");
        }
        Ok(())
    }

    #[test]
    fn same_seed_gives_same_board() -> Result<(), Error> {
        let mines = |seed| -> Result<Vec<bool>, Error> {
            let mut game = Game::new(seed);
            game.open(3, 12)?;
            Ok(game.cells.iter().map(|c| c.mine).collect())
        };
        assert_eq!(mines(123)?, mines(123)?);
        assert_ne!(mines(123)?, mines(124)?);
        Ok(())
    }

    #[test]
    fn out_of_bounds_is_an_error() {
        let mut game = Game::new(0);
        assert_eq!(
            game.open(WIDTH, 0),
            Err(Error::OutOfBounds { x: WIDTH, y: 0 })
        );
        assert_eq!(
            game.toggle_flag(0, HEIGHT),
            Err(Error::OutOfBounds { x: 0, y: HEIGHT })
        );
        assert_eq!(game.state(99, 99), Err(Error::OutOfBounds { x: 99, y: 99 }));
    }
}
