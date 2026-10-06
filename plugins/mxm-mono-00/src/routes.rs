//! mxm-mono-00's routing parameters: one presence and one amount per *(target, source)* pair.
//!
//! `plans/plan-mxm-mono-00-modulation.md`. The derive needs concrete fields and this instrument's
//! source list is its own, so the struct is declared here rather than generated — the shape
//! `mxm-mono-08` established for its 120 routing ids and `mxm-mono-pr1` follows for its 289. What
//! is shared is everything around these fields: [`mxm_modulation_params`] reads them, and
//! [`mxm_modulation`] evaluates them.
//!
//! # Permanent ids
//!
//! One `#[nested(id_prefix = …)]` per target, so a pair's id is `<target>_<source>` and
//! `<target>_<source>on`. Mechanically derived, never typed. **Permanent from here on.**
//!
//! # What retired into this file
//!
//! **Twenty-three ids**, and every one of them named something that stopped existing:
//!
//! - **The fifteen row selectors** — `lfo1cvin` … `phasermanualin`. A row chose one source; a
//!   target has one slot per source and there is nothing to choose. `plan-modulation-routing.md`
//!   §5.0 is why re-lettering an enum is never an option: a stored value is a fraction of the step
//!   count, so adding or removing a step silently re-points every patch that holds one.
//! - **The seven row attenuators** — `lfo1gain`, `lfo2gain`, `extcv1`, `ringlevel`, `filterenv`,
//!   `filterlfo` and `vcaenv`. Each was *one depth shared by whatever was on that row*, which is
//!   exactly what decision 1.6 replaces with a depth per route: the System-100's every-CV-input-
//!   has-an-attenuator design is what becomes a mixer. They could not be re-used in place either,
//!   for the reason §5.1 gives — a live id's range cannot be widened from `0…1` to `-1…1` without
//!   re-pointing every stored value, because both cover the whole normalised input.
//! - **`sync`**. Whether VCO-2 is synced is now whether anything is routed to its sync input, so
//!   the switch had no second authority to be. `syncstrength` stays: strong or weak is the
//!   hardware's own switch and a different question (wart 17).
//!
//! Decision 1.13 permits every one of them, because the sound stays reachable: the reachability
//! argument is in `plugins/mxm-mono-00/NOTES.md` and the renders behind it are
//! `crates/mxm-mono-00-dsp/tests/conversion.rs`.
//!
//! **Rev 3 retired more, for the same two reasons** (`plans/plan-mxm-mono-00-modulation.md`): the
//! panel's own wiring became routing, so a source is chosen on the card it moves and nowhere else.
//!
//! - **Eleven panel controls** — `glide`, `glidedest`, `vcolfo`, `vcolfodest`, `pwmsrc1`, `pwm1`,
//!   `pwmsrc2`, `pwm2`, `vcalfo`, `keytrack` and `shmode`. DESTINATION was a destination chosen at
//!   the source; the others were a depth wired to one source or a source chosen by a switch. Each
//!   is a route now, on a target of its own: both oscillators' pitch and pulse width, the tremolo
//!   and the cutoff, and the S&H input taking LFO 1's core shapes.
//! - **Five routing targets' forty ids each** — `mod_glidein_*`, `mod_vco1cv_*`, `mod_shin_*`,
//!   `mod_vcfenv_*` and `mod_vcflfo_*`. The last two were the Filter's VCF ADSR IN and LFO IN jacks,
//!   which became one Cutoff with each source at the reach its own jack gave it.
//!   GLIDE IN is a direct pitch route now, its exact equivalent. VCO-1's pitch input took a fader
//!   taper and a reach of 144 semitones, and the S&H input's presence turns the sampler on where it
//!   used to feed only the EXT position: a changed meaning under a kept id is what this file's
//!   first retirement forbade, so both were re-declared as `mod_vco1pitch_*` and `mod_shsrc_*`.
//!
//! **What no migration can carry is a host automation lane** (§5.1). A project that *set* one of
//! these keeps every other parameter and loses its routing; one that *automated* one loses that
//! lane, and no state rewrite can rebind it.
//!
//! # The collection's standard retired thirty-four more
//!
//! `plans/plan-modulation-standard.md`: a pair from a performance source the machine never had is
//! **not minted** where it can never mean anything — Velocity, the wheel, pressure and the lever
//! into either envelope gate, the sync input or the mixer's audio input, and Velocity into the S&H
//! (`mxm_mono_00_dsp::routing::offer`). Seventeen pairs, thirty-four ids, never to be re-used; a
//! session or preset naming one loads everything else. So `TargetRoutes` writes its `Params` impl
//! by hand, the derive's ids in the derive's order with those pairs left out. The machine's CV
//! amplifier reads as **VCA level** (its `mod_vcacv_*` ids unchanged), and the standard
//! **Amplitude** after the VCA is new (`mod_amplitude_*`). Readings are the shared
//! `mxm_modulation_params::reading`: Key per octave everywhere, the phaser in octaves.

use mxm_modulation::standard::Offer;
use mxm_modulation_params::Route;
use mxm_modulation_params::reading::{self, Fader, Reach};
use mxm_mono_00_dsp::routing::{
    INIT_AT_FULL, INIT_PRESENT, KEY_UNIT_SEMITONES, Routing, SOURCE_NAMES, SOURCES, SYNC_NORMAL,
    TARGET_NAMES, TARGETS, offer, scale, takes_standard_reach, target,
};
use nice_plug::prelude::*;

