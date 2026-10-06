# AGENTS.md — crates/mxm-mono-00-dsp

Parent: [`../../AGENTS.md`](../../AGENTS.md) · History, measurements and rationale: [`NOTES.md`](NOTES.md)

# Purpose

The mxm-mono-00 instrument as plain Rust — a System-100 as its plug-out pictures it, sounding as
the machine did. Framework-free, so the whole signal path is testable with `cargo test` and no
host involved. Four modules: the voice; VCO-2, sync, ring modulator and LFO-2; the S&H and the
matrix; the three effects ([NOTES.md § Purpose](NOTES.md#purpose-the-design-record-and-the-four-modules)).
The plugin that drives it is [`plugins/mxm-mono-00`](../../plugins/mxm-mono-00/AGENTS.md).

# Ownership

Owns `src/`, `examples/`, `Cargo.toml` and `NOTES.md`. Does **not** own parameter definitions, ranges, smoothing
or the editor — those belong to the plugin and to [`plugins/AGENTS.md`](../../plugins/AGENTS.md).
This crate takes plain values and a sample rate.

# Local Contracts

## The plug-out decides what exists; the hardware decides what it does

The plan's one rule, and every module cites the research section it follows. Where the two part,
the code says which won and why, and the warts are reproduced deliberately with a test that goes
red if somebody tidies them:

| Wart (`system-100.md` §11) | Where | The test |
|---|---|---|
| 1 — the LFO's three shapes sit at three DC levels | `lfo.rs` | `the_three_hardware_shapes_sit_at_three_dc_levels` |
| 3 — tremolo can only dip; nothing at zero gain | `vca.rs`, `voice.rs` | `the_lfo_cannot_open_a_closed_amplifier`, `tremolo_only_dips`, `tremolo_alone_on_a_closed_amplifier_is_inert` |
| 4 — PWM narrows from square and never widens | `routing.rs` (`narrowed_width`), `voice.rs` | `pwm_narrows_from_square_and_never_widens_from_any_source`, `a_routed_pulse_width_rests_at_square_whatever_the_manual_width` |
| 5 — PWM always takes the LFO's own triangle | `lfo.rs`, `routing.rs` (the `LFO*_CORE_TRIANGLE` sources) | `the_pwm_triangle_is_unipolar_and_ignores_the_switch` |
| 7 — the glide is one RC on a gate edge | `voice.rs` | `the_glide_dips_on_a_gate_edge_and_not_on_a_tie_and_recovers_in_its_own_time` |
| 8 — portamento is the hold capacitor | `voice.rs` | `portamento_holds_where_the_key_left_it`, and **no first-note snap** |
| 10 — "white" is high-passed, "pink" is two steps | `noise.rs` | `white_is_high_passed_near_three_kilohertz`, `pink_falls_about_three_decibels_per_octave_across_the_shelves` |
| 11 — low-note priority ignores a higher key | `voice.rs` | `low_note_priority_ignores_a_higher_key_over_a_held_lower_one` |
| 17 — sync is two strengths; weak locks only near the named ratios and "rolls" between | `oscillator.rs` | `strong_sync_pins_the_slaves_period_to_the_masters_at_any_ratio`, `weak_sync_locks_just_under_the_named_ratios_and_rolls_above_them` |
| the bass declines "slightly" with resonance | `voice.rs` | `the_bass_survives_resonance_to_the_chosen_slight_decline` |

## The patch bay (`routing.rs`, `matrix.rs`)

- `matrix.rs` holds the two tables that bind: **the evaluation order** and **the column scale**
  (ten-volt units). Changing the order changes every patch with a backward connection.
- An input takes any number of *(target, source)* pairs at signed depths, summed by `mxm_modulation`
  (`sum_split`, in the order the pinned digests were taken in); a source its stage has not yet
  published reads the previous sample. The frame unit is one: nothing clamps on publication.
- **The plug-out's wiring is data:** `INIT_PRESENT` and `INIT_AT_FULL` are the machine's normalled
  connections plus the panel's six, and they are the init patch. No `default_source` table, no
  `Route::Default`, no `None => <internal signal>` fallback, no `sync` switch: the keyboard gate and
  the glide's dip are sources, sync's presence is the switch, `syncstrength` picks strong or weak.
- **The panel's wiring is routing (Rev 3):** `VCO1_PITCH`/`VCO2_PITCH`, each read in its own
  oscillator's stage; `VCO1_WIDTH`/`VCO2_WIDTH` under `narrowed_width` (anything routed, zero depth
  too, narrows from square); `TREMOLO` (positive part only); one `CUTOFF`, each source keeping its
  own jack's reach; the S&H input (nothing contributing holds the last step; `Voice::sampler_is_on`
  is the one predicate); five LFO core sources, published only while read
  ([NOTES.md § Rev 3](NOTES.md#the-panels-wiring-is-routing-too-rev-3)).

## The modulation standard (`mxm_modulation::standard`)

- Key keeps the keyboard CV's unit, `(note − 60) / 120` (`KEY_UNIT_SEMITONES`). Velocity is `v − 1`
  of the press that last triggered an envelope (`Voice::envelope_velocity`), published after stage
  4, at full before any press and after All Sound Off.
- A performance source the machine did not have takes the standard reach (`takes_standard_reach`,
  `ADDED_SCALE`); generator pairs keep their row's reach. Offers come from `routing::offer`
  (`STANDARD_LAW`): seventeen pairs refused, the sync input offering generators their positive half.
- *VCA level* is `target::AMPLIFIER`; Amplitude is `target::AMPLITUDE`, after `Vca::process`, and
  `OUTPUT_BOUND` includes its doubling. A static exact mute is not `Live`
  (`amplitude_mutes_until_an_event`). `conformance.rs` runs the standard's checks
  ([NOTES.md § The standard](NOTES.md#the-collections-modulation-standard)).

## Combination laws and topology transitions

- **Sum** for continuous inputs. **Gate**: sum, then detect once against `GATE_THRESHOLD`. **Sync**:
  detect per route, reset by the *signed* largest depth; the amount is reset depth (soft sync, a DSP
  addition), full depth the hardware's reset to the bit.
- CV and gate routes **step**; each audio route ramps its own contribution over
  `AUDIO_ROUTE_RAMP_S`. A voice's opening topology and a reset **snap**.
- A topology change owes: a newly read source starts from silence (`SourceFrame::clear` in
  `Graph::set_topology`); a newly live sync detector from zero; a newly present route's smoother
  snaps to its stored value (the plugin's `routes::TargetRoutes::arm`).
- **Every path to `process` owes `set_topology`**, once per interval, never per sample;
  `Graph::begin_sample` panics in debug for a voice never armed.
- The grid travels beside the patch as `&Routing`, never inside `Patch`
  ([NOTES.md § costs](NOTES.md#what-the-conversion-costs-per-sample-measured)).

## Effects

- **phaser → delay → reverb**, after the VCA and before Volume; each bit-transparent at zero and
  emptied, not frozen, the first sample it is at zero.
- **The quiet snap:** an effect tracks what it *writes into its memory* and clears to exact zero
  after nothing above `SNAP_LEVEL` for its longest delay plus `SNAP_HOLD_S`. `holds_anything()`
  decides the verdict; `Voice::settle_samples_for` is only the `Tail(n)` promised to the host
  ([NOTES.md § The effects](NOTES.md#the-effects-are-chosen-models-behind-documented-controls-and-they-snap-to-exact-zero)).
- Emptying a line is O(1). `Delay` and `SpringReverb` size lines in `set_sample_rate`, from
  `activate`, never from `process`.
- **Spring tanks:** `Voice` never calls `set_model` (`Medium` only); lines are sized for
  `LONGEST_TRANSIT_MS`. A `tests/spring_tanks.rs` failure means the reverb changed — not a licence
  to update the digest, which is pinned on Windows only ([NOTES.md § The spring](NOTES.md#the-spring-has-three-tanks-and-the-instrument-can-only-have-one-of-them)).

## Filter and sample rates

- The resonance top is `RESONANCE_MARGIN` past the **discrete** threshold
  (`DiodeConfig::discrete_threshold`, via `native_resonance`), so the slider sings at the same
  position at every cutoff; `no_configuration_asks_for_more_than_k_max` holds `K_MAX`
  ([NOTES.md § Resonance](NOTES.md#the-resonance-control-sings-from-the-same-position-at-every-cutoff)).
- Only the ladder runs at twice the rate (`oversample.rs`); `Voice::latency_samples` exposes
  `LATENCY_SAMPLES` and the plugin deliberately does not report it. Above `max_nominal_cutoff_hz`
  the poles pin one by one ([NOTES.md § The ladder](NOTES.md#the-ladder-runs-at-twice-the-rate-and-past-its-exact-ceiling-the-poles-pin-one-by-one)).
- `hpf::NYQUIST_FRACTION` bounds every prewarped corner; `MIN_SAMPLE_RATE` is the lowest activation
  rate, as `f32::clamp` panics on crossed bounds. It passes NaN: the NaN guard is nice-plug's setter.
- The phaser's MANUAL IN has a control port's bandwidth (`PHASER_MANUAL_HZ`,
  `PHASER_MANUAL_FRACTION`), a **stability** requirement: never swap it for a bound on the feedback
  state ([NOTES.md § A swept corner](NOTES.md#a-swept-corner-also-needs-a-bounded-speed)).

## Oscillators, pitch, gates and triggers

- `Vco::process_slave` holds each sample one call, so PolyBLEP corrects both sides of a reset with
  **no latency**. STRONG resets on the master's **rising** edge; a WEAK kick past the threshold
  discharges to zero, **losing the excess**, or no ratio locks
  ([NOTES.md § Sync](NOTES.md#sync-the-slave-holds-one-sample-so-a-reset-can-be-corrected-on-both-sides-with-no-latency)).
- The triangle comes from the raw phase, never from folding the PolyBLEP'd saw. The ring modulator
  is not oversampled yet; `ring_modulation_aliases_no_worse_than_this` records its aliasing.
- Portamento charges the hold capacitor only while a key is down, no first-note snap, carried as
  remaining distance; glide dip and vibrato are added after it, per VCO, on its own pitch input.
- `process` reconciles the target with `stack.sounding(p.priority)` every sample, **no gate edge**.
- Each gate input has one `GateRow` fed the **summed** level, so a level-changing repatch is an edge.
  A GATE+TRIG press needs a gate route present **and carrying something**, a high gate and a key
  still down: a note ending at its own offset fires nothing (`note_off`, `choke`, `all_notes_off`
  cancel the pending press) ([NOTES.md § Gate input](NOTES.md#a-gate-input-watches-its-summed-signal-whatever-is-on-it)).
- `Trigger::Lfo` ANDs the gate with `Lfo::square_high`. A tie raises no gate: GATE does not
  retrigger, GATE+TRIG does, the glide does not fire.
- All Notes Off (CC 123) clears the stack alone; a choke leaves the effects' tails; All Sound Off
  (CC 120) clears all that holds sound and the settle clock, leaving LFOs, S&H, oscillators running.

## Activity: decided from configuration and control state, never from the audio

- Only the settle clock hears the signal. Live only when the amplifier can open without a key:
  INITIAL GAIN up, a present nonzero-depth route into it carrying a self-running source, or its input
  held open (`amplifier_held_open`). The gate and the four performance inputs are not self-running.
- A mixer level counts only if a route at a depth reaches it, a static source only while nonzero;
  a tremolo never makes a patch live. A tail is an envelope the amplifier reads; `tail_samples`
  takes the routing. Once inert, an envelope nothing can hear is silenced, not frozen. Inert output
  is exactly silent ([NOTES.md § Live](NOTES.md#live-is-decided-from-the-configuration-and-the-control-state-never-from-the-audio)).

## Constants and dependencies

- [NOTES.md § What is chosen](NOTES.md#what-is-chosen-not-measured) lists every chosen constant and
  those copied or re-argued from siblings. Keep it complete.
- **`mxm-modulation` is the one runtime dependency**, dependency-free at the 1.87 floor; check by hand
  with `cargo tree -e normal,build -p mxm-mono-00-dsp` that no GUI crate arrives. Every other edge
  is test-only and reaches no bundle ([NOTES.md § Dependencies](NOTES.md#dependencies-one-at-runtime-and-it-is-why-the-floor-holds)).

## Everything else the collection's DSP already requires

No framework types; realtime rules on every per-sample path; denormals flushed in the DSP itself;
`f32` in the audio path and `f64` for prewarping and coefficients; every saturator bounded exactly
and monotonic; stated `pub const` output bounds (`filter::OUTPUT_BOUND`, `voice::OUTPUT_BOUND`);
deterministic seeded randomness, load-bearing here because the export renders through a second
instance. These were the monorepo root's and are the sibling crates' — mxm-mono-01's
[`crates/mxm-mono-01-dsp/AGENTS.md`](https://github.com/mxm-audio/mxm-mono-01/blob/main/crates/mxm-mono-01-dsp/AGENTS.md)
states them — not restated.

# Work Guidance

- Only the routing frame is extracted (into `mxm-modulation`); the rest is copied and not yet
  extracted from ([NOTES.md § The fourth honest copy](NOTES.md#the-fourth-honest-copy-what-this-crate-owes-the-extraction-plan)).
- What remains is the plugin's: its editor, and the fidelity gate's
  listening comparison, which no test here can stand in for.
- Filter theory is [`docs/filters/`](https://github.com/mxm-audio/mxm-kit/blob/main/docs/filters/README.md); oscillators
  [`docs/oscillators/`](https://github.com/mxm-audio/mxm-kit/blob/main/docs/oscillators/README.md); envelopes, the LFO and the glide
  [`docs/modulation/`](https://github.com/mxm-audio/mxm-kit/blob/main/docs/modulation/README.md). Read the chapter before changing a module.
- Prefer a clear implementation to a clever one. This is reference-quality open source.
- Keep this file the contract: history, measurements and worked examples go in [`NOTES.md`](NOTES.md).

# Verification

Measurements come from `mxm-measure`; every bound and its headroom stays in its test.

```bash
cargo test -p mxm-mono-00-dsp
CARGO_TARGET_DIR=target/msrv-1.87 cargo +1.87.0 test -p mxm-mono-00-dsp   # the declared floor: no let chains
cargo clippy -p mxm-mono-00-dsp --all-targets
cargo run -p mxm-mono-00-dsp --release --example system_demo   # one passage per mechanism
cargo run -p mxm-mono-00-dsp --release --example mono_00_cost  # the per-sample cost gate
```

The tests must keep asserting every property in [NOTES.md § Verification](NOTES.md#verification-the-rulers-the-demo-and-the-properties-the-tests-keep),
because each regresses silently: exact zero after the tail, no NaN or inf at any rate, `reset()`
leaving no tail, the wart table, the activity verdicts, two instances rendering identically.

**Fidelity is UNVERIFIED** and must not be claimed: no hardware was measured, and the listening
comparison has not been run. Linux is checked in WSL before a push and macOS only by CI on a release
tag ([NOTES.md § Not verified](NOTES.md#what-is-not-verified-and-must-not-be-claimed)).

# Child DOX Index

No child AGENTS.md files. `src/` and `examples/` are covered by this doc.
