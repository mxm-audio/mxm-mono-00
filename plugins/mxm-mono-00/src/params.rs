//! Parameter definitions.
//!
//! Every `#[id]` here is **permanent**. Changing one breaks every saved project that used the
//! plugin, so ids are part of the public interface.
//!
//! # The panel is the plug-out's module list, and nothing else
//!
//! `research:instruments/system-100.md` §12.3 is the list: two VCOs with their range switches, waves
//! and manual pulse widths; the mixer's four levels and the noise colour; the HPF and the diode
//! ladder; the VCA with its initial gain and tone; two ADSRs with their three trigger modes; two LFOs
//! with rate, shape and the rate CV input's offset; the sample-and-hold's rate and lag; the three
//! effects; and the top bar's tune, bend range, portamento and note priority. **Every modulation
//! depth is a route** in [`crate::routes`], chosen on the card it moves — the glide and the VCO LFO
//! with their DESTINATION switch, the VCA LFO, both PWM switches and depths, KYBD CV to the filter
//! and SAMPLE MODE included (`plans/plan-mxm-mono-00-modulation.md` Rev 3). **What the plug-out
//! carries and the collection's host already does — the arpeggiator, scatter, key hold, octave
//! shift, the patch bank — is not here**, by the root's rule.
//!
//! # The patch bay is 433 pairs, and it lives in [`crate::routes`]
//!
//! A route is a *(target, source)* pair carrying a presence and a signed amount, nested here as
//! `routes`. Both halves are automatable and per-step modulatable like any other parameter, which
//! is what makes a sequencer able to re-patch the machine — and it is why the routing is not
//! persisted state. There is no source selector: a target has one slot per source.
//!
//! # Two collection contracts
//!
//! **Every amount starts at zero**: every modulation depth, every effect level, VCO-2's level and
//! the noise's. VCO-1's level starts *up*, because a level is not an amount and a fresh patch that
//! made no sound would be an instrument that reads as broken — and so do the four routes whose
//! depth was never a control before the conversion, which `plugins/mxm-mono-00/AGENTS.md` records
//! as this instrument's init deviations. VCO-2's fine tune starts at +7 cents — the init contract's
//! *slightly detuned*, recorded as chosen in the DSP crate.
//!
//! **Smooth signals, not coefficients.** Levels, amounts, the cutoffs, the pulse widths, the tune
//! and the delay time are smoothed, **and so is every route amount**, because it multiplies a
//! signal. Envelope times, rates, switches, a route's *presence* and the trigger modes are not: a
//! constant is not a signal, and topology ramped between two states would be neither.

use mxm_mono_00_dsp::lfo::Shape;
use mxm_mono_00_dsp::noise::Colour;
use mxm_mono_00_dsp::oscillator::{SyncStrength, Wave};
use mxm_mono_00_dsp::routing::TARGETS;
use mxm_mono_00_dsp::voice::{Priority, Trigger};
use mxm_tempo::{Direction, Division, Ladder, Span};
use nice_plug::prelude::*;
use std::sync::{Arc, RwLock};

/// The delay time's tempo sync (`plans/plan-tempo-sync-controls.md`): 1/32 to a half note, the top
/// the longest — **the ends its own eight-step table had** (the owner: *"if they have different
/// ranges of time we keep to that"*), so a stored position at either end keeps its division. The
/// shared ladder holds the dotted divisions that table skipped, so a position between the ends can
/// land on a neighbouring division — accepted by the owner, 2026-09-25 (`NOTES.md`).
pub const DELAY_SYNC: Ladder = Ladder::new(
    Span::new(Division::ThirtySecond, Division::Half),
    Direction::Time,
);
/// The sample clock's: the same span, the same table's ends, the top the fastest.
pub const SH_SYNC: Ladder = Ladder::new(
    Span::new(Division::ThirtySecond, Division::Half),
    Direction::Rate,
);
/// Both LFOs': every LFO's ladder, the top the fastest.
pub const LFO_SYNC: Ladder = Ladder::new(Span::LFO, Direction::Rate);

// ---------------------------------------------------------------------------------------------
// The switches, each a plain mapping onto the DSP crate's own type.

#[derive(Enum, Debug, Clone, Copy, PartialEq, Eq)]
pub enum PriorityKind {
    #[id = "low"]
    #[name = "Low"]
    Low,
    #[id = "last"]
    #[name = "Last"]
    Last,
}

