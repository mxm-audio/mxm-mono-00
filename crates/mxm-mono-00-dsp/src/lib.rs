//! DSP for mxm-mono-00: a System-100 as its plug-out pictures it, sounding as the machine did.
//!
//! Deliberately free of any plugin-framework types: everything here takes plain values and a
//! sample rate, so the whole instrument is testable with `cargo test` and no host involved.
//!
//! **The fourth honest copy.** `flush`, the PolyBLEP residual, the Padé approximants and the ADSR
//! are copied whole from the sibling crates, as `plans/plan-mxm-mono-00.md` §7.2 says, and this
//! crate depends on none of them; its noise generator is `mxm-rng`'s `Xorshift32`. What it adds is the machine's: a triangle by switched
//! inversion, coloured noise, an LFO whose shapes sit at three DC levels, a VCA whose tremolo can
//! only dip, a portamento that is a hold capacitor, and a glide that is one RC on a gate edge.
//!
//! The research every module cites is `research:instruments/system-100.md`; the design is the plan
//! until the crate's own `AGENTS.md` replaces it.

#[cfg(any(test, feature = "conformance"))]
pub mod conformance;
pub mod effects;
pub mod envelope;
pub mod filter;
pub mod hpf;
pub mod lfo;
pub mod matrix;
pub mod noise;
pub mod oscillator;
pub mod oversample;
pub mod ring;
pub mod routing;
pub mod sh;
pub mod vca;
pub mod voice;

/// The lowest host rate the plugin activates at; a non-finite rate is refused with it.
///
/// `f32::clamp` panics when its lower bound is above its upper one or either is NaN, and the
/// corners here are clamped to a fixed floor under 0.45 of the rate they run at: the HPF's 10 Hz
/// and the oversampled ladder's 20 Hz cross below 22.2 Hz, the effects' 20 Hz below 44.4 Hz. 1 kHz
/// is far clear of that, and no higher than the lowest rate clap-validator (1234.57 Hz) or the
/// player's robustness sweeps (1 kHz) ask for.
pub const MIN_SAMPLE_RATE: f32 = 1_000.0;

/// Flush a recursive state toward zero before it can become denormal.
///
/// Denormal arithmetic can cost orders of magnitude more than normal arithmetic, which in a
/// feedback filter shows up as a CPU spike exactly when a note decays into silence. Done in the
/// DSP rather than by a framework FTZ guard, because the guard may be a no-op without an opt-in
/// feature, and flushing here is also what keeps digital silence *exactly* zero.
///
/// `1e-20` is far above the f32 denormal threshold (~1.18e-38) and about -400 dB.
#[inline(always)]
pub fn flush(x: f32) -> f32 {
    if x.abs() < 1e-20 { 0.0 } else { x }
}

/// Bounded, monotonic `tanh` approximation — a [7/6] Padé form.
///
/// The input clamp is load-bearing: without it the rational form diverges for large `x`, which
/// would break every boundedness argument in the crate. The output clamp guards the last ulp so
/// `|tanh_approx(x)| <= 1` is exactly true in `f32`, not nearly true.
#[inline]
pub fn tanh_approx(x: f32) -> f32 {
    let x = x.clamp(-4.0, 4.0);
    let x2 = x * x;
    let num = x * (135135.0 + x2 * (17325.0 + x2 * (378.0 + x2)));
    let den = 135135.0 + x2 * (62370.0 + x2 * (3150.0 + x2 * 28.0));
    (num / den).clamp(-1.0, 1.0)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn flush_preserves_audible_values_and_kills_tiny_ones() {
        assert_eq!(flush(0.0), 0.0);
        assert_eq!(flush(1e-30), 0.0);
        assert_eq!(flush(-1e-30), 0.0);
        assert_eq!(flush(0.5), 0.5);
        assert_eq!(flush(-1e-6), -1e-6);
    }

    #[test]
    fn tanh_approx_is_bounded_monotonic_and_odd() {
        let mut prev = -1.0f32;
        for i in -2000..=2000 {
            let x = i as f32 * 0.01;
            let y = tanh_approx(x);
            assert!(y.abs() <= 1.0, "|tanh({x})| = {y} exceeds 1");
            assert!(y >= prev, "not monotonic at {x}");
            assert!((y + tanh_approx(-x)).abs() < 1e-6, "not odd at {x}");
            prev = y;
        }
        // Bounded by exactly 1, and essentially flat where the input clamp takes over.
        assert!(tanh_approx(f32::MAX) <= 1.0 && tanh_approx(f32::MAX) > 0.999);
        assert!(tanh_approx(f32::MIN) >= -1.0 && tanh_approx(f32::MIN) < -0.999);
    }
}
