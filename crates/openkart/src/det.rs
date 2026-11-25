//! Deterministic arithmetic: the race uses only `+ - * /` and `sqrt` (exact in IEEE 754 on
//! every machine and in WebAssembly), its own polynomial sine and cosine, and a seeded
//! xorshift. No hash-ordered iteration anywhere, so a seed and the inputs give the same race
//! natively and in the browser.

/// Sine and cosine of `a` radians, by range reduction and Taylor series.
pub fn sin_cos(a: f64) -> (f64, f64) {
    const PI: f64 = std::f64::consts::PI;
    const TAU: f64 = 2.0 * PI;
    let mut x = a - TAU * ((a / TAU) + 0.5).floor();
    // Now x is in [-pi, pi); fold to [-pi/2, pi/2] for the series.
    let mut flip = 1.0;
    if x > PI / 2.0 {
        x = PI - x;
        flip = -1.0;
    } else if x < -PI / 2.0 {
        x = -PI - x;
        flip = -1.0;
    }
    let x2 = x * x;
    let mut s = x;
    let mut c = 1.0;
    let mut ts = x;
    let mut tc = 1.0;
    for k in 1..10 {
        let k = k as f64;
        ts *= -x2 / ((2.0 * k) * (2.0 * k + 1.0));
        tc *= -x2 / ((2.0 * k - 1.0) * (2.0 * k));
        s += ts;
        c += tc;
    }
    (s, c * flip)
}

/// A heading `(hx, hz)` turned `a` radians to the left (x east, z south: the left of a heading
/// is `(hz, -hx)`), kept a unit vector.
pub fn turn(hx: f64, hz: f64, a: f64) -> (f64, f64) {
    let (s, c) = sin_cos(a);
    let (x, z) = (hx * c + hz * s, hz * c - hx * s);
    let l = (x * x + z * z).sqrt();
    (x / l, z / l)
}

/// xorshift64*, seeded from a string with FNV-1a.
#[derive(Clone, Debug)]
pub struct Rng(u64);

impl Rng {
    pub fn new(seed: &str) -> Rng {
        let mut h: u64 = 0xcbf2_9ce4_8422_2325;
        for b in seed.bytes() {
            h ^= b as u64;
            h = h.wrapping_mul(0x0000_0100_0000_01b3);
        }
        Rng(if h == 0 { 0x9e37_79b9_7f4a_7c15 } else { h })
    }

    pub fn next_u64(&mut self) -> u64 {
        let mut x = self.0;
        x ^= x >> 12;
        x ^= x << 25;
        x ^= x >> 27;
        self.0 = x;
        x.wrapping_mul(0x2545_f491_4f6c_dd1d)
    }

    /// In `[0, 1)`.
    pub fn f(&mut self) -> f64 {
        (self.next_u64() >> 11) as f64 / (1u64 << 53) as f64
    }

    pub fn range(&mut self, lo: f64, hi: f64) -> f64 {
        lo + (hi - lo) * self.f()
    }

    /// An index picked with these weights (all zero: the first).
    pub fn pick(&mut self, weights: &[f64]) -> usize {
        let total: f64 = weights.iter().sum();
        if total <= 0.0 {
            return 0;
        }
        let mut r = self.f() * total;
        for (i, w) in weights.iter().enumerate() {
            if r < *w {
                return i;
            }
            r -= w;
        }
        weights.len() - 1
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sine_and_cosine_are_close_to_std() {
        for i in -100..100 {
            let a = i as f64 * 0.137;
            let (s, c) = sin_cos(a);
            assert!(
                (s - a.sin()).abs() < 1e-12 && (c - a.cos()).abs() < 1e-12,
                "{a}"
            );
        }
        let (x, z) = turn(1.0, 0.0, std::f64::consts::FRAC_PI_2);
        // East turned left is north: -z.
        assert!(x.abs() < 1e-12 && (z + 1.0).abs() < 1e-12);
    }

    #[test]
    fn the_rng_is_seeded() {
        let mut a = Rng::new("x");
        let mut b = Rng::new("x");
        let mut c = Rng::new("y");
        let va: Vec<u64> = (0..5).map(|_| a.next_u64()).collect();
        assert_eq!(va, (0..5).map(|_| b.next_u64()).collect::<Vec<_>>());
        assert_ne!(va, (0..5).map(|_| c.next_u64()).collect::<Vec<_>>());
        assert_eq!(Rng::new("z").pick(&[0.0, 0.0, 1.0]), 2);
    }
}
