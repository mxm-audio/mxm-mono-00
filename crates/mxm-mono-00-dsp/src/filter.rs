//! The diode ladder — Roland's first, seven years before the 303's.
//!
//! **`crates/mxm-mono-03-dsp/src/filter.rs`, copied whole, with one configuration added.** The
//! pole-set model is that crate's: four TPT one-poles at spread positions with a saturating global
//! feedback solved by Newton iteration, the spread being the whole linear character of a diode
//! ladder (`research:filters/machines/tb303-diode-ladder.md` §2). Nothing about the model changes for
//! the System-100; what changes is the pole set.
//!
//! # The System-100's pole set is chosen, not derived
//!
//! `research:instruments/system-100.md` §8.2 and §13: the service notes give eight transistors wired
//! as diodes, **four equal rung capacitors** (0.068 µF) and a small capacitor across the input
//! pair, and say nothing about which transistors are the rungs and which the diode strings. That
//! is not enough to derive pole positions the way Stinchcombe did for the 303's unequal rungs, so
//! [`DiodeConfig::System100`] is a **chosen** set: narrower than the 303's, because equal rung
//! capacitors load each other more evenly, and stated so a measurement can replace it. The
//! self-oscillation point it produces — "somewhere near 8" on the resonance slider, per the manual —
//! is what the resonance mapping is normalised to, not a number in hertz.
//!
//! **The bass that "declines slightly" is not compensated here.** A diode ladder's DC gain falls
//! as `1/(1+k)`, and the manual's figure 1-29 shows only a small decline; `mxm-mono-03-dsp`'s rule
//! is *compensate outside the filter, never inside*, and this crate's voice does that with a
//! chosen amount labelled against that qualitative source. The mechanism on the hardware — the
//! second gang of the resonance pot — is unverified (§13).

use crate::{Rng, flush, tanh_approx};
use std::f32::consts::PI;

/// Newton steps used to solve the resonance feedback each sample. Three is enough because the
/// equation is strictly monotonic — see [`DiodeLadder::process`].
const NEWTON_ITERATIONS: usize = 3;

/// How far past its own threshold the top of the resonance control reaches: **the machine's
/// number**, not the copied 303's. The 101 self-oscillates "somewhere near 8" on its 0–10 RESONANCE
/// scale (`system-100.md` §3.1), so the threshold sits at 0.8 of the control and the top is 1.25
/// times it. The copy arrived with 1.145 — the threshold at 0.87 — and the owner found the control
/// needed "above 90 %" to sing.
pub const RESONANCE_MARGIN: f32 = 1.25;

/// The largest `k` any configuration can be asked for: the TB-303's threshold where the discrete
/// correction peaks (about 2.9×, with its widest poles pinned at the Nyquist limit and the rest
/// still climbing), times the margin — 66.8. Used only to state [`OUTPUT_BOUND`];
/// `no_configuration_asks_for_more_than_k_max` measures it.
pub const K_MAX: f32 = 70.0;

/// How many cutoff-to-sample-rate ratios the discrete-threshold correction is tabulated at.
const CORRECTION_TABLE: usize = 256;
/// The lowest ratio in the table: 20 Hz at 768 kHz is 2.6e-5, and below the table the correction
/// is unity to four decimals.
const CORRECTION_RATIO_MIN: f32 = 1e-5;

/// Above this resonance the filter is excited so self-oscillation can start from silence:
/// **the singing point itself**, `1 / RESONANCE_MARGIN`. The copy had it at 0.9 with the
/// threshold at 0.87, which left a band where the loop was past its threshold and nothing
/// started it — the owner's "it seems to need to be above 90 %".
pub const EXCITATION_THRESHOLD: f32 = 1.0 / RESONANCE_MARGIN;

/// Amplitude of that excitation. About -100 dB: inaudible, and a decade above the copy's
/// -120 dB so the ring-up just past the threshold, where the loop gain barely exceeds one, takes
/// a second rather than ten.
pub const EXCITATION_LEVEL: f32 = 1e-5;

/// Lowest nominal cutoff, in Hz — the panel's (§3.1).
pub const CUTOFF_MIN_HZ: f32 = 20.0;

