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
//! | S&H | whatever the shared S&H output is | the plug-out's (manual p. 15): the one S&H OUT signal, not a random source inside the LFO |
//!
//! The **PWM triangle** is separate and unipolar, 0 → +6 V — 0 … 0.6 here — and the WAVE FORM
//! switch does not reach it (wart 5). The **rate** is 0.15–25 Hz on the slider (§3.1); the
//! plug-out's rate CV row and OFFSET arrive as a rate multiplier the matrix will supply.

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
    /// The shared S&H OUT signal, passed through.
    SampleHold,
}

#[derive(Debug, Clone)]
pub struct Lfo {
    /// The core's phase, `0..1`. The triangle rises for the first half and falls for the second.
    phase: f32,
    /// The square's level on the previous sample, for the ADSR trigger's rising edge.
    square_prev: bool,
    rose: bool,
}

impl Default for Lfo {
    fn default() -> Self {
        Self::new()
    }
}

impl Lfo {
    pub const fn new() -> Self {
        Self {
            phase: 0.0,
            // Phase zero is in the square's high half, so the first sample is not an edge.
            square_prev: true,
            rose: false,
        }
    }

    /// Zero the phase. `reset()` only — the LFO free-runs across notes and has no retrigger.
    pub fn reset(&mut self) {
        *self = Self::new();
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
    /// shape's own DC level. `sample_hold` is the shared S&H output for the S&H position.
    #[inline]
    pub fn process(
        &mut self,
        rate_hz: f32,
        shape: Shape,
        sample_hold: f32,
        sample_rate: f32,
    ) -> f32 {
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
            Shape::SampleHold => sample_hold,
        };

        self.phase += rate_hz.clamp(RATE_MIN_HZ, RATE_MAX_HZ) / sample_rate;
        if self.phase >= 1.0 {
            self.phase -= 1.0;
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
            let v = lfo.process(4.0, shape, 0.0, FS);
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
            lfo.process(3.0, Shape::Square, 0.0, FS);
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
                lfo.process(4.0, shape, 0.0, FS);
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
            lfo.process(1_000.0, Shape::Square, 0.0, FS);
            if lfo.square_rose() {
                edges += 1;
            }
        }
        assert!(
            (99..=100).contains(&edges),
            "clamped to 25 Hz over 4 s: {edges} edges"
        );
    }

    #[test]
    fn the_sample_hold_position_passes_the_shared_signal_through() {
        let mut lfo = Lfo::new();
        assert_eq!(lfo.process(4.0, Shape::SampleHold, 0.37, FS), 0.37);
    }
}
