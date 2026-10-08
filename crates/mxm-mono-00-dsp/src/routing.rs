//! What mxm-mono-00 can modulate, and with what.
//!
//! `plans/plan-modulation-routing.md` §7.1 and `plans/plan-mxm-mono-00-modulation.md`. The shared
//! machinery is [`mxm_modulation`]; this module is the instrument's own declaration — its **source
//! list**, its **target list**, each target's **scale**, its **combination law** and its
//! **transition class**.
//!
//! # The switching jack was already this, minus the summing
//!
//! The plug-out's patch bay gives every input one connection with no level of its own; the level is
//! the destination module's own attenuator. Decision 1.6 turns that into a mixer: an input takes any
//! number of sources, each with its own signed amount. **Nothing else about the machine changes.**
//! A row that named one source becomes one present pair, and the attenuator that sat beside it
//! becomes that pair's amount.
//!
//! So there is no `Route::Default` here and no `default_source` table. **The plug-out's normalled
//! connections are [`INIT_PRESENT`] and [`INIT_AT_FULL`] — the init patch, not wiring in the voice.**
//! That is the whole of the conversion: what the machine came wired as is a set of present pairs a
//! player can remove, and what it came wired *to* is reachable by adding one.
//!
//! # The frame unit is one, because the columns were already scaled
//!
//! [`mxm_modulation::SourceFrame::write`] bounds every published value to unit magnitude, and this
//! machine's sources are already expressed in **ten-volt units** — the column scale `matrix.rs` has
//! carried since the instrument was built, where 1.0 is +10 V. An LFO reaches 1, a VCO ±0.5, an
//! envelope 0.6, a gate 0/1 and the keyboard CV ±0.56. Nothing clamps on publication, so unlike
//! `mxm-mono-pr1` this instrument needs no frame unit at all and no scale is undone anywhere.
//!
//! # Three laws, and two of them are this machine's
//!
//! - **Sum** for every continuous input, which is decision 1.6's mixer.
//! - **Gate**: sum, then detect once against [`crate::matrix::GATE_THRESHOLD`]. An amount scales its
//!   source *before* detection, so it decides whether that source ever crosses and when within its
//!   rise. Two half-amount sources can together cross where neither reaches alone, which is what
//!   makes the level continuous rather than an enable.
//! - **Sync**: detect per route and reset by the **largest** depth. It cannot sum, because
//!   `EdgeRow`'s interpolated fraction is scale-invariant — multiply both samples by any positive
//!   number and the crossing is identical — so a scaled sync route would be an on/off switch wearing
//!   a knob. The amount is *reset depth* instead: how far the slave's phase is pulled toward zero.
//!
//! # Two transition classes
//!
//! Every CV and gate route **steps** on a topology change, as the patch bay's own switching did. The
//! two rows that carry **audio** into a summing input crossfade over
//! [`crate::matrix::AUDIO_ROUTE_RAMP_S`], and under summing that generalises from *fade between two
//! sources* to *fade each route's own contribution in and out* — which is the same thing when one
//! route replaces another, and is the only form that also covers adding a second.
//!
//! # The collection's standard
//!
//! Key, Velocity, Wheel, Pressure and Bend mean what they mean on every instrument, and a route the
//! machine never had reaches what it reaches on every instrument ([`mxm_modulation::standard`];
//! `plans/plan-modulation-standard.md`). Key keeps the keyboard CV's own ten-volt unit
//! ([`KEY_UNIT_SEMITONES`], 1 V/oct from middle C). **The machine's pairs keep their reach** — every
//! column into a patch-bay row, the panel's own wiring ([`machine`]) — and so does every generator
//! pair the conversion added; **a performance pair the machine did not have takes the standard**
//! ([`takes_standard_reach`]): 12 st of pitch, 4 octaves of LFO rate, cutoff or phaser, 45 % of a
//! pulse width. A uniform row keeps its instruction sequence through `mxm_modulation::sum_split`,
//! to the bit while no such pair is live ([`ADDED_SCALE`]). **Key into a pitch is 1 V/oct** on
//! both oscillators — the EXT CV's own tracking, and the standard's — where the row's 144 would
//! give 14.4 st/oct. The machine's CV amplifier is shown as **VCA level**; the standard
//! **Amplitude** is a new target after the VCA. [`offer`] refuses the pairs that can never mean
//! anything — a gesture into a gate or the sync input, Velocity into the S&H, anything held into
//! the mixer's audio input — and gives the one-sided targets only their live half.

use mxm_modulation::standard::{self, Offer, Performance, Sign, reach as standard_reach};
use mxm_modulation::{Compacted, SourceFrame};

use crate::lfo::PWM_TRIANGLE_PEAK;
use crate::matrix::{AUDIO_ROUTE_RAMP_S, Column};
use crate::voice::{
    ENV_UNITS, FILTER_ENV_OCTAVES, FILTER_LFO_OCTAVES, LFO_CV_OCTAVES_PER_UNIT, VCO_OUT_UNITS,
};

/// Every source this instrument declares.
///
/// **The first fourteen are `matrix.rs`'s columns, at their own indices**, so
/// [`Column::index`] is a source index and the plug-out's output list needs no translation table.
/// The six after them are new, and each one exists because something in the voice was hard-wired
/// and had to stop being:
///
/// - [`GATE`] and [`GLIDE`] are the two signals the patch bay **normalled to without publishing**.
///   The keyboard gate reached both envelope gate rows and the glide's RC dip reached GLIDE IN, and
///   in both cases the voice tested for *no column selected* and substituted the internal signal.
///   As sources they are ordinary pairs in the init patch — and they can now reach anything else.
/// - [`VELOCITY`], [`WHEEL`], [`PRESSURE`] and [`BEND`] are the collection's shared performance
///   inputs, decision 1.7. **This machine's keyboard has none of them**: the hardware reads no
///   velocity and has no aftertouch, so each is a new MIDI path, and every one of their pairs starts
///   absent. `plugins/mxm-mono-00/AGENTS.md` records that as the owner's ruling.
///
/// The last five are **LFO core outputs**, ahead of the shape switch — the signals SAMPLE MODE and
/// the two PWM switches chose between. They became sources when those switches became routing.
pub mod source {
    /// The keyboard CV, 1 V/oct from middle C. `Column::KybdCv`.
    pub const KEY: usize = 0;
    /// LFO-1's output. `Column::Lfo1`.
    pub const LFO1: usize = 1;
    /// LFO-2's output. `Column::Lfo2`.
    pub const LFO2: usize = 2;
    /// The sample-and-hold's output. `Column::ShOut`.
    pub const SH_OUT: usize = 3;
    /// The sample-and-hold's clock. `Column::ShClock`.
    pub const SH_CLOCK: usize = 4;
    /// VCO-1's audio. `Column::Vco1`.
    pub const VCO1: usize = 5;
    /// VCO-2's audio. `Column::Vco2`.
    pub const VCO2: usize = 6;
    /// VCO-1's sync pulse. `Column::Vco1Sync`.
    pub const VCO1_SYNC: usize = 7;
    /// VCO-2's sync pulse. `Column::Vco2Sync`.
    pub const VCO2_SYNC: usize = 8;
    /// The ring modulator's output. `Column::RingMod`.
    pub const RING_MOD: usize = 9;
    /// The noise generator. `Column::Noise`.
    pub const NOISE: usize = 10;
    /// The mixer's saturated sum. `Column::MixerOut`.
    pub const MIXER_OUT: usize = 11;
    /// The filter envelope. `Column::VcfAdsr`.
    pub const VCF_ADSR: usize = 12;
    /// The amplifier envelope. `Column::VcaAdsr`.
    pub const VCA_ADSR: usize = 13;
    /// The keyboard gate, held 0 or 1. **New as a source** — it was the thing both gate rows
    /// normalled to, tested for as *no column* rather than published.
    pub const GATE: usize = 14;
    /// The glide's own RC dip, 0…1, recovering after a gate edge dumped it. **New as a source** —
    /// it was what GLIDE IN normalled to, and for the same reason.
    pub const GLIDE: usize = 15;
    /// Velocity, the standard's `v − 1` of **the press that last triggered an envelope**
    /// (`Voice::envelope_velocity`): zero at the hardest note and before any press. **New MIDI
    /// path.**
    pub const VELOCITY: usize = 16;
    /// The mod wheel, CC 1, as the sounding channel holds it. Unipolar. **New MIDI path.**
    pub const WHEEL: usize = 17;
    /// Channel pressure. Unipolar. **New MIDI path.**
    pub const PRESSURE: usize = 18;
    /// The pitch bender's normalised position, signed. **New as a source**; the bend's own
    /// hard-wired path to both oscillators is untouched beside it (`plan-modulation-routing.md`
    /// §5.2 part 2).
    pub const BEND: usize = 19;
    /// LFO-1's **core** sawtooth, `0…1`, ahead of the shape switch: what the S&H's SAW1 position
    /// sampled (§10.3). **New as a source**, with the four after it, when SAMPLE MODE and the PWM
    /// switches became routing.
    pub const LFO1_CORE_SAW: usize = 20;
    /// LFO-1's core sawtooth through the inverter, `1 − saw` — the S&H's SAW2. **Not a negative
    /// route on the saw**, which would be `−saw`: the two differ by a whole unit of offset.
    pub const LFO1_CORE_REVERSE_SAW: usize = 21;
    /// LFO-1's core triangle, `0…1`: the S&H's TRI, and the PWM section's LFO-1 position, which
    /// always took the triangle whatever the shape switch said (wart 5).
    pub const LFO1_CORE_TRIANGLE: usize = 22;
    /// LFO-1's diode-rounded sine, `−0.5…0.5`, ahead of the shape switch: the S&H's SIN (EXT)
    /// with nothing patched.
    pub const LFO1_CORE_SINE: usize = 23;
    /// LFO-2's core triangle, `0…1`: the PWM section's LFO-2 position.
    pub const LFO2_CORE_TRIANGLE: usize = 24;
}

