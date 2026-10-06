//! The 101's noise generator: white above three kilohertz, pink in two steps.
//!
//! `research:instruments/system-100.md` §7.2 (wart 10). Q341's reverse-biased junction makes the
//! noise; the WHITE / PINK switch then selects one of two passive networks, and **neither is the
//! textbook spectrum**:
//!
//! | Position | Network | What it does (derived from the printed values, not measured) |
//! |---|---|---|
//! | WHITE | a single 470 pF in series into the 100 kΩ mixer slider | a one-pole **high-pass** with its corner near 3.4 kHz — "hiss with the bottom removed" |
//! | PINK | 10 kΩ, then 10 kΩ + 0.01 µF to ground; 47 kΩ, then 47 kΩ + 0.1 µF to ground | two **-6 dB shelves** — each a series R into a shunt R–C, a pole an octave below a zero — at about 0.8/1.6 kHz and 17/34 Hz: a two-step approximation of -3 dB per octave |
//!
//! The corners are arithmetic on the schematic's values and are labelled derived; the shelf
//! reading of the pink network is a topology assumption stated in the table. The noise reaches
//! the mixer and the NOISE OUT column *after* the switch, so what leaves is the coloured version.

use crate::{Rng, flush};

/// Derived: 1 / (2π · 100 kΩ · 470 pF).
pub const WHITE_HP_HZ: f32 = 3_386.0;
/// Derived: the first shelf's zero, 1 / (2π · 10 kΩ · 0.01 µF), and its pole an octave down.
pub const PINK_SHELF_1_ZERO_HZ: f32 = 1_592.0;
/// Derived: the second shelf's zero, 1 / (2π · 47 kΩ · 0.1 µF), and its pole an octave down.
pub const PINK_SHELF_2_ZERO_HZ: f32 = 33.9;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Colour {
    #[default]
    White,
    Pink,
}

/// A first-order section `(1 + s/ωz) / (1 + s/ωp)`, bilinear-transformed, as a direct form I.
///
/// With `ωp < ωz` it is a low shelf cutting by `ωp/ωz`; with `ωz → ∞` it is a plain lowpass.
#[derive(Debug, Clone, Copy, Default)]
struct Section {
    b0: f32,
    b1: f32,
    a1: f32,
    x1: f32,
    y1: f32,
}

impl Section {
    /// A pole at `pole_hz` and a zero at `zero_hz`, unity gain at DC.
    fn pole_zero(pole_hz: f32, zero_hz: f32, sample_rate: f32) -> Self {
        // Bilinear transform with the usual prewarp of each corner, in f64 for the coefficients.
        // Each corner clamped below Nyquist first: the shelves sit at 1.6 kHz and 34 Hz and the
        // white high-pass at 3.4 kHz, all of which are above Nyquist at the lowest sample rates a
        // validator tries, and an unclamped prewarp there makes an unstable filter.
        let warp = |hz: f32| {
            let hz = hz.min(crate::hpf::NYQUIST_FRACTION * sample_rate);
            (std::f64::consts::PI * f64::from(hz) / f64::from(sample_rate)).tan()
        };
        let (wp, wz) = (warp(pole_hz), warp(zero_hz));
        // H(z) = ((wz + 1)/wz · … ) — written out: numerator (1 + s/wz), denominator (1 + s/wp)
        // with s = (1 - z^-1)/(1 + z^-1) after prewarp scaling.
        let num0 = 1.0 + 1.0 / wz;
        let num1 = 1.0 - 1.0 / wz;
        let den0 = 1.0 + 1.0 / wp;
        let den1 = 1.0 - 1.0 / wp;
        Self {
            b0: (num0 / den0) as f32,
            b1: (num1 / den0) as f32,
            a1: (den1 / den0) as f32,
            x1: 0.0,
            y1: 0.0,
        }
    }

    /// A one-pole highpass at `hz`.
    fn highpass(hz: f32, sample_rate: f32) -> Self {
        let hz = hz.min(crate::hpf::NYQUIST_FRACTION * sample_rate);
        let w = (std::f64::consts::PI * f64::from(hz) / f64::from(sample_rate)).tan();
        let den0 = 1.0 + w;
        Self {
            b0: (1.0 / den0) as f32,
            b1: (-1.0 / den0) as f32,
            a1: ((w - 1.0) / den0) as f32,
            x1: 0.0,
            y1: 0.0,
        }
    }

    #[inline]
    fn process(&mut self, x: f32) -> f32 {
        let y = self.b0 * x + self.b1 * self.x1 - self.a1 * self.y1;
        self.x1 = x;
        // Both memories of one recursion go to zero together (`mxm-poly-06-dsp`'s limit-cycle
        // finding); a first-order section has only one recursive memory, so this is enough.
        self.y1 = flush(y);
        y
    }

    fn reset(&mut self) {
        self.x1 = 0.0;
        self.y1 = 0.0;
    }
}

