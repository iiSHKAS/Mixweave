#![allow(clippy::items_after_test_module)]

//! Mic engine: captures the selected (or default) microphone,
//! runs the native DSP chain (EQ → gate → gain → compressor → limiter), and
//! plays the processed signal into a `Audio/Source/Virtual` node - a
//! virtual microphone that Discord/OBS can capture.
//!
//! Topology:  hw mic ──capture stream──▶ EQ ─┬─▶ DSP (Personal gain) ──ring──▶ playback stream ──▶ sink_mic (virtual source)
//!                                            └─▶ DSP (Stream gain)  ──ring──▶ playback stream ──▶ sink_mic_stream (virtual source)
//!
//! The Stream leg (Streamer Mode design, mirroring `stream_send.rs` for
//! channels) shares the EQ and the gate/compressor/limiter *settings* with
//! Personal - noise gating and levelling are signal-quality fixes a viewer
//! wants too, not a personal-taste choice the way volume is - but runs its
//! own `DspChain` instance (so its gate/compressor/limiter envelopes never
//! interact with Personal's) and applies its own independent gain/mute
//! (`MicConfig::stream_send_*`) tapped from a copy of the signal made
//! *before* `DspChain::process` runs, since that call zeroes the buffer in
//! place on mute - reusing Personal's buffer post-mute would silence Stream
//! right along with it. It plays into a second, genuinely separate virtual
//! source node (`stream_mic_node_name`) rather than Streamer Mode's channel
//! mechanism (a hidden hidden stream pair) because this one *must* be
//! independently selectable in a recorder's own microphone picker (a
//! hidden stream pair, like a channel's Stream send, never would be).

use std::sync::atomic::{AtomicBool, AtomicU32, AtomicU8, AtomicUsize, Ordering};
use std::sync::Arc;

use pipewire as pw;
use pw::spa;
use spa::pod::Pod;

use crate::audio::pw_native::clean::{CleanSettings, CleanStage};
use crate::audio::pw_native::denoise::CLEAN_SAMPLE_RATE;
use crate::audio::pw_native::dsp::{DspChain, DspSettings};
use crate::audio::pw_native::eq::{EqEngine, EqParams};
use crate::audio::pw_native::levels::LevelStore;
use crate::audio::pw_native::ring::Ring;
use crate::audio::types::{EqConfig, MicConfig, MicTestAction, MicTestStatus};
use crate::error::SinkError;

/// node.name of the permanent primary virtual microphone.
pub const MIC_NODE: &str = "sink_mic";

pub fn is_mic_node(name: &str) -> bool {
    name == MIC_NODE || name.starts_with("source_mic_") || is_stream_mic_node(name)
}

/// node.name of a mic's independent "Stream" output - a second, genuinely
/// separate virtual source (not the hidden stream-pair channels use for
/// this) so a recorder's own microphone picker can select it directly,
/// distinctly from the Personal one.
pub fn stream_mic_node_name(node_name: &str) -> String {
    format!("{node_name}_stream")
}

pub fn is_stream_mic_node(name: &str) -> bool {
    name.strip_suffix("_stream")
        .is_some_and(|base| base == MIC_NODE || base.starts_with("source_mic_"))
}

const MIC_TEST_SECONDS: usize = 30;
const MIC_TEST_RATE: usize = 48_000;
const TEST_IDLE: u8 = 0;
const TEST_RECORDING: u8 = 1;
const TEST_LOOPING: u8 = 2;
const CAPTURE_CHUNK_SAMPLES: usize = 4096;

fn decode_f32_chunk(bytes: &[u8], output: &mut Vec<f32>) {
    debug_assert!(bytes.len() / 4 <= output.capacity());
    output.clear();
    output.extend(
        bytes
            .as_chunks::<4>()
            .0
            .iter()
            .map(|b| f32::from_ne_bytes([b[0], b[1], b[2], b[3]])),
    );
}

/// A fixed-size lock-free raw microphone recorder. The capture callback
/// records before EQ/dynamics and substitutes the saved raw samples while
/// looping, so every live processing edit is audible immediately.
pub struct MicTestBuffer {
    samples: Box<[AtomicU32]>,
    length: AtomicUsize,
    cursor: AtomicUsize,
    mode: AtomicU8,
}

