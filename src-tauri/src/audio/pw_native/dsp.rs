//! Native mic DSP chain: noise gate → gain → compressor →
//! limiter. Pure Rust, no LV2/LADSPA. Runs per-sample inside the mic
//! capture stream's process callback (mono).
//!
//! All stages use one-pole envelope followers with attack/release smoothing
//! so gain changes never click.

/// Tunable parameters, updated from the UI thread via atomics in `mic.rs`.
#[derive(Debug, Clone, Copy)]
pub struct DspSettings {
    pub gate_enabled: bool,
    pub comp_enabled: bool,
    pub limiter_enabled: bool,
    /// Linear gain multiplier (UI percent / 100).
    pub gain: f32,
    pub muted: bool,
    /// Tunable stage parameters (UI-exposed; time constants stay fixed).
    pub gate_threshold_db: f32,
    pub comp_threshold_db: f32,
    pub comp_ratio: f32,
    pub limiter_ceiling_db: f32,
}

impl Default for DspSettings {
    fn default() -> Self {
        Self {
            gate_enabled: true,
            comp_enabled: true,
            limiter_enabled: true,
            gain: 1.0,
            muted: false,
            gate_threshold_db: -40.0,
            comp_threshold_db: -18.0,
            comp_ratio: 3.0,
            limiter_ceiling_db: -1.0,
        }
    }
}

// Fixed time constants (voice-chain guidance - OBS-style starting
// points). Thresholds/ratio/ceiling are user-tunable via DspSettings.
const GATE_ATTACK_MS: f32 = 5.0;
const GATE_RELEASE_MS: f32 = 150.0;
const GATE_HOLD_MS: f32 = 200.0;

const COMP_ATTACK_MS: f32 = 6.0;
const COMP_RELEASE_MS: f32 = 60.0;
const COMP_MAKEUP_DB: f32 = 4.0;

const LIMIT_RELEASE_MS: f32 = 60.0;

fn db_to_linear(db: f32) -> f32 {
    10f32.powf(db / 20.0)
}

/// One-pole smoothing coefficient for a time constant in ms.
fn coeff(ms: f32, sample_rate: f32) -> f32 {
    if ms <= 0.0 {
        return 0.0;
    }
    (-1.0 / (ms * 0.001 * sample_rate)).exp()
}

pub struct DspChain {
    sample_rate: f32,
    // gate
    gate_env: f32,
    gate_gain: f32,
    gate_hold: u32,
    // compressor
    comp_env: f32,
    // limiter
    limit_gain: f32,
}

impl DspChain {
    pub fn new(sample_rate: f32) -> Self {
        Self {
            sample_rate,
            gate_env: 0.0,
            gate_gain: 0.0,
            gate_hold: 0,
            comp_env: 0.0,
            limit_gain: 1.0,
        }
    }

    /// Process a mono buffer in place.
    pub fn process(&mut self, samples: &mut [f32], s: &DspSettings) {
        if s.muted {
            samples.fill(0.0);
            return;
        }

        let sr = self.sample_rate;
        let gate_thresh = db_to_linear(s.gate_threshold_db);
        let gate_att = coeff(GATE_ATTACK_MS, sr);
        let gate_rel = coeff(GATE_RELEASE_MS, sr);
        let hold_samples = (GATE_HOLD_MS * 0.001 * sr) as u32;

        let comp_thresh_db = s.comp_threshold_db;
        let comp_att = coeff(COMP_ATTACK_MS, sr);
        let comp_rel = coeff(COMP_RELEASE_MS, sr);
        let makeup = db_to_linear(COMP_MAKEUP_DB);

        let ceiling = db_to_linear(s.limiter_ceiling_db);
        let limit_rel = coeff(LIMIT_RELEASE_MS, sr);

        for sample in samples.iter_mut() {
            let mut x = *sample;

            // ---- noise gate ----
            if s.gate_enabled {
                let mag = x.abs();
                // envelope follower (fast attack, slower release)
                self.gate_env = if mag > self.gate_env {
                    mag + gate_att * (self.gate_env - mag)
                } else {
                    mag + gate_rel * (self.gate_env - mag)
                };
                let open = self.gate_env > gate_thresh;
                if open {
                    self.gate_hold = hold_samples;
                } else if self.gate_hold > 0 {
                    self.gate_hold -= 1;
                }
                let target = if open || self.gate_hold > 0 { 1.0 } else { 0.0 };
                let c = if target > self.gate_gain {
                    gate_att
                } else {
                    gate_rel
                };
                self.gate_gain = target + c * (self.gate_gain - target);
                x *= self.gate_gain;
            }

            // ---- gain ----
            x *= s.gain;

            // ---- compressor (downward, feed-forward) ----
            if s.comp_enabled {
                let mag = x.abs().max(1e-9);
                self.comp_env = if mag > self.comp_env {
                    mag + comp_att * (self.comp_env - mag)
                } else {
                    mag + comp_rel * (self.comp_env - mag)
                };
                let env_db = 20.0 * self.comp_env.log10();
                let over = env_db - comp_thresh_db;
                if over > 0.0 {
                    let reduction_db = over * (1.0 - 1.0 / s.comp_ratio.max(1.0));
                    x *= db_to_linear(-reduction_db);
                }
                x *= makeup;
            }

            // ---- limiter (hard knee, instant attack, smooth release) ----
            if s.limiter_enabled {
                let mag = x.abs();
                let needed = if mag * self.limit_gain > ceiling {
                    ceiling / mag
                } else {
                    1.0
                };
                if needed < self.limit_gain {
                    self.limit_gain = needed; // clamp instantly
                } else {
                    self.limit_gain = needed + limit_rel * (self.limit_gain - needed);
                }
                x *= self.limit_gain;
                x = x.clamp(-ceiling, ceiling);
            }

            *sample = x;
        }
    }
}
