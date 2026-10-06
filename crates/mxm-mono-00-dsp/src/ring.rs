//! The 102's ring modulator: an LM1496 balanced modulator with its carriers nulled.
//!
//! `research:instruments/system-100.md` §10.2: input X is always the 102's VCO; input Y is the EXT
//! INPUT jack or the 102's LFO by switch, and nothing is normalled — the manual's bell patch says
//! that with the cord removed "you will get no sound". The plug-out normals Y to VCO-1 (manual
//! p. 17) and the voice follows the plug-out, so the matrix's RING MOD IN row defaults to VCO-1's
//! output and the LFO option is LFO-2's column into the same row.
//!
//! A balanced modulator with its SIG BAL and MOD BAL trims set is a four-quadrant multiplier, and
//! that is the whole model: **the product, with no carrier leak**. The leak a real LM1496 has past
//! its trims is unmeasured and not invented.
//!
//! **Aliasing is the product's, and it is measured** (`ring_modulation_aliases_no_worse_than_this`):
//! two band-limited inputs multiply to twice the bandwidth, and what lands above Nyquist folds.
//! The test records the figure at a representative pair of sawtooths against an oversampled
//! reference; whether that figure calls for oversampling is the plan's phase 1 question, and the
//! answer recorded in the crate's AGENTS.md is *not yet*.

use crate::flush;

/// The product of two bounded inputs is bounded by their product; the VCOs are `|x| <= 1.5`
/// (PolyBLEP overshoot), so this is the ring modulator's stated bound.
pub const OUTPUT_BOUND: f32 = 1.5 * 1.5;

/// One sample of ring modulation.
#[inline]
pub fn ring(x: f32, y: f32) -> f32 {
    flush(x * y)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::oscillator::{Vco, Wave};

    #[test]
    fn a_product_of_two_sines_is_their_sum_and_difference_and_nothing_else() {
        // The textbook: sin(a)·sin(b) = ½[cos(a−b) − cos(a+b)]. A direct DFT on an exactly
        // periodic window finds energy only at the two bins.
        const N: usize = 2048;
        let (ka, kb) = (21usize, 34usize);
        let x: Vec<f64> = (0..N)
            .map(|i| {
                let t = i as f64 / N as f64;
                let a = (std::f64::consts::TAU * ka as f64 * t).sin() as f32;
                let b = (std::f64::consts::TAU * kb as f64 * t).sin() as f32;
                ring(a, b) as f64
            })
            .collect();
        let power_at = |bin: usize| {
            let (mut re, mut im) = (0.0f64, 0.0f64);
            for (i, &v) in x.iter().enumerate() {
                let ang = -std::f64::consts::TAU * bin as f64 * i as f64 / N as f64;
                re += v * ang.cos();
                im += v * ang.sin();
            }
            re * re + im * im
        };
        let wanted = power_at(kb - ka) + power_at(kb + ka);
        let mut other = 0.0;
        for bin in 1..N / 2 {
            if bin != kb - ka && bin != kb + ka {
                other += power_at(bin);
            }
        }
        assert!(
            other < wanted * 1e-8,
            "energy off the sum and difference: {other} vs {wanted}"
        );
        assert_eq!(
            ring(0.7, 0.0),
            0.0,
            "and one silent input silences the product"
        );
    }

    /// The product of two band-limited sawtooths at 48 kHz against the same product rendered at
    /// four times the rate and brought back down through a windowed-sinc lowpass. The difference
    /// is the aliasing the 48 kHz product folded in, plus the decimator's own error, which the
    /// test bounds by running an unmodulated saw through the same comparison first.
    #[test]
    fn ring_modulation_aliases_no_worse_than_this() {
        let fs = 48_000.0f32;
        let (f1, f2) = (440.0f32, 660.0f32);
        let n = 4096usize;
        // Odd, and with a half-length that is a multiple of four, so the decimator's group delay
        // is a whole number of samples at the lower rate.
        let taps = 129usize;

        let render = |rate: f32, oversample: usize, modulate: bool| -> Vec<f32> {
            let mut a = Vco::new();
            let mut b = Vco::new();
            (0..n * oversample + taps)
                .map(|_| {
                    let x = a.process(f1, Wave::Saw, 0.5, rate);
                    let y = b.process(f2, Wave::Saw, 0.5, rate);
                    if modulate { ring(x, y) } else { x }
                })
                .collect()
        };
        // Blackman-windowed sinc at 0.4·fs, applied at 4·fs, then every fourth sample.
        let decimate = |x: &[f32]| -> Vec<f32> {
            let cutoff = 0.4 / 4.0;
            let mid = (taps / 2) as f32;
            let kernel: Vec<f32> = (0..taps)
                .map(|k| {
                    let m = k as f32 - mid;
                    let sinc = if m == 0.0 {
                        2.0 * cutoff
                    } else {
                        (std::f32::consts::TAU * cutoff * m).sin() / (std::f32::consts::PI * m)
                    };
                    let w = 0.42
                        - 0.5 * (std::f32::consts::TAU * k as f32 / (taps - 1) as f32).cos()
                        + 0.08 * (2.0 * std::f32::consts::TAU * k as f32 / (taps - 1) as f32).cos();
                    sinc * w
                })
                .collect();
            (0..n)
                .map(|i| {
                    let centre = i * 4 + taps / 2;
                    kernel
                        .iter()
                        .enumerate()
                        .map(|(k, &c)| c * x[centre + k - taps / 2])
                        .sum()
                })
                .collect()
        };
        let error_db = |modulate: bool| {
            let direct = render(fs, 1, modulate);
            let reference = decimate(&render(fs * 4.0, 4, modulate));
            // `reference[i]` is centred on oversampled index `4i + taps/2`: time `i + taps/8`.
            let offset = taps / 2 / 4;
            let (mut err, mut sig) = (0.0f64, 0.0f64);
            for i in 0..n {
                let d = direct[i + offset] as f64;
                let r = reference[i] as f64;
                err += (d - r) * (d - r);
                sig += r * r;
            }
            10.0 * (err / sig).log10()
        };
        let floor = error_db(false);
        let modulated = error_db(true);
        eprintln!("ring modulation aliasing: {modulated:.1} dB (measurement floor {floor:.1} dB)");
        // The floor is the PolyBLEP saw's own aliasing plus the band the decimator removes; the
        // modulated figure is what the crate's NOTES.md records.
        assert!(
            floor < -20.0,
            "the comparison's own floor is too high: {floor:.1} dB"
        );
        assert!(
            modulated < -15.0,
            "ring modulation aliasing {modulated:.1} dB"
        );
    }
}