/// How many sources the instrument declares.
pub const SOURCES: usize = 25;

/// Their names, in source order, for the interface and for accessibility.
///
/// The first fourteen are the names the retired row enums used, unchanged, so a player reading a
/// route row sees the word the patch bay's menu showed.
pub const SOURCE_NAMES: [&str; SOURCES] = [
    "Keyboard CV",
    "LFO 1",
    "LFO 2",
    "S&H",
    "S&H clock",
    "Oscillator 1",
    "Oscillator 2",
    "Oscillator 1 sync",
    "Oscillator 2 sync",
    "Ring modulator",
    "Noise",
    "Mixer",
    "Envelope 1",
    "Envelope 2",
    "Gate",
    "Glide",
    "Velocity",
    "Wheel",
    "Pressure",
    "Bend",
    // The last five are the LFO cores, ahead of the shape switches — named without the word,
    // which cost every card forty points: *Gate from LFO 1 reverse saw* is as long as the
    // longest row the patch bay already had, *Gate from Oscillator 1 sync*.
    "LFO 1 saw",
    "LFO 1 reverse saw",
    "LFO 1 triangle",
    "LFO 1 sine",
    "LFO 2 triangle",
];

/// Every target this instrument declares: **twelve of the plug-out's fifteen input rows**, in its own
/// order (`research:instruments/system-100.md` §12.2), then **five inputs** the plug-out wired on its
/// panel and this collection routes — the fifth the one cutoff that the VCF's two jack rows merged
/// into, beside KYBD CV — and last **the standard Amplitude**, which the modulation standard added
/// after the VCA.
///
/// **The panel's wiring is routing now** (`plans/plan-mxm-mono-00-modulation.md` Rev 3). The
/// DESTINATION switch that sent the glide and the VCO LFO to one oscillator or both, the fixed VCA
/// LFO, the two PWM switches, KYBD CV to the filter and SAMPLE MODE each chose a source at the
/// source, or wired a depth to one source. Each is now a target on the card it moves, and the source
/// is chosen there — the collection's rule. **GLIDE IN retired with them**: a direct route to an
/// oscillator's pitch is its exact equivalent. The DSP's positions are not stored anywhere — plugin
/// state and presets load by permanent string id — so removing it moved no stored value (the
/// owner's ruling, 2026-09-22).
pub mod target {
    /// LFO-1's rate CV input, summed in octaves.
    pub const LFO1_RATE: usize = 0;
    /// LFO-2's rate CV input, summed in octaves.
    pub const LFO2_RATE: usize = 1;
    /// What the sample-and-hold samples, in ten-volt units. **Anything contributing turns the
    /// sampler on**; nothing contributing is the retired OFF position.
    pub const SH_INPUT: usize = 2;
    /// VCO-1's pitch input, summed in semitones: the EXT CV jack, and the glide and VCO LFO that
    /// DESTINATION used to send here.
    pub const VCO1_PITCH: usize = 3;
    /// VCO-2's SYNC IN. **The one target whose law is sync** — see [`LAW`].
    pub const VCO2_SYNC: usize = 4;
    /// What the ring modulator multiplies VCO-2 by — its Y input, *Ring mod* on the panel. Audio,
    /// so it crossfades.
    pub const RING_INPUT: usize = 5;
    /// The mixer's routable channel, *External input*: the EXT IN jack, which takes any source at a level of
    /// its own, and they add. Audio, so it crossfades. **The ring modulator is not here at Init**:
    /// it has its own level in the mixer, `Patch::level_ring`, as the 102 had its own RING MOD
    /// slider beside its EXT slider (§3.2) — the plug-out's shared RING MOD / EXT IN slider, split
    /// back into the two it was made from.
    pub const MIXER_INPUT: usize = 6;
    /// The filter envelope's gate input. **Gate law.**
    pub const VCF_GATE: usize = 7;
    /// The amplifier's ADSR CV input: what opens the VCA — shown as **VCA level**, the machine's
    /// CV amplifier, beside the standard Amplitude after it.
    pub const AMPLIFIER: usize = 8;
    /// The amplifier envelope's gate input. **Gate law.**
    pub const VCA_GATE: usize = 9;
    /// The phaser's rate CV input.
    pub const PHASER_RATE: usize = 10;
    /// The phaser's manual-centre CV input.
    pub const PHASER_CENTRE: usize = 11;
    /// VCO-2's pitch input, summed in semitones. The glide and VCO LFO halves of DESTINATION, and
    /// **a hardware path the plug-out dropped**: the 102's own VCO had an EXT CV input, and it was
    /// the S/H's only internal destination (§3.2, §4.2).
    pub const VCO2_PITCH: usize = 12;
    /// VCO-1's pulse-width modulation, as a fraction of the cycle. **Narrowing law** — see
    /// [`narrowed_width`].
    pub const VCO1_WIDTH: usize = 13;
    /// VCO-2's pulse-width modulation. Narrowing law.
    pub const VCO2_WIDTH: usize = 14;
    /// The VCA's LFO input, which **can only dip** (wart 3): what it carries is subtracted, above
    /// zero only, from the gain INITIAL GAIN and the ADSR input have set.
    pub const TREMOLO: usize = 15;
    /// **The filter's cutoff, in octaves: one input for everything that moves it.** The plug-out
    /// had three — the VCF ADSR IN and VCF LFO IN jacks, each behind its own attenuator, and the
    /// KYBD CV slider — and they all moved the one thing, so a player read *Modulator 1* and
    /// *Modulator 2* and had to know which jack was which. They merged here (the owner, 2026-09-22),
    /// as `mxm-mono-01` has one Cutoff, and **each source keeps the reach its own jack gave it**:
    /// an envelope seven octaves, an LFO four, the keyboard one octave per octave, any other column
    /// the whole ±12 the cutoff clamps to, and a performance source no jack had the standard's four
    /// — see [`FULL_SCALE`].
    pub const CUTOFF: usize = 16;
    /// **The collection's standard Amplitude** (`standard::amplitude_factor`), after the VCA —
    /// new with the modulation standard, nothing routed to it at Init.
    pub const AMPLITUDE: usize = 17;
}

/// How many targets the instrument declares.
pub const TARGETS: usize = 18;

/// Their names, in target order.
///
/// **Each names what it moves** — *Pitch*, *Cutoff*, *Amplitude*, *Rate* — never an abstraction such
/// as *Modulator* that says only that something is patched there (the owner, 2026-09-22, with
/// `mxm-mono-01`'s *Cutoff from Envelope* as the model). Neutral about what is patched to them, and
/// module-qualified where the collection needs the module to say which oscillator or LFO.
pub const TARGET_NAMES: [&str; TARGETS] = [
    "LFO 1 rate",
    "LFO 2 rate",
    "Sample and hold input",
    "Oscillator 1 pitch",
    "Oscillator 2 sync",
    "Ring mod",
    "External input",
    "Filter envelope gate",
    "VCA level",
    "Amplifier envelope gate",
    "Phaser rate",
    "Phaser centre",
    "Oscillator 2 pitch",
    "Oscillator 1 pulse width",
    "Oscillator 2 pulse width",
    "Tremolo",
    "Cutoff",
    "Amplitude",
];