/// Every routing pair's two permanent ids, `(amount, presence)`, in `[target][source]` order.
///
/// **Written out rather than derived at runtime**, because a preset's parameter list is
/// `&'static str` and because these are permanent ids: they belong in the source where they can be
/// read, grepped and diffed. They are still *produced* by the `#[nested(id_prefix = …)]` groups
/// below — this table only names what the derive emits, and
/// `tests::the_id_table_is_what_the_derive_actually_produces` is what holds the two together.
pub const ROUTE_IDS: [[(&str, &str); SOURCES]; TARGETS] = [
    [
        ("mod_lfo1rate_key", "mod_lfo1rate_keyon"),
        ("mod_lfo1rate_lfo1", "mod_lfo1rate_lfo1on"),
        ("mod_lfo1rate_lfo2", "mod_lfo1rate_lfo2on"),
        ("mod_lfo1rate_sh", "mod_lfo1rate_shon"),
        ("mod_lfo1rate_shclk", "mod_lfo1rate_shclkon"),
        ("mod_lfo1rate_vco1", "mod_lfo1rate_vco1on"),
        ("mod_lfo1rate_vco2", "mod_lfo1rate_vco2on"),
        ("mod_lfo1rate_vco1sy", "mod_lfo1rate_vco1syon"),
        ("mod_lfo1rate_vco2sy", "mod_lfo1rate_vco2syon"),
        ("mod_lfo1rate_ring", "mod_lfo1rate_ringon"),
        ("mod_lfo1rate_noise", "mod_lfo1rate_noiseon"),
        ("mod_lfo1rate_mix", "mod_lfo1rate_mixon"),
        ("mod_lfo1rate_env1", "mod_lfo1rate_env1on"),
        ("mod_lfo1rate_env2", "mod_lfo1rate_env2on"),
        ("mod_lfo1rate_gate", "mod_lfo1rate_gateon"),
        ("mod_lfo1rate_glide", "mod_lfo1rate_glideon"),
        ("mod_lfo1rate_vel", "mod_lfo1rate_velon"),
        ("mod_lfo1rate_wheel", "mod_lfo1rate_wheelon"),
        ("mod_lfo1rate_press", "mod_lfo1rate_presson"),
        ("mod_lfo1rate_bend", "mod_lfo1rate_bendon"),
        ("mod_lfo1rate_lfo1saw", "mod_lfo1rate_lfo1sawon"),
        ("mod_lfo1rate_lfo1rsaw", "mod_lfo1rate_lfo1rsawon"),
        ("mod_lfo1rate_lfo1tri", "mod_lfo1rate_lfo1trion"),
        ("mod_lfo1rate_lfo1sin", "mod_lfo1rate_lfo1sinon"),
        ("mod_lfo1rate_lfo2tri", "mod_lfo1rate_lfo2trion"),
    ],
    [
        ("mod_lfo2rate_key", "mod_lfo2rate_keyon"),
        ("mod_lfo2rate_lfo1", "mod_lfo2rate_lfo1on"),
        ("mod_lfo2rate_lfo2", "mod_lfo2rate_lfo2on"),
        ("mod_lfo2rate_sh", "mod_lfo2rate_shon"),
        ("mod_lfo2rate_shclk", "mod_lfo2rate_shclkon"),
        ("mod_lfo2rate_vco1", "mod_lfo2rate_vco1on"),
        ("mod_lfo2rate_vco2", "mod_lfo2rate_vco2on"),
        ("mod_lfo2rate_vco1sy", "mod_lfo2rate_vco1syon"),
        ("mod_lfo2rate_vco2sy", "mod_lfo2rate_vco2syon"),
        ("mod_lfo2rate_ring", "mod_lfo2rate_ringon"),
        ("mod_lfo2rate_noise", "mod_lfo2rate_noiseon"),
        ("mod_lfo2rate_mix", "mod_lfo2rate_mixon"),
        ("mod_lfo2rate_env1", "mod_lfo2rate_env1on"),
        ("mod_lfo2rate_env2", "mod_lfo2rate_env2on"),
        ("mod_lfo2rate_gate", "mod_lfo2rate_gateon"),
        ("mod_lfo2rate_glide", "mod_lfo2rate_glideon"),
        ("mod_lfo2rate_vel", "mod_lfo2rate_velon"),
        ("mod_lfo2rate_wheel", "mod_lfo2rate_wheelon"),
        ("mod_lfo2rate_press", "mod_lfo2rate_presson"),
        ("mod_lfo2rate_bend", "mod_lfo2rate_bendon"),
        ("mod_lfo2rate_lfo1saw", "mod_lfo2rate_lfo1sawon"),
        ("mod_lfo2rate_lfo1rsaw", "mod_lfo2rate_lfo1rsawon"),
        ("mod_lfo2rate_lfo1tri", "mod_lfo2rate_lfo1trion"),
        ("mod_lfo2rate_lfo1sin", "mod_lfo2rate_lfo1sinon"),
        ("mod_lfo2rate_lfo2tri", "mod_lfo2rate_lfo2trion"),
    ],
    [
        ("mod_shsrc_key", "mod_shsrc_keyon"),
        ("mod_shsrc_lfo1", "mod_shsrc_lfo1on"),
        ("mod_shsrc_lfo2", "mod_shsrc_lfo2on"),
        ("mod_shsrc_sh", "mod_shsrc_shon"),
        ("mod_shsrc_shclk", "mod_shsrc_shclkon"),
        ("mod_shsrc_vco1", "mod_shsrc_vco1on"),
        ("mod_shsrc_vco2", "mod_shsrc_vco2on"),
        ("mod_shsrc_vco1sy", "mod_shsrc_vco1syon"),
        ("mod_shsrc_vco2sy", "mod_shsrc_vco2syon"),
        ("mod_shsrc_ring", "mod_shsrc_ringon"),
        ("mod_shsrc_noise", "mod_shsrc_noiseon"),
        ("mod_shsrc_mix", "mod_shsrc_mixon"),
        ("mod_shsrc_env1", "mod_shsrc_env1on"),
        ("mod_shsrc_env2", "mod_shsrc_env2on"),
        ("mod_shsrc_gate", "mod_shsrc_gateon"),
        ("mod_shsrc_glide", "mod_shsrc_glideon"),
        ("mod_shsrc_vel", "mod_shsrc_velon"),
        ("mod_shsrc_wheel", "mod_shsrc_wheelon"),
        ("mod_shsrc_press", "mod_shsrc_presson"),
        ("mod_shsrc_bend", "mod_shsrc_bendon"),
        ("mod_shsrc_lfo1saw", "mod_shsrc_lfo1sawon"),
        ("mod_shsrc_lfo1rsaw", "mod_shsrc_lfo1rsawon"),
        ("mod_shsrc_lfo1tri", "mod_shsrc_lfo1trion"),
        ("mod_shsrc_lfo1sin", "mod_shsrc_lfo1sinon"),
        ("mod_shsrc_lfo2tri", "mod_shsrc_lfo2trion"),
    ],
    [
        ("mod_vco1pitch_key", "mod_vco1pitch_keyon"),
        ("mod_vco1pitch_lfo1", "mod_vco1pitch_lfo1on"),
        ("mod_vco1pitch_lfo2", "mod_vco1pitch_lfo2on"),
        ("mod_vco1pitch_sh", "mod_vco1pitch_shon"),
        ("mod_vco1pitch_shclk", "mod_vco1pitch_shclkon"),
        ("mod_vco1pitch_vco1", "mod_vco1pitch_vco1on"),
        ("mod_vco1pitch_vco2", "mod_vco1pitch_vco2on"),
        ("mod_vco1pitch_vco1sy", "mod_vco1pitch_vco1syon"),
        ("mod_vco1pitch_vco2sy", "mod_vco1pitch_vco2syon"),
        ("mod_vco1pitch_ring", "mod_vco1pitch_ringon"),
        ("mod_vco1pitch_noise", "mod_vco1pitch_noiseon"),
        ("mod_vco1pitch_mix", "mod_vco1pitch_mixon"),
        ("mod_vco1pitch_env1", "mod_vco1pitch_env1on"),
        ("mod_vco1pitch_env2", "mod_vco1pitch_env2on"),
        ("mod_vco1pitch_gate", "mod_vco1pitch_gateon"),
        ("mod_vco1pitch_glide", "mod_vco1pitch_glideon"),
        ("mod_vco1pitch_vel", "mod_vco1pitch_velon"),
        ("mod_vco1pitch_wheel", "mod_vco1pitch_wheelon"),
        ("mod_vco1pitch_press", "mod_vco1pitch_presson"),
        ("mod_vco1pitch_bend", "mod_vco1pitch_bendon"),
        ("mod_vco1pitch_lfo1saw", "mod_vco1pitch_lfo1sawon"),
        ("mod_vco1pitch_lfo1rsaw", "mod_vco1pitch_lfo1rsawon"),
        ("mod_vco1pitch_lfo1tri", "mod_vco1pitch_lfo1trion"),
        ("mod_vco1pitch_lfo1sin", "mod_vco1pitch_lfo1sinon"),
        ("mod_vco1pitch_lfo2tri", "mod_vco1pitch_lfo2trion"),
    ],
    [
        ("mod_vco2sync_key", "mod_vco2sync_keyon"),
        ("mod_vco2sync_lfo1", "mod_vco2sync_lfo1on"),
        ("mod_vco2sync_lfo2", "mod_vco2sync_lfo2on"),
        ("mod_vco2sync_sh", "mod_vco2sync_shon"),
        ("mod_vco2sync_shclk", "mod_vco2sync_shclkon"),
        ("mod_vco2sync_vco1", "mod_vco2sync_vco1on"),
        ("mod_vco2sync_vco2", "mod_vco2sync_vco2on"),
        ("mod_vco2sync_vco1sy", "mod_vco2sync_vco1syon"),
        ("mod_vco2sync_vco2sy", "mod_vco2sync_vco2syon"),
        ("mod_vco2sync_ring", "mod_vco2sync_ringon"),
        ("mod_vco2sync_noise", "mod_vco2sync_noiseon"),
        ("mod_vco2sync_mix", "mod_vco2sync_mixon"),
        ("mod_vco2sync_env1", "mod_vco2sync_env1on"),
        ("mod_vco2sync_env2", "mod_vco2sync_env2on"),
        ("mod_vco2sync_gate", "mod_vco2sync_gateon"),
        ("mod_vco2sync_glide", "mod_vco2sync_glideon"),
        ("mod_vco2sync_vel", "mod_vco2sync_velon"),
        ("mod_vco2sync_wheel", "mod_vco2sync_wheelon"),
        ("mod_vco2sync_press", "mod_vco2sync_presson"),
        ("mod_vco2sync_bend", "mod_vco2sync_bendon"),
        ("mod_vco2sync_lfo1saw", "mod_vco2sync_lfo1sawon"),
        ("mod_vco2sync_lfo1rsaw", "mod_vco2sync_lfo1rsawon"),
        ("mod_vco2sync_lfo1tri", "mod_vco2sync_lfo1trion"),
        ("mod_vco2sync_lfo1sin", "mod_vco2sync_lfo1sinon"),
        ("mod_vco2sync_lfo2tri", "mod_vco2sync_lfo2trion"),
    ],
    [
        ("mod_ringin_key", "mod_ringin_keyon"),
        ("mod_ringin_lfo1", "mod_ringin_lfo1on"),
        ("mod_ringin_lfo2", "mod_ringin_lfo2on"),
        ("mod_ringin_sh", "mod_ringin_shon"),
        ("mod_ringin_shclk", "mod_ringin_shclkon"),
        ("mod_ringin_vco1", "mod_ringin_vco1on"),
        ("mod_ringin_vco2", "mod_ringin_vco2on"),
        ("mod_ringin_vco1sy", "mod_ringin_vco1syon"),
        ("mod_ringin_vco2sy", "mod_ringin_vco2syon"),
        ("mod_ringin_ring", "mod_ringin_ringon"),
        ("mod_ringin_noise", "mod_ringin_noiseon"),
        ("mod_ringin_mix", "mod_ringin_mixon"),
        ("mod_ringin_env1", "mod_ringin_env1on"),
        ("mod_ringin_env2", "mod_ringin_env2on"),
        ("mod_ringin_gate", "mod_ringin_gateon"),
        ("mod_ringin_glide", "mod_ringin_glideon"),
        ("mod_ringin_vel", "mod_ringin_velon"),
        ("mod_ringin_wheel", "mod_ringin_wheelon"),
        ("mod_ringin_press", "mod_ringin_presson"),
        ("mod_ringin_bend", "mod_ringin_bendon"),
        ("mod_ringin_lfo1saw", "mod_ringin_lfo1sawon"),
        ("mod_ringin_lfo1rsaw", "mod_ringin_lfo1rsawon"),
        ("mod_ringin_lfo1tri", "mod_ringin_lfo1trion"),
        ("mod_ringin_lfo1sin", "mod_ringin_lfo1sinon"),
        ("mod_ringin_lfo2tri", "mod_ringin_lfo2trion"),
    ],
    [
        ("mod_mixin_key", "mod_mixin_keyon"),
        ("mod_mixin_lfo1", "mod_mixin_lfo1on"),
        ("mod_mixin_lfo2", "mod_mixin_lfo2on"),
        ("mod_mixin_sh", "mod_mixin_shon"),
        ("mod_mixin_shclk", "mod_mixin_shclkon"),
        ("mod_mixin_vco1", "mod_mixin_vco1on"),
        ("mod_mixin_vco2", "mod_mixin_vco2on"),
        ("mod_mixin_vco1sy", "mod_mixin_vco1syon"),
        ("mod_mixin_vco2sy", "mod_mixin_vco2syon"),
        ("mod_mixin_ring", "mod_mixin_ringon"),
        ("mod_mixin_noise", "mod_mixin_noiseon"),
        ("mod_mixin_mix", "mod_mixin_mixon"),
        ("mod_mixin_env1", "mod_mixin_env1on"),
        ("mod_mixin_env2", "mod_mixin_env2on"),
        ("mod_mixin_gate", "mod_mixin_gateon"),
        ("mod_mixin_glide", "mod_mixin_glideon"),
        ("mod_mixin_vel", "mod_mixin_velon"),
        ("mod_mixin_wheel", "mod_mixin_wheelon"),
        ("mod_mixin_press", "mod_mixin_presson"),
        ("mod_mixin_bend", "mod_mixin_bendon"),
        ("mod_mixin_lfo1saw", "mod_mixin_lfo1sawon"),
        ("mod_mixin_lfo1rsaw", "mod_mixin_lfo1rsawon"),
        ("mod_mixin_lfo1tri", "mod_mixin_lfo1trion"),
        ("mod_mixin_lfo1sin", "mod_mixin_lfo1sinon"),
        ("mod_mixin_lfo2tri", "mod_mixin_lfo2trion"),
    ],
    [
        ("mod_vcfgate_key", "mod_vcfgate_keyon"),
        ("mod_vcfgate_lfo1", "mod_vcfgate_lfo1on"),
        ("mod_vcfgate_lfo2", "mod_vcfgate_lfo2on"),
        ("mod_vcfgate_sh", "mod_vcfgate_shon"),
        ("mod_vcfgate_shclk", "mod_vcfgate_shclkon"),
        ("mod_vcfgate_vco1", "mod_vcfgate_vco1on"),
        ("mod_vcfgate_vco2", "mod_vcfgate_vco2on"),
        ("mod_vcfgate_vco1sy", "mod_vcfgate_vco1syon"),
        ("mod_vcfgate_vco2sy", "mod_vcfgate_vco2syon"),
        ("mod_vcfgate_ring", "mod_vcfgate_ringon"),
        ("mod_vcfgate_noise", "mod_vcfgate_noiseon"),
        ("mod_vcfgate_mix", "mod_vcfgate_mixon"),
        ("mod_vcfgate_env1", "mod_vcfgate_env1on"),
        ("mod_vcfgate_env2", "mod_vcfgate_env2on"),
        ("mod_vcfgate_gate", "mod_vcfgate_gateon"),
        ("mod_vcfgate_glide", "mod_vcfgate_glideon"),
        ("mod_vcfgate_vel", "mod_vcfgate_velon"),
        ("mod_vcfgate_wheel", "mod_vcfgate_wheelon"),
        ("mod_vcfgate_press", "mod_vcfgate_presson"),
        ("mod_vcfgate_bend", "mod_vcfgate_bendon"),
        ("mod_vcfgate_lfo1saw", "mod_vcfgate_lfo1sawon"),
        ("mod_vcfgate_lfo1rsaw", "mod_vcfgate_lfo1rsawon"),
        ("mod_vcfgate_lfo1tri", "mod_vcfgate_lfo1trion"),
        ("mod_vcfgate_lfo1sin", "mod_vcfgate_lfo1sinon"),
        ("mod_vcfgate_lfo2tri", "mod_vcfgate_lfo2trion"),
    ],
    [
        ("mod_vcacv_key", "mod_vcacv_keyon"),
        ("mod_vcacv_lfo1", "mod_vcacv_lfo1on"),
        ("mod_vcacv_lfo2", "mod_vcacv_lfo2on"),
        ("mod_vcacv_sh", "mod_vcacv_shon"),
        ("mod_vcacv_shclk", "mod_vcacv_shclkon"),
        ("mod_vcacv_vco1", "mod_vcacv_vco1on"),
        ("mod_vcacv_vco2", "mod_vcacv_vco2on"),
        ("mod_vcacv_vco1sy", "mod_vcacv_vco1syon"),
        ("mod_vcacv_vco2sy", "mod_vcacv_vco2syon"),
        ("mod_vcacv_ring", "mod_vcacv_ringon"),
        ("mod_vcacv_noise", "mod_vcacv_noiseon"),
        ("mod_vcacv_mix", "mod_vcacv_mixon"),
        ("mod_vcacv_env1", "mod_vcacv_env1on"),
        ("mod_vcacv_env2", "mod_vcacv_env2on"),
        ("mod_vcacv_gate", "mod_vcacv_gateon"),
        ("mod_vcacv_glide", "mod_vcacv_glideon"),
        ("mod_vcacv_vel", "mod_vcacv_velon"),
        ("mod_vcacv_wheel", "mod_vcacv_wheelon"),
        ("mod_vcacv_press", "mod_vcacv_presson"),
        ("mod_vcacv_bend", "mod_vcacv_bendon"),
        ("mod_vcacv_lfo1saw", "mod_vcacv_lfo1sawon"),
        ("mod_vcacv_lfo1rsaw", "mod_vcacv_lfo1rsawon"),
        ("mod_vcacv_lfo1tri", "mod_vcacv_lfo1trion"),
        ("mod_vcacv_lfo1sin", "mod_vcacv_lfo1sinon"),
        ("mod_vcacv_lfo2tri", "mod_vcacv_lfo2trion"),
    ],
    [
        ("mod_vcagate_key", "mod_vcagate_keyon"),
        ("mod_vcagate_lfo1", "mod_vcagate_lfo1on"),
        ("mod_vcagate_lfo2", "mod_vcagate_lfo2on"),
        ("mod_vcagate_sh", "mod_vcagate_shon"),
        ("mod_vcagate_shclk", "mod_vcagate_shclkon"),
        ("mod_vcagate_vco1", "mod_vcagate_vco1on"),
        ("mod_vcagate_vco2", "mod_vcagate_vco2on"),
        ("mod_vcagate_vco1sy", "mod_vcagate_vco1syon"),
        ("mod_vcagate_vco2sy", "mod_vcagate_vco2syon"),
        ("mod_vcagate_ring", "mod_vcagate_ringon"),
        ("mod_vcagate_noise", "mod_vcagate_noiseon"),
        ("mod_vcagate_mix", "mod_vcagate_mixon"),
        ("mod_vcagate_env1", "mod_vcagate_env1on"),
        ("mod_vcagate_env2", "mod_vcagate_env2on"),
        ("mod_vcagate_gate", "mod_vcagate_gateon"),
        ("mod_vcagate_glide", "mod_vcagate_glideon"),
        ("mod_vcagate_vel", "mod_vcagate_velon"),
        ("mod_vcagate_wheel", "mod_vcagate_wheelon"),
        ("mod_vcagate_press", "mod_vcagate_presson"),
        ("mod_vcagate_bend", "mod_vcagate_bendon"),
        ("mod_vcagate_lfo1saw", "mod_vcagate_lfo1sawon"),
        ("mod_vcagate_lfo1rsaw", "mod_vcagate_lfo1rsawon"),
        ("mod_vcagate_lfo1tri", "mod_vcagate_lfo1trion"),
        ("mod_vcagate_lfo1sin", "mod_vcagate_lfo1sinon"),
        ("mod_vcagate_lfo2tri", "mod_vcagate_lfo2trion"),
    ],
    [
        ("mod_phlfo_key", "mod_phlfo_keyon"),
        ("mod_phlfo_lfo1", "mod_phlfo_lfo1on"),
        ("mod_phlfo_lfo2", "mod_phlfo_lfo2on"),
        ("mod_phlfo_sh", "mod_phlfo_shon"),
        ("mod_phlfo_shclk", "mod_phlfo_shclkon"),
        ("mod_phlfo_vco1", "mod_phlfo_vco1on"),
        ("mod_phlfo_vco2", "mod_phlfo_vco2on"),
        ("mod_phlfo_vco1sy", "mod_phlfo_vco1syon"),
        ("mod_phlfo_vco2sy", "mod_phlfo_vco2syon"),
        ("mod_phlfo_ring", "mod_phlfo_ringon"),
        ("mod_phlfo_noise", "mod_phlfo_noiseon"),
        ("mod_phlfo_mix", "mod_phlfo_mixon"),
        ("mod_phlfo_env1", "mod_phlfo_env1on"),
        ("mod_phlfo_env2", "mod_phlfo_env2on"),
        ("mod_phlfo_gate", "mod_phlfo_gateon"),
        ("mod_phlfo_glide", "mod_phlfo_glideon"),
        ("mod_phlfo_vel", "mod_phlfo_velon"),
        ("mod_phlfo_wheel", "mod_phlfo_wheelon"),
        ("mod_phlfo_press", "mod_phlfo_presson"),
        ("mod_phlfo_bend", "mod_phlfo_bendon"),
        ("mod_phlfo_lfo1saw", "mod_phlfo_lfo1sawon"),
        ("mod_phlfo_lfo1rsaw", "mod_phlfo_lfo1rsawon"),
        ("mod_phlfo_lfo1tri", "mod_phlfo_lfo1trion"),
        ("mod_phlfo_lfo1sin", "mod_phlfo_lfo1sinon"),
        ("mod_phlfo_lfo2tri", "mod_phlfo_lfo2trion"),
    ],
    [
        ("mod_phman_key", "mod_phman_keyon"),
        ("mod_phman_lfo1", "mod_phman_lfo1on"),
        ("mod_phman_lfo2", "mod_phman_lfo2on"),
        ("mod_phman_sh", "mod_phman_shon"),
        ("mod_phman_shclk", "mod_phman_shclkon"),
        ("mod_phman_vco1", "mod_phman_vco1on"),
        ("mod_phman_vco2", "mod_phman_vco2on"),
        ("mod_phman_vco1sy", "mod_phman_vco1syon"),
        ("mod_phman_vco2sy", "mod_phman_vco2syon"),
        ("mod_phman_ring", "mod_phman_ringon"),
        ("mod_phman_noise", "mod_phman_noiseon"),
        ("mod_phman_mix", "mod_phman_mixon"),
        ("mod_phman_env1", "mod_phman_env1on"),
        ("mod_phman_env2", "mod_phman_env2on"),
        ("mod_phman_gate", "mod_phman_gateon"),
        ("mod_phman_glide", "mod_phman_glideon"),
        ("mod_phman_vel", "mod_phman_velon"),
        ("mod_phman_wheel", "mod_phman_wheelon"),
        ("mod_phman_press", "mod_phman_presson"),
        ("mod_phman_bend", "mod_phman_bendon"),
        ("mod_phman_lfo1saw", "mod_phman_lfo1sawon"),
        ("mod_phman_lfo1rsaw", "mod_phman_lfo1rsawon"),
        ("mod_phman_lfo1tri", "mod_phman_lfo1trion"),
        ("mod_phman_lfo1sin", "mod_phman_lfo1sinon"),
        ("mod_phman_lfo2tri", "mod_phman_lfo2trion"),
    ],
    [
        ("mod_vco2pitch_key", "mod_vco2pitch_keyon"),
        ("mod_vco2pitch_lfo1", "mod_vco2pitch_lfo1on"),
        ("mod_vco2pitch_lfo2", "mod_vco2pitch_lfo2on"),
        ("mod_vco2pitch_sh", "mod_vco2pitch_shon"),
        ("mod_vco2pitch_shclk", "mod_vco2pitch_shclkon"),
        ("mod_vco2pitch_vco1", "mod_vco2pitch_vco1on"),
        ("mod_vco2pitch_vco2", "mod_vco2pitch_vco2on"),
        ("mod_vco2pitch_vco1sy", "mod_vco2pitch_vco1syon"),
        ("mod_vco2pitch_vco2sy", "mod_vco2pitch_vco2syon"),
        ("mod_vco2pitch_ring", "mod_vco2pitch_ringon"),
        ("mod_vco2pitch_noise", "mod_vco2pitch_noiseon"),
        ("mod_vco2pitch_mix", "mod_vco2pitch_mixon"),
        ("mod_vco2pitch_env1", "mod_vco2pitch_env1on"),
        ("mod_vco2pitch_env2", "mod_vco2pitch_env2on"),
        ("mod_vco2pitch_gate", "mod_vco2pitch_gateon"),
        ("mod_vco2pitch_glide", "mod_vco2pitch_glideon"),
        ("mod_vco2pitch_vel", "mod_vco2pitch_velon"),
        ("mod_vco2pitch_wheel", "mod_vco2pitch_wheelon"),
        ("mod_vco2pitch_press", "mod_vco2pitch_presson"),
        ("mod_vco2pitch_bend", "mod_vco2pitch_bendon"),
        ("mod_vco2pitch_lfo1saw", "mod_vco2pitch_lfo1sawon"),
        ("mod_vco2pitch_lfo1rsaw", "mod_vco2pitch_lfo1rsawon"),
        ("mod_vco2pitch_lfo1tri", "mod_vco2pitch_lfo1trion"),
        ("mod_vco2pitch_lfo1sin", "mod_vco2pitch_lfo1sinon"),
        ("mod_vco2pitch_lfo2tri", "mod_vco2pitch_lfo2trion"),
    ],
    [
        ("mod_pw1_key", "mod_pw1_keyon"),
        ("mod_pw1_lfo1", "mod_pw1_lfo1on"),
        ("mod_pw1_lfo2", "mod_pw1_lfo2on"),
        ("mod_pw1_sh", "mod_pw1_shon"),
        ("mod_pw1_shclk", "mod_pw1_shclkon"),
        ("mod_pw1_vco1", "mod_pw1_vco1on"),
        ("mod_pw1_vco2", "mod_pw1_vco2on"),
        ("mod_pw1_vco1sy", "mod_pw1_vco1syon"),
        ("mod_pw1_vco2sy", "mod_pw1_vco2syon"),
        ("mod_pw1_ring", "mod_pw1_ringon"),
        ("mod_pw1_noise", "mod_pw1_noiseon"),
        ("mod_pw1_mix", "mod_pw1_mixon"),
        ("mod_pw1_env1", "mod_pw1_env1on"),
        ("mod_pw1_env2", "mod_pw1_env2on"),
        ("mod_pw1_gate", "mod_pw1_gateon"),
        ("mod_pw1_glide", "mod_pw1_glideon"),
        ("mod_pw1_vel", "mod_pw1_velon"),
        ("mod_pw1_wheel", "mod_pw1_wheelon"),
        ("mod_pw1_press", "mod_pw1_presson"),
        ("mod_pw1_bend", "mod_pw1_bendon"),
        ("mod_pw1_lfo1saw", "mod_pw1_lfo1sawon"),
        ("mod_pw1_lfo1rsaw", "mod_pw1_lfo1rsawon"),
        ("mod_pw1_lfo1tri", "mod_pw1_lfo1trion"),
        ("mod_pw1_lfo1sin", "mod_pw1_lfo1sinon"),
        ("mod_pw1_lfo2tri", "mod_pw1_lfo2trion"),
    ],
    [
        ("mod_pw2_key", "mod_pw2_keyon"),
        ("mod_pw2_lfo1", "mod_pw2_lfo1on"),
        ("mod_pw2_lfo2", "mod_pw2_lfo2on"),
        ("mod_pw2_sh", "mod_pw2_shon"),
        ("mod_pw2_shclk", "mod_pw2_shclkon"),
        ("mod_pw2_vco1", "mod_pw2_vco1on"),
        ("mod_pw2_vco2", "mod_pw2_vco2on"),
        ("mod_pw2_vco1sy", "mod_pw2_vco1syon"),
        ("mod_pw2_vco2sy", "mod_pw2_vco2syon"),
        ("mod_pw2_ring", "mod_pw2_ringon"),
        ("mod_pw2_noise", "mod_pw2_noiseon"),
        ("mod_pw2_mix", "mod_pw2_mixon"),
        ("mod_pw2_env1", "mod_pw2_env1on"),
        ("mod_pw2_env2", "mod_pw2_env2on"),
        ("mod_pw2_gate", "mod_pw2_gateon"),
        ("mod_pw2_glide", "mod_pw2_glideon"),
        ("mod_pw2_vel", "mod_pw2_velon"),
        ("mod_pw2_wheel", "mod_pw2_wheelon"),
        ("mod_pw2_press", "mod_pw2_presson"),
        ("mod_pw2_bend", "mod_pw2_bendon"),
        ("mod_pw2_lfo1saw", "mod_pw2_lfo1sawon"),
        ("mod_pw2_lfo1rsaw", "mod_pw2_lfo1rsawon"),
        ("mod_pw2_lfo1tri", "mod_pw2_lfo1trion"),
        ("mod_pw2_lfo1sin", "mod_pw2_lfo1sinon"),
        ("mod_pw2_lfo2tri", "mod_pw2_lfo2trion"),
    ],
    [
        ("mod_trem_key", "mod_trem_keyon"),
        ("mod_trem_lfo1", "mod_trem_lfo1on"),
        ("mod_trem_lfo2", "mod_trem_lfo2on"),
        ("mod_trem_sh", "mod_trem_shon"),
        ("mod_trem_shclk", "mod_trem_shclkon"),
        ("mod_trem_vco1", "mod_trem_vco1on"),
        ("mod_trem_vco2", "mod_trem_vco2on"),
        ("mod_trem_vco1sy", "mod_trem_vco1syon"),
        ("mod_trem_vco2sy", "mod_trem_vco2syon"),
        ("mod_trem_ring", "mod_trem_ringon"),
        ("mod_trem_noise", "mod_trem_noiseon"),
        ("mod_trem_mix", "mod_trem_mixon"),
        ("mod_trem_env1", "mod_trem_env1on"),
        ("mod_trem_env2", "mod_trem_env2on"),
        ("mod_trem_gate", "mod_trem_gateon"),
        ("mod_trem_glide", "mod_trem_glideon"),
        ("mod_trem_vel", "mod_trem_velon"),
        ("mod_trem_wheel", "mod_trem_wheelon"),
        ("mod_trem_press", "mod_trem_presson"),
        ("mod_trem_bend", "mod_trem_bendon"),
        ("mod_trem_lfo1saw", "mod_trem_lfo1sawon"),
        ("mod_trem_lfo1rsaw", "mod_trem_lfo1rsawon"),
        ("mod_trem_lfo1tri", "mod_trem_lfo1trion"),
        ("mod_trem_lfo1sin", "mod_trem_lfo1sinon"),
        ("mod_trem_lfo2tri", "mod_trem_lfo2trion"),
    ],
    [
        ("mod_cutoff_key", "mod_cutoff_keyon"),
        ("mod_cutoff_lfo1", "mod_cutoff_lfo1on"),
        ("mod_cutoff_lfo2", "mod_cutoff_lfo2on"),
        ("mod_cutoff_sh", "mod_cutoff_shon"),
        ("mod_cutoff_shclk", "mod_cutoff_shclkon"),
        ("mod_cutoff_vco1", "mod_cutoff_vco1on"),
        ("mod_cutoff_vco2", "mod_cutoff_vco2on"),
        ("mod_cutoff_vco1sy", "mod_cutoff_vco1syon"),
        ("mod_cutoff_vco2sy", "mod_cutoff_vco2syon"),
        ("mod_cutoff_ring", "mod_cutoff_ringon"),
        ("mod_cutoff_noise", "mod_cutoff_noiseon"),
        ("mod_cutoff_mix", "mod_cutoff_mixon"),
        ("mod_cutoff_env1", "mod_cutoff_env1on"),
        ("mod_cutoff_env2", "mod_cutoff_env2on"),
        ("mod_cutoff_gate", "mod_cutoff_gateon"),
        ("mod_cutoff_glide", "mod_cutoff_glideon"),
        ("mod_cutoff_vel", "mod_cutoff_velon"),
        ("mod_cutoff_wheel", "mod_cutoff_wheelon"),
        ("mod_cutoff_press", "mod_cutoff_presson"),
        ("mod_cutoff_bend", "mod_cutoff_bendon"),
        ("mod_cutoff_lfo1saw", "mod_cutoff_lfo1sawon"),
        ("mod_cutoff_lfo1rsaw", "mod_cutoff_lfo1rsawon"),
        ("mod_cutoff_lfo1tri", "mod_cutoff_lfo1trion"),
        ("mod_cutoff_lfo1sin", "mod_cutoff_lfo1sinon"),
        ("mod_cutoff_lfo2tri", "mod_cutoff_lfo2trion"),
    ],
    [
        ("mod_amplitude_key", "mod_amplitude_keyon"),
        ("mod_amplitude_lfo1", "mod_amplitude_lfo1on"),
        ("mod_amplitude_lfo2", "mod_amplitude_lfo2on"),
        ("mod_amplitude_sh", "mod_amplitude_shon"),
        ("mod_amplitude_shclk", "mod_amplitude_shclkon"),
        ("mod_amplitude_vco1", "mod_amplitude_vco1on"),
        ("mod_amplitude_vco2", "mod_amplitude_vco2on"),
        ("mod_amplitude_vco1sy", "mod_amplitude_vco1syon"),
        ("mod_amplitude_vco2sy", "mod_amplitude_vco2syon"),
        ("mod_amplitude_ring", "mod_amplitude_ringon"),
        ("mod_amplitude_noise", "mod_amplitude_noiseon"),
        ("mod_amplitude_mix", "mod_amplitude_mixon"),
        ("mod_amplitude_env1", "mod_amplitude_env1on"),
        ("mod_amplitude_env2", "mod_amplitude_env2on"),
        ("mod_amplitude_gate", "mod_amplitude_gateon"),
        ("mod_amplitude_glide", "mod_amplitude_glideon"),
        ("mod_amplitude_vel", "mod_amplitude_velon"),
        ("mod_amplitude_wheel", "mod_amplitude_wheelon"),
        ("mod_amplitude_press", "mod_amplitude_presson"),
        ("mod_amplitude_bend", "mod_amplitude_bendon"),
        ("mod_amplitude_lfo1saw", "mod_amplitude_lfo1sawon"),
        ("mod_amplitude_lfo1rsaw", "mod_amplitude_lfo1rsawon"),
        ("mod_amplitude_lfo1tri", "mod_amplitude_lfo1trion"),
        ("mod_amplitude_lfo1sin", "mod_amplitude_lfo1sinon"),
        ("mod_amplitude_lfo2tri", "mod_amplitude_lfo2trion"),
    ],
];

