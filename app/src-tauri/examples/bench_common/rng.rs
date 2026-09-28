//! Small deterministic PRNG (SplitMix64), so datasets and mutations are
//! reproducible across machines and crate versions.

pub struct Rng(u64);

impl Rng {
    pub fn new(seed: u64) -> Self {
        Rng(seed ^ 0x9E37_79B9_7F4A_7C15)
    }

    pub fn next_u64(&mut self) -> u64 {
        self.0 = self.0.wrapping_add(0x9E37_79B9_7F4A_7C15);
        let mut z = self.0;
        z = (z ^ (z >> 30)).wrapping_mul(0xBF58_476D_1CE4_E5B9);
        z = (z ^ (z >> 27)).wrapping_mul(0x94D0_49BB_1331_11EB);
        z ^ (z >> 31)
    }

    /// Uniform in `0..n` (`n > 0`).
    pub fn below(&mut self, n: u64) -> u64 {
        self.next_u64() % n
    }

    /// Uniform in `lo..=hi`.
    pub fn range(&mut self, lo: u64, hi: u64) -> u64 {
        lo + self.below(hi - lo + 1)
    }

    pub fn chance(&mut self, p: f64) -> bool {
        (self.next_u64() >> 11) as f64 / (1u64 << 53) as f64 <= p
    }

    pub fn pick<'a, T>(&mut self, items: &'a [T]) -> &'a T {
        &items[self.below(items.len() as u64) as usize]
    }

    /// Picks from `(item, weight)` pairs.
    pub fn weighted<'a, T>(&mut self, items: &'a [(T, u32)]) -> &'a T {
        let total: u64 = items.iter().map(|(_, w)| *w as u64).sum();
        let mut x = self.below(total);
        for (item, w) in items {
            if x < *w as u64 {
                return item;
            }
            x -= *w as u64;
        }
        &items[items.len() - 1].0
    }
}