impl MicTestBuffer {
    fn new() -> Self {
        Self {
            samples: (0..MIC_TEST_SECONDS * MIC_TEST_RATE)
                .map(|_| AtomicU32::new(0))
                .collect::<Vec<_>>()
                .into_boxed_slice(),
            length: AtomicUsize::new(0),
            cursor: AtomicUsize::new(0),
            mode: AtomicU8::new(TEST_IDLE),
        }
    }

    pub fn apply(&self, action: MicTestAction) -> Result<MicTestStatus, SinkError> {
        match action {
            MicTestAction::StartRecording => {
                self.mode.store(TEST_IDLE, Ordering::Release);
                self.length.store(0, Ordering::Release);
                self.cursor.store(0, Ordering::Release);
                self.mode.store(TEST_RECORDING, Ordering::Release);
            }
            MicTestAction::StopRecording => {
                if self.mode.load(Ordering::Acquire) == TEST_RECORDING {
                    self.mode.store(TEST_IDLE, Ordering::Release);
                }
            }
            MicTestAction::StartLoop => {
                if self.length.load(Ordering::Acquire) == 0 {
                    return Err(SinkError::Config("record a microphone test first".into()));
                }
                self.cursor.store(0, Ordering::Release);
                self.mode.store(TEST_LOOPING, Ordering::Release);
            }
            MicTestAction::StopLoop => {
                if self.mode.load(Ordering::Acquire) == TEST_LOOPING {
                    self.mode.store(TEST_IDLE, Ordering::Release);
                }
            }
            MicTestAction::Status => {}
        }
        Ok(self.status())
    }

    fn status(&self) -> MicTestStatus {
        let mode = self.mode.load(Ordering::Acquire);
        MicTestStatus {
            recording: mode == TEST_RECORDING,
            playing: mode == TEST_LOOPING,
            has_recording: self.length.load(Ordering::Acquire) > 0,
        }
    }

    fn process_raw(&self, buffer: &mut [f32]) {
        match self.mode.load(Ordering::Acquire) {
            TEST_RECORDING => {
                let start = self.cursor.load(Ordering::Relaxed);
                let available = self.samples.len().saturating_sub(start);
                let count = available.min(buffer.len());
                for (slot, sample) in self.samples[start..start + count].iter().zip(buffer.iter()) {
                    slot.store(sample.to_bits(), Ordering::Relaxed);
                }
                let end = start + count;
                self.cursor.store(end, Ordering::Relaxed);
                self.length.store(end, Ordering::Release);
                if count < buffer.len() || end == self.samples.len() {
                    self.mode.store(TEST_IDLE, Ordering::Release);
                }
            }
            TEST_LOOPING => {
                let length = self.length.load(Ordering::Acquire);
                if length == 0 {
                    self.mode.store(TEST_IDLE, Ordering::Release);
                    return;
                }
                let mut cursor = self.cursor.load(Ordering::Relaxed).min(length - 1);
                for sample in buffer {
                    *sample = f32::from_bits(self.samples[cursor].load(Ordering::Relaxed));
                    cursor += 1;
                    if cursor == length {
                        cursor = 0;
                    }
                }
                self.cursor.store(cursor, Ordering::Relaxed);
            }
            _ => {}
        }
    }
}

fn mic_eq_config(config: &MicConfig) -> EqConfig {
    EqConfig {
        enabled: config.eq_enabled,
        preamp_db: config.eq_preamp_db,
        bands: config.eq_bands.clone(),
        tone_bass_db: 0.0,
        tone_voice_db: 0.0,
        tone_treble_db: 0.0,
        ..EqConfig::default()
    }
}

/// Live-tunable DSP parameters, shared with the RT capture callback.
pub struct MicParams {
    pub eq: EqParams,
    gain_bits: AtomicU32,
    gate: AtomicBool,
    comp: AtomicBool,
    limiter: AtomicBool,
    muted: AtomicBool,
    gate_threshold_bits: AtomicU32,
    comp_threshold_bits: AtomicU32,
    comp_ratio_bits: AtomicU32,
    limiter_ceiling_bits: AtomicU32,
    /// Independent Stream gain/mute (Streamer Mode design) - everything
    /// else above (EQ, gate/comp/limiter enable + thresholds) is shared
    /// with Personal; see the module doc comment for why.
    stream_gain_bits: AtomicU32,
    stream_muted: AtomicBool,
    denoise: AtomicBool,
    denoise_strength_bits: AtomicU32,
    echo_cancel: AtomicBool,
}

