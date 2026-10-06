//! mxm-mono-00 on rendered audio, through the player's own hosting path.
//!
//! What no unit test in either crate can see: that the shipped `.clap` loads, that a note played
//! through the player comes out at its pitch, that a parameter written by the host reaches the
//! sound, that **a routing pair is a parameter a host can re-patch**, and that a patch which runs
//! itself keeps sounding with no key down — the `KeepAlive` path, which the collection has never
//! exercised before because no other instrument has one.
//!
//! Skips, with the reason, when the bundle is not built: run
//! `cargo xtask bundle mxm-mono-00 --release` first.

use mxm_player_harness::app_harness;

use mxm_player::events::input::Payload;
use mxm_player::session::{FRAMES_PER_BLOCK, Session};
use std::path::PathBuf;

const PLUGIN: &str = "dk.mxm.mxm-mono-00";
const SAMPLE_RATE: f64 = 48_000.0;
const SKIP: &str = "skipping: run `cargo xtask bundle mxm-mono-00 --release`";

fn bundle() -> Option<(PathBuf, PathBuf)> {
    let dir = app_harness::bundled_dir_with("mxm-mono-00")?;
    let file = dir.join("mxm-mono-00.clap");
    file.exists().then_some((dir, file))
}

fn session(name: &str) -> Option<Session> {
    let (dir, file) = bundle()?;
    let mut s = Session::scratch(name, vec![dir]);
    s.load(&file, PLUGIN);
    Some(s)
}

// --- measuring ---------------------------------------------------------------------------------

use mxm_measure::channels::left;

/// Peak magnitude of a capture.
///
/// **A shim over `mxm-measure`, and the `expect` is the point.** The shared ruler reports absence for
/// a **non-finite** buffer rather than the largest number in it, because `f32::max` would otherwise
/// let a render that is half NaN measure as perfectly healthy — and then pass every "is it quiet?"
/// assertion below. Panicking here is the loud failure that behaviour deserves.
fn peak(samples: &[f32]) -> f32 {
    mxm_measure::level::peak(samples).expect("the capture is finite")
}

/// How much of one frequency is in a captured window — **a relative figure, not an amplitude.**
///
/// Two reasons it is relative, and both matter to anyone quoting a number from these tests:
///
/// - **The capture length is the session's, not ours.** `component_amplitude` reads a component's
///   true amplitude only over a whole number of cycles; these windows are whole blocks, so the
///   reading carries spectral leakage. Comparing one pitch against another in the same window is
///   sound — the leakage is common to both — and calling the result an absolute amplitude is not.
/// - **The absolute value moved by 6 dB with the migration to the shared probe**, which is a
///   correction rather than a regression: every local copy of this helper computed `|X|/N`, half a
///   component's amplitude, and the shared probe reports the amplitude. A figure quoted from an
///   older run of these tests is 6 dB low.
///
/// **Absence panics rather than reading as zero.** The probe declines for two reasons — an empty
/// window, which cannot happen here, and a **non-finite render**, which can. Folding that into `0.0`
/// would let a NaN-producing plugin sail through every "quieter than" and "silent" assertion below,
/// which is the precise failure the shared crate's result-form contract exists to prevent.
fn magnitude_at(samples: &[f32], hz: f64) -> f64 {
    mxm_measure::spectrum::component_amplitude(samples, hz, SAMPLE_RATE)
        .expect("the capture is non-empty and finite")
}

use mxm_measure::convert::note_hz;

/// A crude high-frequency measure: mean absolute sample-to-sample difference. Relative only.
fn brightness(samples: &[f32]) -> f32 {
    if samples.len() < 2 {
        return 0.0;
    }
    samples.windows(2).map(|w| (w[1] - w[0]).abs()).sum::<f32>() / (samples.len() - 1) as f32
}

// --- driving -----------------------------------------------------------------------------------

/// Sets a parameter by display name, as a position in its normalised range, and lets it settle.
fn set_param(session: &mut Session, name: &str, fraction: f64) -> String {
    let param = session
        .state()
        .param(name)
        .unwrap_or_else(|| panic!("`{name}` is not a parameter"))
        .clone();
    let value = param.min + fraction * (param.max - param.min);
    session
        .app()
        .engine_mut()
        .push_gui_event(Payload::ParamValue {
            param_id: param.id,
            value,
        });
    session.advance_blocks(4).expect("the session advances");
    session
        .state()
        .param(name)
        .map(|p| p.text.clone())
        .unwrap_or_default()
}

