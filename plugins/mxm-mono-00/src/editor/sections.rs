//! The instrument's information architecture: which parameter sits in which section, in which
//! order, and the sentence each carries in its tooltip — and how each card is composed.
//!
//! The brief (`docs/briefs/mxm-mono-00.md` §10) owns the section order; this file implements it.
//! [`SECTIONS`] is written in that order so a reordering is a visible diff rather than a drift,
//! and every surface draws from it: the twelve `Synth` cards, the Effects card, the app bar's
//! Volume and the `Parameters` list — so one list is the
//! whole of what is drawn, and `every_parameter_is_drawn_exactly_once` can ask it.
//!
//! # Every card says which rows feed it
//!
//! Source menus are sublabels directly beneath their controls, not a detached routing footer.
//! There is no Patch page or matrix widget in the plugin. [`Section::rows`] owns the mapping;
//! `tests/routing_editor.rs` clicks every target in the shipped panel so a
//! layout rewrite cannot quietly turn these controls back into passive captions.

use std::collections::HashMap;

use egui::Ui;
use mxm_mono_00_dsp::matrix::Column;
use mxm_mono_00_dsp::routing::{TARGET_NAMES, target};
use mxm_ui::control::{Size, Wave};
use mxm_ui::space::{SPACE_1, SPACE_3, SPACE_4};
use mxm_ui::theme::Tokens;
use mxm_ui::tree::{
    self, Height, Kind, Node, leaf, pad, pad_all, row_gap, stack, switch_beside_knob,
};
use nice_plug::prelude::ParamSetter;

use super::binding::{
    Bound, ErasedParam, segmented, segmented_named, segmented_waves_named, toggle_labelled,
};
use crate::params::MxmMono00Params;

/// The brief's §10 order, then routing metadata for Parameters, the effects, and the output that
/// ends the chain after them.
pub const SECTIONS: &[Section] = &[
    Section::Voice,
    Section::Modulator1,
    Section::Modulator2,
    Section::SampleHold,
    Section::Oscillator1,
    Section::Oscillator2,
    Section::RingMod,
    Section::Mixer,
    Section::Filter,
    Section::EnvelopeVcf,
    Section::Amplifier,
    Section::EnvelopeVca,
    Section::Routing,
    Section::Fx,
    Section::Output,
];

/// The twelve cards of the reflowing `Synth` view, in [`SECTIONS`]' signal-chain order.
pub const SYNTH: [Section; 12] = [
    Section::Voice,
    Section::Modulator1,
    Section::Modulator2,
    Section::SampleHold,
    Section::Oscillator1,
    Section::Oscillator2,
    Section::RingMod,
    Section::Mixer,
    Section::Filter,
    Section::EnvelopeVcf,
    Section::Amplifier,
    Section::EnvelopeVca,
];

/// **Each Synth card's permanent paging key.** Keys are identities — the developer channel, the
/// capture scripts and the keyboard cursor's map name cards by them — so a card added later takes the
/// next free key rather than renumbering the ones after it. The ring modulator's card came after the
/// three effects had 11–13, so it is **14**, between Oscillator 2 (5) and the Mixer (6). The
/// Effects card kept 11 when the three merged; 12 and 13 are retired, never reused.
pub const SYNTH_KEYS: [u64; 12] = [0, 1, 2, 3, 4, 5, 14, 6, 7, 8, 9, 10];

/// The runs a row break may not fall inside — design system §3.4.
///
/// The keyboard block; the paired LFOs; sample-and-hold; the two oscillators, which are parallel branches and read as parallel only side by side; the mixer; and
/// each envelope with the block it is wired to.
pub const SYNTH_GROUPS: &[&[usize]] = &[&[0], &[1, 2], &[3], &[4, 5, 6], &[7], &[8, 9], &[10, 11]];

#[derive(Copy, Clone, Debug, PartialEq, Eq)]
pub enum Section {
    Oscillator1,
    Oscillator2,
    /// The ring modulator, a generator of its own: VCO-2 × whatever is routed to its input.
    RingMod,
    Mixer,
    Filter,
    Amplifier,
    EnvelopeVcf,
    EnvelopeVca,
    Modulator1,
    Modulator2,
    Voice,
    SampleHold,
    /// Parameter-list grouping only; never a routing page or a Synth card.
    Routing,
    Fx,
    /// The instrument's output level, drawn in the app bar beside the meter (design system §3.1
    /// item 6) rather than on a card. Never a Synth card.
    Output,
}

const PRIMARY: Size = Size::Primary;
const STANDARD: Size = Size::Standard;
const COMPACT: Size = Size::Compact;

const WAVES: &[(Wave, &str)] = &[
    (Wave::RampUp, "Sawtooth"),
    (Wave::Square, "Square"),
    (Wave::Triangle, "Triangle"),
];
const LFO_SHAPES: &[(Wave, &str)] = &[
    (Wave::Sine, "Sine"),
    (Wave::Triangle, "Triangle"),
    (Wave::RampUp, "Sawtooth"),
    (Wave::Square, "Square"),
    (Wave::Random, "S&H"),
];

