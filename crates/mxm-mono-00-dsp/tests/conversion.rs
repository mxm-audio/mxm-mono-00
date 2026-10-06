//! What the modulation conversion did to the sound, pinned.
//!
//! `plans/plan-mxm-mono-00-modulation.md`. The plug-out's fifteen patch-bay rows became fifteen
//! routing targets, and its normalled connections became the init patch. **Every one of these
//! cases is a sound the machine could already make**, so the conversion had to leave them where
//! they were — and what "where they were" means is recorded here as a number rather than as a
//! claim.
//!
//! # The conversion is not bit-exact, and this is the measurement that says by how much
//!
//! Summing re-associates the arithmetic: where the voice computed `amount × (column ÷ unit) ×
//! reach` it now computes `Σ(amount × source) × scale`, which is the same value to within f32
//! rounding and not to the bit. Measured against the pre-conversion renders on 2026-09-14, over
//! ten patches of 19 200 samples each: the **worst absolute difference is 2.4e-7**, one to four
//! ulps, and the worst relative difference is **−121 dB against the case's own RMS**. Every peak
//! amplitude agrees to six significant figures.
//!
//! The digests below are therefore the *post*-conversion numbers, and they are a regression net
//! rather than a claim of equality with what came before. A change here means the voice moved;
//! whether it was allowed to is a question for whoever moved it.

use mxm_mono_00_dsp::lfo::Shape;
use mxm_mono_00_dsp::oscillator::Wave;
use mxm_mono_00_dsp::routing::{Routing, source, target};
use mxm_mono_00_dsp::voice::{NoteId, Patch, Voice};

/// One sound: the patch **and** the grid beside it, which is how the two travel — the routing is
/// not in `Patch`, because `Patch` is copied every sample.
struct Case {
    name: &'static str,
    patch: Patch,
    routing: Routing,
    release_at: Option<usize>,
    digest: u64,
    peak: f32,
}

/// FNV-1a over the samples' bits — the same ruler `spring_tanks.rs` uses.
fn digest(samples: &[f32]) -> u64 {
    let mut h: u64 = 0xcbf2_9ce4_8422_2325;
    for s in samples {
        h ^= u64::from(s.to_bits());
        h = h.wrapping_mul(0x0000_0100_0000_01b3);
    }
    h
}

fn render(case: &Case, samples: usize) -> Vec<f32> {
    let mut v = Voice::new();
    v.set_sample_rate(48_000.0);
    v.set_topology(&case.routing);
    let id = NoteId {
        voice_id: Some(1),
        channel: 0,
        note: 60,
    };
    v.note_on(id, 1.0, &case.patch);
    let mut out = Vec::with_capacity(samples);
    for i in 0..samples {
        if Some(i) == case.release_at {
            v.note_off(Some(1), 0, 60, &case.patch);
        }
        out.push(v.process(&case.patch, &case.routing));
    }
    out
}

fn case(name: &'static str, patch: Patch, routing: Routing, digest: u64, peak: f32) -> Case {
    Case {
        name,
        patch,
        routing,
        release_at: None,
        digest,
        peak,
    }
}

