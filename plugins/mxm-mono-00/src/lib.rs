//! mxm-mono-00 — a monophonic semi-modular, as the SYSTEM-100 plug-out pictures the machine.
//!
//! Architecture inspired by the Roland System-100: two oscillators with sync and a ring
//! modulator, a diode ladder, two envelopes, two LFOs, a sample-and-hold, and a fourteen-column by
//! fifteen-row routing matrix through which every patchable input reads its source. Not
//! affiliated with or endorsed by Roland.
//!
//! This file is the plugin shell: identity, parameter plumbing, and MIDI. All the signal
//! processing lives in `mxm-mono-00-dsp`, which knows nothing about nice-plug.
//!
//! # The patch decides whether the plugin is done
//!
//! Every other instrument here is silent when no key is down and its release has ended. This one
//! can be **live** with no key at all — the S&H clock gating the amplifier's envelope, an LFO
//! opening it, INITIAL GAIN up — so the process status is the DSP's own activity verdict:
//! `KeepAlive` while the patch runs itself, a tail while an envelope or an effect settles, and
//! normal after. *Live* is structural, decided from the configuration and never from watching
//! the audio; only the effects' settle hears the signal, restarting while their input is above
//! their snap level.
//!
//! # Notes are a stack the voice owns
//!
//! The DSP keeps the held keys and applies the priority rule itself (low-note or last-note, a
//! parameter), because under low-note priority a higher key over a held lower one changes
//! nothing and the shell cannot know that. Every note event is handed through with its voice id,
//! channel and note, and the voice matches releases against what it holds.

/// The plugin's name, and the **only** place it is written in this crate.
///
/// Everything else that names the instrument derives from here: [`NAME`], which the host shows,
/// and [`CLAP_ID`], which it remembers. A rename is this line.
macro_rules! plugin_name {
    () => {
        "mxm-mono-00"
    };
}

/// What the host displays.
pub const NAME: &str = plugin_name!();

/// The permanent CLAP identifier.
///
/// **Deliberately assembled from [`plugin_name!`] and not from `CARGO_PKG_NAME`**, so a `git mv`
/// of this directory cannot silently move the plugin's identity and orphan every saved project.
pub const CLAP_ID: &str = concat!("dk.mxm.", plugin_name!());

// Public for `apps/mxm-layout-lab` on the `dynamic-layout` branch: the lab draws these real
// cards outside a host. Nothing else about them changes, and the shipped cdylib is unaffected.
pub mod editor;
pub mod params;
pub mod preset;
pub mod routes;
pub mod telemetry;

use mxm_mono_00_dsp::matrix::Column;
use mxm_mono_00_dsp::routing::Routing;
use mxm_mono_00_dsp::voice::{Activity, NoteId, Patch, Voice};
use nice_plug::prelude::*;
use params::MxmMono00Params;
use std::sync::Arc;

/// **A developer channel, off unless asked for.** With `MXM_DEV_CC` set in the plugin's process
/// environment when it is instantiated, two otherwise-undefined control changes drive the editor:
/// [`DEV_VIEW_CC`] addresses categories (0–5) or Parameters (127), never tab positions.
/// [`DEV_DISCLOSURE_CC`] opens (≥ 64) or closes the
/// Voice card's expander. It exists so a script — the player's `cc` verb, an AI, a screenshot
/// run — can put the editor in a state that CLAP gives a host no way to ask for, without a mouse.
/// The gate is the environment and not a parameter, so no host and no preset can trip it, and
/// nothing about it is visible to a person who did not set it. The requests travel through the
/// telemetry atomics, the one channel from the audio thread to the editor; the DSP reads nothing.
const DEV_VIEW_CC: u8 = 119;
const DEV_DISCLOSURE_CC: u8 = 118;
const DEV_BROWSER_CC: u8 = 117;
const DEV_THEME_CC: u8 = 116;
const DEV_CC_ENV: &str = "MXM_DEV_CC";

/// The matrix columns that carry audio, whose lit level is a block peak rather than a sample.
const AUDIO_COLUMNS: [Column; 5] = [
    Column::Vco1,
    Column::Vco2,
    Column::RingMod,
    Column::Noise,
    Column::MixerOut,
];

/// Upper bound on how many samples are rendered between event checks.
const MAX_BLOCK_SIZE: usize = 64;

/// MIDI channels, for the per-channel bend state.
const NUM_CHANNELS: usize = 16;

pub struct MxmMono00 {
    params: Arc<MxmMono00Params>,
    voice: Voice,

    /// The patch the last sample was rendered with.
    ///
    /// Note events need one — the priority rule decides what a key press does — and building a
    /// fresh one would advance every smoother a second time in the sample. The previous sample's
    /// patch is at most one sample stale, which for a switch is nothing.
    patch: Patch,

    /// Pitch bend per channel, in `-1..=1`. CLAP delivers `0..=1` with 0.5 centred.
    bend: [f32; NUM_CHANNELS],
    /// The mod wheel (CC 1) and channel pressure, per channel, `0..=1`.
    ///
    /// **New MIDI paths.** `plan-modulation-routing.md` decision 1.7 puts the collection's
    /// performance inputs on every instrument including the ones whose hardware had none, and this
    /// machine's keyboard has neither a wheel destination nor aftertouch. They are held per channel
    /// for the same reason the bend is: the sounding note's channel is what reaches the voice.
    wheel: [f32; NUM_CHANNELS],
    pressure: [f32; NUM_CHANNELS],

    /// The routing topology the last interval ran.
    ///
    /// Kept across buffers so `Routes::topology_from` can tell which routes have **just** become
    /// present and snap only their smoothers — see `routes::TargetRoutes::arm` for why resuming a
    /// smoother that was skipped is wrong.
    topology: Routing,
    /// The channel of the note that last had the bus, so a release keeps following its bend.
    ///
    /// **The sounding note's channel, not the last note-on's.** Under low-note priority a higher
    /// key on another channel does not take the bus, and its bend must not reach the note that
    /// kept it; the first build indexed the bend by the last note-on and did exactly that.
    last_channel: u8,
    /// Per-note pitch expression, **owned by the note it was sent for**, in semitones.
    ///
    /// Applied while that note is the one sounding, and kept through its release so the pitch it
    /// was bent to holds through the tail. **Not inherited**: when the bus falls back to an older
    /// held key the expression reads zero, because no expression was ever addressed to that key.
    /// A note-on that takes the bus clears it; one that does not — a higher key under low-note
    /// priority — leaves the sounding note's expression alone (code review, 2026-09-22). The first
    /// build kept one unowned number and leaked it across the fallback.
    expression: Option<(NoteId, f32)>,

    /// The delay time TEMPO SYNC resolved for this block, when it is on and a tempo arrived.
    synced_delay_s: Option<f32>,
    /// The sample clock's rate S&H sync resolved for this block, likewise.
    synced_sh_hz: Option<f32>,
    /// Each LFO's rate its sync resolved for this block, likewise; the rate CV and the offset move it
    /// from there, as they move a free rate.
    synced_lfo_hz: [Option<f32>; 2],

    sample_rate: f32,

    /// DSP -> editor, atomics only.
    telemetry: Arc<telemetry::Telemetry>,
    /// Whether the developer channel is on: [`DEV_CC_ENV`] was set when this instance was made.
    dev_cc: bool,
}

impl Default for MxmMono00 {
    fn default() -> Self {
        Self {
            params: Arc::new(MxmMono00Params::default()),
            voice: Voice::new(),
            patch: Patch::default(),
            bend: [0.0; NUM_CHANNELS],
            wheel: [0.0; NUM_CHANNELS],
            pressure: [0.0; NUM_CHANNELS],
            topology: Routing::init(),
            last_channel: 0,
            expression: None,
            synced_delay_s: None,
            synced_sh_hz: None,
            synced_lfo_hz: [None; 2],
            sample_rate: 48_000.0,
            telemetry: telemetry::Telemetry::shared(),
            dev_cc: std::env::var_os(DEV_CC_ENV).is_some(),
        }
    }
}

