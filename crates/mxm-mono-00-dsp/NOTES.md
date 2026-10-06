# NOTES.md — crates/mxm-mono-00-dsp

The detail behind this folder's AGENTS.md: history, measurements, rationale and worked examples. AGENTS.md is the contract; this file is the reference it links to.

## Purpose: the design record and the four modules

The design record is `plans/plan-mxm-mono-00.md` (`plans/plan-mxm-mono-00.md` in the private archive); the research is
`research:instruments/system-100.md`. **Module 1, the voice
with the plug-out's defaults fixed in place** — one VCO, the noise, the mixer's saturator, the
passive HPF, the diode ladder, the VCA, two envelopes with their three trigger modes, LFO-1 with its
five shapes, portamento and glide; **module 2** — VCO-2 with strong and weak sync, the ring
modulator, LFO-2, and the plug-out's default cross-modulation of VCO-1 by VCO-2; **module 3** —
the sample-and-hold and **the matrix**, through which every patchable input reads its source; and
**module 4** — the three effects after the VCA, each behind its `research:effects/` page, consuming
the two phaser rows. The plugin that drives it is [`plugins/mxm-mono-00`](../../plugins/mxm-mono-00/AGENTS.md).

## Ownership: the files, and what retired

Owns `src/` (`lib.rs`, `oscillator.rs`, `noise.rs`, `envelope.rs`, `lfo.rs`, `sh.rs`,
`matrix.rs`, `routing.rs`, `hpf.rs`, `filter.rs`, `oversample.rs`, `ring.rs`, `vca.rs`,
`effects.rs`, `voice.rs`), `examples/`
(`system_demo.rs`), and `Cargo.toml`. The hand-written `common/wav.rs` retired to
`mxm-measure`'s encoder, and that encoder to `mxm-audio-file` (2026-09-15).

## Sync: the slave holds one sample so a reset can be corrected on both sides, with no latency

`oscillator.rs`, `Vco::process_slave`: a reset at an arbitrary point in a sample interval is a
discontinuity, and PolyBLEP corrects the sample on each side. The sample before the edge can only
be corrected once the edge is known, so the slave computes each sample one call early and holds it
— and **the emitted sample is for the same instant as the master's**, because the master reports
the edge it crossed while advancing *from* that instant. No latency; the first call emits zero;
the hold is the same whether sync is on or off. `an_unsynced_slave_is_the_direct_oscillator_but_for_its_first_sample`
pins the alignment, `a_hard_synced_saw_is_corrected_at_the_reset` the correction's worth.

**STRONG** resets the core on the master's rising edge — the hardware's edge, per the plan's §4
ruling on the manual's conflicting "fall". **WEAK** injects a fixed kick, and **a kick that carries
the core past its threshold discharges it to zero, losing the excess** — that loss is the only thing
that lets a kick lock rather than merely detune; a kick that wrapped to `p + kick − 1` would add the
same phase at every edge and no ratio could hold. That was the first version, and the lock test
found it. The result is one-sided lock ranges *below* each rational `p/q`, about `kick/q` wide.

## The ring modulator is the product, and its aliasing is measured

`ring.rs`: a balanced modulator with its trims set is a four-quadrant multiplier; no carrier leak
is invented. Two band-limited sawtooths at 440 and 660 Hz multiply to **−17.7 dB alias-to-signal at
48 kHz**, measured against a 4× oversampled reference decimated through a windowed sinc, with the
comparison's own floor at −28.6 dB (`ring_modulation_aliases_no_worse_than_this`). **Not
oversampled yet**: the figure is recorded so the fidelity gate can decide whether it is audible on
the bell patch, which is where a 2× ring modulator would go if it is.

## The patch bay: eighteen targets, twenty-five sources, and a unit delay on backward connections

`routing.rs` is the instrument's own declaration under
`plans/plan-modulation-routing.md` (`plans/plan-modulation-routing.md` in the private archive) and
`plan-mxm-mono-00-modulation.md` (`plans/plan-mxm-mono-00-modulation.md` in the private archive); `matrix.rs` keeps the
two tables that bind — **the evaluation order** (keyboard and the performance inputs → LFOs → S&H →
envelopes → glide and VCO-1 → VCO-2 → ring modulator and noise → mixer → the chain) and **the column
scale** (ten-volt units: an LFO sawtooth 0 … 1, a VCO ±0.5, an envelope 0 … 0.6, a gate 0 / 1, the
keyboard CV in tenths of a volt per octave from middle C). Changing the order changes the sound of
every patch with a backward connection, which is why it is a table there and not an implementation
detail.

**An input takes any number of sources, each at its own signed depth** — decision 1.6, the switching
jack becoming a mixer, and the single largest change this instrument has had. What was one
`EnumParam<Source>` and one attenuator per row is now one *(target, source)* pair per crossing, and
`mxm_modulation` evaluates them. The unit delay is that crate's: **a route reading a source its stage
has not yet published reads the previous sample**, which is what makes every cycle the bay can close
evaluable. `a_cycle_through_the_matrix_stays_finite` tests the mechanism and
`the_mixer_into_its_own_external_input_is_a_bounded_deterministic_loop` a cycle through it.

**The frame unit is one, because the columns were already scaled.** Every source is inside unit
magnitude in ten-volt units, so nothing clamps on publication and — unlike `mxm-mono-pr1` — no scale
is undone anywhere.

### The plug-out's own wiring is data, not code