/// Their **painted** names: the same inputs, with the prefix the card around them already carries
/// dropped — design system §7.1, and [`mxm_modulation_params::ui::stack`]'s `panel`.
///
/// **This is a layout cost, measured rather than a preference.** A route row reads
/// `<target> from <source>` and both halves are module-qualified here, so the longest row —
/// *"Amplifier envelope gate from Oscillator 1 sync"* — set the Envelope 2 card's floor at 396
/// points where the short form sets it at 286. Across eleven cards that is enough to push the
/// editor's first musician page down to a single card.
///
/// Each of these is unambiguous **inside its own card**, which is the only place it is painted:
/// *Gate* under a card titled *Envelope 2*, *Pitch* under *Oscillator 2*. [`TARGET_NAMES`] stays
/// the canonical name and is what the parameter, the host's automation list and the accessibility
/// tree read.
pub const TARGET_PANEL_NAMES: [&str; TARGETS] = [
    "Rate",
    "Rate",
    "Input",
    "Pitch",
    "Sync",
    "Input",
    "External input",
    "Gate",
    "Level",
    "Gate",
    "Phaser rate",
    "Phaser centre",
    "Pitch",
    "Pulse width",
    "Pulse width",
    "Tremolo",
    "Cutoff",
    "Amplitude",
];

/// How a target combines its live routes.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Law {
    /// Add them, in the target's own domain. Every continuous input.
    Sum,
    /// Add them, then run the one edge detector. The amounts scale before the threshold.
    Gate,
    /// Detect per route; reset by the largest depth of the routes that crossed this sample.
    Sync,
}

/// Each target's law, in target order.
pub const LAW: [Law; TARGETS] = {
    let mut table = [Law::Sum; TARGETS];
    table[target::VCO2_SYNC] = Law::Sync;
    table[target::VCF_GATE] = Law::Gate;
    table[target::VCA_GATE] = Law::Gate;
    table
};

/// The two pitch inputs' reach: **semitones per ten-volt unit at full amount.**
///
/// The plug-out's EXT CV is 1 V/oct, 120 semitones per unit, but it was never the only way a source
/// reached a pitch: GLIDE IN added up to 2 semitones and the VCO LFO slider 12 more, beside it and
/// outside its clamp. Those two retired into this input, so it has to reach what the three reached
/// together — at most 134 semitones from one source — or a patch that used them at once would lose
/// modulation it had, and it is audible, because the oscillator clamps at 4 Hz…0.45·fs rather than at
/// the panel's range. **144 is the next round number above 134**; presets written at 120 translate by
/// 120/144. The amount fader carries a square-law taper so that vibrato depths still sit near its
/// centre (the plugin's `routes.rs`).
pub const PITCH_SEMITONES_PER_UNIT: f32 = 144.0;

/// The cutoff's reach for a column no filter jack had its own attenuator for: **the whole ±12
/// octaves** the cutoff clamps to, so merging the three inputs narrows nothing. A performance source,
/// which no jack ever carried, takes the standard's four instead.
pub const CUTOFF_OCTAVES_PER_UNIT: f32 = 12.0;

/// The keyboard on the cutoff: 1 V/oct, **ten octaves per ten-volt unit**, so the keyboard CV at
/// full amount tracks the cutoff one octave per octave — the retired KYBD CV slider at 100 %.
pub const KEY_TRACK_OCTAVES_PER_UNIT: f32 = 10.0;

/// How far pulse-width modulation narrows the pulse at full depth, from square: 50 % to 5 %.
pub const PWM_SWING: f32 = 0.45;

/// What one full-amount route delivers, **in the target's own domain**, per unit of source — **per
/// route**, `mxm-mono-02`'s `FULL_SCALE`.
///
/// Every target but the two pulse widths and the cutoff is **uniform**: a row scales *whatever is on it* into its
/// own domain, which is what the ten-volt column scale exists for and what made a row's default
/// source and the same source selected explicitly sound alike. [`Graph::sum`] evaluates those as
/// `(Σ amount × source) × scale`, the order the pinned conversion digests were taken in.
///
/// **The pulse widths are not**, because the PWM section read its sources in its own units rather
/// than in columns: the LFO position took the 0…0.6 PWM triangle (§5.4), the envelope positions the
/// envelope at 0…1 where its column carries 0.6, and the glide its 0…1 charge. Each source's scale
/// is set so that **an amount is the retired PWM depth, exactly**: `0.5 − 0.45 · depth · level`.
///
/// [`target::VCO2_SYNC`] has no scale: its amount is reset depth, which is not a reach.
pub const FULL_SCALE: [[f32; SOURCES]; TARGETS] = {
    let mut table = [[1.0; SOURCES]; TARGETS];
    let mut t = 0;
    while t < TARGETS {
        let scale = match t {
            target::LFO1_RATE | target::LFO2_RATE => LFO_CV_OCTAVES_PER_UNIT,
            target::VCO1_PITCH | target::VCO2_PITCH => PITCH_SEMITONES_PER_UNIT,
            // The two audio rows read back into the ±1 audio domain the mixer and the ring
            // modulator work in. A power of two, so it is exact.
            target::RING_INPUT | target::MIXER_INPUT => 1.0 / VCO_OUT_UNITS,
            // A full envelope opens the amplifier fully: its column peaks at `ENV_UNITS`.
            target::AMPLIFIER => 1.0 / ENV_UNITS,
            target::VCO1_WIDTH | target::VCO2_WIDTH => PWM_SWING,
            target::CUTOFF => CUTOFF_OCTAVES_PER_UNIT,
            _ => 1.0,
        };
        table[t] = [scale; SOURCES];
        t += 1;
    }
    // **The cutoff, per source: each keeps the reach its own jack gave it.** An envelope reaches
    // `FILTER_ENV_OCTAVES` — per ten-volt unit that is the ratio to its 0.6 peak, which is why
    // `ENV_UNITS` appears — as the VCF ADSR IN jack did; an LFO `FILTER_LFO_OCTAVES`, as the VCF LFO
    // IN jack did; the keyboard one octave per octave, the KYBD CV slider at 100 %. Any other
    // column takes the whole `CUTOFF_OCTAVES_PER_UNIT`, the ±12 the cutoff clamps to, so no source
    // reaches less than any jack gave it; a performance source takes the standard's four (below).
    let lfos = [
        source::LFO1,
        source::LFO2,
        source::LFO1_CORE_SAW,
        source::LFO1_CORE_REVERSE_SAW,
        source::LFO1_CORE_TRIANGLE,
        source::LFO1_CORE_SINE,
        source::LFO2_CORE_TRIANGLE,
    ];
    let mut l = 0;
    while l < lfos.len() {
        table[target::CUTOFF][lfos[l]] = FILTER_LFO_OCTAVES;
        l += 1;
    }
    table[target::CUTOFF][source::VCF_ADSR] = FILTER_ENV_OCTAVES / ENV_UNITS;
    table[target::CUTOFF][source::VCA_ADSR] = FILTER_ENV_OCTAVES / ENV_UNITS;
    table[target::CUTOFF][source::KEY] = KEY_TRACK_OCTAVES_PER_UNIT;
    // A performance source the filter's jacks never had takes the standard's four octaves.
    let added = [
        source::VELOCITY,
        source::WHEEL,
        source::PRESSURE,
        source::BEND,
    ];
    let mut a = 0;
    while a < added.len() {
        table[target::CUTOFF][added[a]] = standard_reach::OCTAVES;
        a += 1;
    }
    // **The standard Amplitude**: a source at its peak and full amount is the whole ±100 %, and Key
    // a fifth of it per octave.
    let mut s = 0;
    while s < SOURCES {
        table[target::AMPLITUDE][s] = standard_reach::AMPLITUDE / SOURCE_PEAK[s];
        s += 1;
    }
    table[target::AMPLITUDE][source::KEY] = standard::key_scale(
        standard_reach::AMPLITUDE * standard_reach::KEY_LINEAR_FRACTION_PER_OCTAVE,
        KEY_UNIT_SEMITONES,
    );
    let widths = [target::VCO1_WIDTH, target::VCO2_WIDTH];
    let mut w = 0;
    while w < widths.len() {
        let t = widths[w];
        // The PWM section's LFO positions took the core triangle at its +6 V (`PWM_TRIANGLE_PEAK`).
        table[t][source::LFO1_CORE_TRIANGLE] = PWM_SWING * PWM_TRIANGLE_PEAK;
        table[t][source::LFO2_CORE_TRIANGLE] = PWM_SWING * PWM_TRIANGLE_PEAK;
        // Its envelope positions took the envelope at 0…1, where the column carries 0…0.6.
        table[t][source::VCF_ADSR] = PWM_SWING / ENV_UNITS;
        table[t][source::VCA_ADSR] = PWM_SWING / ENV_UNITS;
        // Key, which the PWM switch never offered, narrows by the standard's 9 % per octave.
        table[t][source::KEY] = standard::key_scale(
            standard_reach::WIDTH * standard_reach::KEY_LINEAR_FRACTION_PER_OCTAVE,
            KEY_UNIT_SEMITONES,
        );
        w += 1;
    }
    table
};

