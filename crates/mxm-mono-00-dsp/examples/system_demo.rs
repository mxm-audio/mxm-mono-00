//! The listening harness: does the voice sound like a synthesizer, and do its warts sound like
//! the machine's? Renders `mxm-mono-00-system-demo.wav` at the repository root, one passage per
//! mechanism, each announced on stderr with its time so a listener knows what to listen for.
//!
//! Named for the machine rather than for what it does, which is the collection's rule for examples:
//! cargo writes every example in the workspace to one flat `target/*/examples/` directory, so two
//! crates sharing a name share an output file. See `docs/known-issues.md`.
//!
//! ```bash
//! cargo run -p mxm-mono-00-dsp --release --example system_demo
//! ```

/// Writes a listening demo, applying this collection's demo headroom law **at the call site**.
///
/// `mxm_audio_file` encodes what it is given and applies no gain — normalisation is a judgement
/// about the material and the file crate carries no policy. The law here is the one the
/// six hand-written writers all applied internally: leave 2 % of headroom, and scale down further if
/// the material is over full scale.
fn write_demo(path: &str, interleaved: &[f32], channels: u16, rate: u32) {
    let peak = mxm_measure::level::peak(interleaved)
        .expect("a rendered demo is finite; a NaN here is a DSP defect, not a level");
    let gain = if peak > 1.0 { 0.98 / peak } else { 0.98 };
    let scaled: Vec<f32> = interleaved.iter().map(|s| s * gain).collect();
    mxm_audio_file::write(
        path,
        &scaled,
        channels,
        rate,
        mxm_audio_file::Target::Wav(mxm_audio_file::Bits::Sixteen),
    )
    .expect("the demo is written");
}

/// **Where this demo's channel count and sample rate are decided — once, for `main` and for the
/// test below.** Both call this, so a change to either constant changes both paths and the test's
/// literal expectations catch it. With the two supplied separately at each site, a `main` passing
/// the wrong channel count left the test perfectly green.
const DEMO_CHANNELS: u16 = 1;

fn write_demo_file(path: &str, interleaved: &[f32]) {
    write_demo(path, interleaved, DEMO_CHANNELS, FS as u32);
}

use mxm_mono_00_dsp::lfo::Shape;
use mxm_mono_00_dsp::noise::Colour;
use mxm_mono_00_dsp::oscillator::{SyncStrength, Wave};
use mxm_mono_00_dsp::routing::{PITCH_SEMITONES_PER_UNIT, Routing, source, target};
use mxm_mono_00_dsp::voice::{
    GLIDE_MAX_SEMITONES, NoteId, Patch, Priority, Trigger, VCO_LFO_SEMITONES, Voice,
};

const FS: f32 = 48_000.0;

fn id(note: u8) -> NoteId {
    NoteId {
        voice_id: None,
        channel: 0,
        note,
    }
}

/// The retired VCO LFO slider at `slider`, as a pitch route's amount.
fn vibrato(slider: f32) -> f32 {
    slider * VCO_LFO_SEMITONES / PITCH_SEMITONES_PER_UNIT
}

struct Render {
    voice: Voice,
    out: Vec<f32>,
}

/// A patch and its routing, which travel together.
#[derive(Clone, Copy)]
struct Sound {
    patch: Patch,
    routing: Routing,
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
    /// The plug-out as it comes.
    fn default() -> Self {
        Self::with(Patch::default())
    }
}

impl Sound {
    /// The plug-out's own wiring, with some controls moved.
    fn with(patch: Patch) -> Self {
        Self {
            patch,
            routing: Routing::init(),
        }
    }
}

impl Render {
    fn new() -> Self {
        let mut voice = Voice::new();
        voice.set_sample_rate(FS);
        Self {
            voice,
            out: Vec::new(),
        }
    }

    fn seconds(&self) -> f32 {
        self.out.len() as f32 / FS
    }