impl Section {
    /// Card names from the collection's vocabulary, numbered where there are two.
    pub const fn title(self) -> &'static str {
        match self {
            Self::Oscillator1 => "Oscillator 1",
            Self::Oscillator2 => "Oscillator 2",
            Self::RingMod => "Ring mod",
            Self::Mixer => "Mixer",
            Self::Filter => "Filter",
            Self::Amplifier => "Amplifier",
            Self::EnvelopeVcf => "Envelope 1",
            Self::EnvelopeVca => "Envelope 2",
            Self::Modulator1 => "LFO 1",
            Self::Modulator2 => "LFO 2",
            Self::Voice => "Voice",
            Self::SampleHold => "Sample & Hold",
            Self::Routing => "Routing",
            Self::Fx => "FX",
            Self::Output => "Output",
        }
    }

    /// Routing **targets** owned by this card; their stacks sit beneath the controls they move.
    ///
    /// Each goes to the card whose module *reads* it: a gate input to its envelope, the
    /// amplifier's inputs to the amplifier, a rate input to the LFO it bends, a pitch or a width to
    /// its oscillator. **The source is chosen here, never at the source**: the plug-out's
    /// DESTINATION switch, its fixed VCA LFO and KYBD CV, its PWM switches and SAMPLE MODE all
    /// became stacks on the card they move (`plans/plan-mxm-mono-00-modulation.md` Rev 3), so no
    /// card anywhere chooses where its own output goes.
    pub const fn targets(self) -> &'static [usize] {
        match self {
            Self::Oscillator1 => &[target::VCO1_PITCH, target::VCO1_WIDTH],
            Self::Oscillator2 => &[target::VCO2_PITCH, target::VCO2_WIDTH, target::VCO2_SYNC],
            Self::RingMod => &[target::RING_INPUT],
            Self::Mixer => &[target::MIXER_INPUT],
            Self::Filter => &[target::CUTOFF],
            Self::Amplifier => &[target::AMPLIFIER, target::TREMOLO, target::AMPLITUDE],
            Self::EnvelopeVcf => &[target::VCF_GATE],
            Self::EnvelopeVca => &[target::VCA_GATE],
            Self::Modulator1 => &[target::LFO1_RATE],
            Self::Modulator2 => &[target::LFO2_RATE],
            Self::Voice => &[],
            Self::SampleHold => &[target::SH_INPUT],
            Self::Routing | Self::Output => &[],
            Self::Fx => &[target::PHASER_RATE, target::PHASER_CENTRE],
        }
    }

    /// Which parameters this section draws, in order.
    pub const fn parameters(self) -> &'static [&'static str] {
        match self {
            Self::Oscillator1 => &["range1", "coarse1", "fine1", "wave1", "pw1"],
            Self::Oscillator2 => &["range2", "coarse2", "fine2", "wave2", "pw2", "syncstrength"],
            Self::RingMod => &[],
            Self::Mixer => &[
                "vco1level",
                "vco2level",
                "noiselevel",
                "ringmodlevel",
                "noisecolour",
            ],
            Self::Filter => &["hpf", "cutoff", "resonance"],
            Self::Amplifier => &["initialgain", "tone"],
            Self::EnvelopeVcf => &[
                "vcfattack",
                "vcfdecay",
                "vcfsustain",
                "vcfrelease",
                "vcftrigger",
            ],
            Self::EnvelopeVca => &[
                "vcaattack",
                "vcadecay",
                "vcasustain",
                "vcarelease",
                "vcatrigger",
            ],
            Self::Modulator1 => &["lfo1rate", "lfo1shape", "lfo1offset", "lfo1sync"],
            Self::Modulator2 => &["lfo2rate", "lfo2shape", "lfo2offset", "lfo2sync"],
            Self::Voice => &["priority", "portamento", "tune", "bendrange"],
            Self::SampleHold => &["shrate", "shsync", "shlag"],
            // **Routing pairs are not listed here.** This inventory is the permanent musician
            // controls each card owns, and a route is revealed by its own presence: at Init
            // thirteen of 425 pairs are drawn at all, so listing them would assert a card contains controls
            // that are deliberately absent. `Section::targets` is what covers them instead, and
            // `tests/routing_editor.rs` is what drives them.
            Self::Routing => &[],
            Self::Fx => &["phaser", "delay", "delaytime", "temposync", "reverb"],
            Self::Output => &["volume"],
        }
    }
}

// --------------------------------------------------------------------------------------------
// The matrix, named
// --------------------------------------------------------------------------------------------

/// A source's canonical name, shared by cards, route rows and host parameter names.
///
/// **One vocabulary, and it lives in the DSP now.** It used to be the retired `Source` enum's
/// variant list; `crate::routes` builds every route's name from the same array, so a card's title
/// and the row that names it as a source cannot drift.
pub fn column_label(column: Column) -> &'static str {
    mxm_mono_00_dsp::routing::SOURCE_NAMES[column.index()]
}

/// A source's name over a cell 36 points wide. The full name is the tooltip and the accessible
/// name; this is what fits.
pub const fn column_short(column: Column) -> &'static str {
    match column {
        Column::KybdCv => "KYBD",
        Column::Lfo1 => "LFO 1",
        Column::Lfo2 => "LFO 2",
        Column::ShOut => "S&H",
        Column::ShClock => "CLK",
        Column::Vco1 => "VCO-1",
        Column::Vco2 => "VCO-2",
        Column::Vco1Sync => "SYNC 1",
        Column::Vco2Sync => "SYNC 2",
        Column::RingMod => "RING",
        Column::Noise => "NOISE",
        Column::MixerOut => "MIX",
        Column::VcfAdsr => "VCF",
        Column::VcaAdsr => "VCA",
    }
}

// --------------------------------------------------------------------------------------------
// The cards, as trees (plans/plan-layout-tree.md)
//
// Each card is described once — `card` — and that one description is both measured (its floor and
// its height) and drawn, leaf by leaf, through the bindings below (`paint`). Nothing is typed and
// nothing is drawn to learn a size. The geometry is the hand layout's: knob rows in capped columns
// of 72, the slider grid's two stretched columns, the Range selector in its 88-point region, and
// every `add_space` of the old drawing code as a pad over the body's `SPACE_3` rhythm.
// --------------------------------------------------------------------------------------------

/// The one card the three effects share, after the twelve `Synth` cards: one operation, the chain
/// after the amplifier (design system §3.3; the owner, 2026-09-24). A card each left the reverb one
/// knob and a caption, which wrapped a word to a line once the card hugged its knob.
pub const EFFECTS: &str = "Effects";

/// Card `index`'s title, in paging order: the twelve [`SYNTH`] cards, then [`EFFECTS`].
#[must_use]
pub fn card_title(index: usize) -> &'static str {
    SYNTH.get(index).map_or(EFFECTS, |section| section.title())
}