/// Every **offered** pair's two ids, `(amount, presence)`, in `[target][source]` order: [`ROUTE_IDS`]
/// less the pairs the modulation standard refuses, which are no parameter.
pub fn offered_route_ids() -> impl Iterator<Item = (&'static str, &'static str)> {
    ROUTE_IDS.iter().enumerate().flat_map(|(target, row)| {
        row.iter()
            .enumerate()
            .filter(move |&(source, _)| offer(target, source) != Offer::Refused)
            .map(|(_, ids)| *ids)
    })
}

/// Each pair's two ids **inside its target's group**, `(presence, amount)`, in declared source
/// order — what the derive emitted before the hand-written `Params` impl replaced it.
pub const SHORT_IDS: [(&str, &str); SOURCES] = [
    ("keyon", "key"),
    ("lfo1on", "lfo1"),
    ("lfo2on", "lfo2"),
    ("shon", "sh"),
    ("shclkon", "shclk"),
    ("vco1on", "vco1"),
    ("vco2on", "vco2"),
    ("vco1syon", "vco1sy"),
    ("vco2syon", "vco2sy"),
    ("ringon", "ring"),
    ("noiseon", "noise"),
    ("mixon", "mix"),
    ("env1on", "env1"),
    ("env2on", "env2"),
    ("gateon", "gate"),
    ("glideon", "glide"),
    ("velon", "vel"),
    ("wheelon", "wheel"),
    ("presson", "press"),
    ("bendon", "bend"),
    ("lfo1sawon", "lfo1saw"),
    ("lfo1rsawon", "lfo1rsaw"),
    ("lfo1trion", "lfo1tri"),
    ("lfo1sinon", "lfo1sin"),
    ("lfo2trion", "lfo2tri"),
];