/// The highest fraction of the sample rate any single pole may sit at.
pub const NYQUIST_FRACTION: f32 = 0.45;

/// The largest input the voice will hand this filter: unity from the mixer's saturator times the
/// voice's bass compensation at full resonance. Stated here so [`OUTPUT_BOUND`] can be.
pub const INPUT_BOUND: f32 = 1.0 + crate::voice::BASS_COMPENSATION * K_MAX;

/// Output bound, and the argument that establishes it.
///
/// The only path back into the loop is through [`tanh_approx`], whose magnitude never exceeds 1,
/// so the injected feedback is bounded by `k` regardless of internal state and the ladder input
/// satisfies `|x| <= INPUT_BOUND + k`. Each TPT one-pole has unity DC gain, so that bounds the
/// steady state; the extra margin covers transient overshoot while coefficients are moving, and is
/// **measured** by `output_stays_within_the_stated_bound_under_overdrive`.
pub const OUTPUT_BOUND: f32 = INPUT_BOUND + K_MAX + 4.0;

/// Which diode ladder configuration to model. Same code, different pole positions.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum DiodeConfig {
    /// **Chosen** for the System-100's equal-rung ladder — see the module doc.
    #[default]
    System100,
    /// Stinchcombe's identification of the TB-303's configuration, kept so the two can be compared
    /// and so the copy stays honest.
    Tb303,
}

impl DiodeConfig {
    /// Pole positions in units of the nominal cutoff, at `k = 0`.
    pub const fn poles(self) -> [f32; 4] {
        match self {
            DiodeConfig::System100 => [2.6, 1.8, 1.0, 0.2],
            DiodeConfig::Tb303 => [3.532, 2.347, 1.000, 0.121],
        }
    }

    /// The widest pole, which is what bounds the usable cutoff range.
    pub fn widest_pole(self) -> f32 {
        let p = self.poles();
        let mut w = p[0];
        let mut i = 1;
        while i < 4 {
            if p[i] > w {
                w = p[i];
            }
            i += 1;
        }
        w
    }

    /// Loop gain at which this configuration oscillates, in closed form: `s = jw` in the
    /// closed-loop denominator, real and imaginary parts separated, `e_n` the elementary
    /// symmetric polynomials of the poles.
    pub fn threshold(self) -> f32 {
        let p = self.poles();
        let (e1, e2, e3, e4) = symmetric(p);
        let w2 = e3 / e1;
        (e2 * w2 - w2 * w2) / e4 - 1.0
    }

    /// Frequency of self-oscillation, relative to the nominal cutoff, in the analogue prototype.
    pub fn oscillation_ratio(self) -> f32 {
        let p = self.poles();
        let (e1, _, e3, _) = symmetric(p);
        (e3 / e1).sqrt()
    }

    /// The loop gain at which the **discrete** ladder oscillates at a cutoff of `ratio` times the
    /// sample rate, and the frequency it sings at relative to that cutoff.
    ///
    /// The closed form above is the analogue prototype's. Each TPT one-pole is exact at its own
    /// corner and warps above it — its phase reaches −90° at Nyquist whatever its corner — so the
    /// cascade reaches −180° with less gain than the prototype has there, and the `k` that
    /// oscillates rises with the cutoff: ×1.15 at a tenth of the sample rate, ×1.8 at the clamp.
    /// Without this the top of the resonance control stopped singing above about 5 kHz at
    /// 48 kHz, which the owner found from the init patch's 10 kHz cutoff.
    ///
    /// Each stage is `G (z + 1) / (z − (1 − 2G))`, whose phase falls monotonically from 0 to
    /// −90°, so the cascade's phase is monotonic and one bisection finds the −180° crossing.
    pub fn discrete_threshold(self, ratio: f32) -> (f32, f32) {
        let poles = self.poles();
        let mut big_g = [0.0f64; 4];
        for i in 0..4 {
            let fc = (f64::from(poles[i]) * f64::from(ratio)).min(f64::from(NYQUIST_FRACTION));
            let g = (std::f64::consts::PI * fc).tan();
            big_g[i] = g / (1.0 + g);
        }
        let response = |w: f64| -> (f64, f64) {
            let (sin, cos) = w.sin_cos();
            let mut phase = 0.0;
            let mut magnitude = 1.0;
            for g in big_g {
                let a = 1.0 - 2.0 * g;
                phase += 0.5 * w - sin.atan2(cos - a);
                let numerator = ((cos + 1.0).powi(2) + sin * sin).sqrt();
                let denominator = ((cos - a).powi(2) + sin * sin).sqrt();
                magnitude *= g * numerator / denominator;
            }
            (phase, magnitude)
        };
        let (mut lo, mut hi) = (1e-7f64, std::f64::consts::PI - 1e-9);
        for _ in 0..60 {
            let w = 0.5 * (lo + hi);
            if response(w).0 > -std::f64::consts::PI {
                lo = w;
            } else {
                hi = w;
            }
        }
        let w = 0.5 * (lo + hi);
        let (_, magnitude) = response(w);
        let k = 1.0 / magnitude;
        let sung = w / (2.0 * std::f64::consts::PI * f64::from(ratio));
        (k as f32, sung as f32)
    }

