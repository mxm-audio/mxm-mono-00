//! mxm-mono-00's routing as the collection's modulation standard checks it
//! (`mxm_modulation::conformance`; `plans/plan-modulation-standard.md`).
//!
//! Behind the `conformance` feature, which only `[dev-dependencies]` enable — this crate's own
//! tests, and the plugin's, whose route readings are held to [`Declared::deliver`] — so no shipped
//! graph carries it. [`Declared`] answers every question through [`crate::routing`]'s own tables
//! and a real [`Graph`], never a copy of them.

use mxm_modulation::conformance::{Declaration, Kind};
use mxm_modulation::standard::{self, Law, Offer, Performance, Sign};

use crate::effects::{PHASER_LFO_OCTAVES_PER_UNIT, PHASER_MANUAL_OCTAVES_PER_UNIT};
use crate::routing::{
    self, Graph, KEY_UNIT_SEMITONES, Routing, SOURCE_NAMES, SOURCES, TARGET_NAMES, TARGETS,
    narrowed_width, target,
};

/// What each target is, for the standard: the rates and the cutoff in octaves, the pitches in
/// semitones, the widths only narrowing; the machine's own laws where there is no standard reach —
/// the S&H's sampler, the gates' and the sync input's crossings, the ring's and the mixer's audio
/// inputs, the VCA level and the tremolo's dip.
const KINDS: [Kind; TARGETS] = {
    let mut kinds = [Kind::Machine(Law::Sum); TARGETS];
    kinds[target::LFO1_RATE] = Kind::Rate;
    kinds[target::LFO2_RATE] = Kind::Rate;
    kinds[target::SH_INPUT] = Kind::Machine(Law::Sample);
    kinds[target::VCO1_PITCH] = Kind::Pitch;
    kinds[target::VCO2_SYNC] = Kind::Machine(Law::Edge);
    kinds[target::MIXER_INPUT] = Kind::Machine(Law::AudioInput);
    kinds[target::VCF_GATE] = Kind::Machine(Law::Edge);
    kinds[target::AMPLIFIER] = Kind::Machine(Law::MachineAmplifier);
    kinds[target::VCA_GATE] = Kind::Machine(Law::Edge);
    kinds[target::PHASER_RATE] = Kind::Rate;
    kinds[target::PHASER_CENTRE] = Kind::Cutoff;
    kinds[target::VCO2_PITCH] = Kind::Pitch;
    kinds[target::VCO1_WIDTH] = Kind::NarrowingWidth;
    kinds[target::VCO2_WIDTH] = Kind::NarrowingWidth;
    kinds[target::TREMOLO] = Kind::Machine(Law::OneSided(Sign::Negative));
    kinds[target::CUTOFF] = Kind::Cutoff;
    kinds[target::AMPLITUDE] = Kind::Amplitude;
    kinds
};

/// mxm-mono-00's routing declaration.
#[derive(Debug, Clone, Copy, Default)]
pub struct Declared;

/// Exactly one route, at `amount`.
fn one_route(target: usize, source: usize, amount: f32) -> Routing {
    let mut routing = Routing::new();
    routing.set(target, source, amount);
    routing
}

impl Declaration for Declared {
    fn sources(&self) -> usize {
        SOURCES
    }

    fn targets(&self) -> usize {
        TARGETS
    }

    fn performance(&self, source: usize) -> Option<Performance> {
        routing::PERFORMANCE[source]
    }

    fn kind(&self, target: usize) -> Kind {
        KINDS[target]
    }

    fn machine(&self, target: usize, source: usize) -> bool {
        routing::machine(target, source)
    }

    fn offered(&self, target: usize, source: usize) -> Offer {
        routing::offer(target, source)
    }

    fn key_unit(&self) -> f32 {
        KEY_UNIT_SEMITONES
    }

