//! The 102's sample-and-hold: a clock, a sampler, and a lag that is the portamento's trick again.
//!
//! `research:instruments/system-100.md` §10.3: the VCO-5 board reuses the keyboard's sample-and-hold
//! as the sampler — a JFET gated by the clock onto a hold capacitor, read by a follower — and the
//! **OUTPUT LAG** pot sits in series with the switch exactly as the portamento pot does, which is
//! why the manual can say it "will affect this control voltage in exactly the same way as the
//! PORTAMENTO control". The **clock** runs 0.6–125 Hz from SAMPLE TIME and puts pulses on the
//! CLOCK OUTPUT jack; on the hardware it also fires ADSR-2 whenever the mode is not OFF (wart 16),
//! which here is the matrix's business: the clock is a column, and a gate row can take it.
//!
//! **What is sampled is whatever is routed to the S&H input** — the plug-out's SAMPLE MODE switch
//! (OFF, SAW1, SAW2, TRI, SIN (EXT), manual p. 14) is gone as a switch and is the routing now. Its
//! positions are sources: LFO-1's **core** sawtooth, its inverse, its triangle and its sine, taken
//! ahead of the shape switch (§10.3), which is why `lfo.rs` exposes the core's shapes separately.
//! SIN (EXT)'s *the external input when patched, else the sine* is the patched sources, or the
//! core sine alone. **OFF is nothing contributing**: no route, or only routes at zero depth.
//!
//! The lag is an RC that only charges while the sampler is not switching: between clocks the held
//! value slews toward the last sample with the lag's time constant. The lag's time constant is
//! the slider's, "0–2 s" per the specification, and is not smoothed.

use crate::flush;

pub const CLOCK_MIN_HZ: f32 = 0.6;
pub const CLOCK_MAX_HZ: f32 = 125.0;
pub const LAG_MAX_S: f32 = 2.0;

#[derive(Debug, Clone, Default)]
pub struct SampleHold {
    clock_phase: f32,
    /// The value taken on the last clock.
    held: f32,
    /// The lagged output, the hold capacitor's voltage.
    out: f32,
    /// Whether the clock fired on the last `process`.
    fired: bool,
    /// The fraction of the last interval at which it fired, for edge-exact consumers.
    fired_frac: f32,
}

impl SampleHold {
    pub const fn new() -> Self {
        Self {
            clock_phase: 0.0,
            held: 0.0,
            out: 0.0,
            fired: false,
            fired_frac: 0.0,
        }
    }

    pub fn reset(&mut self) {
        *self = Self::new();
    }

    /// The clock's rising edge on the last sample, as the sync consumers want it.
    pub fn clock_edge(&self) -> Option<f32> {
        if self.fired {
            Some(self.fired_frac)
        } else {
            None
        }
    }

    /// The CLOCK OUT signal: high for the first half of the clock period.
    pub fn clock_out(&self) -> f32 {
        if self.clock_phase < 0.5 { 1.0 } else { 0.0 }
    }

    /// The held value before the lag, for tests.
    pub fn held(&self) -> f32 {
        self.held
    }

