//! One VCO of the System-100's family: a sawtooth core, a pulse by comparator, and a triangle by
//! switched inversion.
//!
//! `research:instruments/system-100.md` §5: the VCO-4 board is an exponential converter charging a
//! polystyrene capacitor that a JFET resets — a **sawtooth core**. The square and the pulse are
//! one comparator against the saw, so the square *is* the pulse at 50 % (§5.3). The triangle is
//! the saw through a switched-inversion stage, so one half of each ramp is flipped and **the seam
//! is where the reset was** — the machine's "reedy in the lower range" triangle, not a clean one.
//!
//! Band-limiting is PolyBLEP on the saw's reset and on the pulse's two edges, copied from
//! `mxm-mono-01-dsp`. The triangle is continuous and needs none; see [`triangle`] for why it is
//! taken from the phase rather than by folding the band-limited saw, and what that leaves out.
//!
//! **Phasors free-run.** They are never reset on note-on — a VCO does not do that either.

use crate::flush;

/// Lowest frequency the oscillator will produce. The hardware reaches 3 Hz at the bottom of the
/// keyboard with the knob fully down (§5.1); below this the PolyBLEP correction is meaningless.
pub const FREQ_MIN_HZ: f32 = 4.0;

/// Highest frequency, as a fraction of the sample rate. The 2' range with bend and a full-depth
/// sawtooth LFO on top can otherwise exceed Nyquist, and PolyBLEP does not make an invalid phase
/// increment safe.
pub const NYQUIST_FRACTION: f32 = 0.45;

/// PolyBLEP residual for a step discontinuity, `t` the phase in `0..1`, `dt` the increment.
#[inline]
fn poly_blep(t: f32, dt: f32) -> f32 {
    if t < dt {
        let x = t / dt;
        x + x - x * x - 1.0
    } else if t > 1.0 - dt {
        let x = (t - 1.0) / dt;
        x * x + x + x + 1.0
    } else {
        0.0
    }
}

/// A free-running phase accumulator.
#[derive(Debug, Clone, Copy, Default)]
pub struct Phasor {
    phase: f32,
    inc: f32,
}

impl Phasor {
    pub const fn new() -> Self {
        Self {
            phase: 0.0,
            inc: 0.0,
        }
    }

    #[inline]
    pub fn set_freq(&mut self, hz: f32, sample_rate: f32) {
        let hz = hz.clamp(FREQ_MIN_HZ, NYQUIST_FRACTION * sample_rate);
        self.inc = hz / sample_rate;
    }

    #[inline]
    pub fn set_inc(&mut self, inc: f32) {
        self.inc = inc.clamp(0.0, NYQUIST_FRACTION);
    }

    #[inline]
    pub fn phase(&self) -> f32 {
        self.phase
    }

    #[inline]
    pub fn inc(&self) -> f32 {
        self.inc
    }

    /// Advance, and say whether the phase wrapped — the reset the sync output is built from.
    #[inline]
    pub fn advance(&mut self) -> bool {
        self.phase += self.inc;
        if self.phase >= 1.0 {
            self.phase -= 1.0;
            true
        } else {
            false
        }
    }

    /// Force the reset: what a STRONG sync edge does to the integrator (§5.5). The next module
    /// uses it; it exists here so the phasor owns its own phase.
    #[inline]
    pub fn hard_reset(&mut self) {
        self.phase = 0.0;
    }

    /// Zero the phase. Called from `reset()`, never from note-on.
    pub fn reset(&mut self) {
        self.phase = 0.0;
    }
}

/// Band-limited rising sawtooth, the reset corrected. Rising, per the board's annotation (§5.2);
/// the manual draws it falling and §13 records the conflict.
#[inline]
pub fn saw(p: &Phasor) -> f32 {
    2.0 * p.phase() - 1.0 - poly_blep(p.phase(), p.inc())
}

/// Band-limited pulse of the given width, both edges corrected.
#[inline]
pub fn pulse(p: &Phasor, width: f32) -> f32 {
    let (t, dt) = (p.phase(), p.inc());
    let w = clamp_pulse_width(width, dt);
    let mut y = if t < w { 1.0 } else { -1.0 };
    y += poly_blep(t, dt);
    let second = {
        let x = t - w;
        if x < 0.0 { x + 1.0 } else { x }
    };
    y -= poly_blep(second, dt);
    y
}