#[derive(Debug, Clone)]
pub struct Noise {
    rng: Rng,
    white: Section,
    pink1: Section,
    pink2: Section,
}

impl Default for Noise {
    fn default() -> Self {
        Self::new(48_000.0)
    }
}

impl Noise {
    const SEED: u32 = 0x0100_0101;

    pub fn new(sample_rate: f32) -> Self {
        let mut n = Self {
            rng: Rng::new(Self::SEED),
            white: Section::default(),
            pink1: Section::default(),
            pink2: Section::default(),
        };
        n.set_sample_rate(sample_rate);
        n
    }

    pub fn set_sample_rate(&mut self, sample_rate: f32) {
        self.white = Section::highpass(WHITE_HP_HZ, sample_rate);
        self.pink1 = Section::pole_zero(
            PINK_SHELF_1_ZERO_HZ * 0.5,
            PINK_SHELF_1_ZERO_HZ,
            sample_rate,
        );
        self.pink2 = Section::pole_zero(
            PINK_SHELF_2_ZERO_HZ * 0.5,
            PINK_SHELF_2_ZERO_HZ,
            sample_rate,
        );
    }

    pub fn reset(&mut self) {
        self.rng = Rng::new(Self::SEED);
        self.white.reset();
        self.pink1.reset();
        self.pink2.reset();
    }

    /// One sample of the selected colour, bounded by the shaping's unity gain: `|y| <= 2`.
    #[inline]
    pub fn process(&mut self, colour: Colour) -> f32 {
        let raw = self.rng.next_bipolar();
        // Both networks run so a switch mid-note does not click from an unsettled filter.
        let white = self.white.process(raw);
        let pink = self.pink2.process(self.pink1.process(raw));
        match colour {
            Colour::White => white,
            Colour::Pink => pink,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::f32::consts::TAU;

    /// Band energy of a rendered noise by a direct DFT over a window, in dB.
    fn band_db(x: &[f32], fs: f32, lo_hz: f32, hi_hz: f32) -> f32 {
        let n = x.len();
        let mut power = 0.0f64;
        let mut bins = 0usize;
        let lo = (lo_hz / fs * n as f32) as usize;
        let hi = (hi_hz / fs * n as f32) as usize;
        for bin in lo..hi {
            let (mut re, mut im) = (0.0f64, 0.0f64);
            for (i, &v) in x.iter().enumerate() {
                let ang = -TAU as f64 * bin as f64 * i as f64 / n as f64;
                re += v as f64 * ang.cos();
                im += v as f64 * ang.sin();
            }
            power += re * re + im * im;
            bins += 1;
        }
        10.0 * (power / bins as f64).log10() as f32
    }

    fn render(colour: Colour, n: usize) -> Vec<f32> {
        let mut noise = Noise::new(48_000.0);
        (0..n).map(|_| noise.process(colour)).collect()
    }

    #[test]
    fn white_is_high_passed_near_three_kilohertz() {
        // Wart 10: "we hear mostly the higher hissing frequencies and very little of the lower".
        let x = render(Colour::White, 4096);
        let low = band_db(&x, 48_000.0, 100.0, 800.0);
        let high = band_db(&x, 48_000.0, 6_000.0, 12_000.0);
        assert!(
            high - low > 12.0,
            "white should be well down below its corner: low {low:.1} dB, high {high:.1} dB"
        );
    }

    #[test]
    fn pink_falls_about_three_decibels_per_octave_across_the_shelves() {
        // Two -6 dB steps between 34 Hz and 1.6 kHz: about -12 dB over the whole span, which is
        // -3 dB per octave to within the two-step approximation's own coarseness.
        let x = render(Colour::Pink, 8192);
        let low = band_db(&x, 48_000.0, 60.0, 120.0);
        let high = band_db(&x, 48_000.0, 3_000.0, 6_000.0);
        let drop = low - high;
        assert!(
            (6.0..18.0).contains(&drop),
            "pink should fall across the shelves by about 12 dB, fell {drop:.1} dB"
        );
    }

    #[test]
    fn white_and_pink_differ_and_both_are_bounded() {
        let w = render(Colour::White, 2048);
        let p = render(Colour::Pink, 2048);
        assert_ne!(w, p);
        for v in w.iter().chain(p.iter()) {
            assert!(v.is_finite() && v.abs() <= 2.0, "{v}");
        }
    }

    #[test]
    fn two_instances_render_identically() {
        assert_eq!(render(Colour::Pink, 1000), render(Colour::Pink, 1000));
    }

    #[test]
    fn reset_restarts_the_sequence() {
        let mut n = Noise::new(48_000.0);
        let first: Vec<f32> = (0..100).map(|_| n.process(Colour::White)).collect();
        n.reset();
        let again: Vec<f32> = (0..100).map(|_| n.process(Colour::White)).collect();
        assert_eq!(first, again);
    }
}
