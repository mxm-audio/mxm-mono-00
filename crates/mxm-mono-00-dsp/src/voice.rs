//! The voice: two VCOs with sync and cross-modulation, the ring modulator, noise, the mixer's
//! saturator, the HPF, the ladder, the VCA, two envelopes with their trigger modes, two LFOs, the
//! sample-and-hold, portamento and glide — and **the matrix**, through which every patchable
//! input reads its source. Phase 1's first three modules; the effects are the fourth.
//!
//! ```text
//!  keys ─► key target ─► PORTAMENTO (hold capacitor) ─► keyboard CV ─┬─► [+ pitch input: EXT CV, glide, VCO LFO] ─► VCO-1 ─┐ width ← PW input
//!  (low / last)                                                        └─► [+ pitch input · coarse · fine] ─► VCO-2 ─────────┤ sync ← SYNC IN, width ← PW input
//!                                                                          RING = VCO-2 × RING MOD IN, NOISE ─────────────┴─► MIXER (+ EXT IN, saturator)
//!                                                                          ─► HPF ─► ×comp ─► VCF (cutoff, LFO IN, ADSR IN) ─► VCA (ADSR IN, tremolo) ─► VOLUME
//!  LFO-1, LFO-2 (rate CV inputs) · S&H (its input) · VCF ADSR, VCA ADSR (GATE IN inputs)
//! ```
//!
//! **The evaluation order is `matrix.rs`'s table**, and this file walks it stage by stage. A row
//! reading a column its stage has not yet produced reads the previous sample: the plan's §5.2
//! unit delay on backward connections, which is what lets VCO-1's EXT CV row default to VCO-2 and
//! the mixer feed its own EXT IN.
//!
//! **The pitch path is the hardware's order** (`research:instruments/system-100.md` §2.1, §6): the
//! keyboard's target goes through the portamento lag first, *that* is the keyboard CV the KYBD CV
//! column carries, and only then — per VCO, through that oscillator's own pitch input — are the
//! glide dip and the LFO added in the pitch summer. The plug-out's DESTINATION switch chose which
//! oscillators; routing a source to one oscillator's pitch input or to both is that choice now.

use crate::effects::{Delay, Phaser, SpringReverb};
use crate::envelope::{Adsr, Stage};
use crate::filter::DiodeLadder;
use crate::hpf::Hpf;
use crate::lfo::{Lfo, Shape};
use crate::matrix::{Column, EdgeRow, GateRow};
use crate::noise::{Colour, Noise};
use crate::oscillator::{SyncStrength, Vco, Wave};
use crate::oversample::Oversampler;
use crate::ring::ring;
use crate::routing::{self, Graph, Routing, source, target};
use crate::sh::SampleHold;
use crate::vca::Vca;
use crate::{flush, tanh_approx};
use mxm_modulation::standard;

/// The retired VCO LFO slider's full scale: one ten-volt LFO unit at full slider was an octave of
/// pitch. **Chosen** — the slider's law was not measured; an octave is what makes a full-depth
/// sawtooth vibrato shift the mean pitch by a tritone, which is audibly wart 1. The slider is a
/// route to an oscillator's pitch input now; this is what its presets translate through.
pub const VCO_LFO_SEMITONES: f32 = 12.0;
/// Full-scale VCF envelope amount, in octaves per envelope unit (an envelope's +6 V peak is 0.6 of
/// a ten-volt unit, so a full envelope is `FILTER_ENV_OCTAVES`). **Chosen.**
pub const FILTER_ENV_OCTAVES: f32 = 7.0;
/// Full-scale VCF LFO amount, in octaves per ten-volt unit. **Chosen.**
pub const FILTER_LFO_OCTAVES: f32 = 4.0;
/// The note keyboard tracking pivots on; the plug-out's tracking is bipolar (manual p. 19).
pub const KEY_TRACK_CENTRE: f32 = 60.0;
/// The glide dip's fixed recovery, the 2.2 µF / 22 kΩ RC (§6.3): about 48 ms, derived.
pub const GLIDE_TAU_S: f32 = 0.048;
/// The retired glide depth slider's reach. **Chosen so that the hardware's semitone was the
/// midpoint** — the brief's calibration point: depth 0.5 was one semitone, as the machine was. The
/// glide is a route to an oscillator's pitch input now, where the hardware's semitone is an amount
/// of `−1/144`; this is what its presets translate through.
pub const GLIDE_MAX_SEMITONES: f32 = 2.0;
/// How much of the ladder's `1/(1+k)` droop the voice puts back **at the ladder's input**, so the
/// bass "declines slightly" (§8.2, manual figure 1-29) rather than collapsing. Outside the filter,
/// per `mxm-mono-03-dsp`'s rule, and **chosen** against a qualitative source.
///
/// Input-side, because that is what a second resonance-pot gang reducing the ladder's input
/// attenuation would do (§13 leaves the mechanism unverified). For the **bass** the arithmetic is
/// linear — the fundamental sits far below the loop's saturation knee — and at the threshold
/// `(1 + c·k)/(1 + k)` leaves about -1.5 dB, the manual's "slight". The **resonant peak** is a
/// different matter: driven this hard, the harmonics near the cutoff carry the feedback into its
/// saturator and the peak compresses, which is the *drive it hard* character `mxm-mono-03-dsp`
/// records and is left as the model's. The test measures the fundamental, not the peak; a first
/// version measured the peak and read the ringing at 2 kHz as 8.8 dB of "gained bass".
pub const BASS_COMPENSATION: f32 = 0.8;
/// The voice's output bound per unit of Volume: the ladder's bound through a tone control that can
/// add [`crate::vca::TONE_RANGE_DB`], a VCA gain that never exceeds 1, and the standard Amplitude's
/// factor after it, which reaches `1 + AMPLITUDE_SUM_BOUND` — double. Measured under
/// `output_is_finite_and_bounded_under_extreme_settings`, the Amplitude at full included.
pub const OUTPUT_BOUND: f32 =
    crate::filter::OUTPUT_BOUND * 2.0 * (1.0 + standard::AMPLITUDE_SUM_BOUND);
/// VCO-1's EXT CV input at full slider: the hardware's 1 V/oct. A column in ten-volt units is ten
/// volts per unit, so a VCO output of ±0.5 units swings VCO-1 by ±60 semitones at full slider —
/// the machine's cross-modulation, which is not subtle (§3.1, §5.1). **The pitch inputs reach
/// further now**, `routing::PITCH_SEMITONES_PER_UNIT`, because the glide and VCO LFO sliders that
/// added beside this one retired into them; presets written at this scale translate by 120/144.
pub const EXT_CV_SEMITONES_PER_UNIT: f32 = 120.0;
/// An LFO's rate CV row at full GAIN: 1 V/oct on the ten-volt unit, ten octaves per unit —
/// Roland's article's "FM on a modulation source" needs the full swing. **Chosen.**
pub const LFO_CV_OCTAVES_PER_UNIT: f32 = 10.0;
/// A VCO's output in ten-volt units, for its column: 10 Vp-p is ±0.5.
pub const VCO_OUT_UNITS: f32 = 0.5;
/// An envelope's peak in ten-volt units: +6 V (§9).
pub const ENV_UNITS: f32 = 0.6;
/// Gate rise and fall, so a GATE-mode amplifier does not click.
pub const GATE_TIME_S: f32 = 0.002;
/// How long the post-VCA chain is allowed to settle after the last envelope goes idle before the
/// voice reports inert. Covers the HPF and the tilt; the effects will add theirs.
pub const POST_TAIL_S: f32 = 0.05;

const MAX_HELD_NOTES: usize = 16;

/// The plug-out's KEY ASGN switch: 1 lowest, 2 later (manual p. 3). The hardware is low-note only.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Priority {
    #[default]
    Low,
    Last,
}

/// The plug-out's ADSR TRIG, three positions (manual p. 21).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Trigger {
    /// The envelope rises on the gate's rising edge, so a legato joint does not retrigger. The
    /// hardware's KYBD GATE position.
    #[default]
    Gate,
    /// While the gate is high, the envelope rises again on every rising edge of LFO-1's square:
    /// the hardware's diode AND (§7.1).
    Lfo,
    /// Every key press retriggers, legato or not. The plug-out's addition. With a gate row
    /// patched to a column, every rising edge of that column is a press.
    GateTrig,
}

/// A held key, matched by the host's id when it gave one.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct NoteId {
    pub voice_id: Option<i32>,
    pub channel: u8,
    pub note: u8,
}

impl NoteId {
    pub fn matches(&self, voice_id: Option<i32>, channel: u8, note: u8) -> bool {
        match (self.voice_id, voice_id) {
            (Some(a), Some(b)) => a == b,
            _ => self.channel == channel && self.note == note,
        }
    }
}

/// Fixed-capacity stack of held notes, newest last — `mxm-mono-01-dsp`'s, with two additions: the
/// **lowest** held note, for the hardware's priority, and **each press's velocity**, kept beside it
/// so the Velocity source can follow the press that is sounding (code review, 2026-09-22).
#[derive(Debug, Clone, Default)]
pub struct NoteStack {
    entries: [Option<NoteId>; MAX_HELD_NOTES],
    velocities: [f32; MAX_HELD_NOTES],
    len: usize,
}

impl NoteStack {
    pub fn clear(&mut self) {
        self.entries = [None; MAX_HELD_NOTES];
        self.velocities = [0.0; MAX_HELD_NOTES];
        self.len = 0;
    }

    pub fn is_empty(&self) -> bool {
        self.len == 0
    }

    pub fn len(&self) -> usize {
        self.len
    }

    pub fn newest(&self) -> Option<NoteId> {
        if self.len == 0 {
            None
        } else {
            self.entries[self.len - 1]
        }
    }

    /// The lowest note held: the 37-key bus's "lowest closed key wins" (§6.1).
    pub fn lowest(&self) -> Option<NoteId> {
        self.entries[..self.len]
            .iter()
            .flatten()
            .copied()
            .min_by_key(|n| n.note)
    }

    pub fn sounding(&self, priority: Priority) -> Option<NoteId> {
        match priority {
            Priority::Low => self.lowest(),
            Priority::Last => self.newest(),
        }
    }

    /// The velocity of the press [`Self::sounding`] names.
    pub fn sounding_velocity(&self, priority: Priority) -> Option<f32> {
        Some(self.velocities[self.sounding_index(priority)?])
    }

    /// Where the press [`Self::sounding`] names sits in the stack — a position, so two presses of
    /// one key with no voice id are still two. Among equal lowest notes it is the one `min_by_key`
    /// picks, the first held.
    fn sounding_index(&self, priority: Priority) -> Option<usize> {
        Some(match priority {
            Priority::Last => self.len.checked_sub(1)?,
            Priority::Low => {
                self.entries[..self.len]
                    .iter()
                    .enumerate()
                    .filter_map(|(i, e)| e.map(|n| (i, n.note)))
                    .min_by_key(|&(_, note)| note)?
                    .0
            }
        })
    }

    pub fn push(&mut self, id: NoteId, velocity: f32) {
        if self.len == MAX_HELD_NOTES {
            // **Full: the oldest press goes, unless it is the lowest** — which low-note priority
            // is sounding, and which must not be stolen by a press that did not take the bus.
            // Then the next oldest, as `mxm-mono-02-dsp`'s keyboard does (code review,
            // 2026-09-22). Under last-note priority the new press sounds either way.
            let victim = match self.sounding_index(Priority::Low) {
                Some(0) => 1,
                _ => 0,
            };
            self.entries.copy_within(victim + 1.., victim);
            self.velocities.copy_within(victim + 1.., victim);
            self.len -= 1;
        }
        self.entries[self.len] = Some(id);
        self.velocities[self.len] = velocity;
        self.len += 1;
    }

    /// Remove the most recent entry matching the note. Returns whether anything was removed.
    pub fn remove(&mut self, voice_id: Option<i32>, channel: u8, note: u8) -> bool {
        for i in (0..self.len).rev() {
            let Some(entry) = self.entries[i] else {
                continue;
            };
            if entry.matches(voice_id, channel, note) {
                for j in i..self.len - 1 {
                    self.entries[j] = self.entries[j + 1];
                    self.velocities[j] = self.velocities[j + 1];
                }
                self.entries[self.len - 1] = None;
                self.len -= 1;
                return true;
            }
        }
        false
    }
}

/// The plan's §5.5 activity state, decided from the configuration.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Activity {
    /// The patch can produce future audible output with no host event.
    Live,
    /// Not live, and finite state remains.
    Tailing,
    /// Not live, and the tail has settled: exact zero out.
    Inert,
}

/// Everything the voice needs for one sample, as plain values. Rebuilt per sample by the plugin
/// from its smoothers. `Copy`, and free of any framework type.
///
/// **The routing grid is deliberately not in here.** It travels beside the patch as a `&Routing`,
/// because this struct is copied every sample and the grid is 1.5 KB of presences and amounts that
/// change on a *parameter event*. Carrying it here measured **9 ns/sample** of memcpy against a
/// 266 ns voice — a third of the whole conversion's cost, for nothing. `mxm-mono-pr1` found the
/// same thing and moved its grid out of `control::Params` for the same reason.
#[derive(Debug, Clone, Copy)]
pub struct Patch {
    // Keyboard and pitch
    pub priority: Priority,
    /// The RANGE switch as semitones: 64' … 2' → -36 … +24. VCO-1's.
    pub range_offset: f32,
    /// VCO-1's coarse and fine tune, in semitones and cents. **The hardware's, not the plug-out's**:
    /// the 101's VCO had a continuous FREQUENCY knob and FINE TUNING (§3.1), and the plug-out left
    /// VCO-1 with only its range switch, which made the ring modulator's carrier the one oscillator
    /// that could not move against it. Zero changes nothing.
    pub coarse_semitones1: f32,
    pub fine_cents1: f32,
    pub master_tune_cents: f32,
    pub bend_semitones: f32,
    pub expression_semitones: f32,
    /// Portamento time, seconds; zero is the only off.
    pub portamento_s: f32,

    // The performance inputs, as sources. **This keyboard has none of them in hardware** —
    // `plan-modulation-routing.md` decision 1.7 puts them on every instrument, and every one of
    // their pairs starts absent, so a fresh instance is still the copy.
    /// The mod wheel, CC 1, as the sounding channel holds it, `0..=1`.
    pub wheel: f32,
    /// Channel pressure, `0..=1`.
    pub pressure: f32,
    /// The bender's own normalised position, `-1..=1` — the **source**. `bend_semitones` is the
    /// hard-wired path to both oscillators and is untouched beside it (§5.2 part 2).
    pub bend_position: f32,

    // VCO-1
    pub wave: Wave,
    /// The manual width. **Only while nothing is routed to [`target::VCO1_WIDTH`]** — wart 4:
    /// with a source on it the pulse narrows from square, as the PWM switch's other positions did.
    pub pulse_width: f32,

    // VCO-2
    pub range_offset2: f32,
    pub coarse_semitones2: f32,
    pub fine_cents2: f32,
    pub wave2: Wave,
    /// The manual width, while nothing is routed to [`target::VCO2_WIDTH`].
    pub pulse_width2: f32,
    /// Strong or weak reset, the plug-out's own switch (wart 17). **Not an on/off**: whether
    /// VCO-2 is synced at all is whether anything is routed to [`target::VCO2_SYNC`].
    pub sync_strength: SyncStrength,

    // Mixer
    pub level_vco1: f32,
    pub level_vco2: f32,
    pub level_noise: f32,
    /// The ring modulator's own mixer level: the 102's RING MOD slider (§3.2). The routable *Mix*
    /// channel beside it is the EXT IN jack.
    pub level_ring: f32,
    pub noise_colour: Colour,

    // HPF, VCF
    pub hpf_hz: f32,
    pub cutoff_hz: f32,
    pub resonance: f32,

    // VCA
    pub initial_gain: f32,
    pub tone: f32,

    // Envelopes
    pub vcf_adsr: [f32; 4],
    pub vcf_trigger: Trigger,
    pub vca_adsr: [f32; 4],
    pub vca_trigger: Trigger,

    // LFO-1, LFO-2: rate, shape, and the rate CV row's OFFSET (manual p. 15). Its GAIN was the
    // row's one attenuator and is now each route's own amount.
    pub lfo1_rate_hz: f32,
    pub lfo1_shape: Shape,
    pub lfo1_offset_cents: f32,
    pub lfo2_rate_hz: f32,
    pub lfo2_shape: Shape,
    pub lfo2_offset_cents: f32,

    // Sample-and-hold
    pub sh_rate_hz: f32,
    pub sh_lag_s: f32,

    // The effects, in the chosen order: phaser, delay, reverb. Each level is an amount.
    pub phaser: f32,
    pub delay_level: f32,
    /// Seconds; the plugin's tempo sync, when it exists, passes a time in seconds.
    pub delay_time_s: f32,
    pub reverb: f32,

    /// Master volume, linear.
    pub volume: f32,
}

impl Default for Patch {
    fn default() -> Self {
        Self {
            priority: Priority::Low,
            range_offset: 0.0,
            coarse_semitones1: 0.0,
            fine_cents1: 0.0,
            master_tune_cents: 0.0,
            bend_semitones: 0.0,
            expression_semitones: 0.0,
            portamento_s: 0.0,
            wheel: 0.0,
            pressure: 0.0,
            bend_position: 0.0,
            wave: Wave::Saw,
            pulse_width: 0.5,
            range_offset2: 0.0,
            coarse_semitones2: 0.0,
            // The init contract's *more than one oscillator starts slightly detuned*: a few cents
            // on VCO-2's fine tune, the only tune VCO-1 does not have. Chosen.
            fine_cents2: 7.0,
            wave2: Wave::Saw,
            pulse_width2: 0.5,
            sync_strength: SyncStrength::Strong,
            level_vco1: 0.8,
            level_vco2: 0.0,
            level_noise: 0.0,
            level_ring: 0.0,
            noise_colour: Colour::White,
            hpf_hz: crate::hpf::CUTOFF_MIN_HZ,
            cutoff_hz: 10_000.0,
            resonance: 0.0,
            initial_gain: 0.0,
            tone: 0.0,
            vcf_adsr: [0.005, 0.3, 0.7, 0.2],
            vcf_trigger: Trigger::Gate,
            vca_adsr: [0.005, 0.3, 0.7, 0.2],
            vca_trigger: Trigger::Gate,
            lfo1_rate_hz: 4.0,
            lfo1_shape: Shape::Sine,
            lfo1_offset_cents: 0.0,
            lfo2_rate_hz: 1.0,
            lfo2_shape: Shape::Sine,
            lfo2_offset_cents: 0.0,
            sh_rate_hz: 4.0,
            sh_lag_s: 0.0,
            phaser: 0.0,
            delay_level: 0.0,
            delay_time_s: 0.375,
            reverb: 0.0,
            volume: 1.0,
        }
    }
}

#[derive(Debug, Clone)]
pub struct Voice {
    vco1: Vco,
    vco2: Vco,
    lfo1: Lfo,
    lfo2: Lfo,
    sh: SampleHold,
    noise: Noise,
    hpf: Hpf,
    filter: DiodeLadder,
    /// The ladder runs at twice the rate, between this pair — see `oversample.rs`.
    oversampler: Oversampler,
    vca: Vca,
    vcf_env: Adsr,
    vca_env: Adsr,
    phaser: Phaser,
    delay: Delay,
    reverb: SpringReverb,

    /// The routing frame, the compacted route lists and the two audio fades.
    graph: Graph,
    vcf_gate: GateRow,
    vca_gate: GateRow,
    /// **One detector per sync route**, because the sync law cannot sum: each present route
    /// watches its own source and a crossing resets by that route's own depth.
    sync_edges: [EdgeRow; routing::SOURCES],
    /// Which sync routes were live at the last topology pass.
    ///
    /// A detector that is not fed keeps the `prev` it had when it last was, so a route removed and
    /// re-added would compare this sample against one from a previous phrase — a spurious crossing
    /// or a missed one, and *which* depends on how long the route was gone. It is the same defect
    /// `mxm_modulation::SourceFrame::clear` exists for and the same answer: a newly live detector
    /// starts from zero.
    sync_live: [bool; routing::SOURCES],

    stack: NoteStack,
    /// **The sounding press's velocity**, `0..=1`. It moves where the bus does, on a press, a
    /// release or a choke that changes which key sounds, and is kept when the last key goes up.
    /// The Velocity source is not this but [`Voice::envelope_velocity`].
    velocity: f32,
    /// The velocity of **the press that last triggered an envelope** — the Velocity source, by the
    /// modulation standard. Where a press this sample triggers either envelope it is that press's
    /// ([`Voice::pressed_velocity`]) — a GATE+TRIG tie that does not take the bus included; where
    /// something else triggers it (an LFO gate, a clock) with a key held, the sounding press's;
    /// with no key held, unchanged. So a legato press that raises no gate keeps the phrase's. Full
    /// before any press and after All Sound Off, so the source rests at zero.
    envelope_velocity: f32,
    /// The velocity of the latest press, whether or not it took the bus — what a GATE+TRIG
    /// retrigger carries.
    pressed_velocity: f32,
    /// The velocity of **the press that raised the keyboard gate**, the first after silence — what a
    /// GATE or LFO envelope's edge carries, even when later presses land in the same sample.
    edge_velocity: f32,
    /// The keyboard's target, a note number.
    key_target: f32,
    /// The hold capacitor, as the portamento's **remaining distance** from `hold_target` — the
    /// keyboard target it last charged toward. The keyboard CV after portamento, a note number, is
    /// `hold_target + hold_offset`.
    ///
    /// Kept as the distance rather than the CV because the CV form,
    /// `target + (cv − target) × coef`, stops moving in `f32` once a step is under half an ulp of
    /// the target: a long portamento came to rest short of its note — 18 cents at one second and
    /// 48 kHz — until the next key. The distance keeps full precision down to zero.
    hold_offset: f32,
    hold_target: f32,
    /// The glide RC's state, `0..1`, decaying from 1 after each gate edge.
    glide_charge: f32,
    /// Keyboard-gate events waiting for the next sample: a first key down, and any key down.
    pending_gate_edge: bool,
    pending_note_on: bool,
    /// Whether the keyboard gate was high on the previous sample.
    key_was_down: bool,
    /// The note the bus was last pointed at, so a **change of priority while keys are held**
    /// moves the pitch to the note the new priority selects — without a gate edge, as the
    /// hardware's bus would (review round 2).
    bus_note: Option<u8>,
    /// The priority the Velocity source was last chosen under. A switch can move the bus to
    /// another press **of the same note** — two presses of one key under Low and Last — which the
    /// note alone cannot see (code review, 2026-09-22).
    velocity_priority: Option<Priority>,
    gate_smooth: f32,
    /// Samples since the last envelope went idle, for the post-tail settle, and how many the
    /// patch in force needs — the effects' tails included.
    settle_samples: u32,
    settle_needed: u32,

