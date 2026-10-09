//! Acoustic echo cancellation: WebRTC's AEC3 (via `sonora`, a pure-Rust port).
//!
//! The far end is what the speakers play (a capture of the default output's
//! monitor, see `mic.rs`); the near end is the microphone. Both are fed one
//! 10 ms frame at a time by [`super::clean::CleanStage`]. AEC3 estimates the
//! delay and the room's echo path itself, and its transparent mode backs off
//! when no echo is present (headphones), so it is safe to leave on.

use sonora::config::EchoCanceller as EchoConfig;
use sonora::{AudioProcessing, Config, StreamConfig};

use crate::audio::pw_native::denoise::{CLEAN_SAMPLE_RATE, FRAME};

pub struct EchoCanceller {
    apm: AudioProcessing,
    far: [f32; FRAME],
    out: [f32; FRAME],
}

impl EchoCanceller {
    pub fn new() -> Self {
        let apm = AudioProcessing::builder()
            .config(Config {
                echo_canceller: Some(EchoConfig::default()),
                ..Default::default()
            })
            .capture_config(StreamConfig::new(CLEAN_SAMPLE_RATE, 1))
            .render_config(StreamConfig::new(CLEAN_SAMPLE_RATE, 1))
            .build();
        Self {
            apm,
            far: [0.0; FRAME],
            out: [0.0; FRAME],
        }
    }

    /// Remove the echo of `far` (what was just played) from `near`, in place.
    /// Both slices must hold exactly [`FRAME`] samples.
    pub fn cancel_frame(&mut self, near: &mut [f32], far: &[f32]) {
        debug_assert_eq!(near.len(), FRAME);
        debug_assert_eq!(far.len(), FRAME);
        self.far.copy_from_slice(far);
        let mut render_out = [0.0f32; FRAME];
        // A failed frame (never expected for correctly sized input) simply
        // passes the microphone through untouched.
        let _ = self
            .apm
            .process_render_f32(&[&self.far[..]], &mut [&mut render_out[..]]);
        if self
            .apm
            .process_capture_f32(&[&near[..]], &mut [&mut self.out[..]])
            .is_ok()
            && self.out.iter().all(|s| s.is_finite())
        {
            near.copy_from_slice(&self.out);
        }
    }
}
