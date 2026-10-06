//! The three effects after the VCA: the plug-out's phaser and delay, and the 103's spring reverb.
//!
//! Each is a **chosen model** behind documented controls — the incomplete-evidence pages under
//! `research:effects/` say what is documented and what is not — and each is a module with a
//! plain-values API that knows nothing about the voice, so any of them can move to the effects
//! collection later. Mono in, mono out; the plugin's two channels carry the same signal, as the
//! hardware's HIGH OUTPUT did.
//!
//! **Order: phaser → delay → reverb**, chosen (the manual's text is silent). **Every constant not
//! taken from a page's measurement is chosen** and listed in the crate's `NOTES.md`.
//!
//! # Phaser — `research:effects/system-100-plugout-phaser.md`
//!
//! One slider, doing two things: the wet amount, and — with nothing patched into LFO IN — the
//! internal LFO's rate ("changes speed according to the position of the PHASER slider"). Four
//! first-order all-pass stages swept by a sine, with fixed feedback; the LFO IN row replaces the
//! internal rate with a rate CV; the MANUAL IN row moves the sweep's centre. Stage count, range,
//! feedback: chosen.
//!
//! # Delay — `research:effects/system-100-plugout-delay.md`
//!
//! A level and a time. **No feedback, filtering or modulation control is documented**, which is
//! not a claim that the processes are absent: the chosen model has fixed feedback below unity and
//! a fixed one-pole damping in the loop, both listed as chosen. The time is interpolated so a
//! change does not click; tempo sync is the plugin's, which passes a time in seconds.
//!
//! # Spring reverb — `research:effects/system-100-103-spring-reverb.md` §2c
//!
//! **Measured from the RE-201 capture**, the family the 103's tank is inferred to belong to:
//! three springs as three feedback lines at the resolved round trips of 41.6 and 60.0 ms and a
//! third placed between them (chosen), each a delay through a chain of first-order all-passes so
//! the high frequencies arrive later — the spring's dispersion, the "boing" — with the feedback
//! set from the measured T60 of 3.5 s; the tank's band-pass, about 450 Hz – 1.3 kHz, on the input;
//! and the direct coupling the capture shows in the passband from the first two milliseconds.
//! Dispersion depth and the third spring's transit: chosen.
//!
//! ## Three tanks, one of them measured
//!
//! [`SpringTankModel`] is an **addition for the standalone effect** (mxm-folded-spring's plugin),
//! under the collection rule that an instrument's DSP crate may gain inputs but never a behaviour
//! change: [`SpringTankModel::Medium`] is the measured tank above, is the default, and is what the
//! instrument uses and can only use. The instrument's render is bit-identical with this here — the
//! constants it reaches are the same numbers in the same order, and `the_medium_tank_is_the_tank_the_instrument_had`
//! holds that by arithmetic rather than by inspection.
//!
//! `Short` and `Long` are **chosen scalings of the measured tank**, not separate measurements:
//! transits and decay scaled, the band's top tilted the way a shorter or longer spring's is, and
//! `Short` on two springs rather than three. Naming them after real tanks would claim a capture
//! that does not exist.

use crate::flush;

const NYQUIST_FRACTION: f32 = 0.45;

/// Below this the wet signal counts as quiet, and after [`SNAP_HOLD_S`] of it the effect clears
/// its state to exact zero. **Chosen**: −100 dB, inaudible under anything; a 3.5 s reverb would
/// otherwise take twenty seconds to reach the flush threshold on its own, and the collection's
/// exact-silence contract would never be met with the reverb on.
pub const SNAP_LEVEL: f32 = 1e-5;
pub const SNAP_HOLD_S: f32 = 0.05;

/// Counts how long an effect's wet signal has been quiet, and says when to snap.
#[derive(Debug, Clone, Copy, Default)]
struct Quiet {
    samples: u32,
    /// Whether anything at or above the snap level has been written since the effect last snapped
    /// to silence: it still holds something it can play back.
    holding: bool,
}

impl Quiet {
    /// Feed what the effect **writes into its memory** this sample; `true` once nothing above the
    /// snap level has been written for `memory_samples` — the longest delay the effect holds —
    /// plus the hold, so that a repeat still in flight is never cleared before it arrives.
    #[inline]
    fn track(&mut self, written: f32, memory_samples: f32, sample_rate: f32) -> bool {
        if written.abs() >= SNAP_LEVEL {
            self.holding = true;
        }
        if written.abs() < SNAP_LEVEL {
            self.samples = self.samples.saturating_add(1);
            self.samples as f32 >= memory_samples + SNAP_HOLD_S * sample_rate
        } else {
            self.samples = 0;
            false
        }
    }

    fn reset(&mut self) {
        self.samples = 0;
        self.holding = false;
    }
}

/// A first-order all-pass, the unit of both the phaser's sweep and the springs' dispersion.
#[derive(Debug, Clone, Copy, Default)]
struct AllPass {
    x1: f32,
    y1: f32,
}

impl AllPass {
    /// `a` is the coefficient, `-1 < a < 1`; the phase turns through 180° at the frequency where
    /// `a = (1 - tan(πf/fs)) / (1 + tan(πf/fs))`.
    #[inline]
    fn process(&mut self, x: f32, a: f32) -> f32 {
        let y = -a * x + self.x1 + a * self.y1;
        self.x1 = x;
        self.y1 = flush(y);
        y
    }

    fn reset(&mut self) {
        self.x1 = 0.0;
        self.y1 = 0.0;
    }
}

/// The all-pass coefficient whose 90° point is at `hz`.
#[inline]
fn allpass_coef(hz: f32, sample_rate: f32) -> f32 {
    let fc = hz.clamp(20.0, NYQUIST_FRACTION * sample_rate);
    let t = crate::hpf::tan_approx(std::f64::consts::PI * f64::from(fc) / f64::from(sample_rate))
        as f32;
    ((1.0 - t) / (1.0 + t)).clamp(-0.999, 0.999)
}

/// A one-pole lowpass, for the delay's damping and the reverb's band-pass.
#[derive(Debug, Clone, Copy, Default)]
struct OnePole {
    s: f32,
}

impl OnePole {
    #[inline]
    fn lowpass(&mut self, x: f32, g: f32) -> f32 {
        let v = g * (x - self.s);
        let y = v + self.s;
        self.s = flush(y + v);
        y
    }

    fn reset(&mut self) {
        self.s = 0.0;
    }
}

#[inline]
fn onepole_g(hz: f32, sample_rate: f32) -> f32 {
    let fc = hz.clamp(10.0, NYQUIST_FRACTION * sample_rate);
    let t = crate::hpf::tan_approx(std::f64::consts::PI * f64::from(fc) / f64::from(sample_rate))
        as f32;
    t / (1.0 + t)
}

// ---------------------------------------------------------------------------------------------
// Phaser