    sample_rate: f32,
    gate_coef: f32,
    glide_coef: f32,
}

/// Which press, if any, made an envelope trigger — for the Velocity source.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum PressTrigger {
    /// Something else did: an LFO, a clock, a gate row's other routes.
    None,
    /// The keyboard gate's own rise, which the first press after silence made.
    Edge,
    /// A GATE+TRIG retrigger, which the latest press made.
    Latest,
}

/// What the keyboard did this sample, as a gate input needs to know it.
#[derive(Debug, Clone, Copy)]
struct KeyEvents {
    /// A key went down this sample.
    note_on: bool,
    /// The first key went down after silence: the keyboard gate's own rising edge.
    key_rose: bool,
}

impl Default for Voice {
    fn default() -> Self {
        Self::new()
    }
}

impl Voice {
    pub fn new() -> Self {
        let mut v = Self {
            vco1: Vco::new(),
            vco2: Vco::new(),
            lfo1: Lfo::new(),
            lfo2: Lfo::new(),
            sh: SampleHold::new(),
            noise: Noise::new(48_000.0),
            hpf: Hpf::new(),
            filter: DiodeLadder::default(),
            oversampler: Oversampler::new(),
            vca: Vca::new(),
            vcf_env: Adsr::new(),
            vca_env: Adsr::new(),
            phaser: Phaser::new(),
            delay: Delay::new(),
            reverb: SpringReverb::new(),
            graph: Graph::new(),
            vcf_gate: GateRow::new(),
            vca_gate: GateRow::new(),
            sync_edges: [EdgeRow::new(); routing::SOURCES],
            sync_live: [false; routing::SOURCES],
            stack: NoteStack::default(),
            velocity: 1.0,
            envelope_velocity: 1.0,
            pressed_velocity: 1.0,
            edge_velocity: 1.0,
            key_target: 60.0,
            hold_offset: 0.0,
            hold_target: 60.0,
            glide_charge: 0.0,
            pending_gate_edge: false,
            pending_note_on: false,
            key_was_down: false,
            bus_note: None,
            velocity_priority: None,
            gate_smooth: 0.0,
            settle_samples: u32::MAX,
            settle_needed: 0,
            sample_rate: 48_000.0,
            gate_coef: 0.0,
            glide_coef: 0.0,
        };
        v.set_sample_rate(48_000.0);
        v
    }

    /// What the ladder's oversampling delays the output by, in samples at the base rate. Stated
    /// here; the plugin deliberately does not report it, for the reason its AGENTS.md gives.
    pub const fn latency_samples() -> u32 {
        crate::oversample::LATENCY_SAMPLES as u32
    }

    pub fn set_sample_rate(&mut self, sample_rate: f32) {
        self.sample_rate = sample_rate;
        self.noise.set_sample_rate(sample_rate);
        self.vcf_env.set_sample_rate(sample_rate);
        self.vca_env.set_sample_rate(sample_rate);
        self.gate_coef = (-1.0 / (GATE_TIME_S * sample_rate)).exp();
        self.glide_coef = (-1.0 / (GLIDE_TAU_S * sample_rate)).exp();
        // The two effects with lines size them here — allocation, so activation only.
        self.delay.set_sample_rate(sample_rate);
        self.reverb.set_sample_rate(sample_rate);
    }

    /// Clear every bit of state. Leaves no tail from the previous playback.
    pub fn reset(&mut self) {
        self.vco1.reset();
        self.vco2.reset();
        self.lfo1.reset();
        self.lfo2.reset();
        self.sh.reset();
        self.noise.reset();
        self.hpf.reset();
        self.filter.reset();
        self.oversampler.reset();
        self.vca.reset();
        self.vcf_env.reset();
        self.vca_env.reset();
        self.phaser.reset();
        self.delay.reset();
        self.reverb.reset();
        self.graph.reset();
        self.vcf_gate.reset();
        self.vca_gate.reset();
        for edge in &mut self.sync_edges {
            edge.reset();
        }
        self.sync_live = [false; routing::SOURCES];
        self.stack.clear();
        self.velocity = 1.0;
        self.envelope_velocity = 1.0;
        self.pressed_velocity = 1.0;
        self.edge_velocity = 1.0;
        self.key_target = 60.0;
        self.hold_offset = 0.0;
        self.hold_target = 60.0;
        self.glide_charge = 0.0;
        self.pending_gate_edge = false;
        self.pending_note_on = false;
        self.key_was_down = false;
        self.bus_note = None;
        self.velocity_priority = None;
        self.gate_smooth = 0.0;
        self.settle_samples = u32::MAX;
        self.settle_needed = 0;
    }

    /// The note the voice is sounding under the patch's priority, for per-note expression routing.
    pub fn sounding(&self, priority: Priority) -> Option<NoteId> {
        self.stack.sounding(priority)
    }

    /// The Velocity source: the sounding press's note-on velocity, `0..=1`.
    pub fn velocity(&self) -> f32 {
        self.velocity
    }

    /// The keyboard CV after portamento, as a note number — the KYBD CV OUT column's source.
    pub fn keyboard_cv(&self) -> f32 {
        self.hold_target + self.hold_offset
    }

    /// The glide RC's charge, `0..1` — the GLIDE source for PWM, and a test hook.
    pub fn glide_charge(&self) -> f32 {
        self.glide_charge
    }

    pub fn vcf_env_level(&self) -> f32 {
        self.vcf_env.level()
    }

    pub fn vca_env_level(&self) -> f32 {
        self.vca_env.level()
    }

    pub fn vca_env_stage(&self) -> Stage {
        self.vca_env.stage()
    }

    /// A source's value on the last sample — for telemetry and for tests.
    ///
    /// **Only a source something reads is published**, which is the routing's main saving, so an
    /// unrouted source reads zero here rather than whatever it would have carried. That is the
    /// honest answer: nothing is computing it.
    pub fn source_value(&self, source: usize) -> f32 {
        self.graph.previous(source)
    }

    /// A column's value on the last sample. See [`Voice::source_value`].
    pub fn column(&self, column: Column) -> f32 {
        self.graph.previous(column.index())
    }

    /// Whether the sampler is on: **any present route into its input at a depth that carries
    /// something.** Nothing contributing is the retired OFF position. Processing and activity both
    /// read this, so a zero-depth route neither starts the sampler nor makes a patch live.
    #[inline]
    fn sampler_is_on(&self, r: &Routing) -> bool {
        self.graph
            .sources(target::SH_INPUT)
            .iter()
            .any(|&s| r.amounts[target::SH_INPUT][s] != 0.0)
    }

    /// Whether a source moves with no host event **under this patch**.
    ///
    /// The columns `Column::is_self_running` names, except the S&H output with the sampler off —
    /// nothing is sampled and the held value stays where it was, a static value that opens the
    /// amplifier only through `amplifier_held_open`, if it is high (review round 6). The S&H clock
    /// keeps running with the sampler off, and stays self-running. **Whether the sampler is on is
    /// deliberately not recursive**: it does not ask what the input carries, which is exactly the
    /// rule the retired `mode ≠ OFF` was — a static input kept it live too. It errs toward awake,
    /// and a feedback route such as `S&H input ← S&H` cannot loop it.
    ///
    /// **The five LFO core outputs are self-running**, as the LFO columns are: the core runs
    /// whatever the shape switch says.
    ///
    /// **The glide is self-running while its charge decays**, and static once it is flushed to
    /// zero.
    ///
    /// **None of the five other non-column sources is self-running.** The gate and the four
    /// performance inputs cannot change without a host event, and an event wakes the plugin —
    /// `plan-modulation-routing.md` §2.2's *waking* clause. What they can do is hold a value that
    /// keeps a patch live, which is the *staying live* clause and which `amplifier_held_open`
    /// below is what reads.
    fn source_is_self_running(&self, r: &Routing, source: usize) -> bool {
        match source {
            source::SH_OUT => self.sampler_is_on(r),
            source::LFO1_CORE_SAW
            | source::LFO1_CORE_REVERSE_SAW
            | source::LFO1_CORE_TRIANGLE
            | source::LFO1_CORE_SINE
            | source::LFO2_CORE_TRIANGLE => true,
            // **The glide moves by itself while its charge decays**: a key dumps it and it falls
            // to zero with no further event, so an offset plus a falling glide can cross a gate
            // well after the settle. Until the flush takes it to exactly zero (code review,
            // 2026-09-22).
            source::GLIDE => self.glide_charge > 0.0,
            s if s < crate::matrix::COLUMNS => Column::ALL[s].is_self_running(),
            _ => false,
        }
    }

    /// Whether any live route into a target carries a source that can move with no host event — a
    /// self-running source, or an envelope whose own gate input fires without a key.
    ///
    /// **A route at zero depth carries nothing**, so it cannot make a patch live: the amount is
    /// read here as well as the presence. Before the conversion this question had one answer per
    /// row because a row had one source; it is now *any* of them, which is the mixer.
    fn target_is_self_running(&self, p: &Patch, r: &Routing, target: usize) -> bool {
        self.graph.sources(target).iter().any(|&s| {
            if r.amounts[target][s] == 0.0 {
                return false;
            }
            self.source_moves_without_a_key(p, r, s)
        })
    }

    /// A source that can move with no host event: a self-running one, or an envelope whose own
    /// gate input fires without a key.
    fn source_moves_without_a_key(&self, p: &Patch, r: &Routing, source: usize) -> bool {
        match source {
            source::VCF_ADSR => {
                self.gate_fires_without_a_key(p, r, target::VCF_GATE, p.vcf_trigger)
            }
            source::VCA_ADSR => {
                self.gate_fires_without_a_key(p, r, target::VCA_GATE, p.vca_trigger)
            }
            s => self.source_is_self_running(r, s),
        }
    }

    /// Whether a gate input fires with no key down: carrying a self-running source — or, in the
    /// **LFO** trigger position, any source that is high now. The diode AND chops a static high
    /// gate into a rise on every square, so a held keyboard CV above the threshold re-fires the
    /// envelope for as long as it is held, key or no key (review round 6). Read from the frame,
    /// never from the audio.
    ///
    /// **The keyboard gate is a source like any other now**, so "at the default the keyboard is
    /// the gate, and a key is not no key" needs no special case: `source::GATE` is not
    /// self-running, and with no key down it is zero and cannot be high either.
    ///
    /// **And only if the sum can cross.** The gate law sums, then detects once, so a self-running
    /// route too shallow to lift the input over the threshold never fires, and one riding on a
    /// static level that keeps it high never falls to fire again. The bound takes every moving
    /// source at the frame's unit magnitude, which errs toward awake, and every static one at the
    /// value it holds now. Before, any nonzero self-running route counted, and an LFO at a tenth
    /// kept a silent patch live for ever (code review, 2026-09-22).
    ///
    /// **An envelope on a gate input moves if its own gate fires without a key** — LFO 1 firing
    /// Envelope 1 firing Envelope 2 is a patch in motion, even at a zero between pulses. One level
    /// is the whole chain: there are two envelopes, so the inner question treats envelopes as
    /// static, and a loop between them cannot recurse (code review, 2026-09-22).
    fn gate_fires_without_a_key(
        &self,
        p: &Patch,
        r: &Routing,
        target: usize,
        trigger: Trigger,
    ) -> bool {
        self.gate_fires(p, r, target, trigger, false)
    }

    fn gate_fires(
        &self,
        p: &Patch,
        r: &Routing,
        target: usize,
        trigger: Trigger,
        nested: bool,
    ) -> bool {
        let (mut held, mut swing, mut moves) = (0.0_f32, 0.0_f32, false);
        for &s in self.graph.sources(target) {
            let amount = r.amounts[target][s];
            if amount == 0.0 {
                continue;
            }
            let scale = routing::FULL_SCALE[target][s];
            let moving = self.source_is_self_running(r, s)
                || (!nested
                    && match s {
                        source::VCF_ADSR => {
                            self.gate_fires(p, r, target::VCF_GATE, p.vcf_trigger, true)
                        }
                        source::VCA_ADSR => {
                            self.gate_fires(p, r, target::VCA_GATE, p.vca_trigger, true)
                        }
                        _ => false,
                    });
            if moving {
                moves = true;
                swing += (amount * scale).abs();
            } else {
                held += amount * self.graph.read(s) * scale;
            }
        }
        let threshold = crate::matrix::GATE_THRESHOLD;
        match trigger {
            // The diode AND chops any high into a rise on every square, so reaching is enough.
            Trigger::Lfo => held + swing > threshold,
            // Otherwise an edge needs a rise from below, again and again.
            _ => moves && held + swing > threshold && held - swing <= threshold,
        }
    }

    /// The plan's §5.5 activity predicate, evaluated from the patch and the state.
    ///
    /// *Live* is decided from the configuration alone, never from watching the audio: the
    /// amplifier can open with no key — INITIAL GAIN up, or its ADSR input carrying a self-running
    /// source — over a source that is up, or over a singing filter. Judged after Volume, because
    /// Volume at zero makes any patch inaudible.
    pub fn activity(&self, p: &Patch, r: &Routing) -> Activity {
        // Volume at zero makes any patch exactly silent — `process` multiplies by it — so there
        // is nothing to hear and nothing to report, whatever state is hidden behind it.
        if p.volume <= 0.0 {
            return Activity::Inert;
        }
        // **The mixer's fourth channel is a routing target now**, so "is a source up" asks whether
        // anything is routed into it rather than whether one level is above zero.
        // **A level is only up if something reaches it**: the ring modulator is silent with
        // nothing on its input, and a Mix route at zero depth carries nothing (code review,
        // 2026-09-22).
        let source_up = p.level_vco1 > 0.0
            || p.level_vco2 > 0.0
            || p.level_noise > 0.0
            || (p.level_ring > 0.0 && self.audio_input_carries(p, r, target::RING_INPUT))
            || self.audio_input_carries(p, r, target::MIXER_INPUT);
        let singing = p.resonance > crate::filter::EXCITATION_THRESHOLD;
        // **The amplifier's own input decides, and it alone.** `target_is_self_running` follows
        // the routes into it and asks whether any of them carries something that moves by itself;
        // a separate clause on the VCA's gate input said "opens by itself" for an envelope the
        // amplifier no longer read, and kept a silent patch live for ever (review round 5).
        let amplifier_opens_by_itself =
            p.initial_gain > 0.0 || self.target_is_self_running(p, r, target::AMPLIFIER);
        // **Or is held open by a source that will not fall on its own**: a static value above zero
        // on the amplifier's input (a held keyboard CV, a parked wheel), or an envelope whose own
        // gate input is high now — kept in sustain by a source held high. Read from the control
        // state under *this* patch, never from the audio (review round 3). **An envelope in its
        // release is a tail, not a hold**, whatever its level (review round 7); the gate detectors
        // are the state that says which.
        let carried: f32 = self
            .graph
            .sources(target::AMPLIFIER)
            .iter()
            .map(|&s| match s {
                source::VCA_ADSR if !self.vca_gate.is_high() => 0.0,
                source::VCF_ADSR if !self.vcf_gate.is_high() => 0.0,
                s => r.amounts[target::AMPLIFIER][s] * self.graph.read(s),
            })
            .sum::<f32>()
            * routing::FULL_SCALE[target::AMPLIFIER][0];
        let amplifier_held_open =
            self.stack.is_empty() && Vca::gain(p.initial_gain, carried, 0.0) > 0.0;
        if (amplifier_opens_by_itself || amplifier_held_open)
            && (source_up || singing)
            && !self.amplitude_mutes_until_an_event(p, r)
        {
            return Activity::Live;
        }
        if self.tail_open(p, r) {
            Activity::Tailing
        } else {
            Activity::Inert
        }
    }

    /// **Whether the standard Amplitude holds the VCA's output at exact zero until a host event.**
    /// Every route into it at a depth reads a value only an event can move — the wheel, pressure,
    /// the lever, the keyboard gate, Velocity unless [`Voice::velocity_can_move_by_itself`], and the
    /// keyboard CV unless its portamento is still settling — and together they close the factor
    /// exactly. **Every other source counts as moving**: the LFOs, envelopes, S&H, glide, oscillators,
    /// noise and mixer are this machine's generators, and a mute through one stays `Live`, which is
    /// never wrong, only not free. Then
    /// nothing upstream of the effects is audible, as with Volume at zero, **except that the effects
    /// after it still ring out**: the verdict falls through to the tail rather than to `Inert`. A
    /// route from anything that moves by itself — an LFO, an envelope, the keyboard CV under glide —
    /// can reopen it, so it never counts.
    fn amplitude_mutes_until_an_event(&self, p: &Patch, r: &Routing) -> bool {
        self.graph.is_routed(target::AMPLITUDE)
            && self.graph.sources(target::AMPLITUDE).iter().all(|&s| {
                r.amounts[target::AMPLITUDE][s] == 0.0
                    || matches!(
                        s,
                        source::WHEEL | source::PRESSURE | source::BEND | source::GATE
                    )
                    || (s == source::VELOCITY && !self.velocity_can_move_by_itself(p, r))
                    || (s == source::KEY && !self.key_moves_by_itself())
            })
            && standard::amplitude_factor(self.graph.sum(target::AMPLITUDE, r)) == 0.0
    }

    /// **Whether the published keyboard CV can still change**: the portamento's hold charges toward
    /// the key only while one is down, so it moves by itself only then, and only until the CV it
    /// publishes has reached its target — the remaining distance keeps shrinking long after it has
    /// stopped changing a single bit of `hold_target + hold_offset`.
    fn key_moves_by_itself(&self) -> bool {
        !self.stack.is_empty() && self.keyboard_cv() != self.hold_target
    }

    /// **Whether Velocity can change with no host event**: an envelope fired by something other than
    /// a press — an LFO or clock on a gate row, LFO trigger mode chopping a held gate — re-latches it
    /// from the sounding press, which moves it only while a key is held and the sounding press's
    /// velocity is not the one already latched (a GATE+TRIG tie that did not take the bus, say).
    fn velocity_can_move_by_itself(&self, p: &Patch, r: &Routing) -> bool {
        !self.stack.is_empty()
            && self.velocity != self.envelope_velocity
            && (self.gate_fires_without_a_key(p, r, target::VCF_GATE, p.vcf_trigger)
                || self.gate_fires_without_a_key(p, r, target::VCA_GATE, p.vca_trigger))
    }

    /// Whether an audio input carries anything: a route at a depth, from a source that moves by
    /// itself — an envelope a self-running gate fires included — or holds a value other than zero
    /// now. **A static source at zero is silence** — the
    /// keyboard gate with no key down, an envelope at rest — and only a host event can change it,
    /// which wakes the plugin. Before, any route at a depth counted, and `Mix ← Gate` kept a silent
    /// patch live for ever (code review, 2026-09-22). A static value that is not zero still
    /// counts: into the mixer it is a level the output holds, and into the ring modulator it
    /// scales Oscillator 2.
    fn audio_input_carries(&self, p: &Patch, r: &Routing, target: usize) -> bool {
        self.graph.audio_carries(target, r, |s| {
            self.source_moves_without_a_key(p, r, s) || self.graph.read(s) != 0.0
        })
    }

    /// The post-VCA chain's settle: the fixed settle, or the longest tail an effect that is on can
    /// carry. The effects' levels being amounts, an effect at zero adds nothing.
    fn settle_samples_for(&self, p: &Patch) -> u32 {
        let mut settle = (POST_TAIL_S * self.sample_rate) as u32;
        if p.delay_level > 0.0 {
            settle = settle.max(Delay::tail_samples(p.delay_time_s, self.sample_rate));
        }
        if p.reverb > 0.0 {
            settle = settle.max(SpringReverb::tail_samples(self.sample_rate));
        }
        settle
    }

    /// Whether an envelope reaches the amplifier's own input: a present route at a depth that
    /// carries something. **Only then is its release a tail** — an envelope the amplifier does not
    /// read moves nothing a closed amplifier lets through (audit D13).
    #[inline]
    fn amplifier_reads(r: &Routing, source: usize) -> bool {
        r.present[target::AMPLIFIER][source] && r.amounts[target::AMPLIFIER][source] != 0.0
    }

    /// Whether an envelope the amplifier reads is still running.
    #[inline]
    fn an_envelope_the_amplifier_reads_is_active(&self, r: &Routing) -> bool {
        (self.vcf_env.is_active() && Self::amplifier_reads(r, source::VCF_ADSR))
            || (self.vca_env.is_active() && Self::amplifier_reads(r, source::VCA_ADSR))
    }

    /// **What the amplifier can still hear**, never hidden state before it. The first build counted
    /// both envelopes whatever was routed, so a long filter release that only moved the cutoff kept
    /// an exactly silent voice tailing to its end (audit D13).
    fn tail_open(&self, p: &Patch, r: &Routing) -> bool {
        // **Behind a static exact Amplitude mute nothing upstream of the effects is audible**, so a
        // held gate or a sustaining envelope is no tail: only what the effects still hold is.
        if self.amplitude_mutes_until_an_event(p, r) {
            return self.settle_samples < self.settle_needed;
        }
        self.an_envelope_the_amplifier_reads_is_active(r)
            || self.gate_smooth > crate::envelope::ZERO_THRESHOLD
            || self.settle_samples < self.settle_needed
    }

