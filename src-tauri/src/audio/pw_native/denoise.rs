//! Microphone noise suppression: RNNoise (via `nnnoiseless`, a pure-Rust port
//! of Xiph's RNNoise), applied one 10 ms frame at a time by
//! [`super::clean::CleanStage`], which handles the re-blocking. `strength`
//! blends the original with the cleaned frame (0 = untouched, 1 = fully
//! cleaned). Nothing here allocates after construction.

use nnnoiseless::{DenoiseState, FRAME_SIZE};

/// RNNoise (and the echo canceller) are only correct at 48 kHz.
pub const CLEAN_SAMPLE_RATE: u32 = 48_000;

/// Samples in one processing frame (10 ms at 48 kHz).
pub const FRAME: usize = FRAME_SIZE;

/// RNNoise expects 16-bit-range values carried in floats.
const SCALE: f32 = 32_768.0;

pub struct Denoiser {
    state: Box<DenoiseState<'static>>,
    scaled: [f32; FRAME],
    cleaned: [f32; FRAME],
}

impl Denoiser {
    pub fn new() -> Self {
        Self {
            state: DenoiseState::new(),
            scaled: [0.0; FRAME],
            cleaned: [0.0; FRAME],
        }
    }

    /// Clean one frame in place. `frame` must hold exactly [`FRAME`] samples;
    /// `strength` is clamped to 0..=1.
    pub fn clean_frame(&mut self, frame: &mut [f32], strength: f32) {
        debug_assert_eq!(frame.len(), FRAME);
        let strength = if strength.is_finite() {
            strength.clamp(0.0, 1.0)
        } else {
            1.0
        };
        for (scaled, sample) in self.scaled.iter_mut().zip(frame.iter()) {
            *scaled = sample * SCALE;
        }
        self.state.process_frame(&mut self.cleaned, &self.scaled);
        for (sample, cleaned) in frame.iter_mut().zip(self.cleaned.iter()) {
            let dry = *sample;
            *sample = dry + (cleaned / SCALE - dry) * strength;
        }
    }
}