impl From<PriorityKind> for Priority {
    fn from(k: PriorityKind) -> Self {
        match k {
            PriorityKind::Low => Priority::Low,
            PriorityKind::Last => Priority::Last,
        }
    }
}

/// The plug-out's octave RANGE switch, 64' to 2', in place of the hardware's frequency knob.
#[derive(Enum, Debug, Clone, Copy, PartialEq, Eq)]
pub enum Range {
    #[id = "64"]
    #[name = "64'"]
    Ft64,
    #[id = "32"]
    #[name = "32'"]
    Ft32,
    #[id = "16"]
    #[name = "16'"]
    Ft16,
    #[id = "8"]
    #[name = "8'"]
    Ft8,
    #[id = "4"]
    #[name = "4'"]
    Ft4,
    #[id = "2"]
    #[name = "2'"]
    Ft2,
}

impl Range {
    /// The offset from concert pitch, 8' being the MIDI note as sent.
    pub const fn semitones(self) -> f32 {
        match self {
            Range::Ft64 => -36.0,
            Range::Ft32 => -24.0,
            Range::Ft16 => -12.0,
            Range::Ft8 => 0.0,
            Range::Ft4 => 12.0,
            Range::Ft2 => 24.0,
        }
    }
}

#[derive(Enum, Debug, Clone, Copy, PartialEq, Eq)]
pub enum WaveKind {
    #[id = "saw"]
    #[name = "Sawtooth"]
    Saw,
    #[id = "square"]
    #[name = "Square"]
    Square,
    #[id = "triangle"]
    #[name = "Triangle"]
    Triangle,
}

impl From<WaveKind> for Wave {
    fn from(w: WaveKind) -> Self {
        match w {
            WaveKind::Saw => Wave::Saw,
            WaveKind::Square => Wave::Square,
            WaveKind::Triangle => Wave::Triangle,
        }
    }
}

#[derive(Enum, Debug, Clone, Copy, PartialEq, Eq)]
pub enum SyncKind {
    #[id = "strong"]
    #[name = "Strong"]
    Strong,
    #[id = "weak"]
    #[name = "Weak"]
    Weak,
}

impl From<SyncKind> for SyncStrength {
    fn from(k: SyncKind) -> Self {
        match k {
            SyncKind::Strong => SyncStrength::Strong,
            SyncKind::Weak => SyncStrength::Weak,
        }
    }
}

#[derive(Enum, Debug, Clone, Copy, PartialEq, Eq)]
pub enum NoiseKind {
    #[id = "white"]
    #[name = "White"]
    White,
    #[id = "pink"]
    #[name = "Pink"]
    Pink,
}

impl From<NoiseKind> for Colour {
    fn from(k: NoiseKind) -> Self {
        match k {
            NoiseKind::White => Colour::White,
            NoiseKind::Pink => Colour::Pink,
        }
    }
}

#[derive(Enum, Debug, Clone, Copy, PartialEq, Eq)]
pub enum TriggerKind {
    #[id = "gate"]
    #[name = "Gate"]
    Gate,
    #[id = "lfo"]
    #[name = "LFO 1"]
    Lfo,
    #[id = "gatetrig"]
    #[name = "Gate+Trig"]
    GateTrig,
}

impl From<TriggerKind> for Trigger {
    fn from(k: TriggerKind) -> Self {
        match k {
            TriggerKind::Gate => Trigger::Gate,
            TriggerKind::Lfo => Trigger::Lfo,
            TriggerKind::GateTrig => Trigger::GateTrig,
        }
    }
}

#[derive(Enum, Debug, Clone, Copy, PartialEq, Eq)]
pub enum ShapeKind {
    #[id = "sine"]
    #[name = "Sine"]
    Sine,
    #[id = "triangle"]
    #[name = "Triangle"]
    Triangle,
    #[id = "saw"]
    #[name = "Sawtooth"]
    Saw,
    #[id = "square"]
    #[name = "Square"]
    Square,
    #[id = "sh"]
    #[name = "S&H"]
    SampleHold,
}

impl From<ShapeKind> for Shape {
    fn from(k: ShapeKind) -> Self {
        match k {
            ShapeKind::Sine => Shape::Sine,
            ShapeKind::Triangle => Shape::Triangle,
            ShapeKind::Saw => Shape::Saw,
            ShapeKind::Square => Shape::Square,
            ShapeKind::SampleHold => Shape::SampleHold,
        }
    }
}