    /// Samples of audible tail remaining: the longest release of an envelope the amplifier reads,
    /// and the settle. The first build counted the amplifier's own envelope alone, whether or not
    /// it was routed there, and never the filter envelope that was (audit D13).
    pub fn tail_samples(&self, p: &Patch, r: &Routing) -> u32 {
        let mut env = 0;
        if Self::amplifier_reads(r, source::VCF_ADSR) {
            env = self.vcf_env.tail_samples(p.vcf_adsr[3]);
        }
        if Self::amplifier_reads(r, source::VCA_ADSR) {
            env = env.max(self.vca_env.tail_samples(p.vca_adsr[3]));
        }
        env.saturating_add(self.settle_samples_for(p))
    }

    /// **An envelope nothing can hear any more is silenced, not frozen.** Once the verdict is inert
    /// a host may stop calling, so an envelope the amplifier does not read, still releasing, would
    /// otherwise be whatever a later re-route found: audible if the host kept calling and not if it
    /// slept. `mxm-mono-pr1` silences its filter envelope for the same reason. Only a released
    /// envelope that nothing but a host event can fire again goes: one whose gate is held high, or
    /// driven by something self-running, is a patch in motion rather than a tail.
    fn silence_what_nothing_can_hear(&mut self, p: &Patch, r: &Routing) {
        if self.activity(p, r) != Activity::Inert {
            return;
        }
        if self.vcf_env.is_active()
            && !self.vcf_gate.is_high()
            && !self.gate_fires_without_a_key(p, r, target::VCF_GATE, p.vcf_trigger)
        {
            self.vcf_env.silence();
        }
        if self.vca_env.is_active()
            && !self.vca_gate.is_high()
            && !self.gate_fires_without_a_key(p, r, target::VCA_GATE, p.vca_trigger)
        {
            self.vca_env.silence();
        }
    }

    /// A key went down, at a velocity the Velocity source carries and nothing hard-wired reads.
    ///
    /// Returns whether **this press** took the bus — judged by its place in the stack, not by
    /// comparing ids, because two presses of one key with no voice id are equal ids and only the
    /// first of them sounds under low-note priority (code review, 2026-09-22).
    pub fn note_on(&mut self, id: NoteId, velocity: f32, p: &Patch) -> bool {
        let was_empty = self.stack.is_empty();
        let sounding_before = self.stack.sounding(p.priority).map(|n| n.note);
        self.stack.push(id, velocity.clamp(0.0, 1.0));
        self.pressed_velocity = velocity.clamp(0.0, 1.0);
        self.follow_the_sounding_velocity(p.priority);
        let took_the_bus = self.stack.sounding_index(p.priority) == self.stack.len().checked_sub(1);
        let sounding_now = self.stack.sounding(p.priority).map(|n| n.note);

        // A gate edge: the first key down. Under low-note priority a higher key over a held lower
        // one changes nothing — no edge, no pitch change — which is the bus (§6.1).
        if let Some(n) = sounding_now {
            if sounding_now != sounding_before {
                self.key_target = n as f32;
            }
        }
        if was_empty {
            self.edge_velocity = velocity.clamp(0.0, 1.0);
            // The hold capacitor is **not** snapped to the new key: it charges toward it through
            // the portamento pot from wherever it was left (§6.2, wart 8), a first note after
            // silence included. `mxm-mono-01` snaps its first note; this machine does not.
            // The glide section dumps its capacitor on the edge (§6.3).
            self.glide_charge = 1.0;
            self.pending_gate_edge = true;
        }
        // The envelopes are gated in `process`, where the gate rows decide whether the keyboard
        // still reaches them at all.
        self.pending_note_on = true;
        took_the_bus
    }

    pub fn note_off(&mut self, voice_id: Option<i32>, channel: u8, note: u8, p: &Patch) {
        let before = self.stack.sounding(p.priority).map(|n| n.note);
        if !self.stack.remove(voice_id, channel, note) {
            return;
        }
        self.follow_the_sounding_velocity(p.priority);
        match self.stack.sounding(p.priority) {
            Some(next) => {
                if Some(next.note) != before {
                    // The bus falls back to another held key: a pitch change with no gate edge.
                    self.key_target = next.note as f32;
                }
            }
            None => self.cancel_pending_press(),
        }
        // The release, when every key is up, is the gate's falling edge in `process`.
    }

    /// The Velocity source follows the press that sounds; with no key held it keeps the last one's.
    fn follow_the_sounding_velocity(&mut self, priority: Priority) {
        if let Some(v) = self.stack.sounding_velocity(priority) {
            self.velocity = v;
        }
    }

    /// **A press every key has been taken back from before the next sample is not a press.** A
    /// note-on and a note-off, a final choke or All Notes Off at the same offset leave no key
    /// down, so the gate never rises and no fall would ever release an envelope that press
    /// fired: GATE+TRIG sat in sustain with nothing held, and a final choke was undone one sample
    /// later (audit D8). The pattern is `mxm-para-07`'s, which cancels its pending lines on every
    /// termination.
    fn cancel_pending_press(&mut self) {
        self.pending_gate_edge = false;
        self.pending_note_on = false;
    }

    /// Choke targets one note and is immediate.
    pub fn choke(&mut self, voice_id: Option<i32>, channel: u8, note: u8, p: &Patch) {
        let before = self.stack.sounding(p.priority).map(|n| n.note);
        if !self.stack.remove(voice_id, channel, note) {
            return;
        }
        self.follow_the_sounding_velocity(p.priority);
        match self.stack.sounding(p.priority) {
            None => {
                self.vcf_env.silence();
                self.vca_env.silence();
                self.key_was_down = false;
                self.cancel_pending_press();
            }
            Some(next) => {
                if Some(next.note) != before {
                    self.key_target = next.note as f32;
                }
            }
        }
    }

    /// CC 120. Immediate, no release — **and no tail**: everything audible is cleared, the
    /// filter's ring, the amplifier's state and the three effects included. What keeps running
    /// is what is not audible on its own — the LFOs, the sample-and-hold and its clock, the
    /// oscillators — so a live patch's clock keeps its phase and the next key finds the machine
    /// where it was. The first build silenced the envelopes and left the reverb ringing for
    /// seconds (review round 1).
    pub fn all_sound_off(&mut self) {
        self.stack.clear();
        // Every press is gone, so no phrase is left to keep: the Velocity source rests.
        self.velocity = 1.0;
        self.envelope_velocity = 1.0;
        self.pressed_velocity = 1.0;
        self.edge_velocity = 1.0;
        self.vcf_env.silence();
        self.vca_env.silence();
        self.gate_smooth = 0.0;
        self.key_was_down = false;
        self.pending_gate_edge = false;
        self.pending_note_on = false;
        self.hpf.reset();
        self.filter.reset();
        self.oversampler.reset();
        self.vca.reset();
        self.phaser.reset();
        self.delay.reset();
        self.reverb.reset();
        // Nothing is left to settle: the activity verdict must not claim a tail that was just
        // cleared (review round 2).
        self.settle_samples = u32::MAX;
    }

    /// CC 123. Every key is released, and **nothing more**: each gate row sees its effective
    /// signal fall — or not, if it is patched to a column that is still high — through the same
    /// detector a key release goes through. The first build released both envelopes directly and
    /// so reached past a patched gate row (review round 3).
    pub fn all_notes_off(&mut self) {
        self.stack.clear();
        self.cancel_pending_press();
    }

    /// A rate CV input: the LFO's base rate, moved by everything routed to it, through OFFSET.
    ///
    /// The row's one GAIN knob is gone; each route carries its own amount, and the sum arrives
    /// already in octaves because `TARGET_SCALE` holds `LFO_CV_OCTAVES_PER_UNIT`.
    #[inline]
    fn lfo_rate(&self, r: &Routing, target: usize, base_hz: f32, offset_cents: f32) -> f32 {
        let octaves = self.graph.sum(target, r) + offset_cents / 1200.0;
        base_hz * octaves.clamp(-12.0, 12.0).exp2()
    }

    /// What a gate input does to its envelope this sample, and whether the gate is high after it.
    ///
    /// **Sum, then detect once** — the gate law. Each route's amount scales its source before the
    /// threshold, so it decides whether that source ever crosses and when within its rise; two
    /// half-amount sources can together cross where neither reaches alone. There is one detector,
    /// so coincidence cannot arise.
    #[inline]
    fn gate(
        env: &mut Adsr,
        gate_row: &mut GateRow,
        level: f32,
        keyboard_is_the_gate: bool,
        trigger: Trigger,
        keys: KeyEvents,
        lfo1_square_high: bool,
    ) -> (bool, PressTrigger) {
        let KeyEvents { note_on, key_rose } = keys;
        // Returns whether the envelope **triggered** this sample, and whether **a press made it**
        // (for the Velocity source): under GATE+TRIG a press reaching this envelope retriggers by
        // itself, and under GATE or LFO only the keyboard's own first press after silence can have
        // raised the gate — a rise that coincides with a later press is the other source's.
        // **One signal, whatever is on it**, so a repatch that changes the level is an edge: from
        // a held key to a low source the envelope releases, from a low source back to a held key
        // it retriggers. The first build detected edges on the keyboard and on the column
        // separately and a repatch fell between them (review round 2).
        let mut value = level;
        // **The LFO position is a diode AND** of the gate and LFO-1's square (§7.1): the
        // envelope's gate is high only while both are, so a key pressed in the square's low half
        // waits for the square's rise, and every fall of the square is a release. The first
        // build fired on the key's own edge regardless of the square (review round 3).
        if trigger == Trigger::Lfo && !lfo1_square_high {
            value = 0.0;
        }
        let (rose, fell) = gate_row.process(value);
        let high = gate_row.is_high();
        // **A key press is a press whether or not it is an edge** — a tie under low-note priority
        // raises no new gate and retriggers GATE+TRIG and nothing else. That is a property of the
        // *keyboard* reaching this envelope, so it is read from the patch: the gate source being
        // routed here, rather than a hard-wired "is this row unpatched". **And only while the gate
        // is high**: a route too shallow or inverted to carry the gate over the threshold raises
        // none, so nothing would ever release what a press through it fired.
        let press = rose || (note_on && keyboard_is_the_gate && high);
        let triggered = match trigger {
            Trigger::Gate | Trigger::Lfo => rose,
            Trigger::GateTrig => press,
        };
        if triggered {
            env.trigger();
        }
        if fell {
            env.release();
        }
        let by_press = if !triggered || !keyboard_is_the_gate {
            PressTrigger::None
        } else {
            match trigger {
                Trigger::GateTrig if note_on && high => PressTrigger::Latest,
                Trigger::Gate | Trigger::Lfo if rose && key_rose => PressTrigger::Edge,
                _ => PressTrigger::None,
            }
        };
        (triggered, by_press)
    }

    /// The sync input's law: **detect per route, reset by the largest depth**.
    ///
    /// It cannot sum, because `EdgeRow`'s interpolated fraction is scale-invariant. So each present
    /// route watches its own source, a crossing resets by that route's own amount, and coincident
    /// crossings in one sample reset by the maximum — the strongest sync wins, which is
    /// unambiguous and is what a player would predict.
    ///
    /// VCO-1's own sync pulse is the one source whose edge is **known exactly** rather than
    /// detected, which is what keeps the plug-out's own normal sample-accurate.
    ///
    /// **A negative depth is no reset, and the largest is the *signed* largest.** A reset has no
    /// inverse — `process_slave` clamps the pull to `0…1` — so an inverted sync route pulls the
    /// core nowhere. Choosing by magnitude would then let a route at `-1.0` beat one at `+0.5` and
    /// produce *no* sync where the player had asked for half of one.
    #[inline]
    fn sync_edge(&mut self, r: &Routing, vco1_edge: Option<f32>) -> Option<(f32, f32)> {
        let mut best: Option<(f32, f32)> = None;
        for &s in self.graph.sources(target::VCO2_SYNC) {
            let depth = r.amounts[target::VCO2_SYNC][s];
            let edge = if s == source::VCO1_SYNC {
                // Still fed to the detector, so its `prev` stays current while it is live.
                self.sync_edges[s].process(self.graph.read(s));
                vco1_edge
            } else {
                self.sync_edges[s].process(self.graph.read(s))
            };
            // Nested rather than a `let` chain: chains are stable from Rust 1.88, and this crate builds at 1.87.
            if let Some(frac) = edge {
                if depth > best.map_or(0.0, |(_, d): (f32, f32)| d) {
                    best = Some((frac, depth));
                }
            }
        }
        best
    }

    /// Render one sample, walking `matrix.rs`'s evaluation order.
    #[inline]
    pub fn process(&mut self, p: &Patch, r: &Routing) -> f32 {
        let fs = self.sample_rate;
        self.graph.begin_sample();

        // --- Stage 1: the keyboard. Portamento charges the hold capacitor only while a key is
        // down (§6.2, wart 8); release mid-lag holds the mid-lag pitch.
        let key_down = !self.stack.is_empty();
        let key_rose = self.pending_gate_edge;
        let note_on = self.pending_note_on;
        self.pending_gate_edge = false;
        self.pending_note_on = false;
        self.key_was_down = key_down;
        // The bus follows the priority **in force**: switching it while two keys are held moves
        // the pitch to the newly selected key, with no gate edge — the note handlers only ever
        // saw the priority at the time of the key.
        match self.stack.sounding(p.priority) {
            Some(id) => {
                if self.bus_note != Some(id.note) {
                    self.key_target = f32::from(id.note);
                    self.bus_note = Some(id.note);
                }
                // The velocity follows a priority switch, which may move the bus between two
                // presses of one note and so change no pitch at all.
                if self.velocity_priority != Some(p.priority) {
                    self.follow_the_sounding_velocity(p.priority);
                    self.velocity_priority = Some(p.priority);
                }
            }
            None => self.bus_note = None,
        }
        if key_down {
            // A new target leaves the capacitor where it was: the distance takes up the difference
            // before it decays.
            if p.portamento_s <= 0.0 {
                self.hold_offset = 0.0;
            } else {
                let coef = (-1.0 / (p.portamento_s * fs)).exp();
                self.hold_offset =
                    flush((self.hold_offset + (self.hold_target - self.key_target)) * coef);
            }
            self.hold_target = self.key_target;
        }
        let keyboard_cv = self.keyboard_cv();
        // The standard Key: 1 V/oct from middle C, in ten-volt units.
        self.graph.write(
            source::KEY,
            standard::key(keyboard_cv, routing::KEY_UNIT_SEMITONES),
        );
        // **The keyboard gate is a published source now**, not a thing an unpatched row fell back
        // to. Both envelope gate inputs read it as an ordinary route in the init patch.
        self.graph
            .write(source::GATE, if key_down { 1.0 } else { 0.0 });
        // The gestures through the collection's standard, each zero at its rest. **Velocity is
        // published after the envelopes** (stage 4), because it is the press that last triggered
        // one; a route read before them takes it a sample late, a held value.
        self.graph.write(source::WHEEL, standard::wheel(p.wheel));
        self.graph
            .write(source::PRESSURE, standard::pressure(p.pressure));
        self.graph
            .write(source::BEND, standard::bend(p.bend_position));

        // --- Stage 2: the LFOs, each rate through its CV input.
        let rate1 = self.lfo_rate(r, target::LFO1_RATE, p.lfo1_rate_hz, p.lfo1_offset_cents);
        let rate2 = self.lfo_rate(r, target::LFO2_RATE, p.lfo2_rate_hz, p.lfo2_offset_cents);
        // The S&H shape reads the shared S&H OUT — last sample's, the S&H being stage 3 — **from
        // the module, not the graph**: the graph publishes a source only while a route reads it,
        // so with S&H on the LFO and nothing routed from S&H itself, the LFO read zero (the owner,
        // 2026-10-08: "Nothing happens. If I put the S&H directly on the cutoff it works").
        let sh_shared = self.sh.out();
        let lfo1 = self.lfo1.process(rate1, p.lfo1_shape, sh_shared, fs);
        let lfo2 = self.lfo2.process(rate2, p.lfo2_shape, sh_shared, fs);
        let lfo1_square_high = self.lfo1.square_high();
        self.graph.write(source::LFO1, lfo1);
        self.graph.write(source::LFO2, lfo2);
        // The cores, ahead of the shape switches: what SAMPLE MODE and the PWM switches chose from.
        // Each is computed only when something reads it — the sine's shaper is not free.
        if self.graph.needs(source::LFO1_CORE_SAW) {
            self.graph
                .write(source::LFO1_CORE_SAW, self.lfo1.sh_source_saw());
        }
        if self.graph.needs(source::LFO1_CORE_REVERSE_SAW) {
            self.graph.write(
                source::LFO1_CORE_REVERSE_SAW,
                self.lfo1.sh_source_saw_inverted(),
            );
        }
        if self.graph.needs(source::LFO1_CORE_TRIANGLE) {
            self.graph
                .write(source::LFO1_CORE_TRIANGLE, self.lfo1.sh_source_triangle());
        }
        if self.graph.needs(source::LFO1_CORE_SINE) {
            self.graph.write(source::LFO1_CORE_SINE, self.lfo1.sine());
        }
        if self.graph.needs(source::LFO2_CORE_TRIANGLE) {
            self.graph
                .write(source::LFO2_CORE_TRIANGLE, self.lfo2.sh_source_triangle());
        }

        // --- Stage 3: the sample-and-hold, from whatever is routed to its input — LFO-1's core
        // shapes being what SAMPLE MODE chose between. `None` is *nothing contributes*, the
        // retired OFF, which holds the last step.
        let sh_input = self
            .sampler_is_on(r)
            .then(|| self.graph.sum(target::SH_INPUT, r));
        let sh_out = self.sh.process(p.sh_rate_hz, p.sh_lag_s, sh_input, fs);
        self.graph.write(source::SH_OUT, sh_out);
        self.graph.write(source::SH_CLOCK, self.sh.clock_out());

        // --- Stage 4: the envelopes, each through its gate input.
        let vcf_gate_level = self.graph.sum(target::VCF_GATE, r);
        let vca_gate_level = self.graph.sum(target::VCA_GATE, r);
        // **The keyboard reaches this envelope** only if its route is present *and* carries
        // something: a route at zero depth contributes nothing, so a tie must not retrigger a
        // GATE+TRIG envelope through it.
        let keyboard_gates =
            |t: usize| r.present[t][source::GATE] && r.amounts[t][source::GATE] != 0.0;
        let keys = KeyEvents { note_on, key_rose };
        let (vcf_triggered, vcf_by_press) = Self::gate(
            &mut self.vcf_env,
            &mut self.vcf_gate,
            vcf_gate_level,
            keyboard_gates(target::VCF_GATE),
            p.vcf_trigger,
            keys,
            lfo1_square_high,
        );
        let (vca_triggered, vca_by_press) = Self::gate(
            &mut self.vca_env,
            &mut self.vca_gate,
            vca_gate_level,
            keyboard_gates(target::VCA_GATE),
            p.vca_trigger,
            keys,
            lfo1_square_high,
        );
        // **An edge is the first press's only if the keyboard alone carries the gate over the
        // threshold** — a route too shallow to cross raises no gate, so a rise it rode on is the
        // other source's (the gate row's sources are unit-scaled; the keyboard gate is one while
        // down). **A GATE+TRIG retrigger needs no crossing**: a press retriggers whenever the gate
        // is high, whatever holds it, so it stays the latest press's even in the sample another
        // source raises the gate — as it would be one sample later.
        let keyboard_crosses =
            |t: usize| r.amounts[t][source::GATE] > crate::matrix::GATE_THRESHOLD;
        let credited = |by: PressTrigger, t: usize| match by {
            PressTrigger::Edge if !keyboard_crosses(t) => PressTrigger::None,
            by => by,
        };
        let vcf_by_press = credited(vcf_by_press, target::VCF_GATE);
        let vca_by_press = credited(vca_by_press, target::VCA_GATE);
        // The latest press retriggered last, so it wins over the edge a first press raised in the
        // same sample.
        if vcf_by_press == PressTrigger::Latest || vca_by_press == PressTrigger::Latest {
            self.envelope_velocity = self.pressed_velocity;
        } else if vcf_by_press == PressTrigger::Edge || vca_by_press == PressTrigger::Edge {
            self.envelope_velocity = self.edge_velocity;
        } else if (vcf_triggered || vca_triggered) && !self.stack.is_empty() {
            self.envelope_velocity = self.velocity;
        }
        self.graph
            .write(source::VELOCITY, standard::velocity(self.envelope_velocity));
        let vcf_env = {
            let [a, d, s, r] = p.vcf_adsr;
            self.vcf_env.process(a, d, s, r)
        };
        let vca_env = {
            let [a, d, s, r] = p.vca_adsr;
            self.vca_env.process(a, d, s, r)
        };
        self.graph.write(source::VCF_ADSR, vcf_env * ENV_UNITS);
        self.graph.write(source::VCA_ADSR, vca_env * ENV_UNITS);

        // --- Stage 5: glide, then VCO-1.
        // The glide: one RC recovering after the gate edge dumped it (§6.3). **It is a source**,
        // and each oscillator's pitch input reads it as an ordinary route — so a player can send
        // the dip to one oscillator, both, or anywhere else, and put something else on the pitch.
        self.glide_charge = flush(self.glide_charge * self.glide_coef);
        self.graph.write(source::GLIDE, self.glide_charge);

        // Host-supplied pitch offsets are user input: finite and within a sane reach, or nothing.
        let finite = |semitones: f32| {
            if semitones.is_finite() {
                semitones.clamp(-96.0, 96.0)
            } else {
                0.0
            }
        };
        let common_st = keyboard_cv
            + p.master_tune_cents / 100.0
            + finite(p.bend_semitones)
            + finite(p.expression_semitones);

        // VCO-1's pitch input, in semitones: the EXT CV jack — normalled to VCO-2, which is stage
        // 6, so that route reads the previous sample exactly as the jack did — and the glide and
        // VCO LFO that DESTINATION used to send here.
        let pitch1 = common_st
            + p.range_offset
            + p.coarse_semitones1
            + p.fine_cents1 / 100.0
            + self.graph.sum(target::VCO1_PITCH, r);
        let freq1 = 440.0 * ((pitch1 - 69.0) / 12.0).exp2();

        // Pulse widths: **wart 4** — with a source routed the pulse narrows from square and never
        // widens; with none, the manual width stands. The LFO sources are the cores' triangles,
        // whatever the shape switches say (wart 5).
        let width1 = routing::narrowed_width(
            p.pulse_width,
            self.graph.is_routed(target::VCO1_WIDTH),
            self.graph.sum(target::VCO1_WIDTH, r),
        );
        let vco1 = self.vco1.process(freq1, p.wave, width1, fs);
        self.graph.write(source::VCO1, vco1 * VCO_OUT_UNITS);
        self.graph
            .write(source::VCO1_SYNC, self.vco1.sync_out() * VCO_OUT_UNITS);

        // --- Stage 6: VCO-2, synced by whatever is routed to its sync input.
        // Its pitch input is read **here, in its own stage**, as `matrix.rs`'s order requires: a
        // route from VCO-1 is same-sample, as a cable from the 101's output to the 102's EXT CV was.
        // And its pulse width, for the same reason: a route from VCO-1 is same-sample.
        let width2 = routing::narrowed_width(
            p.pulse_width2,
            self.graph.is_routed(target::VCO2_WIDTH),
            self.graph.sum(target::VCO2_WIDTH, r),
        );
        let pitch2 = common_st
            + p.range_offset2
            + p.coarse_semitones2
            + p.fine_cents2 / 100.0
            + self.graph.sum(target::VCO2_PITCH, r);
        let freq2 = 440.0 * ((pitch2 - 69.0) / 12.0).exp2();
        // **The presence is the SYNC switch.** Nothing routed is nothing synced, which is what the
        // retired `sync` parameter said and what leaves no second authority over the same thing.
        let vco1_edge = self.vco1.sync_edge();
        let (edge, depth) = match self.sync_edge(r, vco1_edge) {
            Some((frac, depth)) => (Some(frac), depth),
            None => (None, 0.0),
        };
        let vco2 =
            self.vco2
                .process_slave(freq2, p.wave2, width2, fs, edge, p.sync_strength, depth);
        self.graph.write(source::VCO2, vco2 * VCO_OUT_UNITS);
        self.graph
            .write(source::VCO2_SYNC, self.vco2.sync_out() * VCO_OUT_UNITS);

        // --- Stage 7: the ring modulator (X is always VCO-2; Y is its input), and the noise.
        let ring_in = self.graph.sum_audio(target::RING_INPUT, r, fs);
        let ring_out = ring(vco2, ring_in);
        self.graph.write(source::RING_MOD, ring_out * VCO_OUT_UNITS);
        let noise = self.noise.process(p.noise_colour);
        self.graph.write(source::NOISE, noise * VCO_OUT_UNITS);

        // --- Stage 8: the mixer, whose summing input is the one audible overdrive (§5.2).
        //
        // Four levels — the ring modulator's its own, the 102's RING MOD slider — and **Mix**, the
        // EXT IN jack as a routing input: each route's own amount is that source's level, and they
        // add, which is the mixer decision 1.6 asked for.
        let ext_in = self.graph.sum_audio(target::MIXER_INPUT, r, fs);
        let sum = vco1 * p.level_vco1.clamp(0.0, 1.0)
            + vco2 * p.level_vco2.clamp(0.0, 1.0)
            + ring_out * p.level_ring.clamp(0.0, 1.0)
            + ext_in
            + noise * p.level_noise.clamp(0.0, 1.0);
        let mixed = tanh_approx(sum);
        self.graph.write(source::MIXER_OUT, mixed * VCO_OUT_UNITS);

        // --- Stage 9: HPF, the bass compensation, the ladder through its two inputs, the VCA
        // through its one, and Volume.
        let high_passed = self.hpf.process(mixed, p.hpf_hz, fs);
        let vca_adsr_in = self.graph.sum(target::AMPLIFIER, r);
        // The cutoff's one input: what the VCF ADSR IN and LFO IN jacks and KYBD CV carried, each
        // source at the reach its own jack gave it.
        let octaves = self.graph.sum(target::CUTOFF, r);
        let cutoff = p.cutoff_hz * octaves.clamp(-12.0, 12.0).exp2();
        // The ladder runs at twice the rate, so the hardware's 20 kHz is reachable at 48 kHz.
        // The compensation follows the loop gain actually applied, which rises with the cutoff.
        let fs2 = 2.0 * fs;
        let k = self.filter.native_resonance(p.resonance, cutoff, fs2);
        let compensated = high_passed * (1.0 + BASS_COMPENSATION * k);
        let (even, odd) = self.oversampler.up(compensated);
        let even = self.filter.process(even, cutoff, p.resonance, fs2);
        let odd = self.filter.process(odd, cutoff, p.resonance, fs2);
        let filtered = self.oversampler.down(even, odd);

        let gate_target = if key_down { 1.0 } else { 0.0 };
        self.gate_smooth = gate_target + (self.gate_smooth - gate_target) * self.gate_coef;
        // The VCA's LFO input, which only dips (wart 3): `Vca::gain` takes the positive part.
        let tremolo = self.graph.sum(target::TREMOLO, r);
        let gain = Vca::gain(p.initial_gain, vca_adsr_in, tremolo);
        let amplified = self.vca.process(filtered, gain, p.tone, fs);
        // **The collection's standard Amplitude**, a factor after the VCA — not inside its gain,
        // whose `0…1` clamp would swallow the doubling. It cannot open a closed VCA.
        let amplified = if self.graph.is_routed(target::AMPLITUDE) {
            amplified * standard::amplitude_factor(self.graph.sum(target::AMPLITUDE, r))
        } else {
            amplified
        };

        // The effects, in the chosen order; the phaser's two inputs come from the patch bay.
        let phaser_lfo_in = self
            .graph
            .is_routed(target::PHASER_RATE)
            .then(|| self.graph.sum(target::PHASER_RATE, r));
        let phaser_manual_in = self.graph.sum(target::PHASER_CENTRE, r);
        let phased = self
            .phaser
            .process(amplified, p.phaser, phaser_lfo_in, phaser_manual_in, fs);
        let delayed = self
            .delay
            .process(phased, p.delay_level, p.delay_time_s, fs);
        let out = self.reverb.process(delayed, p.reverb, fs);

        // The settle clock for the activity state, measured against what this patch needs.
        // **What the effects hold is tracked, not estimated**: each says whether its memory still
        // holds anything above its snap level, and the clock restarts while one does (below). So
        // what is left after the last of them empties is only the fixed post-chain settle; the
        // estimate stays for `tail_samples`, the length promised to the host.
        self.settle_needed = (POST_TAIL_S * self.sample_rate) as u32;
        // **Whatever reaches the effects restarts the clock**, a keyless self-running patch
        // included: the settle measures their tail from the last audible input, and one that only
        // restarted on a key or an envelope let a patch closed by INITIAL GAIN go inert with its
        // delay still repeating (code review, 2026-09-22). **And an effect that still holds
        // anything keeps it restarted** until its quiet snap or a zero level clears it: the
        // delay's feedback builds a sustained input below the snap level up past it (review
        // rounds 5 and 6).
        // A static exact Amplitude mute is a barrier: a key held or an envelope sounding behind it
        // restarts nothing, and only the effects' own memory does.
        let muted = self.amplitude_mutes_until_an_event(p, r);
        if (!muted && (self.an_envelope_the_amplifier_reads_is_active(r) || key_down))
            || amplified.abs() >= crate::effects::SNAP_LEVEL
            || self.phaser.holds_anything()
            || self.delay.holds_anything()
            || self.reverb.holds_anything()
        {
            self.settle_samples = 0;
        } else {
            self.settle_samples = self.settle_samples.saturating_add(1);
            // Cheap to ask first: only a running envelope past the settle can be one nobody hears.
            if (self.vcf_env.is_active() || self.vca_env.is_active()) && !self.tail_open(p, r) {
                self.silence_what_nothing_can_hear(p, r);
            }
        }

        flush(out * p.volume.max(0.0))
    }

