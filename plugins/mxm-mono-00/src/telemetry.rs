//! The **only** channel from the audio thread to the editor.
//!
//! Atomics, written once per block, read whenever the editor happens to look. No locks, no
//! allocation, and the UI may drop as many frames as it likes — a display that made the audio
//! thread wait would be a display that could cause a dropout.
//!
//! Two rules carried from `plugins/mxm-mono-01/src/telemetry.rs`, both of which exist because the
//! obvious implementation loses information:
//!
//! - **A peak is max-combined and reset when the UI reads it.** Overwriting each block means a
//!   transient that landed between two frames is simply gone; combining means the value is always
//!   *loudest since you last looked*.
//! - **A clip latches until acknowledged.** A meter that quietly forgets it clipped is worse than
//!   no meter, and design system §5.4 requires the indication to persist.
//!
//! # The patch's activity is this instrument's own
//!
//! The brief's §8: this is the first instrument in the collection that sounds with no key down,
//! and a person must be able to see that it is the patch doing it and not the host. The DSP
//! decides *live*, *tailing* or *inert* from the configuration (`Voice::activity`), and the value
//! published here is exactly that decision, once per block.

use std::sync::Arc;
use std::sync::atomic::{AtomicBool, AtomicU8, AtomicU32, Ordering};

use mxm_mono_00_dsp::matrix::{COLUMNS, Column};
use mxm_mono_00_dsp::voice::Activity;

#[derive(Debug)]
pub struct Telemetry {
    /// Peak of the samples produced, max-combined, reset on read.
    peak: AtomicU32,
    /// Sticky: set when a sample reaches full scale, cleared only by the user.
    clipped: AtomicBool,
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
    /// The developer channel's requests of the editor (`lib.rs`, *A developer channel*): a view to
    /// show, and whether the Voice card's expander is open. [`NO_REQUEST`] when nothing is asked;
    /// the editor takes each once. The one place a MIDI event reaches the editor, and only when
    /// the plugin was started with the channel enabled.
    dev_view: AtomicU8,
    dev_disclosure: AtomicU8,
    /// The developer channel's request to open or close the preset browser, or `NO_REQUEST`.
    dev_browser: AtomicU8,
    /// The developer channel's request to show a theme, by index, or `NO_REQUEST`. Theme is
    /// interface state, so this reaches the editor and nothing else; the DSP never sees it.
    dev_theme: AtomicU8,
}