    /// One route alone through a real [`Graph`], in what the target makes of its sum: octaves of
    /// rate, cutoff or phaser, semitones of pitch, a width's narrowing, the tremolo's dip, the VCA
    /// level's and the gates' CV, the audio inputs' faded sum, Amplitude's factor less one — and the
    /// sync input's reset depth, which a crossing applies and a negative depth never does.
    fn deliver(&self, target: usize, source: usize, amount: f32, raw: f32) -> f32 {
        let routing = one_route(target, source, amount);
        let mut graph = Graph::new();
        graph.set_topology(&routing);
        graph.begin_sample();
        graph.write(source, raw);
        match target {
            target::VCO2_SYNC => {
                if raw == 0.0 {
                    0.0
                } else {
                    amount.max(0.0)
                }
            }
            t if routing::audio_slot(t).is_some() => graph.sum_audio(t, &routing, 48_000.0),
            target::VCO1_WIDTH | target::VCO2_WIDTH => {
                0.5 - narrowed_width(0.5, true, graph.sum(target, &routing))
            }
            target::TREMOLO => graph.sum(target, &routing).max(0.0),
            target::PHASER_RATE => graph.sum(target, &routing) * PHASER_LFO_OCTAVES_PER_UNIT,
            target::PHASER_CENTRE => graph.sum(target, &routing) * PHASER_MANUAL_OCTAVES_PER_UNIT,
            target::AMPLITUDE => standard::amplitude_factor(graph.sum(target, &routing)) - 1.0,
            _ => graph.sum(target, &routing),
        }
    }

    fn name(&self, target: usize, source: usize) -> String {
        format!("{} from {}", TARGET_NAMES[target], SOURCE_NAMES[source])
    }
}

#[cfg(test)]
mod tests {
    use mxm_modulation::conformance::{self, Case, Input};

    use super::*;
    use crate::routing::source;
    use crate::voice::{Activity, NoteId, Patch, Priority, Trigger, Voice};

    fn report(result: Result<(), Vec<String>>) {
        if let Err(failures) = result {
            panic!("{} failure(s):\n{}", failures.len(), failures.join("\n"));
        }
    }

    fn id(note: u8) -> NoteId {
        NoteId {
            voice_id: None,
            channel: 0,
            note,
        }
    }

    fn voice(routing: &Routing) -> Voice {
        let mut v = Voice::new();
        v.set_sample_rate(48_000.0);
        v.set_topology(routing);
        v
    }

    /// **Every pair means what the standard says**: offered as `standard::offer` says — a gesture
    /// refused at both gates, the sync input and the mixer's audio input, Velocity at the S&H; the
    /// VCA level keeping Velocity's closing half, the widths and the tremolo a one-signed source's
    /// live half — nothing at a source's rest, a meaningful move at full, and the standard reach for
    /// every performance pair the machine did not have.
    ///
    /// Falsified before trusted: with no pair taking the standard reach, it names both halves of
    /// every added pitch and LFO-rate pair; with the cutoff's added pairs left at twelve octaves,
    /// those.
    #[test]
    fn every_pair_means_what_the_standard_says() {
        report(conformance::check_declaration(&Declared));
    }

    /// **A voice publishes what the standard says**, in raw units: Key in ten-volt units from middle
    /// C, Velocity as `v − 1`, the wheel, pressure and lever as they arrive.
    ///
    /// Falsified before trusted: publishing the raw velocity fails at every input.
    #[test]
    fn a_voice_publishes_what_the_standard_says() {
        report(conformance::check_publishers(&Declared, |from, input| {
            let mut routing = Routing::init();
            routing.set(target::CUTOFF, from, 0.0);
            let mut v = voice(&routing);
            let mut p = Patch::default();
            let (note, velocity) = match input {
                Input::Note(n) => (n, 1.0),
                Input::Normalised(value) if from == source::VELOCITY => (60, value),
                _ => (60, 1.0),
            };
            match input {
                Input::Normalised(value) if from == source::WHEEL => p.wheel = value,
                Input::Normalised(value) if from == source::PRESSURE => p.pressure = value,
                Input::Lever(value) => p.bend_position = value,
                _ => {}
            }
            v.note_on(id(note), velocity, &p);
            v.process(&p, &routing);
            v.published_for_test(from)
        }));
    }