impl MicParams {
    pub fn from_config(config: &MicConfig) -> Self {
        let p = Self {
            eq: EqParams::from_config(&mic_eq_config(config)),
            gain_bits: AtomicU32::new(1.0f32.to_bits()),
            gate: AtomicBool::new(true),
            comp: AtomicBool::new(true),
            limiter: AtomicBool::new(true),
            muted: AtomicBool::new(false),
            gate_threshold_bits: AtomicU32::new((-40.0f32).to_bits()),
            comp_threshold_bits: AtomicU32::new((-18.0f32).to_bits()),
            comp_ratio_bits: AtomicU32::new(3.0f32.to_bits()),
            limiter_ceiling_bits: AtomicU32::new((-1.0f32).to_bits()),
            stream_gain_bits: AtomicU32::new(1.0f32.to_bits()),
            stream_muted: AtomicBool::new(false),
            denoise: AtomicBool::new(false),
            denoise_strength_bits: AtomicU32::new(0.8f32.to_bits()),
            echo_cancel: AtomicBool::new(false),
        };
        p.apply(config);
        p
    }

    pub fn apply(&self, config: &MicConfig) {
        self.eq.apply(&mic_eq_config(config));
        let gain = f32::from(config.gain_percent) / 100.0;
        self.gain_bits.store(gain.to_bits(), Ordering::Relaxed);
        self.gate.store(config.gate_enabled, Ordering::Relaxed);
        self.comp.store(config.comp_enabled, Ordering::Relaxed);
        self.limiter
            .store(config.limiter_enabled, Ordering::Relaxed);
        self.muted.store(config.muted, Ordering::Relaxed);
        self.gate_threshold_bits
            .store(config.gate_threshold_db.to_bits(), Ordering::Relaxed);
        self.comp_threshold_bits
            .store(config.comp_threshold_db.to_bits(), Ordering::Relaxed);
        self.comp_ratio_bits
            .store(config.comp_ratio.to_bits(), Ordering::Relaxed);
        self.limiter_ceiling_bits
            .store(config.limiter_ceiling_db.to_bits(), Ordering::Relaxed);
        let stream_gain = f32::from(config.stream_send_gain_percent) / 100.0;
        self.stream_gain_bits
            .store(stream_gain.to_bits(), Ordering::Relaxed);
        self.stream_muted
            .store(config.stream_send_muted, Ordering::Relaxed);
        self.denoise
            .store(config.denoise_enabled, Ordering::Relaxed);
        let strength = f32::from(config.denoise_strength_percent.min(100)) / 100.0;
        self.denoise_strength_bits
            .store(strength.to_bits(), Ordering::Relaxed);
        self.echo_cancel
            .store(config.echo_cancel_enabled, Ordering::Relaxed);
    }

    fn echo_cancel_enabled(&self) -> bool {
        self.echo_cancel.load(Ordering::Relaxed)
    }

    fn clean_settings(&self) -> CleanSettings {
        CleanSettings {
            echo_cancel: self.echo_cancel_enabled(),
            denoise: self.denoise_enabled(),
            denoise_strength: self.denoise_strength(),
        }
    }

    fn denoise_enabled(&self) -> bool {
        self.denoise.load(Ordering::Relaxed)
    }

    fn denoise_strength(&self) -> f32 {
        f32::from_bits(self.denoise_strength_bits.load(Ordering::Relaxed))
    }

    fn settings(&self) -> DspSettings {
        DspSettings {
            gate_enabled: self.gate.load(Ordering::Relaxed),
            comp_enabled: self.comp.load(Ordering::Relaxed),
            limiter_enabled: self.limiter.load(Ordering::Relaxed),
            gain: f32::from_bits(self.gain_bits.load(Ordering::Relaxed)),
            muted: self.muted.load(Ordering::Relaxed),
            gate_threshold_db: f32::from_bits(self.gate_threshold_bits.load(Ordering::Relaxed)),
            comp_threshold_db: f32::from_bits(self.comp_threshold_bits.load(Ordering::Relaxed)),
            comp_ratio: f32::from_bits(self.comp_ratio_bits.load(Ordering::Relaxed)),
            limiter_ceiling_db: f32::from_bits(self.limiter_ceiling_bits.load(Ordering::Relaxed)),
        }
    }