/// Nothing requested on a developer-channel slot.
const NO_REQUEST: u8 = u8::MAX;

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
            peak: AtomicU32::new(0),
            clipped: AtomicBool::new(false),
            activity: AtomicU8::new(encode(Activity::Inert)),
            sample_rate: AtomicU32::new(48_000f32.to_bits()),
            columns: std::array::from_fn(|_| AtomicU32::new(0)),
            tempo: mxm_tempo::TempoCell::new(),
            dev_view: AtomicU8::new(NO_REQUEST),
            dev_disclosure: AtomicU8::new(NO_REQUEST),
            dev_browser: AtomicU8::new(NO_REQUEST),
            dev_theme: AtomicU8::new(NO_REQUEST),
        }
    }

    pub fn shared() -> Arc<Self> {
        Arc::new(Self::new())
    }

    // ---- audio thread ----

    /// Publish a block's peak. **Combined, not overwritten**: see the module doc.
    pub fn publish_peak(&self, peak: f32) {
        let mut current = self.peak.load(Ordering::Relaxed);
        loop {
            let combined = f32::from_bits(current).max(peak);
            match self.peak.compare_exchange_weak(
                current,
                combined.to_bits(),
                Ordering::Relaxed,
                Ordering::Relaxed,
            ) {
                Ok(_) => break,
                Err(seen) => current = seen,
            }
        }
        if peak >= 1.0 {
            self.clipped.store(true, Ordering::Relaxed);
        }
    }

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

    /// Developer category address (0–5), or Parameters (127); never a derived tab index.
    pub fn request_view(&self, view: u8) {
        self.dev_view
            .store(view.min(NO_REQUEST - 1), Ordering::Relaxed);
    }

    /// The developer channel asks the editor to open or close the preset browser.
    pub fn request_browser(&self, open: bool) {
        self.dev_browser.store(u8::from(open), Ordering::Relaxed);
    }

    /// Whether the developer channel asked the browser open or closed since the editor last
    /// looked, if it did.
    pub fn take_browser_request(&self) -> Option<bool> {
        match self.dev_browser.swap(NO_REQUEST, Ordering::Relaxed) {
            NO_REQUEST => None,
            open => Some(open != 0),
        }
    }

    /// The developer channel asks the editor for a theme, by index — 0 light, 1 dark, 2 system,
    /// as `mxm_ui::theme::from_index` reads it.
    pub fn request_theme(&self, theme: u8) {
        self.dev_theme
            .store(theme.min(NO_REQUEST - 1), Ordering::Relaxed);
    }

    /// The theme the developer channel asked for since the editor last looked, if any.
    pub fn take_theme_request(&self) -> Option<u8> {
        match self.dev_theme.swap(NO_REQUEST, Ordering::Relaxed) {
            NO_REQUEST => None,
            theme => Some(theme),
        }
    }

    /// The developer channel asks the editor to open or close the Voice card's expander.
    pub fn request_disclosure(&self, open: bool) {
        self.dev_disclosure.store(u8::from(open), Ordering::Relaxed);
    }

    // ---- editor thread ----

    /// The loudest sample since this was last called, **and resets**.
    pub fn take_peak(&self) -> f32 {
        f32::from_bits(self.peak.swap(0, Ordering::Relaxed))
    }

    pub fn clipped(&self) -> bool {
        self.clipped.load(Ordering::Relaxed)
    }

    /// Acknowledge the clip indication. The user's act, never a timeout.
    pub fn clear_clip(&self) {
        self.clipped.store(false, Ordering::Relaxed);
    }

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

    /// The view the developer channel asked for since the editor last looked, if any.
    pub fn take_view_request(&self) -> Option<usize> {
        match self.dev_view.swap(NO_REQUEST, Ordering::Relaxed) {
            NO_REQUEST => None,
            view => Some(usize::from(view)),
        }
    }

    /// Whether the developer channel asked the expander open or closed since the editor last
    /// looked, if it did.
    pub fn take_disclosure_request(&self) -> Option<bool> {
        match self.dev_disclosure.swap(NO_REQUEST, Ordering::Relaxed) {
            NO_REQUEST => None,
            open => Some(open != 0),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_peak_is_combined_and_reset_on_read() {
        let t = Telemetry::new();
        t.publish_peak(0.4);
        t.publish_peak(0.9);
        t.publish_peak(0.2);
        assert_eq!(t.take_peak(), 0.9, "the loudest of the three, not the last");
        assert_eq!(t.take_peak(), 0.0, "and reading resets it");
    }

    #[test]
    fn a_clip_latches_until_acknowledged() {
        let t = Telemetry::new();
        assert!(!t.clipped());
        t.publish_peak(1.0);
        assert!(t.clipped());
        for _ in 0..100 {
            t.publish_peak(0.1);
        }
        assert!(t.clipped(), "a meter that forgets is worse than no meter");
        t.clear_clip();
        assert!(!t.clipped());
    }

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
    fn a_developer_request_is_taken_once() {
        let t = Telemetry::new();
        assert_eq!(
            t.take_view_request(),
            None,
            "nothing asked on a fresh instance"
        );
        t.request_view(2);
        assert_eq!(t.take_view_request(), Some(2));
        assert_eq!(t.take_view_request(), None, "and taking it clears it");
        t.request_disclosure(true);
        assert_eq!(t.take_disclosure_request(), Some(true));
        t.request_browser(true);
        assert_eq!(t.take_browser_request(), Some(true));
        assert_eq!(t.take_browser_request(), None, "taken once");
        assert_eq!(t.take_disclosure_request(), None);
    }

    #[test]
    fn the_sample_rate_survives_a_round_trip() {
        let t = Telemetry::new();
        t.publish_sample_rate(96_000.0);
        assert_eq!(t.sample_rate(), 96_000.0);
    }
}