**`INIT_PRESENT` and `INIT_AT_FULL` are the machine's thirteen normalled connections, and they are
the init patch** — the patch bay's, less GLIDE IN and less MIXER EXT IN ← the ring modulator, which
is `Patch::level_ring` again (the 102's RING MOD slider), and since Rev 3 the panel's six: the glide and
LFO 1 on both pitch inputs (DESTINATION's default), LFO 1 on the tremolo and the keyboard on the
cutoff, all at the zero their control held. There is no `default_source` table, no `Route::Default`, and nothing in `process`
that asks whether a route is the machine's own. Three mechanisms encoded that before and all three
are gone: the default table, the two `match … { None => <internal signal> }` fallbacks, and the
`sync` switch.

Two consequences, and both are the point:

- **The keyboard gate and the glide's RC dip are sources.** They were the signals the bay normalled
  to *without publishing*, reachable by nothing. They can now reach anything, and the routes that
  carry them to the two envelope gates and to the two pitch inputs are ordinary pairs a player can
  remove.
- **Sync's presence is the switch.** SYNC IN is normalled to VCO-1 SYNC OUT and the panel switch
  decided whether the circuit acted; under pairs those are one thing. A fresh instance, whose switch
  was off, wires nothing. `syncstrength` still chooses strong or weak (wart 17, untouched).

### The panel's wiring is routing too (Rev 3)

`plans/plan-mxm-mono-00-modulation.md` Rev 3: the plug-out's DESTINATION switch, its fixed VCA LFO
and KYBD CV, its two PWM switches and SAMPLE MODE each chose a source, or a destination, somewhere
other than the input it moves. **Each is a target now**, and every path they made is a route:

- **Two pitch inputs**, `VCO1_PITCH` and `VCO2_PITCH`, at `PITCH_SEMITONES_PER_UNIT` (144) — EXT CV's
  120, which a glide input and a vibrato used to add 2 and 12 to beside it, so the sum of all three
  still fits one route. **Each is read in its own oscillator's stage**, as `matrix.rs`'s order
  requires: a route from VCO-1 into VCO-2 is same-sample, like a cable. GLIDE IN retired: a direct
  route is its exact equivalent, and the one of its paths whose timing changed — VCO-1 through
  GLIDE IN into VCO-2, one sample late before and same-sample now — is named in the plan.
- **Two pulse widths**, `VCO1_WIDTH` and `VCO2_WIDTH`, under `narrowed_width`: nothing routed is the
  manual width; anything routed, at zero depth too, narrows from square (wart 4). Their scale is
  per source, so an amount is the retired PWM depth exactly.
- **`TREMOLO`**, into `Vca::gain`'s LFO argument, which takes only its positive part (wart 3).
- **One `CUTOFF`**, where there were three: the VCF ADSR IN and LFO IN jacks and KYBD CV all
  moved the one cutoff, and a player read *Modulator 1* and *Modulator 2* without knowing which jack
  was which. **Each source keeps the reach its own jack gave it** — an envelope 7 octaves, an LFO 4,
  the keyboard one octave per octave, anything else the ±12 the cutoff clamps to — and the merge
  measured **bit-identical** on every filter path, three sources summed included.
- **The S&H input takes whatever is routed**, and nothing contributing — no route, or only routes at
  zero depth — is the retired OFF: `sh.rs` holds its last step. Processing and activity read the one
  predicate, `Voice::sampler_is_on`.
- **Five LFO core sources**, ahead of the shape switches: LFO 1's saw, reverse saw (`1 − saw`, not a
  negative saw), triangle and sine, and LFO 2's triangle — what SAMPLE MODE and the PWM switches chose
  between. Each is published only while something reads it.

`FULL_SCALE` is `[target][source]` because of the pulse widths, the cutoff and the Amplitude; every
other target is uniform, and `Graph::sum` evaluates those in the order the pinned digests were taken
in — through `mxm_modulation::sum_split` since the modulation standard, the row's own instruction
sequence to the bit while no pair with an `ADDED_SCALE` is live.

### The collection's modulation standard

`plans/plan-modulation-standard.md`; `mxm_modulation::standard`. What a performance source and an
added route mean is the collection's; what the machine had stays the machine's.

- **Key keeps the keyboard CV's own unit**, `(note − 60) / 120` — 1 V/oct from middle C in
  ten-volt units (`KEY_UNIT_SEMITONES`), published through `standard::key`. The wheel, pressure and
  the lever go through their standard publishers. **Velocity is `v − 1` of the press that last
  triggered an envelope** (`Voice::envelope_velocity`): `Voice::gate` reports a trigger **and
  whether a press made it**, and one a press made latches **that press's** velocity. A GATE+TRIG
  retrigger is the latest press's (`pressed_velocity`) — a tie under low-note priority that does not
  take the bus included — and needs no crossing: a press retriggers whenever the gate is high,
  whatever holds it, so it is the press's even in the sample another source raises the gate. A GATE
  or LFO edge is the first press's after silence (`edge_velocity`), even with later presses in the
  same sample, **but only where the keyboard gate's route alone crosses the threshold**; a rise a
  shallower route only rode on is the other source's. A trigger something else made (an LFO gate, a
  clock), even in the same sample as a press, latches the sounding press's while a key is held and
  nothing with none. The source is published **after stage 4**, so a route read earlier in the
  sample takes it one sample late — a held value. Under GATE a legato press raises no gate and keeps
  the phrase's. It rests at full before any press and after All Sound Off.
- **`machine` names what the plug-out had**: any column into a patch-bay row — the twelve input
  rows, the filter's two jacks merged into the cutoff, the 102's VCO EXT CV (VCO-2's pitch) — and the
  panel's wiring (the keyboard gate into both envelope gates, the glide into both pitches, LFO 1's
  cores into the S&H, the PWM positions into both widths, LFO 1 into the tremolo). **A performance
  source the machine did not have takes the standard reach** (`takes_standard_reach`): 12 st of
  pitch and 4 octaves of LFO rate through `ADDED_SCALE`, 4 octaves of cutoff in its per-route row;
  the phaser's four octaves a unit and the widths' 45 % already are the standard's. **Key into
  either pitch is 1 V/oct** (120 st a unit, 12 st/oct) — the EXT CV's own tracking and the
  standard's, where the row's 144 gave 14.4. Every generator pair keeps its row's reach.
- **Offers** (`routing::offer`, from `STANDARD_LAW`): a gesture into either envelope gate, the sync
  input or the mixer's audio input, and Velocity into the S&H, are **refused** — seventeen pairs;
  the VCA level keeps Velocity's closing half; the widths and the tremolo a one-signed source's live
  half. **The sync input offers every generator only its positive half**, because a reset has no
  inverse; Key's sync pair is the patch bay's and keeps both, by the standard's machine rule.
- **The machine's CV amplifier is shown as *VCA level*** (`target::AMPLIFIER`), and **the standard
  Amplitude is `target::AMPLITUDE`**, `amplitude_factor` on the VCA's output — after `Vca::process`,
  whose `0…1` gain clamp would otherwise swallow the doubling. It cannot open a closed VCA, and
  **`OUTPUT_BOUND` includes its doubling** (the extremes test routes it at full). Its column is
  normalised by each source's peak, so every source at full reaches exactly the standard swing.
  **A static exact mute is not `Live`** (`amplitude_mutes_until_an_event`): when every route into
  it reads a value only a host event moves — the wheel, pressure, the lever, the keyboard gate, and
  **Velocity unless it can move by itself** (`velocity_can_move_by_itself`: a key held, the sounding
  press's velocity not the one latched, and a gate that fires with no event, whose trigger would
  re-latch it), **the keyboard CV unless the CV it publishes is still gliding** (`key_moves_by_itself`)
  — and the factor is exactly zero. Every generator (LFOs, envelopes, S&H, glide, oscillators, noise,
  mixer) counts as moving: a mute through one stays `Live`, never wrong, only not free. When it
  holds, the verdict falls through to the tail, as Volume at zero
  parks the voice, except that the effects still ring out. **Behind it a held key or a sustaining
  envelope is no tail** (`tail_open` and the settle clock count only the effects' memory), so a muted
  held note parks too. A route from anything self-running can reopen it and never counts.
- `conformance.rs` runs the standard's checks — the declaration (the widths as the standard's
  narrowing kind), the publishers, the velocity rule, the Amplitude factor and release, with the
  keyboard CV into the VCA level and into the VCA envelope's gate as the machine's drones — each
  falsified once. The pinned conversion digests hold.

### Three combination laws, and two of them are this machine's

- **Sum** for every continuous input.
- **Gate**: sum, then detect once against `GATE_THRESHOLD`. An amount scales its source *before*
  detection, so it decides whether that source ever crosses and when within its rise —
  `two_half_amount_sources_cross_a_gate_threshold_neither_reaches_alone`. One detector, so
  coincidence cannot arise.