/// The ten patches, each exercising one of the plug-out's own normalled connections.
fn cases() -> Vec<Case> {
    let mut out = Vec::new();

    out.push(case(
        "init",
        Patch::default(),
        Routing::init(),
        0xae63_5c51_f83b_a151,
        0.618_057,
    ));

    out.push(case(
        "resonant",
        Patch {
            resonance: 0.8,
            cutoff_hz: 1200.0,
            ..Patch::default()
        },
        Routing::init(),
        0xda7c_b532_181d_d33a,
        1.491_423,
    ));

    // VCF ADSR IN ← Envelope 1, the normal, at depth.
    let mut r = Routing::init();
    r.set(target::CUTOFF, source::VCF_ADSR, 0.7);
    out.push(case(
        "filter envelope",
        Patch {
            cutoff_hz: 400.0,
            ..Patch::default()
        },
        r,
        0xe44c_4d5d_a423_baae,
        0.621_297,
    ));

    // VCF LFO IN ← LFO 1, the normal, at depth.
    let mut r = Routing::init();
    r.set(target::CUTOFF, source::LFO1, 0.5);
    out.push(case(
        "filter lfo",
        Patch {
            cutoff_hz: 2000.0,
            lfo1_rate_hz: 6.0,
            ..Patch::default()
        },
        r,
        0x2f3f_fdfc_aa5a_b5ed,
        0.463_052,
    ));

    // RING MOD IN ← VCO-1, the normal, and the ring modulator's own mixer level — which was the
    // mixer channel's route before the ring modulator got its slider back.
    out.push(case(
        "ring through the mixer",
        Patch {
            level_vco2: 0.6,
            level_ring: 0.7,
            ..Patch::default()
        },
        Routing::init(),
        0x9341_e9be_6aa9_bdc7,
        0.897_776,
    ));

    // VCO-1's modulator input ← VCO-2, the plug-out's cross-modulation. The pitch inputs reach
    // 144 semitones per unit where EXT CV reached 120, so the depth pinned at 0.15 is 0.125 now.
    let mut r = Routing::init();
    r.set(target::VCO1_PITCH, source::VCO2, 0.15 * 120.0 / 144.0);
    out.push(case(
        "cross modulation",
        Patch {
            level_vco2: 0.5,
            ..Patch::default()
        },
        r,
        // Re-pinned by the panel conversion (Rev 3): `0.125 × 144` is `0.15 × 120` re-associated,
        // so the digest moved and the peak did not, to six figures. Every other case here is
        // bit-identical across that conversion, the glide included.
        0x73e5_e011_3fd6_67b3,
        0.831_594,
    ));

    // The glide's own RC dip, which is a source rather than a fallback, on both pitch inputs — where
    // GLIDE IN and DESTINATION's default sent it — at the retired slider's 0.6.
    let mut r = Routing::init();
    let glide = -0.6 * 2.0 / 144.0;
    r.set(target::VCO1_PITCH, source::GLIDE, glide);
    r.set(target::VCO2_PITCH, source::GLIDE, glide);
    out.push(case(
        "glide",
        Patch::default(),
        r,
        0xf3b4_632b_9464_2393,
        0.617_101,
    ));

    // SYNC IN ← VCO-1 SYNC OUT at full reset depth: the retired `sync` switch, as a route.
    let mut r = Routing::init();
    r.set(target::VCO2_SYNC, source::VCO1_SYNC, 1.0);
    out.push(case(
        "sync",
        Patch {
            level_vco2: 0.8,
            coarse_semitones2: 7.0,
            ..Patch::default()
        },
        r,
        0x7428_c0bf_672d_0cda,
        0.800_712,
    ));

    // Both envelope gate inputs on the keyboard gate, released mid-render.
    let mut released = case(
        "release",
        Patch {
            vca_adsr: [0.01, 0.2, 0.5, 0.3],
            ..Patch::default()
        },
        Routing::init(),
        0x04d1_d5ba_6741_8689,
        0.610_386,
    );
    released.release_at = Some(9_600);
    out.push(released);

    // A source the plug-out did not normal there — what re-patching a row sounded like.
    let mut r = Routing::init();
    r.clear(target::CUTOFF, source::LFO1);
    r.set(target::CUTOFF, source::LFO2, 0.5);
    out.push(case(
        "lfo 2 patched to the filter",
        Patch {
            cutoff_hz: 2000.0,
            lfo2_rate_hz: 3.0,
            ..Patch::default()
        },
        r,
        0x08e3_7215_f172_255b,
        0.420_346,
    ));

    // ---- The panel's wiring, converted (Rev 3) ---------------------------------------------
    //
    // What DESTINATION, the VCA LFO, the PWM switches, KYBD CV and SAMPLE MODE did, each as the
    // route that replaced it, at the depth the retired control meant. Measured against renders of
    // the pre-conversion voice on 2026-09-22 over 42 patches: every non-pitch path bit-identical
    // or within −132 dB; the pitch paths within −99.7 dB, which is `amount × 144` against
    // `depth × 120` rounding differently and accumulating as phase — one ulp or better over each
    // case's first 500 samples; and the one named timing change, a GLIDE IN route from VCO-1 to
    // VCO-2 becoming same-sample, at −57 dB, and −141 dB with VCO-2's pitch read at the old stage.
    let mut r = Routing::init();
    r.amounts[target::VCO1_PITCH][source::LFO1] = 0.37 * 12.0 / 144.0;
    r.amounts[target::VCO2_PITCH][source::LFO1] = 0.37 * 12.0 / 144.0;
    out.push(case(
        "vibrato to both",
        Patch {
            level_vco2: 0.6,
            lfo1_shape: Shape::Saw,
            lfo1_rate_hz: 5.5,
            ..Patch::default()
        },
        r,
        0x317a_c13c_162d_40ee,
        0.840_579,
    ));

    let mut r = Routing::init();
    r.amounts[target::VCO1_PITCH][source::GLIDE] = 0.0;
    r.amounts[target::VCO2_PITCH][source::GLIDE] = -0.8 * 2.0 / 144.0;
    out.push(case(
        "glide to VCO-2 only",
        Patch {
            level_vco2: 0.6,
            coarse_semitones2: 7.0,
            ..Patch::default()
        },
        r,
        0x2fe2_f5ba_6014_0ed9,
        0.783_053,
    ));

    let mut r = Routing::init();
    r.set(target::TREMOLO, source::LFO1, 0.7);
    out.push(case(
        "tremolo",
        Patch {
            initial_gain: 0.6,
            lfo1_rate_hz: 6.0,
            ..Patch::default()
        },
        r,
        0x64ab_6559_32eb_c335,
        0.632_500,
    ));

    let mut r = Routing::init();
    r.set(target::VCO1_WIDTH, source::LFO1_CORE_TRIANGLE, 0.7);
    out.push(case(
        "pwm from the LFO 1 triangle",
        Patch {
            wave: Wave::Square,
            pulse_width: 0.3,
            ..Patch::default()
        },
        r,
        0x3a7a_32ab_d2a9_01c3,
        0.702_664,
    ));

    let mut r = Routing::init();
    r.set(target::VCO2_WIDTH, source::VCF_ADSR, 0.5);
    out.push(case(
        "pwm from envelope 1",
        Patch {
            wave2: Wave::Square,
            level_vco2: 0.6,
            ..Patch::default()
        },
        r,
        0xc171_42ad_1377_7012,
        0.850_124,
    ));

    let mut r = Routing::init();
    r.set(target::SH_INPUT, source::LFO1_CORE_REVERSE_SAW, 1.0);
    r.set(target::VCO1_PITCH, source::SH_OUT, 0.1 * 120.0 / 144.0);
    out.push(case(
        "sample and hold on the reverse saw",
        Patch {
            sh_rate_hz: 9.0,
            lfo1_rate_hz: 1.3,
            ..Patch::default()
        },
        r,
        0x0fd1_f7c0_7b1c_a55f,
        0.618_057,
    ));

    let mut r = Routing::init();
    r.set(target::CUTOFF, source::KEY, -1.0);
    out.push(case(
        "negative key follow",
        Patch {
            cutoff_hz: 900.0,
            resonance: 0.5,
            ..Patch::default()
        },
        r,
        0x0629_ec59_780f_4ee0,
        1.159_083,
    ));

    out
}