/// One target's twenty-five pairs.
///
/// Every target carries the same grid in memory — this machine's patch bay lets any output reach
/// any input — but **the pairs the modulation standard refuses are no parameter**: the hand-written
/// `Params` impl below leaves them out, and `presences` and `offered_routes` never report them.
pub struct TargetRoutes {
    /// Which target this group is, for its offer. Not a parameter.
    target: usize,
    pub key_on: BoolParam,
    pub key: FloatParam,
    pub lfo1_on: BoolParam,
    pub lfo1: FloatParam,
    pub lfo2_on: BoolParam,
    pub lfo2: FloatParam,
    pub sh_on: BoolParam,
    pub sh: FloatParam,
    pub sh_clock_on: BoolParam,
    pub sh_clock: FloatParam,
    pub vco1_on: BoolParam,
    pub vco1: FloatParam,
    pub vco2_on: BoolParam,
    pub vco2: FloatParam,
    pub vco1_sync_on: BoolParam,
    pub vco1_sync: FloatParam,
    pub vco2_sync_on: BoolParam,
    pub vco2_sync: FloatParam,
    pub ring_on: BoolParam,
    pub ring: FloatParam,
    pub noise_on: BoolParam,
    pub noise: FloatParam,
    pub mixer_on: BoolParam,
    pub mixer: FloatParam,
    pub env1_on: BoolParam,
    pub env1: FloatParam,
    pub env2_on: BoolParam,
    pub env2: FloatParam,
    pub gate_on: BoolParam,
    pub gate: FloatParam,
    pub glide_on: BoolParam,
    pub glide: FloatParam,
    pub velocity_on: BoolParam,
    pub velocity: FloatParam,
    pub wheel_on: BoolParam,
    pub wheel: FloatParam,
    pub pressure_on: BoolParam,
    pub pressure: FloatParam,
    pub bend_on: BoolParam,
    pub bend: FloatParam,
    pub lfo1_saw_on: BoolParam,
    pub lfo1_saw: FloatParam,
    pub lfo1_reverse_saw_on: BoolParam,
    pub lfo1_reverse_saw: FloatParam,
    pub lfo1_triangle_on: BoolParam,
    pub lfo1_triangle: FloatParam,
    pub lfo1_sine_on: BoolParam,
    pub lfo1_sine: FloatParam,
    pub lfo2_triangle_on: BoolParam,
    pub lfo2_triangle: FloatParam,
}

/// What one full-amount route reaches on a given target, **in the target's own unit** — per octave
/// of keyboard for Key.
///
/// `plan-modulation-routing.md` decision 1.11: *"a route amount reads in the target's own unit,
/// not as a percent."* The reach comes from the DSP's own table (`routing::scale`, the row's or the
/// standard's), so a scale that moves moves the reading with it. **The phaser reads in octaves**,
/// its CV inputs' own four per unit; **Key reads per octave** everywhere — the cutoff's one octave
/// per octave, where it read the retired KYBD CV slider's 100 %.
///
/// **The sync input has no reach**: its amount is reset depth, which is a fraction of a full
/// reset rather than an amount of anything, so it reads as a percentage of that.
pub fn reach(target: usize, source: usize) -> Reach {
    use mxm_mono_00_dsp::effects::{PHASER_LFO_OCTAVES_PER_UNIT, PHASER_MANUAL_OCTAVES_PER_UNIT};
    use mxm_mono_00_dsp::routing::{SOURCE_PEAK, source};
    let unit = match target {
        target::VCO1_PITCH | target::VCO2_PITCH => reading::SEMITONES,
        target::LFO1_RATE
        | target::LFO2_RATE
        | target::CUTOFF
        | target::PHASER_RATE
        | target::PHASER_CENTRE => reading::OCTAVES,
        // A gate level, an audio level, a sample-and-hold's input, a tremolo's dip, a pulse width's
        // narrowing and the two amplitudes are fractions of a span. A percentage is the honest
        // reading for those, and for the sync depth.
        _ => reading::PERCENT,
    };
    // **The multiplier of a sync route is not a reach at all**: it is how far the reset is carried
    // out, so full amount means *the hardware's own reset* and reads `+100 %`.
    if target == target::VCO2_SYNC {
        return Reach::new(1.0, unit);
    }
    let domain = match target {
        target::PHASER_RATE => PHASER_LFO_OCTAVES_PER_UNIT,
        target::PHASER_CENTRE => PHASER_MANUAL_OCTAVES_PER_UNIT,
        _ => 1.0,
    };
    let per_unit = scale(target, source) * domain;
    if source == source::KEY {
        // A tenth of a ten-volt unit per octave.
        Reach::per_octave(per_unit * 12.0 / KEY_UNIT_SEMITONES, unit)
    } else {
        // **Per unit of source, and the sources do not all fill the unit.** What a player reads is
        // what *this pair* delivers at full amount — the envelope's seven octaves of cutoff rather
        // than the 11.67 a unit-magnitude source would get. See `SOURCE_PEAK`.
        Reach::new(per_unit * SOURCE_PEAK[source], unit)
    }
}