- **Sync**: detect per route, reset by the **largest** depth — the *signed* largest, because a
  reset has no inverse. `process_slave` clamps the pull to `0…1`, so a negative amount resets
  nothing; choosing by magnitude would let a route at `-1.0` beat one at `+1.0` and produce no sync
  where the player asked for a full one
  (`an_inverted_sync_route_resets_nothing_and_does_not_mask_a_positive_one`). It cannot sum, because `EdgeRow`'s
  interpolated fraction is scale-invariant — `an_edge_rows_fraction_does_not_move_when_its_signal_is_scaled`
  — so a scaled sync route would be an on/off switch wearing a knob. **The amount is reset depth
  instead**: how far the slave's phase is pulled toward zero, which is soft sync and is a **DSP
  addition**. At full depth it is the hardware's reset to the bit, `p − p × 1.0` being zero for every
  phase; `a_full_depth_sync_route_is_a_hard_reset_and_a_partial_one_is_not` holds both halves.

### Two transition classes, and the audio one generalised

Every CV and gate route **steps** on a topology change, as the bay's switching did. The two rows that
carry **audio** ramp over `AUDIO_ROUTE_RAMP_S` — and under summing that is no longer a crossfade
between the two sources a row could name, because an input takes any number. **Each route ramps its
own contribution in and out**, which is the same audio when one replaces another and is the only form
that also covers adding a second or removing the last
(`an_audio_route_change_mid_note_does_not_click`).

**A voice's opening topology snaps rather than fading.** The plug-out's normalled audio connections
are wired from power-on; fading them in over the first five milliseconds made a fresh instance's ring
modulator arrive late, which showed as a 1.8 % peak difference against the pre-conversion render. A
reset snaps for the same reason — it is not a player pulling a cord out.

### Three things a topology transition owes, and each was a defect first

- **A newly read source starts from silence.** Publication is gated on *is anything reading this*,
  so an unread slot keeps whatever it last held; a backward route added to a running voice would
  read it for one sample, and how stale it is depends on how long the source went unread — which
  makes the first audible value depend on the host's buffer sizes. `SourceFrame::clear` is the
  shared crate's answer and `Graph::set_topology` calls it.
- **A newly live sync detector starts from zero**, for exactly the same reason: an `EdgeRow` that
  is not fed keeps its `prev`, so a route removed while its source was high and re-added while it
  is low reports a crossing that never happened.
  `a_newly_live_sync_detector_starts_from_zero_rather_than_an_ancient_value`.
- **A newly present route's amount smoother snaps to its stored value**, which is the plugin's
  (`routes::TargetRoutes::arm`): while a pair is absent nothing advances its smoother, but the
  parameter stays editable, so resuming would ramp the route in from a stale number.

### Every path to `process` owes `set_topology`

Compaction runs **once per processing interval**, never per sample, because topology is discrete and
changes only on a parameter event. A voice that is never armed has no live routes and renders
silence, which is a bug that looks like a quiet patch — `mxm-mono-pr1`'s conversion shipped exactly
that once and the sampler's did too. `Graph::begin_sample` **panics in debug** rather than leaving it
to be found by ear.

### What the conversion costs per sample, measured

**265.8 → ≈277 ns/sample, +4.3 %**, on the init patch's nine live routes. **Rev 3 added +3.6 %**
(464 → 480 ns/sample on the development machine on 2026-09-22, the same benchmark against `main` run
beside it; the absolute figures are not comparable with 2026-09-14's, the machine's state having
changed, so only the same-day ratio is claimed): fourteen live routes at Init, five of them zero-depth panel routes summed every sample.
Two savings pay for part of that — an unrouted target's sum returns before the loop, and the LFO
core sources are computed only when read.
`examples/mono_00_cost.rs` renders through the real `process` and reports the best of four runs of
960 000 samples; the figures either side are four runs each, spread under 1 %.

**The measurement settled a design question rather than merely reporting one.** On the build that
carried the grid inside `Patch`, the same benchmark read **280.1** with the per-sample copy and
**271.1** without it: **9 ns was pure memcpy**, because `Patch` is `Copy` and rebuilt every sample
while the grid is 1.5 KB of presences and amounts that change on a *parameter event*. So the grid
travels beside the patch as a `&Routing`, and `Patch` is now 204 bytes — smaller than the 232 it
was before the conversion, seven fields having retired. `mxm-mono-pr1` found the same thing and
moved its grid out of `control::Params` for the same reason.

What remains is the routing's own work: publication gated on *is anything reading this*,
compaction once per interval, and a per-sample loop over live routes only.

### What the conversion did to the sound, measured

`tests/conversion.rs` pins ten patches, one per normalled connection. Against the pre-conversion
renders, over 19 200 samples each: **worst absolute difference 2.4e-7**, one to four ulps; **worst
relative difference −121 dB** against the case's own RMS; every peak amplitude equal to six
significant figures.

It is a **re-association**, not a change: `amount × (column ÷ unit) × reach` became
`Σ(amount × source) × scale`.

**Rev 3, measured the same way**, against 42 renders of the pre-Rev-3 voice over every panel path it
converted: the tremolo, the manual pulse width, all six key-follow cases, the S&H's triangle and
sine modes and its external input, and the square-wave vibrato **bit-identical**; the rest within
**−99.7 dB** relative, every one a pitch path, where `amount × 144` against `depth × 120` rounds
differently — **measured as accumulation**: every such case agrees to one ulp or better over its
first 500 samples and drifts to about 1e-5 by the end; the pulse widths within −132 dB; and the one
named timing change at −57 dB, which falls to −141 dB when VCO-2's pitch is read at the old stage,
so the timing is all of it. Nine of the ten digests below did not move; the cross-modulation case did, for
the same re-association, with its peak equal to six figures. Seven panel cases are pinned beside
them. Bit-identity was available only by keeping a divisor table beside the
scale table for the two targets whose legacy expression divided first — a contortion for one ulp,
not taken, and this measurement is the reason.

### The S&H and wart 16

The S&H (`sh.rs`) samples whatever is routed to its input on its own clock, slews through the
portamento's RC, and puts its clock on a column — so wart 16, the clock firing the second envelope,
is S&H CLK OUT into the amplifier envelope's gate, one gesture away and self-running
(`a_slow_clock_on_the_gate_row_keeps_the_patch_live_across_silence`).

## The effects are chosen models behind documented controls, and they snap to exact zero

`effects.rs`: **phaser → delay → reverb**, in that order (chosen; the manual states none), after the
VCA and before Volume, each a wet/dry mix under its one level and bit-transparent at zero. What
each page documents and what was chosen is in the module doc and the table below; the spring is
**measured from the RE-201 capture** in `research:effects/system-100-103-spring-reverb.md` §2c — the
two resolved round trips, the T60 and the band-pass — with the third spring and the dispersion
depth chosen.