impl MxmMono00 {
    /// Build one sample's worth of plain values from the parameters.
    ///
    /// Called per sample. Every smoother must be advanced exactly once per sample, so this is the
    /// only place they are read.
    #[inline]
    fn next_patch(&self) -> Patch {
        let p = &self.params;
        let priority = p.priority.value().into();
        let sounding = self.voice.sounding(priority);
        let channel = sounding.map_or(self.last_channel, |id| id.channel) as usize % NUM_CHANNELS;
        let expression_semitones = match (self.expression, sounding) {
            (Some((owner, semitones)), Some(id))
                if owner.matches(id.voice_id, id.channel, id.note) =>
            {
                semitones
            }
            // No key: the release keeps the pitch it was bent to.
            (Some((_, semitones)), None) => semitones,
            _ => 0.0,
        };
        let delay_time_s = p.delay_time.smoothed.next();

        Patch {
            priority,
            range_offset: p.range1.value().semitones(),
            coarse_semitones1: p.coarse1.value() as f32,
            fine_cents1: p.fine1.smoothed.next(),
            master_tune_cents: p.tune.smoothed.next(),
            bend_semitones: self.bend[channel] * p.bend_range.smoothed.next(),
            expression_semitones,
            portamento_s: p.portamento.value(),

            // The performance inputs, as sources. Every one of their pairs starts absent, so a
            // fresh instance is still the copy — and none of them is smoothed here, because a
            // gesture is the host's own stream of values rather than a control this plugin moves.
            wheel: self.wheel[channel],
            pressure: self.pressure[channel],
            bend_position: self.bend[channel],

            wave: p.wave1.value().into(),
            pulse_width: p.pw1.smoothed.next(),

            range_offset2: p.range2.value().semitones(),
            coarse_semitones2: p.coarse2.value() as f32,
            fine_cents2: p.fine2.smoothed.next(),
            wave2: p.wave2.value().into(),
            pulse_width2: p.pw2.smoothed.next(),
            sync_strength: p.sync_strength.value().into(),

            level_vco1: p.vco1_level.smoothed.next(),
            level_vco2: p.vco2_level.smoothed.next(),
            level_noise: p.noise_level.smoothed.next(),
            level_ring: p.ring_level.smoothed.next(),
            noise_colour: p.noise_colour.value().into(),

            hpf_hz: p.hpf.smoothed.next(),
            cutoff_hz: p.cutoff.smoothed.next(),
            resonance: p.resonance.smoothed.next(),

            initial_gain: p.initial_gain.smoothed.next(),
            tone: p.tone.smoothed.next(),

            vcf_adsr: [
                p.vcf_attack.value(),
                p.vcf_decay.value(),
                p.vcf_sustain.smoothed.next(),
                p.vcf_release.value(),
            ],
            vcf_trigger: p.vcf_trigger.value().into(),
            vca_adsr: [
                p.vca_attack.value(),
                p.vca_decay.value(),
                p.vca_sustain.smoothed.next(),
                p.vca_release.value(),
            ],
            vca_trigger: p.vca_trigger.value().into(),

            lfo1_rate_hz: self.synced_lfo_hz[0].unwrap_or_else(|| p.lfo1_rate.value()),
            lfo1_shape: p.lfo1_shape.value().into(),
            lfo1_offset_cents: p.lfo1_offset.smoothed.next(),
            lfo2_rate_hz: self.synced_lfo_hz[1].unwrap_or_else(|| p.lfo2_rate.value()),
            lfo2_shape: p.lfo2_shape.value().into(),
            lfo2_offset_cents: p.lfo2_offset.smoothed.next(),

            sh_rate_hz: self.synced_sh_hz.unwrap_or_else(|| p.sh_rate.value()),
            sh_lag_s: p.sh_lag.value(),

            phaser: p.phaser.smoothed.next(),
            delay_level: p.delay.smoothed.next(),
            // The smoother was advanced above whether or not the sync overrides it, so the
            // slider's own time is current the moment the switch goes off.
            delay_time_s: self.synced_delay_s.unwrap_or(delay_time_s),
            reverb: p.reverb.smoothed.next(),

            volume: p.volume.smoothed.next(),
        }
    }

    /// **Whether a control the activity verdict reads is still ramping** — a level, the
    /// resonance's excitation threshold, INITIAL GAIN, Volume, or an effect that sets the tail.
    /// The verdict is taken from the value reached so far, and a value moves only while the host
    /// calls, so a block ending mid-ramp must not let it park short of the threshold (code
    /// review, 2026-09-22).
    fn controls_are_ramping(&self) -> bool {
        let p = &self.params;
        [
            &p.volume,
            &p.vco1_level,
            &p.vco2_level,
            &p.noise_level,
            &p.ring_level,
            &p.resonance,
            &p.initial_gain,
            &p.delay,
            &p.reverb,
        ]
        .iter()
        .any(|param| param.smoothed.is_smoothing())
    }

    /// Every tempo sync, resolved once per block (`plans/plan-tempo-sync-controls.md`): each
    /// control's modulated position picks a division of the host's tempo on its ladder, clamped to
    /// what its range can hold. With its switch off, or no tempo arriving, its own value stands.
    fn resolve_tempo_sync(&mut self, tempo: Option<f64>) {
        let p = &self.params;
        // **The modulated position**: a sequencer lock on the time is a modulation offset, and it
        // must pick the division the way a moved control would (review round 2).
        let resolve = |ladder: mxm_tempo::Ladder, on: bool, param: &FloatParam| {
            ladder
                .resolve(
                    on,
                    tempo,
                    param.modulated_normalized_value(),
                    f64::from(param.preview_plain(0.0)),
                    f64::from(param.preview_plain(1.0)),
                )
                .map(|value| value as f32)
        };
        self.synced_delay_s = resolve(params::DELAY_SYNC, p.tempo_sync.value(), &p.delay_time);
        self.synced_sh_hz = resolve(params::SH_SYNC, p.sh_sync.value(), &p.sh_rate);
        self.synced_lfo_hz = [
            resolve(params::LFO_SYNC, p.lfo1_sync.value(), &p.lfo1_rate),
            resolve(params::LFO_SYNC, p.lfo2_sync.value(), &p.lfo2_rate),
        ];
        self.telemetry.tempo.publish(tempo);
    }