    /// The highest cutoff-to-sample-rate ratio any configuration is ever run at: the nominal
    /// cutoff at the Nyquist limit, every pole pinned there.
    fn ratio_max(self) -> f32 {
        NYQUIST_FRACTION
    }
}

fn symmetric(p: [f32; 4]) -> (f32, f32, f32, f32) {
    let e1 = p[0] + p[1] + p[2] + p[3];
    let e2 = p[0] * p[1] + p[0] * p[2] + p[0] * p[3] + p[1] * p[2] + p[1] * p[3] + p[2] * p[3];
    let e3 = p[0] * p[1] * p[2] + p[0] * p[1] * p[3] + p[0] * p[2] * p[3] + p[1] * p[2] * p[3];
    let e4 = p[0] * p[1] * p[2] * p[3];
    (e1, e2, e3, e4)
}

/// The highest nominal cutoff at which every pole stays below the Nyquist limit, and so the
/// highest at which the pole *ratios* are exact. **Above it the filter still runs**: each pole
/// clamps at the limit on its own, the widest first, so the control reaches the hardware's 20 kHz
/// at any rate rather than stopping at 8.3 kHz at 48 kHz — the owner's "it goes no higher than
/// 10 kHz". Between here and the top the shape is no longer the ladder's, which is why the voice
/// runs the ladder at twice the rate and puts this ceiling at 16.6 kHz.
pub fn max_nominal_cutoff_hz(config: DiodeConfig, sample_rate: f32) -> f32 {
    NYQUIST_FRACTION * sample_rate / config.widest_pole()
}

/// `tan` for the prewarp, in `f64` because `tan` near `pi/2` loses significance fast.
#[inline]
fn tan_prewarp(x: f64) -> f64 {
    x.tan()
}

/// A 4-pole diode ladder lowpass.
#[derive(Debug, Clone)]
pub struct DiodeLadder {
    config: DiodeConfig,
    s: [f32; 4],
    y_prev: f32,
    rng: Rng,
    /// `discrete_threshold / threshold` and the sung frequency ratio, tabulated over the log of
    /// the cutoff-to-sample-rate ratio from [`CORRECTION_RATIO_MIN`] to the configuration's
    /// clamp. Built once here, on no audio thread; read per sample by [`Self::lookup`].
    correction: [(f32, f32); CORRECTION_TABLE],
}

impl Default for DiodeLadder {
    fn default() -> Self {
        Self::new(DiodeConfig::System100)
    }
}

impl DiodeLadder {
    pub fn new(config: DiodeConfig) -> Self {
        let analog = config.threshold();
        let span = (config.ratio_max() / CORRECTION_RATIO_MIN).ln();
        let correction = std::array::from_fn(|i| {
            let ratio =
                CORRECTION_RATIO_MIN * (span * i as f32 / (CORRECTION_TABLE - 1) as f32).exp();
            let (k, sung) = config.discrete_threshold(ratio);
            (k / analog, sung)
        });
        Self {
            config,
            s: [0.0; 4],
            y_prev: 0.0,
            rng: Rng::new(0x5EED_0100),
            correction,
        }
    }

