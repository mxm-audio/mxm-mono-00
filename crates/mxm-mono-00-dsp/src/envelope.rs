//! Analog-style ADSR envelope — the System-100's discrete Roland ADSR, as far as it is documented.
//!
//! **A copy of `crates/mxm-poly-06-dsp/src/envelope.rs`, deliberately whole**, stall fix
//! included: the fourth honest copy the collection's extraction rule asks for. What
//! `research:instruments/system-100.md` §9 establishes about the machine's envelope this model already
//! has by construction — a charging attack that hands over at its peak, decay and release as the
//! same exponential (one capacitor, two discharge resistors), release allowed during the attack —
//! and what it does not establish (the exact curves, the unijunction's threshold) stays
//! unverified.
//!
//! **There are two of these per instrument**, VCF ADSR and VCA ADSR, each with its own trigger
//! mode. The modes live in `voice.rs`, because two of the three watch things the envelope cannot
//! see: the gate, and LFO-1's square.
//!
//! The A/D/R times are **not** smoothed. They set segment coefficients rather than being signals.

use crate::flush;

/// Level below which the envelope is considered finished and snaps to exactly zero.
pub const ZERO_THRESHOLD: f32 = 1e-4; // -80 dB

/// How far past 1.0 the attack aims. Never reached; it bends the attack into a charging curve.
const ATTACK_OVERSHOOT: f32 = 0.2;

/// `ln((1 + overshoot) / overshoot)`: time constants for the attack to cross 1.0, so the attack
/// time means "time to reach full level".
const ATTACK_TAUS: f32 = 1.791_759_5;

/// Time constants for a decay or release to get within 1 % of its target.
const DECAY_TAUS: f32 = 4.605_170_2;

/// The hardware's shortest attack is 0.4 ms (§3.1). Shorter than a millisecond is a click, which
/// the manual invites you to hear at both controls at zero.
const MIN_TIME_S: f32 = 0.0004;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Stage {
    Idle,
    Attack,
    Decay,
    Sustain,
    Release,
}

#[derive(Debug, Clone)]
pub struct Adsr {
    stage: Stage,
    level: f32,
    sample_rate: f32,
    attack_time: f32,
    decay_time: f32,
    release_time: f32,
    attack_coef: f32,
    decay_coef: f32,
    release_coef: f32,
}

impl Default for Adsr {
    fn default() -> Self {
        Self::new()
    }
}

impl Adsr {
    pub fn new() -> Self {
        Self {
            stage: Stage::Idle,
            level: 0.0,
            sample_rate: 48_000.0,
            attack_time: -1.0,
            decay_time: -1.0,
            release_time: -1.0,
            attack_coef: 0.0,
            decay_coef: 0.0,
            release_coef: 0.0,
        }
    }

    pub fn set_sample_rate(&mut self, sample_rate: f32) {
        self.sample_rate = sample_rate;
        self.attack_time = -1.0;
        self.decay_time = -1.0;
        self.release_time = -1.0;
    }

    pub fn reset(&mut self) {
        self.stage = Stage::Idle;
        self.level = 0.0;
    }

    pub fn stage(&self) -> Stage {
        self.stage
    }

    pub fn level(&self) -> f32 {
        self.level
    }

    pub fn is_active(&self) -> bool {
        self.stage != Stage::Idle
    }

    /// Start (or restart) the attack from wherever the level is, which is what an analogue
    /// envelope does and what keeps a retrigger click-free.
    pub fn trigger(&mut self) {
        self.stage = Stage::Attack;
    }

    /// Begin the release — from any stage, the attack included (§9: "even if it comes before the
    /// attack time has reached maximum").
    pub fn release(&mut self) {
        if self.stage != Stage::Idle {
            self.stage = Stage::Release;
        }
    }

    /// Cut immediately, as an all-sound-off or choke requires.
    pub fn silence(&mut self) {
        self.stage = Stage::Idle;
        self.level = 0.0;
    }

    fn coef(time_s: f32, taus: f32, sample_rate: f32) -> f32 {
        let t = time_s.max(MIN_TIME_S);
        (-taus / (t * sample_rate)).exp()
    }

    fn update_coefficients(&mut self, attack: f32, decay: f32, release: f32) {
        if attack != self.attack_time {
            self.attack_time = attack;
            self.attack_coef = Self::coef(attack, ATTACK_TAUS, self.sample_rate);
        }
        if decay != self.decay_time {
            self.decay_time = decay;
            self.decay_coef = Self::coef(decay, DECAY_TAUS, self.sample_rate);
        }
        if release != self.release_time {
            self.release_time = release;
            self.release_coef = Self::coef(release, DECAY_TAUS, self.sample_rate);
        }
    }

    /// Advance one sample. Times in seconds, `sustain` in `0..=1`; the level is `0..=1`, the
    /// hardware's +6 V peak normalised.
    #[inline]
    pub fn process(&mut self, attack: f32, decay: f32, sustain: f32, release: f32) -> f32 {
        self.update_coefficients(attack, decay, release);
        let sustain = sustain.clamp(0.0, 1.0);

        match self.stage {
            Stage::Idle => return 0.0,
            Stage::Attack => {
                let target = 1.0 + ATTACK_OVERSHOOT;
                self.level = target + (self.level - target) * self.attack_coef;
                if self.level >= 1.0 {
                    self.level = 1.0;
                    self.stage = Stage::Decay;
                }
            }
            Stage::Decay => {
                let before = self.level;
                self.level = sustain + (self.level - sustain) * self.decay_coef;
                // Or the step has stopped moving it — the `f32` stall `mxm-poly-06-dsp` found.
                if (self.level - sustain).abs() <= ZERO_THRESHOLD || self.level == before {
                    self.level = sustain;
                    self.stage = Stage::Sustain;
                }
            }
            Stage::Sustain => {
                self.level = sustain + (self.level - sustain) * self.decay_coef;
            }
            Stage::Release => {
                self.level *= self.release_coef;
                if self.level <= ZERO_THRESHOLD {
                    self.level = 0.0;
                    self.stage = Stage::Idle;
                }
            }
        }

        self.level = flush(self.level);
        self.level
    }