/// Semitones per unit of Key: **the keyboard CV's own ten-volt scale**, 1 V/oct from middle C. The
/// standard's Key is `(glided note − 60) / unit`; this machine's unit is its own.
pub const KEY_UNIT_SEMITONES: f32 = 120.0;

/// Which sources are the collection's performance sources. *Gate* and *Glide* are this machine's
/// own normalled signals, not the standard's.
pub const PERFORMANCE: [Option<Performance>; SOURCES] = {
    let mut table = [None; SOURCES];
    table[source::KEY] = Some(Performance::Key);
    table[source::VELOCITY] = Some(Performance::Velocity);
    table[source::WHEEL] = Some(Performance::Wheel);
    table[source::PRESSURE] = Some(Performance::Pressure);
    table[source::BEND] = Some(Performance::Bend);
    table
};

/// What each target does with its sum, **for the standard's offer** — [`LAW`] is how the voice
/// evaluates it. The gates and the sync input act on a crossing, the S&H samples, the mixer's
/// channel is audio, the VCA level is the machine's CV amplifier, the pulse widths only narrow and
/// the tremolo only dips.
pub const STANDARD_LAW: [standard::Law; TARGETS] = {
    let mut table = [standard::Law::Sum; TARGETS];
    table[target::SH_INPUT] = standard::Law::Sample;
    table[target::VCO2_SYNC] = standard::Law::Edge;
    table[target::MIXER_INPUT] = standard::Law::AudioInput;
    table[target::VCF_GATE] = standard::Law::Edge;
    table[target::AMPLIFIER] = standard::Law::MachineAmplifier;
    table[target::VCA_GATE] = standard::Law::Edge;
    table[target::VCO1_WIDTH] = standard::Law::OneSided(Sign::Negative);
    table[target::VCO2_WIDTH] = standard::Law::OneSided(Sign::Negative);
    table[target::TREMOLO] = standard::Law::OneSided(Sign::Negative);
    table[target::AMPLITUDE] = standard::Law::Factor;
    table
};

/// Whether a pair is one **the machine had**: any column into a patch-bay row — the plug-out's
/// twelve input rows, the filter's two jacks merged into the cutoff, and the 102's own VCO EXT CV,
/// VCO-2's pitch — and the panel's own wiring: the keyboard gate into both envelope gates, the glide
/// into both pitches, LFO 1's core shapes into the S&H, the PWM switch's positions into both widths
/// and LFO 1 into the tremolo. The conversion's performance sources and the Amplitude are not.
#[must_use]
pub const fn machine(target: usize, source: usize) -> bool {
    if target == target::AMPLITUDE {
        return false;
    }
    let patch_bay_row =
        target <= target::PHASER_CENTRE || target == target::VCO2_PITCH || target == target::CUTOFF;
    if patch_bay_row && source <= source::VCA_ADSR {
        return true;
    }
    match target {
        target::VCF_GATE | target::VCA_GATE => source == source::GATE,
        target::VCO1_PITCH | target::VCO2_PITCH => source == source::GLIDE,
        target::SH_INPUT => source >= source::LFO1_CORE_SAW && source <= source::LFO1_CORE_SINE,
        target::VCO1_WIDTH | target::VCO2_WIDTH => {
            source == source::LFO1_CORE_TRIANGLE
                || source == source::LFO2_CORE_TRIANGLE
                || source == source::VCF_ADSR
                || source == source::VCA_ADSR
                || source == source::GLIDE
        }
        target::TREMOLO => source == source::LFO1,
        _ => false,
    }
}

/// Whether a pair takes **the standard reach** rather than its row's: a performance source the
/// machine did not have. Every generator pair keeps its row's reach, a machine pair or not.
#[must_use]
pub const fn takes_standard_reach(target: usize, source: usize) -> bool {
    PERFORMANCE[source].is_some() && !machine(target, source)
}

/// Whether a pair is offered, and on which half: the standard's criterion ([`standard::offer`]) for
/// a performance source, and both halves for a generator — **except into the sync input**, whose
/// depth is a reset: a negative one pulls the core nowhere even beside another route, so a
/// generator there offers only its positive half. Key's sync pair is the patch bay's own, and the
/// standard keeps both halves of a machine pair.
#[must_use]
pub const fn offer(target: usize, source: usize) -> Offer {
    match PERFORMANCE[source] {
        Some(performance) => standard::offer(
            STANDARD_LAW[target],
            Some(performance),
            machine(target, source),
        ),
        None if target == target::VCO2_SYNC => Offer::PositiveOnly,
        None => Offer::Both,
    }
}

/// **The added half of a uniform row's split sum**, per unit of source in the row's own domain —
/// zero for a pair on the row's own scale. Only the rows whose scale differs from the standard need
/// one: the two pitches (12 st for a performance source the machine did not have, and Key's 1 V/oct
/// on both) and the two LFO rates (the standard's four octaves). The cutoff, the widths and the
/// Amplitude are per-route rows already, and their entries are in [`FULL_SCALE`].
pub const ADDED_SCALE: [[f32; SOURCES]; TARGETS] = {
    let mut table = [[0.0; SOURCES]; TARGETS];
    let mut s = 0;
    while s < SOURCES {
        let mut t = 0;
        while t < TARGETS {
            if takes_standard_reach(t, s) {
                table[t][s] = match t {
                    target::VCO1_PITCH | target::VCO2_PITCH => standard_reach::PITCH_SEMITONES,
                    target::LFO1_RATE | target::LFO2_RATE => standard_reach::OCTAVES,
                    _ => 0.0,
                };
            }
            t += 1;
        }
        s += 1;
    }
    let key = standard::key_scale(
        standard_reach::KEY_PITCH_SEMITONES_PER_OCTAVE,
        KEY_UNIT_SEMITONES,
    );
    table[target::VCO1_PITCH][source::KEY] = key;
    table[target::VCO2_PITCH][source::KEY] = key;
    table
};

/// What one full-amount route delivers per unit of source, in its target's domain — the added
/// scale where a pair has one, the row's otherwise. **For readings**; the sum applies the same
/// numbers in its own order.
#[must_use]
pub const fn scale(target: usize, source: usize) -> f32 {
    if ADDED_SCALE[target][source] != 0.0 {
        ADDED_SCALE[target][source]
    } else {
        FULL_SCALE[target][source]
    }
}

/// Whether a target scales every source alike — every target but the pulse widths and the cutoff.
const UNIFORM: [bool; TARGETS] = {
    let mut table = [true; TARGETS];
    let mut t = 0;
    while t < TARGETS {
        let mut s = 1;
        while s < SOURCES {
            if FULL_SCALE[t][s] != FULL_SCALE[t][0] {
                table[t] = false;
            }
            s += 1;
        }
        t += 1;
    }
    table
};

/// **Wart 4, as a law.** With nothing routed the pulse width is the manual width; with anything
/// routed it **narrows from square** by what is routed and never widens past it — the PWM switch's
/// non-MANUAL positions, where the slider stopped being the width and became the depth (§5.4).
/// Presence is the switch: routing a source is moving it off MANUAL.
#[inline]
#[must_use]
pub fn narrowed_width(manual: f32, routed: bool, narrowing: f32) -> f32 {
    if routed {
        (0.5 - narrowing).clamp(0.05, 0.5)
    } else {
        manual.clamp(0.05, 0.5)
    }
}

/// **What each source actually reaches, in ten-volt units** — for reading an amount, never for
/// evaluating one.
///
/// The column scale is one unit per ten volts and the machine's sources do not all fill it: an LFO
/// sawtooth runs the whole 0…1, a VCO is ±0.5, an envelope peaks at 0.6 and the keyboard CV reaches
/// 0.56 at MIDI 127. So [`FULL_SCALE`] is *per unit of source* and what a player should read on a
/// route is `scale × peak`: **what that pair delivers at full amount.**
///
/// Without it a route's own reading is wrong in a way that matters — *"Cutoff from
/// Envelope 1"* would read `+11.67 oct` where the envelope moves the cutoff by seven, which is
/// `FILTER_ENV_OCTAVES` and is the number the retired `filterenv` knob meant. `mxm-mono-pr1` paid
/// for the same thing in its own frame unit.
///
/// **Nothing evaluates with this.** A source that happens to exceed its peak is bounded by the
/// frame, not by this table.
pub const SOURCE_PEAK: [f32; SOURCES] = {
    let mut table = [1.0; SOURCES];
    // The keyboard CV, at MIDI 127: 67 semitones above middle C, in tenths of a volt per octave.
    table[source::KEY] = (67.0 / 12.0) / 10.0;
    // Everything that carries audio, at the VCO's 10 Vp-p.
    table[source::VCO1] = VCO_OUT_UNITS;
    table[source::VCO2] = VCO_OUT_UNITS;
    table[source::VCO1_SYNC] = VCO_OUT_UNITS;
    table[source::VCO2_SYNC] = VCO_OUT_UNITS;
    table[source::RING_MOD] = VCO_OUT_UNITS;
    table[source::NOISE] = VCO_OUT_UNITS;
    table[source::MIXER_OUT] = VCO_OUT_UNITS;
    // The envelopes, at their +6 V peak.
    table[source::VCF_ADSR] = ENV_UNITS;
    table[source::VCA_ADSR] = ENV_UNITS;
    // The core sine is the diode-rounded triangle about zero, ±0.5; the core saws and triangles
    // fill the whole unit.
    table[source::LFO1_CORE_SINE] = 0.5;
    table
};