/// A pair's fader: its offer's halves, and **square-law only on a pitch row's own pairs** — 144
/// semitones a unit, where a vibrato's cents need the taper's first tenth. Key's 1 V/oct and a
/// performance pair's twelve semitones are linear.
fn fader(target: usize, source: usize) -> Fader {
    let square_law = matches!(target, target::VCO1_PITCH | target::VCO2_PITCH)
        && source != mxm_mono_00_dsp::routing::source::KEY
        && !takes_standard_reach(target, source);
    Fader::for_offer(offer(target, source), square_law)
}

/// A route amount: signed, centred on zero, and **an amount, so it usually starts there**.
///
/// Smoothed, because it multiplies a signal — `plugins/AGENTS.md`'s *smooth signals, not
/// coefficients*. Drawn bipolar, because its centre is *no modulation* and a half-filled track
/// would read as "half on" when it means "off".
///
/// **The exception is the plug-out's own wiring.** Four of this machine's normalled connections
/// had no attenuator at all — the jack was made or it was not — so they have no zero to inherit
/// and start at full. [`INIT_AT_FULL`] names them and `plugins/mxm-mono-00/AGENTS.md` records them
/// as init deviations beside the three the instrument already had.
///
/// **And the sync normal waits at full while it is absent.** [`SYNC_NORMAL`]'s presence is the
/// machine's SYNC switch, so adding it — from the card or from the controller's `osc2.sync` role,
/// which writes the presence alone — has to sync at once. At zero it added a route that did
/// nothing (code review, 2026-09-22; the owner's ruling). Init still wires nothing, so a fresh
/// instance is unchanged.
///
/// **A pitch row's own faders are square-law about the centre** — [`PITCH_TAPER`], the shared
/// `Fader::SquareLaw` — so that a vibrato's few cents sit in the first tenth of the travel rather
/// than in its first pixel, while the far end still reaches the whole 144 semitones (the owner's
/// ruling, 2026-09-22). The plain value, and so the DSP and the reading, stay linear: only the
/// travel is curved. A one-sided offer has only its live half (`fader`).
fn amount(target: usize, source: usize, name: String) -> FloatParam {
    let default = if INIT_AT_FULL.contains(&(target, source)) || (target, source) == SYNC_NORMAL {
        1.0
    } else {
        0.0
    };
    reading::amount_param_at(
        name,
        default,
        reach(target, source),
        fader(target, source),
        10.0,
    )
}

/// The pitch inputs' fader law: **square-law about the centre**. A tenth of the travel either side
/// is an amount of 0.01 — ±1.44 semitones from an LFO — and the retired Vibrato knob's whole
/// ±12 semitones sits at about 29 %. mxm-kit's `docs/plugin-conventions.md` (*Parameters*), which
/// `plugins/AGENTS.md` links, names `SymmetricalSkewed` about zero as
/// the tool for a bipolar pitch span this wide, and asks for a reading that resolves below a
/// semitone, which two decimals of semitones does. Shift is the shared controls' fine drag.
pub const PITCH_TAPER: FloatRange = FloatRange::SymmetricalSkewed {
    min: -1.0,
    max: 1.0,
    factor: 0.5,
    center: 0.0,
};

/// Whether a route exists. **Configuration, not an amount**, so its default is the plug-out's own
/// internal connection — the jack as it comes, which a player can pull out.
fn present(target: &str, source: &str, wired: bool) -> BoolParam {
    BoolParam::new(format!("{target} from {source} on"), wired)
}

impl TargetRoutes {
    /// Every pair for one target, at the init patch.
    pub fn new(index: usize) -> Self {
        let n = SOURCE_NAMES;
        let wired =
            |s: usize| INIT_PRESENT.contains(&(index, s)) || INIT_AT_FULL.contains(&(index, s));
        let t = TARGET_NAMES[index];
        Self {
            target: index,
            key_on: present(t, n[0], wired(0)),
            key: amount(index, 0, format!("{t} from {}", n[0])),
            lfo1_on: present(t, n[1], wired(1)),
            lfo1: amount(index, 1, format!("{t} from {}", n[1])),
            lfo2_on: present(t, n[2], wired(2)),
            lfo2: amount(index, 2, format!("{t} from {}", n[2])),
            sh_on: present(t, n[3], wired(3)),
            sh: amount(index, 3, format!("{t} from {}", n[3])),
            sh_clock_on: present(t, n[4], wired(4)),
            sh_clock: amount(index, 4, format!("{t} from {}", n[4])),
            vco1_on: present(t, n[5], wired(5)),
            vco1: amount(index, 5, format!("{t} from {}", n[5])),
            vco2_on: present(t, n[6], wired(6)),
            vco2: amount(index, 6, format!("{t} from {}", n[6])),
            vco1_sync_on: present(t, n[7], wired(7)),
            vco1_sync: amount(index, 7, format!("{t} from {}", n[7])),
            vco2_sync_on: present(t, n[8], wired(8)),
            vco2_sync: amount(index, 8, format!("{t} from {}", n[8])),
            ring_on: present(t, n[9], wired(9)),
            ring: amount(index, 9, format!("{t} from {}", n[9])),
            noise_on: present(t, n[10], wired(10)),
            noise: amount(index, 10, format!("{t} from {}", n[10])),
            mixer_on: present(t, n[11], wired(11)),
            mixer: amount(index, 11, format!("{t} from {}", n[11])),
            env1_on: present(t, n[12], wired(12)),
            env1: amount(index, 12, format!("{t} from {}", n[12])),
            env2_on: present(t, n[13], wired(13)),
            env2: amount(index, 13, format!("{t} from {}", n[13])),
            gate_on: present(t, n[14], wired(14)),
            gate: amount(index, 14, format!("{t} from {}", n[14])),
            glide_on: present(t, n[15], wired(15)),
            glide: amount(index, 15, format!("{t} from {}", n[15])),
            velocity_on: present(t, n[16], wired(16)),
            velocity: amount(index, 16, format!("{t} from {}", n[16])),
            wheel_on: present(t, n[17], wired(17)),
            wheel: amount(index, 17, format!("{t} from {}", n[17])),
            pressure_on: present(t, n[18], wired(18)),
            pressure: amount(index, 18, format!("{t} from {}", n[18])),
            bend_on: present(t, n[19], wired(19)),
            bend: amount(index, 19, format!("{t} from {}", n[19])),
            lfo1_saw_on: present(t, n[20], wired(20)),
            lfo1_saw: amount(index, 20, format!("{t} from {}", n[20])),
            lfo1_reverse_saw_on: present(t, n[21], wired(21)),
            lfo1_reverse_saw: amount(index, 21, format!("{t} from {}", n[21])),
            lfo1_triangle_on: present(t, n[22], wired(22)),
            lfo1_triangle: amount(index, 22, format!("{t} from {}", n[22])),
            lfo1_sine_on: present(t, n[23], wired(23)),
            lfo1_sine: amount(index, 23, format!("{t} from {}", n[23])),
            lfo2_triangle_on: present(t, n[24], wired(24)),
            lfo2_triangle: amount(index, 24, format!("{t} from {}", n[24])),
        }
    }

    /// The pairs in **declared source order**, which is the order the frame and the interface use.
    ///
    /// `target` is its index, needed because each row's keyboard scope is its parameter's permanent
    /// id and those live in [`ROUTE_IDS`], keyed by target.
    pub fn routes(&self, target: usize) -> [Route<'_>; SOURCES] {
        [
            Route {
                source: SOURCE_NAMES[0],
                present: &self.key_on,
                amount: &self.key,
                present_id: ROUTE_IDS[target][0].1,
                amount_id: ROUTE_IDS[target][0].0,
            },
            Route {
                source: SOURCE_NAMES[1],
                present: &self.lfo1_on,
                amount: &self.lfo1,
                present_id: ROUTE_IDS[target][1].1,
                amount_id: ROUTE_IDS[target][1].0,
            },
            Route {
                source: SOURCE_NAMES[2],
                present: &self.lfo2_on,
                amount: &self.lfo2,
                present_id: ROUTE_IDS[target][2].1,
                amount_id: ROUTE_IDS[target][2].0,
            },
            Route {
                source: SOURCE_NAMES[3],
                present: &self.sh_on,
                amount: &self.sh,
                present_id: ROUTE_IDS[target][3].1,
                amount_id: ROUTE_IDS[target][3].0,
            },
            Route {
                source: SOURCE_NAMES[4],
                present: &self.sh_clock_on,
                amount: &self.sh_clock,
                present_id: ROUTE_IDS[target][4].1,
                amount_id: ROUTE_IDS[target][4].0,
            },
            Route {
                source: SOURCE_NAMES[5],
                present: &self.vco1_on,
                amount: &self.vco1,
                present_id: ROUTE_IDS[target][5].1,
                amount_id: ROUTE_IDS[target][5].0,
            },
            Route {
                source: SOURCE_NAMES[6],
                present: &self.vco2_on,
                amount: &self.vco2,
                present_id: ROUTE_IDS[target][6].1,
                amount_id: ROUTE_IDS[target][6].0,
            },
            Route {
                source: SOURCE_NAMES[7],
                present: &self.vco1_sync_on,
                amount: &self.vco1_sync,
                present_id: ROUTE_IDS[target][7].1,
                amount_id: ROUTE_IDS[target][7].0,
            },
            Route {
                source: SOURCE_NAMES[8],
                present: &self.vco2_sync_on,
                amount: &self.vco2_sync,
                present_id: ROUTE_IDS[target][8].1,
                amount_id: ROUTE_IDS[target][8].0,
            },
            Route {
                source: SOURCE_NAMES[9],
                present: &self.ring_on,
                amount: &self.ring,
                present_id: ROUTE_IDS[target][9].1,
                amount_id: ROUTE_IDS[target][9].0,
            },
            Route {
                source: SOURCE_NAMES[10],
                present: &self.noise_on,
                amount: &self.noise,
                present_id: ROUTE_IDS[target][10].1,
                amount_id: ROUTE_IDS[target][10].0,
            },
            Route {
                source: SOURCE_NAMES[11],
                present: &self.mixer_on,
                amount: &self.mixer,
                present_id: ROUTE_IDS[target][11].1,
                amount_id: ROUTE_IDS[target][11].0,
            },
            Route {
                source: SOURCE_NAMES[12],
                present: &self.env1_on,
                amount: &self.env1,
                present_id: ROUTE_IDS[target][12].1,
                amount_id: ROUTE_IDS[target][12].0,
            },
            Route {
                source: SOURCE_NAMES[13],
                present: &self.env2_on,
                amount: &self.env2,
                present_id: ROUTE_IDS[target][13].1,
                amount_id: ROUTE_IDS[target][13].0,
            },
            Route {
                source: SOURCE_NAMES[14],
                present: &self.gate_on,
                amount: &self.gate,
                present_id: ROUTE_IDS[target][14].1,
                amount_id: ROUTE_IDS[target][14].0,
            },
            Route {
                source: SOURCE_NAMES[15],
                present: &self.glide_on,
                amount: &self.glide,
                present_id: ROUTE_IDS[target][15].1,
                amount_id: ROUTE_IDS[target][15].0,
            },
            Route {
                source: SOURCE_NAMES[16],
                present: &self.velocity_on,
                amount: &self.velocity,
                present_id: ROUTE_IDS[target][16].1,
                amount_id: ROUTE_IDS[target][16].0,
            },
            Route {
                source: SOURCE_NAMES[17],
                present: &self.wheel_on,
                amount: &self.wheel,
                present_id: ROUTE_IDS[target][17].1,
                amount_id: ROUTE_IDS[target][17].0,
            },
            Route {
                source: SOURCE_NAMES[18],
                present: &self.pressure_on,
                amount: &self.pressure,
                present_id: ROUTE_IDS[target][18].1,
                amount_id: ROUTE_IDS[target][18].0,
            },
            Route {
                source: SOURCE_NAMES[19],
                present: &self.bend_on,
                amount: &self.bend,
                present_id: ROUTE_IDS[target][19].1,
                amount_id: ROUTE_IDS[target][19].0,
            },
            Route {
                source: SOURCE_NAMES[20],
                present: &self.lfo1_saw_on,
                amount: &self.lfo1_saw,
                present_id: ROUTE_IDS[target][20].1,
                amount_id: ROUTE_IDS[target][20].0,
            },
            Route {
                source: SOURCE_NAMES[21],
                present: &self.lfo1_reverse_saw_on,
                amount: &self.lfo1_reverse_saw,
                present_id: ROUTE_IDS[target][21].1,
                amount_id: ROUTE_IDS[target][21].0,
            },
            Route {
                source: SOURCE_NAMES[22],
                present: &self.lfo1_triangle_on,
                amount: &self.lfo1_triangle,
                present_id: ROUTE_IDS[target][22].1,
                amount_id: ROUTE_IDS[target][22].0,
            },
            Route {
                source: SOURCE_NAMES[23],
                present: &self.lfo1_sine_on,
                amount: &self.lfo1_sine,
                present_id: ROUTE_IDS[target][23].1,
                amount_id: ROUTE_IDS[target][23].0,
            },
            Route {
                source: SOURCE_NAMES[24],
                present: &self.lfo2_triangle_on,
                amount: &self.lfo2_triangle,
                present_id: ROUTE_IDS[target][24].1,
                amount_id: ROUTE_IDS[target][24].0,
            },
        ]
    }