/// The machine's triangle: the saw through switched inversion, taken from the phase.
///
/// IC206 inverts one half of each ramp (§5.3), which on an ideal saw gives a triangle that is
/// continuous in value and discontinuous only in slope, so it needs no PolyBLEP: its harmonics fall
/// at `1/n²` and the naive form aliases quietly. **It is built from the raw phase, not by folding
/// the band-limited saw**: the PolyBLEP residual takes the saw *through zero* at every reset, and
/// folding that produces a full-scale spike per period, which no System-100 does. The hardware's
/// seam — the reset's finite time (§5.2) — is a small glitch the research does not size, and it is
/// **not modelled** rather than invented.
#[inline]
pub fn triangle(p: &Phasor) -> f32 {
    4.0 * (p.phase() - 0.5).abs() - 1.0
}

/// Keep the two PolyBLEP corrections from overlapping.
///
/// The hardware's 5 % SET trim pins the narrow end at 5 % (§5.3); the wide end is the square.
#[inline]
pub fn clamp_pulse_width(width: f32, dt: f32) -> f32 {
    let lo = 0.05f32.max(2.0 * dt);
    let hi = 0.95f32.min(1.0 - 2.0 * dt);
    if lo > hi { 0.5 } else { width.clamp(lo, hi) }
}

/// The plug-out's three-position WAVEFORM switch (manual p. 16). Square and pulse-at-50 % are the
/// same thing on this machine, so there is no separate pulse position.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Wave {
    #[default]
    Saw,
    Square,
    Triangle,
}

/// The naive waveform at a phase, no band-limiting — what the sync corrections measure their
/// step against.
#[inline]
fn naive(p: f32, wave: Wave, width: f32) -> f32 {
    match wave {
        Wave::Saw => 2.0 * p - 1.0,
        Wave::Square => {
            if p < width {
                1.0
            } else {
                -1.0
            }
        }
        Wave::Triangle => 4.0 * (p - 0.5).abs() - 1.0,
    }
}

/// The waveform's own PolyBLEP corrections, split so a sync reset can suppress the one that
/// would double-count a wrap that did not happen.
#[inline]
fn blep_before(t: f32, dt: f32) -> f32 {
    if t > 1.0 - dt {
        let x = (t - 1.0) / dt;
        x * x + x + x + 1.0
    } else {
        0.0
    }
}

#[inline]
fn blep_after(t: f32, dt: f32) -> f32 {
    if t < dt {
        let x = t / dt;
        x + x - x * x - 1.0
    } else {
        0.0
    }
}

/// The band-limited waveform at a phase, with the option of leaving out the correction for a
/// wrap "just before" this sample — used when what happened there was a sync reset instead, whose
/// own correction the caller applies.
#[inline]
fn band_limited(p: f32, wave: Wave, width: f32, dt: f32, suppress_after: bool) -> f32 {
    match wave {
        Wave::Saw => {
            let after = if suppress_after {
                0.0
            } else {
                blep_after(p, dt)
            };
            2.0 * p - 1.0 - after - blep_before(p, dt)
        }
        Wave::Square => {
            let w = clamp_pulse_width(width, dt);
            let mut y = if p < w { 1.0 } else { -1.0 };
            if !suppress_after {
                y += blep_after(p, dt);
            }
            y += blep_before(p, dt);
            let second = {
                let x = p - w;
                if x < 0.0 { x + 1.0 } else { x }
            };
            y -= blep_after(second, dt) + blep_before(second, dt);
            y
        }
        Wave::Triangle => naive(p, wave, width),
    }
}

/// The plug-out's STRONG / WEAK switch (manual p. 16), the 102's two jacks (§5.5).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum SyncStrength {
    /// The slave's core resets on every rising edge of the master: hard sync.
    #[default]
    Strong,
    /// A small kick into the core on every edge: the slave locks only where the kick can make up
    /// its shortfall, near the ratios the manual names, and "rolls" between them.
    Weak,
}

/// How far a WEAK kick advances the slave's phase, in cycles. **Chosen**: the hardware injects a
/// small charge through 15 kΩ (§5.5) and its size was not measured. Each master edge advances the
/// slave by this much, wrapping early if that carries it past its top; a slave running slightly
/// under a rational ratio `p/q` is pushed onto it, one running over drifts on — one-sided lock
/// ranges **below** 1/1, 3/2, 4/3 and the rest, about `kick / q` wide (one early wrap per orbit of
/// `q` edges can absorb at most one kick). The manual's "certain perfect and major intervals" and
/// "rolls" between them, with the ranges the choice implies; the test pins both.
pub const WEAK_SYNC_KICK: f32 = 0.04;

