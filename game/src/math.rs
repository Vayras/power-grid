//! Tiny float helpers (core has no libm).
#[inline] pub fn fabs(x: f64) -> f64 { if x < 0.0 { -x } else { x } }
#[inline] pub fn fabs32(x: f32) -> f32 { if x < 0.0 { -x } else { x } }
#[inline] pub fn fmin(a: f64, b: f64) -> f64 { if a < b { a } else { b } }
#[inline] pub fn fmax(a: f64, b: f64) -> f64 { if a > b { a } else { b } }
#[inline] pub fn clamp(x: f64, lo: f64, hi: f64) -> f64 { fmax(lo, fmin(hi, x)) }
#[inline] pub fn iabs(x: i32) -> i32 { if x < 0 { -x } else { x } }
#[inline] pub fn imax(a: i32, b: i32) -> i32 { if a > b { a } else { b } }
#[inline] pub fn floor32(x: f32) -> i32 { let t = x as i32; if (t as f32) > x { t - 1 } else { t } }
/// Square root by Newton's method (no libm in core). Good to ~1e-12 for the ranges the game uses.
pub fn sqrt(x: f64) -> f64 {
    if x <= 0.0 { return 0.0; }
    let mut r = if x > 1.0 { x * 0.5 } else { 1.0 };
    for _ in 0..40 { let n = 0.5 * (r + x / r); if fabs(n - r) < 1e-12 * r { return n; } r = n; }
    r
}
