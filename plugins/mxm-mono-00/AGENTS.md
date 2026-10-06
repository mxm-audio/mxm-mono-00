# AGENTS.md — plugins/mxm-mono-00

Parent: [`../AGENTS.md`](../AGENTS.md)

# Purpose

The nice-plug shell for **mxm-mono-00**, a monophonic semi-modular inspired by the Roland
System-100 as its plug-out pictures it. Identity, parameters, MIDI, presets, telemetry and the
editor. The whole voice is [`crates/mxm-mono-00-dsp`](../../crates/mxm-mono-00-dsp/AGENTS.md).

Shared conventions — nice-plug's API, the init-patch contract, preset rules, `process()` realtime
rules, the editor contract — live in the parent and are not restated here. This doc holds what is
**local to this plugin**.


# Ownership

`Cargo.toml`, `LICENSE`, `README.md`, `control-map.json`, `presets/`, and `src/` — `lib.rs`,
`params.rs`, `preset.rs`, `routes.rs`, `telemetry.rs`, and `editor.rs` with its
`editor/{binding, sections}.rs`. `tests/routing_editor.rs` exercises the shipped panel's routing
stacks through an applying host stub.

# Local Contracts

## Permanent identifiers

| What | Value |
|---|---|
| `CLAP_ID` | `dk.mxm.mxm-mono-00` — assembled from `plugin_name!` in `src/lib.rs`, **not** from `CARGO_PKG_NAME` |
| Parameter `#[id]`s | **Voice:** `priority` `tune` `bendrange` `portamento` `volume`<br>**VCO-1:** `range1` `coarse1` `fine1` `wave1` `pw1`<br>**VCO-2:** `range2` `coarse2` `fine2` `wave2` `pw2` `syncstrength`<br>**Mixer:** `vco1level` `vco2level` `noiselevel` `ringmodlevel` `noisecolour`<br>**Filter:** `hpf` `cutoff` `resonance`<br>**Amplifier:** `initialgain` `tone`<br>**Envelopes:** `vcfattack` `vcfdecay` `vcfsustain` `vcfrelease` `vcftrigger` and `vca…` likewise<br>**Modulators:** `lfo1rate` `lfo1shape` `lfo1offset` `lfo1sync` and `lfo2…` likewise<br>**S&H:** `shrate` `shlag` `shsync`<br>**FX:** `phaser` `delay` `delaytime` `temposync` `reverb`<br>**Routing:** 433 offered pairs of 450, `<target>_<source>` and `<target>_<source>on`, from eighteen `#[nested(id_prefix = …)]` groups whose `Params` impl is written by hand to leave the seventeen refused pairs out — the table is [`src/routes.rs`](src/routes.rs)'s `ROUTE_IDS` |

