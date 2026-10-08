//! An LFO whose three hardware shapes sit at three DC levels — wart 1 — plus the two the plug-out
//! invented and the unipolar triangle the PWM section always takes.
//!
//! `research:instruments/system-100.md` §7.1: IC301 integrates a triangle and a Schmitt comparator
//! makes the square; the sine is the triangle through back-to-back diodes, the sawtooth is the
//! triangle through switched inversion. **The three outputs do not share a DC level** — the board
//! annotations: sawtooth 0 → +10 V, square 0 / +10 V, sine −5 → +5 V — so switching a vibrato from
//! sine to sawtooth shifts the average pitch up by half the depth, and square adds a standing
//! offset it then toggles. This module keeps those levels, in units of ten volts:
//!
//! | Shape | Range | Standing |
//! |---|---|---|
//! | Saw | 0 … +1 | the hardware's, §7.1 |
//! | Square | 0 / +1 | the hardware's |
//! | Sine | −0.5 … +0.5 | the hardware's |
//! | Triangle | −0.5 … +0.5 | **chosen** — the plug-out invented the position and documents no level; bipolar like the sine it is shaped from |
//! | S&H | −0.5 … +0.5, a new level each cycle | **chosen** (the owner, 2026-10-08): each LFO samples noise at its own rate, so Rate means something and the two LFOs step apart. The plug-out passed the one S&H OUT signal through here, which only mirrored the S&H module; that module stays, patched as a source |
//!
//! The **PWM triangle** is separate and unipolar, 0 → +6 V — 0 … 0.6 here — and the WAVE FORM
//! switch does not reach it (wart 5). The **rate** is 0.15–25 Hz on the slider (§3.1); the
//! plug-out's rate CV row and OFFSET arrive as a rate multiplier the matrix will supply.

use crate::Rng;

/// The slider's range (§3.1).
pub const RATE_MIN_HZ: f32 = 0.15;
pub const RATE_MAX_HZ: f32 = 25.0;

/// The PWM triangle's peak, +6 V of the ten-volt unit (§5.4, §7.1).
pub const PWM_TRIANGLE_PEAK: f32 = 0.6;

/// How hard the diode shaper rounds the triangle into the "sine". **Chosen**: the hardware's
/// shaper is two diodes and three resistors (§7.1) and its curve was not measured; a soft
/// `tanh` of the triangle at this drive has the rounded-triangle look the manual draws.
const SINE_SHAPER_DRIVE: f32 = 1.8;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Shape {
    Sine,
    Triangle,
    #[default]
    Saw,
    Square,
    /// Noise sampled once a cycle and held: a new random level at the LFO's own rate.
    SampleHold,
}

#[derive(Debug, Clone)]
pub struct Lfo {
    /// The core's phase, `0..1`. The triangle rises for the first half and falls for the second.
    phase: f32,
    /// The square's level on the previous sample, for the ADSR trigger's rising edge.
    square_prev: bool,
    rose: bool,
    /// The S&H shape's noise, one generator per LFO so the two step apart, and its seed for
    /// `reset`.
    seed: u32,
    rng: Rng,
    /// The level the S&H shape holds until the next cycle; none until it is first asked for.
    held: Option<f32>,
}

impl Default for Lfo {
    fn default() -> Self {
        Self::new()
    }
}

impl Lfo {
    pub const fn new() -> Self {
        Self::seeded(0x5EED_0001)
    }

    /// An LFO whose S&H shape draws from `seed`: give each LFO its own.
    pub const fn seeded(seed: u32) -> Self {
        Self {
            phase: 0.0,
            // Phase zero is in the square's high half, so the first sample is not an edge.
            square_prev: true,
            rose: false,
            seed,
            rng: Rng::new(seed),
            held: None,
        }
    }

    /// Zero the phase and restart the noise. `reset()` only — the LFO free-runs across notes and
    /// has no retrigger.
    pub fn reset(&mut self) {
        *self = Self::seeded(self.seed);
    }