    fn handle_event(&mut self, event: NoteEvent<()>) {
        // **The priority in force for this event's sample**, not the last rendered sample's:
        // at a block boundary the parameter may have moved since, and an event routed by the
        // old priority would name the wrong key (review round 3). Reading the parameter's value
        // advances no smoother.
        let mut patch = self.patch;
        patch.priority = self.params.priority.value().into();
        let sounding_before = self.voice.sounding(patch.priority);
        match event {
            NoteEvent::NoteOn {
                voice_id,
                channel,
                note,
                velocity,
                ..
            } => {
                // Velocity zero is a note-off by convention. Otherwise it drives nothing hard-wired
                // — the hardware's keyboard has none — and reaches the voice only as the Velocity
                // source, below.
                if velocity <= 0.0 {
                    self.voice.note_off(voice_id, channel, note, &patch);
                } else {
                    // **Velocity is a source now, and is still not a hard-wired path.** The voice
                    // keeps it with the press, so the Velocity source is the sounding press's.
                    let id = NoteId {
                        voice_id,
                        channel,
                        note,
                    };
                    // A note that takes the bus carries no expression until the host sends one.
                    // One that does not leaves the sounding note's alone — asked of the voice,
                    // which knows which *press* sounds: two presses of one key with no voice id
                    // are equal ids.
                    if self.voice.note_on(id, velocity, &patch) {
                        self.expression = None;
                    }
                }
            }

            // Under the same copy as every other arm: this one still read the last rendered
            // sample's priority after round 3 (review round 4). Its reach was one sample -- the
            // render loop puts the bus on the selected key whenever that key changes -- so no
            // test can see it, and it is fixed for the contract's sentence to be true.
            NoteEvent::NoteOff {
                voice_id,
                channel,
                note,
                ..
            } => self.voice.note_off(voice_id, channel, note, &patch),

            // Immediate, no release — for **the note**: CLAP's choke stops a voice, and the
            // effects' tails after the amplifier are not the voice. All Sound Off is what clears
            // them (review round 3 raised it; the distinction is deliberate).
            NoteEvent::Choke {
                voice_id,
                channel,
                note,
                ..
            } => self.voice.choke(voice_id, channel, note, &patch),

            // **Per-note pitch, from the host's piano roll**, applied only while it names the note
            // that is sounding: an expression for a note this monophonic voice is not playing
            // belongs to no gate here.
            // **Addressed by voice id when the host gives one** — two presses of the same key
            // on one channel are two voices, and the expression names one of them (review
            // round 2). **Finite only**: a NaN from a host would otherwise become every sample.
            NoteEvent::PolyTuning {
                voice_id,
                channel,
                note,
                tuning,
                ..
            } => {
                let sounding = self.voice.sounding(patch.priority);
                if let Some(id) = sounding
                    && id.matches(voice_id, channel, note)
                    && tuning.is_finite()
                {
                    self.expression = Some((id, tuning));
                }
            }

            NoteEvent::MidiPitchBend { channel, value, .. } => {
                if value.is_finite() {
                    self.bend[channel as usize % NUM_CHANNELS] =
                        2.0 * (value.clamp(0.0, 1.0) - 0.5);
                }
            }

            NoteEvent::MidiChannelPressure {
                channel, pressure, ..
            } => {
                if pressure.is_finite() {
                    self.pressure[channel as usize % NUM_CHANNELS] = pressure.clamp(0.0, 1.0);
                }
            }

            NoteEvent::MidiCC {
                channel, cc, value, ..
            } => match cc {
                // All sound off: immediate, no release.
                control_change::ALL_SOUND_OFF => self.voice.all_sound_off(),
                // All notes off: deliberately different, the note releases normally.
                control_change::ALL_NOTES_OFF => self.voice.all_notes_off(),
                // The developer channel, only when this instance was started with it.
                DEV_VIEW_CC if self.dev_cc => self
                    .telemetry
                    .request_view((value.clamp(0.0, 1.0) * 127.0).round() as u8),
                DEV_DISCLOSURE_CC if self.dev_cc => {
                    self.telemetry.request_disclosure(value >= 0.5);
                }
                DEV_BROWSER_CC if self.dev_cc => {
                    self.telemetry.request_browser(value >= 0.5);
                }
                // A theme by index, 0 light / 1 dark / 2 system, as the app bar's control lists
                // them. Applied to the editor and never saved: a capture run must not rewrite the
                // choice the person at the machine made.
                DEV_THEME_CC if self.dev_cc => {
                    self.telemetry
                        .request_theme((value.clamp(0.0, 1.0) * 127.0).round() as u8);
                }
                // **The mod wheel is a routable source.** It is still consumed as a gesture and
                // writes no parameter, which is what `docs/MXM_CONTROL_MAP.md`'s CC 1 reservation
                // forbids; what is a parameter is *how much of it reaches this target*, which is
                // the amount every other route has (`plan-modulation-routing.md` §5.2 part 1).
                control_change::MODULATION_MSB if value.is_finite() => {
                    self.wheel[channel as usize % NUM_CHANNELS] = value.clamp(0.0, 1.0);
                }
                _ => {}
            },

            _ => {}
        }

        // **A release keeps the expression of the note it releases, and only that.** When the
        // last key goes up and the note that was sounding never owned the stored expression —
        // the bus had fallen back to it — the expression is retired, or the release would jump to
        // a tuning that note never had (code review, 2026-09-22). A note the bus falls back to
        // keeps its own expression while held; this only decides what the release holds.
        if let (Some(before), None) = (sounding_before, self.voice.sounding(patch.priority))
            && self.expression.is_some_and(|(owner, _)| {
                !owner.matches(before.voice_id, before.channel, before.note)
            })
        {
            self.expression = None;
        }
    }
}

impl Plugin for MxmMono00 {
    const NAME: &'static str = crate::NAME;
    const VENDOR: &'static str = "mxm";
    const URL: &'static str = "https://mxm.dk";
    const EMAIL: &'static str = "plugins@mxm.dk";
    const VERSION: &'static str = env!("CARGO_PKG_VERSION");

    /// An instrument: no main input. Stereo output is dual mono — the voice is monophonic, as the
    /// hardware's HIGH OUTPUT was.
    const AUDIO_IO_LAYOUTS: &'static [AudioIOLayout] = &[
        AudioIOLayout {
            main_input_channels: None,
            main_output_channels: NonZeroU32::new(2),
            ..AudioIOLayout::const_default()
        },
        AudioIOLayout {
            main_input_channels: None,
            main_output_channels: NonZeroU32::new(1),
            ..AudioIOLayout::const_default()
        },
    ];

    /// `MidiCCs` rather than `Basic`: pitch bend, CC 120 and CC 123 are all needed.
    const MIDI_INPUT: MidiConfig = MidiConfig::MidiCCs;

    const SAMPLE_ACCURATE_AUTOMATION: bool = false;

    type Editor = editor::MxmMono00Editor;
    type SysExMessage = ();
    type BackgroundTask = ();

    fn params(&self) -> Arc<dyn Params> {
        self.params.clone()
    }

    fn editor(&mut self, _async_executor: AsyncExecutor<Self>) -> Option<Self::Editor> {
        editor::create(self.params.clone(), self.telemetry.clone())
    }

    fn activate(
        &mut self,
        _audio_io_layout: &AudioIOLayout,
        buffer_config: &BufferConfig,
        _context: &mut impl ActivateContext<Self>,
    ) -> bool {
        // A new activation starts with no tempo and nothing resolved: the first callback reports
        // the tempo, so neither the audio nor an editor frame before it shows the last session's
        // divisions (`plans/plan-tempo-sync-controls.md`).
        self.telemetry.tempo.publish(None);
        self.synced_delay_s = None;
        self.synced_sh_hz = None;
        self.synced_lfo_hz = [None; 2];
        // A rate the DSP's clamps cannot hold is refused before anything changes: a NaN, or one
        // below `MIN_SAMPLE_RATE`, crosses a `clamp` bound and panics on the audio thread.
        if !buffer_config.sample_rate.is_finite()
            || buffer_config.sample_rate < mxm_mono_00_dsp::MIN_SAMPLE_RATE
        {
            return false;
        }
        self.sample_rate = buffer_config.sample_rate;
        self.voice.set_sample_rate(self.sample_rate);
        self.voice.reset();
        self.telemetry.publish_sample_rate(self.sample_rate);
        // The ladder's oversampling delays the audio by `Voice::latency_samples()`, twenty-two
        // samples, and it is **deliberately not reported**: see the crate's AGENTS.md.
        true
    }

    fn reset(&mut self) {
        self.voice.reset();
        self.bend = [0.0; NUM_CHANNELS];
        // The routable performance sources too, as `mxm-mono-02` clears them: a wheel or pressure
        // routed to the amplitude would otherwise reopen the patch from before the reset.
        self.wheel = [0.0; NUM_CHANNELS];
        self.pressure = [0.0; NUM_CHANNELS];
        self.last_channel = 0;
        self.expression = None;
        self.synced_delay_s = None;
        self.synced_sh_hz = None;
        self.synced_lfo_hz = [None; 2];
    }

    /// **A project saved before the tempo syncs** restores each Off rather than keeping this
    /// instance's, and a loaded preset's baseline gains it, so the preset stays clean
    /// (`mxm_preset::add_switches_off`).
    fn filter_state(state: &mut PluginState) {
        mxm_preset::add_switches_off(state, crate::preset::TEMPO_SYNC_IDS);
    }

    fn process(
        &mut self,
        buffer: &mut Buffer,
        _aux: &mut AuxiliaryBuffers,
        context: &mut impl ProcessContext<Self>,
    ) -> ProcessStatus {
        self.resolve_tempo_sync(context.transport().tempo);

        // **The topology, once per buffer.** Which routes exist is discrete and changes only on a
        // parameter event, so compaction runs here rather than per sample — and this is the call
        // `Graph::begin_sample` asserts on, because a voice that is never armed renders silence
        // that looks like a quiet patch.
        self.topology = self.params.routes.topology_from(&self.topology);
        self.voice.set_topology(&self.topology);

        let num_samples = buffer.samples();
        let mut next_event = context.next_event();
        let mut block_start = 0usize;

        while block_start < num_samples {
            let mut block_end = (block_start + MAX_BLOCK_SIZE).min(num_samples);

            // Apply everything scheduled at or before this point, then shorten the block so the
            // next event lands exactly where it should.
            loop {
                match next_event {
                    Some(event) if (event.timing() as usize) <= block_start => {
                        self.handle_event(event);
                        next_event = context.next_event();
                    }
                    Some(event) if (event.timing() as usize) < block_end => {
                        block_end = event.timing() as usize;
                        break;
                    }
                    _ => break,
                }
            }

            {
                let output = buffer.as_slice();
                let mut block_peak = 0.0f32;
                // The audio columns' peaks over the block, for the lit matrix: their last sample
                // would be a random point on a waveform. Five reads and a max per sample.
                let mut audio_peaks = [0.0f32; AUDIO_COLUMNS.len()];
                for i in block_start..block_end {
                    self.patch = self.next_patch();
                    if let Some(id) = self.voice.sounding(self.patch.priority) {
                        self.last_channel = id.channel;
                    }
                    // **This sample's route depths, in place.** Only a live route's smoother is
                    // advanced, which is the whole of the efficiency design: an absent pair costs
                    // the branch that skips it and nothing else. The grid is mutated rather than
                    // rebuilt because it is 1.5 KB and its topology changed at the top of the
                    // buffer — see `Patch`'s own note on why it is not a field there.
                    self.params.routes.advance_into(&mut self.topology);
                    let sample = self.voice.process(&self.patch, &self.topology);
                    block_peak = block_peak.max(sample.abs());
                    for (peak, column) in audio_peaks.iter_mut().zip(AUDIO_COLUMNS) {
                        *peak = peak.max(self.voice.column(column).abs());
                    }
                    for channel in output.iter_mut() {
                        channel[i] = sample;
                    }
                }
                self.telemetry.publish_peak(block_peak);
                for column in Column::ALL {
                    let level = match AUDIO_COLUMNS.iter().position(|c| *c == column) {
                        Some(at) => audio_peaks[at],
                        None => self.voice.column(column),
                    };
                    self.telemetry.publish_column(column, level);
                }
            }

            block_start = block_end;
        }

        let activity = self.voice.activity(&self.patch, &self.topology);
        self.telemetry.publish_activity(activity);
        match activity {
            // The patch runs itself: the host must keep calling whether or not a key is down.
            Activity::Live => ProcessStatus::KeepAlive,
            // A depth still ramping is a verdict not yet reached: the ramp finishes first.
            _ if self.params.routes.is_ramping(&self.topology) || self.controls_are_ramping() => {
                ProcessStatus::KeepAlive
            }
            Activity::Tailing => {
                ProcessStatus::Tail(self.voice.tail_samples(&self.patch, &self.topology))
            }
            Activity::Inert => ProcessStatus::Normal,
        }
    }
}