    /// The correction and the sung ratio at a nominal cutoff, interpolated in the log of the
    /// cutoff-to-sample-rate ratio.
    #[inline]
    fn lookup(&self, cutoff_hz: f32, sample_rate: f32) -> (f32, f32) {
        let ratio = (cutoff_hz / sample_rate).clamp(CORRECTION_RATIO_MIN, self.config.ratio_max());
        let span = (self.config.ratio_max() / CORRECTION_RATIO_MIN).ln();
        let position = (ratio / CORRECTION_RATIO_MIN).ln() / span * (CORRECTION_TABLE - 1) as f32;
        let index = (position as usize).min(CORRECTION_TABLE - 2);
        let frac = (position - index as f32).clamp(0.0, 1.0);
        let (a, b) = (self.correction[index], self.correction[index + 1]);
        (a.0 + (b.0 - a.0) * frac, a.1 + (b.1 - a.1) * frac)
    }

    pub fn config(&self) -> DiodeConfig {
        self.config
    }

    /// Clear all state. Leaves no tail from previous playback.
    pub fn reset(&mut self) {
        self.s = [0.0; 4];
        self.y_prev = 0.0;
        self.rng = Rng::new(0x5EED_0100);
    }

    /// Map a normalised resonance `0..=1` onto **this configuration's own** `k` at this cutoff, so
    /// the slider's singing point sits where the manual puts it whichever set is loaded **and
    /// wherever the cutoff is** — the top of the control is `RESONANCE_MARGIN` past the discrete
    /// ladder's threshold at that cutoff, as it is past the analogue one at low cutoffs.
    #[inline]
    pub fn native_resonance(&self, resonance: f32, cutoff_hz: f32, sample_rate: f32) -> f32 {
        let fc = cutoff_hz.clamp(CUTOFF_MIN_HZ, NYQUIST_FRACTION * sample_rate);
        let (correction, _) = self.lookup(fc, sample_rate);
        self.config.threshold() * correction * RESONANCE_MARGIN * resonance.clamp(0.0, 1.0)
    }

    /// Where this filter sings, for a given nominal cutoff at a given sample rate: the analogue
    /// ratio (+128 cents for the System-100 set) low down, warping upward toward the clamp.
    #[inline]
    pub fn oscillation_hz(&self, cutoff_hz: f32, sample_rate: f32) -> f32 {
        let fc = cutoff_hz.clamp(CUTOFF_MIN_HZ, NYQUIST_FRACTION * sample_rate);
        fc * self.lookup(fc, sample_rate).1
    }

    /// Process one sample. `resonance` is `0..=1`. Coefficients are recomputed every sample so
    /// per-sample envelope and matrix modulation of the cutoff takes effect.
    #[inline]
    pub fn process(&mut self, input: f32, cutoff_hz: f32, resonance: f32, sample_rate: f32) -> f32 {
        let fc = cutoff_hz.clamp(CUTOFF_MIN_HZ, NYQUIST_FRACTION * sample_rate);
        let poles = self.config.poles();
        let limit = NYQUIST_FRACTION * sample_rate;

        let mut big_g = [0.0f32; 4];
        let mut inv = [0.0f32; 4];
        for i in 0..4 {
            // Each pole below the limit on its own: above `max_nominal_cutoff_hz` the widest
            // pins there while the rest keep climbing, which is what lets the control reach the
            // top of its range.
            let g = tan_prewarp((PI * (fc * poles[i]).min(limit) / sample_rate) as f64) as f32;
            big_g[i] = g / (1.0 + g);
            inv[i] = 1.0 / (1.0 + g);
        }

        let resonance = resonance.clamp(0.0, 1.0);
        let k = self.native_resonance(resonance, fc, sample_rate);

        let mut u = input;
        if resonance > EXCITATION_THRESHOLD {
            u += EXCITATION_LEVEL * self.rng.next_bipolar();
        }

        // F(y) = A - P*k*tanh(y) - y = 0, P = G0*G1*G2*G3, A = P*u + S — one root, since F' <= -1.
        let p = big_g[0] * big_g[1] * big_g[2] * big_g[3];
        let a = p * u
            + big_g[1] * big_g[2] * big_g[3] * (self.s[0] * inv[0])
            + big_g[2] * big_g[3] * (self.s[1] * inv[1])
            + big_g[3] * (self.s[2] * inv[2])
            + (self.s[3] * inv[3]);

        let mut y_solved = self.y_prev;
        for _ in 0..NEWTON_ITERATIONS {
            let t = tanh_approx(y_solved);
            let f = a - p * k * t - y_solved;
            let df = -p * k * (1.0 - t * t) - 1.0;
            y_solved -= f / df;
        }

        let x = u - k * tanh_approx(y_solved);

        let mut y = x;
        for (state, g) in self.s.iter_mut().zip(big_g.iter()) {
            let v = (y - *state) * g;
            y = v + *state;
            *state = flush(y + v);
        }

        self.y_prev = flush(y);
        y
    }
}

