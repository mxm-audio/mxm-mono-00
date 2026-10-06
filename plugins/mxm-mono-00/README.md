# mxm-mono-00

A monophonic semi-modular. Architecture inspired by the Roland System-100 as its plug-out pictures
the machine — the 101 keyboard synthesizer and the 102 expander merged into one voice — but the
interface is not, and the name is not. Not affiliated with or endorsed by Roland.

**The synth only.** The plug-out's keyboard, arpeggiator, scatter, key hold, octave shift and patch
bank are [MXM Player](https://github.com/mxm-audio/mxm-player/blob/main/apps/mxm-player)'s or your DAW's, by the collection's rule that an
instrument does not carry what the host already does.

## What it is

| | |
|---|---|
| Oscillators | Two, each with a six-position octave range, coarse and fine tune, sawtooth / square / triangle, a manual pulse width, and a pulse-width input whose modulation **only narrows from square** — the plug-out's six sources and anything else. Each has a pitch input: glide, vibrato, cross-modulation or any other source, on one oscillator or both. VCO-2 has a **sync switch with two strengths**: strong locks at any ratio, weak locks only near the named intervals and rolls between |
| Ring modulator | VCO-1 × VCO-2 by default; either input re-patchable |
| Noise | White, high-passed near 3 kHz as the hardware's is; pink, falling in two shelves |
| Mixer | Four levels — the two oscillators, noise and the ring modulator — and a **Mix** input that takes any other source at a level of its own, into a soft saturator |
| Filter | A passive high-pass, then a **diode ladder** with its own pole set, whose bass declines slightly with resonance. Self-oscillates from 80 % of the resonance control at every cutoff, the ladder running at twice the sample rate so the control reaches the hardware's 20 kHz |
| Amplifier | Initial gain, a modulator input (Envelope 2 by default), a tremolo input (LFO 1 by default) that **can only dip**, and the plug-out's tone tilt |
| Envelopes | Two ADSRs, each with three trigger modes: gate, gate-plus-trigger, or refiring on the LFO while a key is held |
| LFOs | Two with five shapes, whose sawtooth, square and sine **sit at three different levels**, as the hardware's do; each with a rate CV input |
| Sample-and-hold | Its own clock, a lag, and an input that takes LFO 1's saw, reverse saw, triangle or sine — the plug-out's sample modes — or anything else; and the clock is a source, which is how a patch plays itself |
| Routing | **Seventeen inputs, twenty-five sources, any number of sources per input.** Every input has a `‹ modulate ›` menu beneath the control it moves: choose a source and a depth fader appears. **No card chooses where its own output goes.** The machine's own wiring is the init patch. Automatable and per-step modulatable like everything else |
| Effects | The plug-out's phaser and delay, and the Model 103 mixer's spring reverb — serial, phaser → delay → reverb, after the amplifier |
| Keyboard | Low-note priority as the hardware's, or last-note; portamento that is a hold capacitor and never snaps; a glide that dips on every gate edge and recovers in about fifty milliseconds, acting on the oscillators after portamento |

## The controls

897 parameters on three views, 850 of them routing. **Synth** has eleven cards in signal-chain order: Voice,
the two LFOs, sample-and-hold, the oscillators, mixer, filter with its envelope, and amplifier with
its envelope. **Volume** is not on a card: it is in the app bar beside the level meter. Every
input carries its routes directly beneath the control it moves, each a signed depth fader, and a
`‹ modulate ›` menu to add another. The pitch inputs' faders are finer near the centre, so a
vibrato's few cents sit in the first tenth of the travel; Shift drags finer still. Names describe
what the input moves, never what is patched to it: **Cutoff** on Filter — one input, as the
envelope, the LFO and key follow all move the cutoff — **Pitch** and **Pulse width** on the
oscillators, **Amplitude** and **Tremolo** on Amplifier, **Rate** on the LFOs, **Ring mod** (what the ring
modulator multiplies Oscillator 2 by) and **Mix** on Mixer. Generator cards and source menus agree on
**LFO 1 / LFO 2**, **Envelope 1 / Envelope 2** and **Oscillator 1 / Oscillator 2**.
**There is no Patch page or patch-bay grid**; S&H's rate, lag and input are together on Synth.
**FX** has the three effects in processing order, including the Phaser's two inputs. **Parameters** is every parameter as a slider
with direct text entry. Cutoff and Resonance are the two large knobs; everything else is compact,
with its value in the tooltip and on hover.

**Every amount starts at zero.** The init patch is one sawtooth into an open filter, the VCA
envelope up, and VCO-2 seven cents sharp so the moment its level rises the two beat.

**Tempo sync**, on the FX card, makes the delay time a note division of the host's tempo — a
thirty-second note up to two beats — and is inert until a tempo arrives.

## Presets

Fifty factory sounds, compiled into the plugin, the first twenty one per mechanism: two saws, a fifth, strong and
weak sync, the bell, cross-modulation, pink and white noise, a squelch, the sample-and-hold
patches, a self-running one, a drone, and one each for the three effects. **Init is not a file**:
it is generated from the parameter defaults, so it cannot be deleted and cannot drift from them.
Your own presets are saved as readable JSON under the platform config directory.

## Status

**The parameters, MIDI, presets and all three views of the editor shipped.** The editor is built
and not yet signed off: the design system's QA gate by eye at every zoom and in both themes, and
the brief's recognisability trial, have not been run; see
[`docs/briefs/mxm-mono-00.md`](../../docs/briefs/mxm-mono-00.md).

**Fidelity is UNVERIFIED.** No hardware was measured, here or in any source this instrument rests
on. The tests prove the model is self-consistent — not that it sounds like the machine. See
[`crates/mxm-mono-00-dsp`](../../crates/mxm-mono-00-dsp/AGENTS.md) for every constant that was
chosen rather than measured.

## Building

```bash
cargo xtask bundle mxm-mono-00 --release
clap-validator validate "target/bundled/mxm-mono-00.clap"
```

MIT licensed — see [LICENSE](LICENSE). All code is original.