    /// Same gate/comp/limiter enable + thresholds as `settings()`, but with
    /// Stream's own independent gain/mute swapped in - two separate
    /// `DspSettings` views over mostly-shared state, not a shared one.
    fn stream_settings(&self) -> DspSettings {
        DspSettings {
            gain: f32::from_bits(self.stream_gain_bits.load(Ordering::Relaxed)),
            muted: self.stream_muted.load(Ordering::Relaxed),
            ..self.settings()
        }
    }
}

struct CaptureCtx {
    /// Echo cancellation + noise suppression ahead of the EQ. Boxed: the
    /// models' state is large.
    clean: Box<CleanStage>,
    /// Whether the previous chunk went through the cleaner, so it is
    /// restarted cleanly when switched on.
    cleaning: bool,
    /// Both cleaners only work at 48 kHz; at any other negotiated rate the
    /// stage is skipped rather than fed the wrong signal.
    clean_rate_ok: bool,
    /// What the speakers play, fed by the reference stream (see
    /// `MicStreams::sync_echo_reference`).
    reference: Arc<Ring>,
    eq: EqEngine,
    chain: DspChain,
    stream_chain: DspChain,
    params: Arc<MicParams>,
    test: Arc<MicTestBuffer>,
    ring: Arc<Ring>,
    stream_ring: Arc<Ring>,
    levels: Arc<LevelStore>,
    level_slot: usize,
    stream_level_slot: usize,
    scratch: Vec<f32>,
    stream_scratch: Vec<f32>,
}

struct PlaybackCtx {
    ring: Arc<Ring>,
}

pub struct MicStreams {
    _capture: pw::stream::StreamRc,
    _capture_listener: pw::stream::StreamListener<CaptureCtx>,
    playback: pw::stream::StreamRc,
    _playback_listener: pw::stream::StreamListener<PlaybackCtx>,
    stream_playback: pw::stream::StreamRc,
    _stream_playback_listener: pw::stream::StreamListener<PlaybackCtx>,
    pub params: Arc<MicParams>,
    pub test: Arc<MicTestBuffer>,
    capture_target: String,
    core: pw::core::CoreRc,
    node_name: String,
    reference_ring: Arc<Ring>,
    reference: Option<EchoReference>,
}

impl MicStreams {
    /// Node id of the playback stream - the loop links its output ports to
    /// the virtual mic itself (WirePlumber 0.5 does not reliably honor
    /// target.object for playback→virtual-source routing).
    pub fn playback_node_id(&self) -> u32 {
        self.playback.node_id()
    }

    /// Node id of the Stream playback stream - the loop links its output
    /// ports to the mic's Stream companion node (`stream_mic_node_name`).
    pub fn stream_playback_node_id(&self) -> u32 {
        self.stream_playback.node_id()
    }

    pub fn capture_target(&self) -> &str {
        &self.capture_target
    }
}


/// Mono F32 format pod for stream negotiation.
fn mono_f32_format() -> Result<Vec<u8>, SinkError> {
    let mut info = spa::param::audio::AudioInfoRaw::new();
    info.set_format(spa::param::audio::AudioFormat::F32LE);
    info.set_channels(1);
    let object = spa::pod::Object {
        type_: spa::sys::SPA_TYPE_OBJECT_Format,
        id: spa::sys::SPA_PARAM_EnumFormat,
        properties: info.into(),
    };
    spa::pod::serialize::PodSerializer::serialize(
        std::io::Cursor::new(Vec::new()),
        &spa::pod::Value::Object(object),
    )
    .map(|(c, _)| c.into_inner())
    .map_err(|e| SinkError::Config(format!("mic format pod: {e:?}")))
}

