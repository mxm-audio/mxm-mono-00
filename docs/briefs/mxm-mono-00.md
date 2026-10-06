# mxm-mono-00 — UI design brief

Required by `MXM_DESIGN_SYSTEM.md` §14, written before implementation. Answers the ten questions in
order, then records the deliberate deviations and the decisions the plan
(`plans/plan-mxm-mono-00.md` §9) hands to this document.

*Since the split (2026-10-06):* the design system is mxm-kit's
[`docs/MXM_DESIGN_SYSTEM.md`](https://github.com/mxm-audio/mxm-kit/blob/main/docs/MXM_DESIGN_SYSTEM.md),
`crates/ui` is mxm-kit's too, and the plans cited here are in the private archive.

**Instrument:** a monophonic semi-modular with two oscillators, two envelopes, two LFOs, a
sample-and-hold, a ring modulator, seventeen routable targets taking twenty-five sources each, and three effects.
Architecture inspired by the Roland System-100 as its plug-out pictures it; the interface is not.
**Fidelity is UNVERIFIED throughout** — no hardware was measured for the research this rests on.

**Routing is target-local stacks on every target card, directly beneath the control each moves** —
the patch bay's inputs and, since the modulation plan's Rev 3, the plug-out's panel wiring too: a
source is chosen where it arrives, never where it leaves. **No Patch page, optional overview, hidden
matrix or grid mode in the shipped editor.** S&H belongs under Modulators. The no-cables ruling is unchanged: no patch
cables, overlay or cable layer. Target controls must never become passive labels.

---

## 1. Primary sound-design task

**Patching.** On every other instrument in the collection the signal flow is fixed and the task is
setting levels along it; here the flow *is* the patch. The first thing a person does with a
System-100 is decide what feeds what — the S&H clock into an envelope, VCO-2 into VCO-1's pitch, the
mixer into itself — and the sound follows from that. So the task is not performing a filter (mono-03)
or voicing a chord (poly-06) but **building a signal path and then playing it**, and the editor is
organised so that building and playing are two views of the same patch, not two modes.

## 2. The three to five parameters users reach for most

1. **Cutoff** and **Resonance** — the ladder is still the voice's centre.
2. **VCO-2's coarse tune** — the interval between the oscillators, which is what sync and the ring
   modulator turn into timbre.
3. **The S&H sample time** — the clock that runs the machine's most characteristic patches.
4. **The matrix row for the VCA envelope's gate** — the one row people re-patch mid-performance,
   because it is what turns a keyed patch into a self-running one.

Cutoff and Resonance take **Primary** sizing. VCO-2's coarse tune and the sample time are
**Standard**. Everything else on the Synth view is **Compact** (32 px, value on hover), per §7.1 and
the plan's §6: roughly sixty controls besides the matrix, and the tier system is the answer, not a
new size. Sliders where §7.1 says range and comparison matter more than compactness: the
**mixer's four levels** and **each ADSR's four times**, as vertical faders side by side — the
collection's fader row since 2026-09-25.

## 3. Signal flow that must be visible

```
 keys ─► portamento ─► keyboard CV ─┬─► [pitch input: glide · LFO · EXT CV] ─► VCO-1 ─┐
                                    └─► [pitch input: glide · LFO · EXT CV] ─► VCO-2 ─┤ sync · ring mod
                                                                          noise ─┤
                                                                          ring ──┴─► MIXER ─► HPF ─► VCF ─► VCA ─► phaser ─► delay ─► reverb ─► Volume
   sources for the matrix: LFO-1, LFO-2, S&H, S&H clock, the VCOs, sync, ring, noise, mixer, both envelopes, keyboard CV
```

Four things must read without a manual, each a fact about this machine that the plug-out's screen
hides or that emulations get wrong:

- **Glide sits after portamento, on the oscillators, not on the keyboard CV.** The KYBD CV source
  carries no glide dip; the dip and the vibrato are routes on each oscillator's pitch input, which
  is where the plug-out's DESTINATION switch sent them.
- **Two envelopes, each with its own trigger mode, and either can drive anything.** The interface
  calls them **Envelope 1 / Envelope 2**, not their default destinations. Likewise **LFO 1 / LFO 2**
  name the generators consistently across cards, source menus, parameter names and tooltips.
  Amounts name their input role, never a source the user may replace.
- **LFO-1 is wired into the filter and the amplifier by default; LFO-2 into nothing.** A second LFO
  that does nothing until patched must not read as broken.
- **The effects are serial, in a fixed order, after the amplifier**, and cannot be re-patched.

## 4. Which controls belong in Play view

**Not applicable — no `Play` view.** The semi-modular is performed on Synth, with FX and the
Parameters testing list separate; another performance subset would duplicate that workflow. The reached-for parameters in §2 are Primary and Standard
on the Synth view, which is where a performer plays it. The plan's §6 asked this brief to answer
explicitly; this is the answer, and the same one `mxm-poly-06` gave.

## 5. Advanced controls and their disclosure

**One expander, in the Voice card's footer**, `mxm-mono-01`'s idiom, holding what the plug-out puts
on its top bar rather than in a module: **bend range** and **master tune**. TEMPO SYNC sits with
the delay on the Effects card instead (§ deviations below). Each defaults to a value that changes nothing
about the panel's sound — bend range 2 semitones, tune centred — so a fresh patch is unaffected by never opening it.

**Not disclosed, although a review will suggest it:** the VCA's TONE and INITIAL GAIN, the sync
strength, the two envelopes' trigger modes, the LFOs' CV gain and offset. Each is a module control
on the plug-out and changes what the module *does*; hiding it would make the card lie about the
module. They are Compact, not hidden.

## 6. Views

**Space-derived pages**, following design-system §3.2. Voice is Performance; LFOs, S&H and
both envelopes are Modulators; oscillators and the Ring mod card are Generators; Mixer/Filter/Amplifier are Tone;
the one Effects card holds Phaser, Delay and Reverb, in processing order (the owner, 2026-09-24). LFO and oscillator pairs remain preferred
groups; envelope/destination groups split across categories. Every card remains indivisible.
Target-card stacks are the complete routing workflow, directly beneath the controls they move;
there is no Patch page, matrix or destination selector. S&H's four controls stay together after the LFOs.
Full names, no fixed tab count, no bar for one page. Parameters stays separate at developer
CC 119 value 127 and has no tab. Controller maps do not change.

**Volume is on no card.** It is an inline slider in the app bar beside the level meter, which is
where design system §3.1 item 6 puts the master output (owner, 2026-09-18), and the keyboard cursor
reaches it there.

## 7. Identity accent

**Copper.** Dark `#E89A6B`, light `#8A4A1C`.

Measured with the same WCAG formula `mxm_ui::theme::contrast` implements, against the surfaces it is
drawn on, never judged by eye:

| | vs `surface-1` | vs `surface-2` |
|---|---:|---:|
| Dark `#E89A6B` | **7.69 : 1** | **7.03 : 1** |
| Light `#8A4A1C` | **6.82 : 1** | **5.72 : 1** |

Both themes clear 4.5 : 1 for text and 3 : 1 for control boundaries.

**The alternatives, all of which passed the gate**, so the choice was made on hue separation from the
modulation and status colours: selected sources and parameter states must remain distinguishable
from the LFO's own blue.

| Candidate | Dark s1 / s2 | Light s1 / s2 | Why not |
|---|---:|---:|---|
| Sky `#4FC3F7` / `#01579B` | 8.70 / 7.96 | 7.40 / 6.20 | ~15° from `mod-lfo`; on a matrix that routes two LFOs, the one collision that misleads |
| Ice `#7FD8FF` / `#0A5F86` | 10.93 / 10.00 | 7.01 / 5.88 | Same neighbourhood |
| Mint `#4FE3C1` / `#0B6E56` | 10.86 / 9.94 | 6.21 / 5.21 | ~15° from `mod-performance` |
| Sand `#E8C86A` / `#6B5300` | 10.71 / 9.79 | 7.34 / 6.15 | Within 10° of `warning`; a lit connection would read as a clip warning |
| Peach `#FFB37A` / `#9A4A00` | 9.94 / 9.09 | 6.26 / 5.25 | Passes; a lighter neighbour of copper, closer to coral on mono-01's list |
| **Copper** | 7.69 / 7.03 | 6.82 / 5.72 | ~20° from `danger` and from `warning`, the distance the other briefs accepted on a crowded wheel; the darkest of the warm candidates, so the furthest from coral; and it does not resemble the hardware's silver-and-black arrangement |

**What was already taken:** lime is `mxm-mono-03`'s, rose `mxm-poly-06`'s; orchid and coral are on
`mxm-mono-01`'s candidate list and this brief leaves them there. **This brief does not edit mono-01's.**

**§5.3's trade-dress rule is satisfied**: the hardware is silver panels, black sliders and a wooden
case, and this is none of those.

**The plan records this as the owner's choice.** Copper was taken so phase 0 could close; the owner
may pick another passing candidate, and the change is one constant in mxm-kit's `crates/ui/src/theme.rs` and
this table.

## 8. Live visualizations

Two, each answering a question the controls cannot. The retired matrix visualization is not part
of the shipped editor.

1. **Patch activity.** The plan's §5.5 state — *live*, *tailing*, *inert* — as a small indicator in
   the app bar beside the level meter, because a live patch keeps sounding with no key down and a
   person must be able to see that the instrument, not the host, is doing it.
2. **Output level with clip indication** in the app bar, per §3.1, measured **after Volume**, which
   is where the plan places the meter.

Deliberately **not** included: a filter response (the ladder's is not what this machine is about); an
oscilloscope; envelope and LFO displays.

### Ownership

A single `Telemetry` struct, `Arc`-shared, **atomics only**, written once per block — the mono
instruments' pattern, including a **peak that is max-combined and reset on read** and a **clip that
latches** until acknowledged.

| Visualization | Writer | Truth model |
|---|---|---|
| Patch activity | Audio thread, once per block | **Exact** — the state the DSP reported |
| Output level + clip | Audio thread, once per block | **Exact** |

## 9. What is removed from the source hardware layout, and why

**Kept:** the control set — the plug-out's module list — and the signal flow it implies.
**Removed:** the panel layout, appearance, geometry, control style, colour arrangement, typography,
trade dress; the plug-out's cables; the plug-out's keyboard, arpeggiator, scatter, key hold, octave
shift and patch bank.

The hardware's panels were consulted through `research:instruments/system-100.md` §3 and §12 for the
**control set and its grouping**: keyboard CV and glide · envelope · LFO · VCO · mixer · noise · HPF
· VCF · VCA on the 101, the 102's additions, and the plug-out's merge of the two. Nothing about how
either looks was taken, and no image of either is kept in this repository.

| Removed | Why |
|---|---|
| The panel layouts and their sliders | §2 forbids copying the inspiring instrument's panel. Controls are grouped by module; the grouping survives because it is the signal flow |
| **The patch cables and every jack drawn as a jack** | The owner's no-cables ruling: target-local selectors expose the routing parameters without copying the plug-out's trade dress |
| The keyboard, arpeggiator, scatter, key hold, octave shift | §2 forbids a decorative keyboard; the rest is the host's, per the root's *an instrument does not carry what the player or the DAW already does* (the monorepo root's, now in mxm-kit's [`collection-rules.md`](https://github.com/mxm-audio/mxm-kit/blob/main/docs/collection-rules.md#the-goal-and-how-much-licence-a-copy-has)) |
| The patch bank, WRITE / READ / SEND / GET | The preset system is the collection's, in the app bar |
| The A-440 oscillator, check-point jacks, rear jacks | Dropped by the plug-out; nothing for a plugin to do with them. The mixer's external input is kept, as the **External input** target |
| Silver, black and wood; the wordmarks | §2 and §5.3 |

**Interface improvements, each a limit of the panel and not of the circuit** (the plan's §6): the
target-card stacks in place of forty cords and the panel's fixed wiring; two oscillators in one box; range switches in place of a knob in
hertz; the pulse width as a knob with a Pulse width stack beneath it, in place of the PWM source switch
and depth; coarse and fine tune on both oscillators.

## 10. Minimum size and 200% scale

**Resizable: the editor's `REFERENCE` and `MINIMUM`, derived and held by its tests.** Category/card
order is §6's. The minimum is the widest computed floor plus two gutters; the opening size is the
quarter-4K budget hugged around every page.
**Ring mod is a Generators card of its own**, beside the oscillators, and says what the panel
otherwise could not: its carrier is always Oscillator 2. Voice has no routing
of its own any more — the glide is chosen on the oscillators — so it is the narrowest card.
`every_dynamic_page_fits_and_every_card_is_reachable` checks every page with Advanced reserved
open, both themes, at opening size, the quarter-4K content size and the minimum. The Effects card uses the
same paging renderer. Independent zoom remains **75–200%**; §15's fixed-physical-window DPI/zoom and native/DAW
inspection remain open, not established by headless geometry.

### Target-local source selectors

**Amended by the routing conversion, 2026-09-14** (`plans/plan-mxm-mono-00-modulation.md`): there is
no dropdown. Every `Section::targets()` input carries a **routing stack** on its target card —
seventeen in all: fifteen on Synth including S&H, the phaser's two on Effects — the live routes as rows and `‹ modulate ›` beneath
them. There is no *Default* option because there is no selector: the plug-out's normalled
connections are present routes in the init patch, and removing one is what pulling the cord out was.
**Rev 3 left no exception** (the owner, 2026-09-22: *modulation source on the modulation target
card*): DESTINATION, the VCO and VCA LFO depths, both PWM switches, KYBD CV and SAMPLE MODE are
routes too, on the oscillators, the Amplifier, the Filter and S&H.

Every input is named for what it moves, as `mxm-mono-01`'s are: the Filter's one **Cutoff**
(Envelope 1, LFO 1 and the keyboard wired at Init), each oscillator's **Pitch** and **Pulse width**,
the Amplifier's **Amplitude** and **Tremolo**, and each LFO's **Rate**; the host names include their
module prefix where the collection needs it. Mixer's re-routable
levels are the two oscillators, noise and **Ring mod level** — the 102's own RING MOD slider —
beside **External input**, the EXT IN jack as a routing input; the ring modulator's own input is **Ring mod**,
painted *Input* on its card. Gate, sync, S&H input and Phaser
rate/centre name their roles rather than their default sources. No gain knob is invented for an
input that has none — **every route carries its own**, which is what replaced the seven row
attenuators and the panel's depth knobs. The pitch inputs' faders are square-law about the centre,
so a vibrato's depth sits in the first tenth of the travel.

**Short in-card labels only omit repeated module prefixes; full names remain in tooltips,
accessibility and host automation** — and the routing rows are now the main case, because a row
reads `<target> from <source>` and both halves are module-qualified. `routing::TARGET_PANEL_NAMES`
is the painted form. Adding a source and removing one are each **one bracketed host edit**, and a
removal leaves the depth alone so re-adding restores it.

`plugins/mxm-mono-00/tests/routing_editor.rs` must run after layout or editor changes. It clicks
the shipped panel, not the lab prototype, and rejects read-only routing captions, a restored Patch
page or matrix cells, unreachable S&H, and incorrect developer category addresses. It also pins
every target's add and remove gestures, all 425 pairs as data, every stack inside its card through
narrow/wide reflow, and that no card draws a retired destination, depth or source switch. Naming/enum-ID tests pin consistency and state compatibility.

---

## 10a. Coherence with the sibling editors

| Taken | Why |
|---|---|
| **`SECTIONS` as a `const` array, with the order stated as the contract** | It is the information architecture |
| **`mxm_ui::ModuleCard` per section**, names from the shared vocabulary | §3.3's grouping, already themed |
| **`knob_row`: columns of their own width** | The siblings' answer to knobs spread across a card |
| **One place brackets gestures** — `binding::Bound::apply` | Load-bearing for the player's step editing. The fourth copy of `binding.rs`, unless the shared crate lands first |
| **A `Parameters` view, and Init in the app bar's patch actions** | Same placement |
| **`Telemetry` as the only DSP → editor channel** | Same rules |
| **The zoom control, 75–200%** | Independent of native resize and card reflow |
| **The footer expander** | `mxm-mono-01`'s idiom |

**Where it deliberately differs**: editable source menus at each target, because the flow itself
is the patch; S&H is a Modulators card; the three built-in effects share one Effects card, in processing order.

---

## 11. Decisions the plan handed to this brief

| Decision | Standing |
|---|---|
| **The identity accent** | **Copper, measured above**, taken so phase 0 could close. The owner's to change |
| **The effects' order and wet/dry structure** (plan §7.3) | **Phaser → delay → reverb, each a wet/dry mix under its one level.** Chosen: the manual's text and every source read are silent on the order, and the SIGNAL FLOW overlay could not be read from the manual's text. Recorded chosen in the DSP crate's doc too |
| **The two gate rows' defaults** (plan §9.8) | **Both default to the keyboard gate.** Chosen: the manual's text does not state them; the hardware's default for each unit's envelope was its keyboard gate (`system-100.md` §12.2), and the S&H clock's firing of the second envelope (wart 16) is one click away. Recorded chosen; a reading of the overlay may correct it |
| **Glide's calibration point** (plan §4) | **The hardware's semitone in about 50 ms** was the retired depth slider's midpoint; on a pitch route it is an amount of −1/144, at about 8 % of the fader's travel. Zero is the init value. The 50 ms is the fixed RC and is not a control |
| **Which rows are audio** (plan §5.1) | AUDIO MIXER EXT IN and RING MOD IN; every other row is CV or gate |
| **TEMPO SYNC's fallback with no transport** (plan §5.5) | Implementer's; with no tempo the delay keeps its own time and its knob reads that time (the collection's tempo-sync rule, `plugins/AGENTS.md`) |
| **A standalone harness** | Not built; the shipped panel is exercised in-process and the bundle through MXM Player |

---

## 12. The recognisability trial

§9 makes a recognisable *control set* the requirement — not a recognisable panel, which §2 forbids.

**The trial patch:** VCO-1 saw, VCO-2 square a fifth up, mixer both up, S&H clock at a slow rate
patched into the VCA envelope's gate, S&H OUT into VCO-1's CV at a low amount, resonance high,
no key held.

Run with **someone who has used a System-100, the plug-out, or a semi-modular**, without showing them
the hardware or the plug-out.

### Stage 1 — before composition, on a wireframe

1. *"What is making this play by itself?"* → the **S&H clock into the VCA envelope's gate**, found
   on the VCA envelope's source dropdown. Naming the LFO is a fail of the drawing.
2. *"How many envelopes does this have, and what does each drive?"* → **two**, and the answer names
   the defaults *and* that the matrix can change them.
3. *"Where does glide act?"* → **on the oscillators, after portamento.** "On the keyboard" is a fail.

### Stage 2 — on the finished editor

| Task | Control |
|---|---|
| *Make it darker.* | `cutoff` |
| *Make the interval a fourth instead of a fifth.* | VCO-2 coarse tune |
| *Make it run faster.* | S&H sample time |
| *Make it play from the keys again.* | the VCA envelope's gate row, back to default |
| *Add some space.* | reverb level, on the Effects card |
| *Turn the whole thing down.* | `volume` |

**Per task:** found within ten seconds, entering at most one wrong view. **Gate: five of six.**

*Results: not yet run.* An unrun trial is recorded as unmet, never as passed.

---

## What the build measured, and where it decided otherwise

Recorded after the fact, on 2026-09-03, so the brief and the editor say the same thing.

| Brief | Built | Why |
|---|---|---|
| A fixed reference frame | **Reflowing Synth rows**, opening at the budget hugged (§10) | The quarter-4K budget hugged; fit is pinned from above and below |
| Fixed columns | **Signal-chain rows**, grouped parallel branches | Sequence is stable; only row breaks move |
| Routing footer | **Target-card stacks directly beneath their controls** | The owner's requirement: immediate, compact editing of every assignable input, with no Patch page |
| Range and PWM source as switches | **Range as six buttons; the PWM source a route** | The owner, 2026-09-27: every range is buttons, never a drop-down — §7.3's range exception, which the shared control admits. The PWM source became routes in Rev 3 |
| Volume on the Voice card | **In the app bar, beside the level meter** | Design system §3.1 item 6 puts the master output there (owner, 2026-09-18) |
| TEMPO SYNC in the Voice expander | **On the Effects card, beside the delay** | A switch beside the control it changes |
| Card captions as written | Removed (the owner, 2026-09-27: no help text on the panel); each fact is its control's tooltip | Three had been shortened to one line first, at fourteen points per card in the tallest column |

## Deliberate deviations from the design system

### ~~A one-line caption under four cards~~ — withdrawn (§7.6)

The owner ruled out help text on the panel (2026-09-27). Each envelope's default destination is its
Attack's tooltip, LFO 2 doing nothing until added at a target is its Rate's; Voice's *glide acts
after portamento* described the plug-out's internals and went.

### No undo/redo (§3.1)

The only undoable events are parameter edits, which hosts already track — and a matrix row is a
parameter.

### An activity indicator in the app bar (§3.1)

§3.1 lists the app bar's contents and an activity state is not among them. It is added beside the
meter because this is the first instrument that sounds with no key down, and a person must see that
it is the patch doing it. One glyph, three states, no text.

---

## The fidelity gate

**UNVERIFIED.** No hardware was measured for the research this instrument rests on, and no listening
comparison against reference recordings has been run. The three effects are chosen models behind
documented controls and cannot be verified against anything but the plug-out, which has not been
listened to side by side either. `system-100.md` §13 names the first measurements worth making if a
unit appears: the ladder's bass against resonance, the LFO's three output levels, the VCA's response
to the LFO.

---

## Sign-off checklist

- [ ] Signal flow readable without documentation: **glide after portamento**, **two envelopes with
      defaults the matrix can override**, **LFO-2 patched to nothing**, **effects serial and fixed**
- [x] Cutoff and Resonance at Primary sizing, adjacent
- [x] The Voice card's expander collapsed by default, every disclosed control changing nothing at Init
- [x] Identity accent applied, with the measured ratios above holding against `surface-2` as well
- [x] Every parameter present in the section map — `every_parameter_is_drawn_exactly_once`; routing is intentionally also editable on its target card
- [x] Every target-local stack edits its routing parameters, and no card carries a destination or source switch — `tests/routing_editor.rs`
- [x] Card names match the collection's vocabulary; only the numbering is new
- [x] No Patch page or matrix widget; S&H's controls and input on Synth — `navigation_has_no_patch_page_and_sample_hold_is_editable_on_synth`
- [x] Every page reachable, the first musician page active on open — `the_views_are_the_briefs_and_synth_opens`
- [x] Each page's height measured and pinned by a test rather than assumed — with the Voice expander open
- [ ] Verified by eye at 75%, 100%, 150% and 200% — **100 % light only, 2026-09-03**
- [ ] Dark and light both complete, with all control states
- [ ] §12's trial run and recorded
- [ ] §15 QA gate passed in full