/// The bound on a summed *frame-domain* value, before [`FULL_SCALE`] converts it.
///
/// Twenty sources at unit magnitude and full amount is twenty, so this is generous by design: it
/// exists to keep a runaway finite, not to shape a sound.
pub const SUM_BOUND: f32 = 64.0;

/// **Each target's own limit, in its own domain, applied after [`FULL_SCALE`].**
///
/// [`SUM_BOUND`] is in the *frame's* domain, and a target scale multiplies after it — so on
/// `VCO1_PITCH`, whose scale is 120 semitones per unit, a clamped 64 becomes **7 680 semitones**
/// and `exp2` of that is `+inf`. `plan-modulation-routing.md` §4.1 says each target applies its
/// real limit where it matters, and that limit has to be in the target's units or it is not a
/// limit at all.
///
/// **Generous, not tight.** An input is a mixer now, so summing several sources past one route's
/// own reach is the point; these numbers are the far side of anything musical, chosen so that
/// nothing downstream overflows and nothing a player would do is clipped. Where the consumer has
/// its own clamp — the cutoff's ±12 octaves, the widths' `0.05…0.5`, `Vca::gain`'s `0…1` — this is
/// the coarser outer guard and that clamp is still what shapes the sound.
pub const TARGET_BOUND: [f32; TARGETS] = {
    // Octaves, levels and depths: eight of each is far past anything musical and far short of
    // anything that overflows.
    let mut table = [8.0; TARGETS];
    // Octaves of LFO rate and of cutoff, where the consumer clamps to ±12 anyway.
    table[target::LFO1_RATE] = 12.0;
    table[target::LFO2_RATE] = 12.0;
    // **Semitones, and the one that matters**: twice one route's full reach, which keeps summing
    // meaningful and keeps `exp2` far from overflow. It is also above the 268 semitones the retired
    // paths could reach together — the EXT CV sum at its old 240 bound, plus a glide input and a
    // vibrato that were added outside it — so nothing a patch could do before is clipped now.
    table[target::VCO1_PITCH] = 2.0 * PITCH_SEMITONES_PER_UNIT;
    table[target::VCO2_PITCH] = 2.0 * PITCH_SEMITONES_PER_UNIT;
    // Octaves of cutoff, where the consumer clamps to ±12 anyway.
    table[target::CUTOFF] = 12.0;
    // A narrowing: more than the whole 50 % is not a thing, and `narrowed_width` clamps anyway.
    table[target::VCO1_WIDTH] = 1.0;
    table[target::VCO2_WIDTH] = 1.0;
    // Reset depth is a fraction of one reset; more than one is not a thing.
    table[target::VCO2_SYNC] = 1.0;
    table
};

/// The two targets that carry **audio** into a summing input, and therefore crossfade.
///
/// Their order is this array's, which is what [`Graph`]'s fade state is indexed by.
pub const AUDIO_TARGETS: [usize; 2] = [target::RING_INPUT, target::MIXER_INPUT];

/// Which fade slot a target uses, if it is one of [`AUDIO_TARGETS`].
#[inline]
#[must_use]
pub const fn audio_slot(target: usize) -> Option<usize> {
    let mut i = 0;
    while i < AUDIO_TARGETS.len() {
        if AUDIO_TARGETS[i] == target {
            return Some(i);
        }
        i += 1;
    }
    None
}

/// **The plug-out's normalled connections that carry no signal until something is turned up.**
///
/// Present in the init patch at **zero depth**, because the attenuator beside each of them was at
/// zero before the conversion and a route at zero depth is audible as nothing. Removing one is what
/// pulling the cord out of the jack was; adding another source to the same input is the mixer
/// decision 1.6 asked for.
///
/// Each names the manual page it comes from in `research:instruments/system-100.md` §12.2.
pub const INIT_PRESENT: [(usize, usize); 9] = [
    // VCO-1 EXT CV IN ← VCO-2 (p. 16), the plug-out's cross-modulation. Depth was `extcv1`.
    (target::VCO1_PITCH, source::VCO2),
    // GLIDE → DESTINATION (p. 16), which defaults to both oscillators. Depth was `glide`, at zero.
    (target::VCO1_PITCH, source::GLIDE),
    (target::VCO2_PITCH, source::GLIDE),
    // The one VCO LFO slider, on the same DESTINATION. Depth was `vcolfo`, at zero.
    (target::VCO1_PITCH, source::LFO1),
    (target::VCO2_PITCH, source::LFO1),
    // VCA LFO ← LFO-1 (p. 20). Depth was `vcalfo`, at zero.
    (target::TREMOLO, source::LFO1),
    // VCF KYBD CV (p. 19). Depth was `keytrack`, at zero.
    (target::CUTOFF, source::KEY),
    // VCF LFO IN ← LFO-1, on the cutoff now. Depth was `filterlfo`.
    (target::CUTOFF, source::LFO1),
    // VCF ADSR IN ← Envelope 1, on the cutoff now. Depth was `filterenv`.
    (target::CUTOFF, source::VCF_ADSR),
];

/// **The normalled connections that are the sound of a fresh instance, and so start at full.**
///
/// This is the collision `plan-modulation-routing.md` §7.1 names between decision 1.6 — every route
/// carries a level — and the init contract's *every amount starts at zero*. It is resolved per row,
/// and the answer is the same each time: **a route whose depth was not a control before the
/// conversion has no zero to inherit.** Three of the first four were not amounts at all; they were
/// the jack's internal connection, which is either made or not. The fifth is a patch, the owner's.
///
/// `plugins/mxm-mono-00/AGENTS.md` records them as init deviations beside the three it already has.
pub const INIT_AT_FULL: [(usize, usize); 5] = [
    // Both envelope gate rows ← the keyboard gate. Without these a fresh instance never sounds.
    (target::VCF_GATE, source::GATE),
    (target::VCA_GATE, source::GATE),
    // RING MOD IN ← VCO-1 (p. 18). The ring modulator's Y input was VCO-1 at full; its output is
    // inaudible at init anyway, because the mixer channel that carries it is at zero.
    (target::RING_INPUT, source::VCO1),
    // VCA ADSR IN ← Envelope 2. Its depth *was* an amount — `vcaenv` — and it already started at
    // 1.0, which `plugins/mxm-mono-00/AGENTS.md` records as one of the three things that start up
    // because a silent init patch reads as broken. So this one inherits its old default rather
    // than acquiring a new one.
    (target::AMPLIFIER, source::VCA_ADSR),
    // S&H INPUT ← Noise, the manual's own suggestion for the EXT position (p. 14): with nothing
    // sampled, an LFO set to S&H was a flat line (the owner, 2026-10-08: "When I select sample
    // and hold here i cannot hear it"; ruling: Init routes Noise in, visible and removable). Init
    // sounds the same, since nothing reads the S&H at Init.
    (target::SH_INPUT, source::NOISE),
];

/// The plug-out's SYNC IN normal, **deliberately absent at init**.
///
/// SYNC IN is normalled to VCO-1 SYNC OUT, and the machine's SYNC switch decides whether the circuit
/// acts on it. Under pairs the presence *is* that switch — a route that exists syncs — so the
/// retired `sync` parameter has no second authority to be, and a fresh instance, whose switch was
/// off, wires nothing. Turning sync on is adding this pair, which is one gesture, and
/// `syncstrength` still chooses the hardware's strong or weak reset (wart 17, untouched).
pub const SYNC_NORMAL: (usize, usize) = (target::VCO2_SYNC, source::VCO1_SYNC);