/// Measure the resonance `k` at which the filter starts to self-oscillate, on the running filter.
pub fn measure_oscillation_threshold(config: DiodeConfig, cutoff_hz: f32, sample_rate: f32) -> f32 {
    let position = measure_oscillation_position(config, cutoff_hz, sample_rate);
    DiodeLadder::new(config).native_resonance(position, cutoff_hz, sample_rate)
}

/// Measure the **normalised resonance** at which the running filter starts to self-oscillate —
/// the slider position, which is what the correction is meant to hold still across the cutoff.
pub fn measure_oscillation_position(config: DiodeConfig, cutoff_hz: f32, sample_rate: f32) -> f32 {
    let sustains = |resonance: f32| {
        let mut f = DiodeLadder::new(config);
        f.process(1.0, cutoff_hz, resonance, sample_rate);
        let settle = (sample_rate * 0.20) as usize;
        for _ in 0..settle {
            f.process(0.0, cutoff_hz, resonance, sample_rate);
        }
        let mut early = 0.0f32;
        for _ in 0..(sample_rate * 0.05) as usize {
            early = early.max(f.process(0.0, cutoff_hz, resonance, sample_rate).abs());
        }
        for _ in 0..(sample_rate * 0.30) as usize {
            f.process(0.0, cutoff_hz, resonance, sample_rate);
        }
        let mut late = 0.0f32;
        for _ in 0..(sample_rate * 0.05) as usize {
            late = late.max(f.process(0.0, cutoff_hz, resonance, sample_rate).abs());
        }
        late > early * 0.5 && late > 1e-4
    };

    let (mut lo, mut hi) = (0.0f32, 1.0f32);
    for _ in 0..40 {
        let mid = 0.5 * (lo + hi);
        if sustains(mid) { hi = mid } else { lo = mid }
    }
    hi
}

/// Measure the frequency a self-oscillating filter settles at, by counting zero crossings.
pub fn measure_oscillation_hz(config: DiodeConfig, cutoff_hz: f32, sample_rate: f32) -> f32 {
    let mut f = DiodeLadder::new(config);
    let resonance = 1.0;
    for _ in 0..(sample_rate * 1.0) as usize {
        f.process(0.0, cutoff_hz, resonance, sample_rate);
    }
    let window = (sample_rate * 0.5) as usize;
    let mut crossings = 0usize;
    let mut prev = f.process(0.0, cutoff_hz, resonance, sample_rate);
    for _ in 0..window {
        let y = f.process(0.0, cutoff_hz, resonance, sample_rate);
        if prev <= 0.0 && y > 0.0 {
            crossings += 1;
        }
        prev = y;
    }
    crossings as f32 / (window as f32 / sample_rate)
}

#[cfg(test)]
mod tests {
    use super::*;

    const RATES: [f32; 4] = [44_100.0, 48_000.0, 96_000.0, 192_000.0];

    #[test]
    fn the_copied_pole_set_still_reproduces_the_published_threshold() {
        // The copy is honest only if the 303 set still gives Stinchcombe's 18.4.
        let tb = DiodeConfig::Tb303.threshold();
        assert!((tb - 18.34).abs() < 0.05, "TB-303 threshold {tb}");
    }