**The quiet snap.** A 3.5 s reverb tail would take twenty seconds to reach the flush threshold on
its own, and the collection's exact-silence contract would never be met with the reverb on. So each
effect tracks what it **writes into its memory**, and once nothing above `SNAP_LEVEL` (−100 dB) has
been written for the longest delay it holds plus `SNAP_HOLD_S`, it clears its state to exact zero.
The memory length is part of the condition, and it has to be: the first version watched the output,
and the delay cleared its line during the quarter-second of silence *before* a repeat could arrive
(`the_delay_repeats_at_its_time_and_decays_at_the_fixed_feedback` found it). **What an effect
holds is tracked, not estimated**: each reports `holds_anything()` until its own quiet snap or a zero
level empties it, the voice restarts its fixed `POST_TAIL_S` settle while one does, and so *tailing*
covers the repeats. `Voice::settle_samples_for`'s estimate of the longest effect tail is only the
`Tail(n)` promised to the host (`tail_samples`), never what decides the verdict. **The settle restarts on any input the effects can hear** — a key, an envelope the
amplifier reads, the amplifier's output at or above the effects' own snap level, −100 dB, or an
effect that still holds that much — until its own quiet snap or a zero level clears it, since the
delay's feedback builds a quieter input past it
(`a_quiet_input_the_feedback_builds_up_charges_the_delay`) — so a keyless patch closed by INITIAL GAIN
still tails its repeats (`closing_a_keyless_patch_leaves_the_effect_tail`; code review, 2026-09-22).

## The spring has three tanks, and the instrument can only have one of them

**An addition for the `mxm-folded-spring` effect**, under the parent crate's rule that an instrument's
DSP may gain inputs but never a behaviour change (`plugins/AGENTS.md`, *Every effect built into an
instrument is also promoted*). `SpringTankModel::Medium` is the measured tank above, is the default,
and is the only one `Voice` ever selects — it never calls `set_model`. `Short` and `Long` are
**chosen scalings** of the measured one: transits and decay scaled, the band's top tilted the way a
shorter or longer spring's is, and `Short` on two springs. Naming them after real tanks would claim
captures that do not exist.

Three things keep the instrument's render exactly what it was, and the first is the one that would
break quietly:

- **Every line is sized for `LONGEST_TRANSIT_MS`**, the longest transit any tank asks for, not the
  tank in force. So `set_model` allocates nothing and is legal on the audio thread — and the read
  offsets do not depend on the line length, which is what makes the sizing change inaudible.
- **`Medium`'s numbers are the module's own constants**, spelled from them rather than copied, so
  the two cannot drift.
- **`tests/spring_tanks.rs` pins the render by digest**, taken on the revision before the tanks
  existed with a file that compiled against both. A failure there means the instrument's reverb
  changed; it is not a licence to update the number.

`SpringReverb::tail_samples` still answers the measured tank's figure, because that is what `Voice`
asks and `Voice` has no other tank. `tail_samples_for` is the standalone's.

## The resonance control sings from the same position at every cutoff

`filter.rs`: the analogue closed form gives the loop gain at which the prototype oscillates, and
the top of the resonance control is `RESONANCE_MARGIN` past it. But each TPT one-pole is exact only
at its own corner and warps above it — its phase reaches −90° at Nyquist whatever its corner — so
the discrete cascade needs **more** gain to oscillate as the cutoff climbs: ×1.15 at a tenth of
the sample rate, ×1.8 at the clamp. Applied unchanged, the analogue threshold left the control's
top exactly at the discrete threshold at 5 kHz (48 kHz) and below it above, and **the owner found
the init patch's 10 kHz cutoff would not sing at full resonance.**

`DiodeConfig::discrete_threshold(ratio)` is the discrete closed form — the cascade's −180°
crossing by bisection, monotonic because each stage's phase is — and `DiodeLadder::new` tabulates
its ratio to the analogue threshold over the log of `cutoff / fs`, once, on no audio thread.
`native_resonance(resonance, cutoff, fs)` multiplies it in, so the slider sings from `1 /
RESONANCE_MARGIN` = 0.8 wherever the cutoff is — **the hardware's "near 8"**, `RESONANCE_MARGIN`
being 1.25 here and not the 303 copy's 1.145, which put the point at 0.87 with the excitation
switched on only above 0.9, a dead band the owner found
(`the_resonance_sings_at_the_same_slider_position_at_every_cutoff`, six cutoffs at three rates;
`the_filter_self_oscillates_at_the_top_of_the_resonance_with_nothing_feeding_it`, the voice at
every octave to 20 kHz). The closed form is checked against the running filter to 3 % where the
analogue form was 15 % and 80 % wrong. `K_MAX` is 44 to cover the 303 set at its clamp (41.8),
and `no_configuration_asks_for_more_than_k_max` keeps it neither exceeded nor slack.

**What the correction does not fix:** the *pitch* of the oscillation warps upward toward the
clamp too — +128 cents above the nominal cutoff low down, more near the limit — and
`oscillation_hz(cutoff, fs)` reports the warped figure rather than pretending otherwise.

## The ladder runs at twice the rate, and past its exact ceiling the poles pin one by one

The hardware's VCF runs 20 Hz – 20 kHz. The discrete ladder keeps every pole below
`NYQUIST_FRACTION` of the rate it runs at, so at 48 kHz the widest pole (2.6 × the cutoff) capped
the nominal cutoff at 8.3 kHz and the oscillation near 11 kHz — **the owner's "it goes no higher
than 10 kHz"**. Two changes, both in the ladder's path only:

- **`oversample.rs`**: the ladder runs between a half-band interpolator and decimator at twice the
  base rate, so the exact-ratio ceiling is 16.6 kHz at 48 kHz. The oscillators are PolyBLEP and
  the effects sit after the decimator; nothing else is oversampled. The kernel is a Kaiser-windowed
  sinc computed at construction; `a_round_trip_is_a_delay` and
  `the_decimator_removes_what_is_above_the_base_nyquist` measure it. **It adds
  `LATENCY_SAMPLES` (22) of delay at the base rate**, which `Voice::latency_samples` exposes; the
  plugin deliberately does not report it (its AGENTS.md says why). The VCA gates the delayed
  audio, as the hardware's amplifier gates its filter's output.
- **`filter.rs`**: above `max_nominal_cutoff_hz` the nominal cutoff no longer stops. Each pole
  clamps at the limit on its own, the widest first (`above_the_exact_ratio_ceiling_the_poles_pin_one_by_one`),
  so the control reaches 20 kHz at any rate. Between the ceiling and the top the shape is no longer
  the ladder's, and the discrete-threshold correction covers that region too — it peaks there, at
  about 2.7× for this set and 2.9× for the 303's, which is what `K_MAX` (70) covers. Near the
  top the sung pitch passes the base Nyquist and the decimator takes it, as the hardware's 21 kHz
  oscillation is past hearing; the voice's oscillation test stops at 14 kHz for that reason.
**`mxm-mono-03-dsp`'s ladder, the copy this one was taken from, carries the same defect
uncorrected**; the two copies now differ here, which is extraction evidence worth recording.

## The effects' lines are sized at activation, and All Sound Off clears them