/// Which sources are live into which targets, and how much of each.
///
/// **Presence is what the DSP reads.** An absent route contributes nothing whatever its amount
/// holds, which is what makes removing a source one parameter write and re-adding it restore the
/// depth the player last set.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Routing {
    /// Per target, per source: whether that route exists.
    pub present: [[bool; SOURCES]; TARGETS],
    /// Per target, per source: how much, signed, as a fraction of [`FULL_SCALE`] — except on
    /// [`target::VCO2_SYNC`], where it is reset depth.
    pub amounts: [[f32; SOURCES]; TARGETS],
}

impl Default for Routing {
    fn default() -> Self {
        Self::init()
    }
}

impl Routing {
    /// Nothing routed anywhere: every jack pulled.
    #[must_use]
    pub const fn new() -> Self {
        Self {
            present: [[false; SOURCES]; TARGETS],
            amounts: [[0.0; SOURCES]; TARGETS],
        }
    }

    /// **The plug-out as it comes**: [`INIT_PRESENT`] at zero depth and [`INIT_AT_FULL`] at one.
    ///
    /// This is the whole of the machine's hard wiring, and it is data rather than code — which is
    /// what makes every one of those connections something a player can pull out.
    #[must_use]
    pub const fn init() -> Self {
        let mut routing = Self::new();
        let mut i = 0;
        while i < INIT_PRESENT.len() {
            let (t, s) = INIT_PRESENT[i];
            routing.present[t][s] = true;
            i += 1;
        }
        let mut j = 0;
        while j < INIT_AT_FULL.len() {
            let (t, s) = INIT_AT_FULL[j];
            routing.present[t][s] = true;
            routing.amounts[t][s] = 1.0;
            j += 1;
        }
        routing
    }

    /// Adds one route at a depth. For tests and for building a patch by hand.
    pub fn set(&mut self, target: usize, source: usize, amount: f32) {
        self.present[target][source] = true;
        self.amounts[target][source] = amount;
    }

    /// Removes one route, leaving its depth where it was — the shared crate's contract.
    pub fn clear(&mut self, target: usize, source: usize) {
        self.present[target][source] = false;
    }

    /// Removes every route into one target.
    pub fn clear_target(&mut self, target: usize) {
        self.present[target] = [false; SOURCES];
    }
}

/// The voice's routing state: one frame, one compacted list per target, and the two audio fades.
///
/// Compaction runs **once per processing interval**, not per sample, because topology is discrete
/// and changes only on a parameter event. The per-sample loop then runs over the live routes rather
/// than over all 300 pairs.
#[derive(Debug, Clone)]
pub struct Graph {
    frame: SourceFrame<SOURCES>,
    live: [Compacted<SOURCES>; TARGETS],
    /// A uniform row's live routes split in two: the row's own ([`FULL_SCALE`]) and those with an
    /// [`ADDED_SCALE`]. Rebuilt with `live`.
    network: [Compacted<SOURCES>; TARGETS],
    added: [Compacted<SOURCES>; TARGETS],
    /// Which sources any live route actually reads, cached at [`Graph::set_topology`].
    ///
    /// **A source nothing reads is not published.** The init patch wires six sources into nine
    /// routes, where the instrument declares twenty.
    needed: [bool; SOURCES],
    /// Per audio target, per source: how much of that route is currently sounding, `0..=1`.
    fade: [[f32; SOURCES]; AUDIO_TARGETS.len()],
    /// Whether [`Graph::set_topology`] has ever run.
    ///
    /// **A voice that is never armed has no live routes and renders silence**, which is a bug that
    /// looks like a quiet patch. `mxm-mono-pr1`'s conversion shipped exactly that once, and the
    /// sampler's did too, so here it panics in debug rather than being discovered by ear.
    armed: bool,
    /// Whether the next topology pass should **snap** the audio fades instead of ramping them.
    ///
    /// A fade is for a *change*, and the first topology a voice is given is not one: the
    /// plug-out's normalled audio connections are wired from power-on, so a fresh instance must
    /// hear the ring modulator on the mixer's channel immediately rather than over five
    /// milliseconds. A reset is the same case — it is not a player's edit — so it sets this too.
    snap: bool,
    /// Per audio target: the routes that are present **or still fading out**.
    ///
    /// A removed audio route keeps contributing until its gain reaches zero, so the list an audio
    /// target walks is the union rather than the presences. It is rebuilt at each topology change
    /// and only shrinks there, so a settled route costs one multiply by zero until the next
    /// change — bounded, and cheaper than scanning twenty sources every sample.
    fading: [Compacted<SOURCES>; AUDIO_TARGETS.len()],
}

impl Default for Graph {
    fn default() -> Self {
        Self::new()
    }
}

impl Graph {
    /// An empty graph.
    #[must_use]
    pub const fn new() -> Self {
        Self {
            frame: SourceFrame::new(),
            live: [const { Compacted::new() }; TARGETS],
            network: [const { Compacted::new() }; TARGETS],
            added: [const { Compacted::new() }; TARGETS],
            needed: [false; SOURCES],
            fade: [[0.0; SOURCES]; AUDIO_TARGETS.len()],
            armed: false,
            snap: true,
            fading: [const { Compacted::new() }; AUDIO_TARGETS.len()],
        }
    }

    /// Forgets every sample of history. Called from `Voice::reset`.
    ///
    /// **The audio fades are snapped to the topology in force, not zeroed**, so a reset that is
    /// not followed by a topology pass still has the wired routes at full — a reset is not a
    /// player pulling a cord out. The next pass snaps too, for the same reason.
    pub fn reset(&mut self) {
        self.frame.reset();
        for (slot, &target) in AUDIO_TARGETS.iter().enumerate() {
            let mut present = [false; SOURCES];
            for &source in self.live[target].sources() {
                present[source] = true;
            }
            for (gain, &on) in self.fade[slot].iter_mut().zip(present.iter()) {
                *gain = if on { 1.0 } else { 0.0 };
            }
            self.fading[slot].build(&present);
        }
        self.snap = true;
    }

    /// Rebuilds which routes are live. Call once per interval, **never per sample**.
    ///
    /// **A source that becomes needed starts from silence.** While nothing read it, nothing
    /// published it, so its slot still holds whatever it held the last time something did — which
    /// may be from a different phrase entirely. A backward route added to a running voice would
    /// then read that ancient value for exactly one sample. Clearing the slot makes the first
    /// sample a deterministic zero instead: bounded, the same however long the source went unread,
    /// and therefore independent of how the host split its buffers.
    pub fn set_topology(&mut self, routing: &Routing) {
        self.armed = true;
        for (live, present) in self.live.iter_mut().zip(routing.present.iter()) {
            live.build(present);
        }
        for (t, present) in routing.present.iter().enumerate() {
            let split = |added: bool| {
                std::array::from_fn(|s| present[s] && (ADDED_SCALE[t][s] != 0.0) == added)
            };
            self.network[t].build(&split(false));
            self.added[t].build(&split(true));
        }
        let was_needed = self.needed;
        self.needed = [false; SOURCES];
        for present in routing.present.iter() {
            for (needed, &on) in self.needed.iter_mut().zip(present.iter()) {
                *needed |= on;
            }
        }
        for (source, (&needed, &before)) in self.needed.iter().zip(was_needed.iter()).enumerate() {
            if needed && !before {
                self.frame.clear(source);
            }
        }
        // An audio target walks the routes that are present *or* still fading out, so a route the
        // player has just removed can ramp down instead of stopping dead.
        for (slot, &target) in AUDIO_TARGETS.iter().enumerate() {
            let mut union = [false; SOURCES];
            for (out, (&present, &gain)) in union
                .iter_mut()
                .zip(routing.present[target].iter().zip(self.fade[slot].iter()))
            {
                *out = present || gain > 0.0;
            }
            self.fading[slot].build(&union);
            // A source only this fade still reads must keep being published, or the tail fades
            // from a frozen value instead of from the audio it was carrying.
            for (source, &on) in union.iter().enumerate() {
                self.needed[source] |= on;
            }
        }
        if self.snap {
            for (slot, &target) in AUDIO_TARGETS.iter().enumerate() {
                for (gain, &on) in self.fade[slot]
                    .iter_mut()
                    .zip(routing.present[target].iter())
                {
                    *gain = if on { 1.0 } else { 0.0 };
                }
            }
            self.snap = false;
        }
    }

    /// Whether anything reads this source, so a caller can skip producing a value for it.
    #[inline]
    #[must_use]
    pub fn needs(&self, source: usize) -> bool {
        self.needed[source]
    }

    /// Whether this target has any live route at all.
    #[inline]
    #[must_use]
    pub fn is_routed(&self, target: usize) -> bool {
        !self.live[target].is_empty()
    }

