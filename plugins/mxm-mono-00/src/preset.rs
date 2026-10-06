//! Presets: this instrument's factory set, and what the collection's preset crate needs of it.
//!
//! The format, the library on disk, favourites, the loaded identity and the app-bar controls are
//! `mxm-preset`'s — one crate for every instrument and effect, extracted from the five verbatim
//! copies this file used to be one of (`plugins/AGENTS.md`, *A preset is parameter values*). What
//! is left here is what only this instrument knows: its id, its parameters, and its sounds.

use std::sync::RwLock;

pub use mxm_preset::{
    Category, Entry, INIT_NAME, Library, Loaded, Origin, Preset, PresetIdentity, Refused, Value,
    factory, loaded, mark_loaded, mark_none, read_favourites, snapshot, write_favourites,
};

use crate::params::MxmMono00Params;

/// **The tempo syncs this plugin gained on 2026-09-25** (`plans/plan-tempo-sync-controls.md`). A
/// preset file written before them was written unsynced, so each loads off rather than keeping the
/// instance's sync, and without reporting a missing control.
pub(crate) const TEMPO_SYNC_IDS: &[&str] = &["shsync", "lfo1sync", "lfo2sync"];

impl mxm_preset::Instrument for MxmMono00Params {
    fn clap_id(&self) -> &'static str {
        crate::CLAP_ID
    }

    /// In declaration order, from the one list the editor draws from.
    fn parameters(&self) -> Vec<(&'static str, &dyn mxm_preset::ErasedParam)> {
        let mut out: Vec<(&'static str, &dyn mxm_preset::ErasedParam)> =
            crate::editor::sections::all_parameters(self)
                .into_iter()
                .map(|bound| (bound.id, bound.param))
                .collect();
        // **Every routing pair**, in `[target][source]` order. Appended rather than interleaved so
        // the instrument's own controls keep the positions presets already wrote them in — and
        // included at all because a preset that did not carry the patch bay would load a sound and
        // leave the routing wherever the last one put it.
        for (target, group) in self.routes.all() {
            for (slot, route) in group.routes(target).into_iter().enumerate() {
                // A refused pair is no parameter (`routes.rs`), so no preset carries it.
                if mxm_mono_00_dsp::routing::offer(target, slot)
                    == mxm_modulation::standard::Offer::Refused
                {
                    continue;
                }
                let (amount_id, present_id) = crate::routes::ROUTE_IDS[target][slot];
                out.push((present_id, route.present));
                out.push((amount_id, route.amount));
            }
        }
        out
    }

    fn identity(&self) -> &RwLock<PresetIdentity> {
        &self.preset
    }

    fn factory_files(&self) -> &'static [(&'static str, &'static str)] {
        FACTORY_FILES
    }

    fn default_missing_legacy_parameter(&self, id: &str) -> bool {
        TEMPO_SYNC_IDS.contains(&id)
    }
}

/// The factory set, compiled in.
///
/// **Files, and Init is not one of them** — see [`Preset::init`]. Generated from `FACTORY_DESIGN`
/// in the test module by `write_the_factory_presets`.
pub const FACTORY_FILES: &[(&str, &str)] = &[
    ("Init saw", include_str!("../presets/init-saw.json")),
    ("Two saws", include_str!("../presets/two-saws.json")),
    ("Fifth lead", include_str!("../presets/fifth-lead.json")),
    ("Sync sweep", include_str!("../presets/sync-sweep.json")),
    ("Weak sync", include_str!("../presets/weak-sync.json")),
    ("Bell ring", include_str!("../presets/bell-ring.json")),
    ("Metal ring", include_str!("../presets/metal-ring.json")),
    ("Cross mod", include_str!("../presets/cross-mod.json")),
    ("Pink wash", include_str!("../presets/pink-wash.json")),
    ("Wind", include_str!("../presets/wind.json")),
    ("Squelch bass", include_str!("../presets/squelch-bass.json")),
    ("Round bass", include_str!("../presets/round-bass.json")),
    (
        "Sample and hold",
        include_str!("../presets/sample-and-hold.json"),
    ),
    ("Self running", include_str!("../presets/self-running.json")),
    (
        "Clocked filter",
        include_str!("../presets/clocked-filter.json"),
    ),
    ("Drone", include_str!("../presets/drone.json")),
    ("Vibrato lead", include_str!("../presets/vibrato-lead.json")),
    ("Phased pad", include_str!("../presets/phased-pad.json")),
    ("Echo pluck", include_str!("../presets/echo-pluck.json")),
    ("Spring drip", include_str!("../presets/spring-drip.json")),
    ("Octave bass", include_str!("../presets/octave-bass.json")),
    (
        "Sub pulse bass",
        include_str!("../presets/sub-pulse-bass.json"),
    ),
    ("Rubber bass", include_str!("../presets/rubber-bass.json")),
    ("Glide bass", include_str!("../presets/glide-bass.json")),
    ("Ring bass", include_str!("../presets/ring-bass.json")),
    ("Pulse lead", include_str!("../presets/pulse-lead.json")),
    ("Octave lead", include_str!("../presets/octave-lead.json")),
    (
        "Hard sync lead",
        include_str!("../presets/hard-sync-lead.json"),
    ),
    (
        "Soft triangle lead",
        include_str!("../presets/soft-triangle-lead.json"),
    ),
    (
        "Screaming lead",
        include_str!("../presets/screaming-lead.json"),
    ),
    ("Detuned pad", include_str!("../presets/detuned-pad.json")),
    ("Triangle pad", include_str!("../presets/triangle-pad.json")),
    (
        "Breathing pad",
        include_str!("../presets/breathing-pad.json"),
    ),
    ("PWM pad", include_str!("../presets/pwm-pad.json")),
    ("Brass", include_str!("../presets/brass.json")),
    (
        "Electric piano",
        include_str!("../presets/electric-piano.json"),
    ),
    ("Clav", include_str!("../presets/clav.json")),
    ("Glass keys", include_str!("../presets/glass-keys.json")),
    ("Marimba", include_str!("../presets/marimba.json")),
    ("Noise snare", include_str!("../presets/noise-snare.json")),
    ("Hat", include_str!("../presets/hat.json")),
    ("Thunder", include_str!("../presets/thunder.json")),
    ("Random steps", include_str!("../presets/random-steps.json")),
    ("LFO arpeggio", include_str!("../presets/lfo-arpeggio.json")),
    ("Ring drone", include_str!("../presets/ring-drone.json")),
    ("Phaser sweep", include_str!("../presets/phaser-sweep.json")),
    ("Delay stabs", include_str!("../presets/delay-stabs.json")),
    ("Cathedral", include_str!("../presets/cathedral.json")),
    ("Strings", include_str!("../presets/strings.json")),
    (
        "Talking filter",
        include_str!("../presets/talking-filter.json"),
    ),
];