/// What a leaf of this editor's cards draws. Hashed by what it names — a parameter id, a routing
/// target, a caption's text — which is also what keeps its widget ids stable when a route appears
/// above it.
#[derive(Clone, Debug, Hash)]
pub enum Leaf {
    Knob(&'static str, Size),
    Slider(&'static str),
    /// An on/off drawn as a picture: Tempo sync's quarter note.
    Picture(&'static str),
    /// A segmented switch, on the grid of the knob beside it when it has one.
    Switch(&'static str, Option<Size>),
    Waves(&'static str),
    Toggle(&'static str),
    /// One routing target's stack.
    Routes(usize),
}

/// A stepped parameter's cells, **labelled by the parameter itself**: each is its option's own
/// formatted value, so a cell reads what the host's automation list reads, and renaming an option
/// cannot leave a stale copy here.
fn options_of(params: &MxmMono00Params, id: &'static str) -> Vec<String> {
    let param = binding_for(id, params).param;
    let last = param
        .steps()
        .unwrap_or_else(|| unreachable!("{id} is not stepped"));
    (0..=last)
        .map(|option| param.format(option as f32 / last as f32))
        .collect()
}

/// A waveform parameter's pictures.
fn waves_of(id: &str) -> &'static [(Wave, &'static str)] {
    match id {
        "wave1" | "wave2" => WAVES,
        "lfo1shape" | "lfo2shape" => LFO_SHAPES,
        other => unreachable!("{other} has no pictures"),
    }
}

/// A row of knobs in **columns of their own width**, left to right — `mxm-mono-03`'s solution to
/// two knobs sitting 190 points apart in a card that was mostly gap. `ui.columns` inside a width
/// capped at `Σ max(diameter + KNOB_GUTTER, KNOB_COLUMN_MIN)`, so each column is that sum's share,
/// gaps included, and shrinks below it only as far as its widest knob allows.
fn knobs(ui: &Ui, params: &MxmMono00Params, knobs: &[(&'static str, Size)]) -> Node<Leaf> {
    mxm_ui::tree::knob_row(
        ui,
        knobs
            .iter()
            .map(|(id, size)| {
                (*size, {
                    let bound = binding_for(id, params);
                    // A syncable control's column holds its free readings and its divisions.
                    let widest = match sync_of(params, id) {
                        Some(sync) => super::binding::synced_widest(bound.param, sync.ladder.span),
                        None => mxm_ui::control::widest_value(|n| bound.param.format(n as f32)),
                    };
                    leaf(
                        Leaf::Knob(id, *size),
                        Kind::Knob {
                            name: bound.painted().to_owned(),
                            widest,
                            size: *size,
                            column: 0.0,
                        },
                    )
                })
            })
            .collect(),
    )
}

/// Levels that want comparing as the collection's fader row (`tree::fader_row`), in reading order:
/// a mixer's sources, an envelope's A, D, S and R.
fn faders(ui: &Ui, params: &MxmMono00Params, ids: &[&'static str]) -> Node<Leaf> {
    mxm_ui::tree::fader_row(
        ui,
        ids.iter()
            .map(|&id| {
                let bound = binding_for(id, params);
                mxm_ui::tree::fader(
                    Leaf::Slider(id),
                    bound.painted(),
                    mxm_ui::control::widest_value(|n| bound.param.format(n as f32)),
                )
            })
            .collect(),
    )
}

/// A stepped parameter as a segmented switch; `beside` puts it on that knob's grid.
fn switch(params: &MxmMono00Params, id: &'static str, beside: Option<Size>) -> Node<Leaf> {
    leaf(
        Leaf::Switch(id, beside),
        Kind::Segmented {
            label: binding_for(id, params).painted().to_owned(),
            options: options_of(params, id),
            beside,
        },
    )
}

fn waves(params: &MxmMono00Params, id: &'static str) -> Node<Leaf> {
    leaf(
        Leaf::Waves(id),
        Kind::Waves {
            label: Some(binding_for(id, params).painted().to_owned()),
            count: waves_of(id).len(),
            marks: Vec::new(),
            beside: None,
        },
    )
}

/// One target's routes, as the shared stack states its size: its narrowest is every route revealed
/// at its widest reading, its height the patch as it stands. `None` while the layout lab shows the
/// routing as controls of its own ([`routing_shown_as_controls`]), which draws no stack at all.
fn routes(ui: &Ui, params: &MxmMono00Params, target: usize) -> Option<Node<Leaf>> {
    if ui.data(|d| d.get_temp::<bool>(routing_shown_as_controls())) == Some(true) {
        return None;
    }
    let size = mxm_modulation_params::ui::stack_size(
        ui,
        mxm_mono_00_dsp::routing::TARGET_PANEL_NAMES[target],
        &params.routes.all()[target].1.offered_routes(target),
    );
    Some(leaf(
        Leaf::Routes(target),
        Kind::Custom {
            min_width: size.x,
            height: Height::Fixed(size.y),
            fills: true,
        },
    ))
}

/// A target's routes `SPACE_3` below the control they move, as the cards space them.
fn routes_below(ui: &Ui, params: &MxmMono00Params, target: usize) -> Option<Node<Leaf>> {
    routes(ui, params, target).map(|node| pad(SPACE_3, node))
}

/// The body of one `Synth` card, as a tree.
#[must_use]
pub fn synth_card(ui: &Ui, section: Section, params: &MxmMono00Params) -> Node<Leaf> {
    let gap = ui.spacing().item_spacing.x;
    let mut body: Vec<Option<Node<Leaf>>> = match section {
        // The octave as buttons — six, §7.3's range exception (the owner, 2026-09-27: every range
        // is buttons, never a drop-down) — then the waveform, drawn rather than spelled, on a row of
        // its own, since the two do not share a card's width; the tune and the manual width next, with
        // what moves the width beneath; the pitch input last. The PWM switch and its depth are
        // routes: a source narrows the pulse from square and the manual width stands aside (wart
        // 4). The pitch input is the plug-out's EXT CV jack — normalled to VCO-2 — with the glide
        // and VCO LFO that DESTINATION sent here, each now a source chosen on this card.
        Section::Oscillator1 => vec![
            Some(switch(params, "range1", None)),
            Some(pad(SPACE_3, waves(params, "wave1"))),
            Some(pad(
                SPACE_3,
                knobs(
                    ui,
                    params,
                    &[("coarse1", STANDARD), ("fine1", COMPACT), ("pw1", COMPACT)],
                ),
            )),
            routes_below(ui, params, target::VCO1_WIDTH),
            routes_below(ui, params, target::VCO1_PITCH),
        ],
        // The interval is what sync and the ring modulator turn into timbre: Standard. Its pitch
        // input carries the glide and VCO LFO halves of DESTINATION, and the 102's own EXT CV jack.
        // **The SYNC switch is the route's presence**, so there is no separate toggle: a source on
        // that input syncs, and its depth is how hard the reset pulls. STRONG and WEAK stay above
        // it, because that is a different question and the hardware's own switch.
        Section::Oscillator2 => vec![
            Some(switch(params, "range2", None)),
            Some(pad(SPACE_3, waves(params, "wave2"))),
            Some(pad(
                SPACE_3,
                knobs(
                    ui,
                    params,
                    &[("coarse2", STANDARD), ("fine2", COMPACT), ("pw2", COMPACT)],
                ),
            )),
            routes_below(ui, params, target::VCO2_PITCH),
            routes_below(ui, params, target::VCO2_WIDTH),
            Some(pad(SPACE_3, switch(params, "syncstrength", None))),
            routes_below(ui, params, target::VCO2_SYNC),
        ],
        // **A generator of its own**, beside the oscillators: its carrier is always VCO-2, which the
        // caption says because nothing else can, and what multiplies it is chosen here — Oscillator
        // 1 at Init. Its level is the Mixer's, beside the oscillators' levels.
        Section::RingMod => vec![routes(ui, params, target::RING_INPUT)],
        // Four levels as sliders — the two oscillators, noise and the ring modulator, as the 102's
        // mixer had its own RING MOD slider — because the point is comparing them (§7.1); two by
        // two, in reading order, so the card stays the height of two. **The whole card's width**:
        // with the noise's colour switch beside them the sliders came out 82 points wide and a
        // level's name collided with its neighbour's. The switch sits below, on a line of its own. **External input is the EXT IN jack**, the plug-out's shared RING MOD / EXT IN slider
        // split back into the 102's two: any source at a level of its own, and they add.
        Section::Mixer => vec![
            Some(faders(
                ui,
                params,
                &["vco1level", "vco2level", "noiselevel", "ringmodlevel"],
            )),
            Some(pad(SPACE_3, switch(params, "noisecolour", None))),
            routes_below(ui, params, target::MIXER_INPUT),
        ],
        // Cutoff and Resonance at Primary, adjacent — the brief's §2 — the other corner beside them
        // on the same grid. **One cutoff input**, as `mxm-mono-01` has: the VCF ADSR IN and LFO IN
        // jacks and KYBD CV all moved the one thing, and their normals — Envelope 1, LFO 1 and the
        // keyboard, whose depth is key follow — are wired here at zero at Init.
        Section::Filter => vec![
            Some(knobs(
                ui,
                params,
                &[
                    ("cutoff", PRIMARY),
                    ("resonance", PRIMARY),
                    ("hpf", COMPACT),
                ],
            )),
            routes_below(ui, params, target::CUTOFF),
        ],
        // Initial gain and the plug-out's tone tilt side by side at the top (the owner, 2026-09-25),
        // then what opens the VCA — Envelope 2 at full depth in the init patch, where its retired
        // `vcaenv` knob started — and the VCA LFO input, wired to LFO 1 at Init, which can only dip
        // the level (wart 3). Volume, which ends the chain after the effects, is in the app bar
        // beside the meter (§3.1).
        Section::Amplifier => vec![
            Some(knobs(
                ui,
                params,
                &[("initialgain", COMPACT), ("tone", COMPACT)],
            )),
            routes_below(ui, params, target::AMPLIFIER),
            routes_below(ui, params, target::TREMOLO),
            // The collection's standard Amplitude, after the VCA and its tremolo.
            routes_below(ui, params, target::AMPLITUDE),
        ],
        // The four times as sliders, because the point is comparing them (§7.1); two by two in
        // reading order, so the card stays the height of two.
        Section::EnvelopeVcf | Section::EnvelopeVca => {
            let ids = section.parameters();
            vec![
                Some(faders(ui, params, &ids[..4])),
                Some(pad(SPACE_3, switch(params, ids[4], None))),
                routes_below(ui, params, section.targets()[0]),
            ]
        }
        // The rate CV jack. Its one GAIN knob is each route's own depth now, which is what lets two
        // sources bend the rate at once — Roland's own FM-on-a-modulator trick, with a second
        // source available beside it.
        Section::Modulator1 | Section::Modulator2 => {
            let ids = section.parameters();
            // Rate with its tempo sync beside it (the owner, 2026-09-25), then the offset.
            vec![
                Some(row_gap(
                    gap,
                    vec![
                        knobs(ui, params, &[(ids[0], STANDARD)]),
                        switch_beside_knob(STANDARD, leaf(Leaf::Picture(ids[3]), Kind::SyncToggle)),
                        knobs(ui, params, &[(ids[2], COMPACT)]),
                    ],
                )),
                Some(pad(SPACE_3, waves(params, ids[1]))),
                routes_below(ui, params, section.targets()[0]),
            ]
        }
        Section::Voice => vec![Some(row_gap(
            gap,
            vec![
                switch(params, "priority", Some(STANDARD)),
                pad_all(
                    0.0,
                    SPACE_3,
                    0.0,
                    knobs(ui, params, &[("portamento", STANDARD)]),
                ),
            ],
        ))],
        // The sample time is the clock that runs the machine's most characteristic patches:
        // Standard. What is sampled — the retired SAMPLE MODE's positions are LFO 1's core shapes —
        // is chosen as sources beneath; nothing contributing is OFF.
        // Rate with its sync beside it, as the delay's time has its own: a quarter note, and with
        // it on, Rate's position is a division of the tempo and reads as one (the owner,
        // 2026-09-25). Then Lag.
        Section::SampleHold => vec![
            Some(row_gap(
                ui.spacing().item_spacing.x,
                vec![
                    knobs(ui, params, &[("shrate", STANDARD)]),
                    switch_beside_knob(STANDARD, leaf(Leaf::Picture("shsync"), Kind::SyncToggle)),
                    knobs(ui, params, &[("shlag", COMPACT)]),
                ],
            )),
            routes_below(ui, params, target::SH_INPUT),
        ],
        Section::Routing | Section::Fx | Section::Output => {
            unreachable!("{} is not a Synth view card", section.title())
        }
    };
    // The brief's §5: one expander, in the Voice card's footer, for what the plug-out puts on its
    // top bar rather than in a module. Both default to values that change nothing about the sound,
    // so a fresh patch is unaffected by never opening it. TEMPO SYNC, which the brief also names
    // here, sits with the delay it belongs to — a switch beside the control it changes. The card
    // reserves it open, so opening it never grows the card.
    if section == Section::Voice {
        body.push(Some(tree::disclosure(
            ui.ctx(),
            ADVANCED,
            ADVANCED_DESCRIPTION,
            knobs(ui, params, &[("tune", COMPACT), ("bendrange", COMPACT)]),
        )));
    }
    stack(body.into_iter().flatten().collect())
}

/// The Effects card as a tree: the three levels in processing order, with the delay's time and its
/// sync switch beside its level, then what moves the phaser. **The painted names keep their
/// prefixes** — the card holds three modules (design system §7.1). The delay's time reads its
/// division when synced, as every synced control does, so no caption says so.
#[must_use]
pub fn fx_card(ui: &Ui, params: &MxmMono00Params) -> Node<Leaf> {
    let gap = ui.spacing().item_spacing.x;
    let mut body = vec![Some(row_gap(
        gap,
        vec![
            knobs(ui, params, &[("phaser", STANDARD), ("delay", STANDARD)]),
            // The delay's sync on the time knob's grid: a quarter note, not the words (the owner,
            // 2026-09-25); *Delay sync* stays its accessible name. **With Delay time, not between
            // two knobs** (the owner, 2026-09-28: it sat closer to Reverb): the two are one group
            // `SPACE_1` apart, and Reverb stands `SPACE_4` further off.
            row_gap(
                SPACE_1,
                vec![
                    knobs(ui, params, &[("delaytime", STANDARD)]),
                    switch_beside_knob(
                        STANDARD,
                        leaf(Leaf::Picture("temposync"), Kind::SyncToggle),
                    ),
                ],
            ),
            pad_all(
                0.0,
                SPACE_4,
                0.0,
                knobs(ui, params, &[("reverb", STANDARD)]),
            ),
        ],
    ))];
    body.extend(
        Section::Fx
            .targets()
            .iter()
            .map(|&t| routes_below(ui, params, t)),
    );
    stack(body.into_iter().flatten().collect())
}

/// Card `index`'s body, in paging order: the twelve [`SYNTH`] cards, then the Effects card.
#[must_use]
pub fn card(ui: &Ui, index: usize, params: &MxmMono00Params) -> Node<Leaf> {
    match SYNTH.get(index) {
        Some(section) => synth_card(ui, *section, params),
        None => fx_card(ui, params),
    }
}

/// A syncable control's tempo sync: whether it is on, its ladder and its range.
pub struct Sync {
    pub on: bool,
    pub ladder: mxm_tempo::Ladder,
    pub lo: f64,
    pub hi: f64,
    /// The control's position as a person set it: what the knob reads a division from.
    pub position: f32,
}

/// The tempo sync of the control `id`, if it has one (`plans/plan-tempo-sync-controls.md`).
pub fn sync_of(params: &MxmMono00Params, id: &str) -> Option<Sync> {
    use nice_plug::prelude::Param as _;
    let (on, ladder, param) = match id {
        "delaytime" => (
            params.tempo_sync.value(),
            crate::params::DELAY_SYNC,
            &params.delay_time,
        ),
        "shrate" => (
            params.sh_sync.value(),
            crate::params::SH_SYNC,
            &params.sh_rate,
        ),
        "lfo1rate" => (
            params.lfo1_sync.value(),
            crate::params::LFO_SYNC,
            &params.lfo1_rate,
        ),
        "lfo2rate" => (
            params.lfo2_sync.value(),
            crate::params::LFO_SYNC,
            &params.lfo2_rate,
        ),
        _ => return None,
    };
    Some(Sync {
        on,
        ladder,
        lo: f64::from(param.preview_plain(0.0)),
        hi: f64::from(param.preview_plain(1.0)),
        position: param.unmodulated_normalized_value(),
    })
}

/// Everything a leaf draws with: the parameters, their host, and the text-entry buffers.
pub struct Live<'a, 'b> {
    pub params: &'a MxmMono00Params,
    pub setter: &'a ParamSetter<'b>,
    pub text_entry: &'a mut HashMap<&'static str, Option<String>>,
    /// The host tempo in force, read once before the frame: a synced control reads its division
    /// with one and its free value without.
    pub tempo: Option<f64>,
}

/// Draws one leaf, in the `Ui` the tree bounded to `rect`, through the bindings below — so the
/// controls, their gestures and their names are exactly what they were.
pub fn paint(ui: &mut Ui, tokens: &Tokens, leaf: &Leaf, rect: egui::Rect, live: &mut Live<'_, '_>) {
    let params = live.params;
    let setter = live.setter;
    match *leaf {
        // Synced, a control reads the division in force, not hertz or time; the host still reads
        // its value. With no tempo, the free value stands, and it reads that.
        Leaf::Knob(id, size) => {
            let bound = binding_for(id, params);
            let division = sync_of(params, id).filter(|sync| sync.on).and_then(|sync| {
                sync.ladder
                    .shown(sync.position, live.tempo, sync.lo, sync.hi)
            });
            match division {
                Some(division) => bound.knob_with_reading(
                    ui,
                    tokens,
                    setter,
                    size,
                    rect.width(),
                    live.text_entry,
                    division.label(),
                ),
                None => bound.knob(ui, tokens, setter, size, rect.width(), live.text_entry),
            }
        }
        Leaf::Slider(id) => {
            let bound = binding_for(id, params);
            bound.slider_vertical(
                ui,
                tokens,
                setter,
                live.text_entry,
                bound.painted(),
                rect.width(),
                mxm_ui::control::FADER_HEIGHT,
            );
        }
        Leaf::Switch(id, beside) => {
            let bound = binding_for(id, params);
            let labels = options_of(params, id);
            let options: Vec<&str> = labels.iter().map(String::as_str).collect();
            match beside {
                // On a knob's grid: the one switch that sits there, *Note priority*, has no
                // shorter name to paint.
                Some(beside) => {
                    debug_assert!(
                        bound.panel.is_none(),
                        "{id} paints its own name beside a knob"
                    );
                    segmented(
                        ui,
                        tokens,
                        id,
                        bound.param,
                        &options,
                        Some(beside),
                        bound.details,
                        setter,
                    );
                }
                None => segmented_named(
                    ui,
                    tokens,
                    id,
                    bound.param,
                    bound.panel.as_deref(),
                    &options,
                    bound.details,
                    setter,
                    0.0,
                ),
            }
        }
        Leaf::Waves(id) => {
            let bound = binding_for(id, params);
            segmented_waves_named(
                ui,
                tokens,
                id,
                bound.param,
                bound.panel.as_deref(),
                waves_of(id),
                None,
                bound.details,
                setter,
            );
        }
        Leaf::Picture(id) => {
            super::binding::sync_picture(ui, tokens, id, binding_for(id, params).param, setter);
        }
        Leaf::Toggle(id) => {
            let bound = binding_for(id, params);
            toggle_labelled(
                ui,
                tokens,
                id,
                bound.param,
                bound.painted(),
                bound.description,
                setter,
                0.0,
            );
        }
        Leaf::Routes(target) => route_stack(ui, tokens, target, params, live.text_entry, setter),
    }
}

/// The body of one `Synth` view card, drawn from its tree — the layout lab's entry point, which
/// draws these real cards on its bench.
pub fn draw_synth(
    ui: &mut Ui,
    tokens: &Tokens,
    section: Section,
    params: &MxmMono00Params,
    setter: &ParamSetter<'_>,
    text_entry: &mut HashMap<&'static str, Option<String>>,
) {
    let body = synth_card(ui, section, params);
    // The layout lab has no host: no tempo, so a synced control reads its free value.
    let mut live = Live {
        params,
        setter,
        text_entry,
        tempo: None,
    };
    tree::show(ui, tokens, &body, |ui, leaf, rect| {
        paint(ui, tokens, leaf, rect, &mut live);
    });
}

/// The sample-and-hold card's body, its routes and caption included — the layout lab's too.
pub fn draw_sample_hold(
    ui: &mut Ui,
    tokens: &Tokens,
    params: &MxmMono00Params,
    setter: &ParamSetter<'_>,
    text_entry: &mut HashMap<&'static str, Option<String>>,
) {
    draw_synth(ui, tokens, Section::SampleHold, params, setter, text_entry);
}

/// Lab-only suppression of the routing stacks when the bench supplies its own controls.
/// The shipped panel never enables this: every target must retain its editable stack.
pub fn routing_shown_as_controls() -> egui::Id {
    egui::Id::new("mxm-mono-00-routing-as-controls")
}

/// One target's routes: the live ones stacked, and `‹ modulate ›` beneath.
///
/// **This is what replaced the row selector and its attenuator.** A row named one source and a
/// knob beside it set the depth; a target now carries one row per live source, each with its own
/// signed depth and its own removal. `mxm_modulation_params::ui::stack` owns the whole of the
/// drawing, so this function is the binding and nothing else.
///
/// The text-entry buffer is keyed by the target's own name, which is `&'static str` and unique —
/// `routing::TARGET_NAMES` asserts the uniqueness in its own crate.
pub fn route_stack(
    ui: &mut Ui,
    tokens: &Tokens,
    target: usize,
    params: &MxmMono00Params,
    text_entry: &mut HashMap<&'static str, Option<String>>,
    setter: &ParamSetter<'_>,
) {
    if ui.data(|d| d.get_temp::<bool>(routing_shown_as_controls())) == Some(true) {
        return;
    }
    let (index, group) = params.routes.all()[target];
    debug_assert_eq!(index, target, "the target list is in declared order");
    let routes = group.offered_routes(target);
    let entry = text_entry.entry(TARGET_NAMES[target]).or_default();
    mxm_modulation_params::ui::stack(
        ui,
        tokens,
        TARGET_NAMES[target],
        // **The card already says the module**, so the row drops it: *Pitch from LFO 1* under
        // a card titled *Oscillator 2*, where the parameter is *Oscillator 2 pitch*. The long
        // form cost this instrument a hundred points of card width.
        mxm_mono_00_dsp::routing::TARGET_PANEL_NAMES[target],
        &routes,
        entry,
        setter,
    );
}

/// The Voice card's expander, by the name on its toggle.
pub const ADVANCED: &str = "Advanced";

/// What the expander's toggle says it holds (§7.1's sentence).
const ADVANCED_DESCRIPTION: &str = "Master tune and the bend range.";

/// Where egui remembers whether a disclosure is open: the shared disclosure's own memory
/// (`mxm_ui::shell::disclosure_id`), which the card's tree reads when it is built. **A fixed id, not
/// the card's**, so the developer channel and a test can open it before the view is laid out.
#[must_use]
pub fn disclosure_id(label: &str) -> egui::Id {
    mxm_ui::shell::disclosure_id(label)
}

/// One parameter's binding, with the sentence §7.1 requires in its tooltip.
///
/// **The descriptions live here because only the plugin has them.** CLAP carries no such field.
/// How the keyboard steps a parameter: a semitone and an octave for the coarse
/// tunes and the bend reach, a cent and ten for the fine tunes, and a semitone and an octave in
/// hertz for the two filter corners,
/// the owner's ruling of 2026-09-23. Everything not named keeps its own step. See
/// [`crate::editor::binding::StepLaw`].
fn step_law(id: &str) -> super::binding::StepLaw {
    use super::binding::StepLaw;
    match id {
        "coarse1" | "coarse2" | "bendrange" => StepLaw::Semitones,
        "tune" | "fine1" | "fine2" => StepLaw::Cents,
        "hpf" | "cutoff" => StepLaw::Hertz,
        _ => StepLaw::Own,
    }
}

pub fn binding_for<'a>(id: &'static str, p: &'a MxmMono00Params) -> Bound<'a> {
    let (param, description, bipolar): (&'a dyn ErasedParam, &'static str, bool) = match id {
        // ---- Oscillator 1 ----
        "range1" => (
            &p.range1,
            "The oscillator's octave; 8' is the note as played.",
            false,
        ),
        "coarse1" => (&p.coarse1, "Oscillator 1's pitch, in semitones.", true),
        "fine1" => (&p.fine1, "Fine tune, in cents.", true),
        "wave1" => (&p.wave1, "The oscillator's waveform.", false),
        "pw1" => (
            &p.pw1,
            "How narrow the square wave is: square at one end, a thin pulse at the other.",
            false,
        ),

        // ---- Oscillator 2 ----
        "range2" => (
            &p.range2,
            "The oscillator's octave; 8' is the note as played.",
            false,
        ),
        "coarse2" => (
            &p.coarse2,
            "Oscillator 2's pitch against Oscillator 1, in semitones; with sync or the ring modulator it changes the tone.",
            true,
        ),
        "fine2" => (
            &p.fine2,
            "Fine tune, in cents; it starts slightly sharp so the two oscillators beat.",
            true,
        ),
        "wave2" => (&p.wave2, "The oscillator's waveform.", false),
        "pw2" => (
            &p.pw2,
            "How narrow the square wave is: square at one end, a thin pulse at the other.",
            false,
        ),
        "syncstrength" => (
            &p.sync_strength,
            "How firmly Oscillator 2 locks to Oscillator 1.",
            false,
        ),

        // ---- Mixer ----
        "vco1level" => (&p.vco1_level, "Oscillator 1 into the mixer.", false),
        "vco2level" => (&p.vco2_level, "Oscillator 2 into the mixer.", false),
        "noiselevel" => (&p.noise_level, "Noise into the mixer.", false),
        "ringmodlevel" => (
            &p.ring_level,
            "The ring modulator's level: a metallic, bell-like tone.",
            false,
        ),
        "noisecolour" => (
            &p.noise_colour,
            "The noise's colour: bright white or darker pink.",
            false,
        ),

        // ---- Filter ----
        "hpf" => (&p.hpf, "Removes low end before the main filter.", false),
        "cutoff" => (&p.cutoff, "The filter's cutoff: lower is darker.", false),
        "resonance" => (
            &p.resonance,
            "Emphasis at the cutoff; turned up, it thins the bass a little.",
            false,
        ),
        // ---- Amplifier ----
        "initialgain" => (
            &p.initial_gain,
            "Volume with no key held; above zero the sound plays on its own.",
            false,
        ),
        "tone" => (&p.tone, "Tilts the sound darker or brighter.", true),

        // ---- Envelopes ----
        "vcfattack" => (&p.vcf_attack, "How long Envelope 1 takes to rise.", false),
        "vcfdecay" => (
            &p.vcf_decay,
            "How long Envelope 1 takes to fall to its sustain level.",
            false,
        ),
        "vcfsustain" => (
            &p.vcf_sustain,
            "The level Envelope 1 holds while a key is down.",
            false,
        ),
        "vcfrelease" => (
            &p.vcf_release,
            "How long Envelope 1 takes to fade after the key is released.",
            false,
        ),
        "vcftrigger" => (&p.vcf_trigger, "What restarts the envelope.", false),
        "vcaattack" => (&p.vca_attack, "How long Envelope 2 takes to rise.", false),
        "vcadecay" => (
            &p.vca_decay,
            "How long Envelope 2 takes to fall to its sustain level.",
            false,
        ),
        "vcasustain" => (
            &p.vca_sustain,
            "The level Envelope 2 holds while a key is down.",
            false,
        ),
        "vcarelease" => (
            &p.vca_release,
            "How long Envelope 2 takes to fade after the key is released.",
            false,
        ),
        "vcatrigger" => (&p.vca_trigger, "What restarts the envelope.", false),

        // ---- Modulators ----
        "lfo1rate" => (&p.lfo1_rate, "LFO 1's speed.", false),
        "lfo1shape" => (&p.lfo1_shape, "LFO 1's shape.", false),
        "lfo1offset" => (&p.lfo1_offset, "Fine-tunes the LFO's speed.", true),
        "lfo1sync" => (&p.lfo1_sync, super::binding::SYNC_DESCRIPTION, false),
        "lfo2rate" => (&p.lfo2_rate, "LFO 2's speed.", false),
        "lfo2shape" => (&p.lfo2_shape, "LFO 2's shape.", false),
        "lfo2offset" => (&p.lfo2_offset, "Fine-tunes the LFO's speed.", true),
        "lfo2sync" => (&p.lfo2_sync, super::binding::SYNC_DESCRIPTION, false),

        // ---- Voice ----
        "priority" => (&p.priority, "Which held key sounds.", false),
        "portamento" => (
            &p.portamento,
            "How slowly the pitch slides to a new key. Even at its shortest it never quite snaps.",
            false,
        ),
        "tune" => (&p.tune, "Master tuning, in cents.", true),
        "bendrange" => (&p.bend_range, "Pitch-bend range, in semitones.", false),

        // ---- Sample & Hold ----
        "shrate" => (
            &p.sh_rate,
            "How often the sample and hold picks a new value.",
            false,
        ),
        "shlag" => (
            &p.sh_lag,
            "How slowly the held value moves to the next.",
            false,
        ),
        "shsync" => (&p.sh_sync, super::binding::SYNC_DESCRIPTION, false),
        // ---- Routing parameters (unchanged DSP row order) ----

        // ---- FX ----
        "phaser" => (&p.phaser, "How much phaser, and how fast it sweeps.", false),
        "delay" => (&p.delay, "The delay's level. Its feedback is fixed.", false),
        "delaytime" => (
            &p.delay_time,
            "The delay's time; with Tempo sync on, a note length at the host's tempo.",
            false,
        ),
        "temposync" => (&p.tempo_sync, super::binding::SYNC_DESCRIPTION, false),
        "reverb" => (&p.reverb, "The spring reverb's level.", false),

        // ---- Output, in the app bar ----
        "volume" => (&p.volume, "Output level, after the effects.", false),

        other => unreachable!("no binding for {other}"),
    };
    Bound {
        id,
        param,
        description,
        bipolar,
        law: step_law(id),
        panel: panel_label(id).map(std::borrow::Cow::Borrowed),
        stepped: None,
        details: details_of(id),
    }
}

/// What each option of a stepped control does, one sentence per cell in the parameter's own order
/// (design system §7.3; the owner, 2026-09-27: the cells of a row do not share one sentence).
/// Empty for everything drawn as a knob, slider or toggle.
fn details_of(id: &str) -> &'static [&'static str] {
    const SHAPES: &[&str] = &[
        "A smooth wobble.",
        "Rises and falls in straight lines.",
        "Ramps one way, then jumps back.",
        "Jumps between two values, like a trill.",
        "A new random value on every cycle.",
    ];
    const TRIGGERS: &[&str] = &[
        "Starts with the gate and holds; a new key over a held one does not restart it.",
        "While the gate is held, LFO 1 restarts it on every cycle.",
        "Every new key restarts it, even over held keys.",
    ];
    match id {
        "range1" | "range2" => &[
            "Three octaves below the note played.",
            "Two octaves below the note played.",
            "One octave below the note played.",
            "The note as played.",
            "One octave above the note played.",
            "Two octaves above the note played.",
        ],
        "wave1" | "wave2" => &[
            "Bright and buzzy: every harmonic.",
            "Hollow: odd harmonics only; the width knob thins it to a pulse.",
            "Soft and round: a few quiet odd harmonics.",
        ],
        "syncstrength" => &[
            "Oscillator 2 locks to Oscillator 1 at any tuning: the hard-sync sound.",
            "Locks only near simple intervals and drifts free between them.",
        ],
        "noisecolour" => &["Bright, hissing noise.", "Darker, softer noise."],
        "vcftrigger" | "vcatrigger" => TRIGGERS,
        "lfo1shape" | "lfo2shape" => SHAPES,
        "priority" => &[
            "The lowest held key sounds.",
            "The key pressed last sounds.",
        ],
        _ => &[],
    }
}

/// What a control **paints**, where its card already says the rest (design system §7.1): *Range*
/// and *Wave* on *Oscillator 1*, *Attack* on *Envelope 1*, and *Oscillator 1* on the mixer's
/// slider. `None` paints the parameter's own name. The canonical name — *Range 1*, *Envelope 1
/// attack* — is always what a host, a tooltip and a screen reader read. The prefix stays where the
/// card is not the module's: *Noise colour* on the Mixer, *Delay time* on Effects.
fn panel_label(id: &str) -> Option<&'static str> {
    match id {
        "range1" | "range2" => Some("Range"),
        "coarse1" | "coarse2" => Some("Coarse"),
        "fine1" | "fine2" => Some("Fine"),
        "wave1" | "wave2" => Some("Wave"),
        "pw1" | "pw2" => Some("Pulse width"),
        "vco1level" => Some("Oscillator 1"),
        "vco2level" => Some("Oscillator 2"),
        "noiselevel" => Some("Noise"),
        "ringmodlevel" => Some("Ring mod"),
        // An envelope's faders, by the convention (the owner, 2026-09-25).
        "vcfattack" | "vcaattack" => Some("A"),
        "vcfdecay" | "vcadecay" => Some("D"),
        "vcfsustain" | "vcasustain" => Some("S"),
        "vcfrelease" | "vcarelease" => Some("R"),
        "vcftrigger" | "vcatrigger" => Some("Trigger"),
        "lfo1rate" | "lfo2rate" => Some("Rate"),
        "lfo1shape" | "lfo2shape" => Some("Shape"),
        "lfo1offset" | "lfo2offset" => Some("Offset"),
        "shrate" => Some("Rate"),
        "shlag" => Some("Lag"),
        _ => None,
    }
}

/// Every parameter, bound, in the instrument's own order.
///
/// What `preset.rs` iterates: capture, Init, resolve and the dirty baseline all walk this list.
pub fn all_parameters(params: &MxmMono00Params) -> Vec<Bound<'_>> {
    all_ids().map(|id| binding_for(id, params)).collect()
}

/// Every id, in section order. One list, from [`SECTIONS`], so the lookup, the views and the
/// presets cannot disagree.
pub fn all_ids() -> impl Iterator<Item = &'static str> {
    SECTIONS
        .iter()
        .flat_map(|section| section.parameters().iter().copied())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn source_names_match_cards_and_host_values_without_changing_saved_ids() {
        assert_eq!(Section::Modulator1.title(), column_label(Column::Lfo1));
        assert_eq!(Section::Modulator2.title(), column_label(Column::Lfo2));
        assert_eq!(Section::EnvelopeVcf.title(), column_label(Column::VcfAdsr));
        assert_eq!(Section::EnvelopeVca.title(), column_label(Column::VcaAdsr));
        assert_eq!(Section::Oscillator1.title(), column_label(Column::Vco1));
        assert_eq!(Section::Oscillator2.title(), column_label(Column::Vco2));
        // The source vocabulary is the DSP's list now, and it is what every route row is named
        // from — so a card's title and the row that names it as a source cannot drift.
        assert_eq!(
            mxm_mono_00_dsp::routing::SOURCE_NAMES[..14],
            [
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
            ]
        );
        let params = MxmMono00Params::default();
        for bound in all_parameters(&params) {
            for obsolete in ["LFO-", "Filter env", "Filter LFO", "Amp env", "VCF", "VCA"] {
                assert!(
                    !bound.param.name().contains(obsolete),
                    "{}: {}",
                    bound.id,
                    bound.param.name()
                );
            }
        }
        // Every input is named for what it moves: the filter has one, and it is the cutoff.
        assert_eq!(TARGET_NAMES[target::CUTOFF], "Cutoff");
        for name in mxm_mono_00_dsp::routing::TARGET_PANEL_NAMES {
            assert!(
                !name.contains("Modulator"),
                "{name} says what is patched, not what it moves"
            );
        }
    }

    /// Every parameter appears exactly once across the sections.
    ///
    /// A parameter with no control is a parameter nobody can reach, and one drawn twice is two
    /// controls disagreeing about a value.
    /// Every parameter is drawn exactly once — **counting a routing pair's two as drawn by the
    /// stack that owns its target**.
    ///
    /// A route is revealed by its own presence, so at Init thirteen of 425 pairs are painted and the
    /// other 412 are deliberately absent. What can still be checked without a harness is that each
    /// pair belongs to exactly one target, that its target is on exactly one card, and that no id
    /// is both a musician control and a route.
    #[test]
    fn every_parameter_is_drawn_exactly_once() {
        use nice_plug::prelude::Params;
        let params = MxmMono00Params::default();
        let declared: Vec<String> = params
            .param_map()
            .into_iter()
            .map(|(id, _, _)| id)
            .collect();
        let drawn: Vec<&str> = all_ids().collect();
        let routed: Vec<&str> = crate::routes::offered_route_ids()
            .flat_map(|(amount, present)| [amount, present])
            .collect();

        for id in &declared {
            let control = drawn.iter().filter(|d| *d == id).count();
            let route = routed.iter().filter(|r| *r == id).count();
            assert_eq!(
                control + route,
                1,
                "{id} is drawn {control} times as a control and {route} times as a route"
            );
        }
        assert_eq!(
            drawn.len() + routed.len(),
            declared.len(),
            "drawn {} controls plus {} route ids against {} declared",
            drawn.len(),
            routed.len(),
            declared.len()
        );
        // And every target's stack is on exactly one card, so no pair is orphaned.
        for (t, name) in TARGET_NAMES.iter().enumerate() {
            let cards = SECTIONS.iter().filter(|s| s.targets().contains(&t)).count();
            assert_eq!(cards, 1, "{name} is on {cards} cards");
        }
    }

    /// Every binding resolves, which `binding_for`'s `unreachable!` would otherwise turn into a
    /// panic inside a paint call.
    #[test]
    fn every_drawn_parameter_has_a_binding_with_a_sentence() {
        let params = MxmMono00Params::default();
        for id in all_ids() {
            let bound = binding_for(id, &params);
            assert!(!bound.description.is_empty(), "{id} has no description");
            assert!(
                bound.description.ends_with('.'),
                "{id}'s description is not a sentence: {:?}",
                bound.description
            );
        }
    }

    /// The section order is the brief's, asserted rather than assumed.
    #[test]
    fn the_sections_are_in_the_briefs_order() {
        let titles: Vec<&str> = SECTIONS.iter().map(|s| s.title()).collect();
        assert_eq!(
            titles,
            [
                "Voice",
                "LFO 1",
                "LFO 2",
                "Sample & Hold",
                "Oscillator 1",
                "Oscillator 2",
                "Ring mod",
                "Mixer",
                "Filter",
                "Envelope 1",
                "Amplifier",
                "Envelope 2",
                "Routing",
                "FX",
                "Output",
            ]
        );
    }

    /// Every target's routes are nested in the DSP's own target order, so the two never disagree
    /// about which group is which input.
    #[test]
    fn the_routing_groups_are_the_dsp_targets_in_declared_order() {
        let params = MxmMono00Params::default();
        for (index, (target, _)) in params.routes.all().iter().enumerate() {
            assert_eq!(*target, index, "{}", TARGET_NAMES[index]);
        }
        assert_eq!(TARGET_NAMES.len(), params.routes.all().len());
    }

    /// S&H is a Synth card; routing metadata, FX and the app bar's Output are not. Every card has
    /// one reflow group.
    #[test]
    fn all_twelve_synth_cards_are_in_signal_order_and_in_one_group() {
        let on_synth = &SECTIONS[..12];
        assert_eq!(SYNTH, on_synth);
        assert_eq!(
            &SYNTH[1..4],
            &[
                Section::Modulator1,
                Section::Modulator2,
                Section::SampleHold
            ]
        );
        // The `Synth` view is a flow now, so the check is about its groups rather than columns:
        // every card in exactly one group, and no group naming a card that is not there.
        assert_eq!(SYNTH.len(), on_synth.len());
        let mut seen = vec![0usize; SYNTH.len()];
        for group in SYNTH_GROUPS {
            for card in *group {
                assert!(
                    *card < SYNTH.len(),
                    "group names card {card}, which does not exist"
                );
                seen[*card] += 1;
            }
        }
        for (card, count) in seen.iter().enumerate() {
            assert_eq!(
                *count,
                1,
                "{} is in {count} groups; it must be in exactly one",
                SYNTH[card].title()
            );
        }
        for off in [Section::Routing, Section::Fx, Section::Output] {
            assert!(!SYNTH.contains(&off), "{} is not a Synth card", off.title());
        }
    }

    /// Every routing target is edited on exactly one card across Synth and FX.
    #[test]
    fn every_routing_target_is_shown_on_exactly_one_card() {
        for (t, name) in TARGET_NAMES.iter().enumerate() {
            let count = SECTIONS.iter().filter(|s| s.targets().contains(&t)).count();
            assert_eq!(count, 1, "{name} is on {count} cards");
        }
    }

    /// **There is no `Default` to spell out any more**, and that is the point: the plug-out's
    /// internal connections are present routes in the init patch, so what a row used to say in
    /// words the panel now says by drawing the route.
    #[test]
    fn the_plug_outs_normals_are_drawn_as_routes_rather_than_named_as_a_default() {
        let params = MxmMono00Params::default();
        let present = |target: usize, source: usize| {
            params.routes.all()[target].1.routes(target)[source].is_present()
        };
        use mxm_mono_00_dsp::routing::source;
        assert!(present(target::CUTOFF, source::LFO1), "VCF LFO IN");
        assert!(present(target::CUTOFF, source::VCF_ADSR), "VCF ADSR IN");
        assert!(present(target::VCA_GATE, source::GATE), "VCA gate");
        // DESTINATION's default, both oscillators, is the glide and LFO 1 on both pitch inputs.
        for pitch in [target::VCO1_PITCH, target::VCO2_PITCH] {
            assert!(present(pitch, source::GLIDE), "glide to both oscillators");
            assert!(present(pitch, source::LFO1), "vibrato to both oscillators");
        }
        assert!(present(target::TREMOLO, source::LFO1), "VCA LFO");
        assert!(
            present(target::CUTOFF, source::KEY),
            "KYBD CV to the filter"
        );
        // LFO 1's rate input had no internal connection, so nothing is wired to it.
        for sc in 0..mxm_mono_00_dsp::routing::SOURCES {
            assert!(
                !present(target::LFO1_RATE, sc),
                "LFO 1 rate is wired to {sc}"
            );
        }
    }

    /// The picture tables agree with their parameters' step counts, so the shared control's debug
    /// assertion cannot fire in a paint call. The switches' cells are the parameters' own text.
    #[test]
    fn every_picture_table_matches_its_parameter() {
        let params = MxmMono00Params::default();
        let cases: &[(&str, usize)] = &[
            ("wave1", WAVES.len()),
            ("wave2", WAVES.len()),
            ("lfo1shape", LFO_SHAPES.len()),
            ("lfo2shape", LFO_SHAPES.len()),
        ];
        for (id, options) in cases {
            let bound = binding_for(id, &params);
            assert_eq!(
                bound.param.steps().map(|s| s + 1),
                Some(*options),
                "{id}'s pictures are {options}"
            );
        }
        // B4: a cell reads the host's own text for its option.
        assert_eq!(
            options_of(&params, "range1"),
            ["64'", "32'", "16'", "8'", "4'", "2'"]
        );
    }
}