    #[test]
    fn the_system_100_set_sings_lower_and_closer_to_its_cutoff_than_the_303s() {
        // Equal rungs spread the poles less, so the chosen set has a lower threshold and sings
        // nearer the nominal cutoff. Both are properties of the choice, pinned so a retune is
        // deliberate: the threshold about 10, the oscillation about +130 cents.
        let k = DiodeConfig::System100.threshold();
        assert!((k - 10.15).abs() < 0.2, "threshold {k}");
        let ratio = DiodeConfig::System100.oscillation_ratio();
        let cents = 1200.0 * ratio.log2();
        assert!(
            (cents - 128.0).abs() < 5.0,
            "sings {cents:.0} cents above the cutoff"
        );
        assert!(k < DiodeConfig::Tb303.threshold());
        assert!(ratio < DiodeConfig::Tb303.oscillation_ratio());
    }

    #[test]
    fn the_running_filter_agrees_with_the_closed_form_at_every_rate() {
        for fs in RATES {
            let analytic = DiodeConfig::System100.threshold();
            let measured = measure_oscillation_threshold(DiodeConfig::System100, 400.0, fs);
            let error = (measured - analytic).abs() / analytic;
            assert!(
                error < 0.15,
                "at {fs}: measured k {measured} against analytic {analytic}"
            );
        }
    }

    #[test]
    fn self_oscillation_lands_where_the_closed_form_says() {
        let nominal = 400.0;
        let hz = measure_oscillation_hz(DiodeConfig::System100, nominal, 48_000.0);
        let ratio = hz / nominal;
        let expected = DiodeConfig::System100.oscillation_ratio();
        assert!(
            (ratio - expected).abs() < 0.08,
            "sang at {hz} Hz, ratio {ratio}, expected {expected}"
        );
    }

    /// The discrete closed form is the analogue one where the warp is negligible, and it is what
    /// the running filter does everywhere else — measured at the cutoffs where the analogue
    /// form was 15 % and 80 % wrong.
    #[test]
    fn the_discrete_closed_form_matches_the_analogue_low_down_and_the_running_filter_high_up() {
        let config = DiodeConfig::System100;
        let (low, sung_low) = config.discrete_threshold(1e-4);
        assert!(
            (low - config.threshold()).abs() / config.threshold() < 1e-3,
            "{low}"
        );
        assert!(
            (sung_low - config.oscillation_ratio()).abs() < 1e-3,
            "{sung_low}"
        );

        let fs = 48_000.0;
        for cutoff in [2_000.0f32, 5_000.0, 8_000.0] {
            let (predicted, _) = config.discrete_threshold(cutoff / fs);
            let measured = measure_oscillation_threshold(config, cutoff, fs);
            assert!(
                (measured - predicted).abs() / predicted < 0.03,
                "at {cutoff} Hz: measured {measured} against the closed form {predicted}"
            );
        }
    }

    /// **The slider's singing point does not move with the cutoff.** The owner's finding: from
    /// the init patch's 10 kHz the resonance control reached its top without a note of
    /// self-oscillation, because the analogue threshold was being applied to a warped loop.
    #[test]
    fn the_resonance_sings_at_the_same_slider_position_at_every_cutoff() {
        let expected = 1.0 / RESONANCE_MARGIN;
        for fs in [44_100.0f32, 48_000.0, 96_000.0] {
            let exact = max_nominal_cutoff_hz(DiodeConfig::System100, fs);
            // Past `exact` the poles pin one by one; the singing point must hold there too.
            let pinned = 0.3 * fs;
            for cutoff in [100.0f32, 400.0, 2_000.0, 5_000.0, 8_000.0, exact, pinned] {
                let position = measure_oscillation_position(DiodeConfig::System100, cutoff, fs);
                assert!(
                    (position - expected).abs() < 0.015,
                    "at {cutoff} Hz, {fs} Hz: sings from {position}, expected {expected}"
                );
            }
        }
    }

    /// `K_MAX` is a stated bound, so it is measured against the largest `k` either
    /// configuration can be asked for: full resonance at the cutoff clamp.
    #[test]
    fn no_configuration_asks_for_more_than_k_max() {
        let mut largest = 0.0f32;
        for config in [DiodeConfig::System100, DiodeConfig::Tb303] {
            let ladder = DiodeLadder::new(config);
            let fs = 48_000.0;
            // The correction peaks partway up, where the widest poles are pinned and the rest
            // are still climbing, so the whole range is scanned rather than its end read.
            for step in 1..=450 {
                let cutoff = fs * step as f32 / 1000.0;
                let k = ladder.native_resonance(1.0, cutoff, fs);
                assert!(k <= K_MAX, "{config:?} at {cutoff} Hz asks for k = {k}");
                largest = largest.max(k);
            }
        }
        assert!(
            largest > 0.9 * K_MAX,
            "the largest k asked for is {largest}: K_MAX is slack"
        );
    }

