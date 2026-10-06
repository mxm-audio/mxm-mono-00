//! The passive one-pole high-pass in front of the ladder.
//!
//! `research:instruments/system-100.md` §8.1: the mixer's sum passes through a capacitor with the
//! HPF CUTOFF FREQ slider — a 1 MΩ pot — as the shunt to ground. A single RC, 6 dB per octave,
//! 10 Hz to 10 kHz, no gain stage, no resonance, no CV input. "Normal position is 0", which passes
//! everything. It is a separate filter from the VCF and the manual says so.
//!
//! The one-pole TPT form `mxm-poly-06-dsp` uses, as a highpass.

use crate::flush;

pub const CUTOFF_MIN_HZ: f32 = 10.0;
pub const CUTOFF_MAX_HZ: f32 = 10_000.0;
pub const NYQUIST_FRACTION: f32 = 0.45;

/// `tan(x)` via the [5/4] Padé approximant, for `x` in `[0, π · 0.45]`.
#[inline]
pub fn tan_approx(x: f64) -> f64 {
    let x2 = x * x;
    let x4 = x2 * x2;
    x * (945.0 - 105.0 * x2 + x4) / (945.0 - 420.0 * x2 + 15.0 * x4)
}

#[derive(Debug, Clone, Copy, Default)]
pub struct Hpf {
    big_g: f32,
    s: f32,
    cutoff: f32,
}

impl Hpf {
    pub const fn new() -> Self {
        Self {
            big_g: 0.0,
            s: 0.0,
            cutoff: -1.0,
        }
    }

    pub fn reset(&mut self) {
        self.s = 0.0;
    }

    #[inline]
    fn set_cutoff(&mut self, cutoff_hz: f32, sample_rate: f32) {
        let fc = cutoff_hz
            .clamp(CUTOFF_MIN_HZ, CUTOFF_MAX_HZ)
            .min(NYQUIST_FRACTION * sample_rate);
        if fc != self.cutoff {
            self.cutoff = fc;
            let g =
                tan_approx(std::f64::consts::PI * f64::from(fc) / f64::from(sample_rate)) as f32;
            self.big_g = g / (1.0 + g);
        }
    }

    /// One sample of highpass at `cutoff_hz`.
    #[inline]
    pub fn process(&mut self, x: f32, cutoff_hz: f32, sample_rate: f32) -> f32 {
        self.set_cutoff(cutoff_hz, sample_rate);
        let v = self.big_g * (x - self.s);
        let low = v + self.s;
        self.s = flush(low + v);
        x - low
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn magnitude_db(cutoff: f32, hz: f32, fs: f32) -> f32 {
        let mut f = Hpf::new();
        let n = (fs * 0.5) as usize;
        let mut peak = 0.0f32;
        for i in 0..n * 2 {
            let x = (std::f32::consts::TAU * hz * i as f32 / fs).sin();
            let y = f.process(x, cutoff, fs);
            if i >= n {
                peak = peak.max(y.abs());
            }
        }
        20.0 * peak.log10()
    }

    #[test]
    fn at_its_bottom_it_passes_everything() {
        // "Normal position is 0": a 40 Hz fundamental through a 10 Hz corner loses under a dB.
        let db = magnitude_db(CUTOFF_MIN_HZ, 40.0, 48_000.0);
        assert!(db > -0.5, "40 Hz through the HPF at 10 Hz: {db:.2} dB");
    }

    #[test]
    fn it_is_three_db_down_at_its_corner_and_six_per_octave_below() {
        for fs in [44_100.0f32, 48_000.0, 96_000.0] {
            let corner = magnitude_db(1_000.0, 1_000.0, fs);
            assert!(
                (corner + 3.01).abs() < 0.3,
                "at {fs}: {corner:.2} dB at the corner"
            );
            let octave_below = magnitude_db(1_000.0, 500.0, fs);
            let two_below = magnitude_db(1_000.0, 250.0, fs);
            let slope = octave_below - two_below;
            assert!((slope - 6.0).abs() < 0.8, "slope {slope:.2} dB/oct");
        }
    }

    #[test]
    fn silence_stays_exactly_zero() {
        let mut f = Hpf::new();
        f.process(1.0, 1_000.0, 48_000.0);
        for _ in 0..100_000 {
            f.process(0.0, 1_000.0, 48_000.0);
        }
        assert_eq!(f.process(0.0, 1_000.0, 48_000.0), 0.0);
    }
}
