use game_core::{HEIGHT, WIDTH};

/// 盤面が画面（Canvas）のどこに、どの大きさで描かれているか。
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Layout {
    /// 盤面の左端の、画面上の x 座標。
    pub origin_x: f64,
    /// 盤面の上端の、画面上の y 座標。
    pub origin_y: f64,
    /// 1マスの一辺の長さ。
    pub cell_size: f64,
}

impl Layout {
    /// 画面上の位置 `(px, py)` にあるマスの `(列, 行)` を返す。盤面の外なら `None`。
    ///
    /// マスの境目は、右と下のマスに含める（左上の角はそのマス、右下の角は隣のマス）。
    ///
    /// ```rust
    /// use client::Layout;
    ///
    /// let layout = Layout { origin_x: 10.0, origin_y: 20.0, cell_size: 30.0 };
    /// assert_eq!(layout.cell_at(10.0, 20.0), Some((0, 0)));
    /// assert_eq!(layout.cell_at(40.0, 50.0), Some((1, 1)));
    /// assert_eq!(layout.cell_at(5.0, 20.0), None);
    /// ```
    #[must_use]
    pub fn cell_at(&self, px: f64, py: f64) -> Option<(usize, usize)> {
        let column = index_within(px - self.origin_x, self.cell_size, WIDTH)?;
        let row = index_within(py - self.origin_y, self.cell_size, HEIGHT)?;
        Some((column, row))
    }
}

/// 盤面の端から `offset` の位置が、`0..count` のうち何番目のマスかを返す。範囲の外、
/// 大きさが正でないとき、NaN や無限大のときは `None`。
// `count` は盤面の一辺（16）で、f64 で正確に表せる。範囲の内側（0 以上 count 未満）を確かめたあとにだけ
// usize に変換するので、切り捨てや符号の欠落は起きない
#[allow(
    clippy::cast_possible_truncation,
    clippy::cast_sign_loss,
    clippy::cast_precision_loss
)]
fn index_within(offset: f64, cell_size: f64, count: usize) -> Option<usize> {
    let index = (offset / cell_size).floor();
    // NaN は、どの比較にも通らず `None` になる。大きさが負だと、負の位置が正の番号になってしまうので除く
    let inside = cell_size > 0.0 && index >= 0.0 && index < count as f64;
    inside.then_some(index as usize)
}

#[cfg(test)]
mod tests {
    use super::*;

    const LAYOUT: Layout = Layout {
        origin_x: 10.0,
        origin_y: 20.0,
        cell_size: 30.0,
    };

    /// ⑬: すべてのマスの中心が、そのマスになる（大きさと位置を変えても）
    // 0..16 は f64 で正確に表せる
    #[allow(clippy::cast_precision_loss)]
    #[test]
    fn item13_the_center_of_every_cell_maps_to_that_cell() {
        for (origin_x, origin_y, cell_size) in
            [(10.0, 20.0, 30.0), (0.0, 0.0, 24.0), (3.5, 100.0, 32.5)]
        {
            let layout = Layout {
                origin_x,
                origin_y,
                cell_size,
            };
            for row in 0..HEIGHT {
                for column in 0..WIDTH {
                    let px = (column as f64 + 0.5).mul_add(cell_size, origin_x);
                    let py = (row as f64 + 0.5).mul_add(cell_size, origin_y);
                    assert_eq!(
                        layout.cell_at(px, py),
                        Some((column, row)),
                        "{layout:?} ({column}, {row})"
                    );
                }
            }
        }
    }

    /// ⑬: マスの境目は右と下のマスに入り、盤面の四隅のすぐ内側は端のマスになる
    #[test]
    fn item13_cell_boundaries_belong_to_the_right_and_lower_cell() {
        // 左上の角の、ちょうどその位置と、すぐ内側
        assert_eq!(LAYOUT.cell_at(10.0, 20.0), Some((0, 0)));
        assert_eq!(LAYOUT.cell_at(39.999, 49.999), Some((0, 0)));
        // 境目の上は、右と下のマス
        assert_eq!(LAYOUT.cell_at(40.0, 20.0), Some((1, 0)));
        assert_eq!(LAYOUT.cell_at(10.0, 50.0), Some((0, 1)));
        assert_eq!(LAYOUT.cell_at(40.0, 50.0), Some((1, 1)));
        // 右下の端のマス。盤面の端（10 + 16 * 30 = 490、20 + 16 * 30 = 500）のすぐ内側まで
        assert_eq!(LAYOUT.cell_at(489.999, 499.999), Some((15, 15)));
        assert_eq!(LAYOUT.cell_at(489.999, 20.0), Some((15, 0)));
        assert_eq!(LAYOUT.cell_at(10.0, 499.999), Some((0, 15)));
    }

    /// ⑬: 盤面の外は無視する（4辺のすぐ外、遠く、負の座標、片方だけ外）
    #[test]
    fn item13_positions_outside_the_board_are_ignored() {
        // 左・上のすぐ外
        assert_eq!(LAYOUT.cell_at(9.999, 30.0), None);
        assert_eq!(LAYOUT.cell_at(30.0, 19.999), None);
        // 右・下のちょうど端と、すぐ外
        assert_eq!(LAYOUT.cell_at(490.0, 30.0), None);
        assert_eq!(LAYOUT.cell_at(30.0, 500.0), None);
        assert_eq!(LAYOUT.cell_at(490.001, 500.001), None);
        // 遠く・負の座標・原点
        assert_eq!(LAYOUT.cell_at(-100.0, -100.0), None);
        assert_eq!(LAYOUT.cell_at(0.0, 0.0), None);
        assert_eq!(LAYOUT.cell_at(10_000.0, 10_000.0), None);
        // 片方が中でも、もう片方が外なら無視する
        assert_eq!(LAYOUT.cell_at(30.0, 600.0), None);
        assert_eq!(LAYOUT.cell_at(600.0, 30.0), None);
        assert_eq!(LAYOUT.cell_at(-1.0, 30.0), None);
        assert_eq!(LAYOUT.cell_at(30.0, -1.0), None);
    }

    /// ⑬: 位置が読めないとき（NaN・無限大）と、1マスの大きさが正でないときも、盤面の外として無視する
    #[test]
    fn item13_unusable_numbers_are_ignored() {
        for value in [f64::NAN, f64::INFINITY, f64::NEG_INFINITY] {
            assert_eq!(LAYOUT.cell_at(value, 30.0), None, "{value}");
            assert_eq!(LAYOUT.cell_at(30.0, value), None, "{value}");
        }
        for cell_size in [0.0, -30.0, f64::NAN] {
            let layout = Layout {
                cell_size,
                ..LAYOUT
            };
            // 盤面の内側の位置と、左上より外の位置（大きさが負だと、負÷負で正の番号になりうる）
            for (px, py) in [(30.0, 30.0), (5.0, 5.0)] {
                assert_eq!(layout.cell_at(px, py), None, "{cell_size} ({px}, {py})");
            }
        }
    }
}
