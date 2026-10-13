//! The **only** channel from the audio thread to the editor.
//!
//! Atomics, written once per block, read whenever the editor happens to look. No locks, no
//! allocation, and the UI may drop as many frames as it likes — a display that made the audio
//! thread wait would be a display that could cause a dropout.
//!
//! **The meter and the developer channel's requests are `mxm_telemetry`'s**, held as `meter` and
//! `dev`, with one policy for every plugin: a peak is max-combined and reset when the UI reads it,
//! a clip latches until acknowledged (design system §5.4), and a non-finite peak latches the clip
//! and is never the level. What follows here is this plugin's own.
//!
//! # The patch's activity is this instrument's own
//!
//! The brief's §8: this is the first instrument in the collection that sounds with no key down,
//! and a person must be able to see that it is the patch doing it and not the host. The DSP
//! decides *live*, *tailing* or *inert* from the configuration (`Voice::activity`), and the value
//! published here is exactly that decision, once per block.

use std::sync::Arc;
use std::sync::atomic::{AtomicU8, AtomicU32, Ordering};

use mxm_mono_00_dsp::matrix::{COLUMNS, Column};
use mxm_mono_00_dsp::voice::Activity;

#[derive(Debug)]
pub struct Telemetry {
    /// The output level and its clip latch: the collection's one meter policy
    /// (`mxm_telemetry::PeakMeter`).
    pub meter: mxm_telemetry::PeakMeter,
    /// The developer channel's requests of the editor, each taken once
    /// (`mxm_telemetry::DevRequests`).
    pub dev: mxm_telemetry::DevRequests,
    /// The patch's activity, as [`Activity`] encoded by [`encode`].
    activity: AtomicU8,
    /// Published once in `activate`; it changes only when the host reconfigures.
    sample_rate: AtomicU32,
    /// The matrix's fourteen columns, once per block, in `Column::ALL` order: the brief's §8
    /// *matrix lit by signal*. **Exact for gates and CVs** — the last sample's value, since a
    /// gate or a slow CV is the same across a block — and a **block peak for the audio columns**,
    /// whose last sample would be a random point on a waveform. The editor normalises each into
    /// its column's own scale; here they are the matrix's units, as the DSP writes them.
    columns: [AtomicU32; COLUMNS],
    /// The host tempo in force, for the synced controls' readings: a synced control reads its
    /// division with one and its free value without (`plans/plan-tempo-sync-controls.md`).
    pub tempo: mxm_tempo::TempoCell,
}

const fn encode(activity: Activity) -> u8 {
    match activity {
        Activity::Inert => 0,
        Activity::Tailing => 1,
        Activity::Live => 2,
    }
}

const fn decode(code: u8) -> Activity {
    match code {
        2 => Activity::Live,
        1 => Activity::Tailing,
        _ => Activity::Inert,
    }
}

impl Default for Telemetry {
    fn default() -> Self {
        Self::new()
    }
}

impl Telemetry {
    pub fn new() -> Self {
        Self {
            meter: mxm_telemetry::PeakMeter::new(),
            dev: mxm_telemetry::DevRequests::new(),
            activity: AtomicU8::new(encode(Activity::Inert)),
            sample_rate: AtomicU32::new(48_000f32.to_bits()),
            columns: std::array::from_fn(|_| AtomicU32::new(0)),
            tempo: mxm_tempo::TempoCell::new(),
        }
    }

    pub fn shared() -> Arc<Self> {
        Arc::new(Self::new())
    }

    // ---- audio thread ----

    pub fn publish_activity(&self, activity: Activity) {
        self.activity.store(encode(activity), Ordering::Relaxed);
    }

    pub fn publish_sample_rate(&self, rate: f32) {
        self.sample_rate.store(rate.to_bits(), Ordering::Relaxed);
    }

    /// One column's level for the block just rendered. Overwritten, not combined: a lit matrix
    /// shows *now*, and a gate that was high a block ago is not high.
    pub fn publish_column(&self, column: Column, level: f32) {
        self.columns[column.index()].store(level.to_bits(), Ordering::Relaxed);
    }

    // ---- editor thread ----

    pub fn activity(&self) -> Activity {
        decode(self.activity.load(Ordering::Relaxed))
    }

    pub fn sample_rate(&self) -> f32 {
        f32::from_bits(self.sample_rate.load(Ordering::Relaxed))
    }

    /// A column's level as last published, in the matrix's own units.
    pub fn column(&self, column: Column) -> f32 {
        f32::from_bits(self.columns[column.index()].load(Ordering::Relaxed))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_activity_survives_a_round_trip() {
        let t = Telemetry::new();
        assert_eq!(t.activity(), Activity::Inert, "a fresh instance is inert");
        for state in [Activity::Live, Activity::Tailing, Activity::Inert] {
            t.publish_activity(state);
            assert_eq!(t.activity(), state);
        }
    }

    #[test]
    fn a_column_level_is_the_last_published_not_the_loudest() {
        let t = Telemetry::new();
        t.publish_column(Column::Lfo1, 0.9);
        t.publish_column(Column::Lfo1, 0.2);
        assert_eq!(
            t.column(Column::Lfo1),
            0.2,
            "a lit matrix shows now, not the loudest since"
        );
        assert_eq!(t.column(Column::Lfo2), 0.0, "and the others are untouched");
    }

    #[test]
    fn the_tempo_survives_a_round_trip() {
        let t = Telemetry::new();
        assert_eq!(t.tempo.get(), None);
        t.tempo.publish(Some(128.0));
        assert_eq!(t.tempo.get(), Some(128.0));
    }

    #[test]
    fn the_sample_rate_survives_a_round_trip() {
        let t = Telemetry::new();
        t.publish_sample_rate(96_000.0);
        assert_eq!(t.sample_rate(), 96_000.0);
    }
}