    /// Samples remaining before the envelope reaches [`ZERO_THRESHOLD`]; 0 when idle. While held
    /// it reports the full release from the current level, the conservative estimate.
    pub fn tail_samples(&self, release: f32) -> u32 {
        if self.stage == Stage::Idle || self.level <= ZERO_THRESHOLD {
            return 0;
        }
        let coef = Self::coef(release, DECAY_TAUS, self.sample_rate);
        if coef <= 0.0 || coef >= 1.0 {
            return 0;
        }
        let n = (ZERO_THRESHOLD / self.level).ln() / coef.ln();
        n.max(0.0).ceil() as u32
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const FS: f32 = 48_000.0;

    fn run(env: &mut Adsr, secs: f32, a: f32, d: f32, s: f32, r: f32) -> f32 {
        let n = (FS * secs) as usize;
        let mut last = 0.0;
        for _ in 0..n {
            last = env.process(a, d, s, r);
        }
        last
    }

    #[test]
    fn idle_envelope_is_exactly_zero() {
        let mut env = Adsr::new();
        env.set_sample_rate(FS);
        for _ in 0..1_000 {
            assert_eq!(env.process(0.01, 0.1, 0.7, 0.1), 0.0);
        }
    }

    #[test]
    fn attack_reaches_full_level_in_about_the_attack_time() {
        for attack in [0.005f32, 0.05, 0.5] {
            let mut env = Adsr::new();
            env.set_sample_rate(FS);
            env.trigger();
            let mut samples = 0usize;
            while env.stage() == Stage::Attack && samples < (FS * 5.0) as usize {
                env.process(attack, 0.3, 0.7, 0.2);
                samples += 1;
            }
            let measured = samples as f32 / FS;
            let err = (measured - attack).abs() / attack;
            assert!(err < 0.1, "attack {attack}s measured {measured}s");
        }
    }

    #[test]
    fn release_can_begin_during_the_attack() {
        // §9: "even if it comes before the attack time has reached maximum".
        let mut env = Adsr::new();
        env.set_sample_rate(FS);
        env.trigger();
        run(&mut env, 0.01, 1.0, 0.3, 0.7, 0.1);
        assert_eq!(env.stage(), Stage::Attack);
        env.release();
        assert_eq!(env.stage(), Stage::Release);
        let level = run(&mut env, 2.0, 1.0, 0.3, 0.7, 0.1);
        assert_eq!(level, 0.0);
    }

    #[test]
    fn release_returns_to_exactly_zero_and_goes_idle() {
        let mut env = Adsr::new();
        env.set_sample_rate(FS);
        env.trigger();
        run(&mut env, 1.0, 0.005, 0.1, 0.7, 0.1);
        env.release();
        let level = run(&mut env, 2.0, 0.005, 0.1, 0.7, 0.1);
        assert_eq!(level, 0.0);
        assert_eq!(env.stage(), Stage::Idle);
    }

    #[test]
    fn the_decay_does_not_stall_above_a_high_sustain() {
        // The f32 stall the copy carries the fix for: 0.4 s into 0.8 must reach Sustain.
        let mut env = Adsr::new();
        env.set_sample_rate(FS);
        env.trigger();
        run(&mut env, 3.0, 0.001, 0.4, 0.8, 0.1);
        assert_eq!(env.stage(), Stage::Sustain);
    }

    #[test]
    fn tail_estimate_matches_the_actual_release_length() {
        for release in [0.05f32, 0.2, 1.0] {
            let mut env = Adsr::new();
            env.set_sample_rate(FS);
            env.trigger();
            run(&mut env, 1.0, 0.005, 0.1, 0.8, release);
            env.release();
            let predicted = env.tail_samples(release);
            let mut actual = 0u32;
            while env.is_active() && actual < (FS * 10.0) as u32 {
                env.process(0.005, 0.1, 0.8, release);
                actual += 1;
            }
            let err = (predicted as f32 - actual as f32).abs() / actual as f32;
            assert!(
                err < 0.05,
                "release {release}s: predicted {predicted}, actual {actual}"
            );
        }
    }

    #[test]
    fn stays_finite_at_every_sample_rate_and_extreme_time() {
        for fs in [44_100.0f32, 48_000.0, 96_000.0, 192_000.0] {
            let mut env = Adsr::new();
            env.set_sample_rate(fs);
            for &t in &[0.0f32, 0.0001, 1.0, 12.0, 1e6] {
                env.trigger();
                for _ in 0..10_000 {
                    let v = env.process(t, t, 0.5, t);
                    assert!(v.is_finite() && (0.0..=1.0).contains(&v), "level {v}");
                }
                env.release();
                for _ in 0..10_000 {
                    let v = env.process(t, t, 0.5, t);
                    assert!(v.is_finite() && (0.0..=1.0).contains(&v), "level {v}");
                }
            }
        }
    }
}
