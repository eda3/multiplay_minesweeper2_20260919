//! 盤面の再現のために、外部クレートに頼らず自前で持つ小さな乱数生成器。

/// SplitMix64。シードが同じなら、将来も同じ列が出る。
#[derive(Debug, Clone)]
pub struct Rng {
    state: u64,
}

impl Rng {
    pub const fn new(seed: u64) -> Self {
        Self { state: seed }
    }

    pub const fn next_u64(&mut self) -> u64 {
        self.state = self.state.wrapping_add(0x9E37_79B9_7F4A_7C15);
        let mut z = self.state;
        z = (z ^ (z >> 30)).wrapping_mul(0xBF58_476D_1CE4_E5B9);
        z = (z ^ (z >> 27)).wrapping_mul(0x94D0_49BB_1331_11EB);
        z ^ (z >> 31)
    }

    /// `0..bound` の値を返す。`bound` は 1 以上。
    ///
    /// 剰余で丸めているため、`bound` が 2^64 に比べて十分小さい範囲でのみ偏りが無視できる。
    /// 盤面は 256 マスなので問題にならない。計算は `u64` で行うので、
    /// `usize` が 32 ビットの WASM でも結果は変わらない。
    pub fn below(&mut self, bound: usize) -> usize {
        let value = self.next_u64() % (bound as u64);
        usize::try_from(value).expect("bound 未満なので usize に収まる")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// 公開されている `SplitMix64` の参照値（シード 0）と一致すること。
    /// 実装が変わって同じシードの盤面が変わってしまうのを防ぐ。
    #[test]
    fn splitmix64_matches_reference_values() {
        let mut rng = Rng::new(0);
        assert_eq!(rng.next_u64(), 0xE220_A839_7B1D_CDAF);
        assert_eq!(rng.next_u64(), 0x6E78_9E6A_A1B9_65F4);
        assert_eq!(rng.next_u64(), 0x06C4_5D18_8009_454F);
    }

    #[test]
    fn below_stays_in_range() {
        let mut rng = Rng::new(1);
        for bound in 1..=256 {
            assert!(rng.below(bound) < bound);
        }
    }
}