// ---------------------------------------------------------------------------------------------
// Formatting and the builders the panel shares.

/// Formats a parameter value for display.
type ValueToString = Arc<dyn Fn(f32) -> String + Send + Sync>;
/// Parses a typed-in value, returning `None` if it cannot be understood.
type StringToValue = Arc<dyn Fn(&str) -> Option<f32> + Send + Sync>;

/// Format seconds as milliseconds below a second, seconds above.
///
/// **The unit is chosen from what the millisecond text would round to, not from the raw value.**
/// A host parses the text and normalises it before printing it again, so the value it prints lands a
/// hair either side of where it started: switching at the raw second printed `0.99996 s` as
/// `1000.0 ms`, which parses to one second and prints `1.00 s`, and `clap-validator`'s
/// `param-conversions` fails whenever its values land there. Anything that would print `1000.0 ms`
/// prints seconds instead.
fn v2s_time() -> ValueToString {
    Arc::new(|s| {
        if (s * 10_000.0).round() >= 10_000.0 {
            format!("{s:.2} s")
        } else {
            format!("{:.1} ms", s * 1000.0)
        }
    })
}

fn s2v_time() -> StringToValue {
    Arc::new(|text| {
        let t = text.trim().to_lowercase();
        let (number, scale) = if let Some(rest) = t.strip_suffix("ms") {
            (rest, 0.001)
        } else if let Some(rest) = t.strip_suffix('s') {
            (rest, 1.0)
        } else {
            (t.as_str(), 0.001)
        };
        number.trim().parse::<f32>().ok().map(|v| v * scale)
    })
}

/// Show a `0..=1` control as a percentage — **never a negative zero**.
///
/// Key follow and Tone cross zero, and `format!("{:.0}", -0.4)` prints `-0`, which a host parses to
/// zero and prints `0 %`: not idempotent through its conversion. Only a text that reads zero
/// changes.
fn v2s_percent() -> ValueToString {
    Arc::new(|v| {
        let text = format!("{:.0}", v * 100.0);
        match text.strip_prefix('-') {
            Some(digits) if digits.bytes().all(|b| b == b'0') => format!("{digits} %"),
            _ => format!("{text} %"),
        }
    })
}

fn s2v_percent() -> StringToValue {
    Arc::new(|text| {
        text.trim()
            .trim_end_matches('%')
            .trim()
            .parse::<f32>()
            .ok()
            .map(|v| v / 100.0)
    })
}

/// How fast a level or a depth follows its control, in milliseconds.
const AMOUNT_SMOOTHING_MS: f32 = 10.0;

/// An amount or a level: `0..=1`, shown as a percentage, smoothed.
fn amount(name: &'static str, default: f32) -> FloatParam {
    FloatParam::new(name, default, FloatRange::Linear { min: 0.0, max: 1.0 })
        .with_smoother(SmoothingStyle::Linear(AMOUNT_SMOOTHING_MS))
        .with_value_to_string(v2s_percent())
        .with_string_to_value(s2v_percent())
}

/// A time: skewed toward the short end, **unsmoothed** — it sets a state machine's timing.
fn seconds(name: &'static str, default: f32, min: f32, max: f32) -> FloatParam {
    FloatParam::new(
        name,
        default,
        FloatRange::Skewed {
            min,
            max,
            factor: FloatRange::skew_factor(-2.0),
        },
    )
    .with_value_to_string(v2s_time())
    .with_string_to_value(s2v_time())
}

/// A rate or a corner in hertz, logarithmic across its range.
fn hertz(name: &'static str, default: f32, min: f32, max: f32) -> FloatParam {
    FloatParam::new(
        name,
        default,
        FloatRange::Skewed {
            min,
            max,
            factor: FloatRange::skew_factor(-2.0),
        },
    )
    .with_value_to_string(v2s_hertz())
    .with_string_to_value(formatters::s2v_f32_hz_then_khz())
}