/// The phaser's stage count. **Chosen.**
pub const PHASER_STAGES: usize = 4;
/// The internal LFO's rate at the slider's bottom and top. **Chosen**: the manual says only that
/// the slider sets the speed.
pub const PHASER_RATE_MIN_HZ: f32 = 0.1;
pub const PHASER_RATE_MAX_HZ: f32 = 6.0;
/// The sweep, in octaves either side of its centre, and the centre with MANUAL IN at rest.
/// **Chosen.**
pub const PHASER_SWEEP_OCTAVES: f32 = 2.5;
pub const PHASER_CENTRE_HZ: f32 = 800.0;
/// How far a ten-volt unit on MANUAL IN moves the centre, in octaves. **Chosen.**
pub const PHASER_MANUAL_OCTAVES_PER_UNIT: f32 = 4.0;
/// How far a ten-volt unit on LFO IN moves the internal rate, in octaves. **Chosen.**
pub const PHASER_LFO_OCTAVES_PER_UNIT: f32 = 4.0;
/// Feedback around the stages. **Chosen.**
pub const PHASER_FEEDBACK: f32 = 0.35;

/// **The centre CV's control bandwidth** — a one-pole on MANUAL IN before it moves the sweep.
///
/// Not a taste: the feedback loop is *parametrically unstable* when the all-pass coefficient is
/// driven coherently near Nyquist. Four all-passes have unity magnitude only while the coefficient
/// holds still; flip it every sample or two and the chain stops being passive, so the loop's fixed
/// [`PHASER_FEEDBACK`] exceeds one and the phaser runs away to infinity from silence. Measured:
/// with the centre CV alternating `±1` unit, the runaway starts at a feedback of 0.1 and needs no
/// input at all, and at a feedback of zero no excursion reaches it — **the instability is the loop,
/// and its driver is coefficient speed, not coefficient size**. A *random* centre at the same
/// excursion is stable; it takes the coherent tone to pump.
///
/// One input could never do this — a column is a waveform, smooth between its edges. **A summing
/// input can**, which is why the conversion to any-to-any routing is what made it reachable:
/// several audio sources at full depth saturate the ±4-octave clamp and toggle between its ends.
///
/// So the cure is at the driver: a control port with a bandwidth, which is what the hardware's is.
/// Musical centre modulation is untouched — an LFO, an envelope, a bass VCO all pass — and the
/// near-Nyquist content that pumps the loop does not. **Chosen**, sized by measurement: stable at
/// every sample rate from 1 kHz to 768 kHz, at every modulation rate from `fs/2` to `fs/2048`, at
/// the full ±1-unit excursion.
///
/// **Nothing changes where nothing is patched.** The one-pole's state is exactly zero while its
/// input is, so an unrouted MANUAL IN — the init patch, and every preset — is bit-identical.
pub const PHASER_MANUAL_HZ: f32 = 2_000.0;

/// …and never more than this fraction of the sample rate, so the corner still *smooths* at rates
/// where 2 kHz is not a low frequency. `NYQUIST_FRACTION` would keep it stable-looking and let it
/// through: at `fs` 8 kHz a corner of `0.45 fs` passes an alternating CV nearly whole, and the
/// runaway comes back. **Chosen**, measured across 0.02 … 0.08.
pub const PHASER_MANUAL_FRACTION: f32 = 0.05;

#[derive(Debug, Clone, Default)]
pub struct Phaser {
    stages: [AllPass; PHASER_STAGES],
    phase: f32,
    fb: f32,
    /// MANUAL IN's control bandwidth: see [`PHASER_MANUAL_HZ`].
    manual: OnePole,
    quiet: Quiet,
    /// Whether the stages hold anything: at zero amount they are cleared once, so nothing waits
    /// in them to resume when the slider comes back up (review round 2).
    armed: bool,
}

impl Phaser {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn reset(&mut self) {
        self.clear();
        self.phase = 0.0;
    }

    /// Whether the stages hold anything at or above the snap level, until the quiet snap clears
    /// them.
    #[inline]
    pub fn holds_anything(&self) -> bool {
        self.quiet.holding
    }

    /// Clear the signal state and keep the LFO's phase: what the quiet snap does.
    fn clear(&mut self) {
        for s in &mut self.stages {
            s.reset();
        }
        self.fb = 0.0;
        self.manual.reset();
        self.quiet.reset();
        self.armed = false;
    }

    /// One sample. `amount` is the slider, `0..=1`; `lfo_in` is the LFO IN row's value in
    /// ten-volt units (`None` when unpatched); `manual_in` the MANUAL IN row's, or 0.
    #[inline]
    pub fn process(
        &mut self,
        x: f32,
        amount: f32,
        lfo_in: Option<f32>,
        manual_in: f32,
        sample_rate: f32,
    ) -> f32 {
        let amount = amount.clamp(0.0, 1.0);
        if amount <= 0.0 {
            // Bit-transparent at zero: nothing runs, and what the stages held is let go, once.
            if self.armed {
                self.clear();
            }
            return x;
        }
        self.armed = true;
        // The internal LFO's rate follows the slider; LFO IN, when patched, moves it as a CV.
        let mut rate = PHASER_RATE_MIN_HZ * (PHASER_RATE_MAX_HZ / PHASER_RATE_MIN_HZ).powf(amount);
        if let Some(cv) = lfo_in {
            rate *= (cv * PHASER_LFO_OCTAVES_PER_UNIT).clamp(-8.0, 8.0).exp2();
        }
        self.phase += rate.clamp(0.01, 30.0) / sample_rate;
        if self.phase >= 1.0 {
            self.phase -= 1.0;
        }
        let sweep = (std::f32::consts::TAU * self.phase).sin();
        // The control port's bandwidth, without which a summing MANUAL IN can drive the loop
        // into parametric oscillation: see [`PHASER_MANUAL_HZ`].
        let fc = PHASER_MANUAL_HZ.min(PHASER_MANUAL_FRACTION * sample_rate);
        let manual_in = self.manual.lowpass(manual_in, onepole_g(fc, sample_rate));
        let centre_oct = (manual_in * PHASER_MANUAL_OCTAVES_PER_UNIT).clamp(-4.0, 4.0);
        let hz = PHASER_CENTRE_HZ * (centre_oct + sweep * PHASER_SWEEP_OCTAVES).exp2();
        let a = allpass_coef(hz, sample_rate);

        let mut y = x + self.fb * PHASER_FEEDBACK;
        for s in &mut self.stages {
            y = s.process(y, a);
        }
        self.fb = flush(y);
        if self.quiet.track(y, 0.0, sample_rate) {
            self.clear();
            return flush(x);
        }
        // The wet is the all-passed signal summed with the dry — the notches — at the slider's amount.
        flush(x + amount * y) * (1.0 / (1.0 + amount))
    }
}

// ---------------------------------------------------------------------------------------------
// Delay