    /// The routes a player can reach: every pair this target **offers**, in declared source order.
    /// What the stack draws and its menu offers — a refused pair is not a route.
    pub fn offered_routes(&self, target: usize) -> Vec<Route<'_>> {
        self.routes(target)
            .into_iter()
            .enumerate()
            .filter(|&(source, _)| offer(target, source) != Offer::Refused)
            .map(|(_, route)| route)
            .collect()
    }

    /// Whether each of this target's routes exists — a refused pair never. Read **once per
    /// interval**, never per sample.
    pub fn presences(&self, target: usize) -> [bool; SOURCES] {
        let routes = self.routes(target);
        std::array::from_fn(|source| {
            offer(target, source) != Offer::Refused && routes[source].is_present()
        })
    }

    /// This target's presence parameters, in declared source order.
    ///
    /// **For revealing every row in a test.** An absent route draws nothing at all — that is the
    /// interface's own rule — so a check that only ever paints the init patch covers thirteen of
    /// 433 offered pairs and silently passes over the other 420. A refused pair's presence is in
    /// here too, and setting it draws nothing: `presences` and `offered_routes` leave it out.
    pub fn presence_params(&self) -> [&BoolParam; SOURCES] {
        [
            &self.key_on,
            &self.lfo1_on,
            &self.lfo2_on,
            &self.sh_on,
            &self.sh_clock_on,
            &self.vco1_on,
            &self.vco2_on,
            &self.vco1_sync_on,
            &self.vco2_sync_on,
            &self.ring_on,
            &self.noise_on,
            &self.mixer_on,
            &self.env1_on,
            &self.env2_on,
            &self.gate_on,
            &self.glide_on,
            &self.velocity_on,
            &self.wheel_on,
            &self.pressure_on,
            &self.bend_on,
            &self.lfo1_saw_on,
            &self.lfo1_reverse_saw_on,
            &self.lfo1_triangle_on,
            &self.lfo1_sine_on,
            &self.lfo2_triangle_on,
        ]
    }

    /// A `match` rather than an array of references, and the reason is the cost gate: building a
    /// twenty-five-element array of `&FloatParam` would cost 450 pointer stores per sample across the
    /// eighteen targets, for the handful of routes that are actually live. This is branched past
    /// for a source nothing reads.
    #[inline]
    pub(crate) fn amount_param(&self, source: usize) -> &FloatParam {
        match source {
            0 => &self.key,
            1 => &self.lfo1,
            2 => &self.lfo2,
            3 => &self.sh,
            4 => &self.sh_clock,
            5 => &self.vco1,
            6 => &self.vco2,
            7 => &self.vco1_sync,
            8 => &self.vco2_sync,
            9 => &self.ring,
            10 => &self.noise,
            11 => &self.mixer,
            12 => &self.env1,
            13 => &self.env2,
            14 => &self.gate,
            15 => &self.glide,
            16 => &self.velocity,
            17 => &self.wheel,
            18 => &self.pressure,
            19 => &self.bend,
            20 => &self.lfo1_saw,
            21 => &self.lfo1_reverse_saw,
            22 => &self.lfo1_triangle,
            23 => &self.lfo1_sine,
            _ => &self.lfo2_triangle,
        }
    }

    /// Snaps a newly present route's smoother to its stored value.
    ///
    /// **An absent route's smoother is not advanced, so it must not be resumed either.** While the
    /// pair was absent nothing called `next()`, but the parameter itself stayed editable: a host
    /// automating it, or a preset load, moves the *target* and leaves the smoother's current value
    /// wherever the last live sample left it. Resuming from there ramps the route in from a stale
    /// number over a span that depends on how long it was absent — which is the accumulation
    /// across skipped spans `plan-modulation-routing.md` §6.2 forbids, because it makes a route's
    /// first audible value depend on the host's buffer sizes.
    pub fn arm(&self, newly_present: &[bool; SOURCES]) {
        for (source, &now) in newly_present.iter().enumerate() {
            if now {
                let param = self.amount_param(source);
                param.smoothed.reset(param.value());
            }
        }
    }

    /// Fills this target's amounts for the sample about to be rendered, as **fractions of the
    /// target's scale** — the scale itself is applied by the sum, not here.
    ///
    /// **Only a live route's smoother is advanced**, which is the whole of the efficiency design:
    /// an absent pair costs nothing per sample beyond the branch that skips it, and its stored
    /// depth is left exactly where the player put it so re-adding the source restores it.
    #[inline]
    pub fn advance_into(&self, live: &[bool; SOURCES], out: &mut [f32; SOURCES]) {
        for (source, &on) in live.iter().enumerate() {
            if on {
                out[source] = self.amount_param(source).smoothed.next();
            }
        }
    }

    /// Whether any live route's depth is still ramping.
    fn is_ramping(&self, live: &[bool; SOURCES]) -> bool {
        live.iter()
            .enumerate()
            .any(|(source, &on)| on && self.amount_param(source).smoothed.is_smoothing())
    }
}

/// **Only the pairs a target offers are parameters.** Written out rather than derived, because the
/// derive registers every field and the modulation standard refuses seventeen pairs: their fields
/// exist in memory, and no host, state or preset ever sees them. Everything else is what the derive
/// produced — the same ids, in the same order.
unsafe impl Params for TargetRoutes {
    fn param_map(&self) -> Vec<(String, ParamPtr, String)> {
        let presences = self.presence_params();
        let mut map = Vec::with_capacity(SOURCES * 2);
        for (source, (presence, amount)) in SHORT_IDS.iter().enumerate() {
            if offer(self.target, source) == Offer::Refused {
                continue;
            }
            map.push((
                (*presence).to_owned(),
                presences[source].as_ptr(),
                String::new(),
            ));
            map.push((
                (*amount).to_owned(),
                self.amount_param(source).as_ptr(),
                String::new(),
            ));
        }
        map
    }
}

/// All eighteen targets' routes.
#[derive(Params)]
pub struct Routes {
    #[nested(id_prefix = "mod_lfo1rate", group = "Modulation - LFO 1 rate")]
    pub lfo1_rate: TargetRoutes,
    #[nested(id_prefix = "mod_lfo2rate", group = "Modulation - LFO 2 rate")]
    pub lfo2_rate: TargetRoutes,
    #[nested(id_prefix = "mod_shsrc", group = "Modulation - Sample and hold input")]
    pub sh_input: TargetRoutes,
    #[nested(id_prefix = "mod_vco1pitch", group = "Modulation - Oscillator 1 pitch")]
    pub vco1_pitch: TargetRoutes,
    #[nested(id_prefix = "mod_vco2sync", group = "Modulation - Oscillator 2 sync")]
    pub vco2_sync: TargetRoutes,
    #[nested(id_prefix = "mod_ringin", group = "Modulation - Ring mod")]
    pub ring_input: TargetRoutes,
    #[nested(id_prefix = "mod_mixin", group = "Modulation - Mix")]
    pub mixer_input: TargetRoutes,
    #[nested(id_prefix = "mod_vcfgate", group = "Modulation - Filter envelope gate")]
    pub vcf_gate: TargetRoutes,
    #[nested(id_prefix = "mod_vcacv", group = "Modulation - VCA level")]
    pub amplifier: TargetRoutes,
    #[nested(
        id_prefix = "mod_vcagate",
        group = "Modulation - Amplifier envelope gate"
    )]
    pub vca_gate: TargetRoutes,
    #[nested(id_prefix = "mod_phlfo", group = "Modulation - Phaser rate")]
    pub phaser_rate: TargetRoutes,
    #[nested(id_prefix = "mod_phman", group = "Modulation - Phaser centre")]
    pub phaser_centre: TargetRoutes,
    #[nested(id_prefix = "mod_vco2pitch", group = "Modulation - Oscillator 2 pitch")]
    pub vco2_pitch: TargetRoutes,
    #[nested(id_prefix = "mod_pw1", group = "Modulation - Oscillator 1 pulse width")]
    pub vco1_width: TargetRoutes,
    #[nested(id_prefix = "mod_pw2", group = "Modulation - Oscillator 2 pulse width")]
    pub vco2_width: TargetRoutes,
    #[nested(id_prefix = "mod_trem", group = "Modulation - Tremolo")]
    pub tremolo: TargetRoutes,
    #[nested(id_prefix = "mod_cutoff", group = "Modulation - Cutoff")]
    pub cutoff: TargetRoutes,
    /// The collection's standard Amplitude, after the VCA (the modulation standard).
    #[nested(id_prefix = "mod_amplitude", group = "Modulation - Amplitude")]
    pub amplitude: TargetRoutes,
}

impl Default for Routes {
    fn default() -> Self {
        Self::new()
    }
}