/// Hertz to a tenth below a kilohertz, kilohertz to a tenth above — nice-plug's
/// `v2s_f32_hz_then_khz(1)` with **the unit chosen from what the hertz text would round to**.
///
/// That formatter switches at the raw 1000 Hz, so `999.96 Hz` printed `1000.0 Hz`, which parses to
/// 1000 and — normalised and back — prints `1.0 kHz`, and a `1.0 kHz` could come back as
/// `1000.0 Hz`. `mxm-mono-pr1` met it on its cutoff first. The parser is still nice-plug's.
fn v2s_hertz() -> ValueToString {
    Arc::new(|hz| {
        if (hz * 10.0).round() >= 10_000.0 {
            format!("{:.1} kHz", hz / 1_000.0)
        } else {
            format!("{hz:.1} Hz")
        }
    })
}

/// A pitch offset in cents, centred.
fn cents(name: &'static str, default: f32, reach: f32) -> FloatParam {
    FloatParam::new(
        name,
        default,
        FloatRange::Linear {
            min: -reach,
            max: reach,
        },
    )
    .with_smoother(SmoothingStyle::Linear(20.0))
    .with_unit(" cents")
    .with_value_to_string(formatters::v2s_f32_rounded(1))
}

/// The manual pulse width: 50 % is square, and the hardware narrows from there and never widens.
fn pulse_width(name: &'static str) -> FloatParam {
    FloatParam::new(
        name,
        0.5,
        FloatRange::Linear {
            min: PULSE_WIDTH_MIN,
            max: 0.5,
        },
    )
    .with_smoother(SmoothingStyle::Linear(AMOUNT_SMOOTHING_MS))
    .with_value_to_string(v2s_percent())
    .with_string_to_value(s2v_percent())
}

/// The narrowest manual pulse width — the DSP crate's own clamp.
pub const PULSE_WIDTH_MIN: f32 = 0.05;

/// The envelopes' time range. The floor is the machine's 0.4 ms (`envelope::MIN_TIME_S`); the
/// ceiling is chosen.
pub const ENVELOPE_MIN_S: f32 = 0.0004;
pub const ENVELOPE_MAX_S: f32 = 10.0;

/// The portamento pot's reach. **Chosen**: the hardware's is not documented.
pub const PORTAMENTO_MAX_S: f32 = 5.0;

/// VCO-2's coarse tune, in semitones either side.
pub const COARSE_REACH: i32 = 24;

/// How far the LFO rate rows' OFFSET reaches, in cents of rate (1200 is an octave).
pub const LFO_OFFSET_REACH_CENTS: f32 = 2400.0;

/// The delay's time range — the DSP crate's `DELAY_MIN_S` and `DELAY_MAX_S`.
pub const DELAY_MIN_S: f32 = 0.02;
pub const DELAY_MAX_S: f32 = 1.0;

#[derive(Params)]
pub struct MxmMono00Params {
    // ---- Voice: the top bar and the keyboard section ----
    #[id = "priority"]
    pub priority: EnumParam<PriorityKind>,
    #[id = "tune"]
    pub tune: FloatParam,
    #[id = "bendrange"]
    pub bend_range: FloatParam,
    #[id = "portamento"]
    pub portamento: FloatParam,

    // ---- VCO-1 ----
    #[id = "range1"]
    pub range1: EnumParam<Range>,
    /// VCO-1's coarse and fine tune: the 101's own FREQUENCY and FINE TUNING, which the plug-out
    /// dropped (`research:instruments/system-100.md` §3.1, §12.3).
    #[id = "coarse1"]
    pub coarse1: IntParam,
    #[id = "fine1"]
    pub fine1: FloatParam,
    #[id = "wave1"]
    pub wave1: EnumParam<WaveKind>,
    #[id = "pw1"]
    pub pw1: FloatParam,

    // ---- VCO-2 ----
    #[id = "range2"]
    pub range2: EnumParam<Range>,
    #[id = "coarse2"]
    pub coarse2: IntParam,
    #[id = "fine2"]
    pub fine2: FloatParam,
    #[id = "wave2"]
    pub wave2: EnumParam<WaveKind>,
    #[id = "pw2"]
    pub pw2: FloatParam,
    #[id = "syncstrength"]
    pub sync_strength: EnumParam<SyncKind>,

    // ---- Mixer ----
    #[id = "vco1level"]
    pub vco1_level: FloatParam,
    #[id = "vco2level"]
    pub vco2_level: FloatParam,
    #[id = "noiselevel"]
    pub noise_level: FloatParam,
    /// The ring modulator's own mixer level, the 102's RING MOD slider. **Not `ringlevel`**, which
    /// retired with the patch bay's conversion and may not come back with a new meaning.
    #[id = "ringmodlevel"]
    pub ring_level: FloatParam,
    #[id = "noisecolour"]
    pub noise_colour: EnumParam<NoiseKind>,