    #[test]
    fn resonance_means_the_same_thing_on_both_configurations() {
        let sings_at = |config: DiodeConfig| measure_oscillation_position(config, 400.0, 48_000.0);
        let a = sings_at(DiodeConfig::System100);
        let b = sings_at(DiodeConfig::Tb303);
        assert!(
            (a - b).abs() < 0.12,
            "the two should sing at a similar slider position: {a} vs {b}"
        );
    }

    #[test]
    fn silence_in_gives_exactly_zero_out() {
        for fs in RATES {
            let mut f = DiodeLadder::default();
            f.process(1.0, 800.0, 0.5, fs);
            for _ in 0..(fs as usize) {
                f.process(0.0, 800.0, 0.5, fs);
            }
            assert_eq!(
                f.process(0.0, 800.0, 0.5, fs),
                0.0,
                "not exactly silent at {fs} Hz"
            );
        }
    }

    #[test]
    fn no_nan_or_inf_across_a_rate_cutoff_resonance_sweep() {
        for fs in RATES {
            for cutoff_step in 0..12 {
                let cutoff = 20.0 * 2.0f32.powi(cutoff_step);
                for r_step in 0..=10 {
                    let r = r_step as f32 / 10.0;
                    let mut f = DiodeLadder::default();
                    for n in 0..2000 {
                        let x = if n % 100 == 0 { INPUT_BOUND } else { 0.0 };
                        let y = f.process(x, cutoff, r, fs);
                        assert!(
                            y.is_finite(),
                            "non-finite at fs={fs} cutoff={cutoff} r={r}: {y}"
                        );
                    }
                }
            }
        }
    }

    #[test]
    fn output_stays_within_the_stated_bound_under_overdrive() {
        for fs in RATES {
            let mut f = DiodeLadder::default();
            let mut peak = 0.0f32;
            for n in 0..(fs as usize) {
                let t = n as f32 / fs;
                let cutoff = 200.0 + 1500.0 * (1.0 + (2.0 * PI * 3.0 * t).sin());
                let y = f.process(INPUT_BOUND * (2.0 * PI * 110.0 * t).sin(), cutoff, 1.0, fs);
                peak = peak.max(y.abs());
            }
            assert!(
                peak < OUTPUT_BOUND,
                "peak {peak} exceeded bound {OUTPUT_BOUND} at {fs} Hz"
            );
        }
    }

    #[test]
    fn reset_leaves_no_tail() {
        let mut f = DiodeLadder::default();
        for _ in 0..1000 {
            f.process(0.9, 500.0, 0.8, 48_000.0);
        }
        f.reset();
        assert_eq!(f.process(0.0, 500.0, 0.8, 48_000.0), 0.0);
    }

    /// Above the exact-ratio ceiling the filter keeps moving — the poles pin one by one rather
    /// than the nominal cutoff stopping — and once every pole is pinned it stops for good.
    #[test]
    fn above_the_exact_ratio_ceiling_the_poles_pin_one_by_one() {
        let fs = 44_100.0;
        let max = max_nominal_cutoff_hz(DiodeConfig::System100, fs);
        assert!((max * DiodeConfig::System100.widest_pole() - NYQUIST_FRACTION * fs).abs() < 1.0);
        let mut at_ceiling = DiodeLadder::default();
        let mut past = DiodeLadder::default();
        let mut pinned = DiodeLadder::default();
        let mut differs = false;
        for _ in 0..64 {
            let a = at_ceiling.process(0.5, max, 0.3, fs);
            let b = past.process(0.5, max * 3.0, 0.3, fs);
            let c = pinned.process(0.5, max * 100.0, 0.3, fs);
            differs |= a != b;
            // At three times the ceiling the nominal cutoff is at the limit: nothing above it
            // can move the filter further.
            assert_eq!(b, c);
        }
        assert!(
            differs,
            "past the ceiling the filter should still open further"
        );
    }
}