impl Routes {
    /// The init patch: **the plug-out's own thirteen connections, and nothing else.**
    ///
    /// Nine of them inherit the zero their control held — VCO-1's pitch input from VCO-2, the
    /// filter's two inputs, and the panel's wiring:
    /// the glide and LFO 1 on both pitch inputs (DESTINATION's default, both oscillators), LFO 1 on
    /// the tremolo and the keyboard on the cutoff. Four had no attenuator to inherit from and start
    /// at full: both envelope gates from the keyboard gate, the ring modulator's input from VCO-1,
    /// and the amplifier's input from Envelope 2, whose depth already started at 1.0 before the
    /// conversion.
    ///
    /// **It is the same sound it always was**, which
    /// `crates/mxm-mono-00-dsp/tests/conversion.rs` measures rather than asserts.
    pub fn new() -> Self {
        Self {
            lfo1_rate: TargetRoutes::new(target::LFO1_RATE),
            lfo2_rate: TargetRoutes::new(target::LFO2_RATE),
            sh_input: TargetRoutes::new(target::SH_INPUT),
            vco1_pitch: TargetRoutes::new(target::VCO1_PITCH),
            vco2_sync: TargetRoutes::new(target::VCO2_SYNC),
            ring_input: TargetRoutes::new(target::RING_INPUT),
            mixer_input: TargetRoutes::new(target::MIXER_INPUT),
            vcf_gate: TargetRoutes::new(target::VCF_GATE),
            amplifier: TargetRoutes::new(target::AMPLIFIER),
            vca_gate: TargetRoutes::new(target::VCA_GATE),
            phaser_rate: TargetRoutes::new(target::PHASER_RATE),
            phaser_centre: TargetRoutes::new(target::PHASER_CENTRE),
            vco2_pitch: TargetRoutes::new(target::VCO2_PITCH),
            vco1_width: TargetRoutes::new(target::VCO1_WIDTH),
            vco2_width: TargetRoutes::new(target::VCO2_WIDTH),
            tremolo: TargetRoutes::new(target::TREMOLO),
            cutoff: TargetRoutes::new(target::CUTOFF),
            amplitude: TargetRoutes::new(target::AMPLITUDE),
        }
    }

    /// Every target, in declared target order.
    pub fn all(&self) -> [(usize, &TargetRoutes); TARGETS] {
        [
            (target::LFO1_RATE, &self.lfo1_rate),
            (target::LFO2_RATE, &self.lfo2_rate),
            (target::SH_INPUT, &self.sh_input),
            (target::VCO1_PITCH, &self.vco1_pitch),
            (target::VCO2_SYNC, &self.vco2_sync),
            (target::RING_INPUT, &self.ring_input),
            (target::MIXER_INPUT, &self.mixer_input),
            (target::VCF_GATE, &self.vcf_gate),
            (target::AMPLIFIER, &self.amplifier),
            (target::VCA_GATE, &self.vca_gate),
            (target::PHASER_RATE, &self.phaser_rate),
            (target::PHASER_CENTRE, &self.phaser_centre),
            (target::VCO2_PITCH, &self.vco2_pitch),
            (target::VCO1_WIDTH, &self.vco1_width),
            (target::VCO2_WIDTH, &self.vco2_width),
            (target::TREMOLO, &self.tremolo),
            (target::CUTOFF, &self.cutoff),
            (target::AMPLITUDE, &self.amplitude),
        ]
    }

    /// Which routes are live, for the whole instrument. **Once per interval.**
    pub fn topology(&self) -> Routing {
        let mut routing = Routing::new();
        for (index, group) in self.all() {
            routing.present[index] = group.presences(index);
        }
        routing
    }

    /// The topology for this interval, with every **newly present** route's smoother snapped to
    /// its stored value.
    ///
    /// `previous` is the topology the last interval ran, so the caller keeps it across buffers.
    /// See [`TargetRoutes::arm`] for why resuming a skipped smoother is wrong.
    pub fn topology_from(&self, previous: &Routing) -> Routing {
        let mut routing = self.topology();
        // **A removed route keeps the depth it last had.** The DSP ramps a removed audio route out
        // over `AUDIO_ROUTE_RAMP_S` and multiplies by its amount while it does; starting from zero
        // cut it dead on the next block — the click the ramp exists to prevent (code review,
        // 2026-09-22). Present routes are overwritten every sample by `advance_into`.
        routing.amounts = previous.amounts;
        for (index, group) in self.all() {
            let mut newly = [false; SOURCES];
            for (slot, (&now, &before)) in newly.iter_mut().zip(
                routing.present[index]
                    .iter()
                    .zip(previous.present[index].iter()),
            ) {
                *slot = now && !before;
            }
            group.arm(&newly);
        }
        routing
    }

    /// Fills this sample's amounts into an already-topologised [`Routing`].
    #[inline]
    pub fn advance_into(&self, routing: &mut Routing) {
        for (index, group) in self.all() {
            let live = routing.present[index];
            group.advance_into(&live, &mut routing.amounts[index]);
        }
    }