impl ClapPlugin for MxmMono00 {
    /// Permanent. Reverse DNS of a domain the project owns, so it survives moving between forges.
    const CLAP_ID: &'static str = CLAP_ID;
    const CLAP_DESCRIPTION: Option<&'static str> = Some(
        "A monophonic semi-modular synthesizer with two oscillators, a diode-ladder filter, a sample and hold and free modulation routing",
    );
    const CLAP_MANUAL_URL: Option<&'static str> = None;
    const CLAP_SUPPORT_URL: Option<&'static str> = None;
    const CLAP_FEATURES: &'static [ClapFeature] = &[
        ClapFeature::Instrument,
        ClapFeature::Synthesizer,
        ClapFeature::Stereo,
        ClapFeature::Mono,
    ];
}

nice_export_clap!(MxmMono00);

/// The plugin's name, checked where it escapes this crate.
#[cfg(test)]
mod identity {
    use super::{CLAP_ID, NAME};

    /// The id is built from the name, so it cannot drift from it.
    #[test]
    fn the_id_is_the_name_under_the_project_domain() {
        assert_eq!(CLAP_ID, format!("dk.mxm.{NAME}"));
    }

    /// `bundler.toml` names the same instrument this crate does — the one place the plugin's name
    /// is duplicated outside this crate, read by `xtask` at bundle time and never by the plugin.
    #[test]
    fn the_bundle_is_named_after_this_plugin() {
        mxm_plugin_test::bundle::is_named(env!("CARGO_MANIFEST_DIR"), env!("CARGO_PKG_NAME"), NAME);
    }
}

#[cfg(test)]
mod init_patch {
    use super::params::MxmMono00Params;
    use mxm_mono_00_dsp::routing::{INIT_AT_FULL, INIT_PRESENT, SOURCE_NAMES, TARGET_NAMES};
    use nice_plug::prelude::Params;

    /// **Pins the rule, not the taste.** Every amount starts at zero.
    #[test]
    fn every_amount_starts_at_zero() {
        let p = MxmMono00Params::default();
        for (name, value) in [
            ("vco-2 level", p.vco2_level.value()),
            ("noise level", p.noise_level.value()),
            ("ring mod level", p.ring_level.value()),
            ("resonance", p.resonance.value()),
            ("initial gain", p.initial_gain.value()),
            ("tone", p.tone.value()),
            ("phaser", p.phaser.value()),
            ("delay", p.delay.value()),
            ("reverb", p.reverb.value()),
            ("tune", p.tune.value()),
        ] {
            assert_eq!(value, 0.0, "{name} is an amount and must start at zero");
        }
        assert!(!p.tempo_sync.value(), "tempo sync is off");
    }

    /// The init contract's *slightly detuned*: VCO-2 starts seven cents sharp, the one amount that
    /// does not start at zero, recorded as chosen in the DSP crate.
    #[test]
    fn vco2_starts_slightly_detuned() {
        let p = MxmMono00Params::default();
        assert_eq!(p.fine2.value(), 7.0);
    }

    /// Key follow reaches both ways, centred: the plug-out's bipolar control, which is the cutoff
    /// input's keyboard route now — wired at Init, at zero.
    #[test]
    fn key_follow_is_bipolar_and_centred() {
        use nice_plug::params::Param;
        let p = MxmMono00Params::default();
        let key = &p.routes.cutoff.key;
        assert!(
            p.routes.cutoff.key_on.value(),
            "the keyboard is wired to the cutoff"
        );
        assert_eq!(key.value(), 0.0);
        assert_eq!(
            key.preview_plain(0.0),
            -1.0,
            "the bottom of the control is inverse tracking"
        );
        assert_eq!(key.preview_plain(1.0), 1.0);
        // One octave of cutoff per octave of keyboard: the retired KYBD CV slider's 100 %, read per
        // octave like every Key route (the modulation standard).
        assert_eq!(key.normalized_value_to_string(1.0, true), "+1.00 oct/oct");
    }

    /// The init patch makes a sound: VCO-1 up, the amplifier's envelope up, the filter open.
    #[test]
    fn the_init_patch_can_be_heard() {
        let p = MxmMono00Params::default();
        assert!(p.vco1_level.value() > 0.0);
        // The amplifier's own input, wired to Envelope 2 and turned up — what `vcaenv` was.
        let amplifier = p.routes.all()[mxm_mono_00_dsp::routing::target::AMPLIFIER].1;
        assert!(amplifier.env2_on.value());
        assert!(amplifier.env2.value() > 0.0);
        assert!(p.cutoff.value() >= 5_000.0, "{}", p.cutoff.value());
        assert!(p.volume.value() > 0.0);
    }

    /// **The init patch is the plug-out's own wiring**, and every route amount is an amount:
    /// zero, except the four connections that never had an attenuator to inherit a zero from.
    ///
    /// These four are this conversion's init deviations, recorded in
    /// `plugins/mxm-mono-00/AGENTS.md` beside the three the instrument already had.
    #[test]
    fn the_routing_starts_as_the_machines_own_patch() {
        let p = MxmMono00Params::default();
        let mut wired = 0;
        for (target, group) in p.routes.all() {
            for (source, route) in group.routes(target).iter().enumerate() {
                let at_full = INIT_AT_FULL.contains(&(target, source));
                let present = INIT_PRESENT.contains(&(target, source)) || at_full;
                assert_eq!(
                    route.is_present(),
                    present,
                    "{} from {}",
                    TARGET_NAMES[target],
                    SOURCE_NAMES[source]
                );
                if present {
                    wired += 1;
                }
                // The sync normal waits at full while absent (routes.rs `amount()`); every other
                // absent pair's amount is an amount and starts at zero.
                // By plain value: a one-sided fader has its zero at an end.
                if !present && (target, source) != mxm_mono_00_dsp::routing::SYNC_NORMAL {
                    assert_eq!(
                        group.amount_param(source).value(),
                        0.0,
                        "{} from {} starts at zero",
                        TARGET_NAMES[target],
                        SOURCE_NAMES[source]
                    );
                }
                if present && !at_full {
                    assert_eq!(
                        route.amount.normalised(),
                        0.5,
                        "{} from {} is an amount and starts at zero",
                        TARGET_NAMES[target],
                        SOURCE_NAMES[source]
                    );
                }
            }
        }
        assert_eq!(
            wired, 13,
            "the plug-out wires thirteen: seven through the patch bay's inputs and six on its panel"
        );
    }

    /// The routing groups are the DSP's targets and sources, in the DSP's order — so a preset,
    /// a control-map role and a keyboard scope all point at the same pair.
    #[test]
    fn the_routing_surface_is_the_dsp_declaration() {
        use mxm_mono_00_dsp::routing::{SOURCES, TARGETS};
        let p = MxmMono00Params::default();
        assert_eq!(p.routes.all().len(), TARGETS);
        for (index, (target, group)) in p.routes.all().iter().enumerate() {
            assert_eq!(*target, index);
            assert_eq!(group.routes(index).len(), SOURCES);
            for (source, route) in group.routes(index).iter().enumerate() {
                assert_eq!(route.source, SOURCE_NAMES[source]);
            }
        }
    }