**918**: fifty-two controls and 866 routing parameters, read off the parameter map and held by
`every_parameter_has_a_distinct_id`. Treat both as public interface. (This doc said 655 before Rev 3,
when the map already held 657, 899 before the three tempo syncs of 2026-09-25, and 902 before the
modulation standard added the Amplitude's fifty and retired thirty-four, 2026-09-27.)
`every_parameter_has_a_distinct_id`, `every_parameter_is_drawn_exactly_once` and
`routes::tests::the_id_table_is_what_the_derive_actually_produces` keep the list, the table and the
editor agreeing with the derive.

## The ids retired with the routing conversions, and none may come back

**Rev 3 retired eleven controls and 200 route ids** (`plans/plan-mxm-mono-00-modulation.md` Rev 3,
the owner's ruling of 2026-09-22 that a source is chosen on the card it moves). The controls:
`glide` `glidedest` `vcolfo` `vcolfodest` `pwmsrc1` `pwm1` `pwmsrc2` `pwm2` `vcalfo` `keytrack`
`shmode` — DESTINATION chose a destination at the source, and the rest wired a depth to one source
or chose a source by a switch. The route ids: all forty of `mod_glidein_*` (GLIDE IN is a direct
pitch route now, its exact equivalent), `mod_vco1cv_*` (VCO-1's pitch input took a fader taper and a
144-semitone reach) and `mod_shin_*` (the S&H input's presence turns the sampler on, where it only
fed the EXT position); and `mod_vcfenv_*` and `mod_vcflfo_*`, when the Filter's three cutoff inputs
became one Cutoff, each source at the reach its own jack gave it. **A changed meaning under a kept id is the thing this section forbids**, so the
last two were re-declared as `mod_vco1pitch_*` and `mod_shsrc_*` rather than reused. The owner ruled
the removal of a target an acceptable reading of *the target list is chosen once*: its ids retire,
and DSP positions are never stored. `routes::tests::no_retired_id_reappears_among_the_routing_parameters`
holds all of them.

**The modulation standard retired thirty-four more** (`plans/plan-modulation-standard.md`,
2026-09-27): the seventeen pairs from a performance source the machine never had into a target where
they can never mean anything — Velocity, the wheel, pressure and the lever into either envelope gate
(`mod_vcfgate_*`, `mod_vcagate_*`), the sync input (`mod_vco2sync_*`) or the mixer's audio input
(`mod_mixin_*`), and Velocity into the S&H (`mod_shsrc_vel`) — each amount and presence. They are
never minted; the same test holds them out, and a session or preset naming one loads everything else.

**Twenty-three retired before that**, with the patch bay's conversion:

the fifteen row selectors (`lfo1cvin` … `phasermanualin`), **the seven row attenuators**
(`lfo1gain` `lfo2gain` `extcv1` `ringlevel` `filterenv` `filterlfo` `vcaenv`) and **`sync`**. Each
named something that stopped existing: a row chose one source where a target has one slot per source;
an attenuator was *one depth shared by whatever was on that row*, which is what decision 1.6 replaces
with a depth per route; and whether VCO-2 is synced is now whether anything is routed to its sync
input. `syncstrength` stays — strong or weak is the hardware's own switch and a different question.

**None could be re-used in place.** `plan-modulation-routing.md` §5.1's reason is arithmetic rather
than policy: a live id's range cannot widen from `0…1` to `-1…1` without re-pointing every stored
value, because both cover the whole normalised input.

**What no migration can carry is a host automation lane.** A project that *set* one of these keeps
every other parameter and loses its routing; one that *automated* one loses that lane, and no state
rewrite can rebind it. Accepted on an unreleased instrument whose fifty factory sounds are
regenerated either way. `routes::tests::no_retired_id_reappears_among_the_routing_parameters` is what
stops a name whose meaning changed being quietly re-used.

## A route's amount reads what that pair delivers

`routing::TARGET_SCALE` is per **unit of source**, and this machine's sources do not all fill the
ten-volt unit — an envelope peaks at 0.6, a VCO at 0.5, an LFO at 1.0. So what a player reads on a
route is `scale × routing::SOURCE_PEAK[source]`: **what that pair delivers at full amount**, which
is the number the knob it replaced always meant — seven octaves from the filter envelope, four from
the filter LFO, `+100 %` on the mixer's channel and on the amplifier's input, 27 % and 45 % of
pulse-width narrowing from the LFO triangle and an envelope, and — read per octave like every Key
route, the collection's standard — `+1.00 oct/oct` of key tracking (it read the retired slider's
`+100 %`). The phaser's two inputs read in octaves, four per unit. Readings are the shared
`mxm_modulation_params::reading`. The pitch inputs reach **144 semitones per unit** (`routing::PITCH_SEMITONES_PER_UNIT`), so VCO-2 into VCO-1
reads ±72 and an LFO or the glide ±144: EXT CV's 120 plus the glide input's 2 and the vibrato's 12,
which one source could once reach at the same time, fits inside one route. `a_route_at_full_amount_reads_the_machines_own_number` pins each against the
DSP's own constant, so a scale that moves moves the reading with it.
**It never prints a negative zero**: its number goes through `mxm_modulation_params::signed`, and
`every_reading_survives_the_hosts_round_trip_a_rounded_zero_included` sends every amount through
the host's own conversion either side of zero, where a plain signed format printed `-0`, which
`clap-validator`'s `param-conversions` fails whenever its random values land there.

**`SOURCE_PEAK` is display-only.** Nothing evaluates with it; a source that exceeds its peak is
bounded by the frame, not by that table.

## Every parameter's text survives the host's round trip

A host parses a parameter's text and **normalises the number before printing it again**, so the
value it prints lands a hair either side of where it started. A formatter that switches unit or
precision at the raw value is not idempotent there, and `clap-validator`'s `param-conversions` fails
only when its values land in that sliver. So in `src/params.rs` the unit is chosen from what the
finer branch would print: `v2s_time` prints seconds for anything that would read `1000.0 ms`, and
`v2s_hertz` — which replaces nice-plug's `v2s_f32_hz_then_khz(1)` on the HPF, the cutoff and the
rates — kilohertz for anything that would read `1000.0 Hz`. `v2s_percent` never prints `-0 %`, which
Key follow and Tone reached either side of zero.
`params::tests::every_parameter_reads_the_same_after_the_hosts_round_trip` holds all 918, converting
as the wrapper does, with the unit: at the validator's grids, either side of each branch point, and
at every representable normalised value near each. Every envelope time, Portamento, S&H lag, Delay
time, the HPF, the cutoff, Key follow and Tone failed it before the fix.

## The patch bay is 433 parameter pairs, not state

A route is a *(target, source)* pair carrying a **presence** and a signed **amount** — eighteen
targets by twenty-five sources, less the seventeen the modulation standard refuses, one
`#[nested(id_prefix = …)]` per target in [`src/routes.rs`](src/routes.rs).
There is no source selector, because a target has one slot per source and a slot *is* a source.

**Both halves are automatable and per-step modulatable** like any other parameter, which is what lets
a sequencer re-patch the machine mid-pattern; neither is `#[persist]` state, and the parent's
*editable models* rule does not apply because the set is fixed and only a release can add to it.
`the_routing_groups_are_the_dsp_targets_in_declared_order` pins the order against the DSP's.

**Removing a source is one parameter write and leaves the amount alone**, so re-adding it restores the
depth the player last set — design system §8.7's undo with no undo stack, and what the player reads as
a step lock rather than a preset load. `tests/routing_editor.rs` proves both gestures on every target
through the shipped panel.

## Three effects, and two of them by exemption

**The phaser and the delay are here although no System-100 module had either.** The feature set is
the SYSTEM-100 plug-out's, and the owner's ruling of 2026-09-02 exempts this instrument — and only
this one — from *no effect that was not on the original instrument* (root `AGENTS.md`;
`plugins/AGENTS.md`, *An instrument ships the effects its original had, and no others*). **The
spring reverb needs no exemption**: the 103 mixer had it. `research:instruments/system-100.md` §12
and §14 hold the evidence for what the hardware had.

The reverb has been promoted to `mxm-folded-spring`. The phaser and the delay wait for a reference
box and a name of their own (`plans/plan-mxm-fx-collection.md` §1), not for a decision to promote.

## Four MIDI paths this machine's hardware never had

`plan-modulation-routing.md` decision 1.7 puts the collection's performance inputs on every
instrument, and **this keyboard has none of them**: no velocity, no aftertouch, and a mod wheel with
nothing to reach. So the conversion adds four event paths — velocity from note-on, CC 1 and channel
pressure per channel, and the bender's own normalised position beside the semitone path it already
had. **The owner's ruling, 2026-09-14, and it provably costs nothing where it starts**: every one of
their pairs is absent in the init patch, so a fresh instance is still the copy.

**They mean what they mean on every instrument** (`plans/plan-modulation-standard.md`,
`crates/mxm-mono-00-dsp/AGENTS.md`): Velocity `v − 1` of the press that last triggered an envelope,
the wheel, pressure and lever through the standard's publishers, Key in the keyboard CV's own
1 V/oct. Where the machine never had the pair they take the standard reach — 12 st of pitch, 4
octaves of LFO rate or cutoff, and Key into either pitch 12 st/oct — and **a pair that can never mean
anything is not offered** (the retired thirty-four, above). A one-sided target offers only its live
half: the VCA level ← Velocity 0…+100 %, the widths and the tremolo ← Velocity −100…0 and ← the wheel
or pressure 0…+100 %. **The sync input's faders are 0…+100 % for every generator**, because a reset
has no inverse. The machine's CV amplifier reads as **VCA level** (`mod_vcacv_*`); **Amplitude**
(`mod_amplitude_*`) is the standard factor after the VCA, a stack on the Amplifier card, nothing
routed to it at Init. `every_route_parameter_says_what_the_dsp_does` (`mxm_plugin_test::routing_checks`)
holds every pair's registration, travel, unit and reading to `mxm_mono_00_dsp::conformance::Declared`.
The factory files were regenerated: every plain value is the same — the only normalised values that
moved are one-sided pairs' zeros and the sync depths on their new range.

The control map's CC 1 reservation is untouched. The wheel is still consumed as a gesture and writes
no parameter; what is a parameter is *how much of it reaches this target*, which is the amount every
other route has (§5.2 part 1).

## Oscillator 1 has coarse and fine tune, which the plug-out took away

The 101's VCO had a continuous FREQUENCY knob and FINE TUNING, and the 102's likewise (research
§3.1). The plug-out gave VCO-1 only its range switch and kept coarse and fine for VCO-2 alone
(§12.3), which left the ring modulator's carrier-and-multiplier pair with one oscillator that could
not move against the other — the owner's finding, 2026-09-22. `coarse1` and `fine1` put the
hardware's tuning back, laid out as Oscillator 2's; both start at zero, so Init and every factory
sound are unchanged. An interface limit of the plug-out, which a copy is free to lift.

## The panel's own wiring is routing too, and no card chooses where its output goes

Rev 3 (`plans/plan-mxm-mono-00-modulation.md`) finished what the patch bay's conversion started:
**every modulation path the plug-out hard-wired on its panel is a route on the card it moves**, and
every path the machine had is still reachable — the plan's inventory row by row, and the hardware
102's own EXT CV (the S/H's only internal destination) added back on VCO-2's pitch input.