    /// The live sources into one target, in source order.
    #[inline]
    #[must_use]
    pub fn sources(&self, target: usize) -> &[usize] {
        self.live[target].sources()
    }

    /// Opens a sample.
    #[inline]
    pub fn begin_sample(&mut self) {
        debug_assert!(
            self.armed,
            "the voice was never given a topology: every path to `process` owes `set_topology`"
        );
        self.frame.begin_sample();
    }

    /// Publishes a source's value for this sample, in **ten-volt units**.
    ///
    /// Gated on [`Graph::needs`]: a source nothing reads costs nothing. The frame bounds what it
    /// stores, which is what keeps a cycle a player closes finite.
    #[inline]
    pub fn write(&mut self, source: usize, value: f32) {
        if self.needed[source] {
            self.frame.write(source, value);
        }
    }

    /// Publishes a column's value, for the fourteen sources that are columns.
    #[inline]
    pub fn write_column(&mut self, column: Column, value: f32) {
        self.write(column.index(), value);
    }

    /// This sample's value for a source, or last sample's if nothing has published it yet — the
    /// unit delay on a backward connection.
    #[inline]
    #[must_use]
    pub fn read(&self, source: usize) -> f32 {
        self.frame.read(source)
    }

    /// Last sample's value, whatever this sample has done. For telemetry and for tests.
    #[inline]
    #[must_use]
    pub fn previous(&self, source: usize) -> f32 {
        self.frame.previous(source)
    }

    /// One target's summed modulation, **in its own domain**.
    ///
    /// Not for the two audio targets, which fade — [`Graph::sum_audio`] — nor for
    /// [`target::VCO2_SYNC`], whose law is not a sum at all.
    #[inline]
    #[must_use]
    pub fn sum(&self, target: usize, routing: &Routing) -> f32 {
        debug_assert!(
            LAW[target] != Law::Sync,
            "a sync target's amount is reset depth, not a scale"
        );
        debug_assert!(
            audio_slot(target).is_none(),
            "an audio target fades: use sum_audio"
        );
        // Nothing routed is nothing, and most of this instrument's inputs are unrouted.
        if self.live[target].is_empty() {
            return 0.0;
        }
        let summed = if UNIFORM[target] {
            // `(Σ amount × source) × scale`, the order the pinned conversion digests were taken in —
            // to the bit while no pair with an added scale is live, which `sum_split` keeps.
            mxm_modulation::sum_split(
                &self.frame,
                &self.network[target],
                &routing.amounts[target],
                FULL_SCALE[target][0],
                &self.added[target],
                &ADDED_SCALE[target],
                SUM_BOUND,
            )
        } else {
            // A scale per route, applied last in each — the shared crate's own order.
            mxm_modulation::sum_scaled(
                &self.frame,
                &self.live[target],
                &routing.amounts[target],
                &FULL_SCALE[target],
                TARGET_BOUND[target],
            )
        };
        // **In the target's own domain**, which is the only place a limit means anything: see
        // [`TARGET_BOUND`].
        summed.clamp(-TARGET_BOUND[target], TARGET_BOUND[target])
    }

    /// One audio target's sum, **with every route's own contribution ramped**.
    ///
    /// The switching jack's crossfade, generalised. Before the conversion a row faded from the
    /// source it was leaving to the one it was taking; now each route fades its own contribution in
    /// or out over [`AUDIO_ROUTE_RAMP_S`], which is the same audio when one route replaces another
    /// and is the only form that also covers adding a second or removing the last.
    ///
    /// Advances state, so it is called **exactly once per sample per target**.
    #[inline]
    pub fn sum_audio(&mut self, target: usize, routing: &Routing, sample_rate: f32) -> f32 {
        let slot = audio_slot(target).expect("an audio target");
        let step = 1.0 / (AUDIO_ROUTE_RAMP_S * sample_rate).max(1.0);
        let mut total = 0.0;
        for &s in self.fading[slot].sources() {
            let want = if routing.present[target][s] { 1.0 } else { 0.0 };
            let gain = &mut self.fade[slot][s];
            if *gain < want {
                *gain = (*gain + step).min(want);
            } else if *gain > want {
                *gain = (*gain - step).max(want);
            }
            total += (routing.amounts[target][s] * self.frame.read(s)) * *gain;
        }
        if total.is_finite() {
            // Both audio targets are uniform, so one column is the whole row.
            let scaled = total.clamp(-SUM_BOUND, SUM_BOUND) * FULL_SCALE[target][0];
            scaled.clamp(-TARGET_BOUND[target], TARGET_BOUND[target])
        } else {
            0.0
        }
    }

    /// Whether an audio target is carrying anything at all — present routes, or a tail still fading.
    #[inline]
    #[must_use]
    pub fn audio_is_sounding(&self, target: usize) -> bool {
        match audio_slot(target) {
            Some(slot) => !self.fading[slot].is_empty(),
            None => false,
        }
    }