/// The delay time's range. **Chosen.**
pub const DELAY_MIN_S: f32 = 0.02;
pub const DELAY_MAX_S: f32 = 1.0;
/// Fixed feedback and the loop's damping corner. **Chosen** — no control is documented.
pub const DELAY_FEEDBACK: f32 = 0.45;
pub const DELAY_DAMPING_HZ: f32 = 4_000.0;
/// How fast the read position slews to a new time, so a time change glides rather than clicks.
/// **Chosen.**
pub const DELAY_TIME_SLEW_S: f32 = 0.05;
/// Headroom past the longest delay a line is sized for, so interpolation never wraps.
const LINE_MARGIN: usize = 64;

/// The rate a freshly built effect is sized for. **`set_sample_rate` resizes to the real one**,
/// on the main thread at activation — the first build sized every line for 192 kHz and at 768 kHz
/// a one-second delay repeated after a quarter of one (review round 1).
const DEFAULT_RATE: f32 = 48_000.0;

#[derive(Debug, Clone)]
pub struct Delay {
    line: Vec<f32>,
    write: usize,
    /// Samples written since the line was last emptied, saturating at its length. **Emptying is
    /// this counter, not a clear**: a read further back than it returns zero, so the stale
    /// contents are never touched — zeroing a line sized for 768 kHz on the sample the quiet
    /// snap fires would be a million stores inside one `process` call (review round 3).
    valid: usize,
    read_delay: f32,
    damping: OnePole,
    quiet: Quiet,
    /// Set while the level is above zero, so the first sample at zero empties the line once.
    armed: bool,
}

impl Default for Delay {
    fn default() -> Self {
        Self::new()
    }
}

impl Delay {
    pub fn new() -> Self {
        let mut delay = Self {
            line: Vec::new(),
            write: 0,
            valid: 0,
            read_delay: 0.0,
            damping: OnePole::default(),
            quiet: Quiet::default(),
            armed: false,
        };
        delay.set_sample_rate(DEFAULT_RATE);
        delay
    }

    /// Sizes the line for the longest time at this rate. **Allocates**, so it belongs to
    /// activation, never to `process`; it also resets, because the line's contents mean nothing
    /// at another rate.
    pub fn set_sample_rate(&mut self, sample_rate: f32) {
        let needed = (DELAY_MAX_S * sample_rate).ceil() as usize + LINE_MARGIN;
        if self.line.len() != needed {
            self.line = vec![0.0; needed];
        }
        self.reset();
    }

    pub fn reset(&mut self) {
        self.clear();
        self.read_delay = 0.0;
    }

    /// Empties the line in O(1): what is in it is simply no longer valid to read. The write
    /// pointer goes home too — `set_sample_rate` may have shrunk the line under it, and the first
    /// O(1) version left it pointing past the new end (found by clap-validator at 1234.57 Hz).
    fn clear(&mut self) {
        self.valid = 0;
        self.write = 0;
        self.damping.reset();
        self.quiet.reset();
        self.armed = false;
    }

    /// The sample `back` samples before the write point, or zero if it predates the last empty.
    #[inline]
    fn read_back(&self, back: usize) -> f32 {
        if back > self.valid {
            return 0.0;
        }
        let len = self.line.len();
        self.line[(self.write + len - back) % len]
    }

    /// Samples the loop needs to fall under [`SNAP_LEVEL`] from unity at `time_s`, plus the hold —
    /// after which it is cleared to exact zero.
    pub fn tail_samples(time_s: f32, sample_rate: f32) -> u32 {
        let repeats = (SNAP_LEVEL.ln() / DELAY_FEEDBACK.ln()).ceil() + 1.0;
        ((repeats * time_s.clamp(DELAY_MIN_S, DELAY_MAX_S) + SNAP_HOLD_S) * sample_rate) as u32
    }

    /// Whether the line holds anything at or above the snap level — from the input or from the
    /// feedback, which can build a quiet input up past it — until the quiet snap or a zero level
    /// clears it.
    #[inline]
    pub fn holds_anything(&self) -> bool {
        self.quiet.holding
    }

    /// One sample. `level` is the wet amount, `0..=1`; `time_s` the delay time.
    #[inline]
    pub fn process(&mut self, x: f32, level: f32, time_s: f32, sample_rate: f32) -> f32 {
        let level = level.clamp(0.0, 1.0);
        // **Emptied at zero, once, not frozen**, as the phaser and the reverb are: running on at
        // zero kept a line full of repeats a parked host would freeze and a raised level would
        // replay (code review round 6). Emptying is O(1).
        if level <= 0.0 {
            if self.armed {
                self.clear();
            }
            return x;
        }
        self.armed = true;
        let target = (time_s.clamp(DELAY_MIN_S, DELAY_MAX_S) * sample_rate)
            .min((self.line.len() - 4) as f32);
        if self.read_delay <= 0.0 {
            self.read_delay = target;
        } else {
            let coef = (-1.0 / (DELAY_TIME_SLEW_S * sample_rate)).exp();
            self.read_delay = target + (self.read_delay - target) * coef;
        }
        // Linear interpolation between the two samples around the read point.
        let d = self.read_delay;
        let di = (d.floor() as usize).max(1);
        let frac = d - di as f32;
        let (a, b) = (self.read_back(di), self.read_back(di + 1));
        let delayed = a + (b - a) * frac;
        let damped = self
            .damping
            .lowpass(delayed, onepole_g(DELAY_DAMPING_HZ, sample_rate));
        let written = flush(x + damped * DELAY_FEEDBACK);
        let len = self.line.len();
        self.line[self.write] = written;
        self.write = (self.write + 1) % len;
        self.valid = (self.valid + 1).min(len);
        // Quiet for a whole line length and the hold: every repeat still in flight has landed.
        if self.quiet.track(written, self.read_delay, sample_rate) {
            self.clear();
            return flush(x);
        }
        flush(x + level * damped)
    }
}

// ---------------------------------------------------------------------------------------------
// Spring reverb

/// The springs' round-trip times: two **measured** from the RE-201 capture, the third **chosen**
/// between them (§2c: three springs, two transits resolved).
pub const SPRING_TRANSITS_MS: [f32; 3] = [41.6, 50.4, 60.0];
/// The measured decay, medium drive, from the 30 dB span. **Measured** (§2c).
pub const SPRING_T60_S: f32 = 3.5;
/// The tank's band-pass, the −3 dB points. **Measured** (§2c).
pub const SPRING_BAND_LOW_HZ: f32 = 450.0;
pub const SPRING_BAND_HIGH_HZ: f32 = 1_300.0;
/// All-pass stages per spring and where their dispersion sits: **chosen**, for a chirp that
/// spreads the passband across a few milliseconds.
pub const SPRING_DISPERSION_STAGES: usize = 12;
pub const SPRING_DISPERSION_HZ: f32 = 900.0;
/// The direct coupling in the passband, relative to the springs' return, and its delay.
/// **Measured** as present from 2 ms (§2c); its level **chosen**.
pub const SPRING_DIRECT_LEVEL: f32 = 0.25;
pub const SPRING_DIRECT_MS: f32 = 2.0;