| Was | Is |
|---|---|
| GLIDE and VCO LFO with DESTINATION (VCO-1 / VCO-2 / both) | `Glide` and `LFO 1` on each oscillator's pitch input, both wired at zero at Init |
| GLIDE IN | Any source on either pitch input |
| The two PWM switches (six positions) and depths | Each oscillator's pulse-width input; LFO positions are the LFO cores' triangles |
| VCA LFO (tremolo) | The amplifier's tremolo input, LFO 1 wired at zero; dips only (wart 3) |
| VCF KYBD CV (key follow) | The filter's cutoff input, the keyboard wired at zero; `+1.00 oct/oct` is full tracking |
| SAMPLE MODE | LFO 1's saw, reverse saw, triangle or sine on the S&H input; nothing contributing is OFF |

**Wart 4 is a law, not a switch.** With nothing routed to a pulse width the manual width stands;
with anything routed — at zero depth too, as the PWM switch's LFO position with its slider down —
the pulse narrows from square and the manual width stands aside (`routing::narrowed_width`).

**The pitch rows' own faders are square-law about the centre** (`routes::PITCH_TAPER`, the owner's
ruling of 2026-09-22): a tenth of the travel either side is ±1.44 semitones from an LFO, the retired
Vibrato knob's twelve are at about 29 %, and the far end is the whole 144. Plain values, and so the
DSP and the readings, stay linear. Shift is the fine drag. **Key's 1 V/oct and a performance pair's
twelve semitones are linear** — the taper is for a 144-semitone reach (the modulation standard).

