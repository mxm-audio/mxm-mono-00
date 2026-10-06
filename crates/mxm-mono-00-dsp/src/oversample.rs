//! Two-times oversampling for the ladder: a half-band interpolator and decimator pair.
//!
//! **Why the ladder and nothing else.** The hardware's VCF runs 20 Hz – 20 kHz. The discrete
//! ladder clamps its poles below 45 % of the rate it runs at, so at 48 kHz the widest pole
//! (2.6 × the cutoff) capped the nominal cutoff at 8.3 kHz and the oscillation at about 11 kHz —
//! the owner's "it goes no higher than 10 kHz". Run at twice the rate the ceiling is 16.6 kHz,
//! and above it the poles clamp one by one (`filter.rs`) so the control reaches 20 kHz. The
//! oscillators are PolyBLEP and need no oversampling; the VCA and effects sit after the
//! decimator at the base rate.
//!
//! **The filter is a half-band FIR, computed at construction**: a Kaiser-windowed sinc with
//! every even tap but the centre zero, so each direction costs [`HALF_TAPS`] multiplies per base
//! sample. Its transition band straddles the base Nyquist; the stopband is where the ladder's
//! harmonics and an oscillation above 24 kHz would otherwise alias. **Chosen**: [`HALF_TAPS`] and
//! [`KAISER_BETA`], listed in the crate's `NOTES.md`, and measured by the tests below.
//!
//! **Latency.** The pair delays the ladder's path by [`LATENCY_SAMPLES`] base samples, about half
//! a millisecond at 48 kHz. The VCA's gate and envelope act on the delayed audio, which is what
//! the hardware's amplifier does to its filter's output in any case. The plugin does not report
//! the figure — its AGENTS.md says why.

use crate::flush;

/// Nonzero taps on each side of the centre — the odd-indexed ones. The full kernel has
/// `4 * HALF_TAPS + 1` taps.
pub const HALF_TAPS: usize = 11;
/// The Kaiser window's shape: about −70 dB in the stopband, a transition of roughly ±10 % of the
/// doubled rate around the base Nyquist.
pub const KAISER_BETA: f64 = 7.0;
/// The kernel's centre index, and the delay each direction adds at the doubled rate.
const CENTRE: usize = 2 * HALF_TAPS;
/// The round trip's delay at the base rate: the centre each way, halved for the rate.
pub const LATENCY_SAMPLES: usize = CENTRE;

/// The half-band kernel's odd taps, one side, nearest the centre first. The centre tap is 0.5.
fn odd_taps() -> [f32; HALF_TAPS] {
    let mut taps = [0.0f32; HALF_TAPS];
    let denominator = bessel_i0(KAISER_BETA);
    for (j, tap) in taps.iter_mut().enumerate() {
        let k = (2 * j + 1) as f64;
        let sinc = (std::f64::consts::FRAC_PI_2 * k).sin() / (std::f64::consts::PI * k);
        let x = k / CENTRE as f64;
        let window = bessel_i0(KAISER_BETA * (1.0 - x * x).max(0.0).sqrt()) / denominator;
        *tap = (sinc * window) as f32;
    }
    taps
}

/// The modified Bessel function of the first kind, order zero, by its power series.
fn bessel_i0(x: f64) -> f64 {
    let half = x / 2.0;
    let mut term = 1.0;
    let mut sum = 1.0;
    for k in 1..40 {
        term *= (half / k as f64) * (half / k as f64);
        sum += term;
        if term < 1e-17 * sum {
            break;
        }
    }
    sum
}

/// The pair: one input sample in, two out to the ladder, two back, one out.
///
/// With `H = HALF_TAPS` and the kernel's centre at `2H`: the interpolator's even phase is
/// `x[n - H]` — the centre tap alone, the zero-stuffed stream's only even contribution — and its
/// odd phase pairs `x[n - H - j]` with `x[n - H + 1 + j]` under the `j`th odd tap. The decimator
/// is the same kernel the other way: `½ · e[n - H]` plus `o[n - H - 1 - j]` with `o[n - H + j]`
/// under the `j`th odd tap.
#[derive(Debug, Clone)]
pub struct Oversampler {
    taps: [f32; HALF_TAPS],
    /// Base-rate input history, newest at `up_head`.
    up: [f32; 2 * HALF_TAPS],
    up_head: usize,
    /// The ladder's even-phase output history, for the decimator's centre tap.
    even: [f32; HALF_TAPS + 1],
    even_head: usize,
    /// The ladder's odd-phase output history, for the decimator's odd taps.
    odd: [f32; 2 * HALF_TAPS + 1],
    odd_head: usize,
}

impl Default for Oversampler {
    fn default() -> Self {
        Self::new()
    }
}

impl Oversampler {
    pub fn new() -> Self {
        Self {
            taps: odd_taps(),
            up: [0.0; 2 * HALF_TAPS],
            up_head: 0,
            even: [0.0; HALF_TAPS + 1],
            even_head: 0,
            odd: [0.0; 2 * HALF_TAPS + 1],
            odd_head: 0,
        }
    }

    pub fn reset(&mut self) {
        self.up = [0.0; 2 * HALF_TAPS];
        self.even = [0.0; HALF_TAPS + 1];
        self.odd = [0.0; 2 * HALF_TAPS + 1];
        self.up_head = 0;
        self.even_head = 0;
        self.odd_head = 0;
    }