    // ---- Filter ----
    #[id = "hpf"]
    pub hpf: FloatParam,
    #[id = "cutoff"]
    pub cutoff: FloatParam,
    #[id = "resonance"]
    pub resonance: FloatParam,

    // ---- Amplifier ----
    #[id = "initialgain"]
    pub initial_gain: FloatParam,
    #[id = "tone"]
    pub tone: FloatParam,

    // ---- Envelope (VCF) ----
    #[id = "vcfattack"]
    pub vcf_attack: FloatParam,
    #[id = "vcfdecay"]
    pub vcf_decay: FloatParam,
    #[id = "vcfsustain"]
    pub vcf_sustain: FloatParam,
    #[id = "vcfrelease"]
    pub vcf_release: FloatParam,
    #[id = "vcftrigger"]
    pub vcf_trigger: EnumParam<TriggerKind>,

    // ---- Envelope (VCA) ----
    #[id = "vcaattack"]
    pub vca_attack: FloatParam,
    #[id = "vcadecay"]
    pub vca_decay: FloatParam,
    #[id = "vcasustain"]
    pub vca_sustain: FloatParam,
    #[id = "vcarelease"]
    pub vca_release: FloatParam,
    #[id = "vcatrigger"]
    pub vca_trigger: EnumParam<TriggerKind>,

    // ---- LFO 1 ----
    #[id = "lfo1rate"]
    pub lfo1_rate: FloatParam,
    #[id = "lfo1shape"]
    pub lfo1_shape: EnumParam<ShapeKind>,
    #[id = "lfo1offset"]
    pub lfo1_offset: FloatParam,
    /// Tempo sync for LFO 1's rate (the owner, 2026-09-25): its position picks a division of the
    /// host's tempo ([`LFO_SYNC`]); the rate CV and the offset move it from there.
    #[id = "lfo1sync"]
    pub lfo1_sync: BoolParam,

    // ---- LFO 2 ----
    #[id = "lfo2rate"]
    pub lfo2_rate: FloatParam,
    #[id = "lfo2shape"]
    pub lfo2_shape: EnumParam<ShapeKind>,
    #[id = "lfo2offset"]
    pub lfo2_offset: FloatParam,
    /// Tempo sync for LFO 2's rate, as LFO 1's.
    #[id = "lfo2sync"]
    pub lfo2_sync: BoolParam,

    // ---- Sample-and-hold ----
    #[id = "shrate"]
    pub sh_rate: FloatParam,
    #[id = "shlag"]
    pub sh_lag: FloatParam,
    /// Tempo sync for the sample clock (the owner, 2026-09-25): Rate's position picks a note
    /// division of the host's tempo, as the delay's TEMPO SYNC picks its time. Inert with no tempo.
    #[id = "shsync"]
    pub sh_sync: BoolParam,

    // ---- FX, in processing order ----
    #[id = "phaser"]
    pub phaser: FloatParam,
    #[id = "delay"]
    pub delay: FloatParam,
    #[id = "delaytime"]
    pub delay_time: FloatParam,
    /// The plug-out's TEMPO SYNC: the delay time becomes a note division of the host's tempo.
    /// With no tempo arriving it is inert and the time slider is used as it stands.
    #[id = "temposync"]
    pub tempo_sync: BoolParam,
    #[id = "reverb"]
    pub reverb: FloatParam,

    // ---- Output ----
    #[id = "volume"]
    pub volume: FloatParam,

    /// **The patch bay and the panel's wiring**: one presence and one amount per *(target, source)*
    /// pair, 433 of them — 450 less the seventeen the modulation standard refuses — grouped one
    /// `#[nested]` per target. `plan-modulation-routing.md` decision 1.6 — an
    /// input takes any number of sources, each at its own signed depth, which is the switching
    /// jack becoming a mixer.
    ///
    /// The plug-out's own connections are the **defaults** here rather than wiring in the voice,
    /// which is what makes every one of them something a player can pull out.
    #[nested(group = "Modulation")]
    pub routes: crate::routes::Routes,