    /// The parameter ids are declared once and are all reachable through the derive.
    #[test]
    fn every_parameter_has_a_distinct_id() {
        let ids: Vec<String> = MxmMono00Params::default()
            .param_map()
            .into_iter()
            .map(|(id, _, _)| id)
            .collect();
        let mut sorted = ids.clone();
        sorted.sort();
        sorted.dedup();
        assert_eq!(sorted.len(), ids.len(), "duplicate ids in {ids:?}");
        // 52 controls and 866 routing parameters: 49 until the three tempo syncs of 2026-09-25
        // (`shsync`, `lfo1sync`, `lfo2sync`); 850 routing until the modulation standard added the
        // Amplitude's fifty and refused thirty-four (2026-09-27). `AGENTS.md` states the same count.
        assert_eq!(ids.len(), 918, "the inventory moved");
    }
}

/// The path from a host's note events to the voice.
#[cfg(test)]
mod notes {
    use super::MxmMono00;
    use nice_plug::prelude::*;

    fn note_on(plugin: &mut MxmMono00, note: u8) {
        plugin.handle_event(NoteEvent::NoteOn {
            timing: 0,
            voice_id: None,
            channel: 0,
            note,
            velocity: 0.8,
        });
    }

    fn note_off(plugin: &mut MxmMono00, note: u8) {
        plugin.handle_event(NoteEvent::NoteOff {
            timing: 0,
            voice_id: None,
            channel: 0,
            note,
            velocity: 0.0,
        });
    }

    fn tune(plugin: &mut MxmMono00, note: u8, semitones: f32) {
        plugin.handle_event(NoteEvent::PolyTuning {
            timing: 0,
            voice_id: None,
            channel: 0,
            note,
            tuning: semitones,
        });
    }

    fn sounding(plugin: &MxmMono00) -> Option<u8> {
        plugin
            .voice
            .sounding(plugin.patch.priority)
            .map(|id| id.note)
    }

    fn cc(plugin: &mut MxmMono00, cc: u8, raw: u8) {
        plugin.handle_event(NoteEvent::MidiCC {
            timing: 0,
            channel: 0,
            cc,
            value: f32::from(raw) / 127.0,
        });
    }

    /// The developer channel reaches the editor only when the instance was started with it; a
    /// host sending the same control change to an ordinary instance changes nothing.
    #[test]
    fn the_developer_channel_is_off_unless_the_environment_asked_for_it() {
        let mut plugin = MxmMono00 {
            dev_cc: false,
            ..Default::default()
        };
        cc(&mut plugin, crate::DEV_VIEW_CC, 2);
        cc(&mut plugin, crate::DEV_DISCLOSURE_CC, 127);
        cc(&mut plugin, crate::DEV_BROWSER_CC, 127);
        cc(&mut plugin, crate::DEV_THEME_CC, 1);
        assert_eq!(plugin.telemetry.take_view_request(), None);
        assert_eq!(plugin.telemetry.take_disclosure_request(), None);
        assert_eq!(plugin.telemetry.take_browser_request(), None);
        assert_eq!(plugin.telemetry.take_theme_request(), None);

        plugin.dev_cc = true;
        cc(&mut plugin, crate::DEV_VIEW_CC, 2);
        cc(&mut plugin, crate::DEV_DISCLOSURE_CC, 127);
        cc(&mut plugin, crate::DEV_BROWSER_CC, 127);
        cc(&mut plugin, crate::DEV_THEME_CC, 1);
        assert_eq!(
            plugin.telemetry.take_view_request(),
            Some(2),
            "FX is the third view"
        );
        assert_eq!(plugin.telemetry.take_disclosure_request(), Some(true));
        assert_eq!(plugin.telemetry.take_browser_request(), Some(true));
        assert_eq!(
            plugin.telemetry.take_theme_request(),
            Some(1),
            "1 is dark, as mxm_ui::theme::from_index reads it"
        );
        // Light is index 0 — a request like any other, not the absence of one.
        cc(&mut plugin, crate::DEV_THEME_CC, 0);
        assert_eq!(plugin.telemetry.take_theme_request(), Some(0));
        // View zero is a request too, not the absence of one.
        cc(&mut plugin, crate::DEV_VIEW_CC, 0);
        assert_eq!(plugin.telemetry.take_view_request(), Some(0));
        cc(&mut plugin, crate::DEV_DISCLOSURE_CC, 0);
        assert_eq!(plugin.telemetry.take_disclosure_request(), Some(false));
    }

    #[test]
    fn low_note_priority_is_the_default_and_a_higher_key_does_not_take_the_bus() {
        let mut plugin = MxmMono00::default();
        note_on(&mut plugin, 48);
        note_on(&mut plugin, 55);
        assert_eq!(
            sounding(&plugin),
            Some(48),
            "the lower held key keeps the bus"
        );
        note_off(&mut plugin, 48);
        assert_eq!(
            sounding(&plugin),
            Some(55),
            "and the bus falls back to the other key"
        );
        note_off(&mut plugin, 55);
        assert_eq!(sounding(&plugin), None);
    }

    #[test]
    fn a_tuning_expression_for_the_sounding_note_reaches_the_patch() {
        let mut plugin = MxmMono00::default();
        note_on(&mut plugin, 48);
        assert_eq!(plugin.next_patch().expression_semitones, 0.0, "the premise");
        tune(&mut plugin, 48, -3.5);
        assert_eq!(plugin.next_patch().expression_semitones, -3.5);
    }

    #[test]
    fn an_expression_for_another_note_is_ignored() {
        let mut plugin = MxmMono00::default();
        note_on(&mut plugin, 48);
        tune(&mut plugin, 55, 7.0);
        assert_eq!(plugin.next_patch().expression_semitones, 0.0);
    }

    #[test]
    fn a_new_note_starts_clean_and_a_release_keeps_what_it_was_bent_to() {
        let mut plugin = MxmMono00::default();
        note_on(&mut plugin, 48);
        tune(&mut plugin, 48, 5.0);
        note_off(&mut plugin, 48);
        assert_eq!(
            plugin.next_patch().expression_semitones,
            5.0,
            "the note releases at the pitch it was bent to"
        );
        note_on(&mut plugin, 52);
        assert_eq!(plugin.next_patch().expression_semitones, 0.0);
    }

    fn note_on_channel(plugin: &mut MxmMono00, channel: u8, note: u8) {
        plugin.handle_event(NoteEvent::NoteOn {
            timing: 0,
            voice_id: None,
            channel,
            note,
            velocity: 0.8,
        });
    }

    fn note_off_channel(plugin: &mut MxmMono00, channel: u8, note: u8) {
        plugin.handle_event(NoteEvent::NoteOff {
            timing: 0,
            voice_id: None,
            channel,
            note,
            velocity: 0.0,
        });
    }

    fn bend(plugin: &mut MxmMono00, channel: u8, value: f32) {
        plugin.handle_event(NoteEvent::MidiPitchBend {
            timing: 0,
            channel,
            value,
        });
    }

    /// **Review round 1.** Under low-note priority a higher key on another channel does not take
    /// the bus, so its channel's bend must not reach the note that kept it.
    #[test]
    fn pitch_bend_follows_the_sounding_notes_channel_and_not_the_last_note_ons() {
        let mut plugin = MxmMono00::default();
        // Outside a host nothing seeds the smoother with its default, so it would read zero.
        plugin.params.bend_range.smoothed.reset(2.0);
        note_on_channel(&mut plugin, 0, 48);
        bend(&mut plugin, 0, 1.0);
        let full = plugin.next_patch().bend_semitones;
        assert!(full > 1.0, "channel 0's bend reaches its note: {full}");

        // A higher key on channel 1, whose bend is centred: note 48 keeps the bus, and its bend.
        note_on_channel(&mut plugin, 1, 60);
        assert_eq!(
            sounding(&plugin),
            Some(48),
            "the premise: low-note priority"
        );
        assert_eq!(plugin.next_patch().bend_semitones, full);
        bend(&mut plugin, 1, 0.0);
        assert_eq!(
            plugin.next_patch().bend_semitones,
            full,
            "channel 1's bend belongs to a note that is not sounding"
        );

        // Release the lower key: the bus falls back to channel 1's note, and its bend applies.
        note_off_channel(&mut plugin, 0, 48);
        assert_eq!(sounding(&plugin), Some(60));
        assert!(plugin.next_patch().bend_semitones < -1.0);
    }