**Kept as they are**, each a circuit or a trigger mode rather than a CV path: the envelopes' LFO
trigger position (LFO 1's square ANDed with the gate, §7.1), the ring modulator's X input always
being VCO-2 (wart 15), the bend's hard-wired path beside its `Bend` source, and portamento.

## The init patch, its deviations, and the wiring that is now data

Every amount starts at zero — `every_amount_starts_at_zero` names eleven of them, and every route
amount is checked by `the_routing_starts_as_the_machines_own_patch`. Three things start
up because a silent init patch reads as broken: VCO-1's level (0.8), the amplifier's envelope route
(1.0, what `vcaenv` was), and the cutoff (10 kHz). **VCO-2's fine tune starts at +7 cents**, the init
contract's *start slightly detuned*, chosen and recorded in the DSP crate's table;
`vco2_starts_slightly_detuned` pins it.

**The plug-out's thirteen normalled connections are the init patch** — nine on the patch bay less
GLIDE IN and less the ring modulator's mixer channel, which is a slider again, and six on the
panel — which is the whole of the routing
conversion: what the machine came wired as is a set of present pairs a player can pull out, not wiring
in the voice. `crates/mxm-mono-00-dsp`'s `INIT_PRESENT` and `INIT_AT_FULL` are the list and
`the_routing_starts_as_the_machines_own_patch` holds the parameters to it.

Nine inherit the zero their control held — VCO-1's pitch input from VCO-2, the filter's two inputs,
and the panel's six: the glide and LFO 1 on both
pitch inputs (DESTINATION's default was both), LFO 1 on the tremolo and the keyboard on the cutoff.
**Four had no attenuator to inherit from, and start at full**: that is the collision `plan-modulation-routing.md` §7.1 names between decision 1.6 —
every route carries a level — and the init contract's *every amount starts at zero*, and the answer is
the same each time: **a route whose depth was not a control before the conversion has no zero to
inherit.** Three of the four were not amounts at all; they were the jack's internal connection,
which is either made or not.

| At full | What it was |
|---|---|
| Filter envelope gate ← Gate | The keyboard gate on the VCF envelope. Without it a fresh instance never sweeps |
| Amplifier envelope gate ← Gate | The same on the VCA envelope. Without it a fresh instance never sounds |
| Ring mod ← Oscillator 1 | The ring modulator's Y input (p. 18). Inaudible at Init anyway: the ring mod level that carries it is at zero |
| VCA level ← Envelope 2 | `vcaenv`, which already started at 1.0 — this one inherits its old default rather than acquiring a new one |

**Key follow is bipolar**, the keyboard route's `-1..=1` centred on zero: the plug-out's VCF KYBD CV is, where the
hardware’s slider was positive-only (`system-100.md` §12.3), and the plug-out decides the
available range.

## The process status is the patch's activity

The DSP's `Voice::activity` decides *live*, *tailing* or *inert* from the configuration — *live*
never from the audio; only the effects' settle hears the signal, restarting while their input is
above their snap level — and `process()` maps them to `KeepAlive`, `Tail(n)` and `Normal`. **The instrument can be live with no key down**—the S&H clock gating the
amplifier's envelope, INITIAL GAIN up — and a host that stopped calling it would stop the patch.
**A tail is only what the amplifier reads**: `Tail(n)` comes from `Voice::tail_samples` with the
routing in force, so a filter release that only moves the cutoff is `Normal` once the amplifier's
own release has settled, and one routed to the amplifier is promised its length (the DSP crate's
*Activity* section; `process_status::the_status_follows_the_envelopes_the_amplifier_reads` holds
both through the callback, and that re-routing afterwards revives nothing).
**A live route's depth or a control the verdict reads, still ramping, is `KeepAlive` whatever the
verdict**: the DSP judges from the value reached so far — a gate's threshold, the resonance's
excitation, a level, INITIAL GAIN, Volume, an effect that sets the tail — and a value moves only
while the host calls, so a block ending mid-ramp must not let a host park it short
(`Routes::is_ramping`, `controls_are_ramping`; `a_live_route_still_ramping_is_reported`,
`a_resonance_ramp_is_a_control_still_ramping`; code review, 2026-09-22).
The activity is published to telemetry once per block and shown as one glyph in the app bar.

## Notes are handed to the voice with their identity, and the voice keeps the stack

The DSP owns the held-key stack and the priority rule (`priority`, low-note by default as the
hardware's). Every note event passes `voice_id`, `channel` and `note` through, and the voice
matches releases and chokes against what it holds — so a stale release for a replaced note is
dropped there, not here. **A tie is a note-on without a gate edge**, which the DSP decides; the
shell reads no gate. The last sample's `Patch` is kept on the plugin so an event has one to read
the priority from without advancing a smoother twice.

**The bend follows the sounding note’s channel, and expression has an owner.** Under low-note priority a higher key on another channel does not take the bus, so its
channel's bend must not reach the note that kept it: the channel is read from
`voice.sounding()` each sample, the last sounding note's channel holding through a release.
`PolyTuning` is stored with the `NoteId` it was sent for — **matched by voice id when the host
gives one**, so two presses of one key are two voices—and applied only while
that note is the one sounding; when the bus falls back to an older held key it reads zero,
because no expression was ever addressed to that key; a release keeps it, and a note-on that takes
the bus clears it. **One that does not — a higher key under low-note priority — leaves it alone**
(`a_press_that_does_not_take_the_bus_keeps_the_sounding_expression`). **A release holds only the
releasing note's own expression**: when the last key up never owned it, it is retired
(`a_release_after_a_fallback_does_not_revive_another_notes_expression`). **A non-finite tuning or bend is dropped** at the event, and the DSP treats a
non-finite pitch offset as zero besides. **Every event is handled under the priority in force** — read from the parameter at the
event, since at a block boundary it may have moved since the last rendered sample. A per-note **Choke stops the note and leaves the effects' tails running**; All Sound Off
clears them. **Velocity drives no hard-wired path** — the hardware's keyboard has none — but a
note-on's velocity feeds the routable Velocity source. The voice keeps each press's velocity in its
note stack, as `mxm-mono-02` does, so a key that does not take the bus leaves the sounding press's
alone and a fallback brings back the held key's own; **the source is `v − 1` of the press that last
triggered an envelope** (the modulation standard), so a legato press under GATE keeps the phrase's.

## Tempo sync is resolved once per block from the transport

Four controls sync, each by the collection's one contract (`plugins/AGENTS.md`, *Tempo sync*;
`crates/mxm-tempo`): the delay time (`temposync`, host name *Delay sync*), the sample clock
(`shsync`) and both LFO rates (`lfo1sync`, `lfo2sync`, the owner, 2026-09-25). With a switch on and
a tempo arriving, the control's **modulated** position — so a sequencer's lock picks the division a
moved control would — picks a division on its ladder (`params::DELAY_SYNC`, `SH_SYNC`, `LFO_SYNC`),
clamped to what its range holds; the delay's and the clock's are 1/32 to a half note, the ends of the
eight-step table they had, and the LFOs' the collection's 1/32 to four bars. **Stored positions
between the ends moved**: the shared ladder has the dotted divisions and 1/32T that the old table
(1/32, 1/16T, 1/16, 1/8T, 1/8, 1/4T, 1/4, 1/2) skipped, so a synced delay or clock saved before
2026-09-25 can come back on a neighbouring division; no migration can carry an automation lane.
**Accepted by the owner** (2026-09-25, *"Accept it"*), over keeping the eight-step table for mono-00
alone or converting saved projects: the instrument is unreleased. The one synced factory sound,
*Delay stabs*, is written as `DELAY_SYNC.position(QuarterTriplet)` and keeps its division.

**A synced rate is fastest at the top**, as it is free. The rate CV
and the LFO offset move a synced LFO from its division exactly as they move a free one. With a
switch off, or no tempo, the control's own value stands; its smoother is advanced every sample
either way. The tempo in force is `Telemetry::tempo`; a synced knob reads its division with one and
its free value without. `tempo_sync_picks_a_division_of_the_tempo_and_is_inert_without_one`,
`a_synced_rate_ticks_once_per_division_fastest_at_the_top`,
`the_synced_cards_keep_their_size_as_sync_turns_on`.

## Activation refuses a rate the DSP cannot hold

`activate` returns `false`, before anything changes, for a non-finite host rate or one below
`mxm_mono_00_dsp::MIN_SAMPLE_RATE`, 1 kHz: a NaN rate, or one low enough for a corner's floor to
cross 0.45 of it, panicked on the audio thread.
`activation_refuses_a_non_finite_rate_and_any_below_the_floor` holds the refusal, and
`the_rate_floor_activates_and_plays_at_every_parameter_extreme` a held note at the floor with every
parameter at its default and at either end.

## Twenty-two samples of latency, deliberately not reported

The DSP runs its ladder at twice the rate between a half-band pair, which delays the audio by
`Voice::latency_samples()` — twenty-two samples at the base rate, about half a millisecond at
48 kHz. **`activate` does not report it**, and the first build that did could not be loaded by
the player: nice-plug turns a latency report into `request_restart` when the plugin is already
active, the restart runs `activate` again, which reports again, and the player's headless session
does not come back from the restart. A report only on the first activation would avoid the loop
but not the restart. Half a millisecond on an instrument is below any host's note-alignment
concern, so the figure stays a documented fact rather than a declared one. Revisit if a DAW test
shows it mattering.

## Smoothed and unsmoothed, and the one exception

Levels, amounts, the cutoffs, pulse widths, tune, sustains and the LFO offsets are smoothed
(10 ms linear; the corners logarithmic). Envelope times, rates, every switch, the matrix and the
trigger modes are not — a constant is not a signal, and a route ramped between two sources would
be neither. **The delay time is the one time that is smoothed** (50 ms): the line interpolates, so
a moving time is a pitch-bending repeat rather than a click.

## A developer channel, off unless the environment asks for it

With `MXM_DEV_CC` set in the plugin's process environment when an instance is made, CC 119 selects
a stable category (**0 Performance, 1 Modulators, 2 Sequencers, 3 Generators, 4 Tone, 5 Effects**) or **127 Parameters**, with other values ignored and CC 118 opens (≥ 64) or closes the Voice card's expander, and CC 117 opens or closes the preset browser. CC 116 sets the theme by index — 0 light, 1 dark, 2 system — without saving it. It exists
because CLAP gives a host no way to put a plugin's editor in a state — the player's CLI can drive
the player, not the window inside it — and a screenshot run, a script or an AI needs to, without a
mouse; through the player it is `mxm-cli cc 119 1`. **The gate is the environment, not a
parameter**, so no host, preset or automation lane can trip it and a person who did not set it
cannot see it. The requests go through `Telemetry`'s atomics like every other word from the audio
thread to the editor, taken once; the DSP reads nothing.
`the_developer_channel_is_off_unless_the_environment_asked_for_it` holds the gate. Built here
first and then made the collection's rule the same evening: `plugins/AGENTS.md`, *A developer
channel in every editor*.

## The control map claims the roles that existed before this instrument

`control-map.json` fills thirty-five roles. **The LFO 2 page and the `fx.*` roles were added to
the standard for this instrument and are deliberately absent**: a map naming a role a player's
compiled-in standard does not declare is refused whole (`docs/MXM_CONTROL_MAP.md` §9), so claiming
them would cost every other mapping on any player built before they existed. `filter.hpf` is also
left unfilled: its curve metadata is not in the standard.

**Six roles point at routing pairs**, because the controls they named became routes:
`osc2.sync` is the *presence* of `(Oscillator 2 sync ← Oscillator 1 sync)` — which is what the
retired `sync` switch was — and `filter.env_amount`, `filter.key_track`, `filter.lfo_amount`,
`lfo1.to_filter` and `lfo1.to_amp` are the *amounts* of the plug-out's own normalled routes.
`mixer.src3` is the ring modulator's own level, `ringmodlevel`. **Three roles are left unfilled on purpose**, and the map says why:
`lfo1.to_pitch`, because the vibrato reached both oscillators and one parameter cannot drive two
routes; the two `pwm_source` roles, because there is no switch; and the two `pwm_depth` roles,
because Init routes nothing to a pulse width, as MANUAL routed nothing, so a depth would be dead.

**The rule, and it is load-bearing rather than incidental: a role may be filled by a route's
*amount* only where Init wires that route.** An absent pair contributes nothing whatever its amount
holds, so a controller knob bound to the amount of a route the machine does not wire is a **dead
knob** — `mxm-mono-pr1` paid for that lesson first (`plan-modulation-routing.md` decision 1.8). All
five amount roles above are wired at Init.

**A *presence* is live only if the amount it finds is**, which is why the sync normal's amount
**waits at full while the route is absent**: `osc2.sync` writes the presence alone, and at zero it
created a route that reset nothing — dead from the 2026-09-14 conversion until the code review of
2026-09-22 (the owner's ruling). Turning it on now syncs at the hardware's full reset, which is what
the retired switch did; Init still wires nothing, so a fresh instance is unchanged, and the
presets carry the new default. `a_control_map_role_never_points_at_a_dead_route` and
`the_sync_switch_role_turns_on_a_full_reset` hold it. Only saved sessions from before keep the zero
they stored.

## The editor, and its brief

[`docs/briefs/mxm-mono-00.md`](../../docs/briefs/mxm-mono-00.md) gates the editor.
`page_items` keeps thirteen stable keys: Voice (0) is Performance; LFOs/S&H/envelopes
(1,2,3,8,10) are Modulators; oscillators (4,5) and **Ring mod (14)** are Generators;
Mixer/Filter/Amplifier (6,7,9) are Tone; the one **Effects** card (11) is Effects. **A key is a
card's permanent id, not its position**: Ring mod came after the effects had 11–13, so it took 14
and sits between Oscillator 2 and the Mixer by `sections::SYNTH_KEYS`, and no key after it moved.
When the three effects merged, their card kept 11; 12 and 13 are retired, never reused.
The LFO pair and the two oscillators with Ring mod remain preferred groups; former envelope/destination groups split across categories. No card's body is re-cut.
Parameters remains a separate scrolling diagnostic surface with no tab.

**Volume is in the app bar, not on a card** — design system §3.1 item 6 puts the master output
there, and the owner ruled every instrument's master volume into the bar (2026-09-18). It is an
inline slider (`Bound::slider_inline`) beside the level meter, drawn inside
`mxm_ui::navigation::bar_card` under key **64**, outside the fifteen page keys, and the cursor
runs through `navigation::paged_with_bar`. The bar is drawn before the cards, so a fresh cursor
lands on Volume. `Section::Output` holds it for the Parameters list and the preset inventory; it
is never a Synth card, and `volume_is_drawn_once_in_the_app_bar` holds that it is painted once,
there.

Every routing target carries a `mxm_modulation_params::ui::stack` beneath the control it moves:
the live routes as rows, and `‹ modulate ›` under them. `Section::targets` owns the mapping;
completeness tests and `tests/routing_editor.rs` cover the shipped panel, every target's add and
remove gestures, all 433 offered pairs' parameters, navigation and reflow. A synced delay time reads its division, and its
free time when no tempo has arrived.

The editor asks for a frame every 50 ms while it is open: the meter and activity glyph
change between input events, and so do the developer channel's requests, which a frame
that waited for the pointer would strand. Closed, nothing runs.

**The opening size is the quarter-4K budget hugged** (`REFERENCE`, held by
`the_opening_size_is_the_budget_hugged`). **The minimum holds the widest card and the two gutters**
(`MINIMUM`, exercised by `every_dynamic_page_fits_and_every_card_is_reachable`), and the app bar at its last compact step is wider and sets it (`the_app_bar_holds_in_the_minimum_window`).

**Every card is a `mxm_ui::tree`, and every floor is computed** (`plans/plan-layout-tree.md`;
`crates/ui/AGENTS.md`, *A card body as data*). `sections::card` describes each of the thirteen
cards once; `paging::editor::show` measures that tree for the card's floor and height, and
`sections::paint` draws it leaf by leaf through the shared binding. `page_items` computes each
floor every frame and passes it as the card's ceiling too, so **every card is exactly as wide as its
content** (`plans/plan-editor-standard.md` A1). No card declares a usability minimum: usability is
the controls' own size rules (A2). There is no floor to re-measure after a content change.

- **A route stack's floor is every route revealed at its widest reading**
  (`mxm_modulation_params::ui::stack_size`), and that is what sets most cards. A row is named by its
  source under its target's title, with its reading beside the name and never over it, so the
  longest source at its widest reading, over a `TRACK_MIN` track, sets the stack's width. The
  Mixer's and the envelopes' sliders keep their readings off their names the same way (the owner's
  finding, `plans/plan-layout-tree.md` §10.3).
- The knob rows are the collection's (`mxm_ui::tree::knob_row`, at `mxm_ui::control::knob_column`)
  (`Node::Columns` with a column cap); the envelopes' and mixer's slider grids are two columns
  stretched across the card, `SPACE_2` over the body's rhythm between a column's two sliders; every
  other `add_space` of the hand layout is a pad.
- **The Effects card's Delay time and its sync are one group**, `SPACE_1` apart, with Reverb
  `SPACE_4` further off (the owner, 2026-09-28: the quarter note sat closer to Reverb than to the
  knob it syncs).
- **Each oscillator's Range is six buttons**, 64' to 2', on a row of its own above the waveform —
  the owner, 2026-09-27: every range is buttons, never a drop-down (design system §7.3's range
  exception). It was a caret selector beside the waveform; the editor opened 1694 × 1064 then and
  1694 × 987 now.
- The Advanced expander is `tree::disclosure`: the card reserves its body open, so opening it never
  grows the card or moves anything outside it — what is under it in the card moves down into the
  room at the card's foot. Its state is
  `mxm_ui::shell::disclosure_id("Advanced")`, which `sections::disclosure_id` returns and the
  developer channel's CC 118 writes.
- The delay's caption is built from tempo sync and whether a tempo has arrived: the tree draws
  whichever of its three texts is current and reserves the longest (`tree::reserve`), so a tempo
  arriving never moves the card (`the_effects_card_keeps_its_size_as_a_tempo_arrives`). `routing_shown_as_controls` leaves the stacks out of
  the tree entirely.
- `sections::draw_synth` and `draw_sample_hold`, which the layout lab calls, build the section's
  tree and `tree::show` it.

`editor::tests::every_card_passes_the_tree_checks_in_every_state` runs the shared checks
(`mxm_plugin_test::tree_checks`) over every card at Init; with every route revealed at full
negative depth; with Advanced open; with tempo sync on, with a tempo and without one; and with the
lab's routing-as-controls flag.

**The row's painted name is card-relative, and that is a hundred points a card.** A route row reads
`<target> from <source>` and on this machine *both halves are module-qualified*, so *"Amplifier
envelope gate from Oscillator 1 sync"* made the Envelope 2 card 118 points wider than *"Gate from
Oscillator 1 sync"* does. Across eleven cards that was enough to push the first
musician page down to a single card. `routing::TARGET_PANEL_NAMES` is the painted form and
`TARGET_NAMES` stays canonical for the parameter, the host's automation list and the accessibility
tree — design system §7.1's rule, which this instrument already applied to knobs through
`Bound::panel_label`.
`every_dynamic_page_fits_and_every_card_is_reachable` checks every page in both themes at opening,
quarter-4K content and minimum sizes. All thirteen cards share the paging row renderer; there is
no separate FX column layout. Tall component canvases retain floor/order/row checks, not physical
fit claims. Native DPI, resize feel and DAW verification remain separate gates.

**Four decisions of the build, recorded here because the brief said otherwise or said nothing:**
the six-position Range switches are §7.4 **selectors**, since §7.3 stops a
segmented control at five cells (the shared control asserts it); **Volume is in the app bar**,
not on Voice or any card (above); **TEMPO SYNC sits with the delay** on the Effects card, a quarter note
(`binding::sync_picture`, the owner's 2026-09-25 ruling: smaller than the words), as S&H sync sits
beside the S&H Rate and each LFO's beside its Rate (*Tempo sync is resolved once per block*, above),
rather
than in the Voice expander, a switch beside the control it changes; and the envelopes' and mixer's
four-way comparisons are **the collection's fader row** (`tree::fader_row`: vertical faders side by
side, the envelopes' painted *A*, *D*, *S*, *R*), as the brief first asked and the owner ruled for
every envelope and mixer on 2026-09-25.

**The three effects share one Effects card** (the owner, 2026-09-24): a card is one operation, and
the chain after the amplifier is one. Once cards hugged their content, the reverb's own card was
one knob and a caption wrapping a word to a line, and its height cost the editor a page. The card
holds three modules, so its controls paint their full names, and the Phaser's tooltip states the
order.

**No card carries a caption** (the owner, 2026-09-27; design system §7.6), and **every tooltip says
what its control does to the sound** — the same day the owner found Tone's *the plug-out's own
control*: no model history, no circuit (*diode ladder*, *rate CV*, *standing gain*), and no routing
the stacks already show. What the captions explained that a player needs stayed as a control's
sentence; the rest went. The developer Parameters list drew them too;
it no longer does. Only live readings — the delay's — remain as text on a card.

The first derived musician page opens by default. `editor/binding.rs` re-exports `mxm_preset::binding`, the collection's one binding (2026-09-24),
whose `panel` is this editor's module-relative label (`binding_for`); the identity accent is `mxm_ui::theme::COPPER`, measured in the
brief's §7. **Built, not signed off**: the §15 QA gate by eye at every zoom and in both themes, and
the brief's §12 trial, have not been run; the light theme at 100 % has been looked at through the
player and is what the fixes above came from.

## The keyboard cursor reaches every route, and the coverage check has to reveal them

The parent's *The keyboard cursor runs in every editor* owns the contract. Three things are local.

**A route's two halves are parameters**, so `mxm_modulation_params::ui::stack` opens a
`navigation::at` scope on each with its own permanent id — the shared crate's own rule, which it
learned when the scopes were the literals `"present"` and `"amount"` and fifty-five rows collapsed
into two registry entries.

**The coverage check reveals every route before it runs.** An absent route draws nothing at all, so
at the init patch thirteen of 433 pairs are painted; a check that only sees those covers three per cent
of the surface and would pass on an editor that had lost the rest.

**`REVEAL` opens the Voice card's Advanced expander**, where master tune and the bend range live. A
control behind a closed disclosure is never painted and so never registers; without opening it the
check would pass on an editor that had lost those two.

## `editor`, `params` and `telemetry` are public

They are `pub`, along with `editor`'s two sub-modules, so
[`apps/mxm-layout-lab`](https://github.com/mxm-audio/newdawn-workspace/blob/main/apps/mxm-layout-lab/AGENTS.md) can draw **these real cards** on its
bench instead of copying two thousand lines of section code that would then drift. **The same is
true of the other four instruments**, and their `Section` enum, `SECTIONS` and `title()` with it.

This is an in-repository bench API, not a permanent host identifier. `binding` and `sections`
remain public so the lab can draw real cards; its matrix comparison is lab-owned. CLAP identity and
permanent parameter IDs are unaffected.

## Selectors use the shared controls

The Range switches use the shared segmented control, and every routing input the shared
`mxm_modulation_params::ui::stack`. **There is no destination selector and no source selector on
any card** (Rev 3): the last two of each — DESTINATION's pair and the PWM sources — are routes now.
This crate owns parameter binding only: reading the step and sounding cell, and bracketing the
edit. Shared UI owns geometry, typography and interaction.

## Routing belongs beneath every associated control — never passive text or a detached footer

**The owner's requirement:** a simpler, more immediate and compact panel, consistently named
throughout. **Every routing target**, not only the filter's two, carries a
`mxm_modulation_params::ui::stack` directly beneath the control it moves — the live routes as rows,
and `‹ modulate ›` under them. Do not repeat a separate input-name column or collect routing at the
foot of a card. **There is no Patch page, optional overview, hidden matrix or alternate grid mode in
the shipped editor.** Sample-and-hold belongs under Modulators, after the two LFOs; its rate, lag
and stack stay together.

The plugin exports no matrix widget, and the layout lab's archived grid comparison retired with the
conversion. `Section::Routing` groups parameters on the generic Parameters list and is not another
patch bay — it is empty now, because a route is revealed by its own presence and listing 866 ids
there would claim a card holds controls that are deliberately absent. Factory JSON `text` fields are
non-authoritative comments; presets load by permanent ID and numeric value.

- All eighteen `Section::targets()` entries get a stack: sixteen on modulator, generator and tone
  cards including S&H — each oscillator its pitch and pulse width, Oscillator 2 its sync as well, the
  Filter its one cutoff, the Amplifier its VCA level, tremolo and Amplitude — and the phaser's two on the
  Effects card. **Voice carries none**: the glide is a source now, chosen on the
  oscillators, and Voice keeps priority, portamento and its Advanced expander.
- **A target with nothing routed draws no group at all** — an empty bordered box reads as a control
  that failed to load — so it costs one `‹ modulate ›` line and no border. `‹ modulate ›` sits
  *outside* the group, because it belongs to the target rather than to the sources already in it,
  and it carries the target's own name: a card with two stacks would otherwise offer two identical
  unlabelled menus.
- **A row ends in a remove, not a switch** — it is implied that a route is on when its row is
  visible. The cross is unframed and bottom-aligned onto the slider's track.
- Each row's amount is **signed and drawn bipolar**, because its centre is *no modulation*, and it
  **reads what that pair delivers** — see *A route's amount reads what that pair delivers*.
- Adding a source and removing one are each **one begin/set/end gesture** on the presence, and a
  removal leaves the amount alone so re-adding restores the depth.
- `routing_shown_as_controls()` remains a **lab-only suppression hook**. Never enable it in the
  shipped panel.

### Names describe generators and input roles, never the current patch

- Generator cards, source names and host text agree: **LFO 1 / LFO 2**, **Envelope 1 / Envelope 2**,
  **Oscillator 1 / Oscillator 2**. `column_label` reads `routing::SOURCE_NAMES`, which is also what
  every route row is named from, so a card's title and the row that names it as a source cannot
  drift.
- **Every input is named for what it moves**, never *Modulator* (the owner, 2026-09-22, with
  `mxm-mono-01`'s *Cutoff from Envelope* the model): a row reads *Pitch from LFO 1*, *Cutoff from
  Envelope 1*, *VCA level from Envelope 2*, *Rate from Oscillator 2*. **The Filter has one Cutoff**:
  the plug-out's VCF ADSR IN and LFO IN jacks and KYBD CV moved the one thing, and a player could
  not tell *Modulator 1* from *Modulator 2*; each source keeps the reach its own jack gave it.
  **Ring mod is a generator card of its own**, beside the oscillators, because nothing else on the
  panel could say that its carrier is always Oscillator 2: its Ring mod level's tooltip does, and its stack chooses
  what multiplies it. The Mixer carries **four levels** — the two oscillators, noise and **Ring mod
  level**, the 102's own RING MOD slider — and **External input**, the EXT IN jack as a routing input that takes any source at a
  level of its own, added into the mix as sound. It was named *Mix*, which read as *modulate the
  mix* (the owner, 2026-09-25: *"doesn't Mix modulate?"*); the canonical name changed with the
  painted one, ids unchanged. The plug-out shared one RING MOD / EXT IN slider between the two; here they are
  split back into the 102's two, because the ring modulator is a sound and belongs with the other
  levels. The target **Ring mod**, painted *Input* on its card, is the ring modulator's input —
  what it multiplies Oscillator 2 by, Oscillator 1 at Init at full.
- Gate, sync, S&H input and Phaser rate/centre are named for what they move too, and so are the
  panel's inputs — **Oscillator 1 / 2 pitch**, **pulse width**, **Tremolo** and **Cutoff**, painted
  *Pitch*, *Pulse width*, *Tremolo* and *Cutoff*. **Every route carries its
  own depth**, which is what replaced the row attenuators and the panel's depth knobs, so no amount
  is invented and none is missing.
- The LFO cores are sources of their own — **LFO 1 saw / reverse saw / triangle / sine** and
  **LFO 2 triangle** — distinct from **LFO 1** and **LFO 2**, which carry whatever the shape switch
  selects. They are the S&H's and the PWM section's own signals, taken ahead of the switch.
- **A painted name drops the prefix its card's title already states** (`plans/plan-editor-standard.md`
  B1): `sections::panel_label` is the table, and `routing::TARGET_PANEL_NAMES` is the same rule for
  a routing row. The prefix stays where the card is not the module's — *Noise colour* on the Mixer,
  every name on Effects. A stepped control's cells are its parameter's own option text (B4), and
  the Parameters list paints the canonical names. Neither is an alias or a
  source-dependent name; `TARGET_NAMES` and `ParamView::name` stay canonical for the parameter, the
  host's automation list and accessibility.

`tests/routing_editor.rs` drives **the real `editor::panel`**, not the lab or a copied footer:
**every target's last source added and removed**, crossing a row breakpoint at 1400 × 1080 before
editing at 1880 × 960 or 1920 × 1080 logical windows at 1× / 96 DPI, checking target ownership,
pointer height, hover stability, that every other pair is untouched, and exactly one host gesture
per add and per remove, scrolling the option into view as a player must once a menu holds
twenty-five sources. **One source per target, in one theme**: the gesture is the shared stack's and
the same for every source, and a theme is colour, never geometry — every pair in both themes was
twenty thousand frames of the whole editor and a quarter of an hour. `every_pair_is_the_parameter_its_row_names`
checks all 450 pairs as data instead — a refused one registered nowhere — with no drawing. Further tests cover an unrouted
target drawing a menu and no rows; a removal leaving the depth so re-adding restores it; every
stack staying inside its own card at 384/700/1880 point widths **with every route revealed** — its
first and last row, removal marks included, which is the defect the pilot's owner found by eye; and that **no card
draws a control named for a retired destination, depth or source switch** while every stack a card
owns offers its menu — falsified by adding a control that does exist to its list.
Geometry or parameter-count tests alone are not a substitute: they pass when a control becomes a
label. The navigation test clicks the painted tabs, requires S&H's controls on its derived page,
adds a source to its input through the menu, rejects Patch and grid cells on every page, and checks developer indices 0/1/2 and
clamping. These checks must survive future layout rewrites; a stack merely somewhere inside a card
is not enough.

Scope: headless interaction/geometry, not rendered contrast or real-host automation timing. DSP
fidelity, note ownership and audio callbacks are unchanged, not re-audited by these tests. Native
DPI/zoom sweeps and the manual §15/DAW gates remain separate.

## `preset.rs` is this instrument's `Instrument` impl and its factory set

The system is `crates/mxm-preset` (since 2026-09-04; this file was the fourth copy before that).
What is local: the
**fifty factory sounds** in `presets/`, each with its category, generated from `FACTORY_DESIGN` in `preset.rs`'s test
module (`write_the_factory_presets`, `#[ignore]`d; `the_factory_files_match_the_design_they_were_generated_from`
catches a stale file), and `every_factory_preset_can_be_heard`, which asks that the output be up,
a mixer level be up, and something open the amplifier. Init has no file.

**Regenerating after a parameter change:** `cargo test -p mxm-mono-00 --lib write_the_factory_presets -- --ignored`;
`every_factory_preset_covers_every_parameter` fails until it is run.

**And after any change to what a parameter prints.** `text` is never loaded, so a stale reading
loads the same sound and says something else — 134 route readings shipped from before `SOURCE_PEAK`
entered them, because the design test compares only `v` (audit D17).
`every_factory_text_is_this_plugins_formatting_of_its_value` compares every stored `text` with the
plugin's formatting of its `v`, in a test and never at load, and fails until the files are
regenerated; the regenerate's diff must then touch `text` lines only.

**A preset's resolution problems are shown, not dropped** — an unknown id, a missing one, a
non-finite value — in the same row the other problems use. The shared crate does it for every
instrument; never hide or drop them.

# Work Guidance

# Verification

```bash
cargo test -p mxm-mono-00
cargo test -p mxm-mono-00 --test routing_editor # every target through the panel; under a minute
cargo test -p mxm-mono-00 --lib every_card_passes_the_tree_checks_in_every_state
# Every page, light and dark, for review -> target/layout-tree/mxm-mono-00/<MXM_PICTURES tag>/
MXM_PICTURES=after cargo test -p mxm-mono-00 --lib tree_pictures -- --ignored
cargo test -p mxm-layout-lab --no-default-features --features mono-00 # the lab draws these cards
cargo clippy -p mxm-mono-00 --all-targets
cargo xtask bundle mxm-mono-00              # debug, for the allocation assertions
clap-validator validate "target/bundled/mxm-mono-00.clap"
cargo xtask bundle mxm-mono-00 --release
clap-validator validate "target/bundled/mxm-mono-00.clap" # validate each before overwriting
# then newdawn-workspace's editor_resize: every product's editor, natively resized
```

Run validator against both debug and release bundles. Its 1234.57 Hz sample-rate case guards the
VCA tilt and noise-shelf Nyquist clamps, and **`param-fuzz-bounds` is not a formality on this
instrument**: it is what found the phaser self-oscillating after the routing conversion, on
permutation 11 of 50, and the release bundle is where it showed — see the DSP crate's *A swept
corner also needs a bounded speed*. Last run clean on 2026-09-22, after Rev 3 put new summing
inputs on the pulse widths and the tremolo: debug once and release three times, 44 tests each, 35
passed, 0 failed, 9 skipped. Native resize is separate from visual and DPI/zoom review;
run it against a complete single-profile bundle inventory.

**In MXM Player:** `plugins/mxm-mono-00/host-tests/tests/behaviour.rs` loads the shipped bundle
through the player's own hosting path and proves a note at its pitch, a host-written cutoff, a
routing pair re-patched as a parameter — and a second source **summed** onto the same input, which
is the mixer decision 1.6 asked for — a self-running patch sounding with no key down, the filter
singing at full resonance from the init patch (the owner's first finding, fixed in the DSP), and
the reverb tail ending in exact silence. `t7_editor.rs` asserts the floating editor is advertised
and, `#[ignore]`d because it opens a real window, that it opens and reopens. **Opened through the
player by hand on 2026-09-03** (a `load` over the CLI, which opens the editor) and looked at in the
light theme at 100 % — **before the routing conversion**, so nothing about the stacks has been seen
by a person. **Not run by a person yet:** the converted editor at all, a real DAW, the other zooms
and the dark theme, the brief's §12 trial, and any listening comparison against hardware. Fidelity
is UNVERIFIED.

# Child DOX Index

No child AGENTS.md files.
