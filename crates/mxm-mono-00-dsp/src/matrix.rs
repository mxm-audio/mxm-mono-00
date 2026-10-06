//! The patch bay's outputs, its scale, and the detectors that are not plain CV.
//!
//! `research:instruments/system-100.md` §12.2 and `plans/plan-mxm-mono-00.md` §5.1. The fourteen
//! outputs are still [`Column`]; what has gone is the **selector**. An input no longer takes one
//! connection: it takes any number, each with its own signed amount, which is
//! `plans/plan-modulation-routing.md` decision 1.6 turning the switching jack into a mixer.
//! [`crate::routing`] is where the inputs, the amounts and the laws live now.
//!
//! **The plug-out's internal connections are not here either.** They were a `default_source` table
//! this module owned; they are [`crate::routing::INIT_PRESENT`] and
//! [`crate::routing::INIT_AT_FULL`] — the init patch — so every one of them is a route a player can
//! pull out, and nothing in the voice tests for *is this row patched*.
//!
//! # The evaluation order, and the unit delay
//!
//! Modules are evaluated in a fixed order once per sample, and **an input reading a source that has
//! not yet been published this sample reads the previous sample's value** — the plan's §5.2, and
//! `mxm_modulation::SourceFrame`'s contract. That is what makes every cycle the patch bay can close
//! evaluable, and it is a chosen digital deviation: wire closes a loop instantly, this closes it one
//! sample late. The order is durable and is recorded in the crate's `AGENTS.md`:
//!
//! | Stage | Module | Publishes |
//! |---|---|---|
//! | 1 | keyboard, portamento, the performance inputs | `KybdCv`, `Gate`, `Wheel`, `Pressure`, `Bend` |
//! | 2 | LFO-1, LFO-2 | `Lfo1`, `Lfo2` |
//! | 3 | sample-and-hold | `ShOut`, `ShClock` |
//! | 4 | the envelopes, then the press that last triggered one | `VcfAdsr`, `VcaAdsr`, `Velocity` |
//! | 5 | glide, then VCO-1 | `Glide`, `Vco1`, `Vco1Sync` |
//! | 6 | VCO-2 | `Vco2`, `Vco2Sync` |
//! | 7 | the ring modulator, the noise | `RingMod`, `Noise` |
//! | 8 | the mixer | `MixerOut` |
//! | 9 | HPF, VCF, VCA, effects, Volume | — |
//!
//! So VCO-1's modulator input reading VCO-2 reads the previous sample (VCO-2 is stage 6, VCO-1
//! stage 5), VCO-2's sync input reads VCO-1's edge from this sample, and the mixer into its own
//! external input is a one-sample loop.
//!
//! # The column scale
//!
//! Every column carries **ten-volt units**: 1.0 is +10 V. The hardware's levels (§4.1) then map
//! directly — an LFO sawtooth 0 → +10 V is 0 … 1, a VCO output of 10 Vp-p is ±0.5, an envelope's
//! +6 V peak is 0.6, the keyboard CV at 1 V/oct is `octaves_from_middle_C / 10`, a gate is 0 or 1
//! (the hardware's +14 V clipped to the unit; only its edges matter). Each input scales this one
//! unit into its own domain — `crate::routing::TARGET_SCALE` — which is why a full-amount route
//! means the same reach whichever source is on it.
//!
//! **It is also the frame unit.** Every source is already inside unit magnitude, so nothing clamps
//! on publication and this instrument needs no scaling into or out of the routing frame at all.
//!
//! # Boundedness by construction
//!
//! Every column is bounded by its producer regardless of what it reads: the LFOs by their shapes,
//! the S&H by what it sampled, the VCOs by their waveforms, the ring modulator by their product,
//! the noise by its filters' unity gain, the mixer by its saturator, the envelopes by their range,
//! the keyboard CV by the note range. Every input clamps what it sets (a rate, a pitch, a cutoff)
//! at its consumer. So every cycle passes through a bounded module, and no cycle can grow.

/// The fourteen output columns, in the plug-out's order (`system-100.md` §12.2).
///
/// **Their indices are routing source indices** — `crate::routing::source::LFO1` is
/// `Column::Lfo1.index()` — so the two lists need no translation between them.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Column {
    KybdCv,
    Lfo1,
    Lfo2,
    ShOut,
    ShClock,
    Vco1,
    Vco2,
    Vco1Sync,
    Vco2Sync,
    RingMod,
    Noise,
    MixerOut,
    VcfAdsr,
    VcaAdsr,
}

pub const COLUMNS: usize = 14;

impl Column {
    pub const ALL: [Column; COLUMNS] = [
        Column::KybdCv,
        Column::Lfo1,
        Column::Lfo2,
        Column::ShOut,
        Column::ShClock,
        Column::Vco1,
        Column::Vco2,
        Column::Vco1Sync,
        Column::Vco2Sync,
        Column::RingMod,
        Column::Noise,
        Column::MixerOut,
        Column::VcfAdsr,
        Column::VcaAdsr,
    ];

    #[inline]
    pub const fn index(self) -> usize {
        self as usize
    }