#[test]
fn every_converted_patch_renders_the_sound_it_was_pinned_at() {
    for c in cases() {
        let audio = render(&c, 19_200);
        let got = digest(&audio);
        let got_peak = audio.iter().fold(0.0f32, |a, b| a.max(b.abs()));
        assert!(
            (got_peak - c.peak).abs() < 5e-6,
            "{}: peak {got_peak:.6}, pinned {:.6}",
            c.name,
            c.peak
        );
        assert_eq!(
            got, c.digest,
            "{}: digest {got:016x}, pinned {:016x}",
            c.name, c.digest
        );
    }
}

/// **The init patch is the plug-out's wiring**, and that is what makes the conversion a conversion
/// rather than a rewrite: nothing in the voice tests for *is this row patched* any more.
#[test]
fn a_fresh_patch_carries_the_plug_outs_normalled_connections_and_nothing_else() {
    use mxm_mono_00_dsp::routing::{SOURCES, TARGETS};
    let r = Routing::init();
    // The thirteen the machine itself wires — nine on the patch bay, less GLIDE IN and less the
    // ring modulator's mixer channel, which is a slider again, and six on the panel — and no
    // fourteenth.
    let wired: Vec<(usize, usize)> = (0..TARGETS)
        .flat_map(|t| (0..SOURCES).map(move |s| (t, s)))
        .filter(|&(t, s)| r.present[t][s])
        .collect();
    assert_eq!(wired.len(), 13, "{wired:?}");
    // Nothing a performance gesture reaches is wired, so a fresh instance is still the copy.
    for s in [
        source::VELOCITY,
        source::WHEEL,
        source::PRESSURE,
        source::BEND,
    ] {
        for t in 0..TARGETS {
            assert!(!r.present[t][s], "a performance input starts routed");
        }
    }
}