    /// Whether an audio target **carries** anything: a route present or still fading out, at a
    /// depth that is not zero, from a source `sounds` says is sounding. A route at zero depth is
    /// audible as nothing, so it must not make a silent patch live — the same rule every other
    /// activity predicate here follows. Which sources sound is the voice's to say.
    #[inline]
    #[must_use]
    pub fn audio_carries(
        &self,
        target: usize,
        routing: &Routing,
        sounds: impl Fn(usize) -> bool,
    ) -> bool {
        match audio_slot(target) {
            Some(slot) => self.fading[slot].sources().iter().any(|&s| {
                routing.amounts[target][s] != 0.0
                    && (routing.present[target][s] || self.fade[slot][s] > 0.0)
                    && sounds(s)
            }),
            None => false,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::matrix::COLUMNS;

    #[test]
    fn the_first_fourteen_sources_are_the_columns_at_their_own_indices() {
        // The translation table this saves is the point: `Column::index` *is* a source index.
        assert_eq!(COLUMNS, 14);
        assert_eq!(Column::KybdCv.index(), source::KEY);
        assert_eq!(Column::Lfo1.index(), source::LFO1);
        assert_eq!(Column::VcaAdsr.index(), source::VCA_ADSR);
        for (i, c) in Column::ALL.iter().enumerate() {
            assert_eq!(c.index(), i);
        }
        const { assert!(SOURCES > COLUMNS) };
    }

    #[test]
    fn every_target_and_source_is_named() {
        assert_eq!(TARGET_NAMES.len(), TARGETS);
        // The painted names need not be unique across the instrument — each is read inside one
        // card — but every target must have one.
        assert_eq!(TARGET_PANEL_NAMES.len(), TARGETS);
        for name in TARGET_PANEL_NAMES {
            assert!(!name.is_empty());
        }
        assert_eq!(SOURCE_NAMES.len(), SOURCES);
        for (i, a) in TARGET_NAMES.iter().enumerate() {
            assert!(!a.is_empty());
            for b in &TARGET_NAMES[i + 1..] {
                assert_ne!(a, b, "two targets share a name");
            }
        }
        for (i, a) in SOURCE_NAMES.iter().enumerate() {
            assert!(!a.is_empty());
            for b in &SOURCE_NAMES[i + 1..] {
                assert_ne!(a, b, "two sources share a name");
            }
        }
    }

    #[test]
    fn the_init_patch_is_the_plug_outs_wiring_and_nothing_else() {
        let init = Routing::init();
        let mut wired = 0;
        for t in 0..TARGETS {
            for s in 0..SOURCES {
                if init.present[t][s] {
                    wired += 1;
                }
            }
        }
        assert_eq!(wired, INIT_PRESENT.len() + INIT_AT_FULL.len());
        // The four that inherit a zero attenuator.
        for &(t, s) in &INIT_PRESENT {
            assert!(init.present[t][s]);
            assert_eq!(init.amounts[t][s], 0.0);
        }
        // The five whose depth was never a control.
        for &(t, s) in &INIT_AT_FULL {
            assert!(init.present[t][s]);
            assert_eq!(init.amounts[t][s], 1.0);
        }
        // And the SYNC normal is *not* wired: the switch it lived behind was off.
        let (t, s) = SYNC_NORMAL;
        assert!(!init.present[t][s]);
    }

    #[test]
    fn a_target_with_no_route_reads_zero_and_costs_nothing() {
        let mut g = Graph::new();
        let routing = Routing::new();
        g.set_topology(&routing);
        g.begin_sample();
        for (t, law) in LAW.iter().enumerate() {
            if *law == Law::Sync || audio_slot(t).is_some() {
                continue;
            }
            assert_eq!(g.sum(t, &routing), 0.0);
        }
        for s in 0..SOURCES {
            assert!(!g.needs(s), "nothing is routed, so nothing is published");
        }
    }

    #[test]
    fn a_present_route_at_zero_depth_is_a_route_and_still_publishes_its_source() {
        let mut g = Graph::new();
        let mut routing = Routing::new();
        routing.set(target::CUTOFF, source::LFO1, 0.0);
        g.set_topology(&routing);
        assert!(g.is_routed(target::CUTOFF));
        assert!(g.needs(source::LFO1));
        g.begin_sample();
        g.write(source::LFO1, 1.0);
        assert_eq!(g.sum(target::CUTOFF, &routing), 0.0);
    }

    #[test]
    fn an_absent_routes_depth_is_kept_and_contributes_nothing() {
        let mut g = Graph::new();
        let mut routing = Routing::new();
        routing.set(target::CUTOFF, source::LFO1, 0.5);
        routing.clear(target::CUTOFF, source::LFO1);
        g.set_topology(&routing);
        g.begin_sample();
        g.write(source::LFO1, 1.0);
        assert_eq!(g.sum(target::CUTOFF, &routing), 0.0);
        assert_eq!(routing.amounts[target::CUTOFF][source::LFO1], 0.5);
    }

    #[test]
    fn two_sources_into_one_input_sum_because_the_jack_is_a_mixer_now() {
        // Decision 1.6, and the single largest change this conversion makes to the machine.
        let mut g = Graph::new();
        let mut routing = Routing::new();
        routing.set(target::CUTOFF, source::LFO1, 0.5);
        routing.set(target::CUTOFF, source::LFO2, 0.25);
        g.set_topology(&routing);
        g.begin_sample();
        g.write(source::LFO1, 1.0);
        g.write(source::LFO2, 1.0);
        let expected = (0.5 + 0.25) * FILTER_LFO_OCTAVES;
        assert!((g.sum(target::CUTOFF, &routing) - expected).abs() < 1e-6);
    }

    #[test]
    fn an_audio_route_fades_in_over_the_ramp_and_out_again() {
        let fs = 48_000.0;
        let mut g = Graph::new();
        let mut routing = Routing::new();
        // The first pass snaps — a voice's opening topology is not a change — so the route has to
        // arrive on a *second* one for there to be a fade to watch at all.
        g.set_topology(&routing);
        routing.set(target::MIXER_INPUT, source::NOISE, 1.0);
        g.set_topology(&routing);
        // The first sample is a long way from full, and the ramp reaches it in AUDIO_ROUTE_RAMP_S.
        g.begin_sample();
        g.write(source::NOISE, VCO_OUT_UNITS);
        let first = g.sum_audio(target::MIXER_INPUT, &routing, fs);
        assert!(
            first < 0.1,
            "a route fades in rather than stepping: {first}"
        );
        let ramp = (AUDIO_ROUTE_RAMP_S * fs) as usize;
        for _ in 0..ramp {
            g.begin_sample();
            g.write(source::NOISE, VCO_OUT_UNITS);
            g.sum_audio(target::MIXER_INPUT, &routing, fs);
        }
        let settled = {
            g.begin_sample();
            g.write(source::NOISE, VCO_OUT_UNITS);
            g.sum_audio(target::MIXER_INPUT, &routing, fs)
        };
        assert!((settled - 1.0).abs() < 1e-6, "settled at {settled}");

        // Removing it fades out rather than stopping dead.
        routing.clear(target::MIXER_INPUT, source::NOISE);
        g.set_topology(&routing);
        g.begin_sample();
        g.write(source::NOISE, VCO_OUT_UNITS);
        let after = g.sum_audio(target::MIXER_INPUT, &routing, fs);
        assert!(after > 0.9, "a removal fades out: {after}");
        for _ in 0..ramp {
            g.begin_sample();
            g.write(source::NOISE, VCO_OUT_UNITS);
            g.sum_audio(target::MIXER_INPUT, &routing, fs);
        }
        g.begin_sample();
        g.write(source::NOISE, VCO_OUT_UNITS);
        assert_eq!(g.sum_audio(target::MIXER_INPUT, &routing, fs), 0.0);
    }

    #[test]
    fn a_source_only_a_fading_tail_reads_is_still_published() {
        // Otherwise the tail fades from a frozen value and the ramp does not do its job.
        let fs = 48_000.0;
        let mut g = Graph::new();
        let mut routing = Routing::new();
        routing.set(target::MIXER_INPUT, source::NOISE, 1.0);
        g.set_topology(&routing);
        routing.clear(target::MIXER_INPUT, source::NOISE);
        g.set_topology(&routing);
        assert!(g.needs(source::NOISE));
        assert!(g.audio_is_sounding(target::MIXER_INPUT));
        for _ in 0..(AUDIO_ROUTE_RAMP_S * fs) as usize + 2 {
            g.begin_sample();
            g.write(source::NOISE, VCO_OUT_UNITS);
            g.sum_audio(target::MIXER_INPUT, &routing, fs);
        }
        // Once it has settled the next topology pass drops it.
        g.set_topology(&routing);
        assert!(!g.needs(source::NOISE));
        assert!(!g.audio_is_sounding(target::MIXER_INPUT));
    }

    #[test]
    fn a_newly_needed_source_starts_from_silence_rather_than_an_ancient_value() {
        // `mxm-modulation`'s *A gated publication owes a clear*, on this instrument.
        let mut g = Graph::new();
        let mut routing = Routing::new();
        routing.set(target::CUTOFF, source::LFO1, 1.0);
        g.set_topology(&routing);
        g.begin_sample();
        g.write(source::LFO1, 1.0);
        assert!((g.sum(target::CUTOFF, &routing) - FILTER_LFO_OCTAVES).abs() < 1e-6);

        // Nothing reads it for a while, so nothing publishes it and the slot goes stale.
        routing.clear(target::CUTOFF, source::LFO1);
        g.set_topology(&routing);
        for _ in 0..100 {
            g.begin_sample();
        }
        // Re-added, a *backward* read must be zero rather than that ancient 1.0.
        routing.set(target::CUTOFF, source::LFO1, 1.0);
        g.set_topology(&routing);
        g.begin_sample();
        assert_eq!(g.read(source::LFO1), 0.0);
    }

    #[test]
    #[should_panic(expected = "never given a topology")]
    fn a_voice_that_was_never_armed_says_so_rather_than_rendering_silence() {
        Graph::new().begin_sample();
    }

    #[test]
    fn a_cycle_through_the_matrix_stays_finite() {
        // The mixer into its own EXT IN, which the plug-out's manual permits and which the unit
        // delay plus the frame's bound is what makes evaluable and bounded.
        let fs = 48_000.0;
        let mut g = Graph::new();
        let mut routing = Routing::new();
        routing.set(target::MIXER_INPUT, source::MIXER_OUT, 1.0);
        g.set_topology(&routing);
        let mut feedback = 0.0f32;
        for _ in 0..10_000 {
            g.begin_sample();
            g.write(source::MIXER_OUT, (0.5 + feedback).clamp(-1.0, 1.0));
            feedback = g.sum_audio(target::MIXER_INPUT, &routing, fs);
            assert!(feedback.is_finite());
        }
        assert!(feedback.abs() <= SUM_BOUND);
    }

    #[test]
    fn a_reset_leaves_no_tail() {
        let fs = 48_000.0;
        let mut g = Graph::new();
        let mut routing = Routing::new();
        routing.set(target::MIXER_INPUT, source::NOISE, 1.0);
        g.set_topology(&routing);
        for _ in 0..1000 {
            g.begin_sample();
            g.write(source::NOISE, VCO_OUT_UNITS);
            g.sum_audio(target::MIXER_INPUT, &routing, fs);
        }
        g.reset();
        g.begin_sample();
        assert_eq!(g.read(source::NOISE), 0.0);
        assert_eq!(g.previous(source::NOISE), 0.0);
        // The *frame* is cleared, but the fade is snapped to the topology in force rather than
        // zeroed: a reset is not a player pulling the cord out, so the route is still at full.
        g.write(source::NOISE, VCO_OUT_UNITS);
        let first = g.sum_audio(target::MIXER_INPUT, &routing, fs);
        assert_eq!(first, 1.0, "a reset route is still wired: {first}");
    }

    #[test]
    fn a_voices_opening_topology_snaps_rather_than_fading_in() {
        // The plug-out's normalled audio connections are wired from power-on. Fading them in over
        // the first five milliseconds made a fresh instance's ring modulator arrive late.
        let mut g = Graph::new();
        let routing = Routing::init();
        g.set_topology(&routing);
        g.begin_sample();
        g.write(source::VCO1, VCO_OUT_UNITS);
        let first = g.sum_audio(target::RING_INPUT, &routing, 48_000.0);
        assert_eq!(first, 1.0, "the normalled ring input faded in: {first}");
    }
}
