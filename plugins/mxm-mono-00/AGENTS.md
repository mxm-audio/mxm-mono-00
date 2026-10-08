# AGENTS.md — plugins/mxm-mono-00

Parent: [`../AGENTS.md`](../AGENTS.md) · History, measurements and rationale: [`NOTES.md`](NOTES.md)

# Purpose

The nice-plug shell for **mxm-mono-00**, a monophonic semi-modular inspired by the Roland
System-100 as its plug-out pictures it. Identity, parameters, MIDI, presets, telemetry and the
editor. The whole voice is [`crates/mxm-mono-00-dsp`](../../crates/mxm-mono-00-dsp/AGENTS.md).

Shared conventions — nice-plug's API, the init-patch contract, preset rules, `process()` realtime
rules, the editor contract — live in the parent and are not restated here. This doc holds what is
**local to this plugin**.

# Ownership

`Cargo.toml`, `README.md`, `NOTES.md`, `control-map.json`, `presets/`, `src/` and `tests/`
([NOTES.md § Ownership](NOTES.md#ownership-the-files)); its licence is the repository's root `LICENSE`.

# Local Contracts

## Permanent identifiers

| What | Value |
|---|---|
| `CLAP_ID` | `dk.mxm.mxm-mono-00` — assembled from `plugin_name!` in `src/lib.rs`, **not** from `CARGO_PKG_NAME` |
| Parameter `#[id]`s | **Voice:** `priority` `tune` `bendrange` `portamento` `volume`<br>**VCO-1:** `range1` `coarse1` `fine1` `wave1` `pw1`<br>**VCO-2:** `range2` `coarse2` `fine2` `wave2` `pw2` `syncstrength`<br>**Mixer:** `vco1level` `vco2level` `noiselevel` `ringmodlevel` `noisecolour`<br>**Filter:** `hpf` `cutoff` `resonance`<br>**Amplifier:** `initialgain` `tone`<br>**Envelopes:** `vcfattack` `vcfdecay` `vcfsustain` `vcfrelease` `vcftrigger` and `vca…` likewise<br>**Modulators:** `lfo1rate` `lfo1shape` `lfo1offset` `lfo1sync` and `lfo2…` likewise<br>**S&H:** `shrate` `shlag` `shsync`<br>**FX:** `phaser` `delay` `delaytime` `temposync` `reverb`<br>**Routing:** 433 offered pairs of 450, `<target>_<source>` and `<target>_<source>on`, from eighteen `#[nested(id_prefix = …)]` groups whose `Params` impl is written by hand to leave the seventeen refused pairs out — the table is [`src/routes.rs`](src/routes.rs)'s `ROUTE_IDS` |

- The ids and their count are public interface. `every_parameter_has_a_distinct_id`,
  `every_parameter_is_drawn_exactly_once` and `routes::tests::the_id_table_is_what_the_derive_actually_produces`
  keep the list, the table and the editor agreeing with the derive.
- **Retired ids never come back** — Rev 3's eleven controls and 200 route ids, the standard's
  thirty-four refused pairs, the patch bay's twenty-three. A changed meaning under a kept id is
  forbidden: a re-declared input gets new ids (`mod_vco1pitch_*`, `mod_shsrc_*`).
  `routes::tests::no_retired_id_reappears_among_the_routing_parameters` holds them all. A preset
  naming one loads everything else; an automation lane on one is lost, accepted while unreleased
  ([NOTES.md § Retired ids](NOTES.md#the-ids-retired-with-the-routing-conversions-and-none-may-come-back)).

## The patch bay is parameter pairs, not state

- A route is a *(target, source)* pair: a **presence** and a signed **amount**, one
  `#[nested(id_prefix = …)]` per target in `src/routes.rs`. No source selector: a slot *is* a source.
- Both halves are automatable and per-step modulatable; neither is `#[persist]`
  (`the_routing_groups_are_the_dsp_targets_in_declared_order`). Removing a source is one parameter
  write that leaves the amount alone, so re-adding restores the depth.
- A route reads `scale × routing::SOURCE_PEAK[source]`, what the pair delivers at full amount
  (`a_route_at_full_amount_reads_the_machines_own_number`), through `mxm_modulation_params::signed`,
  never `-0`. `SOURCE_PEAK` is display-only
  ([NOTES.md § Readings](NOTES.md#a-routes-amount-reads-what-that-pair-delivers)).
- The pitch rows' faders are square-law about the centre (`routes::PITCH_TAPER`); plain values stay
  linear; Key's 1 V/oct and a performance pair's twelve semitones are linear.
- Every formatter picks its unit from what the finer branch would print (`v2s_time`, `v2s_hertz`;
  `v2s_percent` never prints `-0 %`):
  `params::tests::every_parameter_reads_the_same_after_the_hosts_round_trip` holds all of them
  ([NOTES.md § Round trip](NOTES.md#every-parameters-text-survives-the-hosts-round-trip)).

## What the machine had, and what was added

- **Effects:** the phaser and delay exist by the owner's exemption for this instrument only; the
  spring reverb is the 103's and is promoted to `mxm-folded-spring`
  ([NOTES.md § Three effects](NOTES.md#three-effects-and-two-of-them-by-exemption)).
- **Four MIDI paths the hardware never had** (velocity, CC 1, pressure, the bend's normalised
  position), every pair absent at Init. They mean what the modulation standard says; a pair that can
  never mean anything is not offered; one-sided targets offer their live half, the sync faders
  0…+100 %. `every_route_parameter_says_what_the_dsp_does` holds every pair to
  `mxm_mono_00_dsp::conformance::Declared`. The wheel writes no parameter
  ([NOTES.md § Four MIDI paths](NOTES.md#four-midi-paths-this-machines-hardware-never-had)).
- `coarse1` and `fine1` restore the hardware's VCO-1 tuning; both start at zero.
- **The panel's wiring is routing (Rev 3):** no card chooses where its output goes. Wart 4 is a law,
  not a switch (`routing::narrowed_width`). The LFO trigger position, the ring modulator's X input,
  the bend's hard-wired path and portamento stay circuits
  ([NOTES.md § Panel wiring](NOTES.md#the-panels-own-wiring-is-routing-too-and-no-card-chooses-where-its-output-goes)).

## The init patch

- Every amount starts at zero (`every_amount_starts_at_zero`,
  `the_routing_starts_as_the_machines_own_patch`), except: VCO-1's level, the amplifier's envelope
  route and the cutoff start up; VCO-2's fine tune starts slightly detuned
  (`vco2_starts_slightly_detuned`).
- **The plug-out's thirteen normalled connections are the init patch**: the DSP's `INIT_PRESENT` and
  `INIT_AT_FULL`. Nine inherit their control's zero; four that had no attenuator start at full — both
  envelope gates ← Gate, Ring mod ← Oscillator 1, VCA level ← Envelope 2. **A route whose depth was
  not a control before has no zero to inherit.** Key follow is bipolar
  ([NOTES.md § The init patch](NOTES.md#the-init-patch-its-deviations-and-the-wiring-that-is-now-data)).
- **Init also patches Noise into the S&H input, at full** (`INIT_AT_FULL`; the owner, 2026-10-08):
  the manual's own suggestion, so an LFO set to S&H gives random steps at once instead of a flat
  line, as a route row the player can see and remove. The S&H keeps its own clock (Sample time),
  shared by both LFOs. Init sounds the same: nothing reads the S&H at Init.

## Process, notes and timing

- `process()` maps `Voice::activity` to `KeepAlive`, `Tail(n)` and `Normal`; the patch can be live
  with no key down. `Tail(n)` is `Voice::tail_samples` with the routing in force. A live route's
  depth or a control the verdict reads, still ramping, is `KeepAlive` (`Routes::is_ramping`,
  `controls_are_ramping`). The activity goes to telemetry once per block
  ([NOTES.md § Process status](NOTES.md#the-process-status-is-the-patchs-activity)).
- The DSP owns the held-key stack and the priority rule. Every note event passes `voice_id`,
  `channel` and `note` through; the shell reads no gate; the last sample's `Patch` is kept on the
  plugin for event handling, under the priority in force at the event.
- The bend follows the sounding note's channel (`voice.sounding()`). `PolyTuning` is kept with its
  `NoteId`, matched by voice id when given, and applied only while that note sounds. Non-finite
  tuning or bend is dropped. A choke leaves the effects' tails; All Sound Off clears them. Velocity
  drives no hard-wired path, only the routable source
  ([NOTES.md § Notes](NOTES.md#notes-are-handed-to-the-voice-with-their-identity-and-the-voice-keeps-the-stack)).
- **Tempo sync** (`temposync`, `shsync`, `lfo1sync`, `lfo2sync`) follows `plugins/AGENTS.md`'s
  contract: with a tempo, the control's **modulated** position picks a division on
  `params::DELAY_SYNC`, `SH_SYNC` or `LFO_SYNC`; a synced rate is fastest at the top; the smoother
  advances every sample either way; the tempo in force is `Telemetry::tempo`
  ([NOTES.md § Tempo sync](NOTES.md#tempo-sync-is-resolved-once-per-block-from-the-transport)).
- `activate` returns `false`, before anything changes, for a non-finite rate or one below
  `mxm_mono_00_dsp::MIN_SAMPLE_RATE` (`activation_refuses_a_non_finite_rate_and_any_below_the_floor`).
- **`activate` does not report `Voice::latency_samples()`**: nice-plug turns a report on an active
  plugin into a restart loop the player does not survive. Revisit if a DAW test shows it mattering
  ([NOTES.md § Latency](NOTES.md#twenty-two-samples-of-latency-deliberately-not-reported)).
- Smoothed: levels, amounts, cutoffs, pulse widths, tune, sustains, LFO offsets. Not smoothed:
  envelope times, rates, switches, the matrix, trigger modes. The delay time is the one smoothed time.

## Developer channel and control map

- `MXM_DEV_CC` in the process environment at instance creation enables CC 119 (category 0–5, 127
  Parameters), CC 118 (Voice expander), CC 117 (preset browser) and CC 116 (theme, unsaved). The
  gate is the environment, never a parameter; requests go through `Telemetry`'s atomics; the DSP
  reads nothing (`the_developer_channel_is_off_unless_the_environment_asked_for_it`).
- `control-map.json` leaves out roles a player's compiled-in standard may not declare (the LFO 2
  page, `fx.*`), because a map naming one is refused whole; `filter.hpf` is unfilled.
- **A role may be filled by a route's *amount* only where Init wires that route**, or it is a dead
  knob. A *presence* is live only if the amount it finds is, so the sync normal's amount waits at
  full (`a_control_map_role_never_points_at_a_dead_route`,
  `the_sync_switch_role_turns_on_a_full_reset`;
  [NOTES.md § Control map](NOTES.md#the-control-map-claims-the-roles-that-existed-before-this-instrument)).

## The editor

- [`docs/briefs/mxm-mono-00.md`](../../docs/briefs/mxm-mono-00.md) gates the editor. **A
  `page_items` key is a card's permanent id, not its position**; retired keys (12, 13) are never
  reused ([NOTES.md § The editor](NOTES.md#the-editor-and-its-brief)).
- Volume is in the app bar under key 64, never on a card (`volume_is_drawn_once_in_the_app_bar`).
- **Every routing target carries a `mxm_modulation_params::ui::stack` directly beneath the control it
  moves.** No Patch page, overview, hidden matrix or grid mode; no destination or source selector on
  any card; no input-name column or routing footer. S&H sits under Modulators; Voice carries no stack.
- A target with nothing routed draws no group, only `‹ modulate ›` (outside the group, carrying the
  target's name). A row ends in a remove; its amount is signed and bipolar; add and remove are each
  one begin/set/end gesture. `routing_shown_as_controls()` is lab-only: never in the shipped panel
  ([NOTES.md § Routing beneath](NOTES.md#routing-belongs-beneath-every-associated-control--never-passive-text-or-a-detached-footer)).
- Inputs are named for what they move, never *Modulator*; generator names come from
  `routing::SOURCE_NAMES`. A painted name drops the prefix its card title states
  (`sections::panel_label`, `routing::TARGET_PANEL_NAMES`); `TARGET_NAMES` and `ParamView::name`
  stay canonical ([NOTES.md § Names](NOTES.md#names-describe-generators-and-input-roles-never-the-current-patch)).
- Every card is a `mxm_ui::tree` whose floor is computed every frame, so every card is exactly as
  wide as its content; no card declares a usability minimum. `REFERENCE` and `MINIMUM` are held by
  `the_opening_size_is_the_budget_hugged` and `every_dynamic_page_fits_and_every_card_is_reachable`.
- No card carries a caption, and every tooltip says what its control does to the sound.
- The editor asks for a frame every 50 ms while open; closed, nothing runs.
- Route halves get their own permanent `navigation::at` ids; the coverage check reveals every route
  and opens Advanced (`REVEAL`) before it runs
  ([NOTES.md § Keyboard cursor](NOTES.md#the-keyboard-cursor-reaches-every-route-and-the-coverage-check-has-to-reveal-them)).
- `editor`, `params` and `telemetry` are `pub` for the layout lab (`apps/mxm-layout-lab`, in the
  private archive since the split): a bench API, not a host identifier. This crate owns parameter binding only; shared UI owns geometry and interaction.
- `tests/routing_editor.rs` must drive the real `editor::panel`; geometry or parameter-count tests
  are no substitute, and these checks must survive layout rewrites
  ([NOTES.md § Routing editor tests](NOTES.md#what-the-routing-editor-tests-prove-and-what-they-do-not)).

## Presets

- `preset.rs` is the `Instrument` impl of mxm-kit's `mxm-preset`; the fifty factory sounds are generated
  from `FACTORY_DESIGN`. Regenerate after any parameter change **or any change to what a parameter
  prints** with `cargo test -p mxm-mono-00 --lib write_the_factory_presets -- --ignored`; a print
  change's diff touches `text` lines only (`every_factory_text_is_this_plugins_formatting_of_its_value`;
  [NOTES.md § Presets](NOTES.md#presetrs-is-this-instruments-instrument-impl-and-its-factory-set)).
- A preset's resolution problems are shown, never hidden or dropped.

# Work Guidance

- Keep this file the contract: history, measurements and worked examples go in [`NOTES.md`](NOTES.md).

# Verification

```bash
cargo test -p mxm-mono-00
cargo test -p mxm-mono-00 --test routing_editor # every target through the panel; under a minute
cargo test -p mxm-mono-00 --lib every_card_passes_the_tree_checks_in_every_state
# Every page, light and dark, for review -> target/layout-tree/mxm-mono-00/<MXM_PICTURES tag>/
MXM_PICTURES=after cargo test -p mxm-mono-00 --lib tree_pictures -- --ignored
cargo test -p mxm-layout-lab --no-default-features --features mono-00 # the lab draws these cards (private archive only)
cargo clippy -p mxm-mono-00 --all-targets
cargo xtask bundle mxm-mono-00              # debug, for the allocation assertions
clap-validator validate "target/bundled/mxm-mono-00.clap"
cargo xtask bundle mxm-mono-00 --release
clap-validator validate "target/bundled/mxm-mono-00.clap" # validate each before overwriting
# then newdawn-workspace's collection-tests/editor_resize.rs: every editor, natively resized
```

- Run the validator against both the debug and the release bundle. **`param-fuzz-bounds` is not a
  formality here**: it found the phaser self-oscillating after the routing conversion
  ([NOTES.md § Verification](NOTES.md#verification-the-validator-runs-and-the-host-tests)).
- `host-tests/tests/behaviour.rs` loads the shipped bundle through MXM Player.
- **Built, not signed off. Not run by a person yet:** the converted editor, a real DAW, the other
  zooms and the dark theme, the brief's §15 QA gate and §12 trial, any listening comparison against
  hardware. Fidelity is UNVERIFIED.

# Child DOX Index

No child AGENTS.md files.