    /// **Velocity is the press that last triggered an envelope**: under GATE+TRIG a legato press
    /// retriggers and brings its own; under GATE it raises no gate and keeps the phrase's.
    ///
    /// Falsified before trusted: publishing the sounding press's velocity reads the legato press's
    /// under GATE.
    #[test]
    fn velocity_is_the_press_that_last_triggered_an_envelope() {
        // Low-note priority too: a higher press does not take the bus, but under GATE+TRIG it still
        // retriggers, so it is the press that last triggered, whatever sounds.
        for (priority, trigger, expected) in [
            (Priority::Last, Trigger::GateTrig, -0.125),
            (Priority::Last, Trigger::Gate, -0.75),
            (Priority::Low, Trigger::GateTrig, -0.125),
            (Priority::Low, Trigger::Gate, -0.75),
        ] {
            let mut routing = Routing::init();
            routing.set(target::CUTOFF, source::VELOCITY, 0.0);
            let mut v = voice(&routing);
            let p = Patch {
                priority,
                vcf_trigger: trigger,
                vca_trigger: trigger,
                ..Patch::default()
            };
            v.note_on(id(60), 0.25, &p);
            v.process(&p, &routing);
            assert_eq!(v.published_for_test(source::VELOCITY), -0.75);
            v.note_on(id(72), 0.875, &p);
            v.process(&p, &routing);
            assert_eq!(
                v.published_for_test(source::VELOCITY),
                expected,
                "{priority:?}, {trigger:?}"
            );
        }
    }

    /// **Two presses in one sample**: the first raises the gate and the second lands with it. Under
    /// GATE the gate's edge is the first press's, so Velocity is its, not the later one's; under
    /// GATE+TRIG the later press retriggers last, so it is that one's.
    ///
    /// Falsified before trusted: crediting a GATE edge to the latest press reads the later `−0.125`.
    #[test]
    fn two_presses_in_one_sample_credit_the_press_that_triggered() {
        for (trigger, expected) in [(Trigger::Gate, -0.75), (Trigger::GateTrig, -0.125)] {
            let mut routing = Routing::init();
            routing.set(target::CUTOFF, source::VELOCITY, 0.0);
            let mut v = voice(&routing);
            let p = Patch {
                priority: Priority::Low,
                vcf_trigger: trigger,
                vca_trigger: trigger,
                ..Patch::default()
            };
            v.note_on(id(48), 0.25, &p);
            v.note_on(id(72), 0.875, &p);
            v.process(&p, &routing);
            assert_eq!(
                v.published_for_test(source::VELOCITY),
                expected,
                "{trigger:?}"
            );
        }
    }

    /// **A gate edge the keyboard only rode on is not the press's.** Under last-note priority, with
    /// the keyboard gate at a depth too shallow to cross the threshold alone (0.1) and an LFO that
    /// crosses by itself on the VCA envelope's gate, two presses land exactly as the LFO rises: the
    /// LFO made the edge, so Velocity is the sounding (second) press's, not the first's.
    ///
    /// Falsified before trusted: crediting every first-press edge reads the first press's `−0.75`.
    #[test]
    fn a_gate_edge_the_keyboard_only_rode_on_is_not_the_presss() {
        use crate::lfo::Shape;
        let mut routing = Routing::init();
        routing.set(target::CUTOFF, source::VELOCITY, 0.0);
        routing.clear_target(target::VCF_GATE);
        routing.clear_target(target::VCA_GATE);
        routing.set(target::VCA_GATE, source::GATE, 0.1);
        routing.set(target::VCA_GATE, source::LFO1, 1.0);
        let p = Patch {
            priority: Priority::Last,
            vcf_trigger: Trigger::Gate,
            vca_trigger: Trigger::Gate,
            lfo1_shape: Shape::Square,
            lfo1_rate_hz: 20.0,
            ..Patch::default()
        };
        let rise = lfo1_rise(&p, &routing);

        let mut v = voice(&routing);
        for _ in 0..rise {
            v.process(&p, &routing);
        }
        v.note_on(id(48), 0.25, &p);
        v.note_on(id(60), 0.875, &p);
        v.process(&p, &routing);
        assert_eq!(
            v.published_for_test(source::VELOCITY),
            -0.125,
            "the LFO's edge: the sounding press's velocity"
        );
    }