impl MicStreams {
    /// Build both streams. `mic_target` is the already validated node.name
    /// of a hardware microphone. The chain is never started without an
    /// explicit target: an unpinned AUTOCONNECT capture could select Mixweave's
    /// own virtual microphone when it is the system default.
    pub fn new(
        core: &pw::core::CoreRc,
        config: &MicConfig,
        node_name: &str,
        mic_target: &str,
        levels: Arc<LevelStore>,
    ) -> Result<Self, SinkError> {
        let err = |stage: &str, e: pw::Error| SinkError::Config(format!("mic {stage}: {e}"));
        let params = Arc::new(MicParams::from_config(config));
        let test = Arc::new(MicTestBuffer::new());
        let level_slot = levels
            .slot_for(node_name)
            .ok_or_else(|| SinkError::Config("meter budget exhausted for mic".into()))?;
        let stream_level_slot = levels
            .slot_for(&crate::audio::pw_native::stream_send::stream_meter_key(
                node_name,
            ))
            .ok_or_else(|| SinkError::Config("meter budget exhausted for mic".into()))?;
        // ~85 ms of headroom at 48 kHz; actual added latency is one quantum.
        let ring = Arc::new(Ring::new(4096));
        let stream_ring = Arc::new(Ring::new(4096));
        // Far-end (speaker) audio for the echo canceller: ~340 ms of room.
        let reference_ring = Arc::new(Ring::new(16384));

        // ---- capture: hardware mic -> DSP -> ring ----
        // NOT passive: this stream must hold the hardware mic running for
        // as long as the chain is enabled. With passive links the source
        // suspends the moment its last real consumer leaves (e.g. Discord
        // switching from the raw mic to the virtual one) - and the chain
        // starves exactly when someone starts using it.
        let capture_name = format!("sink-internal-{node_name}-capture");
        let playback_name = format!("sink-internal-{node_name}-playback");
        let mut capture_props = pw::properties::properties! {
            "media.type" => "Audio",
            "media.category" => "Capture",
            "node.name" => capture_name.as_str(),
            // Never let the session manager migrate this stream (e.g. when
            // the default source changes - it could land on sink_mic and
            // feed the chain its own output). Default-follow is handled by
            // rebuilding with a resolved hardware target instead.
            "node.dont-reconnect" => "true",
        };
        capture_props.insert("target.object", mic_target);
        let capture = pw::stream::StreamRc::new(core.clone(), &capture_name, capture_props)
            .map_err(|e| err("capture stream", e))?;

        let capture_listener = capture
            .add_local_listener_with_user_data(CaptureCtx {
                clean: Box::new(CleanStage::new()),
                cleaning: false,
                clean_rate_ok: true,
                reference: reference_ring.clone(),
                eq: EqEngine::new(48000.0),
                chain: DspChain::new(48000.0),
                stream_chain: DspChain::new(48000.0),
                params: params.clone(),
                test: test.clone(),
                ring: ring.clone(),
                stream_ring: stream_ring.clone(),
                levels,
                level_slot,
                stream_level_slot,
                scratch: Vec::with_capacity(CAPTURE_CHUNK_SAMPLES),
                stream_scratch: Vec::with_capacity(CAPTURE_CHUNK_SAMPLES),
            })
            .param_changed(|_, ctx, id, param| {
                // Track the negotiated rate so DSP time constants are right.
                if id != spa::param::ParamType::Format.as_raw() {
                    return;
                }
                let Some(param) = param else { return };
                let mut info = spa::param::audio::AudioInfoRaw::new();
                if info.parse(param).is_ok() && info.rate() > 0 {
                    ctx.clean_rate_ok = info.rate() == CLEAN_SAMPLE_RATE;
                    ctx.eq.set_sample_rate(info.rate() as f32);
                    ctx.chain = DspChain::new(info.rate() as f32);
                    ctx.stream_chain = DspChain::new(info.rate() as f32);
                }
            })
            .process(|stream, ctx| {
                let Some(mut buffer) = stream.dequeue_buffer() else {
                    return;
                };
                let datas = buffer.datas_mut();
                let Some(data) = datas.first_mut() else {
                    return;
                };
                let Some(bytes) = super::capture_chunk_bytes(data) else {
                    return;
                };

                let samples = bytes.len() / 4;
                let settings = ctx.params.settings();
                let stream_settings = ctx.params.stream_settings();
                let mut peak = 0.0f32;
                let mut stream_peak = 0.0f32;
                for chunk in bytes[..samples * 4].chunks(CAPTURE_CHUNK_SAMPLES * 4) {
                    decode_f32_chunk(chunk, &mut ctx.scratch);

                    // Record or substitute the raw hardware signal before any
                    // processing. A running loop therefore follows EQ/dynamics
                    // edits in real time.
                    ctx.test.process_raw(&mut ctx.scratch);

                    // Echo cancellation and noise suppression first, so the
                    // EQ, gate and both output legs all work on the cleaned
                    // signal.
                    let clean_settings = ctx.params.clean_settings();
                    let cleaning = ctx.clean_rate_ok && clean_settings.any();
                    if cleaning {
                        if !ctx.cleaning {
                            ctx.clean.restart();
                            // Reference audio queued while the canceller was
                            // off is stale.
                            ctx.reference
                                .discard_through(ctx.reference.write_position());
                        }
                        ctx.clean
                            .process(&mut ctx.scratch, clean_settings, &ctx.reference);
                    }
                    ctx.cleaning = cleaning;

                    ctx.eq.process_mono_eq(&mut ctx.scratch, &ctx.params.eq);

                    // Fork for the Stream leg before `chain.process` runs -
                    // it zeroes the buffer in place on mute, and reusing
                    // that afterward would couple Stream to Personal's mute
                    // (see the module doc comment).
                    ctx.stream_scratch.clear();
                    ctx.stream_scratch.extend_from_slice(&ctx.scratch);
                    ctx.stream_chain
                        .process(&mut ctx.stream_scratch, &stream_settings);
                    stream_peak = ctx
                        .stream_scratch
                        .iter()
                        .fold(stream_peak, |maximum, sample| maximum.max(sample.abs()));
                    let _ = ctx.stream_ring.push(&ctx.stream_scratch);

                    ctx.chain.process(&mut ctx.scratch, &settings);
                    peak = ctx
                        .scratch
                        .iter()
                        .fold(peak, |maximum, sample| maximum.max(sample.abs()));
                    let _ = ctx.ring.push(&ctx.scratch);
                }

                // Post-DSP level for the UI (mono → both meter channels).
                ctx.levels.raise(ctx.level_slot, 0, peak);
                ctx.levels.raise(ctx.level_slot, 1, peak);
                ctx.levels.raise(ctx.stream_level_slot, 0, stream_peak);
                ctx.levels.raise(ctx.stream_level_slot, 1, stream_peak);
            })
            .register()
            .map_err(|e| err("capture listener", e))?;

        let format = mono_f32_format()?;
        let mut capture_params = [Pod::from_bytes(&format)
            .ok_or_else(|| SinkError::Config("mic capture format pod invalid".into()))?];
        capture
            .connect(
                spa::utils::Direction::Input,
                None,
                pw::stream::StreamFlags::AUTOCONNECT
                    | pw::stream::StreamFlags::MAP_BUFFERS
                    | pw::stream::StreamFlags::RT_PROCESS,
                &mut capture_params,
            )
            .map_err(|e| err("capture connect", e))?;

        // ---- playback: ring -> virtual source ----
        // node.autoconnect=false keeps WirePlumber's hands off this stream
        // (it routes playback streams to the default *sink*, i.e. the
        // speakers - observed live); the loop links it to sink_mic itself.
        let playback = pw::stream::StreamRc::new(
            core.clone(),
            &playback_name,
            // NOT passive (see capture): the processed signal must reach
            // sink_mic whenever the chain is up, regardless of who is -
            // or isn't - capturing at this instant.
            pw::properties::properties! {
                "media.type" => "Audio",
                "media.category" => "Playback",
                "node.name" => playback_name.as_str(),
                "node.autoconnect" => "false",
                "node.dont-reconnect" => "true",
            },
        )
        .map_err(|e| err("playback stream", e))?;

        let playback_listener = playback
            .add_local_listener_with_user_data(PlaybackCtx { ring })
            .process(|stream, ctx| {
                let Some(mut buffer) = stream.dequeue_buffer() else {
                    return;
                };
                // Fill only what the graph asked for this cycle - filling
                // the whole mmap'd buffer (8k+ frames vs ~1k produced per
                // quantum) starves the ring and chops the audio.
                let requested = buffer.requested() as usize;
                let datas = buffer.datas_mut();
                let Some(data) = datas.first_mut() else {
                    return;
                };
                let max_bytes = data.data().map(|d| d.len()).unwrap_or(0);
                let max_frames = max_bytes / 4;
                let n = if requested > 0 {
                    requested.min(max_frames)
                } else {
                    max_frames.min(1024)
                };
                if n == 0 {
                    return;
                }
                {
                    let bytes = data.data().expect("checked above");
                    // Pop straight into the buffer as f32 ne bytes.
                    let mut frame = [0.0f32; 1024];
                    let mut written = 0;
                    while written < n {
                        let take = (n - written).min(frame.len());
                        ctx.ring.pop(&mut frame[..take]);
                        for (i, s) in frame[..take].iter().enumerate() {
                            let off = (written + i) * 4;
                            bytes[off..off + 4].copy_from_slice(&s.to_ne_bytes());
                        }
                        written += take;
                    }
                }
                let chunk = data.chunk_mut();
                *chunk.offset_mut() = 0;
                *chunk.stride_mut() = 4;
                *chunk.size_mut() = (n * 4) as u32;
            })
            .register()
            .map_err(|e| err("playback listener", e))?;

        let mut playback_params = [Pod::from_bytes(&format)
            .ok_or_else(|| SinkError::Config("mic playback format pod invalid".into()))?];
        playback
            .connect(
                spa::utils::Direction::Output,
                None,
                // No AUTOCONNECT: the loop creates the links to sink_mic.
                pw::stream::StreamFlags::MAP_BUFFERS | pw::stream::StreamFlags::RT_PROCESS,
                &mut playback_params,
            )
            .map_err(|e| err("playback connect", e))?;

        // ---- Stream playback: ring -> the mic's Stream companion source ----
        // Same construction as the Personal playback above, just fed by
        // `stream_ring` and linked (by the loop) to `stream_mic_node_name`
        // instead of `node_name`.
        let stream_playback_name = format!("sink-internal-{node_name}-stream-playback");
        let stream_playback = pw::stream::StreamRc::new(
            core.clone(),
            &stream_playback_name,
            pw::properties::properties! {
                "media.type" => "Audio",
                "media.category" => "Playback",
                "node.name" => stream_playback_name.as_str(),
                "node.autoconnect" => "false",
                "node.dont-reconnect" => "true",
            },
        )
        .map_err(|e| err("stream playback stream", e))?;

        let stream_playback_listener = stream_playback
            .add_local_listener_with_user_data(PlaybackCtx { ring: stream_ring })
            .process(|stream, ctx| {
                let Some(mut buffer) = stream.dequeue_buffer() else {
                    return;
                };
                let requested = buffer.requested() as usize;
                let datas = buffer.datas_mut();
                let Some(data) = datas.first_mut() else {
                    return;
                };
                let max_bytes = data.data().map(|d| d.len()).unwrap_or(0);
                let max_frames = max_bytes / 4;
                let n = if requested > 0 {
                    requested.min(max_frames)
                } else {
                    max_frames.min(1024)
                };
                if n == 0 {
                    return;
                }
                {
                    let bytes = data.data().expect("checked above");
                    let mut frame = [0.0f32; 1024];
                    let mut written = 0;
                    while written < n {
                        let take = (n - written).min(frame.len());
                        ctx.ring.pop(&mut frame[..take]);
                        for (i, s) in frame[..take].iter().enumerate() {
                            let off = (written + i) * 4;
                            bytes[off..off + 4].copy_from_slice(&s.to_ne_bytes());
                        }
                        written += take;
                    }
                }
                let chunk = data.chunk_mut();
                *chunk.offset_mut() = 0;
                *chunk.stride_mut() = 4;
                *chunk.size_mut() = (n * 4) as u32;
            })
            .register()
            .map_err(|e| err("stream playback listener", e))?;

        let mut stream_playback_params = [Pod::from_bytes(&format)
            .ok_or_else(|| SinkError::Config("mic stream playback format pod invalid".into()))?];
        stream_playback
            .connect(
                spa::utils::Direction::Output,
                None,
                pw::stream::StreamFlags::MAP_BUFFERS | pw::stream::StreamFlags::RT_PROCESS,
                &mut stream_playback_params,
            )
            .map_err(|e| err("stream playback connect", e))?;

        let mut streams = Self {
            _capture: capture,
            _capture_listener: capture_listener,
            playback,
            _playback_listener: playback_listener,
            stream_playback,
            _stream_playback_listener: stream_playback_listener,
            params,
            test,
            capture_target: mic_target.to_string(),
            core: core.clone(),
            node_name: node_name.to_string(),
            reference_ring,
            reference: None,
        };
        streams.sync_echo_reference(config.echo_cancel_enabled);
        Ok(streams)
    }

