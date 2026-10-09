//! The microphone's signal-cleaning stage: echo cancellation, then noise
//! suppression, both working on fixed 10 ms frames.
//!
//! The capture callback's chunks are arbitrary lengths, so they are
//! re-blocked here into 480-sample frames. The stage's output is its input
//! delayed by exactly one frame (10 ms), whichever cleaners are on, and it
//! allocates nothing after construction, so it is safe for the real-time
//! callback. While both cleaners are off the stage is bypassed entirely (no
//! delay); switching one on calls [`CleanStage::restart`].

use crate::audio::pw_native::denoise::{Denoiser, FRAME};
use crate::audio::pw_native::echo::EchoCanceller;
use crate::audio::pw_native::ring::Ring;

/// How much of the far-end reference to keep when the reference stream has
/// run ahead of the microphone (clock drift, a stalled callback): a couple
/// of frames, so the echo path the canceller sees stays short and steady.
const REFERENCE_KEEP: usize = FRAME * 2;
const REFERENCE_MAX: usize = FRAME * 6;

/// What to run for one call.
#[derive(Clone, Copy)]
pub struct CleanSettings {
    pub echo_cancel: bool,
    pub denoise: bool,
    /// 0..=1 blend of the noise-suppressed signal.
    pub denoise_strength: f32,
}

impl CleanSettings {
    pub fn any(&self) -> bool {
        self.echo_cancel || self.denoise
    }
}

pub struct CleanStage {
    denoiser: Box<Denoiser>,
    /// Built on first use: it is the heaviest part and most users never
    /// enable it.
    echo: Option<Box<EchoCanceller>>,
    input: [f32; FRAME],
    output: [f32; FRAME],
    far: [f32; FRAME],
    input_len: usize,
    output_pos: usize,
    output_len: usize,
}

impl CleanStage {
    pub fn new() -> Self {
        Self {
            denoiser: Box::new(Denoiser::new()),
            echo: None,
            input: [0.0; FRAME],
            output: [0.0; FRAME],
            far: [0.0; FRAME],
            input_len: 0,
            output_pos: 0,
            output_len: 0,
        }
    }

    /// Drop any half-collected frame, so stale audio is never played after a
    /// bypass. The models keep what they have learned.
    pub fn restart(&mut self) {
        self.input_len = 0;
        self.output_pos = 0;
        self.output_len = 0;
    }


    /// Clean `samples` in place. `reference` carries what the speakers play
    /// (only read while echo cancellation is on).
    pub fn process(&mut self, samples: &mut [f32], settings: CleanSettings, reference: &Ring) {
        for sample in samples {
            let out = if self.output_pos < self.output_len {
                let value = self.output[self.output_pos];
                self.output_pos += 1;
                value
            } else {
                0.0
            };
            self.input[self.input_len] = *sample;
            self.input_len += 1;
            if self.input_len == FRAME {
                self.finish_frame(settings, reference);
            }
            *sample = out;
        }
    }

    fn finish_frame(&mut self, settings: CleanSettings, reference: &Ring) {
        self.output = self.input;
        if settings.echo_cancel {
            if reference.available() > REFERENCE_MAX {
                reference.discard_through(reference.write_position().wrapping_sub(REFERENCE_KEEP));
            }
            // Underrun (nothing playing, or the reference stream not up yet)
            // arrives as silence, which is the truth: no far end.
            reference.pop(&mut self.far);
            self.echo
                .get_or_insert_with(|| Box::new(EchoCanceller::new()))
                .cancel_frame(&mut self.output, &self.far);
        }
        if settings.denoise {
            self.denoiser
                .clean_frame(&mut self.output, settings.denoise_strength);
        }
        self.output_pos = 0;
        self.output_len = FRAME;
        self.input_len = 0;
    }
}