#[cfg(test)]
mod tests {
    use super::*;

    /// **A project saved before the tempo syncs restores them Off** (`mxm_preset::add_switches_off`),
    /// whatever this instance had.
    #[test]
    fn an_older_state_restores_the_tempo_syncs_off() {
        use nice_plug::prelude::Plugin as _;
        let mut state = nice_plug::prelude::PluginState {
            version: String::new(),
            params: Default::default(),
            fields: Default::default(),
        };
        crate::MxmMono00::filter_state(&mut state);
        for id in TEMPO_SYNC_IDS {
            assert!(
                matches!(
                    state.params.get(*id),
                    Some(nice_plug::plugin::ParamValue::Bool(false))
                ),
                "{{id}} was not restored off"
            );
        }
    }

    /// **A preset saved before the tempo syncs loads them off, and cleanly** ([`TEMPO_SYNC_IDS`]).
    #[test]
    fn a_preset_from_before_the_tempo_syncs_loads_them_off() {
        let params = crate::params::MxmMono00Params::default();
        let mut old = mxm_preset::Preset::init(&params);
        for id in TEMPO_SYNC_IDS {
            old.params.remove(*id);
        }
        let (writes, problems) = old.resolve(&params);
        assert!(problems.is_empty(), "{{problems:?}}");
        for id in TEMPO_SYNC_IDS {
            assert!(
                writes.iter().any(|(w, _, v)| w == id && *v == 0.0),
                "{{id}} was not written off"
            );
        }
    }

    use mxm_preset::user_root;

    fn params() -> MxmMono00Params {
        MxmMono00Params::default()
    }

    /// Prints every parameter as eleven `normalised=formatted` steps.
    ///
    /// A facility, not a test: designing a factory preset means choosing normalised values, and
    /// choosing them blind is how a preset ends up with a filter at 0.5 that nobody meant.
    ///
    /// ```text
    /// cargo test -p mxm-mono-00 --lib the_mapping_table -- --ignored --nocapture
    /// ```
    #[test]
    #[ignore = "prints what each normalised value means, for preset design"]
    fn the_mapping_table() {
        let params = params();
        for bound in crate::editor::sections::all_parameters(&params) {
            let steps: Vec<String> = (0..=10)
                .map(|i| {
                    let v = i as f32 / 10.0;
                    format!("{v:.1}={}", bound.param.format(v))
                })
                .collect();
            eprintln!("{:<12} {}", bound.id, steps.join("  "));
        }
    }