    /// A patch and the grid beside it: the routing is not in `Patch`, because `Patch` is copied
    /// every sample. The demo's `..init` inheritance carries the patch; the grid travels with it.
    fn hold(&mut self, s: &Sound, secs: f32) {
        self.voice.set_topology(&s.routing);
        for _ in 0..(FS * secs) as usize {
            self.out.push(self.voice.process(&s.patch, &s.routing));
        }
    }

    fn note(&mut self, s: &Sound, note: u8, on: f32, off: f32) {
        self.voice.set_topology(&s.routing);
        self.voice.note_on(id(note), 1.0, &s.patch);
        self.hold(s, on);
        self.voice.note_off(None, 0, note, &s.patch);
        self.hold(s, off);
    }

    fn passage(&mut self, name: &str) {
        eprintln!("{:>6.2}s  {name}", self.seconds());
    }
}

fn main() {
    let mut r = Render::new();

    r.passage("the init patch: one plain sawtooth, filter open, the ladder's own tone");
    let init = Sound::default();
    for n in [48u8, 52, 55, 60] {
        r.note(&init, n, 0.35, 0.15);
    }

    r.passage("the triangle, with its seam (reedy low, flute-like high)");
    let tri = Sound::with(Patch {
        wave: Wave::Triangle,
        cutoff_hz: 8_000.0,
        ..init.patch
    });
    for n in [36u8, 48, 60, 72, 84] {
        r.note(&tri, n, 0.3, 0.1);
    }

    r.passage("the filter: envelope sweep at rising resonance, the bass surviving");
    for (i, res) in [0.0f32, 0.5, 0.8, 0.95].iter().enumerate() {
        let mut p = Sound::with(Patch {
            cutoff_hz: 150.0,
            resonance: *res,
            vcf_adsr: [0.005, 0.4, 0.1, 0.2],
            ..init.patch
        });
        // The filter's own ADSR input, normalled to Envelope 1 in the init patch — its depth.
        p.routing.amounts[target::CUTOFF][source::VCF_ADSR] = 0.6;
        r.note(&p, 36 + (i as u8 % 2) * 7, 0.6, 0.2);
    }

    r.passage("auto glide: a semitone from below on every gate edge, none across a tie");
    let mut glide = Sound::with(Patch {
        priority: Priority::Last,
        ..init.patch
    });
    // The hardware's semitone, on both pitch inputs as DESTINATION's default sent it.
    let semitone = -0.5 * GLIDE_MAX_SEMITONES / PITCH_SEMITONES_PER_UNIT;
    glide.routing.amounts[target::VCO1_PITCH][source::GLIDE] = semitone;
    glide.routing.amounts[target::VCO2_PITCH][source::GLIDE] = semitone;
    r.note(&glide, 60, 0.4, 0.1);
    r.note(&glide, 60, 0.4, 0.1);
    r.voice.note_on(id(60), 1.0, &glide);
    r.hold(&glide, 0.4);
    r.voice.note_on(id(67), 1.0, &glide); // the tie: no dip
    r.hold(&glide, 0.4);
    r.voice.note_off(None, 0, 67, &glide);
    r.voice.note_off(None, 0, 60, &glide);
    r.hold(&glide, 0.3);

    r.passage("portamento: the hold capacitor, and a release mid-lag that holds its pitch");
    let porta = Sound::with(Patch {
        portamento_s: 0.8,
        priority: Priority::Last,
        vca_adsr: [0.01, 0.2, 0.8, 0.6],
        ..init.patch
    });
    r.voice.note_on(id(48), 1.0, &porta);
    r.hold(&porta, 0.5);
    r.voice.note_on(id(72), 1.0, &porta);
    r.hold(&porta, 0.25); // release a third of the way up
    r.voice.note_off(None, 0, 72, &porta);
    r.voice.note_off(None, 0, 48, &porta);
    r.hold(&porta, 0.8);
    r.note(&porta, 60, 0.6, 0.6); // starts from wherever the slide stopped

    r.passage("wart 1: the same vibrato depth as sine, then sawtooth, then square");
    for shape in [Shape::Sine, Shape::Saw, Shape::Square] {
        let mut p = Sound::with(Patch {
            lfo1_rate_hz: 5.5,
            lfo1_shape: shape,
            ..init.patch
        });
        p.routing.amounts[target::VCO1_PITCH][source::LFO1] = vibrato(0.15);
        p.routing.amounts[target::VCO2_PITCH][source::LFO1] = vibrato(0.15);
        r.note(&p, 60, 0.7, 0.15);
    }

    r.passage("wart 3: tremolo that only dips — sine dips on its top half, sawtooth every cycle");
    for shape in [Shape::Sine, Shape::Saw] {
        let mut p = Sound::with(Patch {
            initial_gain: 0.7,
            lfo1_rate_hz: 4.0,
            lfo1_shape: shape,
            ..init.patch
        });
        p.routing.clear_target(target::AMPLIFIER);
        p.routing.amounts[target::TREMOLO][source::LFO1] = 0.6;
        r.hold(&p, 1.0); // no key: INITIAL GAIN holds the amplifier open
    }
    r.hold(&init, 0.3);

    r.passage("PWM from the LFO's own triangle: narrowing from square, never past it");
    let mut pwm = Sound::with(Patch {
        wave: Wave::Square,
        lfo1_rate_hz: 0.7,
        lfo1_shape: Shape::Square, // and the switch does not reach the PWM
        ..init.patch
    });
    pwm.routing
        .set(target::VCO1_WIDTH, source::LFO1_CORE_TRIANGLE, 1.0);
    r.note(&pwm, 48, 2.5, 0.3);

    r.passage("noise: white with its bottom removed, then pink in two steps");
    for colour in [Colour::White, Colour::Pink] {
        let p = Sound::with(Patch {
            level_vco1: 0.0,
            level_noise: 0.8,
            noise_colour: colour,
            ..init.patch
        });
        r.note(&p, 60, 0.8, 0.2);
    }

    r.passage("the LFO trigger: the envelope refires on LFO-1's square while the key is held");
    let repeat = Sound::with(Patch {
        vca_trigger: Trigger::Lfo,
        vcf_trigger: Trigger::Lfo,
        vca_adsr: [0.002, 0.12, 0.0, 0.1],
        vcf_adsr: [0.002, 0.12, 0.0, 0.1],
        cutoff_hz: 300.0,
        resonance: 0.6,
        lfo1_rate_hz: 6.0,
        ..init.patch
    });
    r.note(&repeat, 40, 2.0, 0.3);

    r.passage("two oscillators: VCO-2 a fifth up, then the init detune alone");
    let two = Sound::with(Patch {
        level_vco2: 0.8,
        coarse_semitones2: 7.0,
        fine_cents2: 0.0,
        ..init.patch
    });
    r.note(&two, 48, 0.8, 0.2);
    let beating = Sound::with(Patch {
        level_vco2: 0.8,
        ..init.patch
    });
    r.note(&beating, 48, 1.2, 0.3);

    r.passage("strong sync: VCO-2 swept up two octaves against VCO-1, the timbre under it");
    for i in 0..24 {
        let mut p = Sound::with(Patch {
            level_vco1: 0.0,
            level_vco2: 0.8,
            coarse_semitones2: i as f32,
            fine_cents2: 0.0,
            sync_strength: SyncStrength::Strong,
            vca_adsr: [0.002, 0.1, 1.0, 0.05],
            ..init.patch
        });
        // Sync is on because the plug-out's normal is wired, at the hardware's full reset depth.
        p.routing.set(target::VCO2_SYNC, source::VCO1_SYNC, 1.0);
        r.note(&p, 45, 0.1, 0.0);
    }
    r.hold(&init, 0.3);

    r.passage("weak sync: the same sweep, locking near the ratios and rolling between");
    for i in 0..24 {
        let mut p = Sound::with(Patch {
            level_vco1: 0.0,
            level_vco2: 0.8,
            coarse_semitones2: i as f32,
            fine_cents2: -15.0,
            sync_strength: SyncStrength::Weak,
            vca_adsr: [0.002, 0.1, 1.0, 0.05],
            ..init.patch
        });
        p.routing.set(target::VCO2_SYNC, source::VCO1_SYNC, 1.0);
        r.note(&p, 45, 0.1, 0.0);
    }
    r.hold(&init, 0.3);

    r.passage("the ring modulator: VCO-1 × VCO-2 a major seventh apart — the bell patch");
    let mut bell = Sound::with(Patch {
        level_vco1: 0.0,
        coarse_semitones2: 11.0,
        fine_cents2: 0.0,
        vca_adsr: [0.002, 0.6, 0.0, 0.4],
        vcf_adsr: [0.002, 0.6, 0.0, 0.4],
        cutoff_hz: 4_000.0,
        ..init.patch
    });
    bell.level_ring = 0.8;
    for n in [60u8, 64, 67, 72] {
        r.note(&bell, n, 0.5, 0.3);
    }

    r.passage("cross-modulation: VCO-2 into VCO-1's pitch, the route's depth rising");
    for amount in [0.05f32, 0.15, 0.4] {
        let mut p = Sound::with(Patch {
            coarse_semitones2: 5.0,
            fine_cents2: 0.0,
            ..init.patch
        });
        // Depths chosen at EXT CV's 120 semitones per unit; the pitch input reaches 144.
        p.routing.amounts[target::VCO1_PITCH][source::VCO2] = amount * 120.0 / 144.0;
        r.note(&p, 48, 0.6, 0.15);
    }

    r.passage("the matrix: S&H into VCO-1's pitch — the random-pitch patch, clocked at 6 Hz");
    let mut random = Sound::with(Patch {
        sh_rate_hz: 6.0,
        lfo1_rate_hz: 0.37,
        cutoff_hz: 3_000.0,
        vca_adsr: [0.005, 0.2, 0.9, 0.3],
        ..init.patch
    });
    random
        .routing
        .set(target::SH_INPUT, source::LFO1_CORE_SAW, 1.0);
    random.routing.clear_target(target::VCO1_PITCH);
    random
        .routing
        .set(target::VCO1_PITCH, source::SH_OUT, 0.12 * 120.0 / 144.0);
    r.note(&random, 48, 3.0, 0.4);

    r.passage(
        "the same, with the S&H clock into the VCA envelope's gate — wart 16, self-running, no key",
    );
    let mut clocked = Sound::with(Patch {
        vca_adsr: [0.002, 0.15, 0.0, 0.1],
        vcf_adsr: [0.002, 0.15, 0.0, 0.1],
        cutoff_hz: 200.0,
        resonance: 0.5,
        ..random.patch
    });
    clocked.routing.amounts[target::CUTOFF][source::VCF_ADSR] = 0.6;
    clocked
        .routing
        .set(target::SH_INPUT, source::LFO1_CORE_SAW, 1.0);
    clocked.routing.clear_target(target::VCA_GATE);
    clocked.routing.set(target::VCA_GATE, source::SH_CLOCK, 1.0);
    clocked.routing.clear_target(target::VCF_GATE);
    clocked.routing.set(target::VCF_GATE, source::SH_CLOCK, 1.0);
    r.hold(&clocked, 3.0);
    r.hold(&init, 0.3);

    r.passage(
        "LFO-2 into the VCF's LFO row, sawtooth: the DC level lifts the cutoff, then sweeps it",
    );
    let mut lfo2_vcf = Sound::with(Patch {
        lfo2_rate_hz: 1.5,
        lfo2_shape: Shape::Saw,
        cutoff_hz: 300.0,
        resonance: 0.6,
        vca_adsr: [0.005, 0.3, 0.8, 0.3],
        ..init.patch
    });
    lfo2_vcf.routing.clear(target::CUTOFF, source::LFO1);
    lfo2_vcf.routing.set(target::CUTOFF, source::LFO2, 0.5);
    r.note(&lfo2_vcf, 40, 2.5, 0.3);

    r.passage("VCO-2 into LFO-1's rate: FM on a modulation source, the vibrato turned to grit");
    let mut lfo_fm = Sound::with(Patch {
        lfo1_rate_hz: 5.0,
        coarse_semitones2: -12.0,
        fine_cents2: 0.0,
        ..init.patch
    });
    lfo_fm.routing.amounts[target::VCO1_PITCH][source::LFO1] = vibrato(0.25);
    lfo_fm.routing.amounts[target::VCO2_PITCH][source::LFO1] = vibrato(0.25);
    lfo_fm.routing.clear_target(target::LFO1_RATE);
    lfo_fm.routing.set(target::LFO1_RATE, source::VCO2, 0.6);
    r.note(&lfo_fm, 55, 1.5, 0.3);

    r.passage("the mixer into its own external input, level rising: the loop and the overdrive");
    for level in [0.4f32, 0.7, 1.0] {
        let mut fb = Sound::with(Patch {
            cutoff_hz: 1_500.0,
            resonance: 0.4,
            ..init.patch
        });
        fb.routing.clear_target(target::MIXER_INPUT);
        fb.routing
            .set(target::MIXER_INPUT, source::MIXER_OUT, level);
        r.note(&fb, 43, 0.7, 0.15);
    }

    r.passage("the phaser: the slider rising, its own sweep speeding up with it");
    for amount in [0.3f32, 0.7, 1.0] {
        let p = Sound::with(Patch {
            level_vco2: 0.8,
            fine_cents2: 9.0,
            cutoff_hz: 6_000.0,
            phaser: amount,
            ..init.patch
        });
        r.note(&p, 48, 1.2, 0.2);
    }

    r.passage("the phaser under the matrix: LFO-1's sawtooth on MANUAL IN sweeps the centre");
    let mut manual = Sound::with(Patch {
        level_vco2: 0.8,
        fine_cents2: 9.0,
        cutoff_hz: 6_000.0,
        phaser: 0.8,
        lfo1_rate_hz: 0.5,
        lfo1_shape: Shape::Saw,
        ..init.patch
    });
    manual.routing.clear_target(target::PHASER_CENTRE);
    manual.routing.set(target::PHASER_CENTRE, source::LFO1, 1.0);
    r.note(&manual, 48, 2.5, 0.3);

    r.passage("the delay: a pluck at 375 ms, then at a dotted 250 ms, the repeats decaying alone");
    for time in [0.375f32, 0.25] {
        let p = Sound::with(Patch {
            vca_adsr: [0.002, 0.25, 0.0, 0.1],
            vcf_adsr: [0.002, 0.25, 0.0, 0.1],
            cutoff_hz: 800.0,
            delay_level: 0.5,
            delay_time_s: time,
            ..init.patch
        });
        r.note(&p, 55, 0.15, 2.2);
    }

    r.passage("the spring: the same pluck into the 103's tank, then its drip on a square");
    let spring = Sound::with(Patch {
        vca_adsr: [0.002, 0.25, 0.0, 0.1],
        vcf_adsr: [0.002, 0.25, 0.0, 0.1],
        cutoff_hz: 800.0,
        reverb: 0.7,
        ..init.patch
    });
    r.note(&spring, 55, 0.15, 3.0);
    let drip = Sound::with(Patch {
        wave: Wave::Square,
        vca_adsr: [0.002, 0.08, 0.0, 0.05],
        cutoff_hz: 5_000.0,
        reverb: 0.9,
        ..init.patch
    });
    for n in [67u8, 72, 79] {
        r.note(&drip, n, 0.05, 1.2);
    }

    r.passage("end");

    let peak = r.out.iter().fold(0.0f32, |m, s| m.max(s.abs()));
    let path = "mxm-mono-00-system-demo.wav";
    write_demo_file(path, &r.out);
    eprintln!("wrote {path}: {:.1} s, peak {peak:.3}", r.seconds());
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The demo's own write path, exercised through the same wrapper `main` uses.
    ///
    /// The shared encoder is proved in `mxm-measure` against fixed header and payload bytes. What
    /// that cannot see is *this* file later writing the wrong channel count or rate, so the
    /// expectations here are **literals** — the facts about this instrument — rather than the
    /// constants under test.
    #[test]
    fn the_demo_write_path_produces_a_playable_file() {
        let frames = 256;
        let samples: Vec<f32> = (0..frames * DEMO_CHANNELS as usize)
            .map(|i| {
                let t = i as f32 / 48_000 as f32;
                // Past full scale, so the headroom branch is taken rather than skipped.
                1.6 * (std::f32::consts::TAU * 220.0 * t).sin()
            })
            .collect();

        let mut path = std::env::temp_dir();
        path.push(format!("system-demo-demo-{}.wav", std::process::id()));
        write_demo_file(path.to_str().expect("a utf-8 path"), &samples);

        let read = mxm_audio_file_decode::decode_file(
            &path,
            &mxm_audio_file_decode::Limits::new(
                usize::MAX,
                mxm_audio_file_decode::AtLimit::Refuse,
                mxm_audio_file_decode::Keep::AllUpTo(2),
            ),
        )
        .expect("the demo file parses");
        assert_eq!(read.channels, 1, "the demo wrote the wrong channel count");
        assert_eq!(
            read.sample_rate, 48_000,
            "the demo wrote the wrong sample rate"
        );
        assert_eq!(read.frames(), frames, "the demo dropped or invented frames");

        // The headroom law, asserted rather than assumed: a source at 1.6 comes back just under
        // full scale, not clipped to it and not left loud.
        let peak = mxm_measure::level::peak(&read.interleaved).expect("a finite file");
        assert!(
            (0.97..=0.985).contains(&peak),
            "the 0.98 headroom law did not run: peak {peak}"
        );
        std::fs::remove_file(&path).ok();
    }

    /// **The whole production path, `main` included.** This is what a writer test cannot otherwise
    /// reach: the render itself, the buffer `main` chooses, and the channel count and rate it hands
    /// over. An empty or truncated render fails here and nowhere else.
    ///
    /// `#[ignore]`d because it renders the demo in full, which is tens of seconds of audio; run it
    /// with `cargo test --all-targets -- --ignored` when the demo or its write path changes.
    #[test]
    #[ignore = "renders the whole demo; run with --ignored"]
    fn the_whole_demo_renders_and_writes_a_playable_file() {
        main();
        let read = mxm_audio_file_decode::decode_file(
            "mxm-mono-00-system-demo.wav",
            &mxm_audio_file_decode::Limits::new(
                usize::MAX,
                mxm_audio_file_decode::AtLimit::Refuse,
                mxm_audio_file_decode::Keep::AllUpTo(2),
            ),
        )
        .expect("the demo file parses");
        assert_eq!(read.channels, 1, "the demo wrote the wrong channel count");
        assert_eq!(
            read.sample_rate, 48000,
            "the demo wrote the wrong sample rate"
        );
        assert!(
            read.frames() > 48000,
            "the demo rendered under a second of audio"
        );
        let peak = mxm_measure::level::peak(&read.interleaved).expect("a finite render");
        assert!(peak > 0.1, "the demo rendered near-silence: peak {peak}");

        // `main` writes into the working directory, which under `cargo test` is the crate root.
        // Leaving it there drops an untracked WAV into the tree every time this runs.
        std::fs::remove_file("mxm-mono-00-system-demo.wav").ok();
    }
}