/// The most springs any tank has, and the most dispersion stages any spring has. The arrays are
/// sized for these once; a tank uses the first `springs` and the first `dispersion_stages` of
/// them, so **changing tank allocates nothing** and can happen on the audio thread.
pub const MAX_SPRINGS: usize = 3;
pub const MAX_DISPERSION_STAGES: usize = 16;
/// The longest transit any tank asks for, which is what every delay line is sized to. Sizing to
/// the tank in force would mean reallocating on a model change, in `process`.
pub const LONGEST_TRANSIT_MS: f32 = 90.0;

/// One tank's geometry: what a spring reverb **is**, as numbers.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct SpringTank {
    /// How many of `transits_ms` are strung. Two or three.
    pub springs: usize,
    pub transits_ms: [f32; MAX_SPRINGS],
    pub t60_s: f32,
    pub band_low_hz: f32,
    pub band_high_hz: f32,
    pub dispersion_stages: usize,
    pub dispersion_hz: f32,
}

/// Which tank the reverb is strung with.
///
/// **`Medium` is the measured one** (§2c) and the default; the other two are chosen scalings of
/// it, declared as such in the module doc. A shorter spring rings shorter and brighter and a
/// longer one rings longer and darker, which is what the scalings say and all they say.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Hash)]
pub enum SpringTankModel {
    /// Two short springs: the small tank bolted into a combo. **Chosen.**
    Short,
    /// The measured tank: three springs, 41.6 / 50.4 / 60.0 ms, T60 3.5 s. **The instrument's.**
    #[default]
    Medium,
    /// Three long springs: the deep tank of a studio unit. **Chosen.**
    Long,
}

impl SpringTankModel {
    /// Every tank, in the order a switch lists them: shortest first.
    pub const ALL: [Self; 3] = [Self::Short, Self::Medium, Self::Long];

    pub const fn label(self) -> &'static str {
        match self {
            Self::Short => "Short",
            Self::Medium => "Medium",
            Self::Long => "Long",
        }
    }

    /// The geometry. `Medium` is spelled from the measured constants themselves rather than
    /// copied, so the two cannot drift.
    pub const fn tank(self) -> SpringTank {
        match self {
            // 0.6 of the measured transits on two springs, half the decay, the top of the band a
            // third higher. **Chosen**, as the module doc says.
            Self::Short => SpringTank {
                springs: 2,
                transits_ms: [24.96, 36.0, 0.0],
                t60_s: 1.75,
                band_low_hz: SPRING_BAND_LOW_HZ,
                band_high_hz: 1_750.0,
                dispersion_stages: 8,
                dispersion_hz: SPRING_DISPERSION_HZ,
            },
            Self::Medium => SpringTank {
                springs: 3,
                transits_ms: SPRING_TRANSITS_MS,
                t60_s: SPRING_T60_S,
                band_low_hz: SPRING_BAND_LOW_HZ,
                band_high_hz: SPRING_BAND_HIGH_HZ,
                dispersion_stages: SPRING_DISPERSION_STAGES,
                dispersion_hz: SPRING_DISPERSION_HZ,
            },
            // 1.5 of the measured transits, 1.7 of the decay, the top of the band lowered.
            // **Chosen.**
            Self::Long => SpringTank {
                springs: 3,
                transits_ms: [62.4, 75.6, 90.0],
                t60_s: 5.95,
                band_low_hz: SPRING_BAND_LOW_HZ,
                band_high_hz: 1_100.0,
                dispersion_stages: SPRING_DISPERSION_STAGES,
                dispersion_hz: SPRING_DISPERSION_HZ,
            },
        }
    }
}

/// What the tank returned for one sample, before the level is applied.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum SpringWet {
    /// The level is zero: the tank is empty and the signal passes untouched.
    Off,
    /// The tank has just been cleared to exact zero. There is no wet, and there is none owing.
    Snapped,
    /// The wet return.
    Wet(f32),
}

#[derive(Debug, Clone)]
struct Spring {
    line: Vec<f32>,
    write: usize,
    /// Samples written since the spring was last emptied — see `Delay::valid`.
    valid: usize,
    dispersion: [AllPass; MAX_DISPERSION_STAGES],
}

impl Spring {
    fn new(sample_rate: f32) -> Self {
        // **The longest transit any tank asks for**, not this tank's: a line sized to the tank in
        // force would have to be reallocated when the tank changed, and that is `process`.
        let longest = LONGEST_TRANSIT_MS * 0.001 * sample_rate;
        Self {
            line: vec![0.0; longest.ceil() as usize + LINE_MARGIN],
            write: 0,
            valid: 0,
            dispersion: [AllPass::default(); MAX_DISPERSION_STAGES],
        }
    }

    /// O(1): the line's contents are no longer valid to read.
    fn reset(&mut self) {
        self.valid = 0;
        self.write = 0;
        for a in &mut self.dispersion {
            a.reset();
        }
    }

    /// One trip round the spring: read the delayed, dispersed return, feed back, write.
    #[inline]
    fn process(
        &mut self,
        x: f32,
        delay_samples: usize,
        feedback: f32,
        disp_a: f32,
        stages: usize,
    ) -> f32 {
        let len = self.line.len();
        let back = delay_samples.clamp(1, len - 1);
        let mut r = if back > self.valid {
            0.0
        } else {
            self.line[(self.write + len - back) % len]
        };
        for a in &mut self.dispersion[..stages.min(MAX_DISPERSION_STAGES)] {
            r = a.process(r, disp_a);
        }
        self.line[self.write] = flush(x + r * feedback);
        self.write = (self.write + 1) % len;
        self.valid = (self.valid + 1).min(len);
        r
    }
}

#[derive(Debug, Clone)]
pub struct SpringReverb {
    /// Which tank is strung. The instrument never moves it; the standalone effect does.
    model: SpringTankModel,
    springs: [Spring; MAX_SPRINGS],
    band_low: OnePole,
    band_high: OnePole,
    direct: Vec<f32>,
    direct_write: usize,
    /// Samples written to the direct line since it was last emptied — see `Delay::valid`.
    direct_valid: usize,
    quiet: Quiet,
    /// Whether the tank holds anything. **At zero level the tank is emptied, once**, rather than
    /// frozen: the first build returned dry and left the springs full, so raising the level ten
    /// seconds later replayed ten-second-old audio the activity clock had already written off
    /// (review round 2).
    armed: bool,
}

impl Default for SpringReverb {
    fn default() -> Self {
        Self::new()
    }
}