    /// Start or stop the far-end capture the echo canceller compares the
    /// microphone against: the default output's monitor, i.e. exactly what
    /// the speakers play. It only exists while echo cancellation is on, so
    /// nobody pays for it otherwise. A failure leaves the canceller running
    /// against silence (it then does nothing) rather than failing the mic.
    pub fn sync_echo_reference(&mut self, enabled: bool) {
        if !enabled {
            self.reference = None;
            return;
        }
        if self.reference.is_some() {
            return;
        }
        match build_echo_reference(
            &self.core,
            &self.node_name,
            self.reference_ring.clone(),
            self.params.clone(),
        ) {
            Ok(reference) => self.reference = Some(reference),
            Err(error) => eprintln!("mixweave: echo cancellation reference unavailable: {error}"),
        }
    }
}

/// The far-end capture stream (see [`MicStreams::sync_echo_reference`]).
struct EchoReference {
    _stream: pw::stream::StreamRc,
    _listener: pw::stream::StreamListener<ReferenceCtx>,
}

struct ReferenceCtx {
    ring: Arc<Ring>,
    params: Arc<MicParams>,
    scratch: Vec<f32>,
}

/// Mono F32 at 48 kHz: the only format the echo canceller runs in.
fn mono_f32_48k_format() -> Result<Vec<u8>, SinkError> {
    let mut info = spa::param::audio::AudioInfoRaw::new();
    info.set_format(spa::param::audio::AudioFormat::F32LE);
    info.set_rate(CLEAN_SAMPLE_RATE);
    info.set_channels(1);
    let object = spa::pod::Object {
        type_: spa::sys::SPA_TYPE_OBJECT_Format,
        id: spa::sys::SPA_PARAM_EnumFormat,
        properties: info.into(),
    };
    spa::pod::serialize::PodSerializer::serialize(
        std::io::Cursor::new(Vec::new()),
        &spa::pod::Value::Object(object),
    )
    .map(|(c, _)| c.into_inner())
    .map_err(|e| SinkError::Config(format!("echo reference format pod: {e:?}")))
}