    /// **Review round 3.** An event at a block boundary is routed by the priority in force, not
    /// by the priority of the last rendered sample.
    #[test]
    fn an_event_is_routed_by_the_priority_in_force() {
        use super::params::PriorityKind;
        use nice_plug::params::InternalParamMut;
        let mut plugin = MxmMono00::default();
        note_on(&mut plugin, 48);
        note_on(&mut plugin, 60);
        // The parameter moves, and no sample has been rendered since.
        unsafe {
            plugin
                .params
                .priority
                ._internal_set_plain_value(PriorityKind::Last)
        };
        tune(&mut plugin, 60, 4.0);
        assert_eq!(
            plugin.next_patch().expression_semitones,
            4.0,
            "under the priority in force, 60 is the sounding note and takes the tuning"
        );
    }

    /// **Review round 2.** Two presses of one key with distinct voice ids are two voices, and a
    /// tuning names one of them; a NaN tuning names nothing.
    #[test]
    fn a_tuning_is_addressed_by_voice_id_and_a_non_finite_one_is_dropped() {
        let mut plugin = MxmMono00::default();
        let press = |plugin: &mut MxmMono00, voice_id: i32| {
            plugin.handle_event(NoteEvent::NoteOn {
                timing: 0,
                voice_id: Some(voice_id),
                channel: 0,
                note: 60,
                velocity: 0.8,
            });
        };
        let tune_voice = |plugin: &mut MxmMono00, voice_id: i32, semitones: f32| {
            plugin.handle_event(NoteEvent::PolyTuning {
                timing: 0,
                voice_id: Some(voice_id),
                channel: 0,
                note: 60,
                tuning: semitones,
            });
        };
        press(&mut plugin, 1);
        press(&mut plugin, 2);
        // Under low-note priority the first press, voice 1, keeps the bus.
        tune_voice(&mut plugin, 2, 5.0);
        assert_eq!(
            plugin.next_patch().expression_semitones,
            0.0,
            "voice 2's expression must not bend voice 1"
        );
        tune_voice(&mut plugin, 1, 3.0);
        assert_eq!(plugin.next_patch().expression_semitones, 3.0);

        tune_voice(&mut plugin, 1, f32::NAN);
        assert_eq!(
            plugin.next_patch().expression_semitones,
            3.0,
            "a NaN is dropped"
        );
        plugin.handle_event(NoteEvent::MidiPitchBend {
            timing: 0,
            channel: 0,
            value: f32::NAN,
        });
        assert!(plugin.next_patch().bend_semitones.is_finite());
    }

    /// **Review round 2.** A sequencer's lock on the delay time is a modulation offset, and the
    /// synced division must follow it.
    #[test]
    fn tempo_sync_follows_a_modulation_offset_on_the_time() {
        use nice_plug::params::InternalParamMut;
        let mut plugin = MxmMono00::default();
        // SAFETY: the same calls the wrapper makes when a host writes and modulates a parameter.
        unsafe {
            plugin.params.tempo_sync._internal_set_plain_value(true);
            plugin.params.delay_time._internal_set_normalized_value(0.0);
        }
        plugin.resolve_tempo_sync(Some(120.0));
        assert_eq!(
            plugin.synced_delay_s,
            Some(0.0625),
            "the bottom: a thirty-second"
        );
        unsafe { plugin.params.delay_time._internal_modulate_value(1.0) };
        plugin.resolve_tempo_sync(Some(120.0));
        assert_eq!(
            plugin.synced_delay_s,
            Some(1.0),
            "modulated to the top: two beats"
        );
    }

    /// **A key that does not take the bus leaves the sounding note's expression alone.** Every
    /// note-on cleared it, so a higher key under low-note priority wiped the bend of the note that
    /// kept sounding (code review, 2026-09-22).
    #[test]
    fn a_press_that_does_not_take_the_bus_keeps_the_sounding_expression() {
        let mut plugin = MxmMono00::default();
        plugin.patch = plugin.next_patch();
        note_on(&mut plugin, 48);
        tune(&mut plugin, 48, 2.0);
        note_on(&mut plugin, 60);
        assert_eq!(
            sounding(&plugin),
            Some(48),
            "low-note priority keeps the lower key"
        );
        assert_eq!(
            plugin.next_patch().expression_semitones,
            2.0,
            "a key that did not take the bus wiped the sounding note's expression"
        );
        // A second press of the sounding key, with no voice id: the first still sounds.
        note_on(&mut plugin, 48);
        assert_eq!(
            plugin.next_patch().expression_semitones,
            2.0,
            "a duplicate press that did not take the bus wiped the expression"
        );
        note_on(&mut plugin, 36);
        assert_eq!(
            plugin.next_patch().expression_semitones,
            0.0,
            "a key that takes the bus starts clean"
        );
    }

    /// **A control the verdict reads keeps the plugin called while it ramps** — Resonance on its
    /// way to self-oscillation above all. **Falsified** by dropping `resonance` from
    /// `controls_are_ramping`.
    #[test]
    fn a_resonance_ramp_is_a_control_still_ramping() {
        let plugin = MxmMono00::default();
        assert!(!plugin.controls_are_ramping(), "at rest");
        plugin.params.resonance.smoothed.set_target(48_000.0, 1.0);
        assert!(plugin.controls_are_ramping());
        plugin.params.resonance.smoothed.reset(1.0);
        assert!(!plugin.controls_are_ramping(), "and arrived");
    }

    /// **The Velocity source reaches the voice from the press it is sounding** — through the
    /// plugin, whose own last-note-on copy this replaced (code review, 2026-09-22).
    #[test]
    fn the_velocity_source_is_the_sounding_presss() {
        use nice_plug::prelude::NoteEvent;
        let mut plugin = MxmMono00::default();
        plugin.patch = plugin.next_patch();
        for (note, velocity) in [(48, 0.3), (60, 0.9)] {
            plugin.handle_event(NoteEvent::NoteOn {
                timing: 0,
                voice_id: None,
                channel: 0,
                note,
                velocity,
            });
        }
        assert_eq!(sounding(&plugin), Some(48));
        assert_eq!(
            plugin.voice.velocity(),
            0.3,
            "the lower press sounds, at its own velocity"
        );
    }

    /// **Review round 1.** A tuning sent for the newer key must not follow the bus back to the
    /// older one when the newer is released.
    #[test]
    fn a_tuning_does_not_follow_the_bus_back_to_an_older_key() {
        use super::params::PriorityKind;
        use nice_plug::params::InternalParamMut;
        let mut plugin = MxmMono00::default();
        // SAFETY: the same call the wrapper makes when a host writes a parameter.
        unsafe {
            plugin
                .params
                .priority
                ._internal_set_plain_value(PriorityKind::Last)
        };
        plugin.patch = plugin.next_patch();

        note_on(&mut plugin, 60);
        note_on(&mut plugin, 64);
        tune(&mut plugin, 64, 5.0);
        assert_eq!(plugin.next_patch().expression_semitones, 5.0, "the premise");

        note_off(&mut plugin, 64);
        assert_eq!(
            sounding(&plugin),
            Some(60),
            "last-note priority falls back to the held key"
        );
        assert_eq!(
            plugin.next_patch().expression_semitones,
            0.0,
            "no expression was ever addressed to the older key"
        );

        // And the release of the last key keeps what *it* was bent to.
        tune(&mut plugin, 60, -2.0);
        note_off(&mut plugin, 60);
        assert_eq!(plugin.next_patch().expression_semitones, -2.0);
    }

    /// **A release after a fallback holds nothing it was not sent.** Hold 48, tune it, hold 60,
    /// release 48: 60 sounds at zero. Releasing 60 used to bring 48's +5 back for the release,
    /// because the stored expression outlived the note that owned it (code review, 2026-09-22).
    /// **Falsified** by dropping the retire block at the end of `handle_event`.
    #[test]
    fn a_release_after_a_fallback_does_not_revive_another_notes_expression() {
        let mut plugin = MxmMono00::default();
        plugin.patch = plugin.next_patch();
        note_on(&mut plugin, 48);
        tune(&mut plugin, 48, 5.0);
        note_on(&mut plugin, 60);
        note_off(&mut plugin, 48);
        assert_eq!(sounding(&plugin), Some(60));
        assert_eq!(plugin.next_patch().expression_semitones, 0.0, "the premise");
        note_off(&mut plugin, 60);
        assert_eq!(
            plugin.next_patch().expression_semitones,
            0.0,
            "the release jumped to 48's tuning"
        );
    }