/// One VCO. Any VCO can be a master; VCO-2 is also a slave, through [`Vco::process_slave`].
#[derive(Debug, Clone, Default)]
pub struct Vco {
    phasor: Phasor,
    /// Whether the core wrapped on the last `advance`, and where in that interval.
    wrapped: bool,
    wrap_frac: f32,
    /// Slave state: the sample computed last call and not yet emitted, and whether one exists.
    pending: f32,
    has_pending: bool,
}

impl Vco {
    pub const fn new() -> Self {
        Self {
            phasor: Phasor::new(),
            wrapped: false,
            wrap_frac: 0.0,
            pending: 0.0,
            has_pending: false,
        }
    }

    pub fn reset(&mut self) {
        self.phasor.reset();
        self.wrapped = false;
        self.wrap_frac = 0.0;
        self.pending = 0.0;
        self.has_pending = false;
    }

    /// The phase, for tests and the sync module.
    pub fn phase(&self) -> f32 {
        self.phasor.phase()
    }

    /// Whether the core reset on the last sample — the rising edge of the SYNC OUT square.
    pub fn just_reset(&self) -> bool {
        self.wrapped
    }

    /// The rising edge of the SYNC OUT square, as a slave needs it: `Some(f)` if the core wrapped
    /// during the last sample interval, `f` the fraction of that interval at which it did.
    #[inline]
    pub fn sync_edge(&self) -> Option<f32> {
        if self.wrapped {
            Some(self.wrap_frac)
        } else {
            None
        }
    }

    /// The VCO's square at its own frequency, which is what the SYNC OUT jack carries (§5.3):
    /// a square, not a pulse, whatever the WAVEFORM switch shows.
    #[inline]
    pub fn sync_out(&self) -> f32 {
        if self.phasor.phase() < 0.5 { 1.0 } else { -1.0 }
    }

    /// Render one sample of the selected waveform and advance.
    ///
    /// `width` is the pulse width in `0..1` and only matters for `Square`, which is the pulse
    /// with the PWM section allowed to move its reference (§5.3).
    #[inline]
    pub fn process(&mut self, freq_hz: f32, wave: Wave, width: f32, sample_rate: f32) -> f32 {
        self.phasor.set_freq(freq_hz, sample_rate);
        let y = match wave {
            Wave::Saw => saw(&self.phasor),
            Wave::Square => pulse(&self.phasor, width),
            Wave::Triangle => triangle(&self.phasor),
        };
        let before = self.phasor.phase();
        self.wrapped = self.phasor.advance();
        if self.wrapped {
            // The wrap sat this far into the interval that just elapsed.
            self.wrap_frac = ((1.0 - before) / self.phasor.inc()).clamp(0.0, 1.0);
        }
        flush(y)
    }