    /// One base-rate sample in; the two samples at the doubled rate out, in time order.
    ///
    /// The gain of two a zero-stuffed stream needs is applied here; the centre tap's half then
    /// makes the even phase the input itself, delayed.
    #[inline]
    pub fn up(&mut self, x: f32) -> (f32, f32) {
        const LEN: usize = 2 * HALF_TAPS;
        self.up_head = (self.up_head + 1) % LEN;
        self.up[self.up_head] = x;
        let at = |i: usize| self.up[(self.up_head + LEN - i) % LEN];
        let even = at(HALF_TAPS);
        let mut odd = 0.0f32;
        for (j, tap) in self.taps.iter().enumerate() {
            odd += tap * (at(HALF_TAPS + j) + at(HALF_TAPS - 1 - j));
        }
        (even, 2.0 * odd)
    }

    /// The two samples at the doubled rate back, in time order; one base-rate sample out.
    #[inline]
    pub fn down(&mut self, even: f32, odd: f32) -> f32 {
        const LEN_EVEN: usize = HALF_TAPS + 1;
        const LEN_ODD: usize = 2 * HALF_TAPS + 1;
        self.even_head = (self.even_head + 1) % LEN_EVEN;
        self.even[self.even_head] = even;
        self.odd_head = (self.odd_head + 1) % LEN_ODD;
        self.odd[self.odd_head] = odd;
        let odd_at = |i: usize| self.odd[(self.odd_head + LEN_ODD - i) % LEN_ODD];
        let mut sum = 0.5 * self.even[(self.even_head + LEN_EVEN - HALF_TAPS) % LEN_EVEN];
        for (j, tap) in self.taps.iter().enumerate() {
            sum += tap * (odd_at(HALF_TAPS + 1 + j) + odd_at(HALF_TAPS - j));
        }
        flush(sum)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const FS: f32 = 48_000.0;

    fn rms(x: &[f32]) -> f32 {
        (x.iter().map(|v| v * v).sum::<f32>() / x.len() as f32).sqrt()
    }

    #[test]
    fn the_odd_taps_sum_to_a_half_and_the_kernel_is_unity_at_dc() {
        let taps = odd_taps();
        let sum: f32 = taps.iter().sum();
        assert!(
            (2.0 * sum - 0.5).abs() < 1e-3,
            "odd taps sum to {}",
            2.0 * sum
        );
    }

    #[test]
    fn silence_in_is_exact_silence_out() {
        let mut o = Oversampler::new();
        for _ in 0..1000 {
            let (e, d) = o.up(0.0);
            assert_eq!((e, d), (0.0, 0.0));
            assert_eq!(o.down(e, d), 0.0);
        }
    }

    /// Up and straight back down is the input, delayed by the stated latency, to −60 dB.
    #[test]
    fn a_round_trip_is_a_delay() {
        let mut o = Oversampler::new();
        let n = 4_800;
        let input: Vec<f32> = (0..n)
            .map(|i| {
                let t = i as f32 / FS;
                0.5 * (std::f32::consts::TAU * 1_000.0 * t).sin()
                    + 0.3 * (std::f32::consts::TAU * 7_000.0 * t).sin()
            })
            .collect();
        let output: Vec<f32> = input
            .iter()
            .map(|&x| {
                let (e, d) = o.up(x);
                o.down(e, d)
            })
            .collect();
        let error: Vec<f32> = (LATENCY_SAMPLES + 200..n)
            .map(|i| output[i] - input[i - LATENCY_SAMPLES])
            .collect();
        let db = 20.0 * (rms(&error) / rms(&input)).log10();
        assert!(db < -60.0, "round-trip error {db:.1} dB");
    }

    /// A tone above the base Nyquist, injected at the doubled rate, does not survive decimation.
    #[test]
    fn the_decimator_removes_what_is_above_the_base_nyquist() {
        let mut o = Oversampler::new();
        let n = 4_800;
        let fs2 = 2.0 * FS;
        let output: Vec<f32> = (0..n)
            .map(|i| {
                let t0 = (2 * i) as f32 / fs2;
                let t1 = (2 * i + 1) as f32 / fs2;
                let tone = |t: f32| (std::f32::consts::TAU * 32_000.0 * t).sin();
                o.down(tone(t0), tone(t1))
            })
            .collect();
        let db = 20.0 * rms(&output[200..]).log10();
        assert!(db < -50.0, "32 kHz leaked through at {db:.1} dB");
    }

    /// The interpolator's odd phase lands between the base samples: a low tone read at the
    /// doubled rate is the same tone.
    #[test]
    fn the_interpolator_fills_the_gaps_with_the_same_tone() {
        let mut o = Oversampler::new();
        let n = 4_800;
        let mut stream = Vec::with_capacity(2 * n);
        for i in 0..n {
            let t = i as f32 / FS;
            let (e, d) = o.up((std::f32::consts::TAU * 2_000.0 * t).sin());
            stream.push(e);
            stream.push(d);
        }
        // Against the ideal, allowing the centre's delay at the doubled rate.
        let error: Vec<f32> = (2 * CENTRE + 400..2 * n)
            .map(|k| {
                let t = (k - CENTRE) as f32 / (2.0 * FS);
                stream[k] - (std::f32::consts::TAU * 2_000.0 * t).sin()
            })
            .collect();
        let db = 20.0 * (rms(&error) / 0.707).log10();
        assert!(db < -60.0, "interpolation error {db:.1} dB");
    }
}