    #[test]
    fn a_velocity_zero_note_on_is_a_release() {
        let mut plugin = MxmMono00::default();
        note_on(&mut plugin, 48);
        plugin.handle_event(NoteEvent::NoteOn {
            timing: 0,
            voice_id: None,
            channel: 0,
            note: 48,
            velocity: 0.0,
        });
        assert_eq!(sounding(&plugin), None);
    }

    #[test]
    fn tempo_sync_picks_a_division_of_the_tempo_and_is_inert_without_one() {
        use nice_plug::params::InternalParamMut;
        let mut plugin = MxmMono00::default();
        plugin.resolve_tempo_sync(Some(120.0));
        assert_eq!(plugin.synced_delay_s, None, "off by default");

        // SAFETY: the same call the wrapper makes when a host writes a parameter; a unit test
        // has no host to obtain a `ParamSetter` from.
        unsafe { plugin.params.tempo_sync._internal_set_plain_value(true) };
        plugin.resolve_tempo_sync(None);
        assert_eq!(plugin.synced_delay_s, None, "no tempo, no sync");

        // The time at its top is the longest division: a half note, one second at 120.
        unsafe { plugin.params.delay_time._internal_set_normalized_value(1.0) };
        plugin.resolve_tempo_sync(Some(120.0));
        assert_eq!(plugin.synced_delay_s, Some(1.0));

        // And at its bottom the shortest, a thirty-second: 62.5 ms at 120 — the old table's ends.
        unsafe { plugin.params.delay_time._internal_set_normalized_value(0.0) };
        plugin.resolve_tempo_sync(Some(120.0));
        assert_eq!(plugin.synced_delay_s, Some(0.0625));
    }

    /// **A synced rate moves the way it moves free** (`plans/plan-tempo-sync-controls.md`): the
    /// sample clock's and both LFOs' tops are their fastest divisions. The sample clock's was
    /// reversed when S&H sync first landed — its bottom was the fastest (review, 2026-09-25).
    #[test]
    fn a_synced_rate_ticks_once_per_division_fastest_at_the_top() {
        use mxm_tempo::Division;
        use nice_plug::params::InternalParamMut;
        let mut plugin = MxmMono00::default();
        // SAFETY: as above.
        unsafe {
            plugin.params.sh_sync._internal_set_plain_value(true);
            plugin.params.lfo1_sync._internal_set_plain_value(true);
            plugin.params.lfo2_sync._internal_set_plain_value(true);
        }
        plugin.resolve_tempo_sync(None);
        assert_eq!(plugin.synced_sh_hz, None, "no tempo, no sync");
        assert_eq!(plugin.synced_lfo_hz, [None; 2]);

        // The tops: the sample clock's 1/32 at 120 is sixteen ticks a second, and an LFO's too.
        unsafe {
            plugin.params.sh_rate._internal_set_normalized_value(1.0);
            plugin.params.lfo1_rate._internal_set_normalized_value(1.0);
        }
        plugin.resolve_tempo_sync(Some(120.0));
        assert_eq!(plugin.synced_sh_hz, Some(16.0));
        assert_eq!(plugin.synced_lfo_hz[0], Some(16.0));

        // A quarter note at 120 is two a second, on either.
        unsafe {
            plugin
                .params
                .sh_rate
                ._internal_set_normalized_value(crate::params::SH_SYNC.position(Division::Quarter));
            plugin.params.lfo2_rate._internal_set_normalized_value(
                crate::params::LFO_SYNC.position(Division::Quarter),
            );
        }
        plugin.resolve_tempo_sync(Some(120.0));
        assert_eq!(plugin.synced_sh_hz, Some(2.0));
        assert_eq!(plugin.synced_lfo_hz[1], Some(2.0));
        // The bottom of an LFO's ladder is four bars — 0.125 Hz at 120, under the LFO's 0.15 Hz
        // floor — so it clamps to the nearest it can hold, two bars, rather than rescaling.
        unsafe { plugin.params.lfo1_rate._internal_set_normalized_value(0.0) };
        plugin.resolve_tempo_sync(Some(120.0));
        assert_eq!(plugin.synced_lfo_hz[0], Some(0.25));
    }
}

/// The host's sample rate at activation: the floor the DSP's clamps are safe above.
#[cfg(test)]
mod sample_rate_floor {
    use super::*;
    use mxm_mono_00_dsp::MIN_SAMPLE_RATE;

    struct Activation;

    impl ActivateContext<MxmMono00> for Activation {
        fn plugin_api(&self) -> PluginApi {
            PluginApi::Clap
        }
        fn execute(&self, _task: ()) {}
        fn set_latency_samples(&self, _samples: u32) {}
        fn set_current_voice_capacity(&self, _capacity: u32) {}
    }

    struct NoEvents {
        transport: Transport,
    }

    impl ProcessContext<MxmMono00> for NoEvents {
        fn plugin_api(&self) -> PluginApi {
            PluginApi::Clap
        }
        fn execute_background(&self, _task: ()) {}
        fn execute_gui(&self, _task: ()) {}
        fn transport(&self) -> &Transport {
            &self.transport
        }
        fn next_event(&mut self) -> Option<NoteEvent<()>> {
            None
        }
        fn send_event(&mut self, _event: NoteEvent<()>) {}
        fn set_latency_samples(&self, _samples: u32) {}
        fn set_current_voice_capacity(&self, _capacity: u32) {}
    }

    fn activate_at(plugin: &mut MxmMono00, sample_rate: f32) -> bool {
        plugin.activate(
            &MxmMono00::AUDIO_IO_LAYOUTS[0],
            &BufferConfig {
                sample_rate,
                min_buffer_size: Some(1),
                max_buffer_size: 4096,
                process_mode: ProcessMode::Realtime,
            },
            &mut Activation,
        )
    }

    fn render(plugin: &mut MxmMono00, frames: usize) -> Vec<f32> {
        let mut out = vec![0.0f32; frames];
        let mut buffer = Buffer::default();
        unsafe {
            buffer.set_slices(frames, |channels| {
                channels.clear();
                channels.push(out.as_mut_slice());
            });
        }
        let mut inputs = [];
        let mut outputs = [];
        let mut aux = AuxiliaryBuffers {
            inputs: &mut inputs,
            outputs: &mut outputs,
        };
        plugin.process(
            &mut buffer,
            &mut aux,
            &mut NoEvents {
                transport: Transport::new(MIN_SAMPLE_RATE),
            },
        );
        drop(buffer);
        out
    }

    /// **The floor activates and plays, whatever the parameters say.** Every parameter at its
    /// default, then all at the bottom of their ranges, then all at the top — every route present
    /// at full — with a note held for four seconds at 1 kHz.
    #[test]
    fn the_rate_floor_activates_and_plays_at_every_parameter_extreme() {
        for extreme in [None, Some(0.0), Some(1.0)] {
            let mut plugin = MxmMono00::default();
            for (_, ptr, _) in plugin.params.param_map() {
                if let Some(value) = extreme {
                    let _ = unsafe { ptr._internal_set_normalized_value(value) };
                }
                unsafe { ptr._internal_update_smoother(MIN_SAMPLE_RATE, true) };
            }
            assert!(activate_at(&mut plugin, MIN_SAMPLE_RATE), "{extreme:?}");
            assert_eq!(plugin.sample_rate, MIN_SAMPLE_RATE);
            plugin.handle_event(NoteEvent::NoteOn {
                timing: 0,
                voice_id: None,
                channel: 0,
                note: 48,
                velocity: 0.8,
            });
            let out = render(&mut plugin, 4_000);
            assert!(out.iter().all(|s| s.is_finite()), "{extreme:?}");
        }
    }

    /// **A rate the DSP's clamps cannot hold is refused at activation.** `f32::clamp` panics on
    /// a NaN or inverted bound, so a NaN rate or one low enough to cross a corner's floor over its
    /// Nyquist fraction panicked on the audio thread. A refusal leaves the plugin as it was.
    #[test]
    fn activation_refuses_a_non_finite_rate_and_any_below_the_floor() {
        for unsupported in [
            MIN_SAMPLE_RATE.next_down(),
            100.0,
            20.0,
            1.0,
            0.0,
            -48_000.0,
            f32::NAN,
            f32::INFINITY,
            f32::NEG_INFINITY,
        ] {
            let mut refused = MxmMono00::default();
            assert!(
                !activate_at(&mut refused, unsupported),
                "accepted {unsupported} Hz"
            );
            assert_eq!(refused.sample_rate, 48_000.0, "{unsupported} Hz");
        }
    }
}

