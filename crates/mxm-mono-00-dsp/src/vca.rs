//! The CA3080 amplifier, whose tremolo can only dip.
//!
//! `research:instruments/system-100.md` §8.3: the OTA's control current comes from a summing node that
//! takes INITIAL GAIN, the ADSR through its slider, and **the LFO through its slider and a
//! germanium diode**. The diode is why the manual can say "the VCA LFO control has no effect when
//! the VCA INITIAL GAIN and VCA ADSR controls are at 0": the LFO cannot open a closed amplifier,
//! it can only take away from a gain the other two set. Wart 3, and modelling the LFO as bipolar
//! around the gain would be "wrong twice".
//!
//! **The diode's orientation is a reading, not a measurement** (§13): the *behaviour* is the
//! manual's; that the diode passes the LFO's positive excursions and blocks its negative ones is
//! the model here. With the LFO's shapes at their three DC levels (`lfo.rs`) that gives three
//! different tremolos — a sawtooth dips on every cycle, a square dips for half of it, a sine dips
//! on its positive half only — which is the machine.
//!
//! The plug-out's **TONE** control ("boosts the high or low-frequency range", manual p. 20) is a
//! plug-out invention with no hardware to defer to; it is modelled as a tilt about 1 kHz, chosen,
//! and lives here because that is where the plug-out puts it.

use crate::flush;

/// The tilt's pivot. **Chosen** — the plug-out documents no figure.
pub const TONE_PIVOT_HZ: f32 = 1_000.0;
/// The tilt's reach at either end of the control, in dB. **Chosen**.
pub const TONE_RANGE_DB: f32 = 6.0;

#[derive(Debug, Clone, Copy, Default)]
pub struct Vca {
    /// The tilt's one-pole state.
    tilt_s: f32,
    tilt_g: f32,
    tilt_rate: f32,
}

impl Vca {
    pub const fn new() -> Self {
        Self {
            tilt_s: 0.0,
            tilt_g: 0.0,
            tilt_rate: 0.0,
        }
    }

    pub fn reset(&mut self) {
        self.tilt_s = 0.0;
    }

    /// The control current, `0..=1`, from the three summing inputs.
    ///
    /// `standing` is INITIAL GAIN, `env` the ADSR's level already scaled by its slider, `lfo` the
    /// LFO's output at its own DC level scaled by its slider. The diode passes only the positive
    /// part of the LFO, and it subtracts.
    #[inline]
    pub fn gain(standing: f32, env: f32, lfo: f32) -> f32 {
        let dip = lfo.max(0.0);
        (standing.clamp(0.0, 1.0) + env.clamp(0.0, 1.0) - dip).clamp(0.0, 1.0)
    }

    /// One sample: the input through the tilt, then the gain.
    ///
    /// `tone` is `-1..=1`, negative for a low boost and positive for a high boost; zero is flat.
    #[inline]
    pub fn process(&mut self, x: f32, gain: f32, tone: f32, sample_rate: f32) -> f32 {
        if sample_rate != self.tilt_rate {
            self.tilt_rate = sample_rate;
            // The pivot is clamped below Nyquist like every other corner in the crate: at the
            // validator's 1.2 kHz sample rate an unclamped 1 kHz pivot put the prewarp past the
            // approximation's range and the tilt returned NaN on its twenty-third sample.
            let pivot = TONE_PIVOT_HZ.min(crate::hpf::NYQUIST_FRACTION * sample_rate);
            let g = crate::hpf::tan_approx(
                std::f64::consts::PI * f64::from(pivot) / f64::from(sample_rate),
            ) as f32;
            self.tilt_g = g / (1.0 + g);
        }
        // Split at the pivot and weight the halves: a tilt is a low shelf and a high shelf that
        // mirror each other.
        let v = self.tilt_g * (x - self.tilt_s);
        let low = v + self.tilt_s;
        self.tilt_s = flush(low + v);
        let high = x - low;
        let t = tone.clamp(-1.0, 1.0);
        let boost = 10f32.powf(TONE_RANGE_DB * t.abs() / 20.0);
        let tilted = if t >= 0.0 {
            low + high * (1.0 + (boost - 1.0) * t)
        } else {
            low * (1.0 + (boost - 1.0) * -t) + high
        };
        flush(tilted * gain.clamp(0.0, 1.0))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_lfo_cannot_open_a_closed_amplifier() {
        // "has no effect when INITIAL GAIN and ADSR are at 0" — at any LFO level or sign.
        for lfo in [-1.0f32, -0.5, 0.0, 0.5, 1.0] {
            assert_eq!(Vca::gain(0.0, 0.0, lfo), 0.0);
        }
    }

    #[test]
    fn the_lfo_only_takes_away_and_only_on_its_positive_side() {
        let standing = 0.6;
        assert_eq!(Vca::gain(standing, 0.0, 0.0), standing);
        assert!(
            Vca::gain(standing, 0.0, 0.3) < standing,
            "a positive LFO dips"
        );
        assert_eq!(
            Vca::gain(standing, 0.0, -0.3),
            standing,
            "a negative LFO is blocked"
        );
        assert_eq!(
            Vca::gain(standing, 0.0, 5.0),
            0.0,
            "and it cannot go below closed"
        );
    }

    #[test]
    fn standing_gain_and_envelope_sum_and_clamp() {
        assert_eq!(Vca::gain(0.5, 0.3, 0.0), 0.8);
        assert_eq!(Vca::gain(0.9, 0.9, 0.0), 1.0);
    }

    #[test]
    fn a_flat_tone_is_transparent_and_the_ends_tilt() {
        let fs = 48_000.0;
        // RMS, not peak: at 8 kHz there are six samples per cycle and the peak is never hit.
        let level = |tone: f32, hz: f32| {
            let mut vca = Vca::new();
            let n = (fs * 0.2) as usize;
            let mut sum = 0.0f64;
            for i in 0..n * 2 {
                let x = (std::f32::consts::TAU * hz * i as f32 / fs).sin();
                let y = vca.process(x, 1.0, tone, fs);
                if i >= n {
                    sum += (y as f64) * (y as f64);
                }
            }
            let rms = (sum / n as f64).sqrt() as f32;
            20.0 * (rms * std::f32::consts::SQRT_2).log10()
        };
        assert!(level(0.0, 100.0).abs() < 0.1 && level(0.0, 8_000.0).abs() < 0.1);
        assert!(
            level(1.0, 8_000.0) > 4.0,
            "high boost: {}",
            level(1.0, 8_000.0)
        );
        assert!(
            level(-1.0, 100.0) > 4.0,
            "low boost: {}",
            level(-1.0, 100.0)
        );
        assert!(level(1.0, 100.0).abs() < 1.0, "the other end stays put");
    }

    #[test]
    fn silence_stays_exactly_zero() {
        let mut vca = Vca::new();
        vca.process(1.0, 1.0, 0.5, 48_000.0);
        for _ in 0..100_000 {
            vca.process(0.0, 1.0, 0.5, 48_000.0);
        }
        assert_eq!(vca.process(0.0, 1.0, 0.5, 48_000.0), 0.0);
    }
}