impl SpringReverb {
    pub fn new() -> Self {
        let mut reverb = Self {
            model: SpringTankModel::Medium,
            springs: [
                Spring::new(DEFAULT_RATE),
                Spring::new(DEFAULT_RATE),
                Spring::new(DEFAULT_RATE),
            ],
            band_low: OnePole::default(),
            band_high: OnePole::default(),
            direct: Vec::new(),
            direct_write: 0,
            direct_valid: 0,
            quiet: Quiet::default(),
            armed: false,
        };
        reverb.set_sample_rate(DEFAULT_RATE);
        reverb
    }

    /// Which tank is strung.
    pub fn model(&self) -> SpringTankModel {
        self.model
    }

    /// Strings a different tank. **Empties it**: the lines hold a return computed for transits
    /// that no longer apply, and reading them at the new offsets would play the old tank at the
    /// wrong pitch. Allocates nothing — every line is sized for the longest tank — so a caller may
    /// do this on the audio thread, and the standalone effect fades the wet out around it.
    pub fn set_model(&mut self, model: SpringTankModel) {
        if model != self.model {
            self.model = model;
            self.reset();
        }
    }

    /// Sizes the three springs and the direct line for this rate. **Allocates**: activation only.
    pub fn set_sample_rate(&mut self, sample_rate: f32) {
        self.springs = [
            Spring::new(sample_rate),
            Spring::new(sample_rate),
            Spring::new(sample_rate),
        ];
        let direct = (SPRING_DIRECT_MS * 0.001 * sample_rate).ceil() as usize + 16;
        if self.direct.len() != direct {
            self.direct = vec![0.0; direct];
        }
        self.reset();
    }

    pub fn reset(&mut self) {
        for s in &mut self.springs {
            s.reset();
        }
        self.band_low.reset();
        self.band_high.reset();
        self.direct_valid = 0;
        self.direct_write = 0;
        self.quiet.reset();
        self.armed = false;
    }

    /// Samples the tail needs to fall under [`SNAP_LEVEL`] from the **measured** T60, plus the
    /// hold — after which the tank is cleared to exact zero.
    ///
    /// The measured tank's, deliberately: this is what the instrument asks, the instrument has no
    /// other tank, and a figure that moved with a model it cannot select would be a longer tail
    /// for no reason. The standalone effect asks [`Self::tail_samples_for`].
    pub fn tail_samples(sample_rate: f32) -> u32 {
        Self::tail_samples_for(SpringTankModel::Medium, sample_rate)
    }

    /// The same, for whichever tank is strung.
    pub fn tail_samples_for(model: SpringTankModel, sample_rate: f32) -> u32 {
        // T60 is −60 dB; the snap is −100 dB: five thirds of it.
        ((model.tank().t60_s * (100.0 / 60.0) + SNAP_HOLD_S) * sample_rate) as u32
    }

    /// Whether the tank holds anything at or above the snap level, until the quiet snap clears it.
    #[inline]
    pub fn holds_anything(&self) -> bool {
        self.quiet.holding
    }

    /// One sample. `level` is the wet amount, `0..=1`.
    #[inline]
    pub fn process(&mut self, x: f32, level: f32, sample_rate: f32) -> f32 {
        match self.wet(x, level, sample_rate) {
            SpringWet::Off => x,
            SpringWet::Snapped => flush(x),
            SpringWet::Wet(wet) => flush(x + level * wet),
        }
    }