/// The status the host is told, through the real callback.
#[cfg(test)]
mod process_status {
    use super::*;
    use nice_plug::params::InternalParamMut;
    use std::collections::VecDeque;

    const FS: f32 = 48_000.0;
    const BLOCK: usize = 512;

    struct Activation;

    impl ActivateContext<MxmMono00> for Activation {
        fn plugin_api(&self) -> PluginApi {
            PluginApi::Clap
        }
        fn execute(&self, _task: ()) {}
        fn set_latency_samples(&self, _samples: u32) {}
        fn set_current_voice_capacity(&self, _capacity: u32) {}
    }

    struct Events {
        transport: Transport,
        events: VecDeque<NoteEvent<()>>,
    }

    impl ProcessContext<MxmMono00> for Events {
        fn plugin_api(&self) -> PluginApi {
            PluginApi::Clap
        }
        fn execute_background(&self, _task: ()) {}
        fn execute_gui(&self, _task: ()) {}
        fn transport(&self) -> &Transport {
            &self.transport
        }
        fn next_event(&mut self) -> Option<NoteEvent<()>> {
            self.events.pop_front()
        }
        fn send_event(&mut self, _event: NoteEvent<()>) {}
        fn set_latency_samples(&self, _samples: u32) {}
        fn set_current_voice_capacity(&self, _capacity: u32) {}
    }

    fn block(plugin: &mut MxmMono00, event: Option<NoteEvent<()>>) -> (Vec<f32>, ProcessStatus) {
        let mut out = vec![0.0f32; BLOCK];
        let mut buffer = Buffer::default();
        unsafe {
            buffer.set_slices(BLOCK, |channels| {
                channels.clear();
                channels.push(out.as_mut_slice());
            });
        }
        let mut inputs = [];
        let mut outputs = [];
        let mut aux = AuxiliaryBuffers {
            inputs: &mut inputs,
            outputs: &mut outputs,
        };
        let status = plugin.process(
            &mut buffer,
            &mut aux,
            &mut Events {
                transport: Transport::new(FS),
                events: event.into_iter().collect(),
            },
        );
        drop(buffer);
        (out, status)
    }

    /// A two-second filter release on the cutoff and a 20 ms amplifier release, the filter
    /// envelope routed to the amplifier as well or not.
    fn plugin(amplifier_reads_the_filter_envelope: bool) -> MxmMono00 {
        let mut plugin = MxmMono00 {
            dev_cc: false,
            ..Default::default()
        };
        // SAFETY: the writes the wrapper makes for host parameter events; a unit test has no host.
        unsafe {
            let p = &plugin.params;
            p.vcf_sustain._internal_set_plain_value(1.0);
            p.vcf_release._internal_set_plain_value(2.0);
            p.vca_release._internal_set_plain_value(0.02);
            p.routes.cutoff.env1._internal_set_plain_value(1.0);
            p.routes
                .amplifier
                .env1_on
                ._internal_set_plain_value(amplifier_reads_the_filter_envelope);
            p.routes.amplifier.env1._internal_set_plain_value(1.0);
            for (_, ptr, _) in p.param_map() {
                ptr._internal_update_smoother(FS, true);
            }
        }
        assert!(plugin.activate(
            &MxmMono00::AUDIO_IO_LAYOUTS[0],
            &BufferConfig {
                sample_rate: FS,
                min_buffer_size: Some(1),
                max_buffer_size: BLOCK as u32,
                process_mode: ProcessMode::Realtime,
            },
            &mut Activation,
        ));
        plugin
    }

    /// Plays a 300 ms note and releases it; returns what the release's first block promised and
    /// how many samples after the release the status turned `Normal`.
    fn release(plugin: &mut MxmMono00) -> (ProcessStatus, usize) {
        block(
            plugin,
            Some(NoteEvent::NoteOn {
                timing: 0,
                voice_id: None,
                channel: 0,
                note: 60,
                velocity: 0.8,
            }),
        );
        for _ in 0..(0.3 * FS) as usize / BLOCK {
            block(plugin, None);
        }
        let (_, promised) = block(
            plugin,
            Some(NoteEvent::NoteOff {
                timing: 0,
                voice_id: None,
                channel: 0,
                note: 60,
                velocity: 0.0,
            }),
        );
        let mut elapsed = BLOCK;
        while block(plugin, None).1 != ProcessStatus::Normal {
            elapsed += BLOCK;
            assert!(elapsed < (8.0 * FS) as usize, "never Normal");
        }
        (promised, elapsed + BLOCK)
    }

    /// **Audit D13, through the callback.** A filter release that only moves the cutoff is no
    /// tail: the status is `Normal` once the amplifier's own release and the settle are over, the
    /// output is exact silence, and routing that envelope to the amplifier afterwards revives
    /// nothing. Routed there from the start, it is the tail, and the host is promised its length.
    #[test]
    fn the_status_follows_the_envelopes_the_amplifier_reads() {
        let tail = |status: ProcessStatus| match status {
            ProcessStatus::Tail(n) => n as usize,
            other => panic!("the release did not report a tail: {other:?}"),
        };

        let mut connected = plugin(true);
        let (promised, normal_after) = release(&mut connected);
        assert!(
            normal_after > (3.5 * FS) as usize,
            "the amplifier hears the filter's release: Normal after {normal_after}"
        );
        assert!(
            tail(promised) + 2 * BLOCK >= normal_after,
            "promised {} samples, Normal after {normal_after}",
            tail(promised)
        );

        let mut disconnected = plugin(false);
        let (promised, normal_after) = release(&mut disconnected);
        assert!(
            normal_after < (0.2 * FS) as usize,
            "a release on the cutoff alone kept the status a tail for {normal_after} samples"
        );
        assert!(
            tail(promised) < (0.2 * FS) as usize,
            "promised {} samples",
            tail(promised)
        );
        let (silence, _) = block(&mut disconnected, None);
        assert!(silence.iter().all(|&s| s == 0.0), "Normal, and not silent");

        // SAFETY: as above.
        unsafe {
            disconnected
                .params
                .routes
                .amplifier
                .env1_on
                ._internal_set_plain_value(true);
        }
        let (rerouted, status) = block(&mut disconnected, None);
        assert_eq!(status, ProcessStatus::Normal, "re-routing revived a tail");
        assert!(
            rerouted.iter().all(|&s| s == 0.0),
            "re-routing revived the release"
        );
    }
}

/// **Activation forgets the last session's tempo and resolved syncs**: the first callback reports the
/// tempo, so nothing — the audio, or an editor frame before it — starts from the previous session's
/// divisions.
#[cfg(test)]
mod activation_forgets_the_tempo {
    use super::*;

    #[test]
    fn activation_forgets_the_last_tempo_and_resolved_syncs() {
        use nice_plug::prelude::Plugin as _;
        let mut plugin = MxmMono00::default();
        plugin.telemetry.tempo.publish(Some(120.0));
        plugin.synced_lfo_hz = [Some(1.0); 2];
        plugin.synced_delay_s = Some(0.5);
        let layout = MxmMono00::AUDIO_IO_LAYOUTS[0];
        let config = BufferConfig {
            sample_rate: 48_000.0,
            min_buffer_size: None,
            max_buffer_size: 512,
            process_mode: ProcessMode::Realtime,
        };
        let _ = plugin.activate(&layout, &config, &mut NoInit);
        assert_eq!(plugin.telemetry.tempo.get(), None);
        assert_eq!(plugin.synced_lfo_hz, [None; 2]);
        assert_eq!(plugin.synced_delay_s, None);
    }

    /// An activation context that asks nothing of a host.
    struct NoInit;

    impl ActivateContext<MxmMono00> for NoInit {
        fn plugin_api(&self) -> PluginApi {
            PluginApi::Clap
        }
        fn execute(&self, _task: <MxmMono00 as Plugin>::BackgroundTask) {}
        fn set_latency_samples(&self, _samples: u32) {}
        fn set_current_voice_capacity(&self, _capacity: u32) {}
    }
}

/// What a player reads — on hover in the editor, and in a host's plugin browser — speaks to the
/// player about the sound, never about the machine or the code (`mxm_plugin_test::hover_text`).
#[cfg(test)]
mod speaks_to_the_player {
    #[test]
    fn hover_text() {
        mxm_plugin_test::hover_text::speaks_to_the_player(env!("CARGO_MANIFEST_DIR"));
    }

    #[test]
    fn host_description() {
        mxm_plugin_test::hover_text::host_description_speaks_to_the_player(env!(
            "CARGO_MANIFEST_DIR"
        ));
    }
}