    /// The factory sounds, as **overrides on the defaults**.
    ///
    /// Written as the handful of values that make each sound rather than as eighty numbers apiece.
    /// `write_the_factory_presets` turns each into a complete file, because the format takes no
    /// sparse overlays. **Normalised values**: `the_mapping_table` prints what each means.
    ///
    /// **Twenty, and Init is not one of them.** Every design leaves the output up and a source
    /// audible; `every_factory_preset_can_be_heard` checks that from the files.
    const FACTORY_DESIGN: &[Design] = &[
        // ---- the oscillators ----
        (
            "Init saw",
            Category::Template,
            &[("cutoff", 0.62), ("vcfrelease", 0.42)],
        ),
        (
            "Two saws",
            Category::Lead,
            &[
                ("vco2level", 0.8),
                ("fine2", 0.56),
                ("cutoff", 0.7),
                ("mod_cutoff_env1", 0.65),
                ("vcfdecay", 0.5),
            ],
        ),
        (
            "Fifth lead",
            Category::Lead,
            &[
                ("vco2level", 0.75),
                ("coarse2", 0.6458333),
                ("wave2", 0.5),
                ("cutoff", 0.66),
                ("resonance", 0.3),
                ("mod_cutoff_env1", 0.725),
                ("vcfdecay", 0.55),
                ("mod_vco1pitch_lfo1", 0.535355),
                ("mod_vco2pitch_lfo1", 0.535355),
            ],
        ),
        (
            "Sync sweep",
            Category::Lead,
            &[
                ("vco1level", 0.0),
                ("vco2level", 0.9),
                ("mod_vco2sync_vco1syon", 1.0),
                ("mod_vco2sync_vco1sy", 1.0),
                ("coarse2", 0.75),
                ("mod_pw2_env1on", 1.0),
                ("mod_pw2_env1", 0.5),
                ("cutoff", 0.75),
                ("mod_cutoff_env1", 0.75),
                ("vcfdecay", 0.6),
                ("vcfattack", 0.3),
                ("vcfsustain", 0.3),
            ],
        ),
        (
            "Weak sync",
            Category::Lead,
            &[
                ("vco1level", 0.3),
                ("vco2level", 0.9),
                ("mod_vco2sync_vco1syon", 1.0),
                ("mod_vco2sync_vco1sy", 1.0),
                ("syncstrength", 1.0),
                ("coarse2", 0.64),
                ("fine2", 0.48),
                ("cutoff", 0.7),
            ],
        ),
        // ---- the ring modulator ----
        (
            "Bell ring",
            Category::Keys,
            &[
                ("vco1level", 0.0),
                ("ringmodlevel", 0.9),
                ("coarse2", 0.8125),
                ("fine2", 0.58),
                ("cutoff", 0.8),
                ("vcaattack", 0.1),
                ("vcadecay", 0.65),
                ("vcasustain", 0.0),
                ("vcarelease", 0.65),
                ("reverb", 0.4),
            ],
        ),
        (
            "Metal ring",
            Category::Fx,
            &[
                ("vco1level", 0.2),
                ("ringmodlevel", 0.9),
                ("wave1", 0.5),
                ("wave2", 0.5),
                ("coarse2", 0.7291667),
                ("cutoff", 0.85),
                ("resonance", 0.4),
                ("vcadecay", 0.5),
                ("vcasustain", 0.2),
            ],
        ),
        (
            "Cross mod",
            Category::Fx,
            &[
                ("mod_vco1pitch_vco2", 0.770031),
                ("vco2level", 0.3),
                ("coarse2", 0.9),
                ("cutoff", 0.7),
                ("mod_cutoff_env1", 0.7),
                ("vcfdecay", 0.55),
            ],
        ),
        // ---- the noise ----
        (
            "Pink wash",
            Category::Fx,
            &[
                ("vco1level", 0.0),
                ("noiselevel", 0.9),
                ("noisecolour", 1.0),
                ("cutoff", 0.55),
                ("resonance", 0.5),
                ("mod_cutoff_lfo1", 0.65),
                ("lfo1rate", 0.15),
                ("vcaattack", 0.5),
                ("vcarelease", 0.6),
            ],
        ),
        (
            "Wind",
            Category::Fx,
            &[
                ("vco1level", 0.0),
                ("noiselevel", 0.8),
                ("cutoff", 0.5),
                ("resonance", 0.75),
                ("mod_cutoff_lfo1", 0.675),
                ("lfo1rate", 0.05),
                ("lfo1shape", 0.25),
                ("vcaattack", 0.55),
                ("vcasustain", 0.8),
                ("vcarelease", 0.6),
                ("reverb", 0.5),
            ],
        ),
        // ---- the filter ----
        (
            "Squelch bass",
            Category::Bass,
            &[
                ("range1", 0.4),
                ("cutoff", 0.35),
                ("resonance", 0.8),
                ("mod_cutoff_env1", 0.8),
                ("vcfdecay", 0.4),
                ("vcfsustain", 0.1),
                ("vcadecay", 0.5),
                ("vcasustain", 0.5),
                ("vcarelease", 0.3),
            ],
        ),
        (
            "Round bass",
            Category::Bass,
            &[
                ("range1", 0.4),
                ("wave1", 1.0),
                ("vco2level", 0.5),
                ("range2", 0.2),
                ("cutoff", 0.4),
                ("mod_cutoff_env1", 0.625),
                ("vcfdecay", 0.5),
                ("vcasustain", 0.9),
            ],
        ),
        // ---- the sample-and-hold and the matrix ----
        (
            "Sample and hold",
            Category::Sequence,
            &[
                ("mod_shsrc_lfo1sawon", 1.0),
                ("mod_shsrc_lfo1saw", 1.0),
                ("shrate", 0.4),
                ("mod_cutoff_lfo1on", 0.0),
                ("mod_cutoff_shon", 1.0),
                ("mod_cutoff_sh", 0.583333),
                ("cutoff", 0.45),
                ("resonance", 0.6),
            ],
        ),
        (
            "Self running",
            Category::Drone,
            &[
                ("mod_shsrc_lfo1sawon", 1.0),
                ("mod_shsrc_lfo1saw", 1.0),
                ("shrate", 0.3),
                ("mod_vcagate_gateon", 0.0),
                ("mod_vcagate_shclkon", 1.0),
                ("mod_vcagate_shclk", 1.0),
                ("mod_vcfgate_gateon", 0.0),
                ("mod_vcfgate_shclkon", 1.0),
                ("mod_vcfgate_shclk", 1.0),
                ("mod_vco1pitch_vco2on", 0.0),
                ("mod_vco1pitch_shon", 1.0),
                ("mod_vco1pitch_sh", 0.629099),
                ("cutoff", 0.5),
                ("resonance", 0.55),
                ("mod_cutoff_env1", 0.75),
                ("vcfdecay", 0.4),
                ("vcfsustain", 0.0),
                ("vcadecay", 0.45),
                ("vcasustain", 0.0),
            ],
        ),
        (
            "Clocked filter",
            Category::Sequence,
            &[
                ("shrate", 0.45),
                ("mod_vcfgate_gateon", 0.0),
                ("mod_vcfgate_shclkon", 1.0),
                ("mod_vcfgate_shclk", 1.0),
                ("mod_cutoff_env1", 0.8),
                ("vcfdecay", 0.35),
                ("vcfsustain", 0.0),
                ("cutoff", 0.4),
                ("resonance", 0.5),
                ("vcasustain", 1.0),
            ],
        ),
        (
            "Drone",
            Category::Drone,
            &[
                ("initialgain", 0.7),
                ("mod_vcacv_env2", 0.5),
                ("vco2level", 0.7),
                ("coarse2", 0.5),
                ("fine2", 0.53),
                ("cutoff", 0.5),
                ("mod_cutoff_lfo1", 0.6),
                ("lfo1rate", 0.1),
                ("lfo1shape", 0.25),
                ("reverb", 0.3),
            ],
        ),
        // ---- the modulators ----
        (
            "Vibrato lead",
            Category::Lead,
            &[
                ("wave1", 0.5),
                ("mod_pw1_lfo1trion", 1.0),
                ("mod_pw1_lfo1tri", 0.7),
                ("mod_vco1pitch_lfo1", 0.540825),
                ("mod_vco2pitch_lfo1", 0.540825),
                ("lfo1rate", 0.55),
                ("cutoff", 0.7),
                ("mod_cutoff_env1", 0.65),
                ("vcfdecay", 0.5),
                ("portamento", 0.25),
                ("priority", 1.0),
            ],
        ),
        // ---- the effects ----
        (
            "Phased pad",
            Category::Pad,
            &[
                ("vco2level", 0.8),
                ("fine2", 0.58),
                ("wave1", 0.5),
                ("wave2", 0.5),
                ("mod_pw1_lfo1trion", 1.0),
                ("mod_pw1_lfo1tri", 0.65),
                ("mod_pw2_lfo2trion", 1.0),
                ("mod_pw2_lfo2tri", 0.65),
                ("cutoff", 0.6),
                ("vcaattack", 0.45),
                ("vcarelease", 0.6),
                ("phaser", 0.5),
            ],
        ),
        (
            "Echo pluck",
            Category::Pluck,
            &[
                ("cutoff", 0.5),
                ("resonance", 0.3),
                ("mod_cutoff_env1", 0.75),
                ("vcfdecay", 0.35),
                ("vcfsustain", 0.0),
                ("vcadecay", 0.4),
                ("vcasustain", 0.0),
                ("delay", 0.45),
                ("delaytime", 0.55),
            ],
        ),
        (
            "Spring drip",
            Category::Fx,
            &[
                ("wave1", 1.0),
                ("cutoff", 0.65),
                ("vcadecay", 0.3),
                ("vcasustain", 0.0),
                ("vcarelease", 0.2),
                ("reverb", 0.8),
                ("volume", 0.8),
            ],
        ),
        (
            "Octave bass",
            Category::Bass,
            &[
                ("range1", 0.4),
                ("vco2level", 0.7),
                ("range2", 0.2),
                ("cutoff", 0.45),
                ("mod_cutoff_env1", 0.65),
                ("vcfdecay", 0.45),
                ("vcfsustain", 0.2),
                ("vcasustain", 0.8),
                ("resonance", 0.2),
            ],
        ),
        (
            "Sub pulse bass",
            Category::Bass,
            &[
                ("range1", 0.4),
                ("wave1", 0.5),
                ("pw1", 0.6),
                ("vco2level", 0.5),
                ("range2", 0.2),
                ("wave2", 0.5),
                ("cutoff", 0.4),
                ("mod_cutoff_env1", 0.7),
                ("vcfdecay", 0.4),
                ("vcfsustain", 0.1),
                ("vcadecay", 0.5),
                ("vcasustain", 0.6),
            ],
        ),
        (
            "Rubber bass",
            Category::Bass,
            &[
                ("range1", 0.4),
                ("cutoff", 0.35),
                ("resonance", 0.5),
                ("mod_cutoff_env1", 0.775),
                ("vcfdecay", 0.3),
                ("vcfsustain", 0.0),
                ("vcadecay", 0.4),
                ("vcasustain", 0.3),
                ("vcarelease", 0.25),
                ("tone", 0.35),
            ],
        ),
        (
            "Glide bass",
            Category::Bass,
            &[
                ("range1", 0.4),
                ("vco2level", 0.6),
                ("fine2", 0.56),
                ("cutoff", 0.4),
                ("mod_cutoff_env1", 0.65),
                ("vcfdecay", 0.5),
                ("portamento", 0.35),
                ("priority", 0.0),
                ("vcasustain", 0.9),
            ],
        ),
        (
            "Ring bass",
            Category::Bass,
            &[
                ("range1", 0.4),
                ("ringmodlevel", 0.5),
                ("vco1level", 0.6),
                ("coarse2", 0.5),
                ("fine2", 0.5),
                ("range2", 0.2),
                ("cutoff", 0.4),
                ("mod_cutoff_env1", 0.65),
                ("vcfdecay", 0.45),
                ("vcasustain", 0.8),
            ],
        ),
        (
            "Pulse lead",
            Category::Lead,
            &[
                ("wave1", 0.5),
                ("mod_pw1_lfo1trion", 1.0),
                ("mod_pw1_lfo1tri", 0.75),
                ("lfo1rate", 0.45),
                ("cutoff", 0.7),
                ("resonance", 0.2),
                ("mod_cutoff_env1", 0.65),
                ("vcfdecay", 0.5),
                ("vcasustain", 0.9),
                ("priority", 1.0),
            ],
        ),
        (
            "Octave lead",
            Category::Lead,
            &[
                ("vco2level", 0.9),
                ("range2", 0.8),
                ("cutoff", 0.7),
                ("mod_cutoff_env1", 0.675),
                ("vcfdecay", 0.5),
                ("portamento", 0.2),
                ("resonance", 0.15),
            ],
        ),
        (
            "Hard sync lead",
            Category::Lead,
            &[
                ("mod_vco2sync_vco1syon", 1.0),
                ("mod_vco2sync_vco1sy", 1.0),
                ("vco1level", 0.0),
                ("vco2level", 0.9),
                ("coarse2", 0.65),
                ("fine2", 0.55),
                ("cutoff", 0.75),
                ("resonance", 0.3),
                ("mod_cutoff_env1", 0.7),
                ("vcfdecay", 0.55),
                ("mod_vco1pitch_lfo1", 0.532275),
                ("mod_vco2pitch_lfo1", 0.532275),
                ("lfo1rate", 0.5),
            ],
        ),
        (
            "Soft triangle lead",
            Category::Lead,
            &[
                ("wave1", 1.0),
                ("cutoff", 0.75),
                ("vcaattack", 0.3),
                ("vcarelease", 0.45),
                ("mod_vco1pitch_lfo1", 0.535355),
                ("lfo1rate", 0.5),
                ("portamento", 0.15),
            ],
        ),
        (
            "Screaming lead",
            Category::Lead,
            &[
                ("cutoff", 0.55),
                ("resonance", 0.85),
                ("mod_cutoff_env1", 0.7),
                ("vcfdecay", 0.55),
                ("vcfsustain", 0.6),
                ("mod_cutoff_key", 0.9),
                ("vco2level", 0.7),
                ("fine2", 0.56),
                ("tone", 0.7),
            ],
        ),
        (
            "Detuned pad",
            Category::Pad,
            &[
                ("vco2level", 0.9),
                ("fine2", 0.6),
                ("cutoff", 0.5),
                ("vcaattack", 0.55),
                ("vcarelease", 0.65),
                ("vcfattack", 0.4),
                ("mod_cutoff_env1", 0.6),
                ("resonance", 0.1),
                ("reverb", 0.3),
            ],
        ),
        (
            "Triangle pad",
            Category::Pad,
            &[
                ("wave1", 1.0),
                ("wave2", 1.0),
                ("vco2level", 0.8),
                ("fine2", 0.57),
                ("cutoff", 0.6),
                ("vcaattack", 0.6),
                ("vcarelease", 0.7),
                ("vcasustain", 0.9),
                ("phaser", 0.3),
                ("reverb", 0.3),
            ],
        ),
        (
            "Breathing pad",
            Category::Pad,
            &[
                ("vco2level", 0.8),
                ("fine2", 0.56),
                ("cutoff", 0.4),
                ("mod_cutoff_lfo1", 0.65),
                ("lfo1rate", 0.2),
                ("lfo1shape", 0.25),
                ("vcaattack", 0.6),
                ("vcarelease", 0.7),
                ("vcasustain", 1.0),
                ("reverb", 0.4),
            ],
        ),
        (
            "PWM pad",
            Category::Pad,
            &[
                ("wave1", 0.5),
                ("wave2", 0.5),
                ("mod_pw1_lfo2trion", 1.0),
                ("mod_pw1_lfo2tri", 0.75),
                ("mod_pw2_lfo1trion", 1.0),
                ("mod_pw2_lfo1tri", 0.75),
                ("vco2level", 0.8),
                ("fine2", 0.55),
                ("lfo1rate", 0.35),
                ("lfo2rate", 0.3),
                ("cutoff", 0.5),
                ("vcaattack", 0.5),
                ("vcarelease", 0.6),
                ("delay", 0.3),
            ],
        ),
        (
            "Brass",
            Category::Brass,
            &[
                ("vco2level", 0.8),
                ("fine2", 0.55),
                ("cutoff", 0.4),
                ("mod_cutoff_env1", 0.75),
                ("vcfattack", 0.3),
                ("vcfdecay", 0.5),
                ("vcfsustain", 0.5),
                ("vcaattack", 0.25),
                ("vcasustain", 0.8),
                ("resonance", 0.1),
            ],
        ),
        (
            "Electric piano",
            Category::Keys,
            &[
                ("wave1", 1.0),
                ("ringmodlevel", 0.3),
                ("coarse2", 0.75),
                ("cutoff", 0.6),
                ("mod_cutoff_env1", 0.65),
                ("vcfdecay", 0.55),
                ("vcfsustain", 0.0),
                ("vcadecay", 0.6),
                ("vcasustain", 0.2),
                ("vcarelease", 0.4),
            ],
        ),
        (
            "Clav",
            Category::Keys,
            &[
                ("wave1", 0.5),
                ("pw1", 0.3),
                ("cutoff", 0.55),
                ("resonance", 0.45),
                ("mod_cutoff_env1", 0.75),
                ("vcfdecay", 0.35),
                ("vcfsustain", 0.1),
                ("vcadecay", 0.4),
                ("vcasustain", 0.2),
                ("mod_cutoff_key", 0.8),
            ],
        ),
        (
            "Glass keys",
            Category::Keys,
            &[
                ("wave1", 1.0),
                ("vco2level", 0.7),
                ("wave2", 1.0),
                ("coarse2", 0.75),
                ("cutoff", 0.7),
                ("vcadecay", 0.6),
                ("vcasustain", 0.0),
                ("vcarelease", 0.55),
                ("reverb", 0.35),
                ("delay", 0.25),
            ],
        ),
        (
            "Marimba",
            Category::Percussion,
            &[
                ("wave1", 1.0),
                ("range1", 0.8),
                ("cutoff", 0.65),
                ("mod_cutoff_env1", 0.65),
                ("vcfdecay", 0.3),
                ("vcfsustain", 0.0),
                ("vcadecay", 0.3),
                ("vcasustain", 0.0),
                ("vcarelease", 0.25),
            ],
        ),
        (
            "Noise snare",
            Category::Percussion,
            &[
                ("noiselevel", 0.9),
                ("vco1level", 0.3),
                ("wave1", 1.0),
                ("range1", 0.4),
                ("cutoff", 0.6),
                ("resonance", 0.3),
                ("mod_cutoff_env1", 0.7),
                ("vcfdecay", 0.25),
                ("vcfsustain", 0.0),
                ("vcadecay", 0.25),
                ("vcasustain", 0.0),
                ("vcarelease", 0.15),
            ],
        ),
        (
            "Hat",
            Category::Percussion,
            &[
                ("vco1level", 0.0),
                ("noiselevel", 0.9),
                ("hpf", 0.7),
                ("cutoff", 0.9),
                ("vcadecay", 0.15),
                ("vcasustain", 0.0),
                ("vcarelease", 0.1),
            ],
        ),
        (
            "Thunder",
            Category::Fx,
            &[
                ("vco1level", 0.0),
                ("noiselevel", 1.0),
                ("noisecolour", 1.0),
                ("cutoff", 0.3),
                ("resonance", 0.4),
                ("mod_cutoff_env1", 0.7),
                ("vcfattack", 0.5),
                ("vcfdecay", 0.7),
                ("vcfsustain", 0.0),
                ("vcaattack", 0.4),
                ("vcadecay", 0.75),
                ("vcasustain", 0.0),
                ("vcarelease", 0.7),
                ("reverb", 0.7),
            ],
        ),
        (
            "Random steps",
            Category::Sequence,
            &[
                ("mod_shsrc_noiseon", 1.0),
                ("mod_shsrc_noise", 1.0),
                ("shrate", 0.45),
                ("mod_vco1pitch_vco2on", 0.0),
                ("mod_vco1pitch_shon", 1.0),
                ("mod_vco1pitch_sh", 0.75),
                ("mod_vcagate_gateon", 0.0),
                ("mod_vcagate_shclkon", 1.0),
                ("mod_vcagate_shclk", 1.0),
                ("mod_vcfgate_gateon", 0.0),
                ("mod_vcfgate_shclkon", 1.0),
                ("mod_vcfgate_shclk", 1.0),
                ("cutoff", 0.55),
                ("resonance", 0.4),
                ("mod_cutoff_env1", 0.7),
                ("vcfdecay", 0.35),
                ("vcfsustain", 0.0),
                ("vcadecay", 0.4),
                ("vcasustain", 0.0),
            ],
        ),
        (
            "LFO arpeggio",
            Category::Sequence,
            &[
                ("vcatrigger", 0.5),
                ("vcftrigger", 0.5),
                ("lfo1rate", 0.6),
                ("lfo1shape", 0.75),
                ("cutoff", 0.5),
                ("mod_cutoff_env1", 0.75),
                ("vcfdecay", 0.3),
                ("vcfsustain", 0.0),
                ("vcadecay", 0.3),
                ("vcasustain", 0.0),
                ("delay", 0.3),
                ("delaytime", 0.7),
            ],
        ),
        (
            "Ring drone",
            Category::Drone,
            &[
                ("initialgain", 0.6),
                ("mod_vcacv_env2", 0.5),
                ("ringmodlevel", 0.6),
                ("vco1level", 0.5),
                ("coarse2", 0.55),
                ("fine2", 0.52),
                ("cutoff", 0.45),
                ("mod_cutoff_lfo1", 0.575),
                ("lfo1rate", 0.1),
                ("reverb", 0.5),
                ("phaser", 0.3),
            ],
        ),
        (
            "Phaser sweep",
            Category::Fx,
            &[
                ("phaser", 0.8),
                ("mod_phlfo_lfo2on", 1.0),
                ("mod_phlfo_lfo2", 1.0),
                ("lfo2rate", 0.2),
                ("vco2level", 0.7),
                ("fine2", 0.56),
                ("cutoff", 0.6),
                ("vcasustain", 1.0),
                ("vcarelease", 0.5),
            ],
        ),
        (
            "Delay stabs",
            Category::Pluck,
            &[
                ("wave1", 0.5),
                ("pw1", 0.4),
                ("cutoff", 0.5),
                ("resonance", 0.4),
                ("mod_cutoff_env1", 0.775),
                ("vcfdecay", 0.3),
                ("vcfsustain", 0.0),
                ("vcadecay", 0.3),
                ("vcasustain", 0.0),
                ("delay", 0.6),
                // A quarter-note triplet: the division, not a number, so the one ladder decides
                // where it sits (`plans/plan-tempo-sync-controls.md`).
                (
                    "delaytime",
                    crate::params::DELAY_SYNC.position(mxm_tempo::Division::QuarterTriplet),
                ),
                ("temposync", 1.0),
            ],
        ),
        (
            "Cathedral",
            Category::Pad,
            &[
                ("vco2level", 0.8),
                ("fine2", 0.57),
                ("wave1", 1.0),
                ("wave2", 0.5),
                ("cutoff", 0.5),
                ("vcaattack", 0.6),
                ("vcarelease", 0.8),
                ("vcasustain", 1.0),
                ("reverb", 0.9),
                ("tone", 0.4),
            ],
        ),
        (
            "Strings",
            Category::Strings,
            &[
                ("vco2level", 0.9),
                ("fine2", 0.58),
                ("cutoff", 0.55),
                ("vcaattack", 0.5),
                ("vcarelease", 0.6),
                ("vcasustain", 1.0),
                ("phaser", 0.4),
                ("mod_vco1pitch_lfo1", 0.528868),
                ("mod_vco2pitch_lfo1", 0.528868),
                ("lfo1rate", 0.4),
                ("hpf", 0.4),
            ],
        ),
        (
            "Talking filter",
            Category::Fx,
            &[
                ("mod_cutoff_lfo1on", 0.0),
                ("mod_cutoff_lfo2on", 1.0),
                ("mod_cutoff_lfo2", 0.8),
                ("lfo2shape", 0.9),
                ("lfo2rate", 0.5),
                ("cutoff", 0.4),
                ("resonance", 0.75),
                ("vco2level", 0.6),
                ("fine2", 0.55),
                ("vcasustain", 1.0),
            ],
        ),
    ];