    /// **A GATE+TRIG retrigger through a shallow route is the press's, whatever raised the gate.**
    /// With the keyboard gate at 0.1 beside an LFO on the VCA envelope's gate, under low-note
    /// priority a soft low press and then a hard high one — a tie, which retriggers GATE+TRIG and
    /// takes no bus — land while the LFO holds the gate high: the tie retriggers, so Velocity is
    /// its. The same pair landing exactly as the LFO rises reads the same, since the tie would have
    /// retriggered either way.
    ///
    /// Falsified before trusted: requiring the keyboard alone to cross for a GATE+TRIG retrigger
    /// reads the sounding press's `−0.75` on the rise.
    #[test]
    fn a_gate_trig_press_through_a_shallow_route_is_the_presss_whatever_raised_the_gate() {
        use crate::lfo::Shape;
        let mut routing = Routing::init();
        routing.set(target::CUTOFF, source::VELOCITY, 0.0);
        routing.clear_target(target::VCF_GATE);
        routing.clear_target(target::VCA_GATE);
        routing.set(target::VCA_GATE, source::GATE, 0.1);
        routing.set(target::VCA_GATE, source::LFO1, 1.0);
        let p = Patch {
            priority: Priority::Low,
            vcf_trigger: Trigger::GateTrig,
            vca_trigger: Trigger::GateTrig,
            lfo1_shape: Shape::Square,
            lfo1_rate_hz: 20.0,
            ..Patch::default()
        };
        let rise = lfo1_rise(&p, &routing);
        for (after_the_rise, landing) in [(0, "on the rise"), (1, "a sample after it")] {
            let mut v = voice(&routing);
            for _ in 0..rise + after_the_rise {
                v.process(&p, &routing);
            }
            v.note_on(id(48), 0.25, &p);
            v.note_on(id(72), 0.875, &p);
            v.process(&p, &routing);
            assert_eq!(
                v.published_for_test(source::VELOCITY),
                -0.125,
                "{landing}: the tie retriggered"
            );
        }
    }

    /// The first sample after the first on which LFO-1's square rises, under `p` and `routing`.
    fn lfo1_rise(p: &Patch, routing: &Routing) -> usize {
        let mut probe = voice(routing);
        let mut was_high = true;
        for n in 0..48_000 {
            probe.process(p, routing);
            let high = probe.published_for_test(source::LFO1) > 0.5;
            if n > 0 && high && !was_high {
                return n;
            }
            was_high = high;
        }
        panic!("the premise: the LFO rises");
    }

    /// **A trigger no press made moves nothing**: an LFO on the VCA envelope's gate fires it with no
    /// key, and Velocity rests — before any press, and after All Sound Off has forgotten one.
    ///
    /// Falsified before trusted: with All Sound Off keeping the press, the LFO's triggers after it
    /// publish that press's `−0.75`.
    #[test]
    fn a_trigger_no_press_made_leaves_velocity_at_rest() {
        let mut routing = Routing::init();
        routing.set(target::CUTOFF, source::VELOCITY, 0.0);
        routing.set(target::VCA_GATE, source::LFO1, 1.0);
        let p = Patch {
            lfo1_rate_hz: 20.0,
            lfo1_shape: crate::lfo::Shape::Square,
            ..Patch::default()
        };
        let mut v = voice(&routing);
        for _ in 0..24_000 {
            v.process(&p, &routing);
            assert_eq!(v.published_for_test(source::VELOCITY), 0.0, "no press yet");
        }
        v.note_on(id(60), 0.25, &p);
        v.process(&p, &routing);
        v.note_off(None, 0, 60, &p);
        v.all_sound_off();
        for _ in 0..24_000 {
            v.process(&p, &routing);
        }
        assert_eq!(
            v.published_for_test(source::VELOCITY),
            0.0,
            "All Sound Off forgot the press"
        );
    }