    /// Whether the square rose on the last `process` — what the ADSR's LFO trigger position
    /// fires on (§7.1, the diode AND with the keyboard gate).
    pub fn square_rose(&self) -> bool {
        self.rose
    }

    /// The core square's level now: the other input of the ADSR trigger's diode AND.
    pub fn square_high(&self) -> bool {
        self.square_prev
    }

    /// The core's raw triangle, `0..1` over the cycle: what the PWM section and the S&H's TRI mode
    /// take directly from the core (§5.4, §10.3).
    #[inline]
    fn core_triangle(&self) -> f32 {
        if self.phase < 0.5 {
            2.0 * self.phase
        } else {
            2.0 - 2.0 * self.phase
        }
    }

    /// The core's rising sawtooth, `0..1`, before the level shifter.
    #[inline]
    fn core_saw(&self) -> f32 {
        self.phase
    }

    /// The unipolar PWM triangle, `0..PWM_TRIANGLE_PEAK`, whatever the shape switch says.
    #[inline]
    pub fn pwm_triangle(&self) -> f32 {
        self.core_triangle() * PWM_TRIANGLE_PEAK
    }

    /// The core's shapes, regardless of the switch (§10.3): the routing's `LFO1_CORE_*` and
    /// `LFO2_CORE_TRIANGLE` sources, which SAMPLE MODE and the PWM switches used to choose between.
    /// The triangle is the PWM section's too; its +6 V peak is applied in the route's scale.
    #[inline]
    pub fn sh_source_saw(&self) -> f32 {
        self.core_saw()
    }

    #[inline]
    pub fn sh_source_saw_inverted(&self) -> f32 {
        1.0 - self.core_saw()
    }

    #[inline]
    pub fn sh_source_triangle(&self) -> f32 {
        self.core_triangle()
    }

    /// The sine, `-0.5..0.5`: the hardware's diode-rounded triangle.
    #[inline]
    pub fn sine(&self) -> f32 {
        let tri = 2.0 * self.core_triangle() - 1.0; // -1..1
        0.5 * crate::tanh_approx(SINE_SHAPER_DRIVE * tri) / crate::tanh_approx(SINE_SHAPER_DRIVE)
    }