/// **Sync's reset depth is the hardware's at full, and a control below it.**
///
/// The amount had to mean *something* — a sync source cannot be scaled, because `EdgeRow`'s
/// interpolated fraction is scale-invariant — and reset depth is what it means. Two things have to
/// hold for that to be a conversion rather than a change: full depth has to be the machine's own
/// reset **to the bit**, and below it has to be continuously different.
///
/// **Falsified before it was trusted**: dropping the `depth` argument from the slave's strong arm
/// — which is the pre-conversion code — turns the *second* assertion red, because a zero-depth
/// route then still resets. The two forms `p × (1 − d)` and `p − p × d` are both exact at `d = 1`,
/// so the first assertion does not separate them and does not claim to; what it pins is that
/// whichever form is in use leaves no residual phase, which is what a hard reset means.
///
/// The locked threshold is `1e-3` rather than zero because the band-limited step's residual
/// correction is not itself periodic at the master's rate: the measured drift is 1.6e-4 against a
/// ±1 waveform, or −76 dB, where an unsynced slave drifts by more than 1.
#[test]
fn a_full_depth_sync_route_is_a_hard_reset_and_a_partial_one_is_not() {
    use mxm_mono_00_dsp::oscillator::{SyncStrength, Vco, Wave};

    // The arithmetic the bit-exactness rests on: at depth one the core lands on exact zero for
    // every phase it could be at, with no residual to accumulate.
    for i in 0..1000 {
        let phase = i as f32 / 1000.0;
        assert_eq!(phase - phase * 1.0, 0.0, "phase {phase} left a residual");
    }

    // And the audible half: at full depth the slave is *periodic at the master's period*, which
    // is what a hard reset means and what no partial pull produces.
    let fs = 48_000.0;
    // 240 Hz is exactly 200 samples at 48 kHz, so the period is an integer and the comparison
    // needs no interpolation.
    let master_hz = 240.0;
    let master_period = 200usize;
    let render = |depth: f32| -> Vec<f32> {
        let mut master = Vco::new();
        let mut slave = Vco::new();
        let mut out = Vec::new();
        for _ in 0..4_000 {
            master.process(master_hz, Wave::Saw, 0.5, fs);
            let edge = master.sync_edge();
            out.push(slave.process_slave(
                337.0,
                Wave::Saw,
                0.5,
                fs,
                edge,
                SyncStrength::Strong,
                depth,
            ));
        }
        out
    };
    // How far the render is from repeating at the master's period, over the settled tail.
    let drift = |a: &[f32]| -> f32 {
        a[1_000..a.len() - master_period]
            .iter()
            .zip(a[1_000 + master_period..].iter())
            .fold(0.0f32, |worst, (x, y)| worst.max((x - y).abs()))
    };
    let locked = render(1.0);
    let free = render(0.0);
    let half = render(0.5);
    assert!(
        drift(&locked) < 1e-3,
        "full depth did not lock to the master's period: worst drift {}",
        drift(&locked)
    );
    assert!(
        drift(&free) > 0.5,
        "zero depth reset the core anyway: worst drift {}",
        drift(&free)
    );
    assert_ne!(locked, half, "half depth is the same as a hard reset");
    assert_ne!(free, half, "half depth is the same as no reset at all");
}

// -------------------------------------------------------------------------------------------
// Three defects the conversion introduced and a review found. Each test was falsified against
// the code that had them.
// -------------------------------------------------------------------------------------------