    /// **Whether any live route's depth is still on its way.** The DSP judges what a route can
    /// reach — a gate's threshold, above all — from the depth it has *now*, and a depth only
    /// moves while the host keeps calling. A block that ends mid-ramp must not report a verdict
    /// the target depth would overturn, or a host that sleeps the plugin parks the ramp short of
    /// it (code review, 2026-09-22).
    pub fn is_ramping(&self, routing: &Routing) -> bool {
        self.all()
            .into_iter()
            .any(|(index, group)| group.is_ramping(&routing.present[index]))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    use mxm_plugin_test::routing_checks;

    /// **Every route parameter says what the DSP does** — the modulation standard's plugin half:
    /// a pair has a registered parameter exactly where it is offered (the seventeen refused pairs
    /// nowhere), its travel is its offer's, its reading carries its target's unit and states what
    /// `mxm_mono_00_dsp::conformance` measures the graph delivering, and every reading survives the
    /// host's round trip.
    ///
    /// Falsified before trusted: with every pair read on its row's own scale, it names the eighteen
    /// the standard re-reached — the performance pairs into both pitches and both LFO rates, and Key
    /// into both pitches; with every fader two-sided, the thirty one-sided and sync pairs.
    #[test]
    fn every_route_parameter_says_what_the_dsp_does() {
        let routes = Routes::new();
        let registered: std::collections::BTreeSet<String> = routes
            .param_map()
            .into_iter()
            .map(|(id, _, _)| id)
            .collect();
        let groups = routes.all();
        if let Err(failures) =
            routing_checks::amounts(&mxm_mono_00_dsp::conformance::Declared, |target, source| {
                registered
                    .contains(ROUTE_IDS[target][source].0)
                    .then(|| groups[target].1.amount_param(source))
            })
        {
            panic!("{} failure(s):\n{}", failures.len(), failures.join("\n"));
        }
    }

    /// [`ROUTE_IDS`] is a hand-written mirror of what the derive emits, and the two have to agree
    /// or every preset, control-map entry and keyboard scope that reads the table is pointing at
    /// nothing. This is the test that holds them together.
    #[test]
    fn the_id_table_is_what_the_derive_actually_produces() {
        let routes = Routes::new();
        let emitted: Vec<String> = routes
            .param_map()
            .into_iter()
            .map(|(id, _, _)| id)
            .collect();
        let refused = (0..TARGETS)
            .flat_map(|t| (0..SOURCES).map(move |s| (t, s)))
            .filter(|&(t, s)| offer(t, s) == Offer::Refused)
            .count();
        assert_eq!(refused, 17, "the standard refuses seventeen pairs here");
        assert_eq!(
            emitted.len(),
            (TARGETS * SOURCES - refused) * 2,
            "one presence and one amount per offered pair"
        );
        for (t, row) in ROUTE_IDS.iter().enumerate() {
            for (s, (amount, present)) in row.iter().enumerate() {
                if offer(t, s) == Offer::Refused {
                    continue;
                }
                assert!(
                    emitted.iter().any(|id| id == amount),
                    "{amount} is in the table but not in the derive"
                );
                assert!(
                    emitted.iter().any(|id| id == present),
                    "{present} is in the table but not in the derive"
                );
            }
        }
        let mut seen: Vec<&str> = ROUTE_IDS
            .iter()
            .flat_map(|target| target.iter())
            .flat_map(|(a, p)| [*a, *p])
            .collect();
        seen.sort_unstable();
        let before = seen.len();
        seen.dedup();
        assert_eq!(before, seen.len(), "a permanent id is duplicated");
    }

    /// **No retired id came back.** Every one named in this module's doc is gone from the routing
    /// surface — the twenty-three of the patch-bay conversion, the eleven panel controls of Rev 3,
    /// and the 200 route ids of the five targets that retired or whose meaning changed — so nothing can quietly
    /// re-use a name whose meaning changed.
    #[test]
    fn no_retired_id_reappears_among_the_routing_parameters() {
        let routes = Routes::new();
        let emitted: Vec<String> = routes
            .param_map()
            .into_iter()
            .map(|(id, _, _)| id)
            .collect();
        // **The modulation standard's thirty-four** (2026-09-27): a gesture into either envelope
        // gate, the sync input or the mixer's audio input, and Velocity into the S&H.
        let standard_retired = [
            "mod_shsrc_vel",
            "mod_vco2sync_vel",
            "mod_vco2sync_wheel",
            "mod_vco2sync_press",
            "mod_vco2sync_bend",
            "mod_mixin_vel",
            "mod_mixin_wheel",
            "mod_mixin_press",
            "mod_mixin_bend",
            "mod_vcfgate_vel",
            "mod_vcfgate_wheel",
            "mod_vcfgate_press",
            "mod_vcfgate_bend",
            "mod_vcagate_vel",
            "mod_vcagate_wheel",
            "mod_vcagate_press",
            "mod_vcagate_bend",
        ];
        for amount in standard_retired {
            for id in [amount.to_owned(), format!("{amount}on")] {
                assert!(
                    !emitted.contains(&id),
                    "{id} retired with the modulation standard and came back"
                );
            }
        }
        for prefix in [
            "mod_glidein_",
            "mod_vco1cv_",
            "mod_shin_",
            "mod_vcfenv_",
            "mod_vcflfo_",
        ] {
            assert!(
                !emitted.iter().any(|id| id.starts_with(prefix)),
                "a {prefix} id retired and came back"
            );
        }
        let whole = crate::params::MxmMono00Params::default();
        let all: Vec<String> = whole.param_map().into_iter().map(|(id, _, _)| id).collect();
        for retired in [
            "glide",
            "glidedest",
            "vcolfo",
            "vcolfodest",
            "pwmsrc1",
            "pwm1",
            "pwmsrc2",
            "pwm2",
            "vcalfo",
            "keytrack",
            "shmode",
        ] {
            assert!(
                !all.iter().any(|id| id == retired),
                "{retired} retired with the panel's wiring and came back"
            );
        }
        for retired in [
            "lfo1cvin",
            "lfo2cvin",
            "shextin",
            "glidein",
            "vco1cvin",
            "vco2syncin",
            "ringmodin",
            "mixerextin",
            "vcflfoin",
            "vcfadsrin",
            "vcfgatein",
            "vcaadsrin",
            "vcagatein",
            "phaserlfoin",
            "phasermanualin",
            "lfo1gain",
            "lfo2gain",
            "extcv1",
            "ringlevel",
            "filterenv",
            "filterlfo",
            "vcaenv",
            "sync",
        ] {
            assert!(
                !emitted.iter().any(|id| id == retired),
                "{retired} retired and came back"
            );
        }
    }

    /// Init wires exactly what the DSP declares. The connections that never had an attenuator start
    /// at full, and so does the absent sync normal, whose presence is the SYNC switch; every other
    /// amount starts at zero.
    #[test]
    fn init_wires_the_plug_outs_own_connections_at_the_declared_depths() {
        let routes = Routes::new();
        assert_eq!(routes.topology().present, Routing::init().present);
        for (index, group) in routes.all() {
            for (source, route) in group.routes(index).iter().enumerate() {
                let wired = INIT_PRESENT.contains(&(index, source))
                    || INIT_AT_FULL.contains(&(index, source));
                assert_eq!(
                    route.is_present(),
                    wired,
                    "{}: {} presence",
                    TARGET_NAMES[index],
                    SOURCE_NAMES[source]
                );
                let want =
                    if INIT_AT_FULL.contains(&(index, source)) || (index, source) == SYNC_NORMAL {
                        1.0
                    } else {
                        0.0
                    };
                assert_eq!(
                    group.amount_param(source).default_plain_value(),
                    want,
                    "{}: {} depth",
                    TARGET_NAMES[index],
                    SOURCE_NAMES[source]
                );
            }
        }
    }

    /// **A route's amount reads what that pair actually delivers**, which is the number the
    /// retired knob meant — not the number a unit-magnitude source would get.
    ///
    /// `TARGET_SCALE` is per *unit of source* and this machine's sources do not all fill the
    /// ten-volt unit: an envelope peaks at 0.6 and a VCO at 0.5. Reading a route as
    /// `scale` alone made the filter's envelope route say `+11.67 oct` where it sweeps seven —
    /// `FILTER_ENV_OCTAVES`, and exactly what `filterenv` used to mean.
    ///
    /// **Falsified**: dropping `SOURCE_PEAK` from `reach` turns every assertion below red.
    #[test]
    fn a_route_at_full_amount_reads_the_machines_own_number() {
        use mxm_mono_00_dsp::routing::PITCH_SEMITONES_PER_UNIT;
        use mxm_mono_00_dsp::routing::source;
        use mxm_mono_00_dsp::voice::{FILTER_ENV_OCTAVES, FILTER_LFO_OCTAVES, VCO_OUT_UNITS};
        let routes = Routes::new();
        let text = |target: usize, s: usize| {
            let group = routes.all()[target].1;
            group.amount_param(s).preview_normalized(1.0);
            group.amount_param(s).normalized_value_to_string(1.0, false)
        };

        // The filter's two inputs, at the reaches the retired knobs named.
        assert_eq!(
            text(target::CUTOFF, source::VCF_ADSR),
            format!("{:+.2} oct", FILTER_ENV_OCTAVES)
        );
        assert_eq!(
            text(target::CUTOFF, source::LFO1),
            format!("{:+.2} oct", FILTER_LFO_OCTAVES)
        );
        // VCO-1's modulator input from VCO-2: half the pitch inputs' 144 semitones, a VCO being
        // ±0.5 of a unit.
        assert_eq!(
            text(target::VCO1_PITCH, source::VCO2),
            format!("{:+.2} st", PITCH_SEMITONES_PER_UNIT * VCO_OUT_UNITS)
        );
        // The panel's wiring, at full: an LFO or the glide on either pitch input reaches all 144;
        // the PWM section narrows by 27 % from the LFO triangle and 45 % from an envelope, as its
        // slider did at the top; a tremolo takes the whole gain; key follow is 100 % tracking.
        for pitch in [target::VCO1_PITCH, target::VCO2_PITCH] {
            assert_eq!(text(pitch, source::LFO1), "+144.00 st");
            assert_eq!(text(pitch, source::GLIDE), "+144.00 st");
        }
        assert_eq!(
            text(target::VCO1_WIDTH, source::LFO1_CORE_TRIANGLE),
            "+27 %"
        );
        assert_eq!(text(target::VCO2_WIDTH, source::VCF_ADSR), "+45 %");
        assert_eq!(text(target::TREMOLO, source::LFO1), "+100 %");
        assert_eq!(text(target::CUTOFF, source::KEY), "+1.00 oct/oct");
        // The amplifier at full opens fully, and the mixer's channel carries its source at unity.
        assert_eq!(text(target::AMPLIFIER, source::VCA_ADSR), "+100 %");
        assert_eq!(text(target::MIXER_INPUT, source::RING_MOD), "+100 %");
        // And a sync route's amount is reset depth, so full is the hardware's own reset.
        assert_eq!(text(target::VCO2_SYNC, source::VCO1_SYNC), "+100 %");
    }

    /// **The pitch faders are square-law about the centre**: a tenth of the travel either side
    /// is a vibrato's depth, not a twelfth of an octave, and the far end is still the whole reach.
    /// The owner's ruling (2026-09-22); **falsified** by making the range linear, which puts
    /// `+14.40 st` a tenth of the way out.
    #[test]
    fn a_tenth_of_a_pitch_faders_travel_is_a_vibrato() {
        use mxm_mono_00_dsp::routing::source;
        let routes = Routes::new();
        for (index, group) in [
            (target::VCO1_PITCH, &routes.vco1_pitch),
            (target::VCO2_PITCH, &routes.vco2_pitch),
        ] {
            let lfo = group.amount_param(source::LFO1);
            assert_eq!(lfo.normalized_value_to_string(0.55, false), "+1.44 st");
            assert_eq!(lfo.normalized_value_to_string(0.45, false), "-1.44 st");
            assert_eq!(lfo.normalized_value_to_string(1.0, false), "+144.00 st");
            assert_eq!(lfo.normalized_value_to_string(0.5, false), "+0.00 st");
            // The old Vibrato knob's whole twelve semitones sits under a third of the way out.
            assert!(
                lfo.preview_normalized(12.0 / 144.0) < 0.5 + 0.5 * 0.3,
                "{index}"
            );
        }
        // Every other input's fader stays linear.
        let lfo = routes.lfo1_rate.amount_param(source::LFO2);
        assert_eq!(lfo.normalized_value_to_string(0.75, false), "+5.00 oct");
    }

    /// **Every reading survives the host's round trip, a rounded zero included**: printed, parsed and
    /// printed again, it is the same text — at amounts either side of zero, not only the round numbers.
    /// A plain signed format printed `-0 %` there, which parses to zero and prints `+0 %`, and
    /// `clap-validator`'s `param-conversions` fails on that whenever its random values land in the
    /// sliver (`mxm_modulation_params::signed`).
    #[test]
    fn every_reading_survives_the_hosts_round_trip_a_rounded_zero_included() {
        let routes = Routes::new();
        let check = |param: &FloatParam| {
            for normalised in [
                0.0f32, 0.25, 0.4999, 0.49999, 0.5, 0.50001, 0.5001, 0.75, 1.0,
            ] {
                let text = param.normalized_value_to_string(normalised, true);
                let back = param
                    .string_to_normalized_value(&text)
                    .unwrap_or_else(|| panic!("{}: {text} does not parse", param.name()));
                assert_eq!(
                    text,
                    param.normalized_value_to_string(back, true),
                    "{} at {normalised}",
                    param.name()
                );
            }
        };
        for (_, group) in routes.all() {
            for source in 0..SOURCES {
                check(group.amount_param(source));
            }
        }
    }

    /// **A control-map role never points at a dead route.**
    ///
    /// An absent pair contributes nothing whatever its amount holds, so a controller knob bound to
    /// the *amount* of a route the machine does not wire does nothing at all — `mxm-mono-pr1` paid
    /// for that first (`plan-modulation-routing.md` decision 1.8). A role may take an amount only
    /// where Init wires the route.
    ///
    /// **A *presence* is the exception and is never dead**: turning it on is what creates the
    /// route, which is exactly what `osc2.sync`'s retired switch did.
    ///
    /// **Falsified**: pointing `filter.env_amount` at a pair Init leaves unwired turns it red.
    #[test]
    fn a_control_map_role_never_points_at_a_dead_route() {
        let text = include_str!("../control-map.json");
        let routes = Routes::new();

        // Every routing id the map names, with whether it is a presence.
        let mut checked = 0;
        for target in ROUTE_IDS.iter() {
            for (amount, present) in target.iter() {
                // A presence may be wired or not; only an amount has to be live.
                if text.contains(&format!("\"{present}\"")) {
                    checked += 1;
                }
                if text.contains(&format!("\"{amount}\"")) {
                    checked += 1;
                    let wired = routes
                        .all()
                        .iter()
                        .flat_map(|(t, g)| {
                            g.routes(*t)
                                .into_iter()
                                .zip(ROUTE_IDS[*t].iter())
                                .map(|(r, ids)| (ids.0, r.is_present()))
                                .collect::<Vec<_>>()
                        })
                        .find(|(id, _)| id == amount)
                        .map(|(_, present)| present)
                        .unwrap_or(false);
                    assert!(
                        wired,
                        "the control map binds a role to {amount}, which Init does not wire: \
                         a knob on it would do nothing"
                    );
                }
            }
        }
        // **Five ids, six roles**: `filter.lfo_amount` and `lfo1.to_filter` both name the
        // filter's LFO route, which is the same control reached two ways. `mixer.src3` is the ring
        // modulator's own level slider now, not a route.
        assert_eq!(
            checked, 5,
            "the map names {checked} routing ids; it named five when this was written, so either \
             the map or this check has drifted"
        );
    }

    /// **The controller's sync switch syncs.** `osc2.sync` writes the sync normal's presence alone,
    /// so the amount it finds has to be the hardware's full reset — and Init must still wire
    /// nothing. **Falsified** by dropping the `SYNC_NORMAL` clause from `amount()`.
    #[test]
    fn the_sync_switch_role_turns_on_a_full_reset() {
        use nice_plug::params::Param;
        let text = include_str!("../control-map.json");
        assert!(text.contains(r#""osc2.sync": "mod_vco2sync_vco1syon""#));
        let routes = Routes::new();
        let group = &routes.vco2_sync;
        assert!(
            !group.vco1_sync_on.value(),
            "a fresh instance does not sync"
        );
        assert_eq!(group.vco1_sync.default_plain_value(), 1.0);
        assert!(
            !routes.topology().present[target::VCO2_SYNC][SYNC_NORMAL.1],
            "and the topology agrees"
        );
    }

    /// **A live route's ramping depth keeps the plugin called**; an absent one's does not, because
    /// nothing advances it. **Falsified** by returning `false` from `TargetRoutes::is_ramping`.
    #[test]
    fn a_live_route_still_ramping_is_reported() {
        let routes = Routes::new();
        let topology = routes.topology();
        assert!(!routes.is_ramping(&topology), "at rest");

        // Amplitude ← Envelope 2 is wired at Init.
        routes.amplifier.env2.smoothed.set_target(48_000.0, 0.5);
        assert!(routes.is_ramping(&topology), "a live depth on its way");
        routes.amplifier.env2.smoothed.reset(0.5);
        assert!(!routes.is_ramping(&topology), "and arrived");

        // Oscillator 2 sync ← Oscillator 1 sync is absent at Init.
        routes
            .vco2_sync
            .vco1_sync
            .smoothed
            .set_target(48_000.0, 0.5);
        assert!(
            !routes.is_ramping(&topology),
            "an absent route's ramp is not the patch's"
        );
    }

    /// **Osc 1 Tune is Oscillator 1's own tune**, as Osc 2 Tune is Oscillator 2's. Before VCO-1
    /// had `coarse1` the role took master tune, which moves both oscillators and so can never
    /// change the interval the ring modulator and sync turn into timbre (code review,
    /// 2026-09-22). **Falsified** by pointing the role back at `tune`.
    #[test]
    fn each_oscillator_tune_role_moves_that_oscillator_alone() {
        let text = include_str!("../control-map.json");
        assert!(text.contains(r#""osc1.tune": "coarse1""#));
        assert!(text.contains(r#""osc2.tune": "coarse2""#));
    }

    /// **A removed route keeps its depth while the DSP fades it out.** The topology used to start
    /// from zero amounts, so a removed audio route was multiplied by zero on the next block while
    /// its fade gain was still near one: a click (code review, 2026-09-22). **Falsified** by
    /// dropping the carry in `topology_from`.
    #[test]
    fn a_removed_route_keeps_its_depth_for_the_fade() {
        let routes = Routes::new();
        let mut previous = routes.topology();
        previous.set(target::MIXER_INPUT, 10, 0.7);
        let next = routes.topology_from(&previous);
        assert!(!next.present[target::MIXER_INPUT][10], "the route is gone");
        assert_eq!(
            next.amounts[target::MIXER_INPUT][10],
            0.7,
            "its depth stayed for the fade"
        );
    }

    /// **Removing a route leaves its depth alone**, which is what makes re-adding it restore what
    /// the player set — and it is why a topology read must never look at an amount.
    #[test]
    fn a_topology_read_ignores_the_amounts_entirely() {
        let routes = Routes::new();
        let group = &routes.cutoff;
        // LFO 1 is wired to the cutoff at zero depth in the init patch: still a route.
        assert!(group.lfo1_on.value());
        assert_eq!(group.lfo1.default_plain_value(), 0.0);
        assert!(routes.topology().present[target::CUTOFF][1]);
    }
}