    /// Which preset is loaded, and what it looked like when it was.
    ///
    /// **Persisted with the patch, not beside it.** nice-plug carries non-parameter state through
    /// the `Params` derive's `#[persist]`, so it belongs here rather than as a field on the plugin
    /// struct — and it has its own version number, because nice-plug's state version is
    /// `Plugin::VERSION`, which moves for unrelated reasons.
    #[persist = "preset"]
    pub preset: RwLock<mxm_preset::PresetIdentity>,
}

impl Default for MxmMono00Params {
    /// The init patch, which is also the set of CLAP `default_value`s: the plug-out's module
    /// defaults with every amount at zero, as `crates/mxm-mono-00-dsp`'s `Patch::default` has them.
    fn default() -> Self {
        Self {
            priority: EnumParam::new("Note priority", PriorityKind::Low),
            tune: cents("Tune", 0.0, 100.0),
            bend_range: FloatParam::new(
                "Bend range",
                2.0,
                FloatRange::Linear {
                    min: 0.0,
                    max: 24.0,
                },
            )
            .with_smoother(SmoothingStyle::Linear(20.0))
            .with_unit(" st")
            .with_value_to_string(formatters::v2s_f32_rounded(0)),
            portamento: seconds("Portamento", 0.0, 0.0, PORTAMENTO_MAX_S),

            range1: EnumParam::new("Range 1", Range::Ft8),
            coarse1: IntParam::new(
                "Coarse 1",
                0,
                IntRange::Linear {
                    min: -COARSE_REACH,
                    max: COARSE_REACH,
                },
            )
            .with_unit(" st"),
            fine1: cents("Fine 1", 0.0, 100.0),
            wave1: EnumParam::new("Wave 1", WaveKind::Saw),
            pw1: pulse_width("Pulse width 1"),

            range2: EnumParam::new("Range 2", Range::Ft8),
            coarse2: IntParam::new(
                "Coarse 2",
                0,
                IntRange::Linear {
                    min: -COARSE_REACH,
                    max: COARSE_REACH,
                },
            )
            .with_unit(" st"),
            fine2: cents("Fine 2", 7.0, 100.0),
            wave2: EnumParam::new("Wave 2", WaveKind::Saw),
            pw2: pulse_width("Pulse width 2"),
            sync_strength: EnumParam::new("Sync strength", SyncKind::Strong),

            vco1_level: amount("Oscillator 1 level", 0.8),
            vco2_level: amount("Oscillator 2 level", 0.0),
            noise_level: amount("Noise level", 0.0),
            ring_level: amount("Ring mod level", 0.0),
            noise_colour: EnumParam::new("Noise colour", NoiseKind::White),

            hpf: hertz(
                "HPF",
                mxm_mono_00_dsp::hpf::CUTOFF_MIN_HZ,
                mxm_mono_00_dsp::hpf::CUTOFF_MIN_HZ,
                mxm_mono_00_dsp::hpf::CUTOFF_MAX_HZ,
            )
            .with_smoother(SmoothingStyle::Logarithmic(20.0)),
            cutoff: hertz(
                "Cutoff",
                10_000.0,
                mxm_mono_00_dsp::filter::CUTOFF_MIN_HZ,
                20_000.0,
            )
            .with_smoother(SmoothingStyle::Logarithmic(20.0)),
            resonance: amount("Resonance", 0.0),
            // **Bipolar**: the plug-out's VCF KYBD CV is, where the hardware's slider was
            // positive-only (`system-100.md` §12.3), and the plug-out decides what exists. The
            // first build made it an amount and half the control was unreachable.
            initial_gain: amount("Initial gain", 0.0),
            tone: FloatParam::new(
                "Tone",
                0.0,
                FloatRange::Linear {
                    min: -1.0,
                    max: 1.0,
                },
            )
            .with_smoother(SmoothingStyle::Linear(AMOUNT_SMOOTHING_MS))
            .with_value_to_string(v2s_percent())
            .with_string_to_value(s2v_percent()),

            vcf_attack: seconds("Envelope 1 attack", 0.005, ENVELOPE_MIN_S, ENVELOPE_MAX_S),
            vcf_decay: seconds("Envelope 1 decay", 0.3, ENVELOPE_MIN_S, ENVELOPE_MAX_S),
            vcf_sustain: amount("Envelope 1 sustain", 0.7),
            vcf_release: seconds("Envelope 1 release", 0.2, ENVELOPE_MIN_S, ENVELOPE_MAX_S),
            vcf_trigger: EnumParam::new("Envelope 1 trigger", TriggerKind::Gate),

            vca_attack: seconds("Envelope 2 attack", 0.005, ENVELOPE_MIN_S, ENVELOPE_MAX_S),
            vca_decay: seconds("Envelope 2 decay", 0.3, ENVELOPE_MIN_S, ENVELOPE_MAX_S),
            vca_sustain: amount("Envelope 2 sustain", 0.7),
            vca_release: seconds("Envelope 2 release", 0.2, ENVELOPE_MIN_S, ENVELOPE_MAX_S),
            vca_trigger: EnumParam::new("Envelope 2 trigger", TriggerKind::Gate),

            lfo1_rate: hertz(
                "LFO 1 rate",
                4.0,
                mxm_mono_00_dsp::lfo::RATE_MIN_HZ,
                mxm_mono_00_dsp::lfo::RATE_MAX_HZ,
            ),
            lfo1_shape: EnumParam::new("LFO 1 shape", ShapeKind::Sine),
            lfo1_offset: cents("LFO 1 offset", 0.0, LFO_OFFSET_REACH_CENTS),
            lfo1_sync: BoolParam::new("LFO 1 sync", false),

            lfo2_rate: hertz(
                "LFO 2 rate",
                1.0,
                mxm_mono_00_dsp::lfo::RATE_MIN_HZ,
                mxm_mono_00_dsp::lfo::RATE_MAX_HZ,
            ),
            lfo2_shape: EnumParam::new("LFO 2 shape", ShapeKind::Sine),
            lfo2_offset: cents("LFO 2 offset", 0.0, LFO_OFFSET_REACH_CENTS),
            lfo2_sync: BoolParam::new("LFO 2 sync", false),

            sh_rate: hertz(
                "S&H rate",
                4.0,
                mxm_mono_00_dsp::sh::CLOCK_MIN_HZ,
                mxm_mono_00_dsp::sh::CLOCK_MAX_HZ,
            ),
            sh_lag: seconds("S&H lag", 0.0, 0.0, mxm_mono_00_dsp::sh::LAG_MAX_S),
            sh_sync: BoolParam::new("S&H sync", false),

            phaser: amount("Phaser", 0.0),
            delay: amount("Delay", 0.0),
            delay_time: seconds("Delay time", 0.375, DELAY_MIN_S, DELAY_MAX_S)
                // The one time that is smoothed: the line interpolates, so a moving time is a
                // pitch-bending repeat rather than a click, and a control that jumps would be one.
                .with_smoother(SmoothingStyle::Linear(50.0)),
            tempo_sync: BoolParam::new("Delay sync", false),
            reverb: amount("Reverb", 0.0),

            volume: FloatParam::new(
                "Volume",
                util::db_to_gain(-6.0),
                FloatRange::Skewed {
                    min: util::db_to_gain(-60.0),
                    max: util::db_to_gain(0.0),
                    factor: FloatRange::gain_skew_factor(-60.0, 0.0),
                },
            )
            .with_smoother(SmoothingStyle::Logarithmic(20.0))
            .with_unit(" dB")
            .with_value_to_string(formatters::v2s_f32_gain_to_db(1))
            .with_string_to_value(formatters::s2v_f32_gain_to_db()),

            routes: crate::routes::Routes::new(),
            preset: RwLock::new(mxm_preset::PresetIdentity::none()),
        }
    }
}