    /// **The Amplitude target is the standard factor after the VCA**: at −100 % from a wheel at full
    /// it silences the voice exactly, and at +100 % doubles it — past the VCA's own `0…1` gain.
    ///
    /// Falsified before trusted: with the voice's factor removed, the doubled render equals the
    /// plain one.
    #[test]
    fn amplitude_is_the_standard_factor_after_the_vca() {
        let render = |amount: Option<f32>| {
            let mut routing = Routing::init();
            if let Some(amount) = amount {
                routing.set(target::AMPLITUDE, source::WHEEL, amount);
            }
            let mut v = voice(&routing);
            let p = Patch {
                wheel: 1.0,
                ..Patch::default()
            };
            v.note_on(id(60), 1.0, &p);
            (0..4_800)
                .map(|_| v.process(&p, &routing))
                .collect::<Vec<_>>()
        };
        let plain = render(None);
        assert!(
            plain.iter().any(|s| s.abs() > 1e-4),
            "the premise: it sounds"
        );
        assert!(render(Some(-1.0)).iter().all(|&s| s == 0.0), "silence");
        for (doubled, plain) in render(Some(1.0)).iter().zip(&plain) {
            assert_eq!(*doubled, plain * 2.0);
        }
    }

    /// **Every source's Amplitude route is the standard swing at its own peak**: half an amount with
    /// the source at its peak moves the factor by exactly half, below the clamp, so the reading and
    /// the delivery agree for every source, the envelopes' 0.6 and the VCOs' 0.5 included.
    ///
    /// Falsified before trusted: with a unit column, an envelope at half depth moves it by 0.3.
    #[test]
    fn every_amplitude_route_reaches_the_standard_swing_at_its_sources_peak() {
        for s in (0..SOURCES).filter(|&s| s != source::KEY) {
            let peak = routing::SOURCE_PEAK[s];
            for amount in [0.5_f32, -0.5] {
                let moved = Declared.deliver(target::AMPLITUDE, s, amount, peak);
                assert!(
                    (moved - amount).abs() < 1e-6,
                    "{}: at {amount:+} and its peak {peak} it moves the factor by {moved}",
                    SOURCE_NAMES[s]
                );
            }
        }
    }

    /// **A static exact mute on the Amplitude is not `Live`** — a drone held open by INITIAL GAIN
    /// behind `Amplitude ← Wheel −100 %` with the wheel at full is silent until a host event, so it
    /// parks as Volume at zero does — **but a moving route can reopen it and stays `Live`**, and
    /// **an effect's tail still rings out** behind a mute rather than being cut.
    ///
    /// Falsified before trusted: without the mute clause the muted drone stays `Live` for ever.
    #[test]
    fn a_static_amplitude_mute_parks_and_a_moving_one_does_not() {
        let drone = |amplitude_source: usize, delay: bool| {
            let mut routing = Routing::init();
            routing.set(target::AMPLITUDE, amplitude_source, -1.0);
            let mut v = voice(&routing);
            let p = Patch {
                initial_gain: 1.0,
                level_vco1: 1.0,
                wheel: 1.0,
                delay_level: if delay { 1.0 } else { 0.0 },
                ..Patch::default()
            };
            (v.process(&p, &routing), v, p, routing)
        };

        let (_, mut v, p, routing) = drone(source::WHEEL, false);
        let parked = (0..96_000)
            .any(|_| v.process(&p, &routing) == 0.0 && v.activity(&p, &routing) == Activity::Inert);
        assert!(parked, "a static exact mute parks like Volume at zero");

        let (_, mut v, p, routing) = drone(source::LFO1, false);
        for _ in 0..96_000 {
            v.process(&p, &routing);
            assert_eq!(
                v.activity(&p, &routing),
                Activity::Live,
                "an LFO can reopen it"
            );
        }

        // A tail first: the drone unmuted into the delay, then muted by the wheel. It rings, and
        // then it parks.
        let (_, mut v, mut p, routing) = drone(source::WHEEL, true);
        p.wheel = 0.0;
        for _ in 0..24_000 {
            v.process(&p, &routing);
        }
        p.wheel = 1.0;
        v.process(&p, &routing);
        assert_eq!(
            v.activity(&p, &routing),
            Activity::Tailing,
            "the delay still holds what the mute let through"
        );
        let drained = (0..48_000 * 30)
            .any(|_| v.process(&p, &routing) == 0.0 && v.activity(&p, &routing) == Activity::Inert);
        assert!(drained, "the delay's tail drains and the voice parks");

        // **A key held behind the mute is no tail either**: `Amplitude ← Gate −100 %` closes the
        // factor exactly while the key is down, and the envelope sustaining behind it is inaudible.
        let mut routing = Routing::init();
        routing.set(target::AMPLITUDE, source::GATE, -1.0);
        let mut v = voice(&routing);
        let p = Patch {
            initial_gain: 1.0,
            level_vco1: 1.0,
            ..Patch::default()
        };
        v.note_on(id(60), 1.0, &p);
        let parked = (0..96_000)
            .any(|_| v.process(&p, &routing) == 0.0 && v.activity(&p, &routing) == Activity::Inert);
        assert!(parked, "a held key behind a static mute parks");
    }