    /// Advance the core by one sample at `rate_hz` and return the switch's output, at the
    /// shape's own DC level.
    #[inline]
    pub fn process(&mut self, rate_hz: f32, shape: Shape, sample_rate: f32) -> f32 {
        let out = match shape {
            Shape::Sine => self.sine(),
            Shape::Triangle => self.core_triangle() - 0.5,
            Shape::Saw => self.core_saw(),
            Shape::Square => {
                if self.phase < 0.5 {
                    1.0
                } else {
                    0.0
                }
            }
            // The first ask takes a level at once, so switching to S&H never waits a cycle.
            Shape::SampleHold => *self
                .held
                .get_or_insert_with(|| 0.5 * self.rng.next_bipolar()),
        };

        self.phase += rate_hz.clamp(RATE_MIN_HZ, RATE_MAX_HZ) / sample_rate;
        if self.phase >= 1.0 {
            self.phase -= 1.0;
            // A new level once a cycle, at the LFO's own rate, whatever the shape: the noise runs
            // on, so switching to S&H finds the latest level.
            self.held = Some(0.5 * self.rng.next_bipolar());
        }
        let square_now = self.phase < 0.5;
        self.rose = square_now && !self.square_prev;
        self.square_prev = square_now;

        out
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const FS: f32 = 48_000.0;

    fn mean_and_range(shape: Shape) -> (f32, f32, f32) {
        let mut lfo = Lfo::new();
        let n = FS as usize; // one second at 4 Hz: whole cycles
        let (mut sum, mut lo, mut hi) = (0.0f64, f32::INFINITY, f32::NEG_INFINITY);
        for _ in 0..n {
            let v = lfo.process(4.0, shape, FS);
            sum += v as f64;
            lo = lo.min(v);
            hi = hi.max(v);
        }
        ((sum / n as f64) as f32, lo, hi)
    }

    #[test]
    fn the_three_hardware_shapes_sit_at_three_dc_levels() {
        // Wart 1, as numbers: saw and square average +0.5, the sine averages 0.
        let (saw_mean, saw_lo, saw_hi) = mean_and_range(Shape::Saw);
        let (sq_mean, sq_lo, sq_hi) = mean_and_range(Shape::Square);
        let (sin_mean, sin_lo, sin_hi) = mean_and_range(Shape::Sine);
        assert!((saw_mean - 0.5).abs() < 0.01, "saw mean {saw_mean}");
        assert!(
            saw_lo >= 0.0 && saw_hi <= 1.0 && saw_hi > 0.99,
            "saw {saw_lo}..{saw_hi}"
        );
        assert!((sq_mean - 0.5).abs() < 0.01, "square mean {sq_mean}");
        assert_eq!((sq_lo, sq_hi), (0.0, 1.0));
        assert!(sin_mean.abs() < 0.01, "sine mean {sin_mean}");
        assert!(sin_lo < -0.49 && sin_hi > 0.49, "sine {sin_lo}..{sin_hi}");
    }

    #[test]
    fn the_rate_is_what_was_asked_for() {
        let mut lfo = Lfo::new();
        let mut edges = 0usize;
        for _ in 0..(FS * 10.0) as usize {
            lfo.process(3.0, Shape::Square, FS);
            if lfo.square_rose() {
                edges += 1;
            }
        }
        assert_eq!(edges, 30, "3 Hz over 10 s");
    }

    #[test]
    fn the_pwm_triangle_is_unipolar_and_ignores_the_switch() {
        for shape in [Shape::Sine, Shape::Saw, Shape::Square] {
            let mut lfo = Lfo::new();
            let (mut lo, mut hi) = (f32::INFINITY, f32::NEG_INFINITY);
            for _ in 0..(FS as usize) {
                lfo.process(4.0, shape, FS);
                let t = lfo.pwm_triangle();
                lo = lo.min(t);
                hi = hi.max(t);
            }
            assert!(
                (0.0..0.01).contains(&lo),
                "{shape:?}: pwm triangle low {lo}"
            );
            assert!(
                (hi - PWM_TRIANGLE_PEAK).abs() < 0.01,
                "{shape:?}: pwm triangle high {hi}"
            );
        }
    }

    #[test]
    fn the_rate_is_clamped_to_the_slider() {
        let mut lfo = Lfo::new();
        let mut edges = 0usize;
        for _ in 0..(FS * 4.0) as usize {
            lfo.process(1_000.0, Shape::Square, FS);
            if lfo.square_rose() {
                edges += 1;
            }
        }
        assert!(
            (99..=100).contains(&edges),
            "clamped to 25 Hz over 4 s: {edges} edges"
        );
    }

    /// **S&H takes a new level once a cycle, at the LFO's own rate** (the owner, 2026-10-08:
    /// "Make the lfo sample the noise at its own time"), held in between, within the sine's range.
    #[test]
    fn the_sample_hold_position_steps_once_a_cycle_at_its_own_rate() {
        let mut lfo = Lfo::new();
        let mut levels = Vec::new();
        for _ in 0..FS as usize {
            let level = lfo.process(4.0, Shape::SampleHold, FS);
            assert!(
                (-0.5..=0.5).contains(&level),
                "{level} is outside the sine's range"
            );
            if levels.last() != Some(&level) {
                levels.push(level);
            }
        }
        assert!(
            (4..=5).contains(&levels.len()),
            "a second at 4 Hz is four or five levels, not {}: {levels:?}",
            levels.len()
        );
    }

    /// Each LFO has its own noise, so two on S&H step apart.
    #[test]
    fn two_lfos_on_sample_and_hold_step_apart() {
        let (mut a, mut b) = (Lfo::seeded(1), Lfo::seeded(2));
        let run = |lfo: &mut Lfo| -> Vec<f32> {
            (0..FS as usize)
                .map(|_| lfo.process(4.0, Shape::SampleHold, FS))
                .collect()
        };
        assert_ne!(run(&mut a), run(&mut b));
    }
}
