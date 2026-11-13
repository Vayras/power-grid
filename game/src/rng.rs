//! xorshift64* — small, fast, deterministic per seed.
pub struct Rng(u64);
impl Rng {
    pub fn new(seed: u64) -> Rng { Rng(if seed == 0 { 0x2545_F491_4F6C_DD1D } else { seed }) }
    pub fn next(&mut self) -> u64 {
        let mut x = self.0; x ^= x >> 12; x ^= x << 25; x ^= x >> 27; self.0 = x;
        x.wrapping_mul(0x2545_F491_4F6C_DD1D)
    }
    /// Uniform in [0, 1).
    pub fn f(&mut self) -> f64 { (self.next() >> 11) as f64 / (1u64 << 53) as f64 }
    pub fn below(&mut self, n: u32) -> u32 { if n == 0 { 0 } else { (self.next() >> 33) as u32 % n } }
}