/// Holds `note`, renders `blocks`, releases it, and returns the left channel of the held part
/// after the attack.
fn note(session: &mut Session, note: u8, blocks: u64) -> Vec<f32> {
    session.clear_capture();
    session.app().note_on(note, 100.0 / 127.0);
    session.advance_blocks(blocks).expect("advances");
    let audio = session.captured();
    session.app().note_off(note);
    session.advance_blocks(80).expect("advances");
    let skip = (FRAMES_PER_BLOCK * 2 * 3).min(audio.len());
    left(&audio[skip..])
}

// --- the tests ---------------------------------------------------------------------------------

#[test]
fn at_rest_it_is_exactly_silent() {
    let Some(mut s) = session("mono00-rest") else {
        eprintln!("{SKIP}");
        return;
    };
    s.advance_blocks(20).expect("advances");
    assert_eq!(
        peak(&s.captured()),
        0.0,
        "an idle synth must render exact zeros, not merely something quiet"
    );
}

#[test]
fn a_note_sounds_at_its_pitch_and_the_release_ends_in_exact_silence() {
    let Some(mut s) = session("mono00-note") else {
        eprintln!("{SKIP}");
        return;
    };
    let held = note(&mut s, 57, 40);
    assert!(
        peak(&held) > 0.05,
        "the note is audible: peak {}",
        peak(&held)
    );

    // The init patch is one sawtooth: its fundamental stands above the neighbouring semitones.
    let hz = note_hz(57.0);
    let at = magnitude_at(&held, hz);
    let below = magnitude_at(&held, hz / 2f64.powf(1.0 / 12.0));
    let above = magnitude_at(&held, hz * 2f64.powf(1.0 / 12.0));
    assert!(
        at > 2.0 * below && at > 2.0 * above,
        "A3 should dominate its neighbours: {at:.4} against {below:.4} and {above:.4}"
    );

    // After the release and the settle, exact zeros: the activity verdict said inert.
    s.clear_capture();
    s.advance_blocks(40).expect("advances");
    assert_eq!(
        peak(&s.captured()),
        0.0,
        "the tail must end in exact silence"
    );
}

#[test]
fn the_cutoff_written_by_the_host_darkens_the_note() {
    let Some(mut s) = session("mono00-cutoff") else {
        eprintln!("{SKIP}");
        return;
    };
    let open = note(&mut s, 45, 40);
    let text = set_param(&mut s, "Cutoff", 0.25);
    let closed = note(&mut s, 45, 40);
    assert!(
        brightness(&closed) < 0.5 * brightness(&open),
        "closing the filter (now {text}) should darken the note: {} against {}",
        brightness(&closed),
        brightness(&open)
    );
}

/// The owner's finding on the first play: from the init patch, resonance at its top produced no
/// self-oscillation, because the discrete ladder's threshold rises with the cutoff and the init
/// cutoff is 10 kHz. The correction holds the slider's singing point still; this is that, from
/// the init patch exactly as a person meets it.
#[test]
fn the_filter_sings_at_full_resonance_from_the_init_patch() {
    let Some(mut s) = session("mono00-sing") else {
        eprintln!("{SKIP}");
        return;
    };
    set_param(&mut s, "Oscillator 1 level", 0.0);
    let quiet = note(&mut s, 48, 40);
    // Not exactly zero: the level smoother's ramp leaves a residue in the ladder for a moment.
    assert!(
        peak(&quiet) < 0.01,
        "nothing feeds the filter: peak {}",
        peak(&quiet)
    );

    set_param(&mut s, "Resonance", 1.0);
    // Three seconds held: the oscillation grows from a −120 dB excitation.
    let sung = note(&mut s, 48, 3 * 94);
    let last = &sung[sung.len() - 48_000..];
    assert!(
        peak(last) > 0.05,
        "at the init cutoff the filter should sing on its own: peak {}",
        peak(last)
    );
}