impl MxmMono00Params {
    /// Every target's routes, in declared target order.
    ///
    /// The fifteen `EnumParam<Source>` rows this replaces are gone: a target has one slot per
    /// source, so there is no selector and nothing to letter. See [`crate::routes`].
    pub fn targets(&self) -> [(usize, &crate::routes::TargetRoutes); TARGETS] {
        self.routes.all()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// **Every parameter reads the same after the host's own round trip**: printed with its unit,
    /// parsed, and printed again, it is the same text (mxm-kit's `docs/code-review-notes.md` §6).
    ///
    /// The host never hands a formatter a plain value. The CLAP wrapper's `value_to_text` and
    /// `text_to_value` carry a normalised value in `f64`, scaled by the step count, so a parsed number
    /// goes through the range's normalisation and back before it is printed again — and a formatter
    /// that picks its unit or its precision from the *raw* value flips branch when that trip lands a
    /// hair the other side of the switch: `0.99996 s` printed `1000.0 ms`, which parses to one second
    /// and prints `1.00 s`. `clap-validator`'s `param-conversions` fails only when its values land in
    /// that sliver, so one clean run proves nothing.
    ///
    /// So this walks the whole parameter map, as the wrapper converts, at the validator's grids, at
    /// plain values either side of every branch point this file's formatters have — a second and a
    /// kilohertz, zero where a range crosses it, 0 dB — and at every representable normalised value
    /// near each of those points.
    #[test]
    fn every_parameter_reads_the_same_after_the_hosts_round_trip() {
        let params = MxmMono00Params::default();
        let map = params.param_map();
        // `clap-validator` 0.4.1 spends 4000 conversions across the parameters, 5 to 100 each.
        let installed = 4000usize.div_ceil(map.len()).clamp(5, 100);

        let mut probes: Vec<f32> = vec![
            // Seconds, printed `{:.1} ms` below one second: the millisecond text reaches `1000.0`
            // at 0.99995 s.
            0.9994, 0.9995, 0.9996, 0.999_94, 0.999_949, 0.999_95, 0.999_951, 0.999_96, 0.9999, 1.0,
            1.000_01, 1.004, 1.005, 1.006,
            // Hertz, printed `{:.1} Hz` below one kilohertz: the text reaches `1000.0` at 999.95 Hz.
            999.4, 999.46, 999.49, 999.5, 999.9, 999.94, 999.95, 999.96, 1_000.0, 1_000.04, 1_000.1,
            1_049.9, 1_050.0, 1_050.1,
        ];
        // Either side of zero, where a plain `{:.N}` prints a negative zero.
        probes.push(0.0);
        for decade in [1e-7, 1e-6, 1e-5, 1e-4, 1e-3, 1e-2, 1e-1] {
            for multiple in [1.0, 4.0, 5.0, 6.0] {
                probes.extend([decade * multiple, -decade * multiple]);
            }
        }
        // Either side of 0 dB, for a gain shown in decibels.
        for db in [1e-4, 1e-3, 0.04, 0.05, 0.06] {
            probes.extend([util::db_to_gain(db), util::db_to_gain(-db)]);
        }
        // The branch points themselves, where every nearby normalised value is tried too.
        let edges = [0.0, 0.999_95, 1.0, 999.95, 1_000.0];

        let mut failures = Vec::new();
        for (id, param, _) in &map {
            // SAFETY: `params` owns every parameter these pointers name and outlives the loop; this
            // is the access the wrapper makes.
            unsafe {
                let steps = param.step_count();
                let scale = steps.unwrap_or(1) as f64;
                let mut values: Vec<f64> = (0..=19)
                    .map(|i| scale * f64::from(i) / 19.0)
                    .chain((0..installed).map(|i| scale * i as f64 / (installed - 1) as f64))
                    .collect();
                if steps.is_none() {
                    let (low, high) = (param.preview_plain(0.0), param.preview_plain(1.0));
                    values.extend(
                        probes
                            .iter()
                            .map(|&plain| f64::from(param.preview_normalized(plain))),
                    );
                    for &edge in edges.iter().filter(|&&edge| low <= edge && edge <= high) {
                        let mut up = param.preview_normalized(edge);
                        let mut down = up;
                        for _ in 0..=64 {
                            values.extend([f64::from(up), f64::from(down)]);
                            up = up.next_up().min(1.0);
                            down = down.next_down().max(0.0);
                        }
                    }
                }
                // `ext_params_value_to_text` and `ext_params_text_to_value`, as the wrapper has them.
                let text_of = |value: f64| {
                    param.normalized_value_to_string(value as f32 / scale as f32, true)
                };
                for value in values {
                    let first = text_of(value);
                    let Some(back) = param.string_to_normalized_value(&first) else {
                        failures.push(format!("{id}: {first:?} does not parse"));
                        continue;
                    };
                    let second = text_of(f64::from(back) * scale);
                    if second != first {
                        failures.push(format!("{id}: {first:?} parses and reads {second:?}"));
                    }
                }
            }
        }
        failures.sort();
        failures.dedup();
        assert!(
            failures.is_empty(),
            "{} texts changed through the host's conversion:\n{}",
            failures.len(),
            failures.join("\n")
        );
    }
}