    /// What the frame holds for a source now — the conformance checks' view of a publisher.
    #[cfg(test)]
    pub(crate) fn published_for_test(&self, source: usize) -> f32 {
        self.graph.read(source)
    }

    /// Arms the routing topology for the coming interval. **Every path that can reach
    /// [`Voice::process`] owes this** — a voice that is never armed renders with nothing routed.
    pub fn set_topology(&mut self, routing: &Routing) {
        self.graph.set_topology(routing);
        // **A sync route that has just become live starts from zero**, for the same reason the
        // frame clears a newly read source: an unfed detector holds the `prev` it had when it was
        // last live, so without this the first sample after re-adding a route is compared against
        // one from a previous phrase.
        let live = routing.present[target::VCO2_SYNC];
        for (source, (&now, was)) in live.iter().zip(self.sync_live.iter_mut()).enumerate() {
            if now && !*was {
                self.sync_edges[source].reset();
            }
            *was = now;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const FS: f32 = 48_000.0;

    fn id(note: u8) -> NoteId {
        NoteId {
            voice_id: None,
            channel: 0,
            note,
        }
    }

    fn unarmed_voice() -> Voice {
        let mut v = Voice::new();
        v.set_sample_rate(FS);
        v
    }

    /// **An LFO on S&H plays the S&H's steps even when no route reads S&H itself** (the owner,
    /// 2026-10-08: "Nothing happens. If I put the S&H directly on the cutoff it works"). The init
    /// patch samples noise; LFO 1 reaches the cutoff and nothing reaches it from S&H.
    #[test]
    fn an_lfo_on_sample_and_hold_steps_without_a_route_from_the_sample_and_hold() {
        let mut sound = Sound::default();
        sound.patch.lfo1_shape = Shape::SampleHold;
        sound.patch.sh_rate_hz = 40.0;
        sound.routing.set(target::CUTOFF, source::LFO1, 0.5);
        assert!(
            (0..routing::TARGETS).all(|t| !sound.routing.present[t][source::SH_OUT]),
            "nothing routes from the S&H itself"
        );
        let mut v = unarmed_voice();
        v.set_topology(&sound.routing);
        let mut steps = Vec::new();
        for _ in 0..(FS as usize / 4) {
            let _ = v.process(&sound.patch, &sound.routing);
            let lfo = v.source_value(source::LFO1);
            if steps.last() != Some(&lfo) {
                steps.push(lfo);
            }
        }
        assert!(
            steps.len() > 5,
            "a quarter second at 40 Hz is about ten steps, not {}: {steps:?}",
            steps.len()
        );
    }

    /// **A patch and the grid beside it**, which is how the two travel everywhere now: the routing
    /// is not in `Patch`, because `Patch` is copied every sample and a 1.5 KB grid that changes on
    /// a parameter event has no business being copied 48 000 times a second.
    ///
    /// It derefs to the patch, so a test moves a control exactly as it did before.
    #[derive(Clone, Copy)]
    pub(super) struct Sound {
        pub(super) patch: Patch,
        pub(super) routing: Routing,
    }

    impl std::ops::Deref for Sound {
        type Target = Patch;
        fn deref(&self) -> &Patch {
            &self.patch
        }
    }

    impl std::ops::DerefMut for Sound {
        fn deref_mut(&mut self) -> &mut Patch {
            &mut self.patch
        }
    }

    impl Default for Sound {
        /// The plug-out as it comes: the init patch, wiring included.
        fn default() -> Self {
            Self {
                patch: Patch::default(),
                routing: Routing::init(),
            }
        }
    }

    impl Sound {
        /// The plug-out's wiring with some controls moved, which is what most of these tests want.
        pub(super) fn with(patch: Patch) -> Self {
            Self {
                patch,
                routing: Routing::init(),
            }
        }
    }

    /// Rendering a sound, which is the patch **and** the grid.
    pub(super) trait Play {
        fn play(&mut self, sound: &Sound) -> f32;
        fn state(&self, sound: &Sound) -> Activity;
    }

    impl Play for Voice {
        fn play(&mut self, sound: &Sound) -> f32 {
            self.process(&sound.patch, &sound.routing)
        }
        fn state(&self, sound: &Sound) -> Activity {
            self.activity(&sound.patch, &sound.routing)
        }
    }

    /// One processing interval: **the topology is armed first**, exactly where the plugin arms
    /// it, so a test that re-patches between calls gets the change it asked for.
    fn run(v: &mut Voice, sound: &Sound, secs: f32) -> f32 {
        v.set_topology(&sound.routing);
        let n = (FS * secs) as usize;
        let mut peak = 0.0f32;
        for _ in 0..n {
            peak = peak.max(v.play(sound).abs());
        }
        peak
    }

    fn render(sound: &Sound, note: Option<u8>, samples: usize) -> Vec<f32> {
        let mut v = voice_for(sound);
        if let Some(n) = note {
            v.note_on(id(n), 1.0, sound);
        }
        (0..samples).map(|_| v.play(sound)).collect()
    }

    /// A voice armed with a sound's topology. **Every path to `process` owes that**, which is why
    /// `Graph::begin_sample` panics in debug without it; these helpers are where the tests pay it,
    /// and `unarmed_voice` exists only for the tests that arm themselves.
    fn voice_for(sound: &Sound) -> Voice {
        let mut v = unarmed_voice();
        v.set_topology(&sound.routing);
        v
    }

    /// Put one source on one input at a depth, **replacing whatever was on it** — which is what
    /// pulling the plug-out's cord out and pushing another one in did, and what most of these
    /// tests mean. The mixer that decision 1.6 turned that jack into is exercised by the tests
    /// that name it.
    fn patch(sound: &mut Sound, target: usize, source: usize, amount: f32) {
        sound.routing.clear_target(target);
        sound.routing.set(target, source, amount);
    }

    /// [`patch`], on a copy.
    fn with_patch(sound: &Sound, target: usize, source: usize, amount: f32) -> Sound {
        let mut q = *sound;
        patch(&mut q, target, source, amount);
        q
    }

    /// The depth of one route, for a test that moves an attenuator that used to be a knob.
    fn depth(sound: &mut Sound, target: usize, source: usize, amount: f32) {
        sound.routing.amounts[target][source] = amount;
    }

    /// The retired glide slider at `slider`, `0..=1`: the glide's route on both pitch inputs, which
    /// is where DESTINATION's default sent it, at the depth that slider meant.
    fn glided(patch: Patch, slider: f32) -> Sound {
        let mut sound = Sound::with(patch);
        let amount = -slider * GLIDE_MAX_SEMITONES / routing::PITCH_SEMITONES_PER_UNIT;
        depth(&mut sound, target::VCO1_PITCH, source::GLIDE, amount);
        depth(&mut sound, target::VCO2_PITCH, source::GLIDE, amount);
        sound
    }

    #[test]
    fn silent_until_a_note_arrives_and_inert() {
        let p = Sound::default();
        let mut v = voice_for(&p);
        for _ in 0..10_000 {
            assert_eq!(v.play(&p), 0.0);
        }
        assert_eq!(v.state(&p), Activity::Inert);
    }

    #[test]
    fn a_note_sounds_and_the_release_returns_to_exact_zero_and_inert() {
        let p = Sound::default();
        let mut v = voice_for(&p);
        v.note_on(id(60), 1.0, &p);
        assert!(run(&mut v, &p, 0.2) > 0.01, "note produced no sound");
        assert_eq!(v.state(&p), Activity::Tailing);
        v.note_off(None, 0, 60, &p);
        run(&mut v, &p, 2.0);
        assert_eq!(v.state(&p), Activity::Inert);
        for _ in 0..1_000 {
            assert_eq!(v.play(&p), 0.0, "not exactly silent after release");
        }
    }

    #[test]
    fn low_note_priority_ignores_a_higher_key_over_a_held_lower_one() {
        let p = Sound::default();
        let mut v = voice_for(&p);
        v.note_on(id(48), 1.0, &p);
        run(&mut v, &p, 0.1);
        let cv_before = v.keyboard_cv();
        let level_before = v.vca_env_level();
        v.note_on(id(60), 1.0, &p);
        v.play(&p);
        assert_eq!(v.keyboard_cv(), cv_before, "a higher key moved the pitch");
        assert!(
            (v.vca_env_level() - level_before).abs() < 0.01,
            "a higher key retriggered"
        );
        v.note_off(None, 0, 48, &p);
        v.play(&p);
        assert_eq!(v.keyboard_cv(), 60.0);
    }

    #[test]
    fn last_note_priority_takes_the_newest_key() {
        let p = Sound::with(Patch {
            priority: Priority::Last,
            ..Patch::default()
        });
        let mut v = voice_for(&p);
        v.note_on(id(48), 1.0, &p);
        v.note_on(id(60), 1.0, &p);
        v.play(&p);
        assert_eq!(v.keyboard_cv(), 60.0);
    }

    #[test]
    fn gate_trig_retriggers_on_a_higher_key_that_low_priority_does_not_sound() {
        let p = Sound::with(Patch {
            vca_trigger: Trigger::GateTrig,
            ..Patch::default()
        });
        let mut v = voice_for(&p);
        v.note_on(id(48), 1.0, &p);
        run(&mut v, &p, 1.0);
        assert_eq!(v.vca_env_stage(), Stage::Sustain);
        v.note_on(id(60), 1.0, &p);
        v.play(&p);
        assert_eq!(
            v.vca_env_stage(),
            Stage::Attack,
            "GATE+TRIG did not retrigger"
        );
        assert_eq!(
            v.keyboard_cv(),
            48.0,
            "and the pitch stayed with the lower key"
        );
    }

    #[test]
    fn a_tie_retriggers_nothing_at_gate_and_the_envelope_at_gate_trig() {
        let tie = |trigger: Trigger| {
            let p = Sound::with(Patch {
                priority: Priority::Last,
                vca_trigger: trigger,
                ..Patch::default()
            });
            let mut v = voice_for(&p);
            v.note_on(id(48), 1.0, &p);
            run(&mut v, &p, 1.0);
            v.note_on(id(60), 1.0, &p);
            v.play(&p);
            v.vca_env_stage()
        };
        assert_eq!(tie(Trigger::Gate), Stage::Sustain);
        assert_eq!(tie(Trigger::GateTrig), Stage::Attack);
    }

    /// **Audit D8.** A note that ends at its own offset — a note-on and then a note-off, a final
    /// choke or All Notes Off before the next sample — leaves no key down, so the gate never rises
    /// and no fall will ever release an envelope it fired. GATE+TRIG counted the note-on as a
    /// press anyway: the envelope sat in sustain behind an open amplifier with nothing held, and a
    /// final choke was undone one sample later. In every trigger mode the voice must render
    /// exactly what a voice that never heard the note renders.
    #[test]
    fn a_note_that_ends_at_its_own_offset_fires_no_envelope_in_any_trigger_mode() {
        type Ending = fn(&mut Voice, &Sound);
        let endings: [(&str, Ending); 3] = [
            ("note-off", |v, p| v.note_off(None, 0, 60, p)),
            ("final choke", |v, p| v.choke(None, 0, 60, p)),
            ("All Notes Off", |v, _| v.all_notes_off()),
        ];
        let mut failures = Vec::new();
        for trigger in [Trigger::Gate, Trigger::GateTrig, Trigger::Lfo] {
            let p = Sound::with(Patch {
                vcf_trigger: trigger,
                vca_trigger: trigger,
                ..Patch::default()
            });
            for (name, end) in endings {
                let mut v = voice_for(&p);
                let mut untouched = voice_for(&p);
                v.note_on(id(60), 1.0, &p);
                end(&mut v, &p);
                let mut same = true;
                for _ in 0..FS as usize {
                    same &= v.play(&p) == untouched.play(&p);
                }
                let envelopes = (v.vcf_env_level(), v.vca_env_stage());
                let state = v.state(&p);
                if !same || envelopes != (0.0, Stage::Idle) || state != Activity::Inert {
                    failures.push(format!(
                        "{trigger:?}, {name}: identical {same}, envelopes {envelopes:?}, {state:?}"
                    ));
                }
            }
        }
        assert!(failures.is_empty(), "{failures:#?}");
    }

    /// **D8, where the gate stays high without the key.** A second source can hold a gate input
    /// high after the key is up — here the keyboard CV the hold capacitor keeps above the
    /// threshold — so the gate alone cannot tell a zero-length note from a press. A final choke
    /// arriving with its own note-on must still stop the envelope, not be undone a sample later.
    #[test]
    fn a_final_choke_is_not_undone_by_its_own_note_on_while_another_source_holds_the_gate() {
        let mut p = Sound::with(Patch {
            vca_trigger: Trigger::GateTrig,
            vca_adsr: [0.001, 0.05, 1.0, 0.02],
            ..Patch::default()
        });
        // Beside the keyboard gate, not instead of it.
        p.routing.set(target::VCA_GATE, source::KEY, 1.0);
        let mut v = voice_for(&p);
        v.note_on(id(96), 1.0, &p);
        run(&mut v, &p, 0.3);
        v.note_off(None, 0, 96, &p);
        run(&mut v, &p, 0.3);
        assert_eq!(
            v.vca_env_stage(),
            Stage::Sustain,
            "the premise: the CV holds the gate"
        );
        v.note_on(id(96), 1.0, &p);
        v.choke(None, 0, 96, &p);
        run(&mut v, &p, 0.5);
        assert_eq!(v.vca_env_stage(), Stage::Idle, "the choke was undone");
    }

    /// **The boundary of D8's fix.** A note that ends at its own offset *over a held key* is still
    /// a press: a key is down, the gate is high, and the held key's own release closes the
    /// envelope it retriggered.
    #[test]
    fn a_note_that_ends_at_its_own_offset_over_a_held_key_still_retriggers_gate_trig() {
        let p = Sound::with(Patch {
            vca_trigger: Trigger::GateTrig,
            ..Patch::default()
        });
        let mut v = voice_for(&p);
        v.note_on(id(48), 1.0, &p);
        run(&mut v, &p, 1.0);
        assert_eq!(v.vca_env_stage(), Stage::Sustain, "the premise");
        v.note_on(id(60), 1.0, &p);
        v.note_off(None, 0, 60, &p);
        v.play(&p);
        assert_eq!(v.vca_env_stage(), Stage::Attack, "a key is down: a press");
        v.note_off(None, 0, 48, &p);
        run(&mut v, &p, 2.0);
        assert_eq!(
            v.state(&p),
            Activity::Inert,
            "and the held key's release ends it"
        );
    }

    /// **Found reproducing D8.** A keyboard gate route whose depth never carries the gate over the
    /// threshold — too shallow, or inverted — raises no gate, so nothing will release what a press
    /// through it fires. GATE+TRIG must then do what GATE does: nothing.
    #[test]
    fn a_keyboard_gate_route_that_never_crosses_the_threshold_is_no_press() {
        for amount in [0.1, -1.0] {
            let render = |trigger: Trigger| {
                let mut p = Sound::with(Patch {
                    vca_trigger: trigger,
                    ..Patch::default()
                });
                depth(&mut p, target::VCA_GATE, source::GATE, amount);
                let mut v = voice_for(&p);
                v.note_on(id(60), 1.0, &p);
                let mut out: Vec<f32> = (0..9_600).map(|_| v.play(&p)).collect();
                v.note_off(None, 0, 60, &p);
                out.extend((0..96_000).map(|_| v.play(&p)));
                (out, v.vca_env_stage(), v.state(&p))
            };
            let (gate, _, _) = render(Trigger::Gate);
            let (gate_trig, stage, state) = render(Trigger::GateTrig);
            assert_eq!((stage, state), (Stage::Idle, Activity::Inert), "{amount}");
            assert!(gate == gate_trig, "{amount}: GATE+TRIG differs from GATE");
        }
    }

    #[test]
    fn the_lfo_trigger_refires_the_envelope_only_while_a_key_is_held() {
        let p = Sound::with(Patch {
            vca_trigger: Trigger::Lfo,
            lfo1_rate_hz: 5.0,
            vca_adsr: [0.001, 0.05, 0.0, 0.05],
            ..Patch::default()
        });
        let mut v = voice_for(&p);
        v.note_on(id(60), 1.0, &p);
        let mut attacks = 0;
        let mut was_attack = false;
        for _ in 0..(FS as usize) {
            v.play(&p);
            let now = v.vca_env_stage() == Stage::Attack;
            if now && !was_attack {
                attacks += 1;
            }
            was_attack = now;
        }
        assert!(
            (4..=6).contains(&attacks),
            "{attacks} attacks over a second at 5 Hz"
        );
        v.note_off(None, 0, 60, &p);
        run(&mut v, &p, 0.5);
        let mut refired = false;
        for _ in 0..(FS as usize) {
            v.play(&p);
            refired |= v.vca_env_stage() == Stage::Attack;
        }
        assert!(!refired, "the LFO fired the envelope with no key down");
    }

    #[test]
    fn the_glide_dips_on_a_gate_edge_and_not_on_a_tie_and_recovers_in_its_own_time() {
        let onset = |portamento: f32, tie: bool| {
            let p = glided(
                Patch {
                    priority: Priority::Last,
                    portamento_s: portamento,
                    ..Patch::default()
                },
                0.5,
            );
            let mut v = voice_for(&p);
            if tie {
                v.note_on(id(60), 1.0, &p);
                run(&mut v, &p, 0.5);
                v.note_on(id(64), 1.0, &p);
            } else {
                v.note_on(id(60), 1.0, &p);
            }
            let charge_at_onset = v.glide_charge();
            let mut n = 0usize;
            while v.glide_charge() > 0.05 && n < FS as usize {
                v.play(&p);
                n += 1;
            }
            (charge_at_onset, n as f32 / FS)
        };
        let (edge_charge, edge_recovery) = onset(0.0, false);
        assert_eq!(edge_charge, 1.0, "a gate edge dumps the capacitor");
        assert!(
            (edge_recovery - 3.0 * GLIDE_TAU_S).abs() < 0.02,
            "recovery {edge_recovery}"
        );
        let (tie_charge, _) = onset(0.0, true);
        assert!(tie_charge < 0.05, "a tie must not dip: charge {tie_charge}");
        let (_, slow_recovery) = onset(2.0, false);
        assert!(
            (slow_recovery - edge_recovery).abs() < 0.002,
            "portamento lengthened the glide"
        );
    }

    #[test]
    fn the_keyboard_cv_carries_no_glide_dip() {
        let p = glided(Patch::default(), 1.0);
        let mut v = voice_for(&p);
        v.note_on(id(60), 1.0, &p);
        for _ in 0..100 {
            v.play(&p);
            assert_eq!(v.keyboard_cv(), 60.0);
            assert_eq!(
                v.column(Column::KybdCv),
                0.0,
                "middle C is 0 V on the column"
            );
        }
    }

    #[test]
    fn portamento_holds_where_the_key_left_it() {
        let p = Sound::with(Patch {
            priority: Priority::Last,
            portamento_s: 1.0,
            ..Patch::default()
        });
        let mut v = voice_for(&p);
        v.note_on(id(48), 1.0, &p);
        run(&mut v, &p, 0.2);
        v.note_on(id(72), 1.0, &p);
        run(&mut v, &p, 0.1);
        v.note_off(None, 0, 72, &p);
        v.note_off(None, 0, 48, &p);
        let frozen = v.keyboard_cv();
        assert!(frozen > 48.5 && frozen < 71.5, "mid-lag pitch {frozen}");
        run(&mut v, &p, 1.0);
        assert_eq!(
            v.keyboard_cv(),
            frozen,
            "the capacitor kept charging with no key down"
        );
        v.note_on(id(60), 1.0, &p);
        v.play(&p);
        assert!(
            (v.keyboard_cv() - frozen).abs() < 0.01,
            "the next note did not start from the hold"
        );
    }

    /// **A portamento lands exactly on its note.** The CV form, `target + (cv − target) × coef`,
    /// stopped moving once a step fell under half an ulp of the target: at half a second it came to
    /// rest 9 cents short at 48 kHz and 18 at 96 kHz, until the next key.
    ///
    /// Falsified before trusted: with the CV form back, it rests at 71.91 at 48 kHz.
    #[test]
    fn a_portamento_lands_exactly_on_its_note() {
        for rate in [48_000.0f32, 96_000.0] {
            let p = Sound::with(Patch {
                priority: Priority::Last,
                portamento_s: 0.5,
                ..Patch::default()
            });
            let mut v = voice_for(&p);
            v.set_sample_rate(rate);
            v.note_on(id(48), 1.0, &p);
            v.play(&p);
            v.note_on(id(72), 1.0, &p);
            for _ in 0..(10.0 * rate) as usize {
                v.play(&p);
            }
            assert_eq!(
                v.keyboard_cv(),
                72.0,
                "at {rate} Hz the portamento rests short"
            );
        }
    }

    #[test]
    fn with_portamento_at_zero_every_note_is_immediate_and_with_it_up_even_the_first_slides() {
        let p = Sound::default();
        let mut v = voice_for(&p);
        v.note_on(id(36), 1.0, &p);
        v.play(&p);
        assert_eq!(v.keyboard_cv(), 36.0);
        let slow = Sound::with(Patch {
            portamento_s: 1.0,
            ..Patch::default()
        });
        let mut v = voice_for(&slow);
        v.note_on(id(36), 1.0, &slow);
        v.play(&slow);
        assert!(
            v.keyboard_cv() > 50.0,
            "the first note snapped instead of sliding"
        );
    }

    #[test]
    fn tremolo_only_dips() {
        let render_peak = |initial_gain: f32, vca_lfo: f32| {
            let mut p = Sound::with(Patch {
                initial_gain,
                lfo1_rate_hz: 6.0,
                lfo1_shape: Shape::Saw,
                ..Patch::default()
            });
            // The retired VCA LFO slider is the tremolo input's LFO 1 route, wired at Init.
            depth(&mut p, target::TREMOLO, source::LFO1, vca_lfo);
            let mut v = voice_for(&p);
            // The amplifier's envelope route out of the way: this is about the LFO alone.
            p.routing.clear_target(target::AMPLIFIER);
            run(&mut v, &p, 1.0)
        };
        assert_eq!(
            render_peak(0.0, 1.0),
            0.0,
            "no effect with the amplifier closed"
        );
        let standing = render_peak(0.5, 0.0);
        assert!(
            render_peak(0.5, 1.0) <= standing + 1e-6,
            "the LFO opened the amplifier"
        );
    }

    #[test]
    fn pwm_narrows_from_square_and_never_widens_from_any_source() {
        // The PWM switch's five non-MANUAL positions, each a source on the pulse-width input now.
        for source in [
            source::LFO1_CORE_TRIANGLE,
            source::GLIDE,
            source::VCF_ADSR,
            source::VCA_ADSR,
            source::LFO2_CORE_TRIANGLE,
        ] {
            let mut p = Sound::with(Patch {
                wave: Wave::Square,
                // A manual width that a routed source must ignore: with anything on the input the
                // pulse narrows from *square*, not from here.
                pulse_width: 0.2,
                lfo1_rate_hz: 2.0,
                lfo2_rate_hz: 3.0,
                vcf_adsr: [0.2, 0.3, 0.5, 0.2],
                vca_adsr: [0.2, 0.3, 0.5, 0.2],
                ..Patch::default()
            });
            patch(&mut p, target::VCO1_WIDTH, source, 1.0);
            let mut v = voice_for(&p);
            v.note_on(id(48), 1.0, &p);
            let mut worst_duty = 0.0f32;
            let mut prev = 0.0f32;
            let (mut period_high, mut period_len, mut high) = (0usize, 0usize, 0usize);
            for _ in 0..(FS as usize) {
                // The oscillator's own column: the wart is the oscillator's, and downstream the
                // ladder and the decimator ring on a narrow pulse's edges, which puts zero
                // crossings where the duty is not.
                v.play(&p);
                let y = v.column(Column::Vco1);
                if prev <= 0.0 && y > 0.0 && period_len > 0 {
                    worst_duty = worst_duty.max(period_high as f32 / period_len as f32);
                    period_high = 0;
                    period_len = 0;
                }
                period_len += 1;
                if y > 0.0 {
                    period_high += 1;
                    high += 1;
                }
                prev = y;
            }
            assert!(high > 0);
            assert!(worst_duty <= 0.55, "{source:?}: duty reached {worst_duty}");
        }
    }

    /// **Wart 4, at rest.** Routing a source to a pulse-width input is moving the PWM switch off
    /// MANUAL: the pulse is square before anything moves it, and the manual width no longer counts —
    /// even at zero depth, which is the switch in an LFO position with its slider down. With nothing
    /// routed the manual width is the width.
    #[test]
    fn a_routed_pulse_width_rests_at_square_whatever_the_manual_width() {
        let duty = |sound: &Sound| {
            let mut v = voice_for(sound);
            v.note_on(id(48), 1.0, sound);
            let (mut high, mut total) = (0usize, 0usize);
            for i in 0..(FS as usize / 2) {
                v.play(sound);
                if i > 4_800 {
                    total += 1;
                    if v.column(Column::Vco1) > 0.0 {
                        high += 1;
                    }
                }
            }
            high as f32 / total as f32
        };
        let manual = Sound::with(Patch {
            wave: Wave::Square,
            pulse_width: 0.2,
            ..Patch::default()
        });
        let at_manual = duty(&manual);
        assert!(
            (at_manual - 0.2).abs() < 0.03,
            "manual width: duty {at_manual}"
        );
        let routed = with_patch(&manual, target::VCO1_WIDTH, source::LFO1_CORE_TRIANGLE, 0.0);
        let at_rest = duty(&routed);
        assert!(
            (at_rest - 0.5).abs() < 0.03,
            "a routed width rests at square, not at the manual width: duty {at_rest}"
        );
    }

    /// The five LFO core outputs run whatever the shape switches say, so an amplifier opened by one
    /// of them sounds with no key — live, as an amplifier opened by an LFO column is.
    #[test]
    fn every_core_source_keeps_a_self_running_patch_live() {
        for s in [
            source::LFO1_CORE_SAW,
            source::LFO1_CORE_REVERSE_SAW,
            source::LFO1_CORE_TRIANGLE,
            source::LFO1_CORE_SINE,
            source::LFO2_CORE_TRIANGLE,
        ] {
            let p = with_patch(&Sound::default(), target::AMPLIFIER, s, 1.0);
            assert_eq!(
                voice_for(&p).state(&p),
                Activity::Live,
                "{} on the amplifier is not live",
                routing::SOURCE_NAMES[s]
            );
        }
    }

    /// **A mixer level is only up if something reaches it.** The ring modulator with nothing on
    /// its input, or only a zero-depth route, is silent; so is a Mix route at zero depth. Either
    /// made an exactly silent patch report live for ever (code review, 2026-09-22). **Falsified**:
    /// with the old `level_ring > 0 || audio_is_sounding` each of the first three is live.
    #[test]
    fn a_level_nothing_reaches_does_not_keep_a_patch_live() {
        let open = Sound::with(Patch {
            level_vco1: 0.0,
            initial_gain: 0.5,
            ..Patch::default()
        });
        let state = |p: &Sound| voice_for(p).state(p);

        let mut ring_unfed = open;
        ring_unfed.level_ring = 1.0;
        ring_unfed.routing.clear_target(target::RING_INPUT);
        assert_ne!(
            state(&ring_unfed),
            Activity::Live,
            "a ring modulator with no input"
        );

        let mut ring_zero = ring_unfed;
        ring_zero.routing.set(target::RING_INPUT, source::VCO1, 0.0);
        assert_ne!(
            state(&ring_zero),
            Activity::Live,
            "a ring input at zero depth"
        );

        let mix_zero = with_patch(&open, target::MIXER_INPUT, source::NOISE, 0.0);
        assert_ne!(
            state(&mix_zero),
            Activity::Live,
            "a Mix route at zero depth"
        );

        // The premise: each is live the moment it carries something.
        let mut ring_fed = open;
        ring_fed.level_ring = 1.0;
        assert_eq!(
            state(&ring_fed),
            Activity::Live,
            "the ring modulator fed by VCO-1"
        );
        let mix_up = with_patch(&open, target::MIXER_INPUT, source::NOISE, 0.5);
        assert_eq!(state(&mix_up), Activity::Live, "noise on Mix");
    }

    /// **Tremolo never makes a patch live on its own.** It can only dip an amplifier that INITIAL
    /// GAIN or an envelope has opened (wart 3), so on a closed one it moves nothing audible, whatever
    /// source is on it.
    #[test]
    fn tremolo_alone_on_a_closed_amplifier_is_inert() {
        let mut p = with_patch(&Sound::default(), target::TREMOLO, source::LFO1, 1.0);
        p.routing.set(target::TREMOLO, source::LFO1_CORE_SAW, -1.0);
        p.routing.clear_target(target::AMPLIFIER);
        let mut v = voice_for(&p);
        assert_ne!(
            v.state(&p),
            Activity::Live,
            "a tremolo opened a closed amplifier"
        );
        let peak = run(&mut v, &p, 0.5);
        assert_eq!(peak, 0.0, "and it is exactly silent: {peak}");
        assert_eq!(v.state(&p), Activity::Inert);
    }

    /// **A gate route too shallow to cross never fires, so it keeps nothing live.** The gate law
    /// sums, then detects once against the threshold; LFO 1 at a tenth cannot reach it, the
    /// envelope never opens, and the patch is silent. Two shallow routes that sum past it can fire,
    /// and do count (code review, 2026-09-22). **Falsified** with the old any-nonzero-route rule:
    /// the shallow patch reads live.
    #[test]
    fn a_gate_route_too_shallow_to_fire_does_not_keep_a_patch_live() {
        let deep = with_patch(&Sound::default(), target::VCA_GATE, source::LFO1, 1.0);
        assert_eq!(voice_for(&deep).state(&deep), Activity::Live, "the premise");

        let shallow = with_patch(&Sound::default(), target::VCA_GATE, source::LFO1, 0.1);
        let mut v = voice_for(&shallow);
        assert_ne!(
            v.state(&shallow),
            Activity::Live,
            "LFO 1 at a tenth cannot reach the gate threshold"
        );
        assert_eq!(
            run(&mut v, &shallow, 0.5),
            0.0,
            "and the envelope never opened"
        );

        let mut two = shallow;
        two.routing.set(target::VCA_GATE, source::LFO2, 0.2);
        assert_eq!(
            voice_for(&two).state(&two),
            Activity::Live,
            "two shallow routes that sum past the threshold"
        );
    }

    /// **A static source at zero on an audio input carries nothing.** `Mix ← Gate` and
    /// `Ring mod ← Gate` with no key down are silence, and only a key — a host event, which wakes
    /// the plugin — can change that (code review, 2026-09-22). **Falsified** with the old
    /// any-route-at-a-depth rule: both read live.
    #[test]
    fn a_static_source_at_zero_on_an_audio_input_keeps_nothing_live() {
        let open = Sound::with(Patch {
            level_vco1: 0.0,
            initial_gain: 0.5,
            ..Patch::default()
        });
        let mix = with_patch(&open, target::MIXER_INPUT, source::GATE, 1.0);
        let mut v = voice_for(&mix);
        run(&mut v, &mix, 0.01);
        assert_ne!(v.state(&mix), Activity::Live, "Mix from the gate, no key");

        let mut ring = with_patch(&open, target::RING_INPUT, source::GATE, 1.0);
        ring.level_ring = 1.0;
        let mut w = voice_for(&ring);
        run(&mut w, &ring, 0.01);
        assert_ne!(
            w.state(&ring),
            Activity::Live,
            "Ring mod from the gate, no key"
        );

        // The premise: with the key down the gate is a level on each.
        v.note_on(id(60), 1.0, &mix);
        run(&mut v, &mix, 0.01);
        assert_eq!(v.state(&mix), Activity::Live, "Mix from a held gate");
        w.note_on(id(60), 1.0, &ring);
        run(&mut w, &ring, 0.01);
        assert_eq!(w.state(&ring), Activity::Live, "Ring mod from a held gate");

        // **An envelope a self-running gate fires is not static**, even at a zero between its
        // pulses: LFO 1 will fire it again with no host event.
        let mut clocked = with_patch(&open, target::MIXER_INPUT, source::VCF_ADSR, 1.0);
        patch(&mut clocked, target::VCF_GATE, source::LFO1, 1.0);
        let v = voice_for(&clocked);
        assert_eq!(v.graph.read(source::VCF_ADSR), 0.0, "the premise: at rest");
        assert_eq!(
            v.state(&clocked),
            Activity::Live,
            "Mix from a clocked envelope"
        );
    }

    /// **An effect's tail outlives a keyless patch.** VCO-1 through INITIAL GAIN into the delay,
    /// with no key, then the gain closed: the repeats are still in the line, so the patch is
    /// tailing, not inert. **Falsified** by dropping the `amplified` clause from the settle.
    #[test]
    fn closing_a_keyless_patch_leaves_the_effect_tail() {
        let mut p = Sound::with(Patch {
            initial_gain: 0.5,
            delay_level: 0.5,
            ..Patch::default()
        });
        let mut v = voice_for(&p);
        run(&mut v, &p, 0.3);
        assert_eq!(v.state(&p), Activity::Live, "the premise");
        p.initial_gain = 0.0;
        run(&mut v, &p, 0.1);
        assert_eq!(
            v.state(&p),
            Activity::Tailing,
            "the delay is still repeating what it was fed"
        );
    }

    /// **A full stack keeps the sounding low note.** Sixteen held, the lowest first; a
    /// seventeenth higher press evicts the oldest *other* press, so the bus and its velocity stay
    /// on the low note. **Falsified** with the old rotate-left eviction.
    #[test]
    fn a_full_stack_keeps_the_sounding_low_note() {
        let p = Sound::default();
        let mut v = voice_for(&p);
        v.note_on(id(36), 0.2, &p);
        for n in 0..(MAX_HELD_NOTES as u8 - 1) {
            v.note_on(id(48 + n), 0.9, &p);
        }
        assert_eq!(
            v.sounding(p.priority).map(|n| n.note),
            Some(36),
            "the premise"
        );
        v.note_on(id(80), 1.0, &p);
        assert_eq!(v.sounding(p.priority).map(|n| n.note), Some(36));
        assert_eq!(v.velocity(), 0.2);
        v.note_off(None, 0, 48, &p);
        assert_eq!(
            v.sounding(p.priority).map(|n| n.note),
            Some(36),
            "48 was the press evicted, so its release changes nothing"
        );
    }

    /// **A decaying glide is a moving source.** Velocity's offset plus the glide inverted sits
    /// below the gate threshold at the key and rises through it as the charge falls — after the
    /// release and the settle, with no further event. It was read as static, so the patch went
    /// inert before the envelope fired (code review, 2026-09-22). **Falsified** by dropping the
    /// `GLIDE` arm from `source_is_self_running`.
    #[test]
    fn a_decaying_glide_on_a_gate_keeps_the_patch_live() {
        let mut p = with_patch(&Sound::default(), target::VCA_GATE, source::VELOCITY, 0.4);
        p.routing.set(target::VCA_GATE, source::GLIDE, -1.0);
        let mut v = voice_for(&p);
        v.note_on(id(60), 1.0, &p);
        run(&mut v, &p, 0.005);
        v.note_off(None, 0, 60, &p);
        run(&mut v, &p, 0.06);
        assert!(v.glide_charge() > 0.0, "the premise: still decaying");
        assert_eq!(
            v.state(&p),
            Activity::Live,
            "a falling glide will cross the gate"
        );
    }

    /// **A chain of envelopes a self-running gate starts is live**, from a zero phase: LFO 1 fires
    /// Envelope 1, Envelope 1 fires Envelope 2, Envelope 2 opens the amplifier. **Falsified** by
    /// dropping the envelope clause from `gate_fires`: the chain reads inert. And a loop of the
    /// two envelopes with nothing self-running is not.
    #[test]
    fn an_envelope_chain_a_clock_starts_is_live() {
        let mut chain = with_patch(&Sound::default(), target::VCF_GATE, source::LFO1, 1.0);
        patch(&mut chain, target::VCA_GATE, source::VCF_ADSR, 1.0);
        let v = voice_for(&chain);
        assert_eq!(v.graph.read(source::VCF_ADSR), 0.0, "the premise: at rest");
        assert_eq!(v.state(&chain), Activity::Live);

        let mut ring = with_patch(&Sound::default(), target::VCF_GATE, source::VCA_ADSR, 1.0);
        patch(&mut ring, target::VCA_GATE, source::VCF_ADSR, 1.0);
        assert_ne!(
            voice_for(&ring).state(&ring),
            Activity::Live,
            "two envelopes alone"
        );
    }

    /// **A trigger another source makes in the same sample as a press is not the press's.** Under
    /// low-note priority and GATE, hold a hard low note and press a soft higher one exactly as an
    /// LFO on the VCA envelope's gate rises: the LFO triggered the envelope, the higher press took
    /// neither the bus nor the gate, so Velocity takes the sounding low note's — not the soft
    /// press's.
    ///
    /// Falsified before trusted: crediting every trigger in a press's sample to that press reads
    /// the soft press's `−0.75`.
    #[test]
    fn a_trigger_another_source_makes_beside_a_press_is_not_the_presss() {
        let mut sound = Sound::with(Patch {
            priority: Priority::Low,
            vcf_trigger: Trigger::Gate,
            vca_trigger: Trigger::Gate,
            lfo1_shape: Shape::Square,
            lfo1_rate_hz: 20.0,
            ..Patch::default()
        });
        sound.routing.clear_target(target::VCA_GATE);
        sound.routing.set(target::VCA_GATE, source::LFO1, 1.0);
        sound.routing.set(target::CUTOFF, source::VELOCITY, 0.0);

        // A probe finds a sample, after the low note is under way, on which the LFO raises the
        // VCA envelope's gate.
        let mut probe = voice_for(&sound);
        probe.note_on(id(48), 1.0, &sound);
        probe.play(&sound);
        let mut rise = None;
        let mut was_high = probe.vca_gate.is_high();
        for n in 1..48_000 {
            probe.play(&sound);
            let high = probe.vca_gate.is_high();
            if high && !was_high {
                rise = Some(n);
                break;
            }
            was_high = high;
        }
        let rise = rise.expect("the premise: the LFO raises the VCA envelope's gate");

        let mut v = voice_for(&sound);
        v.note_on(id(48), 1.0, &sound);
        for _ in 0..rise {
            v.play(&sound);
        }
        assert!(
            !v.vca_gate.is_high(),
            "the premise: the gate is low before the rise"
        );
        assert_eq!(v.graph.read(source::VELOCITY), 0.0, "the low note's own");
        v.note_on(id(72), 0.25, &sound);
        v.play(&sound);
        assert!(
            v.vca_gate.is_high(),
            "the premise: the LFO raised it on the press's sample"
        );
        assert_eq!(
            v.graph.read(source::VELOCITY),
            0.0,
            "the sounding low note's, not the soft press's"
        );
    }

    /// **The sounding press's velocity follows the bus** when it falls back — `mxm-mono-02`'s rule.
    /// It was the last note-on's, so a key that did not take the bus under low-note priority changed
    /// it, and a fallback kept the released key's (code review, 2026-09-22). With every key up it
    /// keeps the last sounding press's. **It is what an envelope trigger latches** as the Velocity
    /// source (the modulation standard; `conformance::tests`), not the source itself.
    #[test]
    fn the_velocity_source_follows_the_sounding_press() {
        // Routed, so the voice publishes it: an unread source is never written.
        let mut low = Sound::default();
        low.routing.set(target::CUTOFF, source::VELOCITY, 0.5);
        assert_eq!(low.priority, Priority::Low, "the premise");
        let mut v = voice_for(&low);
        v.note_on(id(60), 0.9, &low);
        v.note_on(id(48), 0.3, &low);
        assert_eq!(v.velocity, 0.3, "the lower press sounds");
        v.note_on(id(72), 1.0, &low);
        assert_eq!(v.velocity, 0.3, "a higher key does not take the bus");
        v.note_off(None, 0, 48, &low);
        assert_eq!(
            v.velocity, 0.9,
            "the bus falls back to 60, with its own velocity"
        );
        v.note_off(None, 0, 60, &low);
        assert_eq!(v.velocity, 1.0, "and on to 72");
        v.note_off(None, 0, 72, &low);
        assert_eq!(v.velocity, 1.0, "the release keeps the last press's");
        v.play(&low);
        assert_eq!(
            v.graph.read(source::VELOCITY),
            0.0,
            "every press came and went inside one sample, so none triggered an envelope: at rest"
        );

        let mut last = low;
        last.priority = Priority::Last;
        let mut w = voice_for(&last);
        w.note_on(id(48), 0.3, &last);
        w.note_on(id(60), 0.9, &last);
        assert_eq!(w.velocity, 0.9, "last-note priority: the newest press");
        w.note_off(None, 0, 60, &last);
        assert_eq!(w.velocity, 0.3, "and back to the held one");

        // A priority switch with two keys held moves the bus, and the velocity follows it.
        w.note_on(id(72), 0.6, &last);
        assert_eq!(w.velocity, 0.6);
        w.play(&low);
        assert_eq!(w.velocity, 0.3, "low-note priority now: 48's velocity");

        // Two presses of one key: a switch moves the bus between them with no pitch change.
        let mut x = voice_for(&low);
        let press = |voice_id| NoteId {
            voice_id: Some(voice_id),
            channel: 0,
            note: 60,
        };
        x.note_on(press(1), 0.2, &low);
        x.note_on(press(2), 0.9, &low);
        x.play(&low);
        assert_eq!(
            x.velocity, 0.2,
            "low-note priority: the first of the equal lows"
        );
        x.play(&last);
        assert_eq!(
            x.velocity, 0.9,
            "last-note priority: the newer press of the same key"
        );
    }

    #[test]
    fn initial_gain_makes_the_patch_live_with_no_key_and_volume_zero_makes_it_inert() {
        let p = Sound::with(Patch {
            initial_gain: 0.5,
            ..Patch::default()
        });
        let mut v = voice_for(&p);
        assert_eq!(v.state(&p), Activity::Live);
        assert!(
            run(&mut v, &p, 0.2) > 0.01,
            "a live patch sounds with no key"
        );
        let mut muted = p;
        muted.volume = 0.0;
        assert_eq!(v.state(&muted), Activity::Inert);
        // Closed, what it just sounded is still in the post-amplifier chain: the settle, then
        // nothing (the settle restarts on audible input since the code review of 2026-09-22).
        let mut closed = p;
        closed.initial_gain = 0.0;
        assert_eq!(v.state(&closed), Activity::Tailing);
        run(&mut v, &closed, 0.2);
        assert_eq!(v.state(&closed), Activity::Inert);
    }

    /// **Review round 3.** The LFO trigger position is a diode AND: a key pressed in the
    /// square's low half waits for the square, and the square's fall is a release.
    #[test]
    fn the_lfo_trigger_is_an_and_with_the_square() {
        // LFO-1 at 2 Hz: the square is high for the first 250 ms of each cycle, low for the next.
        let p = Sound::with(Patch {
            lfo1_rate_hz: 2.0,
            vca_trigger: Trigger::Lfo,
            vca_adsr: [0.001, 0.05, 1.0, 0.01],
            ..Patch::default()
        });
        let mut v = voice_for(&p);
        // Into the low half, then press.
        run(&mut v, &p, 0.3);
        v.note_on(id(60), 1.0, &p);
        run(&mut v, &p, 0.1);
        assert_eq!(
            v.vca_env_stage(),
            Stage::Idle,
            "pressed while the square is low, the envelope waits for the square"
        );
        // The square rises at 500 ms.
        run(&mut v, &p, 0.15);
        assert!(
            matches!(
                v.vca_env_stage(),
                Stage::Attack | Stage::Decay | Stage::Sustain
            ),
            "the square's rise fired the envelope: {:?}",
            v.vca_env_stage()
        );
        // And its fall at 750 ms releases it.
        run(&mut v, &p, 0.25);
        assert!(
            matches!(v.vca_env_stage(), Stage::Release | Stage::Idle),
            "the square's fall released it: {:?}",
            v.vca_env_stage()
        );
    }

    /// **Review round 5.** A self-running gate on the VCA's envelope makes the patch live only
    /// while the amplifier reads that envelope. Patch the ADSR IN row elsewhere and the clocked
    /// envelope reaches nothing: the patch is exactly silent, and must not be called live.
    #[test]
    fn a_clocked_envelope_the_amplifier_does_not_read_is_not_live() {
        let mut p = Sound::with(Patch {
            sh_rate_hz: 4.0,
            vca_adsr: [0.001, 0.05, 1.0, 0.02],
            ..Patch::default()
        });
        patch(&mut p, target::VCA_GATE, source::SH_CLOCK, 1.0);
        // The premise: with the ADSR IN row at its default the clocked envelope opens the
        // amplifier, and that is live — the slow-clock case.
        assert_eq!(voice_for(&p).state(&p), Activity::Live, "the premise");

        // The same gate, but the amplifier reads the keyboard CV instead — zero with no key.
        patch(&mut p, target::AMPLIFIER, source::KEY, 1.0);
        let mut v = voice_for(&p);
        assert_ne!(
            v.state(&p),
            Activity::Live,
            "a clock on an envelope the amplifier does not read opens nothing"
        );
        let peak = run(&mut v, &p, 1.0);
        assert_eq!(peak, 0.0, "and the patch is exactly silent: {peak}");
        assert_eq!(
            v.state(&p),
            Activity::Inert,
            "so once the settle has run it is inert, not live for ever"
        );
    }

    /// **Review round 6.** In the LFO trigger position the diode AND chops a static high gate
    /// into a rise on every square. A gate row patched to the keyboard CV, which holds after the
    /// release, therefore re-fires the envelope with no key down for as long as the CV is high:
    /// the patch sounds, and must report live — it did report tailing, because the CV column is
    /// not self-running on its own.
    #[test]
    fn an_lfo_trigger_chopping_a_held_cv_is_live() {
        let mut p = Sound::with(Patch {
            vca_trigger: Trigger::Lfo,
            lfo1_rate_hz: 4.0,
            vca_adsr: [0.001, 0.05, 1.0, 0.02],
            ..Patch::default()
        });
        patch(&mut p, target::VCA_GATE, source::KEY, 1.0);
        let mut v = voice_for(&p);
        // A high key: the keyboard CV column holds well above the gate threshold afterwards.
        v.note_on(id(96), 1.0, &p);
        run(&mut v, &p, 0.5);
        v.note_off(None, 0, 96, &p);
        run(&mut v, &p, 1.0);
        let level = run(&mut v, &p, 0.5);
        assert!(
            level > 1e-3,
            "the premise: the chopped CV keeps the envelope firing with no key: {level}"
        );
        assert_eq!(
            v.state(&p),
            Activity::Live,
            "sounding with no key, from the configuration alone, is live"
        );
    }

    /// **Review round 6.** The S&H output is self-running only while the sampler samples. With
    /// nothing contributing to its input — the retired OFF — it holds the last step, a static
    /// value, so an amplifier fed by it with nothing held is exactly silent and must not be called
    /// live for ever. **A route at zero depth contributes nothing**, so it is OFF too: the sampler
    /// does not start on it and the patch is not live on it.
    #[test]
    fn a_sampler_switched_off_is_a_static_source_and_not_live() {
        let mut p = Sound::with(Patch {
            sh_rate_hz: 4.0,
            vca_adsr: [0.001, 0.05, 1.0, 0.02],
            ..Patch::default()
        });
        patch(&mut p, target::SH_INPUT, source::LFO1_CORE_SAW, 1.0);
        patch(&mut p, target::AMPLIFIER, source::SH_OUT, 1.0);
        assert_eq!(
            voice_for(&p).state(&p),
            Activity::Live,
            "the premise: a sampling S&H on the amplifier's row is live"
        );

        // The route stays present at zero depth: that is OFF, not a sampler sampling zero.
        depth(&mut p, target::SH_INPUT, source::LFO1_CORE_SAW, 0.0);
        let mut v = voice_for(&p);
        assert_ne!(
            v.state(&p),
            Activity::Live,
            "with the sampler off the output holds a static value: nothing opens by itself"
        );
        let peak = run(&mut v, &p, 1.0);
        assert_eq!(peak, 0.0, "and the patch is exactly silent: {peak}");
        assert_eq!(
            v.state(&p),
            Activity::Inert,
            "so it is inert after the settle"
        );
    }

    /// **Review round 7.** An envelope kept in sustain by a gate row patched to a column that
    /// stays high sounds for as long as the column does, with no key down: live, not a tail. The
    /// held-open clause asked the VCA envelope to be idle and so called exactly this a tail.
    #[test]
    fn an_envelope_kept_in_sustain_by_a_high_gate_row_is_live() {
        let mut p = Sound::with(Patch {
            vca_adsr: [0.001, 0.05, 1.0, 0.02],
            ..Patch::default()
        });
        patch(&mut p, target::VCA_GATE, source::KEY, 1.0);
        let mut v = voice_for(&p);
        v.note_on(id(96), 1.0, &p);
        run(&mut v, &p, 0.2);
        v.note_off(None, 0, 96, &p);
        run(&mut v, &p, 1.0);
        assert_eq!(
            v.vca_env_stage(),
            Stage::Sustain,
            "the premise: the held CV keeps the gate high"
        );
        let level = run(&mut v, &p, 0.5);
        assert!(
            level > 1e-3,
            "and it is still sounding a second and a half later: {level}"
        );
        assert_eq!(
            v.state(&p),
            Activity::Live,
            "sounding with no key, held by the gate row, is live"
        );
    }

    /// **Review round 7.** The other direction: an amplifier reading the *filter* envelope hears
    /// that envelope's release after the key goes up. A release ends on its own — it is a tail,
    /// and must not be called live because its level is still above zero.
    #[test]
    fn an_amplifier_reading_a_releasing_envelope_is_tailing() {
        let mut p = Sound::with(Patch {
            vcf_adsr: [0.001, 0.05, 1.0, 2.0],
            vca_adsr: [0.001, 0.05, 1.0, 0.02],
            ..Patch::default()
        });
        patch(&mut p, target::AMPLIFIER, source::VCF_ADSR, 1.0);
        let mut v = voice_for(&p);
        v.note_on(id(60), 1.0, &p);
        run(&mut v, &p, 0.3);
        v.note_off(None, 0, 60, &p);
        run(&mut v, &p, 0.3);
        assert!(
            v.vcf_env_level() > 0.1,
            "the premise: the filter envelope is still releasing"
        );
        assert_eq!(
            v.state(&p),
            Activity::Tailing,
            "a release the amplifier hears is a tail, not live"
        );
        run(&mut v, &p, 4.0);
        assert_eq!(v.state(&p), Activity::Inert, "and it ends");
    }

    /// **Audit D13.** A tail is what the amplifier can still hear. An envelope it does not read —
    /// a long filter release that only moves the cutoff, or the amplifier's own envelope unpatched
    /// from its input — kept a silent voice tailing to the end of that release, and one it does
    /// read was left out of the samples promised to the host. In each case the verdict turns inert
    /// the settle after the last envelope the amplifier reads goes idle, the host is promised that
    /// long, the output is exactly silent from there, and **re-routing the unread envelope to the
    /// amplifier afterwards revives nothing**, whether the host kept calling or slept.
    #[test]
    fn only_an_envelope_the_amplifier_reads_holds_the_tail_open() {
        let long = [0.001, 0.05, 1.0, 2.0];
        let short = [0.001, 0.05, 1.0, 0.02];
        let settle = (POST_TAIL_S * FS) as usize;
        let mut failures = Vec::new();
        for (name, vcf_adsr, vca_adsr, reads_vcf, reads_vca) in [
            (
                "filter release on the cutoff alone",
                long,
                short,
                false,
                true,
            ),
            (
                "filter release the amplifier reads",
                long,
                short,
                true,
                true,
            ),
            (
                "amplifier envelope it does not read",
                short,
                long,
                true,
                false,
            ),
        ] {
            let mut p = Sound::with(Patch {
                vcf_adsr,
                vca_adsr,
                ..Patch::default()
            });
            depth(&mut p, target::CUTOFF, source::VCF_ADSR, 1.0);
            p.routing.clear_target(target::AMPLIFIER);
            if reads_vcf {
                p.routing.set(target::AMPLIFIER, source::VCF_ADSR, 1.0);
            }
            if reads_vca {
                p.routing.set(target::AMPLIFIER, source::VCA_ADSR, 1.0);
            }
            let heard_idle = |v: &Voice| {
                (!reads_vcf || v.vcf_env_level() == 0.0) && (!reads_vca || v.vca_env_level() == 0.0)
            };
            for host_slept in [true, false] {
                let case = format!("{name}, host slept {host_slept}");
                let mut v = voice_for(&p);
                v.note_on(id(60), 1.0, &p);
                run(&mut v, &p, 0.3);
                v.note_off(None, 0, 60, &p);
                v.play(&p);
                let promised = v.tail_samples(&p, &p.routing) as usize;
                let (mut idle_at, mut inert_at) = (None, None);
                for n in 1..(8.0 * FS) as usize {
                    if idle_at.is_none() && heard_idle(&v) {
                        idle_at = Some(n);
                    }
                    if v.state(&p) == Activity::Inert {
                        inert_at = Some(n);
                        break;
                    }
                    v.play(&p);
                }
                let (Some(idle_at), Some(inert_at)) = (idle_at, inert_at) else {
                    failures.push(format!("{case}: idle at {idle_at:?}, never inert"));
                    continue;
                };
                if inert_at.abs_diff(idle_at + settle) > 1 {
                    failures.push(format!(
                        "{case}: inert at {inert_at}, the amplifier's envelopes idle at {idle_at}"
                    ));
                }
                if promised.abs_diff(inert_at) > 64 {
                    failures.push(format!(
                        "{case}: promised {promised} samples, inert at {inert_at}"
                    ));
                }
                if !host_slept {
                    run(&mut v, &p, 0.2);
                }
                let mut rerouted = p;
                rerouted
                    .routing
                    .set(target::AMPLIFIER, source::VCF_ADSR, 1.0);
                rerouted
                    .routing
                    .set(target::AMPLIFIER, source::VCA_ADSR, 1.0);
                let peak = run(&mut v, &rerouted, 1.0);
                if peak != 0.0 || v.state(&rerouted) != Activity::Inert {
                    failures.push(format!(
                        "{case}: re-routed after inert, peak {peak}, {:?}",
                        v.state(&rerouted)
                    ));
                }
            }
        }
        assert!(failures.is_empty(), "{failures:#?}");
    }

    /// **The boundary of D13's silencing.** An envelope the amplifier does not read is silenced
    /// only when nothing can hear it and nothing but a host event can fire it again. Through an
    /// amplifier INITIAL GAIN holds open its release is heard on the cutoff; with a self-running
    /// clock on its gate it is a patch in motion; with its gate held high by the keyboard CV it is
    /// a hold, not a tail. In each its motion must be exactly what it is when the amplifier does
    /// read it.
    #[test]
    fn an_unread_envelope_that_is_heard_held_or_self_running_keeps_its_motion() {
        let base = Sound::with(Patch {
            vcf_adsr: [0.001, 0.05, 0.5, 0.5],
            vca_adsr: [0.001, 0.05, 1.0, 0.02],
            sh_rate_hz: 3.0,
            ..Patch::default()
        });
        let mut open = base;
        open.initial_gain = 0.5;
        depth(&mut open, target::CUTOFF, source::VCF_ADSR, 1.0);
        let mut clocked = base;
        patch(&mut clocked, target::VCF_GATE, source::SH_CLOCK, 1.0);
        let mut held = base;
        patch(&mut held, target::VCF_GATE, source::KEY, 1.0);
        for (name, unread, key) in [
            ("an open amplifier", open, Some(60)),
            ("a clocked gate", clocked, None),
            ("a gate held high", held, Some(96)),
        ] {
            let mut read = unread;
            read.routing.set(target::AMPLIFIER, source::VCF_ADSR, 1.0);
            let trace = |sound: &Sound| {
                let mut v = voice_for(sound);
                if let Some(note) = key {
                    v.note_on(id(note), 1.0, sound);
                    run(&mut v, sound, 0.2);
                    v.note_off(None, 0, note, sound);
                }
                (0..FS as usize)
                    .map(|_| {
                        v.play(sound);
                        v.vcf_env_level()
                    })
                    .collect::<Vec<f32>>()
            };
            let heard = trace(&unread);
            assert!(heard.iter().any(|&l| l > 0.0), "{name}: the premise");
            assert!(heard == trace(&read), "{name}: the envelope was silenced");
        }
    }

    /// **Review round 3.** A source that will not fall on its own keeps the amplifier open with
    /// no key down, and the verdict must say live, not inert.
    #[test]
    fn a_held_keyboard_cv_on_the_amplifiers_row_keeps_the_patch_live() {
        let mut p = Sound::with(Patch {
            vca_adsr: [0.001, 0.05, 1.0, 0.02],
            ..Patch::default()
        });
        patch(&mut p, target::AMPLIFIER, source::KEY, 1.0);
        let mut v = voice_for(&p);
        // A high key: the keyboard CV column holds well above zero after the release.
        v.note_on(id(96), 1.0, &p);
        run(&mut v, &p, 0.2);
        v.note_off(None, 0, 96, &p);
        run(&mut v, &p, 1.0);
        let level = (0..480).map(|_| v.play(&p).abs()).fold(0.0f32, f32::max);
        assert!(level > 1e-3, "the premise: it is still sounding: {level}");
        assert_eq!(v.state(&p), Activity::Live, "sounding with no key is live");
    }

    /// **Review round 3.** All Notes Off releases keys; a gate row patched to something still
    /// high keeps its envelope up, exactly as a key release would.
    #[test]
    fn all_notes_off_does_not_reach_past_a_patched_gate_row() {
        let mut p = Sound::with(Patch {
            vca_adsr: [0.001, 0.05, 1.0, 0.02],
            ..Patch::default()
        });
        patch(&mut p, target::VCA_GATE, source::KEY, 1.0);
        let mut v = voice_for(&p);
        v.note_on(id(96), 1.0, &p);
        run(&mut v, &p, 0.2);
        assert_eq!(v.vca_env_stage(), Stage::Sustain, "the premise");
        v.all_notes_off();
        run(&mut v, &p, 0.1);
        assert_eq!(
            v.vca_env_stage(),
            Stage::Sustain,
            "the held CV still gates the envelope; All Notes Off is not a release for it"
        );

        // While on the default gate, All Notes Off is exactly a release.
        let plain = Sound::default();
        let mut v = voice_for(&plain);
        v.note_on(id(60), 1.0, &plain);
        run(&mut v, &plain, 0.2);
        v.all_notes_off();
        run(&mut v, &plain, 0.01);
        assert!(matches!(v.vca_env_stage(), Stage::Release | Stage::Idle));
    }

    /// **Review round 3.** Volume at zero is inert whatever rings behind it.
    #[test]
    fn volume_at_zero_is_inert_even_over_a_tail() {
        let p = Sound::with(Patch {
            reverb: 1.0,
            ..Patch::default()
        });
        let mut v = voice_for(&p);
        v.note_on(id(60), 1.0, &p);
        run(&mut v, &p, 0.1);
        v.note_off(None, 0, 60, &p);
        run(&mut v, &p, 0.2);
        assert_eq!(v.state(&p), Activity::Tailing, "the premise");
        let mut muted = p;
        muted.volume = 0.0;
        v.play(&muted);
        assert_eq!(v.state(&muted), Activity::Inert);
    }

    /// **Review round 2.** Changing the priority while two keys are held moves the pitch to the
    /// key the new priority selects, and fires no envelope.
    #[test]
    fn a_priority_change_while_keys_are_held_moves_the_bus_without_a_gate_edge() {
        let low = Sound::with(Patch {
            vcf_adsr: [0.001, 0.2, 1.0, 0.1],
            vca_adsr: [0.001, 0.2, 1.0, 0.1],
            ..Patch::default()
        });
        let mut last = low;
        last.priority = Priority::Last;
        let mut v = voice_for(&low);
        v.note_on(id(48), 1.0, &low);
        v.note_on(id(60), 1.0, &low);
        run(&mut v, &low, 0.3);
        assert_eq!(
            v.keyboard_cv(),
            48.0,
            "the premise: low-note priority sounds the lower key"
        );

        run(&mut v, &last, 0.05);
        assert_eq!(
            v.keyboard_cv(),
            60.0,
            "last-note priority sounds the newer key"
        );
        assert_eq!(
            v.vca_env_stage(),
            Stage::Sustain,
            "the switch is not a gate edge: the envelope stays where it was"
        );

        run(&mut v, &low, 0.05);
        assert_eq!(v.keyboard_cv(), 48.0, "and back");
    }

    /// **Review round 2.** A repatch of a gate row is an edge when it changes the row's level:
    /// from a held key to a low column the envelope releases; from a low column back to the held
    /// key it retriggers.
    #[test]
    fn repatching_a_gate_row_to_a_different_level_is_a_gate_edge() {
        let mut held = Sound::with(Patch {
            vca_adsr: [0.001, 0.05, 1.0, 0.05],
            ..Patch::default()
        });
        let mut v = voice_for(&held);
        v.note_on(id(60), 1.0, &held);
        run(&mut v, &held, 0.2);
        assert_eq!(v.vca_env_stage(), Stage::Sustain, "the premise");

        // Middle C on the keyboard CV column is zero volts: a low gate.
        patch(&mut held, target::VCA_GATE, source::KEY, 1.0);
        run(&mut v, &held, 0.01);
        assert!(
            matches!(v.vca_env_stage(), Stage::Release | Stage::Idle),
            "the repatch took the gate low: {:?}",
            v.vca_env_stage()
        );

        patch(&mut held, target::VCA_GATE, source::GATE, 1.0);
        run(&mut v, &held, 0.2);
        assert_eq!(
            v.vca_env_stage(),
            Stage::Sustain,
            "back on the held key the gate rose again and the envelope is up"
        );
    }

    /// **Review round 2.** After CC 120 nothing is left to settle, and the verdict says so.
    #[test]
    fn all_sound_off_leaves_no_tail_to_report() {
        let p = Sound::with(Patch {
            reverb: 1.0,
            vca_adsr: [0.002, 0.2, 0.0, 0.05],
            ..Patch::default()
        });
        let mut v = voice_for(&p);
        v.note_on(id(60), 1.0, &p);
        run(&mut v, &p, 0.1);
        v.note_off(None, 0, 60, &p);
        run(&mut v, &p, 0.3);
        assert_eq!(
            v.state(&p),
            Activity::Tailing,
            "the premise: the spring is ringing"
        );
        v.all_sound_off();
        v.play(&p);
        assert_eq!(
            v.state(&p),
            Activity::Inert,
            "cleared means nothing to report"
        );
    }

    /// **Review round 2.** A host-supplied NaN bend or expression reaches the pitch as nothing.
    #[test]
    fn a_non_finite_bend_or_expression_leaves_the_output_finite() {
        let p = Sound::with(Patch {
            bend_semitones: f32::NAN,
            expression_semitones: f32::INFINITY,
            ..Patch::default()
        });
        let mut v = voice_for(&p);
        v.note_on(id(60), 1.0, &p);
        for _ in 0..4_800 {
            assert!(v.play(&p).is_finite());
        }
    }

    /// **Review round 1.** CC 120 is silence *now*: the spring's tail does not survive it.
    #[test]
    fn all_sound_off_silences_the_effects_tails_at_once() {
        let p = Sound::with(Patch {
            reverb: 1.0,
            delay_level: 0.8,
            vca_adsr: [0.002, 0.2, 0.0, 0.05],
            ..Patch::default()
        });
        let mut v = voice_for(&p);
        v.note_on(id(60), 1.0, &p);
        run(&mut v, &p, 0.1);
        v.note_off(None, 0, 60, &p);
        run(&mut v, &p, 0.3);
        let ringing = (0..480).map(|_| v.play(&p).abs()).fold(0.0f32, f32::max);
        assert!(
            ringing > 1e-3,
            "the premise: the tail is audible: {ringing}"
        );

        v.all_sound_off();
        for i in 0..4_800 {
            assert_eq!(
                v.play(&p),
                0.0,
                "sample {i} after All Sound Off is not silent"
            );
        }
    }

    /// **Review round 1.** Negative key follow closes the filter as the pitch rises: the
    /// plug-out's bipolar control, which the parameter now reaches.
    #[test]
    fn negative_key_follow_closes_the_filter_above_middle_c() {
        let at = |key_track: f32| {
            let mut p = Sound::with(Patch {
                cutoff_hz: 1_000.0,
                ..Patch::default()
            });
            // The retired KYBD CV slider is the cutoff input's keyboard route, wired at Init.
            depth(&mut p, target::CUTOFF, source::KEY, key_track);
            // The fifth harmonic of C6, at 5.2 kHz: above the cutoff, so what the filter does
            // to it is what key follow did.
            tone_db(&p, 84, 5.0 * 1046.5)
        };
        let flat = at(0.0);
        let up = at(1.0);
        let down = at(-1.0);
        assert!(
            up > flat + 6.0,
            "positive follow opens: {up} against {flat}"
        );
        assert!(
            down < flat - 6.0,
            "negative follow closes: {down} against {flat}"
        );
    }

    /// No dead band: the moment the control passes the singing point, silence rings up. The copy
    /// switched its excitation on at 0.9 with the threshold at 0.87, so from 0.87 to 0.9 the loop
    /// was past its threshold with nothing to start it — "it seems to need to be above 90 %".
    #[test]
    fn the_filter_rings_up_from_silence_just_past_the_singing_point() {
        let fs = 48_000.0;
        let singing_point = 1.0 / crate::filter::RESONANCE_MARGIN;
        let p = Sound::with(Patch {
            level_vco1: 0.0,
            resonance: singing_point + 0.02,
            cutoff_hz: 1_000.0,
            vca_adsr: [0.002, 0.3, 1.0, 0.2],
            ..Patch::default()
        });
        let mut v = voice_for(&p);
        v.note_on(id(60), 1.0, &p);
        let mut out = Vec::new();
        for _ in 0..(fs * 4.0) as usize {
            out.push(v.play(&p));
        }
        let last = &out[out.len() - fs as usize..];
        let rms = (last.iter().map(|x| x * x).sum::<f32>() / last.len() as f32).sqrt();
        assert!(
            rms > 0.01,
            "two percent past the singing point the filter should ring up within four seconds: rms {rms}"
        );

        // And two percent below it, nothing — the excitation is inaudible and the loop decays.
        let mut below = p;
        below.resonance = singing_point - 0.02;
        let mut v = voice_for(&below);
        v.note_on(id(60), 1.0, &below);
        let mut peak = 0.0f32;
        for _ in 0..(fs * 4.0) as usize {
            peak = peak.max(v.play(&below).abs());
        }
        assert!(
            peak < 1e-3,
            "below the singing point the filter is silent: peak {peak}"
        );
    }

    /// The hardware's resonance runs "0 → self-oscillation", singing near 8 on its scale, and the
    /// model's feedback reaches `RESONANCE_MARGIN` past its threshold. With nothing feeding the
    /// ladder and a key held, the filter must ring up from the excitation on its own — a sine,
    /// about +128 cents above the nominal cutoff for the chosen pole set.
    #[test]
    fn the_filter_self_oscillates_at_the_top_of_the_resonance_with_nothing_feeding_it() {
        let fs = 48_000.0;
        // Every octave of the control, the init patch's 10 kHz included — the cutoff at which
        // the owner found it silent — up to where the sung pitch nears the base Nyquist and the
        // decimator takes it, as the hardware's 21 kHz oscillation is past hearing.
        for cutoff in [60.0f32, 440.0, 2_000.0, 5_000.0, 10_000.0, 14_000.0] {
            let p = Sound::with(Patch {
                level_vco1: 0.0,
                resonance: 1.0,
                cutoff_hz: cutoff,
                vca_adsr: [0.002, 0.3, 1.0, 0.2],
                ..Patch::default()
            });
            let mut v = voice_for(&p);
            v.note_on(id(60), 1.0, &p);
            let mut out = Vec::new();
            for _ in 0..(fs * 3.0) as usize {
                out.push(v.play(&p));
            }
            let last = &out[out.len() - fs as usize..];
            let rms = (last.iter().map(|x| x * x).sum::<f32>() / last.len() as f32).sqrt();
            assert!(
                rms > 0.05,
                "at {cutoff} Hz the filter should be singing after three seconds: rms {rms}"
            );

            // A sine's pitch, by rising zero crossings over the last second, against where the
            // ladder says it sings — +128 cents low down, warping upward toward the clamp.
            let crossings = last
                .windows(2)
                .filter(|w| w[0] < 0.0 && w[1] >= 0.0)
                .count();
            let sung_hz = crossings as f32 / (last.len() as f32 / fs);
            let expected = v.filter.oscillation_hz(cutoff, 2.0 * fs);
            let cents = 1200.0 * (sung_hz / expected).log2();
            assert!(
                cents.abs() < 60.0,
                "at {cutoff} Hz: sings at {sung_hz} Hz, {cents:.0} cents from the predicted {expected}"
            );
        }
    }

    #[test]
    fn two_instances_render_identically() {
        let mut p = Sound::with(Patch {
            level_noise: 0.5,
            resonance: 0.95,
            ..Patch::default()
        });
        patch(&mut p, target::SH_INPUT, source::LFO1_CORE_SAW, 1.0);
        p.level_ring = 0.5;
        patch(&mut p, target::VCO1_PITCH, source::SH_OUT, 0.2);
        assert_eq!(render(&p, Some(50), 24_000), render(&p, Some(50), 24_000));
    }

    #[test]
    fn reset_leaves_no_tail() {
        let mut p = Sound::with(Patch {
            portamento_s: 1.0,
            ..Patch::default()
        });
        // The retired glide slider at full, on both oscillators as DESTINATION's default had it.
        let full_glide = glided(Patch::default(), 1.0).routing;
        p.routing = full_glide;
        let mut v = voice_for(&p);
        v.note_on(id(60), 1.0, &p);
        run(&mut v, &p, 0.3);
        v.reset();
        assert_eq!(v.state(&p), Activity::Inert);
        assert_eq!(v.glide_charge(), 0.0);
        for _ in 0..5_000 {
            assert_eq!(v.play(&p), 0.0, "state survived reset");
        }
    }

    /// The sample rates a CLAP validator tries, 1 kHz to 768 kHz with a fractional one: every
    /// corner in the crate must be clamped below Nyquist, or a coefficient goes wrong and a
    /// module nobody turned up poisons the mix — `NaN * 0` is `NaN`. Found by clap-validator's
    /// `process-varying-sample-rates` at 1234.57 Hz, in the VCA's tilt and the noise's shelves.
    #[test]
    fn the_init_patch_is_finite_at_every_sample_rate_a_validator_tries() {
        for fs in [
            1_000.0f32, 1_234.57, 8_000.0, 22_050.0, 48_000.0, 384_000.0, 768_000.0,
        ] {
            let mut v = Voice::new();
            v.set_sample_rate(fs);
            let p = Sound::with(Patch {
                level_noise: 0.5,
                tone: 0.5,
                phaser: 0.5,
                delay_level: 0.5,
                reverb: 0.5,
                ..Patch::default()
            });
            v.set_topology(&p.routing);
            v.note_on(id(60), 1.0, &p);
            for i in 0..(fs * 0.3) as usize {
                let y = v.play(&p);
                assert!(y.is_finite(), "non-finite {y} at sample {i} at {fs} Hz");
            }
            v.note_off(None, 0, 60, &p);
            for i in 0..(fs * 0.3) as usize {
                let y = v.play(&p);
                assert!(
                    y.is_finite(),
                    "non-finite {y} after release at sample {i} at {fs} Hz"
                );
            }
        }
    }

    #[test]
    fn output_is_finite_and_bounded_under_extreme_settings_and_cycles() {
        for fs in [
            1_234.57f32,
            44_100.0,
            48_000.0,
            96_000.0,
            192_000.0,
            768_000.0,
        ] {
            let mut v = Voice::new();
            v.set_sample_rate(fs);
            let mut p = Sound::with(Patch {
                level_vco1: 1.0,
                level_vco2: 1.0,
                level_noise: 1.0,
                sync_strength: SyncStrength::Weak,
                wave2: Wave::Square,
                lfo2_rate_hz: 25.0,
                sh_rate_hz: 125.0,
                resonance: 1.0,
                lfo1_rate_hz: 25.0,
                lfo1_shape: Shape::Square,
                wave: Wave::Square,
                initial_gain: 1.0,
                tone: 1.0,
                vcf_adsr: [0.0004, 0.0008, 1.0, 0.0008],
                vca_adsr: [0.0004, 0.0008, 1.0, 0.0008],
                volume: 2.0,
                ..Patch::default()
            });
            // Every route at full depth, and every cycle the patch bay can close at once: the
            // mixer into itself, VCO-1 into its own CV, the LFOs into each other's rate, the S&H
            // into the S&H, the sync from the ring. This is the extremes test, so the two filter
            // inputs and the amplifier are held at full as well.
            depth(&mut p, target::CUTOFF, source::VCF_ADSR, 1.0);
            depth(&mut p, target::CUTOFF, source::LFO1, 1.0);
            p.level_ring = 1.0;
            patch(&mut p, target::MIXER_INPUT, source::MIXER_OUT, 1.0);
            patch(&mut p, target::VCO1_PITCH, source::VCO1, 1.0);
            patch(&mut p, target::LFO1_RATE, source::LFO2, 1.0);
            patch(&mut p, target::LFO2_RATE, source::LFO1, 1.0);
            patch(&mut p, target::SH_INPUT, source::SH_OUT, 1.0);
            patch(&mut p, target::VCO2_SYNC, source::RING_MOD, 1.0);
            p.routing.set(target::CUTOFF, source::MIXER_OUT, 1.0);
            patch(&mut p, target::VCA_GATE, source::VCO2, 1.0);
            // The panel inputs the plug-out hard-wired, every one at full: the pitch inputs with
            // the retired vibrato and glide beside an audio source, both widths, the tremolo and
            // the keyboard on the cutoff.
            p.routing.set(target::VCO1_PITCH, source::LFO1, 1.0);
            p.routing.set(target::VCO1_PITCH, source::GLIDE, -1.0);
            patch(&mut p, target::VCO2_PITCH, source::NOISE, 1.0);
            p.routing.set(target::VCO2_PITCH, source::LFO1, 1.0);
            p.routing.set(target::VCO2_PITCH, source::VCO1, 1.0);
            patch(&mut p, target::VCO1_WIDTH, source::LFO1_CORE_TRIANGLE, 1.0);
            patch(&mut p, target::VCO2_WIDTH, source::LFO2_CORE_TRIANGLE, 1.0);
            p.routing.set(target::VCO2_WIDTH, source::MIXER_OUT, -1.0);
            patch(&mut p, target::TREMOLO, source::LFO1, 1.0);
            p.routing.set(target::CUTOFF, source::KEY, 1.0);
            p.routing.set(target::CUTOFF, source::LFO1_CORE_SINE, 1.0);
            // The standard Amplitude at its top while a key is down: the VCA's output doubled.
            patch(&mut p, target::AMPLITUDE, source::GATE, 1.0);
            v.set_topology(&p.routing);
            for note in [0u8, 60, 127] {
                v.note_on(id(note), 1.0, &p);
                for _ in 0..(fs as usize / 10) {
                    let y = v.play(&p);
                    assert!(y.is_finite(), "non-finite at note {note}, {fs} Hz");
                    assert!(y.abs() <= OUTPUT_BOUND * 2.0, "runaway output {y}");
                }
                v.note_off(None, 0, note, &p);
            }
        }
    }

    #[test]
    fn the_bass_survives_resonance_to_the_chosen_slight_decline() {
        let fundamental_at = |resonance: f32| {
            let p = Sound::with(Patch {
                resonance,
                cutoff_hz: 2_000.0,
                vca_adsr: [0.001, 0.1, 1.0, 0.1],
                ..Patch::default()
            });
            let mut v = voice_for(&p);
            v.note_on(id(36), 1.0, &p);
            run(&mut v, &p, 0.5);
            let hz = 440.0 * ((36.0 - 69.0) / 12.0f32).exp2();
            let cycles = 40.0;
            let n = (cycles * FS / hz) as usize;
            let (mut re, mut im) = (0.0f64, 0.0f64);
            for i in 0..n {
                let y = v.play(&p) as f64;
                let ang = std::f64::consts::TAU * hz as f64 * i as f64 / FS as f64;
                re += y * ang.cos();
                im += y * ang.sin();
            }
            ((re * re + im * im).sqrt() * 2.0 / n as f64) as f32
        };
        let flat = fundamental_at(0.0);
        let resonant = fundamental_at(0.85);
        let decline_db = 20.0 * (resonant / flat).log10();
        assert!(
            decline_db < 0.0,
            "resonance should cost a little bass, gained {decline_db:.1} dB"
        );
        assert!(
            decline_db > -6.0,
            "the bass collapsed by {decline_db:.1} dB, which is not slight"
        );
    }

    /// The level of one frequency in a render, in dB, by a Hann-windowed correlation.
    fn tone_db(p: &Sound, note: u8, hz: f32) -> f32 {
        let mut v = voice_for(p);
        v.note_on(id(note), 1.0, p);
        for _ in 0..(FS as usize / 5) {
            v.play(p);
        }
        let n = (FS * 1.0) as usize;
        let (mut re, mut im) = (0.0f64, 0.0f64);
        for i in 0..n {
            let y = v.play(p) as f64;
            let w = 0.5 - 0.5 * (std::f64::consts::TAU * i as f64 / n as f64).cos();
            let ang = std::f64::consts::TAU * hz as f64 * i as f64 / FS as f64;
            re += y * w * ang.cos();
            im += y * w * ang.sin();
        }
        20.0 * (((re * re + im * im).sqrt() * 4.0 / n as f64).max(1e-12)).log10() as f32
    }

    /// The fundamental of a rendered patch, by interpolated rising zero crossings after settling.
    fn fundamental_hz(p: &Sound, note: u8, secs: f32) -> f32 {
        let mut v = voice_for(p);
        v.note_on(id(note), 1.0, p);
        for _ in 0..(FS as usize / 5) {
            v.play(p);
        }
        let n = (FS * secs) as usize;
        let (mut first, mut last, mut count) = (None::<f64>, 0.0f64, 0usize);
        let mut prev = v.play(p);
        for i in 1..n {
            let y = v.play(p);
            if prev <= 0.0 && y > 0.0 {
                let frac = (-prev / (y - prev)) as f64;
                let t = (i as f64 - 1.0 + frac) / FS as f64;
                if first.is_none() {
                    first = Some(t);
                } else {
                    last = t;
                    count += 1;
                }
            }
            prev = y;
        }
        (count as f64 / (last - first.expect("no crossings"))) as f32
    }

    #[test]
    fn vco2_tunes_in_semitones_and_cents_and_starts_slightly_detuned() {
        let only_vco2 = Sound::with(Patch {
            level_vco1: 0.0,
            level_vco2: 0.8,
            cutoff_hz: 20_000.0,
            vca_adsr: [0.001, 0.001, 1.0, 0.1],
            ..Patch::default()
        });
        let detuned = fundamental_hz(&only_vco2, 69, 2.0);
        let cents = 1200.0 * (detuned / 440.0).log2();
        assert!(
            (cents - 7.0).abs() < 2.0,
            "the init detune: {cents:+.1} cents"
        );
        let mut fifth = only_vco2;
        fifth.coarse_semitones2 = 7.0;
        fifth.fine_cents2 = 0.0;
        let want = 440.0 * 2f32.powf(7.0 / 12.0);
        let got = fundamental_hz(&fifth, 69, 2.0);
        assert!(
            (1200.0 * (got / want).log2()).abs() < 2.0,
            "coarse: {got} Hz for {want}"
        );
    }

    #[test]
    fn the_ring_modulator_sounds_only_through_its_level_and_is_not_a_vco() {
        let mut ring_only = Sound::with(Patch {
            level_vco1: 0.0,
            coarse_semitones2: 11.0,
            fine_cents2: 0.0,
            cutoff_hz: 20_000.0,
            vca_adsr: [0.001, 0.001, 1.0, 0.1],
            ..Patch::default()
        });
        // The ring modulator's own mixer level, the 102's RING MOD slider.
        ring_only.level_ring = 0.8;
        let mut v = voice_for(&ring_only);
        v.note_on(id(60), 1.0, &ring_only);
        assert!(
            run(&mut v, &ring_only, 0.5) > 0.05,
            "the ring modulator is silent"
        );
        let f1 = 440.0 * 2f32.powf((60.0 - 69.0) / 12.0);
        let f2 = f1 * 2f32.powf(11.0 / 12.0);
        let difference = tone_db(&ring_only, 60, f2 - f1);
        let carrier = tone_db(&ring_only, 60, f1);
        assert!(
            difference > carrier + 20.0,
            "difference tone {difference:.1} dB against VCO-1's fundamental {carrier:.1} dB"
        );
    }

    #[test]
    fn strong_sync_pins_vco2_to_vco1_and_removing_the_route_takes_it_off() {
        let mut p = Sound::with(Patch {
            level_vco1: 0.0,
            level_vco2: 0.8,
            coarse_semitones2: 9.0,
            fine_cents2: 20.0,
            sync_strength: SyncStrength::Strong,
            cutoff_hz: 20_000.0,
            vca_adsr: [0.001, 0.001, 1.0, 0.1],
            ..Patch::default()
        });
        // **The route's presence is the SYNC switch.** The plug-out normals SYNC IN to VCO-1's
        // SYNC OUT and its switch decided whether the circuit acted; under pairs those are one
        // thing, so wiring the normal is turning sync on, at full reset depth.
        patch(&mut p, target::VCO2_SYNC, source::VCO1_SYNC, 1.0);
        let f1 = 440.0 * 2f32.powf((57.0 - 69.0) / 12.0);
        let f2 = f1 * 2f32.powf(9.2 / 12.0);
        let at_f1 = tone_db(&p, 57, f1);
        let at_f2 = tone_db(&p, 57, f2);
        assert!(
            at_f1 > at_f2 + 20.0,
            "synced: {at_f1:.1} dB at f1, {at_f2:.1} dB at VCO-2's own"
        );
        let mut free = p;
        free.routing.clear_target(target::VCO2_SYNC);
        let free_f2 = tone_db(&free, 57, f2);
        let free_f1 = tone_db(&free, 57, f1);
        assert!(
            free_f2 > free_f1 + 20.0,
            "unsynced: {free_f2:.1} dB at f2, {free_f1:.1} dB at f1"
        );
    }

    #[test]
    fn the_modulator_input_lets_vco2_frequency_modulate_vco1() {
        let plain = Sound::with(Patch {
            cutoff_hz: 20_000.0,
            vca_adsr: [0.001, 0.001, 1.0, 0.1],
            coarse_semitones2: 5.0,
            ..Patch::default()
        });
        // VCO-1's modulator input is normalled to VCO-2 in the init patch; this is its depth.
        let modulated = with_patch(&plain, target::VCO1_PITCH, source::VCO2, 0.3);
        let f1 = fundamental_hz(&plain, 48, 1.0);
        let fm = fundamental_hz(&modulated, 48, 1.0);
        assert!(
            (fm - f1).abs() > f1 * 0.05,
            "EXT CV changed nothing: {f1} vs {fm}"
        );
    }

    #[test]
    fn lfo2_reaches_the_pulse_width_and_nothing_else_by_default() {
        let base = Sound::with(Patch {
            wave: Wave::Square,
            lfo2_rate_hz: 3.0,
            ..Patch::default()
        });
        let a = render(&base, Some(48), 24_000);
        let mut shaped = base;
        shaped.lfo2_shape = Shape::Saw;
        let b = render(&shaped, Some(48), 24_000);
        assert_eq!(a, b, "LFO-2 reached something it is not routed to");
        let from_lfo2 = with_patch(&base, target::VCO1_WIDTH, source::LFO2_CORE_TRIANGLE, 1.0);
        let c = render(&from_lfo2, Some(48), 24_000);
        assert_ne!(a, c, "PWM from LFO-2 changed nothing");
    }

    // ---- The patch bay --------------------------------------------------------------------

    /// **A sync route's detector must not carry a stale `prev` across an absence.**
    ///
    /// An `EdgeRow` that is not fed keeps the value it had when it last was, so a route removed
    /// while its source is high and re-added while it is low compares this sample against one from
    /// a previous phrase: the crossing it reports is imaginary, and *whether* it reports one
    /// depends on how long the route was gone — which makes a patch's first synced cycle depend on
    /// the host's buffer sizes. It is the same defect `mxm_modulation::SourceFrame::clear` exists
    /// for, and it is checked here rather than through the audio because the state is the subject.
    ///
    /// **Falsified**: without `Voice::set_topology`'s reset, the second assertion goes red — the
    /// detector still holds the high it saw before the route was pulled.
    #[test]
    fn a_newly_live_sync_detector_starts_from_zero_rather_than_an_ancient_value() {
        let mut wired = Routing::init();
        wired.set(target::VCO2_SYNC, source::GATE, 1.0);
        let mut v = unarmed_voice();
        v.set_topology(&wired);

        // Drive the detector high through a held gate, so it has something stale to keep.
        let sound = Sound {
            patch: Patch::default(),
            routing: wired,
        };
        v.note_on(id(60), 1.0, &sound.patch);
        run(&mut v, &sound, 0.05);
        assert!(
            v.sync_edges[source::GATE].last() > 0.0,
            "the premise: the detector has seen the gate high"
        );

        // Pull the route, leave it out, and put it back.
        let mut gone = wired;
        gone.clear(target::VCO2_SYNC, source::GATE);
        v.set_topology(&gone);
        v.set_topology(&wired);
        assert_eq!(
            v.sync_edges[source::GATE].last(),
            0.0,
            "a re-added sync route kept the detector's ancient value"
        );

        // And a route that stays live is not reset under it: only the transition owes this.
        run(&mut v, &sound, 0.05);
        let carried = v.sync_edges[source::GATE].last();
        v.set_topology(&wired);
        assert_eq!(
            v.sync_edges[source::GATE].last(),
            carried,
            "a live sync route's detector was reset by an unrelated topology pass"
        );
    }

    /// **The plug-out's internal connections are the init patch, and this is where that is
    /// checked against the voice rather than against `routing.rs`'s own tables.**
    ///
    /// Before the conversion a row's `Default` step named its normalled source and the DSP looked
    /// the connection up; there is no `Default` to select now, so the property that replaces
    /// *selecting the default explicitly is the same sound* is that a fresh `Patch` already holds
    /// exactly those routes. Wiring one of them again by hand must therefore change nothing at
    /// all.
    #[test]
    fn a_fresh_patch_is_already_wired_the_way_the_plug_out_was() {
        let base = Sound::with(Patch {
            level_vco2: 0.5,
            level_noise: 0.2,
            resonance: 0.5,
            cutoff_hz: 800.0,
            ..Patch::default()
        });
        for &(t, sc) in routing::INIT_PRESENT.iter() {
            assert!(base.routing.present[t][sc]);
            let again = with_patch(&base, t, sc, base.routing.amounts[t][sc]);
            assert_eq!(
                render(&base, Some(48), 12_000),
                render(&again, Some(48), 12_000),
                "{}: wiring the normal again is not the same sound",
                routing::TARGET_NAMES[t]
            );
        }
        for &(t, sc) in routing::INIT_AT_FULL.iter() {
            assert!(base.routing.present[t][sc]);
            assert_eq!(base.routing.amounts[t][sc], 1.0);
            let again = with_patch(&base, t, sc, 1.0);
            assert_eq!(
                render(&base, Some(48), 12_000),
                render(&again, Some(48), 12_000),
                "{}: wiring the normal again is not the same sound",
                routing::TARGET_NAMES[t]
            );
        }
    }

    /// **Removing a route leaves its depth behind, so putting the source back restores the
    /// sound** — the shared crate's contract, heard rather than asserted at the array.
    #[test]
    fn removing_a_source_and_adding_it_back_restores_the_render_exactly() {
        let mut base = Sound::with(Patch {
            cutoff_hz: 600.0,
            lfo1_rate_hz: 3.0,
            lfo2_rate_hz: 11.0,
            ..Patch::default()
        });
        depth(&mut base, target::CUTOFF, source::LFO1, 0.6);
        let a = render(&base, Some(48), 24_000);

        let mut gone = base;
        gone.routing.clear(target::CUTOFF, source::LFO1);
        assert_ne!(
            a,
            render(&gone, Some(48), 24_000),
            "removal changed nothing"
        );
        assert_eq!(
            gone.routing.amounts[target::CUTOFF][source::LFO1],
            0.6,
            "the depth is deliberately left alone"
        );

        let mut back = gone;
        back.routing.present[target::CUTOFF][source::LFO1] = true;
        assert_eq!(a, render(&back, Some(48), 24_000));
    }

    /// **Decision 1.6, heard.** The switching jack summed nothing; the input is a mixer now, and
    /// two sources on one input are louder than either alone.
    #[test]
    fn two_sources_on_one_input_add_where_the_jack_took_only_the_last() {
        let base = Sound::with(Patch {
            cutoff_hz: 400.0,
            resonance: 0.2,
            lfo1_rate_hz: 3.0,
            lfo2_rate_hz: 3.0,
            ..Patch::default()
        });
        let swing = |p: &Sound| {
            let mut v = voice_for(p);
            v.note_on(id(48), 1.0, p);
            let mut lo = f32::INFINITY;
            let mut hi = f32::NEG_INFINITY;
            for _ in 0..24_000 {
                let y = v.play(p);
                lo = lo.min(y);
                hi = hi.max(y);
            }
            hi - lo
        };
        let one = with_patch(&base, target::CUTOFF, source::LFO1, 0.3);
        let mut both = one;
        both.routing.set(target::CUTOFF, source::LFO2, 0.3);
        assert!(
            swing(&both) > swing(&one),
            "two half-depth sources on one input did not add"
        );
    }

    #[test]
    fn a_gate_row_patched_to_a_column_takes_the_keyboard_away_from_that_envelope() {
        // The plug breaks the keyboard gate (§4.2): with the VCA's gate on LFO-1, a key press
        // does not trigger the VCA envelope, and the LFO does — with no key at all.
        let mut p = Sound::with(Patch {
            lfo1_rate_hz: 4.0,
            lfo1_shape: Shape::Square,
            vca_adsr: [0.001, 0.05, 0.0, 0.03],
            ..Patch::default()
        });
        patch(&mut p, target::VCA_GATE, source::LFO1, 1.0);
        let mut v = voice_for(&p);
        // No key: the LFO's square gates the envelope at 4 Hz.
        let mut attacks = 0;
        let mut was = false;
        for _ in 0..(FS as usize) {
            v.play(&p);
            let now = v.vca_env_stage() == Stage::Attack;
            if now && !was {
                attacks += 1;
            }
            was = now;
        }
        assert!(
            (3..=5).contains(&attacks),
            "{attacks} attacks from the LFO with no key"
        );
        assert_eq!(
            v.state(&p),
            Activity::Live,
            "a self-gated amplifier is a live patch"
        );
        // A key press between the LFO's edges changes nothing about the VCA envelope.
        // A 3.3 s half-period: low for a long time after its first half.
        let mut square_low = p;
        square_low.lfo1_rate_hz = 0.15;
        let mut w = voice_for(&square_low);
        run(&mut w, &square_low, 3.5); // now in the low half
        let before = w.vca_env_stage();
        w.note_on(id(60), 1.0, &square_low);
        w.play(&square_low);
        assert_eq!(
            w.vca_env_stage(),
            before,
            "the keyboard reached an envelope whose gate is patched"
        );
    }

    #[test]
    fn a_slow_clock_on_the_gate_row_keeps_the_patch_live_across_silence() {
        // The plan's slow-clock case: silent for longer than the player's eight-buffer sleep
        // window, then sounding, and reporting live throughout so the host never sleeps it.
        let mut p = Sound::with(Patch {
            sh_rate_hz: 1.0,
            vca_adsr: [0.001, 0.05, 0.0, 0.02],
            ..Patch::default()
        });
        patch(&mut p, target::SH_INPUT, source::LFO1_CORE_SAW, 1.0);
        patch(&mut p, target::VCA_GATE, source::SH_CLOCK, 1.0);
        let mut v = voice_for(&p);
        assert_eq!(v.state(&p), Activity::Live);
        let out: Vec<f32> = (0..(FS as usize * 2)).map(|_| v.play(&p)).collect();
        // Some stretch longer than eight 512-sample buffers is exactly silent…
        let window = 8 * 512;
        let quiet = out.windows(window).any(|w| w.iter().all(|&y| y == 0.0));
        assert!(quiet, "the patch never went quiet for a sleep window");
        // …and the patch sounds again afterwards, more than once.
        let mut bursts = 0;
        let mut was_loud = false;
        for y in &out {
            let loud = y.abs() > 0.01;
            if loud && !was_loud {
                bursts += 1;
            }
            was_loud = loud;
        }
        assert!(bursts >= 2, "{bursts} bursts in two seconds at 1 Hz");
        assert_eq!(v.state(&p), Activity::Live);
    }

    #[test]
    fn the_sample_and_hold_into_the_vco_steps_the_pitch_at_the_clock() {
        let mut p = Sound::with(Patch {
            sh_rate_hz: 8.0,
            lfo1_rate_hz: 0.7,
            cutoff_hz: 20_000.0,
            vca_adsr: [0.001, 0.001, 1.0, 0.1],
            ..Patch::default()
        });
        patch(&mut p, target::SH_INPUT, source::LFO1_CORE_SAW, 1.0);
        patch(&mut p, target::VCO1_PITCH, source::SH_OUT, 0.1);
        let mut v = voice_for(&p);
        v.note_on(id(48), 1.0, &p);
        // The column steps exactly at the clock and holds between.
        let mut prev = v.column(Column::ShOut);
        let mut steps = 0;
        for _ in 0..(FS as usize) {
            v.play(&p);
            let now = v.column(Column::ShOut);
            if now != prev {
                steps += 1;
            }
            prev = now;
        }
        assert!(
            (7..=9).contains(&steps),
            "{steps} steps in a second at 8 Hz"
        );
    }

    #[test]
    fn a_vco_into_an_lfos_rate_input_is_fm_on_a_modulation_source() {
        // Roland's article's trick: VCO-2 into LFO-1 EXT CV IN. The row's one GAIN knob is the
        // route's own amount now.
        let mut base = Sound::default();
        // A vibrato, as the retired VCO LFO slider at 0.3 had it.
        depth(
            &mut base,
            target::VCO1_PITCH,
            source::LFO1,
            0.3 * VCO_LFO_SEMITONES / routing::PITCH_SEMITONES_PER_UNIT,
        );
        let patched = with_patch(&base, target::LFO1_RATE, source::VCO2, 0.5);
        let a = render(&base, Some(48), 12_000);
        let b = render(&patched, Some(48), 12_000);
        assert_ne!(a, b);
        assert!(b.iter().all(|y| y.is_finite()));
    }

    #[test]
    fn an_audio_route_change_mid_note_does_not_click() {
        // The two audio inputs ramp each route's own contribution: the largest per-sample step
        // across the change is no larger than the material's own. An instant swap of uncorrelated
        // audio would jump.
        let mut p = Sound::with(Patch {
            level_vco1: 0.0,
            coarse_semitones2: 11.0,
            cutoff_hz: 20_000.0,
            vca_adsr: [0.001, 0.001, 1.0, 0.1],
            ..Patch::default()
        });
        p.routing.set(target::MIXER_INPUT, source::RING_MOD, 0.8);
        let mut v = voice_for(&p);
        v.note_on(id(40), 1.0, &p);
        run(&mut v, &p, 0.3);
        let mut worst_before = 0.0f32;
        let mut prev = v.play(&p);
        for _ in 0..2_000 {
            let y = v.play(&p);
            worst_before = worst_before.max((y - prev).abs());
            prev = y;
        }
        patch(&mut p, target::MIXER_INPUT, source::NOISE, 1.0);
        let mut worst_after = 0.0f32;
        for _ in 0..2_000 {
            let y = v.play(&p);
            worst_after = worst_after.max((y - prev).abs());
            prev = y;
        }
        assert!(
            worst_after < worst_before * 3.0 + 0.05,
            "the switch jumped {worst_after} against the material's {worst_before}"
        );
    }

    #[test]
    fn the_effects_extend_the_tail_and_the_patch_still_reaches_exact_zero_and_inert() {
        let p = Sound::with(Patch {
            phaser: 0.6,
            delay_level: 0.5,
            delay_time_s: 0.2,
            reverb: 0.5,
            vca_adsr: [0.005, 0.1, 0.5, 0.05],
            ..Patch::default()
        });
        let mut v = voice_for(&p);
        v.note_on(id(60), 1.0, &p);
        run(&mut v, &p, 0.3);
        v.note_off(None, 0, 60, &p);
        // Well after the envelope's release the reverb is still tailing, and the voice says so.
        run(&mut v, &p, 1.0);
        assert_eq!(
            v.state(&p),
            Activity::Tailing,
            "the reverb's tail should keep it tailing"
        );
        assert!(
            v.tail_samples(&p, &p.routing) as f32 > 3.0 * FS,
            "the tail should be the reverb's seconds"
        );
        // And after the reverb's T60-based tail, exact zero and inert.
        run(&mut v, &p, 6.0);
        assert_eq!(v.state(&p), Activity::Inert);
        for _ in 0..1_000 {
            assert_eq!(v.play(&p), 0.0);
        }
    }

    #[test]
    fn the_phaser_inputs_reach_the_phaser() {
        let mut base = Sound::with(Patch {
            phaser: 0.5,
            ..Patch::default()
        });
        let a = render(&base, Some(48), 24_000);
        patch(&mut base, target::PHASER_CENTRE, source::LFO2, 1.0);
        let b = render(&base, Some(48), 24_000);
        assert_ne!(a, b, "MANUAL IN from LFO-2 changed nothing");
        base.routing.clear_target(target::PHASER_CENTRE);
        patch(&mut base, target::PHASER_RATE, source::LFO1, 1.0);
        let c = render(&base, Some(48), 24_000);
        assert_ne!(a, c, "LFO IN from LFO-1 changed nothing");
    }

    #[test]
    fn the_mixer_into_its_own_external_input_is_a_bounded_deterministic_loop() {
        let mut p = Sound::with(Patch {
            resonance: 0.3,
            ..Patch::default()
        });
        patch(&mut p, target::MIXER_INPUT, source::MIXER_OUT, 1.0);
        let a = render(&p, Some(48), 24_000);
        let b = render(&p, Some(48), 24_000);
        assert_eq!(a, b);
        assert!(a.iter().all(|y| y.is_finite() && y.abs() <= OUTPUT_BOUND));
        assert!(
            a.iter().any(|y| y.abs() > 0.01),
            "the loop should still sound"
        );
    }
}

#[cfg(test)]
mod pitch_tests {
    use super::tests::{Play, Sound};
    use super::*;

    const FS: f32 = 48_000.0;

    fn measure_note(note: u8, range_offset: f32, wave: Wave) -> f32 {
        let mut v = Voice::new();
        v.set_sample_rate(FS);
        let p = Sound::with(Patch {
            range_offset,
            wave,
            cutoff_hz: 20_000.0,
            vca_adsr: [0.001, 0.001, 1.0, 0.1],
            ..Patch::default()
        });
        v.set_topology(&p.routing);
        v.note_on(
            NoteId {
                voice_id: None,
                channel: 0,
                note,
            },
            1.0,
            &p,
        );
        for _ in 0..(FS as usize / 10) {
            v.play(&p);
        }
        let n = (FS * 4.0) as usize;
        let (mut first, mut last, mut count) = (None::<f64>, 0.0f64, 0usize);
        let mut prev = 0.0f32;
        for i in 0..n {
            let y = v.play(&p);
            if prev <= 0.0 && y > 0.0 {
                let frac = (-prev / (y - prev)) as f64;
                let t = (i as f64 - 1.0 + frac) / FS as f64;
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

    fn expected(note: u8, semitones: f32) -> f32 {
        440.0 * ((note as f32 + semitones - 69.0) / 12.0).exp2()
    }

    #[test]
    fn the_voice_plays_the_note_it_is_given_on_every_waveform() {
        for wave in [Wave::Saw, Wave::Square, Wave::Triangle] {
            for note in [36u8, 48, 60, 72] {
                let measured = measure_note(note, 0.0, wave);
                let want = expected(note, 0.0);
                let cents = 1200.0 * (measured / want).log2();
                assert!(
                    cents.abs() < 2.0,
                    "{wave:?} MIDI {note}: {measured:.2} Hz, {cents:+.1} cents"
                );
            }
        }
    }

    #[test]
    fn the_range_switch_transposes_by_exact_octaves() {
        for (offset, label) in [(-36.0, "64'"), (-12.0, "16'"), (0.0, "8'"), (24.0, "2'")] {
            let measured = measure_note(60, offset, Wave::Saw);
            let want = expected(60, offset);
            let cents = 1200.0 * (measured / want).log2();
            assert!(
                cents.abs() < 2.0,
                "{label}: {measured:.2} Hz, {cents:+.1} cents"
            );
        }
    }
}