    /// Advance one sample: run the clock, sample on its edge, slew the output.
    ///
    /// Returns the S&H OUT signal. `input` is the summed S&H input, or `None` when nothing
    /// contributes to it — the retired OFF position. Then nothing is sampled and the held value
    /// stays where it was, so a patch that switches the sampler off holds its last step. **The
    /// clock runs either way**, as it did in OFF.
    #[inline]
    pub fn process(
        &mut self,
        rate_hz: f32,
        lag_s: f32,
        input: Option<f32>,
        sample_rate: f32,
    ) -> f32 {
        let inc = rate_hz.clamp(CLOCK_MIN_HZ, CLOCK_MAX_HZ) / sample_rate;
        let before = self.clock_phase;
        self.clock_phase += inc;
        self.fired = false;
        if self.clock_phase >= 1.0 {
            self.clock_phase -= 1.0;
            self.fired = true;
            self.fired_frac = ((1.0 - before) / inc).clamp(0.0, 1.0);
            if let Some(v) = input {
                self.held = v;
            }
        }

        // The lag: the same RC as the portamento, charging toward the held value.
        let lag = lag_s.clamp(0.0, LAG_MAX_S);
        if lag <= 0.0 {
            self.out = self.held;
        } else {
            let coef = (-1.0 / (lag * sample_rate)).exp();
            self.out = flush(self.held + (self.out - self.held) * coef);
        }
        self.out
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const FS: f32 = 48_000.0;

    /// A rising ramp standing in for whatever is routed to the input.
    fn ramp(i: usize) -> Option<f32> {
        Some((i % 1000) as f32 / 1000.0)
    }

    #[test]
    fn the_clock_fires_at_its_rate_and_each_step_holds_the_source() {
        let mut sh = SampleHold::new();
        let mut fires = 0;
        let mut last_held = f32::NAN;
        for i in 0..(FS as usize) {
            sh.process(10.0, 0.0, ramp(i), FS);
            if sh.clock_edge().is_some() {
                fires += 1;
                // With no lag the output is exactly the sample just taken, and it is the source's
                // value at the clock — a step, not a smear.
                assert_eq!(sh.process(10.0, 0.0, ramp(i + 1), FS), sh.held());
                assert_ne!(sh.held(), last_held);
                last_held = sh.held();
            }
        }
        assert!(
            (9..=11).contains(&fires),
            "{fires} clocks in a second at 10 Hz"
        );
    }

    #[test]
    fn the_lag_slews_between_steps_like_the_portamento() {
        let mut sh = SampleHold::new();
        // A slow clock and a lag shorter than its period: the output arrives before the next step.
        let mut max_jump = 0.0f32;
        let mut prev = 0.0f32;
        for i in 0..(FS as usize) {
            let y = sh.process(2.0, 0.05, ramp(i * 37), FS);
            max_jump = max_jump.max((y - prev).abs());
            prev = y;
        }
        assert!(max_jump < 0.01, "the lag let a step through: {max_jump}");
    }

    #[test]
    fn off_samples_nothing_and_holds_the_last_step() {
        let mut sh = SampleHold::new();
        for i in 0..(FS as usize / 4) {
            sh.process(20.0, 0.0, ramp(i), FS);
        }
        let held = sh.held();
        assert_ne!(held, 0.0);
        for _ in 0..(FS as usize / 4) {
            assert_eq!(sh.process(20.0, 0.0, None, FS), held);
        }
    }

    #[test]
    fn the_sampler_takes_whatever_is_routed_and_the_clock_runs_with_nothing() {
        // SIN (EXT)'s override is the routing's business now: the sampler takes its one input.
        let mut sh = SampleHold::new();
        let mut clocks = 0;
        for _ in 0..(FS as usize / 2) {
            let y = sh.process(30.0, 0.0, Some(0.42), FS);
            if sh.clock_edge().is_some() {
                assert_eq!(y, 0.42, "the routed input is what is sampled");
                clocks += 1;
            }
        }
        assert!(clocks > 0);
        // With nothing contributing the clock keeps running, as it did in OFF.
        let mut ran = 0;
        for _ in 0..(FS as usize / 2) {
            sh.process(30.0, 0.0, None, FS);
            if sh.clock_edge().is_some() {
                ran += 1;
            }
        }
        assert_eq!(ran, clocks, "the clock stopped when the input went away");
        assert_eq!(sh.held(), 0.42, "an unfed sampler holds its last step");
    }

    #[test]
    fn the_clock_out_is_a_square_and_reset_leaves_nothing() {
        let mut sh = SampleHold::new();
        let (mut hi, mut lo) = (0usize, 0usize);
        for i in 0..(FS as usize) {
            sh.process(5.0, 0.0, ramp(i), FS);
            if sh.clock_out() > 0.5 {
                hi += 1
            } else {
                lo += 1
            }
        }
        assert!((hi as f32 / lo as f32 - 1.0).abs() < 0.05, "duty {hi}:{lo}");
        sh.reset();
        assert_eq!(sh.held(), 0.0);
        assert_eq!(sh.clock_out(), 1.0, "phase zero is the high half");
    }
}
