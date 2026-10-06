//! **The instrument's tank, pinned.**
//!
//! `SpringTankModel` was added so the standalone effect (the mxm-folded-spring repository's
//! `plugins/mxm-folded-spring`) can string a different tank. The collection's rule for that is in
//! this crate's `NOTES.md` (*The spring has three tanks*): an instrument's DSP crate may gain
//! inputs, never a behaviour change. This is that rule as arithmetic.
//!
//! The digest below was taken **on the revision before the models existed**, with a file that
//! compiled against both, and it did not move when they landed. So `mxm-mono-00` renders exactly
//! what it rendered: same numbers, same order, same rounding.
//!
//! A failure here is not a licence to update the number. It means the instrument's reverb changed,
//! which is the thing this file exists to forbid — find out why first.

use mxm_mono_00_dsp::effects::{SpringReverb, SpringTankModel};

const FS: f32 = 48_000.0;

/// The render below, as it stood on 2026-09-04, before tank models were added.
const MEDIUM_DIGEST: u64 = 0xa9c7_74ad_77ed_32d9;

fn digest(samples: &[f32]) -> u64 {
    let mut h: u64 = 0xcbf2_9ce4_8422_2325;
    for s in samples {
        for byte in s.to_bits().to_le_bytes() {
            h ^= u64::from(byte);
            h = h.wrapping_mul(0x0000_0100_0000_01b3);
        }
    }
    h
}

/// An impulse, then a burst, then silence: the band-pass, all three springs, the direct coupling,
/// the feedback and the snap to exact zero.
fn render(reverb: &mut SpringReverb) -> Vec<f32> {
    render_for(reverb, 48_000)
}

fn render_for(reverb: &mut SpringReverb, frames: usize) -> Vec<f32> {
    let mut out = Vec::with_capacity(frames);
    for i in 0..frames {
        let x = if i == 0 {
            1.0
        } else if i < 4_800 {
            (i as f32 * 0.01).sin() * 0.3
        } else {
            0.0
        };
        out.push(reverb.process(x, 0.6, FS));
    }
    out
}

#[test]
fn the_medium_tank_is_the_tank_the_instrument_had() {
    let mut reverb = SpringReverb::new();
    reverb.set_sample_rate(FS);
    assert_eq!(
        reverb.model(),
        SpringTankModel::Medium,
        "a fresh reverb is the measured tank, which is the only one the instrument can have"
    );
    let rendered = digest(&render(&mut reverb));
    // Windows' bits: each platform's maths library rounds in its own way (the owner, 2026-10-06:
    // pin on Windows only).
    if cfg!(target_os = "windows") {
        assert_eq!(
            rendered, MEDIUM_DIGEST,
            "the instrument's reverb has changed; adding a tank must never do that"
        );
    }
}

#[test]
fn a_tank_selected_back_is_the_same_tank() {
    // Strung to another tank and back, the reverb renders what it renders fresh: `set_model`
    // empties the lines, so nothing of the other tank survives into this one.
    let mut reverb = SpringReverb::new();
    reverb.set_sample_rate(FS);
    reverb.set_model(SpringTankModel::Long);
    let _ = render(&mut reverb);
    reverb.set_model(SpringTankModel::Medium);
    // Against a fresh medium tank on this machine, not the pin: the property holds bit for bit on
    // every platform, while the pinned bits are Windows'.
    let mut fresh = SpringReverb::new();
    fresh.set_sample_rate(FS);
    assert_eq!(
        digest(&render(&mut reverb)),
        digest(&render(&mut fresh)),
        "a tank carries nothing over from the one before it"
    );
}

#[test]
fn each_tank_is_a_different_reverb() {
    // Three names for one sound would be three ways of saying nothing.
    let mut digests = Vec::new();
    for model in SpringTankModel::ALL {
        let mut reverb = SpringReverb::new();
        reverb.set_sample_rate(FS);
        reverb.set_model(model);
        digests.push((model, digest(&render(&mut reverb))));
    }
    for (index, (model, d)) in digests.iter().enumerate() {
        for (other, e) in &digests[index + 1..] {
            assert_ne!(
                d,
                e,
                "{} and {} render identically",
                model.label(),
                other.label()
            );
        }
    }
}

#[test]
fn a_shorter_tank_rings_shorter_and_a_longer_one_rings_longer() {
    // What the names claim, measured: how much is still ringing a second and a half after the
    // input stopped. Not "when does it reach exact zero" — every tank snaps to zero eventually and
    // a one-second window catches none of them doing it, which is how the first version of this
    // test passed three times with the same number.
    let mut left = Vec::new();
    for model in SpringTankModel::ALL {
        let mut reverb = SpringReverb::new();
        reverb.set_sample_rate(FS);
        reverb.set_model(model);
        let out = render_for(&mut reverb, 3 * FS as usize);
        let window = &out[(1.6 * FS) as usize..(1.9 * FS) as usize];
        let rms = (window.iter().map(|s| s * s).sum::<f32>() / window.len() as f32).sqrt();
        left.push((model, rms));
    }
    for pair in left.windows(2) {
        let ((short, a), (long, b)) = (pair[0], pair[1]);
        assert!(
            a < b,
            "{} still has {a} ringing and {} has {b}; the tanks are listed shortest first",
            short.label(),
            long.label()
        );
    }
}
