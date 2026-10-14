//! DSP for mxm-mono-00: a System-100 as its plug-out pictures it, sounding as the machine did.
//!
//! Deliberately free of any plugin-framework types: everything here takes plain values and a
//! sample rate, so the whole instrument is testable with `cargo test` and no host involved.
//!
//! **The fourth honest copy.** What this crate copied from its siblings and proved identical is the
//! kit's now: `flush`, `tanh_approx`, `tan_approx`, the PolyBLEP residual and the pulse clamp are
//! `mxm-dsp`'s, the noise generator `mxm-rng`'s. Its ADSR stays its own (the machine's 0.4 ms floor
//! is the one difference), as do the diode ladder and everything else that is the machine's. What it
//! adds is the machine's: a triangle by switched
//! inversion, coloured noise, an LFO whose shapes sit at three DC levels, a VCA whose tremolo can
//! only dip, a portamento that is a hold capacitor, and a glide that is one RC on a gate edge.
//!
//! The research every module cites is `research:instruments/system-100.md`; the design is the plan
//! until the crate's own `AGENTS.md` replaces it.

#[cfg(any(test, feature = "conformance"))]
pub mod conformance;
pub mod effects;
pub mod envelope;
pub mod filter;
pub mod hpf;
pub mod lfo;
pub mod matrix;
pub mod noise;
pub mod oscillator;
pub mod oversample;
pub mod ring;
pub mod routing;
pub mod sh;
pub mod vca;
pub mod voice;

/// The lowest host rate the plugin activates at; a non-finite rate is refused with it.
///
/// `f32::clamp` panics when its lower bound is above its upper one or either is NaN, and the
/// corners here are clamped to a fixed floor under 0.45 of the rate they run at: the HPF's 10 Hz
/// and the oversampled ladder's 20 Hz cross below 22.2 Hz, the effects' 20 Hz below 44.4 Hz. 1 kHz
/// is far clear of that, and no higher than the lowest rate clap-validator (1234.57 Hz) or the
/// player's robustness sweeps (1 kHz) ask for.
pub const MIN_SAMPLE_RATE: f32 = 1_000.0;