    /// One designed sound: its name, its category, and the values that make it.
    type Design = (&'static str, Category, &'static [(&'static str, f32)]);

    /// Writes the twenty factory presets to `plugins/mxm-mono-00/presets/`.
    ///
    /// A facility, not a test — and the *only* thing that writes those files, so the numbers in
    /// `FACTORY_DESIGN` stay the readable statement of each sound and the JSON stays generated
    /// output. `every_factory_preset_covers_every_parameter` is what catches a file that has fallen
    /// behind a new parameter.
    ///
    /// ```text
    /// cargo test -p mxm-mono-00 --lib write_the_factory_presets -- --ignored --nocapture
    /// ```
    #[test]
    #[ignore = "writes the factory preset files"]
    fn write_the_factory_presets() {
        let params = params();
        let bindings = mxm_preset::Instrument::parameters(&params);

        for (name, category, overrides) in FACTORY_DESIGN {
            let mut preset = Preset::init(&params);
            preset.name = (*name).to_owned();
            preset.category = *category;

            for (id, v) in *overrides {
                let bound = bindings
                    .iter()
                    .find(|(bid, _)| *bid == *id)
                    .unwrap_or_else(|| panic!("{name:?} names `{id}`, which is not a parameter"));
                preset.params.insert(
                    (*id).to_owned(),
                    Value {
                        v: *v,
                        text: bound.1.format(*v),
                    },
                );
            }

            // From the manifest directory, not the working one: a test's cwd is the crate root
            // and not the workspace root, which is the sort of thing that only says so once.
            let file = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
                .join("presets")
                .join(format!("{}.json", name.to_lowercase().replace(' ', "-")));
            std::fs::write(&file, preset.to_json()).expect("write the preset");
            eprintln!("wrote {}", file.display());
        }
    }

    #[test]
    fn every_designed_preset_names_real_parameters() {
        // Runs by default, unlike the generator: a typo in `FACTORY_DESIGN` would otherwise only
        // surface the next time somebody regenerated the files, and silently leave that value at
        // its default in the meantime.
        let params = params();
        let bindings = mxm_preset::Instrument::parameters(&params);
        for (name, _category, overrides) in FACTORY_DESIGN {
            for (id, v) in *overrides {
                assert!(
                    bindings.iter().any(|(bid, _)| *bid == *id),
                    "{name:?} names `{id}`, which is not a parameter of this instrument"
                );
                assert!(
                    (0.0..=1.0).contains(v),
                    "{name:?} sets `{id}` to {v}, which is not a normalised value"
                );
            }
        }
    }

    #[test]
    fn the_factory_files_match_the_design_they_were_generated_from() {
        // The generator is `#[ignore]`d, so nothing forces it to have been run. This is what says
        // the shipped files are the current design rather than a stale one — the same class of
        // mistake as a stale `.clap` bundle, and just as quiet.
        for (name, _category, overrides) in FACTORY_DESIGN {
            let (_, text) = FACTORY_FILES
                .iter()
                .find(|(file_name, _)| file_name == name)
                .unwrap_or_else(|| panic!("no factory file for {name:?}"));
            let preset = Preset::parse(text, crate::CLAP_ID).expect("parses");

            for (id, v) in *overrides {
                let value = preset
                    .params
                    .get(*id)
                    .unwrap_or_else(|| panic!("{name:?} is missing `{id}`"));
                assert!(
                    (value.v - v).abs() < 1e-6,
                    "{name:?} ships `{id}` at {} but is designed at {v} — regenerate the files",
                    value.v
                );
            }
        }
    }

    /// **Every stored `text` is this plugin's own formatting of the `v` beside it.**
    ///
    /// `text` is never loaded, which is exactly why nothing else notices it going stale: a change
    /// to a reading leaves every file loading the same sound while telling whoever reads it
    /// something else. The test above compares only `v`, and 134 route readings shipped from
    /// before `SOURCE_PEAK` entered them — `+5.83 oct` where the plugin prints `+3.50 oct` (audit
    /// D17). This compares text, in a test, and never at load: the format's *no mismatch
    /// detection* is about loading.
    #[test]
    fn every_factory_text_is_this_plugins_formatting_of_its_value() {
        let params = params();
        let bindings = mxm_preset::Instrument::parameters(&params);
        let mut stale = Vec::new();
        for (name, text) in FACTORY_FILES {
            let preset = Preset::parse(text, crate::CLAP_ID).expect("parses");
            for (id, value) in &preset.params {
                // An unknown id is `every_factory_preset_covers_every_parameter`'s to report.
                let Some((_, param)) = bindings.iter().find(|(bound, _)| bound == id) else {
                    continue;
                };
                let printed = param.format(value.v);
                if printed != value.text {
                    stale.push(format!(
                        "{name:?} `{id}` stores {:?}, the plugin prints {printed:?}",
                        value.text
                    ));
                }
            }
        }
        assert!(
            stale.is_empty(),
            "{} stale texts — regenerate the files:\n{}",
            stale.len(),
            stale.join("\n")
        );
    }

    #[test]
    fn the_user_root_is_under_this_instruments_own_id() {
        // Namespaced by CLAP id so another instrument's presets cannot appear in this one's list.
        let Some(root) = user_root(crate::CLAP_ID) else {
            return;
        };
        assert!(root.ends_with("presets"));
        assert!(root.to_string_lossy().contains(crate::CLAP_ID));
    }

    #[test]
    fn every_factory_preset_can_be_heard() {
        // A preset with the output at nothing, every mixer level at zero, or the amplifier with
        // nothing to open it, loads without complaint and reads as the instrument being broken.
        for (name, text) in FACTORY_FILES {
            let preset = Preset::parse(text, crate::CLAP_ID).expect("parses");
            let v = |id: &str| preset.params.get(id).map_or(0.0, |value| value.v);
            // **A route carries something only if it is present *and* turned up**, and a signed
            // amount's zero is the middle of its range rather than the bottom of it.
            let carries = |target: usize| {
                use mxm_mono_00_dsp::routing::SOURCES;
                (0..SOURCES).any(|s| {
                    let (amount, present) = crate::routes::ROUTE_IDS[target][s];
                    v(present) > 0.5 && (v(amount) - 0.5).abs() > 0.01
                })
            };
            use mxm_mono_00_dsp::routing::target;
            assert!(v("volume") > 0.05, "{name:?} is turned down to nothing");
            assert!(
                v("vco1level") > 0.0
                    || v("vco2level") > 0.0
                    || v("noiselevel") > 0.0
                    || v("ringmodlevel") > 0.0
                    || carries(target::MIXER_INPUT),
                "{name:?} has every mixer level at zero"
            );
            assert!(
                carries(target::AMPLIFIER) || v("initialgain") > 0.0,
                "{name:?} has nothing to open the amplifier"
            );
            assert!(
                v("cutoff") > 0.05 || carries(target::CUTOFF),
                "{name:?} has the filter shut and nothing to open it"
            );
        }
    }

    #[test]
    fn no_two_factory_presets_are_the_same_sound() {
        // Twenty is enough that a copied-and-edited design could lose its edit unnoticed.
        for (index, (name, text)) in FACTORY_FILES.iter().enumerate() {
            let a = Preset::parse(text, crate::CLAP_ID).expect("parses");
            for (other, text) in &FACTORY_FILES[index + 1..] {
                let b = Preset::parse(text, crate::CLAP_ID).expect("parses");
                assert_ne!(a.params, b.params, "{name:?} and {other:?} are identical");
            }
        }
    }

    #[test]
    fn every_factory_preset_has_a_category() {
        // A sound is saved with its category (the owner's rule, 2026-09-04), and the factory set
        // is where a person first sees what the categories mean. *Uncategorised* is for files
        // written before the field existed, not for sounds this instrument ships.
        for (name, text) in FACTORY_FILES {
            let preset = Preset::parse(text, crate::CLAP_ID).expect("parses");
            assert_ne!(
                preset.category,
                Category::Uncategorised,
                "factory preset {name:?} has no category"
            );
        }
    }

    #[test]
    fn every_factory_preset_parses_and_is_for_this_instrument() {
        // A malformed factory preset is a build mistake, not a user's, so it is caught here rather
        // than skipped quietly in the browser.
        for (name, text) in FACTORY_FILES {
            let preset = Preset::parse(text, crate::CLAP_ID)
                .unwrap_or_else(|e| panic!("factory preset {name:?} does not parse: {e}"));
            assert_eq!(preset.name, *name, "the file's name must match its listing");
        }
    }

    #[test]
    fn every_factory_preset_covers_every_parameter() {
        // The one that catches a factory preset written before a parameter existed: it would load
        // and quietly leave that parameter wherever the last patch left it.
        let params = params();
        for (name, text) in FACTORY_FILES {
            let preset = Preset::parse(text, crate::CLAP_ID).expect("parses");
            let (_, problems) = preset.resolve(&params);
            assert!(
                problems.is_empty(),
                "factory preset {name:?} is incomplete: {problems:?}"
            );
        }
    }

    #[test]
    fn the_factory_list_begins_with_init() {
        let params = params();
        let all = factory(&params);
        assert_eq!(all[0].name, INIT_NAME);
        assert_eq!(all.len(), FACTORY_FILES.len() + 1);
    }
}