    /// Render one sample as a **slave**, synchronised to a master's edge.
    ///
    /// A sync reset is a discontinuity at an arbitrary point in a sample interval, and PolyBLEP
    /// corrects the sample on each side of it. The sample before the edge can only be corrected
    /// once the edge is known, so this VCO computes each sample one call early and holds it: on
    /// every call it emits the sample it computed last time, corrected for any edge the master
    /// reports now. **The emitted sample is for the same instant as the master's** — the master
    /// reports the edge it crossed while advancing *from* that instant — so there is no latency
    /// between the two; the only visible difference from [`Vco::process`] is that the first call
    /// emits zero. The hold is the same whether sync is on or off, so throwing the switch does
    /// not shift the waveform.
    ///
    /// `edge` is the master's [`Vco::sync_edge`]. STRONG resets the core at the edge; WEAK advances
    /// it by [`WEAK_SYNC_KICK`]. The step either makes is measured against the naive waveform and
    /// corrected on both sides, and the slave's own "a wrap just happened" correction is suppressed
    /// for that interval, since what happened there was the reset.
    ///
    /// `depth` is the **sync route's own amount**, `0..=1` — how far the reset is carried out.
    /// `plan-modulation-routing.md` §3: scaling a sync source does nothing, because the detector's
    /// interpolated fraction is scale-invariant, so the amount has to mean something else and the
    /// musical thing it can mean is how hard the reset pulls. At `1.0` both arms are the hardware's
    /// exactly — a strong reset lands on zero and a weak kick is the full `WEAK_SYNC_KICK` — and
    /// below it the core is pulled part of the way instead, which is soft sync and is continuous.
    /// This is a **DSP addition**, and it is provably identical to the machine at full depth.
    #[inline]
    // One sync route's whole context, and the eighth argument is the one the conversion added.
    #[allow(clippy::too_many_arguments)]
    pub fn process_slave(
        &mut self,
        freq_hz: f32,
        wave: Wave,
        width: f32,
        sample_rate: f32,
        edge: Option<f32>,
        strength: SyncStrength,
        depth: f32,
    ) -> f32 {
        self.phasor.set_freq(freq_hz, sample_rate);
        let inc = self.phasor.inc();
        let p_prev = self.phasor.phase();

        let (p_now, after_correction, reset_happened, core_went_to_zero) = match edge {
            Some(f) => {
                // The slave's phase at the instant of the edge, its own wraps folded.
                let p_at_edge = (p_prev + f * inc).fract();
                let d = depth.clamp(0.0, 1.0);
                let p_kicked = match strength {
                    // Pulled toward zero by the route's depth. At full this is `0.0` exactly —
                    // `x - x * 1.0` is `x - x` — so the hardware's reset is bit-identical.
                    SyncStrength::Strong => p_at_edge - p_at_edge * d,
                    // A charge injected into the integrator: if it carries the capacitor past
                    // the comparator's threshold the JFET discharges it to zero and the excess
                    // is lost — which is the only thing that lets a kick *lock* rather than
                    // merely detune. A kick that wrapped to `p + kick - 1` would add the same
                    // phase at every edge whatever the phase, and no ratio could hold.
                    // **Depth is how much charge arrives**, so at full it is the machine's.
                    SyncStrength::Weak => {
                        let kicked = p_at_edge + WEAK_SYNC_KICK * d;
                        if kicked >= 1.0 { 0.0 } else { kicked }
                    }
                };
                let h = naive(p_kicked, wave, width) - naive(p_at_edge, wave, width);
                // Before the step, at distance `f` from the previous sample: (h/2)(1 - f)².
                // After it, at distance `1 - f` from this sample: -(h/2) f².
                if self.has_pending {
                    self.pending += 0.5 * h * (1.0 - f) * (1.0 - f);
                }
                let p_now = (p_kicked + (1.0 - f) * inc).fract();
                (p_now, -0.5 * h * f * f, true, p_kicked == 0.0)
            }
            None => ((p_prev + inc).fract(), 0.0, false, false),
        };

        // An own wrap in this interval is real only if no reset pre-empted it.
        let own_wrap = !reset_happened && p_now < p_prev;
        let value_now = band_limited(p_now, wave, width, inc, reset_happened) + after_correction;

        self.phasor.phase = p_now;
        // The core reset — by its own wrap, a strong edge, or a weak kick that discharged it.
        // That is the SYNC OUT edge and the cycle count, whichever way it happened.
        self.wrapped = own_wrap || core_went_to_zero;
        if own_wrap {
            self.wrap_frac = ((1.0 - p_prev) / inc).clamp(0.0, 1.0);
        } else if self.wrapped {
            self.wrap_frac = edge.unwrap_or(0.0);
        }

        let out = if self.has_pending { self.pending } else { 0.0 };
        self.pending = value_now;
        self.has_pending = true;
        flush(out)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const RATES: [f32; 4] = [44_100.0, 48_000.0, 96_000.0, 192_000.0];

    /// Frequency from the time between the first and last rising zero crossing, interpolated —
    /// counting whole crossings over a fixed window quantises to ±1 cycle, which at 55 Hz over
    /// two seconds is ±9 cents, too coarse to tell a tuning error from rounding.
    fn measure_freq(wave: Wave, freq: f32, fs: f32, secs: f32) -> f32 {
        let mut vco = Vco::new();
        let n = (fs * secs) as usize;
        let (mut first, mut last, mut count) = (None::<f64>, 0.0f64, 0usize);
        let mut prev = vco.process(freq, wave, 0.5, fs);
        for i in 1..n {
            let y = vco.process(freq, wave, 0.5, fs);
            if prev <= 0.0 && y > 0.0 {
                let frac = (-prev / (y - prev)) as f64;
                let t = (i as f64 - 1.0 + frac) / fs as f64;
                if first.is_none() {
                    first = Some(t);
                } else {
                    last = t;
                    count += 1;
                }
            }
            prev = y;
        }
        let first = first.expect("no zero crossings");
        (count as f64 / (last - first)) as f32
    }

    #[test]
    fn every_waveform_is_at_the_frequency_asked_for() {
        for fs in RATES {
            for wave in [Wave::Saw, Wave::Square, Wave::Triangle] {
                for freq in [55.0f32, 110.0, 440.0, 1_000.0, 4_000.0] {
                    let measured = measure_freq(wave, freq, fs, 2.0);
                    let cents = 1200.0 * (measured / freq).log2();
                    assert!(
                        cents.abs() < 1.0,
                        "{wave:?} {freq} Hz at {fs}: measured {measured}, off by {cents:.3} cents"
                    );
                }
            }
        }
    }

    #[test]
    fn the_square_is_the_pulse_at_fifty_percent() {
        // The manual: "exactly the same as the square wave". Here by construction, and pinned.
        let mut p = Phasor::new();
        p.set_freq(220.0, 48_000.0);
        for _ in 0..4_000 {
            let square = pulse(&p, 0.5);
            let symmetric = if p.phase() < 0.5 { 1.0 } else { -1.0 };
            // Away from the edges the pulse is the square exactly.
            if p.phase() > 0.01 && (p.phase() - 0.5).abs() > 0.01 && p.phase() < 0.99 {
                assert_eq!(square, symmetric);
            }
            p.advance();
        }
    }

    #[test]
    fn the_triangle_is_two_straight_lines_with_no_spike_at_the_reset() {
        // The defect this guards: folding the band-limited saw put a full-scale spike at every
        // reset. A triangle's largest per-sample step is its slope, and nothing else.
        let fs = 48_000.0;
        let mut vco = Vco::new();
        let mut prev = vco.process(100.0, Wave::Triangle, 0.5, fs);
        let nominal = 4.0 * 100.0 / fs;
        let mut steady = 0usize;
        let mut worst = 0.0f32;
        for _ in 0..2_000 {
            let y = vco.process(100.0, Wave::Triangle, 0.5, fs);
            let step = (y - prev).abs();
            worst = worst.max(step);
            if (step - nominal).abs() < nominal * 0.05 {
                steady += 1;
            }
            prev = y;
        }
        assert!(
            worst < nominal * 1.5,
            "a step of {worst} where the slope is {nominal}: a spike"
        );
        assert!(
            steady > 1_900,
            "straight lines almost everywhere: {steady} of 2000"
        );
    }

    #[test]
    fn output_is_bounded_and_finite_at_extreme_pitch() {
        for fs in RATES {
            let mut vco = Vco::new();
            for freq in [0.0f32, 1.0, 20_000.0, 40_000.0, 1e9, -100.0] {
                for wave in [Wave::Saw, Wave::Square, Wave::Triangle] {
                    for width in [0.0f32, 0.05, 0.5, 0.95, 1.0] {
                        for _ in 0..1_000 {
                            let y = vco.process(freq, wave, width, fs);
                            assert!(y.is_finite(), "non-finite at {freq} Hz {wave:?} {fs}");
                            assert!(y.abs() <= 1.5, "unbounded: {y}");
                        }
                    }
                }
            }
        }
    }

    #[test]
    fn sync_out_is_a_square_at_the_vco_frequency_whatever_the_waveform() {
        let fs = 48_000.0;
        for wave in [Wave::Saw, Wave::Square, Wave::Triangle] {
            let mut vco = Vco::new();
            let (mut crossings, mut prev) = (0usize, -1.0f32);
            for _ in 0..(fs as usize) {
                vco.process(440.0, wave, 0.2, fs);
                let s = vco.sync_out();
                if prev < 0.0 && s > 0.0 {
                    crossings += 1;
                }
                prev = s;
            }
            assert!(
                (crossings as i32 - 440).abs() <= 1,
                "{wave:?}: {crossings} edges"
            );
        }
    }

    /// A master at `f_m` and a slave at `f_m · ratio`, synced; the slave's average frequency from
    /// its own wraps, after settling.
    fn slave_frequency(ratio: f32, strength: SyncStrength, enabled: bool) -> (f32, f32) {
        let fs = 48_000.0;
        let f_m = 220.0;
        let mut master = Vco::new();
        let mut slave = Vco::new();
        let settle = (fs * 0.5) as usize;
        let window = (fs * 2.0) as usize;
        let mut wraps = 0usize;
        for i in 0..settle + window {
            master.process(f_m, Wave::Saw, 0.5, fs);
            let edge = if enabled { master.sync_edge() } else { None };
            slave.process_slave(f_m * ratio, Wave::Saw, 0.5, fs, edge, strength, 1.0);
            if i >= settle && slave.just_reset() {
                wraps += 1;
            }
        }
        (f_m, wraps as f32 / (window as f32 / fs))
    }

    #[test]
    fn strong_sync_pins_the_slaves_period_to_the_masters_at_any_ratio() {
        for ratio in [1.3f32, 1.5, 1.71, 2.3] {
            let (f_m, f_s) = slave_frequency(ratio, SyncStrength::Strong, true);
            // Every master edge resets the slave, so the slave's resets number the master's
            // plus its own wraps inside each master period.
            let expected = f_m * ratio.ceil();
            assert!(
                (f_s - expected).abs() < 2.0,
                "ratio {ratio}: slave reset at {f_s} Hz, expected {expected}"
            );
        }
    }

    #[test]
    fn weak_sync_locks_just_under_the_named_ratios_and_rolls_above_them() {
        // The manual: 2/3, 3/4, 1/1, 4/3, 3/2. The chosen kick gives one-sided ranges below each.
        for (num, den) in [(3.0f32, 2.0f32), (4.0, 3.0), (1.0, 1.0)] {
            let ratio = num / den;
            // Inside the one-sided range: an orbit of `den` edges can absorb one kick's worth of
            // shortfall through its early wrap, so the range is (ratio - kick, ratio - kick(1-1/den)).
            let under = ratio - WEAK_SYNC_KICK * (1.0 - 0.5 / den);
            let (f_m, f_locked) = slave_frequency(under, SyncStrength::Weak, true);
            let cents = 1200.0 * (f_locked / (f_m * ratio)).log2();
            assert!(
                cents.abs() < 3.0,
                "{num}/{den} from below: {cents:+.1} cents off the lock"
            );

            let over = ratio + 0.1;
            let (_, f_free) = slave_frequency(over, SyncStrength::Weak, true);
            let (_, f_unsynced) = slave_frequency(over, SyncStrength::Weak, false);
            let cents = 1200.0 * (f_free / (f_m * ratio)).log2();
            assert!(
                cents > 20.0,
                "{num}/{den} from above should roll: {cents:+.1} cents"
            );
            assert!(f_free > f_unsynced, "the kicks push it on, not back");
        }
    }

    #[test]
    fn an_unsynced_slave_is_the_direct_oscillator_but_for_its_first_sample() {
        // The hold is internal: the emitted sample is for the same instant a master emits, so a
        // slave with no edges is bit-identical to `process` from the second sample on.
        let fs = 48_000.0;
        let mut a = Vco::new();
        let mut b = Vco::new();
        let mut unsynced = Vec::new();
        let mut synced_off = Vec::new();
        for _ in 0..2_000 {
            unsynced.push(a.process_slave(
                330.0,
                Wave::Saw,
                0.5,
                fs,
                None,
                SyncStrength::Strong,
                1.0,
            ));
            synced_off.push(b.process_slave(
                330.0,
                Wave::Saw,
                0.5,
                fs,
                None,
                SyncStrength::Weak,
                1.0,
            ));
        }
        assert_eq!(
            unsynced, synced_off,
            "the strength is irrelevant with no edges"
        );
        let mut reference = Vco::new();
        let direct: Vec<f32> = (0..2_000)
            .map(|_| reference.process(330.0, Wave::Saw, 0.5, fs))
            .collect();
        assert_eq!(unsynced[0], 0.0, "the first output is the empty hold");
        assert!(
            unsynced[1..]
                .iter()
                .zip(&direct[1..])
                .all(|(s, d)| (s - d).abs() < 1e-6),
            "then the direct output, aligned"
        );
    }

    #[test]
    fn a_hard_synced_saw_is_corrected_at_the_reset() {
        // The oracle: the band-limited synced saw's aliasing against the same thing with the
        // corrections removed — a naive reset. The correction has to buy a real margin.
        let fs = 48_000.0;
        let f_m = 21.0 * fs / 2048.0; // periodic in the window
        let ratio = 1.7;
        let render = |corrected: bool| -> Vec<f64> {
            let mut master = Vco::new();
            let mut slave = Vco::new();
            let mut out = Vec::new();
            let mut naive_phase = 0.0f32;
            for _ in 0..4096 + 1 {
                master.process(f_m, Wave::Saw, 0.5, fs);
                let edge = master.sync_edge();
                let y = slave.process_slave(
                    f_m * ratio,
                    Wave::Saw,
                    0.5,
                    fs,
                    edge,
                    SyncStrength::Strong,
                    1.0,
                );
                if corrected {
                    out.push(y as f64);
                } else {
                    // A naive slave: phase reset on the edge, no corrections at all.
                    if edge.is_some() {
                        naive_phase = 0.0;
                    }
                    out.push((2.0 * naive_phase - 1.0) as f64);
                    naive_phase = (naive_phase + f_m * ratio / fs).fract();
                }
            }
            out[1..2049].to_vec()
        };
        let corrected = alias_to_signal_db(&render(true), 21);
        let naive_db = alias_to_signal_db(&render(false), 21);
        assert!(
            corrected < naive_db - 6.0,
            "the reset correction should buy at least 6 dB: corrected {corrected:.1} dB, naive {naive_db:.1} dB"
        );
    }

    #[test]
    fn changing_frequency_never_resets_the_phase() {
        let fs = 48_000.0;
        let mut p = Phasor::new();
        p.set_freq(220.0, fs);
        for _ in 0..100 {
            p.advance();
        }
        let before = p.phase();
        p.set_freq(880.0, fs);
        assert_eq!(p.phase(), before, "set_freq moved the phase");
    }

    /// Alias-to-signal ratio of an exactly-periodic buffer, in dB — `mxm-mono-01-dsp`'s method.
    fn alias_to_signal_db(x: &[f64], periods: usize) -> f64 {
        let n = x.len();
        let half = n / 2;
        let (mut wanted, mut alias) = (0.0f64, 0.0f64);
        for bin in 1..half {
            let (mut re, mut im) = (0.0f64, 0.0f64);
            for (i, &v) in x.iter().enumerate() {
                let ang = -2.0 * std::f64::consts::PI * bin as f64 * i as f64 / n as f64;
                re += v * ang.cos();
                im += v * ang.sin();
            }
            let power = re * re + im * im;
            if bin % periods == 0 {
                wanted += power;
            } else {
                alias += power;
            }
        }
        10.0 * (alias / wanted.max(1e-30)).max(1e-30).log10()
    }

    fn render(wave: Wave, n: usize, periods: usize) -> Vec<f64> {
        let mut p = Phasor::new();
        p.set_inc(periods as f32 / n as f32);
        (0..n)
            .map(|_| {
                let y = match wave {
                    Wave::Saw => saw(&p),
                    Wave::Square => pulse(&p, 0.5),
                    Wave::Triangle => triangle(&p),
                };
                p.advance();
                y as f64
            })
            .collect()
    }

    #[test]
    fn sawtooth_aliasing_stays_far_below_the_trivial_waveform() {
        const N: usize = 2048;
        const PERIODS: usize = 21;
        let shipped_db = alias_to_signal_db(&render(Wave::Saw, N, PERIODS), PERIODS);
        assert!(shipped_db < -30.0, "sawtooth aliasing {shipped_db:.1} dB");
    }

    /// A naive triangle's harmonics fall at `1/n²`, so even without band-limiting it aliases
    /// quietly — quieter than the band-limited saw. Pinned so a later "improvement" that
    /// reintroduces a spike at the reset is caught by its aliasing as well as by its shape.
    #[test]
    fn the_naive_triangle_aliases_less_than_the_band_limited_saw() {
        const N: usize = 2048;
        const PERIODS: usize = 21;
        let tri_db = alias_to_signal_db(&render(Wave::Triangle, N, PERIODS), PERIODS);
        let saw_db = alias_to_signal_db(&render(Wave::Saw, N, PERIODS), PERIODS);
        assert!(
            tri_db < -35.0,
            "triangle aliasing {tri_db:.1} dB, expected below -35 dB"
        );
        assert!(
            tri_db < saw_db,
            "tri {tri_db:.1} dB should be under saw {saw_db:.1} dB"
        );
    }
}