    /// **A mute through Velocity parks only while nothing but an event can lift it.** A drone muted
    /// by `Amplitude ← Velocity +100 %` after a zero-velocity press, the press itself sounding,
    /// parks: nothing can re-latch Velocity to anything else. **But a soft GATE+TRIG tie over a hard
    /// sounding note under low-note priority**, muted the same way, **stays awake while an LFO can
    /// fire the other envelope** — its trigger re-latches the sounding note's velocity and lifts the
    /// mute, which it does.
    ///
    /// Falsified before trusted: counting Velocity as event-only parks the second before the LFO
    /// lifts it; never counting it keeps the first awake for ever.
    #[test]
    fn a_mute_through_velocity_parks_only_when_nothing_can_lift_it() {
        let parks = |v: &mut Voice, p: &Patch, r: &Routing| {
            (0..96_000).any(|_| v.process(p, r) == 0.0 && v.activity(p, r) == Activity::Inert)
        };

        let mut routing = Routing::init();
        routing.set(target::AMPLITUDE, source::VELOCITY, 1.0);
        let mut v = voice(&routing);
        let p = Patch {
            initial_gain: 1.0,
            level_vco1: 1.0,
            vcf_trigger: Trigger::GateTrig,
            vca_trigger: Trigger::GateTrig,
            ..Patch::default()
        };
        v.note_on(id(60), 0.0, &p);
        v.process(&p, &routing);
        assert_eq!(
            v.published_for_test(source::VELOCITY),
            -1.0,
            "the premise: muted"
        );
        assert!(
            parks(&mut v, &p, &routing),
            "nothing can lift it, so it parks"
        );

        let mut routing = Routing::init();
        routing.set(target::AMPLITUDE, source::VELOCITY, 1.0);
        routing.clear_target(target::VCA_GATE);
        routing.set(target::VCA_GATE, source::LFO1, 1.0);
        let mut v = voice(&routing);
        let p = Patch {
            priority: Priority::Low,
            initial_gain: 1.0,
            level_vco1: 1.0,
            vcf_trigger: Trigger::GateTrig,
            vca_trigger: Trigger::Gate,
            lfo1_shape: crate::lfo::Shape::Square,
            lfo1_rate_hz: 1.0,
            ..Patch::default()
        };
        v.note_on(id(48), 1.0, &p);
        v.process(&p, &routing);
        v.note_on(id(72), 0.0, &p);
        v.process(&p, &routing);
        assert_eq!(
            v.published_for_test(source::VELOCITY),
            -1.0,
            "the premise: the tie muted it"
        );
        let mut lifted = false;
        for _ in 0..96_000 {
            v.process(&p, &routing);
            if v.published_for_test(source::VELOCITY) == 0.0 {
                lifted = true;
                break;
            }
            assert_eq!(
                v.activity(&p, &routing),
                Activity::Live,
                "awake until the LFO lifts it"
            );
        }
        assert!(
            lifted,
            "the LFO's trigger re-latched the sounding note's velocity"
        );
    }