    /// Whether the column can move with no host event: everything but the keyboard CV and the
    /// envelopes, whose periodicity depends on what gates them.
    pub const fn is_self_running(self) -> bool {
        !matches!(self, Column::KybdCv | Column::VcfAdsr | Column::VcaAdsr)
    }
}

/// How long an audio route takes to fade its own contribution in or out.
///
/// **Chosen**: the same order as the collection's waveform-switch ramps; the transition test bounds
/// the broadband energy a change puts out against an absolute figure. Before the conversion this
/// crossfaded between the two sources a row could name; it now ramps each route's own contribution,
/// which is the same audio when one replaces another — see `crate::routing::Graph::sum_audio`.
pub const AUDIO_ROUTE_RAMP_S: f32 = 0.005;

/// A gate input's edge detector: high above the threshold, and the edge is a rising crossing.
///
/// The hardware's gates are +14 V and its envelopes +6 V; an LFO square is 0 / +10 V, a VCO ±5 V.
/// A quarter of the unit (2.5 V) is high for every one of those and low for none of their lows:
/// **chosen**, the hardware's gate detector threshold (Q208, §6.1) not being documented.
///
/// **One detector per input, not per route.** The gate law sums first and detects once, so two
/// half-amount sources can together cross where neither reaches alone — which is what makes a gate
/// route's amount a level rather than an enable, and what keeps coincidence from arising at all.
pub const GATE_THRESHOLD: f32 = 0.25;

#[derive(Debug, Clone, Copy, Default)]
pub struct GateRow {
    high: bool,
}

impl GateRow {
    pub const fn new() -> Self {
        Self { high: false }
    }

    pub fn reset(&mut self) {
        self.high = false;
    }

    pub fn is_high(&self) -> bool {
        self.high
    }

    /// Feed the input's summed signal; returns `(rose, fell)`.
    #[inline]
    pub fn process(&mut self, value: f32) -> (bool, bool) {
        let now = value > GATE_THRESHOLD;
        let rose = now && !self.high;
        let fell = !now && self.high;
        self.high = now;
        (rose, fell)
    }
}

/// A sync route's edge detector: a rising crossing of zero with the fraction interpolated, so any
/// source can sync VCO-2.
///
/// **One per route, which is why the sync law cannot sum.** The interpolated fraction
/// `-prev / (value - prev)` is scale-invariant — multiply both samples by any positive number and
/// the crossing and its fraction are identical — so a summed and scaled sync input would be an
/// on/off switch wearing a knob. Each present route watches its own source and a crossing resets by
/// that route's own depth instead; coincident crossings reset by the largest.
#[derive(Debug, Clone, Copy, Default)]
pub struct EdgeRow {
    prev: f32,
}

impl EdgeRow {
    pub const fn new() -> Self {
        Self { prev: 0.0 }
    }

    pub fn reset(&mut self) {
        self.prev = 0.0;
    }

    /// The last value fed to this detector, which is the state a stale one would be carrying.
    pub fn last(&self) -> f32 {
        self.prev
    }

    /// The fraction into the last interval at which the signal rose through zero, if it did.
    #[inline]
    pub fn process(&mut self, value: f32) -> Option<f32> {
        let edge = if self.prev <= 0.0 && value > 0.0 {
            let frac = (-self.prev / (value - self.prev)).clamp(0.0, 1.0);
            Some(frac)
        } else {
            None
        };
        self.prev = value;
        edge
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_gate_row_reports_rising_and_falling_edges_at_the_threshold() {
        let mut g = GateRow::new();
        assert_eq!(g.process(0.0), (false, false));
        assert_eq!(g.process(1.0), (true, false));
        assert_eq!(
            g.process(0.6),
            (false, false),
            "an envelope's peak keeps it high"
        );
        assert_eq!(g.process(0.1), (false, true));
        assert_eq!(
            g.process(-0.5),
            (false, false),
            "a VCO's negative half is low"
        );
        assert_eq!(
            g.process(0.5),
            (true, false),
            "and its positive half is high"
        );
    }

    #[test]
    fn two_half_amount_sources_cross_a_gate_threshold_neither_reaches_alone() {
        // The gate law: sum, then detect once. This is what makes a gate route's amount continuous.
        let mut g = GateRow::new();
        assert_eq!(g.process(0.2), (false, false));
        assert_eq!(g.process(0.2 + 0.2), (true, false));
    }

    #[test]
    fn an_edge_row_interpolates_the_crossing() {
        let mut e = EdgeRow::new();
        assert_eq!(e.process(-0.5), None);
        let f = e.process(0.5).expect("a rising crossing");
        assert!((f - 0.5).abs() < 1e-6, "halfway between -0.5 and 0.5: {f}");
        assert_eq!(e.process(0.7), None);
    }

    #[test]
    fn an_edge_rows_fraction_does_not_move_when_its_signal_is_scaled() {
        // Why a sync route's amount is reset depth rather than a scale: scaling changes nothing
        // the detector can see.
        let mut a = EdgeRow::new();
        let mut b = EdgeRow::new();
        a.process(-0.5);
        b.process(-0.5 * 0.1);
        let fa = a.process(0.5).expect("a crossing");
        let fb = b.process(0.5 * 0.1).expect("the same crossing");
        assert!((fa - fb).abs() < 1e-6, "{fa} vs {fb}");
    }
}