**Emptying a line is O(1)**: each line keeps the count of samples written since it was last
emptied and a read further back returns zero, so the quiet snap, the zero-level emptying and
`reset` touch no memory — zeroing a line sized for 768 kHz inside one `process` call would be a
million stores (`an_emptied_line_reads_exact_zeros_without_being_zeroed`).

`Delay` and `SpringReverb` size their lines for the rate they are given in `set_sample_rate`,
which `Voice::set_sample_rate` calls from the plugin's `activate` — the main thread, where
allocation is allowed — and never from `process`. The first build sized every line for 192 kHz,
and at 768 kHz a one-second delay repeated after a quarter of one while every spring transit
collapsed to 15.6 ms (`the_delay_and_the_springs_keep_their_times_at_the_highest_rates`).

`Voice::all_sound_off` (CC 120) is silence *now*: the stack and envelopes, then the HPF, the
ladder, the oversampler, the VCA and all three effects are cleared, **and the settle clock is
closed**, so the activity verdict does not claim a tail that was just cleared; the LFOs, the
sample-and-hold and the oscillators keep running so a live patch's clock keeps its phase
(`all_sound_off_silences_the_effects_tails_at_once`, `all_sound_off_leaves_no_tail_to_report`).
The first build silenced the envelopes and left the reverb ringing for seconds.

**An effect at zero level is emptied, once, not frozen.** All three return the dry signal at zero
and clear their state the first sample they do. The delay used to keep running and wait for its
quiet snap, which a host that parked the patch froze with a repeat in flight
(`a_delay_turned_to_zero_is_emptied_not_frozen`; code review, 2026-09-22). The first build returned dry over a full tank, so raising the level
ten seconds later replayed ten-second-old audio the activity clock had already written off
(`a_zero_reverb_empties_the_tank_rather_than_freezing_it`).

## Every corner is clamped below Nyquist, because a validator will try 1 kHz

`hpf::NYQUIST_FRACTION` (0.45) bounds every prewarped corner in the crate — the HPF, the ladder's
poles, the effects' all-passes and one-poles, the VCA's tone pivot and the noise's shelves. The
last two were unclamped and clap-validator's `process-varying-sample-rates` found them at
1234.57 Hz: a 1 kHz pivot prewarped past the approximation's range and the tilt returned NaN, and
**a module nobody had turned up poisoned the mix**, because `NaN * 0` is `NaN`.
`the_init_patch_is_finite_at_every_sample_rate_a_validator_tries` sweeps 1 kHz to 768 kHz with a
fractional rate and fails without the clamps.

**A clamped corner also has a floor, so a low enough rate crosses its bounds**, and `f32::clamp`
panics on a crossed or NaN bound: the HPF and the oversampled ladder cross below 22.2 Hz, the
effects' corners below 44.4 Hz. `MIN_SAMPLE_RATE` (1 kHz) is the lowest rate the plugin activates
at.

### A swept corner also needs a bounded *speed*

Clamping a corner below Nyquist keeps it evaluable; it does not keep a loop around it stable. The
phaser's four all-passes have unity magnitude **only while the coefficient holds still**. Drive the
sweep's centre coherently near Nyquist and the chain stops being passive, the loop's fixed
`PHASER_FEEDBACK` exceeds one, and the phaser **runs away to infinity from silence**.

Measured, and each of these is the load-bearing part:

- at a feedback of **zero, no excursion reaches it** — the instability is the loop, not the cascade;
- at full excursion it starts at a feedback of **0.1**, so no non-zero fixed feedback is safe;
- a **random** centre at the same excursion is stable — it takes the coherent tone to pump;
- it needs **no input**: the output is a self-oscillation, not an amplified signal.

**One input could never do this. A summing input can** — which is why converting to any-to-any
routing is what made it reachable: several audio sources at full depth saturate the ±4-octave clamp
and toggle between its ends. `clap-validator`'s `param-fuzz-bounds` found it, on permutation 11 of
50, as `-inf` at output sample 460; the pre-conversion build passed, and a probe of **every** column
into PHASER MANUAL IN there stays finite and quiet.

So the cure is at the driver, not the symptom: `PHASER_MANUAL_HZ` gives MANUAL IN the bandwidth a
control port has. A guard that merely bounded the feedback state would have been finite and wrong —
it leaves a full-scale self-oscillation at roughly six times whatever bound it is given, from
silence, which is not what the machine does. `PHASER_MANUAL_FRACTION` is why the corner is not in Hz
alone: `NYQUIST_FRACTION` would keep it *looking* clamped and let it through, because at `fs` 8 kHz a
corner of `0.45 fs` passes an alternating CV nearly whole and the runaway comes back.

**Nothing patched, nothing changed.** The one-pole's state is exactly zero while its input is, so an
unrouted MANUAL IN — the init patch, every preset, every pinned conversion digest — is bit-identical
(`the_centre_cvs_bandwidth_is_bit_transparent_when_nothing_is_patched`).
`a_centre_cv_alternating_at_nyquist_does_not_set_the_phaser_oscillating` holds the rest, at every
sample rate from 1 kHz to 768 kHz and every alternation period from `fs/2` to `fs/2048`, and asserts
the *level* as well as finiteness. The RATE CV cannot drive it: its phase integrates.