    /// **A mute through the keyboard CV parks once it has settled, and not while it glides.** Key at
    /// MIDI 0 is exactly `−0.5` of a ten-volt unit, and at `+100 %` into the Amplitude (two a unit,
    /// 20 %/oct) closes the factor exactly: settled, only an event can move it, so a drone muted so
    /// parks. **Gliding, it can reopen by itself**: with the wheel at `−100 %` beside it the factor is
    /// clamped shut while Key is below middle C, and a portamento climbing from note 0 to 127 lifts it
    /// — so the voice must stay awake through the climb, and it does sound again.
    ///
    /// Falsified before trusted: without the keyboard CV classified, the settled mute stays awake
    /// for ever; counting it as static while it glides parks the climb, which never reopens.
    #[test]
    fn a_mute_through_the_key_parks_when_settled_and_not_while_it_glides() {
        let mut routing = Routing::init();
        routing.set(target::AMPLITUDE, source::KEY, 1.0);
        let drone = Patch {
            initial_gain: 1.0,
            level_vco1: 1.0,
            ..Patch::default()
        };
        let mut v = voice(&routing);
        v.note_on(id(0), 1.0, &drone);
        let parked = (0..96_000).any(|_| {
            v.process(&drone, &routing) == 0.0 && v.activity(&drone, &routing) == Activity::Inert
        });
        assert!(parked, "settled at note 0, only an event can lift it");

        routing.set(target::AMPLITUDE, source::WHEEL, -1.0);
        let mut v = voice(&routing);
        let mut p = Patch {
            wheel: 1.0,
            ..drone
        };
        v.note_on(id(0), 1.0, &p);
        for _ in 0..4_800 {
            v.process(&p, &routing);
        }
        v.note_off(None, 0, 0, &p);
        p.portamento_s = 0.5;
        v.note_on(id(127), 1.0, &p);
        let mut sounded = false;
        for _ in 0..(48_000 * 4) {
            if v.process(&p, &routing) != 0.0 {
                sounded = true;
                break;
            }
            assert_ne!(
                v.activity(&p, &routing),
                Activity::Inert,
                "muted by the clamp while it climbs, but awake"
            );
        }
        assert!(sounded, "the climb past middle C lifts the mute");

        // **Landed to the bit is settled**, whatever distance the lag still carries: a climb from
        // note 0 to middle C, clamped shut all the way, parks once the published Key is exactly zero
        // — about nine seconds at 0.5 s — not when the remaining distance flushes, sixteen later.
        let mut v = voice(&routing);
        let mut p = Patch {
            wheel: 1.0,
            ..drone
        };
        v.note_on(id(0), 1.0, &p);
        for _ in 0..4_800 {
            v.process(&p, &routing);
        }
        v.note_off(None, 0, 0, &p);
        p.portamento_s = 0.5;
        v.note_on(id(60), 1.0, &p);
        let parked_at = (0..(48_000 * 30)).find(|_| {
            v.process(&p, &routing);
            v.activity(&p, &routing) == Activity::Inert
        });
        assert_eq!(
            v.published_for_test(source::KEY),
            0.0,
            "only once it has landed"
        );
        assert!(
            parked_at.is_some_and(|n| n < 48_000 * 12),
            "parked at {parked_at:?}: once landed to the bit, not when the distance flushes"
        );
    }

    /// **After a release, no performance route holds a note open** — every offered pair, on its
    /// offered half, at the softest and hardest notes and the keyboard's ends, gestures held at full
    /// through the note and let go at the release. The keyboard CV into the VCA level and into the
    /// VCA envelope's gate is the patch bay's own, holding the amplifier open as that patch does;
    /// both are declared drones.
    ///
    /// Falsified before trusted: without the drones declared, it names both.
    #[test]
    fn after_a_release_no_performance_route_holds_a_note_open() {
        let drones = [
            (target::AMPLIFIER, source::KEY),
            (target::VCA_GATE, source::KEY),
        ];
        report(conformance::check_release_silence(
            &Declared,
            &drones,
            |case: Case| {
                let mut routing = Routing::init();
                routing.set(case.target, case.source, case.amount);
                let mut v = voice(&routing);
                let mut p = Patch {
                    wheel: 1.0,
                    pressure: 1.0,
                    bend_position: 1.0,
                    ..Patch::default()
                };
                p.vcf_adsr[3] = 0.02;
                p.vca_adsr[3] = 0.02;
                v.note_on(id(case.key), case.velocity, &p);
                for _ in 0..4_800 {
                    v.process(&p, &routing);
                }
                v.note_off(None, 0, case.key, &p);
                p.wheel = 0.0;
                p.pressure = 0.0;
                p.bend_position = 0.0;
                (0..96_000).any(|_| {
                    v.process(&p, &routing) == 0.0 && v.activity(&p, &routing) == Activity::Inert
                })
            },
        ));
    }
}