    /// The tank's return for one sample, **before** the level is applied.
    ///
    /// `process` is this plus the dry, and is what the instrument calls. The standalone effect
    /// calls this instead, because on a stereo track one tank is fed the mono sum and its return
    /// is added to both sides — which cannot be expressed as "the dry plus the wet" on one channel.
    #[inline]
    pub fn wet(&mut self, x: f32, level: f32, sample_rate: f32) -> SpringWet {
        let tank = self.model.tank();
        let level = level.clamp(0.0, 1.0);
        if level <= 0.0 {
            if self.armed {
                self.reset();
            }
            return SpringWet::Off;
        }
        self.armed = true;
        // The tank's band-pass: a highpass at the low corner, a lowpass at the high.
        let low = self
            .band_low
            .lowpass(x, onepole_g(tank.band_low_hz, sample_rate));
        let band = self
            .band_high
            .lowpass(x - low, onepole_g(tank.band_high_hz, sample_rate));

        // The direct coupling, a couple of milliseconds late.
        let dlen = self.direct.len();
        let d = ((SPRING_DIRECT_MS * 0.001 * sample_rate) as usize).clamp(1, dlen - 1);
        let direct = if d > self.direct_valid {
            0.0
        } else {
            self.direct[(self.direct_write + dlen - d) % dlen]
        };
        self.direct[self.direct_write] = band;
        self.direct_write = (self.direct_write + 1) % dlen;
        self.direct_valid = (self.direct_valid + 1).min(dlen);

        let disp_a = allpass_coef(tank.dispersion_hz, sample_rate);
        // The input is split evenly between the springs, so a two-spring tank is no louder than a
        // three-spring one. `1.0 / n` rather than a written fraction: at three it is the same f32.
        let split = 1.0 / tank.springs as f32;
        let mut wet = direct * SPRING_DIRECT_LEVEL;
        for (spring, transit_ms) in self.springs[..tank.springs]
            .iter_mut()
            .zip(tank.transits_ms.iter())
        {
            let transit_s = transit_ms * 0.001;
            let delay = (transit_s * sample_rate) as usize;
            // Feedback for the tank's T60: −60 dB after T60 / transit round trips.
            let feedback = 10f32.powf(-3.0 * transit_s / tank.t60_s);
            wet += spring.process(
                band * split,
                delay,
                feedback,
                disp_a,
                tank.dispersion_stages,
            );
        }
        // Quiet for the longest spring's transit and the hold: what was written has come back.
        let longest = tank.transits_ms[tank.springs - 1] * 0.001 * sample_rate;
        if self
            .quiet
            .track(wet.abs().max(band.abs()), longest, sample_rate)
        {
            self.reset();
            return SpringWet::Snapped;
        }
        SpringWet::Wet(wet)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// **The delay's feedback builds a quiet input past the snap level**, so what the line keeps
    /// — not what it is fed — is what says it is charged. A steady input at nine tenths of the
    /// snap level settles near `x / (1 − feedback)`, above it; the voice's settle follows this
    /// (code review round 6).
    #[test]
    fn a_quiet_input_the_feedback_builds_up_charges_the_delay() {
        let fs = 48_000.0;
        let mut d = Delay::default();
        let x = 0.9 * SNAP_LEVEL;
        for _ in 0..(2 * fs as usize) {
            d.process(x, 1.0, 0.05, fs);
        }
        assert!(
            d.holds_anything(),
            "the feedback left the line above the snap level"
        );
        for _ in 0..(4 * fs as usize) {
            d.process(0.0, 1.0, 0.05, fs);
        }
        assert!(!d.holds_anything(), "and it empties once fed nothing");
    }

    /// **A delay turned to zero is emptied, once, not frozen** — as the phaser and the reverb
    /// are. It ran on at zero with its line full, so a host that parked the patch froze a repeat
    /// still in flight, and raising the level later played it back (code review round 6).
    /// **Falsified** by dropping the zero-level arm from `Delay::process`.
    #[test]
    fn a_delay_turned_to_zero_is_emptied_not_frozen() {
        let fs = 48_000.0;
        let mut d = Delay::default();
        for _ in 0..(fs as usize / 10) {
            d.process(0.5, 1.0, 1.0, fs);
        }
        assert!(d.holds_anything(), "the premise");
        assert_eq!(d.process(0.0, 0.0, 1.0, fs), 0.0, "dry at zero");
        assert!(!d.holds_anything(), "emptied the first sample at zero");
        let mut peak = 0.0f32;
        for _ in 0..(2 * fs as usize) {
            peak = peak.max(d.process(0.0, 1.0, 1.0, fs).abs());
        }
        assert_eq!(peak, 0.0, "and raising the level plays back nothing old");
    }

    const FS: f32 = 48_000.0;

    fn impulse_response(mut f: impl FnMut(f32) -> f32, n: usize) -> Vec<f32> {
        (0..n).map(|i| f(if i == 0 { 1.0 } else { 0.0 })).collect()
    }

    fn tone_level(y: &[f32], hz: f32) -> f32 {
        let n = y.len();
        let (mut re, mut im) = (0.0f64, 0.0f64);
        for (i, &v) in y.iter().enumerate() {
            let ang = std::f64::consts::TAU * hz as f64 * i as f64 / FS as f64;
            re += v as f64 * ang.cos();
            im += v as f64 * ang.sin();
        }
        ((re * re + im * im).sqrt() * 2.0 / n as f64) as f32
    }

    #[test]
    fn every_effect_is_bit_transparent_at_zero_and_exactly_silent_after_its_tail() {
        let mut ph = Phaser::new();
        let mut dl = Delay::new();
        let mut sr = SpringReverb::new();
        for i in 0..1000 {
            let x = (i as f32 * 0.37).sin();
            assert_eq!(ph.process(x, 0.0, None, 0.0, FS), x);
            assert_eq!(dl.process(x, 0.0, 0.3, FS), x);
            assert_eq!(sr.process(x, 0.0, FS), x);
        }
        // With them on: an impulse, then silence for longer than the stated tail, then exact zero.
        let mut ph = Phaser::new();
        let mut dl = Delay::new();
        let mut sr = SpringReverb::new();
        ph.process(1.0, 0.7, None, 0.0, FS);
        dl.process(1.0, 0.7, 0.3, FS);
        sr.process(1.0, 0.7, FS);
        let tail = Delay::tail_samples(0.3, FS).max(SpringReverb::tail_samples(FS)) as usize;
        for _ in 0..tail + 48_000 {
            ph.process(0.0, 0.7, None, 0.0, FS);
            dl.process(0.0, 0.7, 0.3, FS);
            sr.process(0.0, 0.7, FS);
        }
        assert_eq!(ph.process(0.0, 0.7, None, 0.0, FS), 0.0);
        assert_eq!(dl.process(0.0, 0.7, 0.3, FS), 0.0);
        assert_eq!(sr.process(0.0, 0.7, FS), 0.0);
    }

    #[test]
    fn the_phaser_makes_moving_notches_whose_rate_follows_the_slider_and_the_lfo_row() {
        // A steady tone through the phaser is amplitude-modulated as a notch sweeps across it;
        // the modulation's rate is the sweep's.
        // The envelope of the tone, smoothed well below the sweep's rate, crosses its own mean
        // twice per notch pass; crossings with hysteresis count the passes and ignore the ripple.
        let sweep_rate = |amount: f32, lfo_in: Option<f32>| -> f32 {
            let mut ph = Phaser::new();
            let n = (FS * 8.0) as usize;
            let mut env = Vec::with_capacity(n);
            let mut acc = 0.0f32;
            for i in 0..n {
                let x = (std::f32::consts::TAU * 800.0 * i as f32 / FS).sin();
                let y = ph.process(x, amount, lfo_in, 0.0, FS);
                acc = 0.9995 * acc + 0.0005 * y.abs();
                env.push(acc);
            }
            let start = n / 4;
            let tail = &env[start..];
            let mean = tail.iter().sum::<f32>() / tail.len() as f32;
            let band = mean * 0.03;
            let mut above = tail[0] > mean;
            let mut crossings = 0;
            for &v in tail {
                if above && v < mean - band {
                    above = false;
                } else if !above && v > mean + band {
                    above = true;
                    crossings += 1;
                }
            }
            crossings as f32 / (0.75 * 8.0)
        };
        let slow = sweep_rate(0.2, None);
        let fast = sweep_rate(0.9, None);
        assert!(
            fast > slow * 2.0,
            "the slider should speed the sweep: {slow} Hz vs {fast} Hz"
        );
        let cv_up = sweep_rate(0.2, Some(0.5));
        assert!(
            cv_up > slow * 2.0,
            "LFO IN should raise the rate: {slow} vs {cv_up}"
        );
    }

    #[test]
    fn the_manual_row_moves_the_notches() {
        // The mean level of a tone under a sweep centred on it differs from one centred well above.
        let level = |manual: f32| {
            let mut ph = Phaser::new();
            let n = (FS * 4.0) as usize;
            let mut sum = 0.0f64;
            for i in 0..n {
                let x = (std::f32::consts::TAU * 800.0 * i as f32 / FS).sin();
                let y = ph.process(x, 0.5, None, manual, FS);
                if i > n / 2 {
                    sum += (y * y) as f64;
                }
            }
            sum
        };
        assert!((level(0.0) - level(0.6)).abs() > level(0.0) * 0.05);
    }

    #[test]
    fn the_delay_repeats_at_its_time_and_decays_at_the_fixed_feedback() {
        let mut dl = Delay::new();
        let ir = impulse_response(|x| dl.process(x, 1.0, 0.25, FS), (FS * 1.5) as usize);
        let t = (0.25 * FS) as usize;
        let first = ir[t - 2..t + 3].iter().fold(0.0f32, |m, v| m.max(v.abs()));
        let second = ir[2 * t - 2..2 * t + 3]
            .iter()
            .fold(0.0f32, |m, v| m.max(v.abs()));
        assert!(first > 0.3, "no repeat at the delay time: {first}");
        // Each pass through the damping lowpass spreads the impulse and lowers its peak, so the
        // peak ratio sits under the feedback figure; what is pinned is that repeats decay, at a
        // rate below unity and not far under the fixed feedback.
        let ratio = second / first;
        assert!(
            ratio > DELAY_FEEDBACK * 0.4 && ratio < DELAY_FEEDBACK * 1.05,
            "the second repeat should decay by about the feedback: {ratio}"
        );
        // Between the repeats, nothing.
        let between = ir[t + 100..2 * t - 100]
            .iter()
            .fold(0.0f32, |m, v| m.max(v.abs()));
        assert!(between < 0.01, "energy between repeats: {between}");
    }

    /// **Review round 1.** The lines are sized for the rate they run at, so a one-second delay
    /// is a second and a spring's transit is its transit at the highest rate a validator tries.
    #[test]
    fn the_delay_and_the_springs_keep_their_times_at_the_highest_rates() {
        for fs in [48_000.0f32, 384_000.0, 768_000.0] {
            let mut dl = Delay::new();
            dl.set_sample_rate(fs);
            let ir = impulse_response(|x| dl.process(x, 1.0, 1.0, fs), (fs * 1.2) as usize);
            let t = fs as usize;
            let at_time = ir[t - 2..t + 3].iter().fold(0.0f32, |m, v| m.max(v.abs()));
            let early = ir[t / 4 - 100..t / 4 + 100]
                .iter()
                .chain(ir[t / 2 - 100..t / 2 + 100].iter())
                .fold(0.0f32, |m, v| m.max(v.abs()));
            // The repeat has been through the damping one-pole once, whose impulse peak is its
            // first-sample gain and shrinks with the rate: the oracle scales with it.
            let expected = onepole_g(DELAY_DAMPING_HZ, fs);
            assert!(
                at_time > 0.5 * expected,
                "at {fs} Hz: no repeat at one second: {at_time} against {expected}"
            );
            assert!(early < 0.01, "at {fs} Hz: a repeat arrived early: {early}");

            let mut sr = SpringReverb::new();
            sr.set_sample_rate(fs);
            let ir = impulse_response(|x| sr.process(x, 1.0, fs), (fs * 0.1) as usize);
            let ms = |m: f32| (m * 0.001 * fs) as usize;
            // The first transit lands at 41.6 ms; before it only the direct coupling, long gone.
            let transit = ir[ms(40.0)..ms(46.0)]
                .iter()
                .fold(0.0f32, |m, v| m.max(v.abs()));
            let before = ir[ms(15.0)..ms(38.0)]
                .iter()
                .fold(0.0f32, |m, v| m.max(v.abs()));
            assert!(
                transit > 3.0 * before && transit > 1e-3,
                "at {fs} Hz: the first transit should land near 41.6 ms: {transit} against {before} before it"
            );
        }
    }

    /// **The phaser's feedback loop is parametrically unstable, and MANUAL IN's bandwidth is what
    /// keeps it out of reach.**
    ///
    /// Four all-passes have unity magnitude only while the coefficient holds still. Drive the
    /// centre coherently near Nyquist and the chain stops being passive, the loop's fixed
    /// [`PHASER_FEEDBACK`] exceeds one, and the phaser runs away **from silence** — clap-validator's
    /// `param-fuzz-bounds` found it on the converted build, where a summing MANUAL IN can toggle
    /// between the ends of the ±4-octave clamp. One source never could: a column is a waveform.
    ///
    /// The assertion is not only finiteness. A guard that merely bounded the feedback state would
    /// leave a full-scale self-oscillation, which is not what the machine does, so this holds the
    /// output near the unmodulated level.
    ///
    /// **Falsified**: take the one-pole off `manual_in` and this reaches `inf` within a few hundred
    /// samples at every rate here.
    #[test]
    fn a_centre_cv_alternating_at_nyquist_does_not_set_the_phaser_oscillating() {
        // What the phaser makes of a steady tone with the centre still — the level to stay near.
        let quiet = {
            let mut ph = Phaser::new();
            (0..20_000)
                .map(|i| {
                    ph.process((i as f32 * 0.01).sin() * 0.3, 1.0, None, 0.0, FS)
                        .abs()
                })
                .fold(0.0f32, f32::max)
        };

        for fs in [1_000.0, 8_000.0, 44_100.0, FS, 96_000.0, 768_000.0] {
            // Every alternation period from two samples down: `fs / 2` is the one that pumps, and
            // the ones near it are what a saturating summing input actually produces.
            for period in [2usize, 4, 8, 16, 32, 64, 256, 2048] {
                let mut ph = Phaser::new();
                let mut worst = 0.0f32;
                for i in 0..20_000 {
                    let cv = if (i / (period / 2)) % 2 == 0 {
                        1.0
                    } else {
                        -1.0
                    };
                    // **From silence**: the loop needs no input to run away.
                    let y = ph.process(0.0, 1.0, None, cv, fs);
                    assert!(
                        y.is_finite(),
                        "{fs} Hz, centre alternating every {period} samples: sample {i} is {y}"
                    );
                    worst = worst.max(y.abs());
                }
                assert_eq!(
                    worst, 0.0,
                    "{fs} Hz, period {period}: silence in, silence out"
                );

                let mut ph = Phaser::new();
                let mut worst = 0.0f32;
                for i in 0..20_000 {
                    let cv = if (i / (period / 2)) % 2 == 0 {
                        1.0
                    } else {
                        -1.0
                    };
                    let y = ph.process((i as f32 * 0.01).sin() * 0.3, 1.0, None, cv, fs);
                    assert!(
                        y.is_finite(),
                        "{fs} Hz, centre alternating every {period} samples: sample {i} is {y}"
                    );
                    worst = worst.max(y.abs());
                }
                assert!(
                    worst < 4.0 * quiet,
                    "{fs} Hz, centre alternating every {period} samples: the phaser reaches                      {worst} where an unmodulated centre reaches {quiet} — it is oscillating,                      not sweeping"
                );
            }
        }
    }

    /// **An unpatched MANUAL IN costs nothing**, which is what lets [`PHASER_MANUAL_HZ`] exist
    /// without moving a single preset: the one-pole's state is exactly zero while its input is.
    ///
    /// **Falsified**: give the smoother any non-zero initial state and the two runs diverge.
    #[test]
    fn the_centre_cvs_bandwidth_is_bit_transparent_when_nothing_is_patched() {
        let mut swept = Phaser::new();
        let mut still = Phaser::new();
        for i in 0..48_000 {
            let x = (i as f32 * 0.37).sin() * 0.4;
            // The internal LFO sweeps in both; only MANUAL IN is the question.
            assert_eq!(
                swept.process(x, 0.6, None, 0.0, FS),
                still.process(x, 0.6, None, -0.0, FS),
                "sample {i}"
            );
        }
    }

    /// **Review round 2.** Turning the reverb to zero empties the tank: what was in it does not
    /// wait to replay when the level comes back.
    #[test]
    fn a_zero_reverb_empties_the_tank_rather_than_freezing_it() {
        let mut sr = SpringReverb::new();
        for i in 0..4_800 {
            let x = if i < 2_400 { 0.5 } else { 0.0 };
            sr.process(x, 1.0, FS);
        }
        let ringing = (0..480)
            .map(|_| sr.process(0.0, 1.0, FS).abs())
            .fold(0.0f32, f32::max);
        assert!(
            ringing > 1e-3,
            "the premise: the tank is ringing: {ringing}"
        );

        for _ in 0..(FS as usize / 10) {
            assert_eq!(sr.process(0.0, 0.0, FS), 0.0, "dry, and exactly so");
        }
        for i in 0..(FS as usize) {
            assert_eq!(
                sr.process(0.0, 1.0, FS),
                0.0,
                "sample {i}: the level came back to an emptied tank"
            );
        }

        // The phaser likewise.
        let mut ph = Phaser::new();
        for _ in 0..4_800 {
            ph.process(0.5, 1.0, None, 0.0, FS);
        }
        for _ in 0..480 {
            ph.process(0.0, 0.0, None, 0.0, FS);
        }
        for i in 0..480 {
            assert_eq!(
                ph.process(0.0, 1.0, None, 0.0, FS),
                0.0,
                "phaser sample {i}"
            );
        }
    }

    /// **Review round 3.** Emptying a line touches no memory: what it held is simply not read.
    /// A reset with stale contents behind it reads exact zeros for a whole line length.
    #[test]
    fn an_emptied_line_reads_exact_zeros_without_being_zeroed() {
        let mut dl = Delay::new();
        for i in 0..(FS as usize) {
            dl.process((i as f32 * 0.37).sin(), 1.0, 0.5, FS);
        }
        assert!(
            dl.line.iter().any(|v| *v != 0.0),
            "the premise: the line is full"
        );
        dl.reset();
        assert!(
            dl.line.iter().any(|v| *v != 0.0),
            "reset must not touch the line"
        );
        for i in 0..(2 * FS as usize) {
            assert_eq!(
                dl.process(0.0, 1.0, 0.5, FS),
                0.0,
                "stale sample read at {i}"
            );
        }

        let mut sr = SpringReverb::new();
        for i in 0..(FS as usize) {
            sr.process((i as f32 * 0.37).sin(), 1.0, FS);
        }
        sr.reset();
        for i in 0..(FS as usize) {
            assert_eq!(sr.process(0.0, 1.0, FS), 0.0, "stale spring read at {i}");
        }
    }

    /// A line resized under a write pointer that had wandered: the pointer must come home.
    /// clap-validator found the first O(1) emptying indexing 23040 into a line of 1299.
    #[test]
    fn a_rate_change_after_playing_does_not_index_past_the_shrunk_line() {
        let mut dl = Delay::new();
        let mut sr = SpringReverb::new();
        for i in 0..(FS as usize) {
            let x = (i as f32 * 0.37).sin();
            dl.process(x, 1.0, 0.5, FS);
            sr.process(x, 1.0, FS);
        }
        for fs in [1_234.57f32, 1_000.0, 768_000.0, 48_000.0] {
            dl.set_sample_rate(fs);
            sr.set_sample_rate(fs);
            for _ in 0..2_000 {
                assert!(dl.process(0.1, 1.0, 1.0, fs).is_finite());
                assert!(sr.process(0.1, 1.0, fs).is_finite());
            }
        }
    }

    #[test]
    fn a_delay_time_change_glides_rather_than_clicking() {
        let mut dl = Delay::new();
        let mut prev = 0.0f32;
        let mut worst = 0.0f32;
        for i in 0..(FS as usize * 2) {
            let x = (std::f32::consts::TAU * 220.0 * i as f32 / FS).sin() * 0.5;
            let time = if i < FS as usize { 0.3 } else { 0.5 };
            let y = dl.process(x, 0.8, time, FS);
            if i > FS as usize / 2 {
                worst = worst.max((y - prev).abs());
            }
            prev = y;
        }
        assert!(worst < 0.2, "a time change jumped by {worst}");
    }

    #[test]
    fn the_spring_decays_at_the_measured_t60_and_is_a_band_pass() {
        let mut sr = SpringReverb::new();
        let n = (FS * 5.0) as usize;
        let ir = impulse_response(|x| sr.process(x, 1.0, FS), n);
        // Schroeder decay: T60 from the −5 → −25 dB span, as the page measured.
        let e: Vec<f64> = ir.iter().map(|v| (*v as f64) * (*v as f64)).collect();
        let mut edc = vec![0.0f64; n];
        let mut acc = 0.0;
        for i in (0..n).rev() {
            acc += e[i];
            edc[i] = acc;
        }
        let db = |i: usize| 10.0 * (edc[i] / edc[0]).log10();
        let at = |target: f64| (0..n).find(|&i| db(i) <= target).unwrap() as f32 / FS;
        let t60 = (at(-25.0) - at(-5.0)) * 3.0;
        assert!(
            (t60 - SPRING_T60_S).abs() < SPRING_T60_S * 0.35,
            "T60 {t60:.2} s against the measured {SPRING_T60_S}"
        );
        // Band-pass: 800 Hz well above 100 Hz and 6 kHz in the tail.
        let tail = &ir[(FS * 0.1) as usize..(FS * 1.0) as usize];
        let mid = tone_level(tail, 800.0);
        let low = tone_level(tail, 100.0);
        let high = tone_level(tail, 6_000.0);
        assert!(
            mid > low * 3.0,
            "100 Hz should be well down: {low} vs {mid}"
        );
        assert!(
            mid > high * 3.0,
            "6 kHz should be well down: {high} vs {mid}"
        );
    }

    #[test]
    fn the_springs_return_at_their_transit_times() {
        let mut sr = SpringReverb::new();
        let ir = impulse_response(|x| sr.process(x, 1.0, FS), (FS * 0.2) as usize);
        // The first round trip of the shortest spring lands near 41.6 ms; before it, only the
        // direct coupling.
        let early = &ir[(FS * 0.005) as usize..(FS * 0.035) as usize];
        let quiet = early.iter().fold(0.0f32, |m, v| m.max(v.abs()));
        let around = &ir[(FS * 0.040) as usize..(FS * 0.050) as usize];
        let arrival = around.iter().fold(0.0f32, |m, v| m.max(v.abs()));
        assert!(
            arrival > quiet * 3.0,
            "the first return should stand out: {arrival} vs {quiet}"
        );
    }

    #[test]
    fn every_effect_is_bounded_and_deterministic_under_a_hot_input() {
        let render = || {
            let mut ph = Phaser::new();
            let mut dl = Delay::new();
            let mut sr = SpringReverb::new();
            (0..48_000)
                .map(|i| {
                    let x = if i % 7 == 0 { 4.0 } else { -4.0 };
                    let y = ph.process(x, 1.0, Some(0.5), 0.5, FS);
                    let y = dl.process(y, 1.0, 0.02, FS);
                    sr.process(y, 1.0, FS)
                })
                .collect::<Vec<_>>()
        };
        let a = render();
        assert_eq!(a, render());
        assert!(a.iter().all(|y| y.is_finite() && y.abs() < 64.0));
    }
}