**A non-finite parameter never reaches this crate.** Its bounds are `f32::clamp`, which returns
NaN for NaN — so they bound finite input and are not a NaN guard, and a host's NaN would have
poisoned the mix the same way the unclamped corners did. The guard is not here but at the one boundary every host value passes, the vendored wrapper's parameter setter
(the nice-plug fork's `PATCHES.md`, defect 4), so every instrument in the collection has it at once.
`apps/mxm-player/tests/plugin_robustness.rs` holds it against a real host.

## The pitch path is the hardware's order, and glide sits after portamento

`voice.rs`: the keyboard's target charges the hold capacitor through the portamento pot — only
while a key is down, so a release mid-lag holds the mid-lag pitch and **the next key charges from
there, a first note after silence included** (`mxm-mono-01` snaps its first note; this machine
does not). The capacitor is carried as its **remaining distance** from the target it last charged
toward, so a long portamento lands on its note, where the voltage form stalled — 9 cents short at
half a second and 48 kHz (`a_portamento_lands_exactly_on_its_note`;
`crates/mxm-mono-01-dsp/AGENTS.md`, *Numeric contracts*). That capacitor's voltage is the keyboard CV, the KYBD CV OUT column the matrix will
carry, and it carries no glide. The glide dip and the vibrato are added afterwards, per VCO,
through that oscillator's own pitch input — DESTINATION's choice of one oscillator or both is which
pitch inputs carry the route. A long portamento does not lengthen the dip's recovery, and the tests
say so.

## The bus follows the priority in force

`priority` is a parameter, so it can change while keys are held. The note handlers set the
keyboard target under the priority of the moment; `process` then reconciles it every sample
against `stack.sounding(p.priority)` and moves the target when the selected key changes — with
**no gate edge**, as switching the hardware's bus would not retrigger anything
(`a_priority_change_while_keys_are_held_moves_the_bus_without_a_gate_edge`).

## A gate input watches its summed signal, whatever is on it

Each gate input has one `GateRow` detector and it is fed every sample with the target's **summed**
level, so a **repatch that changes the level is an edge**: from a held key to a low source the
envelope releases, from a low source back to a held key it retriggers. The first build detected edges
on the keyboard and on the column separately, and a repatch fell between the two
(`repatching_a_gate_row_to_a_different_level_is_a_gate_edge`).

**A key press is a *press* for GATE+TRIG whether or not it is an edge**, so a tie retriggers that
mode alone — and that is a property of *the keyboard reaching this envelope*, so it is read from the
patch rather than from a hard-wired *is this row unpatched*. The condition is the gate route being
present **and carrying something**: a route at zero depth contributes nothing, so reading presence
alone made a silent route retrigger on every note-on while its gate never crossed the threshold
(`a_gate_route_at_zero_depth_does_not_retrigger_a_gate_trig_envelope`).

**A press needs a gate that is high, and a key still down.** An envelope only a fall can release
must not be fired where no gate rose, or it sits in sustain with nothing held. Two ways reached that
(audit D8), and each has its own guard:

- **A note that ends at its own offset is no press.** A note-on followed before the next sample by
  its note-off, a final choke or All Notes Off leaves every key up, so `note_off`, `choke` and
  `all_notes_off` cancel the pending press when the stack empties — `mxm-para-07`'s pending-line
  cancellation. Without it a final choke was undone one sample later. In every trigger mode such a
  note renders exactly what no note renders
  (`a_note_that_ends_at_its_own_offset_fires_no_envelope_in_any_trigger_mode`), and a second source
  holding the gate high cannot undo the choke
  (`a_final_choke_is_not_undone_by_its_own_note_on_while_another_source_holds_the_gate`). Over a
  held key it is still a press, which the held key's release closes
  (`a_note_that_ends_at_its_own_offset_over_a_held_key_still_retriggers_gate_trig`).
- **A keyboard route too shallow or inverted to carry the gate over the threshold is no press
  while the summed gate stays below it**: GATE+TRIG then does exactly what GATE does
  (`a_keyboard_gate_route_that_never_crosses_the_threshold_is_no_press`). While another source
  holds the gate high, a press through that route retriggers GATE+TRIG as any press does — there
  is a gate for it to ride, and a fall to close it
  (`a_gate_trig_press_through_a_shallow_route_is_the_presss_whatever_raised_the_gate`).

## The LFO trigger position is a diode AND with the square

`Trigger::Lfo`: the envelope's gate is the row's effective level ANDed with LFO-1's square
(`Lfo::square_high`), as the hardware's two diodes into a pull-up make it (`system-100.md` §7.1).
A key pressed in the square's low half waits for the rise; every fall of the square is a release;
"re-fires on every cycle while a key is held" falls out of that. The first build fired on the key's
own edge regardless of the square (`the_lfo_trigger_is_an_and_with_the_square`).

## All Notes Off is the stack alone

CC 123 clears the held keys and nothing else: each gate row's detector sees its effective signal
fall, or — patched to a column that is still high — not, exactly as a key release would. The first
build released both envelopes directly and so reached past a patched gate row
(`all_notes_off_does_not_reach_past_a_patched_gate_row`). **A per-note choke stops
the note** — envelopes silenced, the amplifier closed — and leaves the effects' tails running; that
is CLAP's choke, and All Sound Off is what clears the tails (deliberate distinction).

## A tie is a note-on without a gate edge

Under low-note priority a second key over a held one raises no new gate (§6.1). So across a tie:
portamento lags the pitch; the glide does not fire; an envelope at **GATE** does not retrigger;
one at **GATE+TRIG** does; and at **LFO** the envelope refires on LFO-1's square while the key is
held and never without one. Each envelope's trigger is its own field of the patch.

## *Live* is decided from the configuration and the control state, never from the audio

**The one thing that hears the signal is the tail's settle clock**, which restarts while the
amplifier's output — the effects' input — is at or above their snap level, or while an effect still
holds anything, so their tail is measured from the last thing they kept (below). Whether a patch is *live* never asks it.

`Voice::activity` is **live** only when the amplifier can open without a key:

- INITIAL GAIN is up; or
- **any live route into the amplifier's own input** carries a self-running source under the current
  patch — *any*, because the input is a mixer now, and a route at **zero depth carries nothing** and
  so cannot make a patch live; or
- that input is held open by a positive static source or by an envelope whose own gate input is held
  high.

**The five LFO core sources are self-running**, as the LFO columns are.
**The glide is self-running while its charge decays** — a key dumps it and it falls with no further
event, so an offset plus a falling glide can cross a gate after the settle
(`a_decaying_glide_on_a_gate_keeps_the_patch_live`) — and static once flushed to zero.
**None of the five other non-column sources is.** The gate and the four performance
inputs cannot change without a host event, and an event wakes the plugin — `plan-modulation-routing.md`
§2.2's *waking* clause. What they can do is hold a value that keeps a patch live, which is its
*staying live* clause, and which `amplifier_held_open` is what reads.

**The sounding press's velocity follows the bus.** `Voice::note_on` takes the press's velocity and
the note stack keeps it beside the note, as `mxm-mono-02-dsp` does; it moves with the bus — on a
press, release or choke that changes the sounding key, and on a priority switch — and a release keeps
the last one (`the_velocity_source_follows_the_sounding_press`). **It is what an envelope trigger
latches** as the Velocity source (*The collection's modulation standard*, above).

**A mixer level is only up if something reaches it**: the ring modulator's level counts only while
its input carries a route at a depth, and the Mix channel only while a route on it does — present or
still fading, and never at zero depth (`a_level_nothing_reaches_does_not_keep_a_patch_live`). **And a
static source counts only while its value is not zero**: `Mix ← Gate` or `Ring mod ← Gate` with no
key down is silence that only a host event can end
(`a_static_source_at_zero_on_an_audio_input_keeps_nothing_live`). A static value that is not zero
still counts — into the mixer it is a level the output holds — and an envelope whose own gate
fires without a key is moving, not static, even at a zero between its pulses.

**A gate input fires without a key only if its sum can cross.** `gate_fires_without_a_key` bounds the
summed input: every self-running route at its full magnitude, every static one at the value it holds.
At **LFO** the diode AND chops any high, so reaching the threshold is enough; otherwise the sum must
also be able to fall back to it, or no second edge comes. A shallow route — LFO 1 at a tenth — never
fires and keeps nothing live; two that sum past the threshold do
(`a_gate_route_too_shallow_to_fire_does_not_keep_a_patch_live`). An envelope on a gate input is moving when its own
gate fires without a key, one level deep — LFO 1 firing Envelope 1 firing Envelope 2 is live from a
zero phase, and the two envelopes looped with nothing self-running are not
(`an_envelope_chain_a_clock_starts_is_live`).

The VCA gate row cannot make an envelope live when ADSR IN does not read it
(`a_clocked_envelope_the_amplifier_does_not_read_is_not_live`). S&H is self-running only while the
sampler is on — anything contributing to its input — and deliberately not recursively: a static input
keeps it live, as the retired `mode ≠ OFF` did, and a feedback route cannot loop the question. With
nothing contributing it holds a static last step
(`a_sampler_switched_off_is_a_static_source_and_not_live`, which switches it off with a zero-depth
route). **A tremolo never makes a patch live**: it only dips a gain something else opened
(`tremolo_alone_on_a_closed_amplifier_is_inert`).
An LFO-position gate row can retrigger from any currently high column because the diode AND chops it
with the LFO square (`an_lfo_trigger_chopping_a_held_cv_is_live`). Held static CV and held envelope
sustain remain live (`a_held_keyboard_cv_on_the_amplifiers_row_keeps_the_patch_live`,
`an_envelope_kept_in_sustain_by_a_high_gate_row_is_live`). An envelope release is a tail, not a hold
(`an_amplifier_reading_a_releasing_envelope_is_tailing`). Volume zero is inert regardless of internal
ringing; envelope or settle decay is tailing; completed decay is inert. Inert output is exactly
silent.

**A tail is an envelope the amplifier reads**, never hidden state before it (audit D13). An envelope
counts toward the tail, the settle clock and `tail_samples` only while a present route at a nonzero
depth carries it into `target::AMPLIFIER` — whichever envelope that is. The first build counted both
whatever was routed, so a long filter release that only moved the cutoff kept an exactly silent
voice tailing to its end, while `tail_samples` promised only the amplifier envelope's release even
when the filter envelope was the one the amplifier heard. **`tail_samples` takes the routing** for
that reason. `only_an_envelope_the_amplifier_reads_holds_the_tail_open` holds all three cases to the
sample: inert one settle after the last envelope the amplifier reads goes idle, promised that long.

**Once the verdict is inert, an envelope nothing can hear is silenced rather than frozen** —
`mxm-mono-pr1`'s filter-envelope rule. A host may stop calling an inert plugin, so a release left
running would otherwise be whatever a later re-route found: audible if the host kept calling and not
if it slept. Only a *released* envelope that nothing but a host event can fire again goes; one held
in sustain by a high gate, driven by something self-running, or heard on the cutoff through an
amplifier INITIAL GAIN holds open is a patch in motion, and the verdict is not inert there anyway
(`an_unread_envelope_that_is_heard_held_or_self_running_keeps_its_motion`). The test above re-routes
the silenced envelope to the amplifier after inert, host asleep and awake, and hears nothing.

## The triangle is taken from the phase, not by folding the band-limited saw

`oscillator.rs`: folding the PolyBLEP'd saw with `abs` puts a **full-scale spike** at every reset,
because the residual takes the saw through zero there — found by the frequency test measuring 88 Hz
for 55. The triangle is continuous, so it needs no PolyBLEP and is built from the raw phase; the
hardware's seam (the reset's finite time) is a small glitch the research does not size, and it is
not modelled rather than invented. `the_triangle_is_two_straight_lines_with_no_spike_at_the_reset`.

## What is chosen, not measured

Every number about the machine that the research does not give, taken so the voice could be built,
to be listened to in the fidelity gate:

| Where | What | Chosen |
|---|---|---|
| `filter.rs` | `DiodeConfig::System100`'s pole set — equal rung capacitors, structure unverified | `[2.6, 1.8, 1.0, 0.2]`: threshold ≈ 10.2, sings ≈ +128 cents |
| `voice.rs` | `BASS_COMPENSATION`, input-side, the second pot gang's presumed job | 0.8 — the fundamental declines 1.1 dB at resonance 0.85, measured; the resonant peak compresses in the loop's saturator, which is the model's |
| `voice.rs` | `VCO_LFO_SEMITONES`, `FILTER_ENV_OCTAVES`, `FILTER_LFO_OCTAVES` | 12 st per LFO unit (the retired slider's, now what its presets translate through), 7 oct, 4 oct |
| `voice.rs` | `GLIDE_MAX_SEMITONES` | 2 — the retired slider's reach, its midpoint the hardware's semitone; on a pitch route that semitone is an amount of −1/144 |
| `routing.rs` | `PITCH_SEMITONES_PER_UNIT`, `CUTOFF_OCTAVES_PER_UNIT`, `PWM_SWING` | 144 (the next round number above the 134 st one source could reach before Rev 3), 10 (1 V/oct), 0.45 (50 % to 5 %) |
| `voice.rs` | `POST_TAIL_S` | 50 ms; an effect that is on lengthens the settle to its own tail |
| `effects.rs` | the phaser: `PHASER_STAGES`, its rate range, sweep, centre, feedback, and the two rows' scales | 4 stages; 0.1–6 Hz; ±2.5 octaves about 800 Hz; 0.35; 4 octaves per ten-volt unit on each row |
| `effects.rs` | `PHASER_MANUAL_HZ`, `PHASER_MANUAL_FRACTION` — MANUAL IN's control bandwidth, a **stability** requirement, not a taste | 2 kHz, and never past 0.05 of the rate; sized by measurement (see below) |
| `effects.rs` | the delay's fixed feedback and damping, undocumented on the plug-out | 0.45; one pole at 4 kHz |
| `effects.rs` | the spring: the third transit, `SPRING_T60_S`, the dispersion depth, the direct coupling | 50.4 ms between the two measured; 3.5 s within the measured 3.2–4.5; twelve all-passes; 2 ms |
| `effects.rs` | `SNAP_LEVEL`, `SNAP_HOLD_S` | −100 dB; 50 ms past the effect's longest memory |
| `oversample.rs` | `HALF_TAPS`, `KAISER_BETA` — the half-band kernel around the ladder | 11 odd taps a side (45 in all), β = 7: about −70 dB in the stopband, a transition of roughly ±10 % of the doubled rate |
| `oscillator.rs` | `WEAK_SYNC_KICK` — the charge a weak edge injects, in cycles | 0.04, so the lock range below 3/2 is about 0.02 wide |
| `voice.rs` | `EXT_CV_SEMITONES_PER_UNIT` — VCO-1's EXT CV at full slider on a ±5 V source | 120 per ten-volt unit: ±60 semitones from VCO-2, the machine's 1 V/oct |
| `voice.rs` | VCO-2's init fine tune, the init contract's *start slightly detuned* | +7 cents |
| `lfo.rs` | the invented triangle's level; the sine shaper's drive | bipolar ±0.5; `tanh` at 1.8 |
| `matrix.rs` | `GATE_THRESHOLD`, `AUDIO_ROUTE_RAMP_S`, the two gate rows' defaults | 0.25 units; 5 ms; the keyboard gate |
| `voice.rs` | `LFO_CV_OCTAVES_PER_UNIT` — a rate CV row at full GAIN | 10 octaves per ten-volt unit, 1 V/oct |
| `vca.rs` | `TONE_PIVOT_HZ`, `TONE_RANGE_DB` — the plug-out's TONE, no hardware to defer to | 1 kHz, ±6 dB |
| `oscillator.rs` | the triangle's seam | **not modelled** |
| `noise.rs` | the pink network read as two -6 dB shelves | a topology assumption on the printed values, corners derived |

Constants copied unchanged from the siblings and not re-argued: `flush`, `Rng`, `tanh_approx`,
the PolyBLEP residual and `clamp_pulse_width`, the ADSR's `ZERO_THRESHOLD`, `ATTACK_OVERSHOOT`,
`ATTACK_TAUS` and `DECAY_TAUS`, the ladder's `NEWTON_ITERATIONS` and `NYQUIST_FRACTION`.
`MIN_TIME_S` is the machine's 0.4 ms. **Re-argued from the copy:** `RESONANCE_MARGIN` (1.25, the
manual's "near 8"), `EXCITATION_THRESHOLD` (the singing point itself), `EXCITATION_LEVEL`
(−100 dB) and `K_MAX` — see *The resonance control sings from the same position*.

## Dependencies: one at runtime, and it is why the floor holds

**`mxm-modulation`, and nothing else.** The routing conversion added it; it has **zero dependencies
of its own** and sits at this same 1.87 floor, so the shipped graph still does not reach the 1.95 GUI
floor. That is a claim checkable with `cargo tree -e normal,build -p mxm-mono-00-dsp`, and running it
by hand is what keeps the split honest — nothing else would catch a GUI crate arriving through it.

`[dev-dependencies]` holds **`mxm-measure`**, the collection's measurement rulers — zero dependencies
at this same floor, reaching only tests and `examples/`, never a shipped `.clap`.
[`../mxm-measure/AGENTS.md`](https://github.com/mxm-audio/mxm-kit/blob/main/crates/mxm-measure/AGENTS.md)'s verification section checks that rather than
asserting it.

It also holds **`mxm-audio-file`**, which writes the listening demo, and **`mxm-audio-file-decode`**,
which its test reads the file back through — test-only edges on the same terms — and
**`mxm-modulation` again with its `conformance` feature**, for `src/conformance.rs`; this crate's
own `conformance` feature forwards it for the plugin's tests, and neither reaches a bundle. The decoder's
MPL-2.0 symphonia therefore reaches this crate's tests and never its shipped graph.

**`system_demo` no longer writes its WAV by hand.** It used to, and the copy it carried was one of
six that had drifted apart; it now encodes through `mxm_audio_file` and applies its own headroom at
the call site, because normalisation is this crate's judgement and not the encoder's.

## The fourth honest copy: what this crate owes the extraction plan

- **The fourth honest copy**, and its routing frame has now been extracted: the collection's first
  shared DSP crate is [`crates/mxm-modulation`](https://github.com/mxm-audio/mxm-kit/blob/main/crates/mxm-modulation/AGENTS.md), on the owner's ruling
  of 2026-09-10 and on the evidence of this crate's `Columns` being the same object as two siblings'.
  Everything else here was copied whole and **is still not extracted from**. What
  this crate owes the extraction plan is the evidence row `plans/plan-mxm-mono-00.md` §7.2 names:
  so far the ADSR, `flush`, `Rng`, `tanh_approx` and the PolyBLEP residual are **identical** to
  `mxm-poly-06-dsp`'s; the diode ladder is `mxm-mono-03-dsp`'s **with one configuration added and
  its `tanh` shared from `lib.rs`** — identical in equation, not in file.

## Verification: the rulers, the demo and the properties the tests keep

**The rulers are shared, the thresholds are not.** `mxm-measure` is a `[dev-dependencies]` entry —
zero dependencies at this same 1.87 floor, and **not in the shipped graph**, which is what the
manifest's *no runtime dependencies* comment means. Measurements come from there; every bound and
its headroom stays in the test that argues for it.

The demo renders `mxm-mono-00-system-demo.wav` at the repository root — twenty-six passages over
70 seconds, the last four covering effects — and announces each passage time on stderr.

Properties the tests must keep asserting, because each regresses silently:

- silence in gives **exactly** zero out after the tail, and the voice reports inert
- no NaN or inf across a sample-rate × cutoff × resonance sweep; output inside the stated bounds
  under overdrive
- `reset()` leaves no tail — the glide's charge and the hold capacitor included
- the ladder's threshold **measured at four sample rates** against the closed form, and the copied
  303 set still reproducing Stinchcombe's 18.4; the resonance control singing from the same
  position at every cutoff, pinned poles included, and the voice singing at every octave to 14 kHz
- the oversampler's round trip a pure delay to −60 dB, its decimator removing what is above the
  base Nyquist, and exact silence through it
- the delay and the springs at their times at 384 and 768 kHz; a crossfade retargeted mid-fade
  continuous; All Sound Off exactly silent on its next sample and inert on the verdict; negative
  key follow closing the filter above middle C
- a priority change moving the bus with no edge; a gate-row repatch that changes the level
  releasing or retriggering; a zero-level reverb emptied; a non-finite bend or expression
  leaving the output finite
- the LFO trigger waiting for the square and releasing on its fall; a held CV on the amplifier's
  row reporting live; a clocked envelope the amplifier does not read **not** reporting live; the
  sampler OFF on the amplifier's row **not** live; a held CV chopped by the LFO trigger live; an
  envelope held in sustain by a high gate row live; a releasing envelope the amplifier reads tailing; All Notes Off stopping at a patched gate row; Volume zero inert over a
  tail; an emptied line reading zeros with its memory untouched
- an envelope the amplifier does not read holding no tail open, one it reads counted in
  `tail_samples`, and a re-route after inert reviving nothing
- every waveform at the frequency asked for, on every range; the triangle with no spike
- the wart table above, row by row
- a tie retriggers nothing at GATE and the envelope at GATE+TRIG; a higher key under low-note
  priority changes no pitch and still retriggers a GATE+TRIG envelope; a note ending at its own
  offset, or pressed through a keyboard route that never crosses the threshold while nothing else
  holds the gate high, fires nothing
- INITIAL GAIN makes the patch live with no key; Volume at zero makes it inert
- strong sync pins VCO-2's period to VCO-1's and the switch frees it; weak sync locks just under
  3/2, 4/3 and 1/1 and rolls above them; the ring modulator carries the difference tone and not
  VCO-1's fundamental; the EXT CV row cross-modulates; LFO-2 reaches nothing until it is routed
- a routed pulse width rests at square whatever the manual width; every LFO core source keeps a
  patch it opens live; a tremolo alone on a closed amplifier is inert
- the phaser's rate follows the slider and a rate CV on LFO IN; the delay repeats at its time and
  decays at the fixed feedback; the spring rings at its measured band and T60; each is
  bit-transparent at zero and reaches **exact** zero after its tail
- a centre CV alternating at Nyquist leaves the phaser sweeping rather than oscillating, at every
  sample rate and every alternation period, and an unpatched MANUAL IN is bit-transparent
- the init patch is finite at every sample rate a validator tries, 1 kHz to 768 kHz
- two instances render identically

## What is not verified, and must not be claimed

**No hardware was measured**, here or in any source this instrument rests on. The table above is
every constant that was chosen. The tests prove the model is self-consistent; **they do not
establish that it sounds like the machine.** The gate that would is a listening comparison against
reference recordings, and it has not been run. Fidelity is UNVERIFIED.

Linux and macOS are unverified — there is no CI (root *Windows, Linux and macOS*) — and the
development machine is Windows.