#[test]
fn a_routing_pair_is_a_parameter_and_re_patching_it_changes_the_sound() {
    let Some(mut s) = session("mono00-row") else {
        eprintln!("{SKIP}");
        return;
    };
    // The mixer's External input is wired to nothing at Init; the ring modulator has a level of its
    // own. Patching the ring modulator into External input gives a tone, and adding the noise
    // beside it carries broadband noise, which no filter setting could be mistaken for — and **the
    // two add**, which is the switching jack having become a mixer (`plan-modulation-routing.md`
    // decision 1.6).
    set_param(&mut s, "Oscillator 1 level", 0.0);
    let text = set_param(&mut s, "External input from Ring modulator on", 1.0);
    assert_eq!(text, "On", "the presence reads back");
    set_param(&mut s, "External input from Ring modulator", 1.0);
    let ring = note(&mut s, 45, 40);
    assert!(
        peak(&ring) > 0.05,
        "the ring modulator is audible: {}",
        peak(&ring)
    );

    // A presence is a host parameter like any other: the noise joins the same input.
    set_param(&mut s, "External input from Noise on", 1.0);
    set_param(&mut s, "External input from Noise", 1.0);
    let noise = note(&mut s, 45, 40);
    assert!(
        brightness(&noise) > 2.0 * brightness(&ring),
        "noise summed onto the mixer's external input should be far brighter than the ring modulator alone: {} against {}",
        brightness(&noise),
        brightness(&ring)
    );

    // And removing it is one write that leaves the depth behind, so the input is the ring again.
    set_param(&mut s, "External input from Noise on", 0.0);
    let back = note(&mut s, 45, 40);
    assert!(
        brightness(&back) < 0.5 * brightness(&noise),
        "removing the noise route left it sounding: {} against {}",
        brightness(&back),
        brightness(&noise)
    );
}

#[test]
fn a_self_running_patch_keeps_sounding_with_no_key_down() {
    let Some(mut s) = session("mono00-live") else {
        eprintln!("{SKIP}");
        return;
    };
    // The plan's slow-clock case: the S&H clock into the VCA envelope's gate. Every rising edge is
    // a press, so the patch plays itself — and the host must keep calling it.
    set_param(&mut s, "S&H rate", 0.5);
    // The keyboard gate off this envelope, the S&H clock on it: two writes, because a gate input
    // sums and a plug that replaces another is two gestures rather than one dropdown.
    set_param(&mut s, "Amplifier envelope gate from Gate on", 0.0);
    let text = set_param(&mut s, "Amplifier envelope gate from S&H clock on", 1.0);
    assert_eq!(text, "On", "the presence reads back");
    set_param(&mut s, "Amplifier envelope gate from S&H clock", 1.0);

    // A key was pressed once and released; what follows is the clock's doing.
    s.app().note_on(48, 100.0 / 127.0);
    s.advance_blocks(10).expect("advances");
    s.app().note_off(48);
    s.advance_blocks(60).expect("advances");

    s.clear_capture();
    s.advance_blocks(200).expect("advances");
    assert!(
        peak(&s.captured()) > 0.05,
        "the patch should keep sounding with no key down: peak {}",
        peak(&s.captured())
    );

    // Patching the input back to the keyboard gate silences it again, after the tail.
    set_param(&mut s, "Amplifier envelope gate from S&H clock on", 0.0);
    set_param(&mut s, "Amplifier envelope gate from Gate on", 1.0);
    s.advance_blocks(120).expect("advances");
    s.clear_capture();
    s.advance_blocks(40).expect("advances");
    assert_eq!(
        peak(&s.captured()),
        0.0,
        "back on the keyboard gate, with no key down, it is exactly silent"
    );
}

#[test]
fn the_effects_lengthen_the_tail_and_still_end_in_exact_silence() {
    let Some(mut s) = session("mono00-fx") else {
        eprintln!("{SKIP}");
        return;
    };
    set_param(&mut s, "Reverb", 0.8);
    set_param(&mut s, "Envelope 2 release", 0.0);
    let _ = note(&mut s, 60, 20);

    // Well inside the spring's decay the tail is audible...
    s.clear_capture();
    s.advance_blocks(40).expect("advances");
    assert!(
        peak(&s.captured()) > 0.0,
        "the spring should still be ringing a fraction of a second after the note"
    );

    // ...and after it, exact zeros: the quiet snap.
    s.advance_blocks(48_000 * 6 / FRAMES_PER_BLOCK as u64)
        .expect("advances");
    s.clear_capture();
    s.advance_blocks(40).expect("advances");
    assert_eq!(
        peak(&s.captured()),
        0.0,
        "the reverb tail must end in exact silence"
    );
}