fn build_echo_reference(
    core: &pw::core::CoreRc,
    node_name: &str,
    ring: Arc<Ring>,
    params: Arc<MicParams>,
) -> Result<EchoReference, SinkError> {
    let err = |stage: &str, e: pw::Error| SinkError::Config(format!("echo reference {stage}: {e}"));
    let name = format!("sink-internal-{node_name}-echo-reference");
    let stream = pw::stream::StreamRc::new(
        core.clone(),
        &name,
        pw::properties::properties! {
            "media.type" => "Audio",
            "media.category" => "Capture",
            "node.name" => name.as_str(),
            // Capture the monitor of the default output (not a microphone),
            // without keeping that output awake on its own.
            "stream.capture.sink" => "true",
            "node.passive" => "true",
        },
    )
    .map_err(|e| err("stream", e))?;

    let listener = stream
        .add_local_listener_with_user_data(ReferenceCtx {
            ring,
            params,
            scratch: Vec::with_capacity(CAPTURE_CHUNK_SAMPLES),
        })
        .process(|stream, ctx| {
            let Some(mut buffer) = stream.dequeue_buffer() else {
                return;
            };
            let datas = buffer.datas_mut();
            let Some(data) = datas.first_mut() else {
                return;
            };
            // Nothing to keep while the canceller is off (the stream is torn
            // down soon after; until then the audio is simply dropped).
            if !ctx.params.echo_cancel_enabled() {
                return;
            }
            let Some(bytes) = super::capture_chunk_bytes(data) else {
                return;
            };
            let samples = bytes.len() / 4;
            for chunk in bytes[..samples * 4].chunks(CAPTURE_CHUNK_SAMPLES * 4) {
                decode_f32_chunk(chunk, &mut ctx.scratch);
                let _ = ctx.ring.push(&ctx.scratch);
            }
        })
        .register()
        .map_err(|e| err("listener", e))?;

    let format = mono_f32_48k_format()?;
    let mut params = [Pod::from_bytes(&format)
        .ok_or_else(|| SinkError::Config("echo reference format pod invalid".into()))?];
    stream
        .connect(
            spa::utils::Direction::Input,
            None,
            pw::stream::StreamFlags::AUTOCONNECT
                | pw::stream::StreamFlags::MAP_BUFFERS
                | pw::stream::StreamFlags::RT_PROCESS,
            &mut params,
        )
        .map_err(|e| err("connect", e))?;

    Ok(EchoReference {
        _stream: stream,
        _listener: listener,
    })
}