/// **A negative sync depth is no reset, and it must not mask a positive one.**
///
/// `process_slave` clamps the pull to `0…1`, because a reset has no inverse. Choosing the winning
/// route by *magnitude* therefore let a route at `-1.0` beat one at `+1.0` and produce no sync at
/// all, where the player had asked for a full one.
///
/// **Falsified**: with `depth.abs() > best.abs()` the second assertion goes red — the inverted
/// route wins and resets nothing.
#[test]
fn an_inverted_sync_route_resets_nothing_and_does_not_mask_a_positive_one() {
    let p = Patch {
        level_vco1: 0.0,
        level_vco2: 0.8,
        coarse_semitones2: 7.0,
        ..Patch::default()
    };
    let render = |r: &Routing| {
        let mut v = Voice::new();
        v.set_sample_rate(48_000.0);
        v.set_topology(r);
        v.note_on(
            NoteId {
                voice_id: Some(1),
                channel: 0,
                note: 45,
            },
            1.0,
            &p,
        );
        (0..8_000).map(|_| v.process(&p, r)).collect::<Vec<f32>>()
    };

    let free = Routing::init();
    assert!(!free.present[target::VCO2_SYNC][source::VCO1_SYNC]);
    let mut inverted = Routing::init();
    inverted.set(target::VCO2_SYNC, source::VCO1_SYNC, -1.0);
    assert_eq!(
        render(&inverted),
        render(&free),
        "an inverted sync route reset something"
    );

    // And a full route beside it still syncs, however deep the inverted one is.
    let mut both = Routing::init();
    both.set(target::VCO2_SYNC, source::VCO1_SYNC, 1.0);
    both.set(target::VCO2_SYNC, source::VCO2_SYNC, -1.0);
    assert_ne!(
        render(&both),
        render(&free),
        "a full sync route was masked by an inverted one"
    );
}

/// **A gate route at zero depth carries nothing, so a key press through it is not a press.**
///
/// The keyboard gate reaches an envelope as an ordinary route now, and a tie under low-note
/// priority retriggers a GATE+TRIG envelope because *the keyboard* reaches it. Reading that as
/// "the pair is present" rather than "the pair carries something" made a zero-depth route
/// retrigger on every note-on while its gate never crossed the threshold — a press with no gate.
///
/// **Falsified**: with `r.present[t][GATE]` alone the last assertion goes red.
#[test]
fn a_gate_route_at_zero_depth_does_not_retrigger_a_gate_trig_envelope() {
    use mxm_mono_00_dsp::envelope::Stage;
    use mxm_mono_00_dsp::voice::Trigger;

    let p = Patch {
        vca_trigger: Trigger::GateTrig,
        vca_adsr: [0.05, 0.3, 0.8, 0.2],
        ..Patch::default()
    };
    let play = |r: &Routing, tie: bool| -> Stage {
        let mut v = Voice::new();
        v.set_sample_rate(48_000.0);
        v.set_topology(r);
        v.note_on(
            NoteId {
                voice_id: Some(1),
                channel: 0,
                note: 48,
            },
            1.0,
            &p,
        );
        for _ in 0..48_000 {
            v.process(&p, r);
        }
        if tie {
            // A higher key under low-note priority: a tie, which raises no gate edge.
            v.note_on(
                NoteId {
                    voice_id: Some(2),
                    channel: 0,
                    note: 60,
                },
                1.0,
                &p,
            );
            for _ in 0..200 {
                v.process(&p, r);
            }
        }
        v.vca_env_stage()
    };

    // The premise: wired at full, a tie retriggers GATE+TRIG.
    let wired = Routing::init();
    assert_eq!(play(&wired, false), Stage::Sustain, "the premise");
    assert_eq!(
        play(&wired, true),
        Stage::Attack,
        "a tie retriggers GATE+TRIG through the keyboard gate"
    );

    // At zero depth the route carries nothing, so there is no gate and no press.
    let mut silent = Routing::init();
    silent.amounts[target::VCA_GATE][source::GATE] = 0.0;
    assert_ne!(
        play(&silent, true),
        Stage::Attack,
        "a zero-depth gate route retriggered on a tie"
    );
}
