//! Per-channel "Stream send" (Streamer Mode design): captures each app
//! stream assigned to the channel in parallel with (never through) that
//! app's normal link into the channel sink, applies this channel's own
//! independent Stream gain/mute, and plays the result out through a
//! stream whose ports the loop links into the Streamer Mode mix.
//!
//! Deliberately built as a stream pair, not a virtual sink: a sink-shaped
//! node is a real, selectable audio device (it showed up as a confusing
//! extra entry in pavucontrol/OBS's device pickers in an earlier version of
//! this feature). A stream, like the EQ insert below, is invisible to those
//! pickers - only the single Streamer Mode mix should ever be selectable
//! there. Same construction as the EQ insert (`eq_chain.rs`), minus DSP and
//! the channel-test recorder: this is just gain and mute.
//!
//! Topology: each assigned app's own output ──parallel link──▶ capture ──gain──▶ ring ──▶ playback ──▶ Streamer Mode bus

use std::sync::atomic::{AtomicBool, AtomicU32, Ordering};
use std::sync::Arc;

use pipewire as pw;
use pw::spa;
use spa::pod::Pod;

use crate::audio::pw_native::levels::LevelStore;
use crate::audio::pw_native::ring::Ring;
use crate::error::SinkError;

/// node.name prefixes of the stream-send helper streams (under
/// INTERNAL_PREFIX, so they never show up in app/stream listings, and -
/// unlike a virtual sink - never as a selectable device anywhere).
pub const STREAM_SEND_CAPTURE_PREFIX: &str = "sink-internal-streamsend-capture-";
pub const STREAM_SEND_PLAYBACK_PREFIX: &str = "sink-internal-streamsend-playback-";

/// `LevelStore` key for a channel's independent Stream meter - distinct from
/// the channel's own name (its Personal meter's key), so the two lanes'
/// VU meters never show the same animation. The frontend builds this same
/// string from `channel.name` (see `VuMeter`'s callers in StreamerLanes).
pub fn stream_meter_key(channel_name: &str) -> String {
    format!("{channel_name}__stream")
}

fn decode_f32_chunk(bytes: &[u8], output: &mut Vec<f32>) {
    output.clear();
    output.extend(
        bytes
            .as_chunks::<4>()
            .0
            .iter()
            .map(|b| f32::from_ne_bytes([b[0], b[1], b[2], b[3]])),
    );
}

/// Stereo-only interleaved F32 format pod: the Streamer Mode mix (like
/// every mix bus) is always stereo, regardless of whether the channel
/// feeding this send is itself a spatial 7.1 channel.
fn f32_format() -> Result<Vec<u8>, SinkError> {
    let mut info = spa::param::audio::AudioInfoRaw::new();
    info.set_format(spa::param::audio::AudioFormat::F32LE);
    info.set_rate(48_000);
    info.set_channels(2);
    let mut position = [0; spa::param::audio::MAX_CHANNELS];
    position[0] = spa::sys::SPA_AUDIO_CHANNEL_FL;
    position[1] = spa::sys::SPA_AUDIO_CHANNEL_FR;
    info.set_position(position);
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
    .map_err(|e| SinkError::Config(format!("stream send format pod: {e:?}")))
}

/// Runtime-adjustable gain/mute, shared between the command layer (writer)
/// and the RT capture callback (reader). Lock-free: a channel's Stream
/// fader can be dragged continuously without ever blocking the audio
/// thread.
struct StreamSendParams {
    gain_bits: AtomicU32,
    muted: AtomicBool,
}

impl StreamSendParams {
    fn new() -> Self {
        Self {
            gain_bits: AtomicU32::new(1.0f32.to_bits()),
            muted: AtomicBool::new(false),
        }
    }

    fn set_gain(&self, fraction: f32) {
        self.gain_bits.store(fraction.to_bits(), Ordering::Relaxed);
    }

    fn set_muted(&self, muted: bool) {
        self.muted.store(muted, Ordering::Relaxed);
    }

    fn snapshot(&self) -> (f32, bool) {
        (
            f32::from_bits(self.gain_bits.load(Ordering::Relaxed)),
            self.muted.load(Ordering::Relaxed),
        )
    }
}

struct CaptureCtx {
    params: Arc<StreamSendParams>,
    ring: Arc<Ring>,
    input: Vec<f32>,
    levels: Arc<LevelStore>,
    meter_slot: usize,
}

struct PlaybackCtx {
    ring: Arc<Ring>,
}

pub struct StreamSendHandle {
    capture: pw::stream::StreamRc,
    _capture_listener: pw::stream::StreamListener<CaptureCtx>,
    playback: pw::stream::StreamRc,
    _playback_listener: pw::stream::StreamListener<PlaybackCtx>,
    params: Arc<StreamSendParams>,
    levels: Arc<LevelStore>,
    meter_key: String,
}

impl Drop for StreamSendHandle {
    fn drop(&mut self) {
        self.levels.release(&self.meter_key);
    }
}

impl StreamSendHandle {
    /// Build both streams. Neither auto-connects to anything - the loop
    /// (`ensure_all_links`) links the capture's input from every app
    /// stream currently assigned to `channel_name`, and the playback's
    /// output into the Streamer Mode mix, exactly like it already links a
    /// channel's own monitor into a regular bus.
    pub fn new(
        core: &pw::core::CoreRc,
        channel_name: &str,
        levels: Arc<LevelStore>,
    ) -> Result<Self, SinkError> {
        let err =
            |stage: &str, e: pw::Error| SinkError::Config(format!("stream send {stage}: {e}"));
        let params = Arc::new(StreamSendParams::new());
        let ring = Arc::new(Ring::new(8192));
        let meter_key = stream_meter_key(channel_name);
        let meter_slot = levels
            .slot_for(&meter_key)
            .ok_or_else(|| SinkError::Config(format!("meter budget exhausted for {meter_key}")))?;

        let capture_name = format!("{STREAM_SEND_CAPTURE_PREFIX}{channel_name}");
        let capture = pw::stream::StreamRc::new(
            core.clone(),
            &capture_name,
            pw::properties::properties! {
                "media.type" => "Audio",
                "media.category" => "Capture",
                "node.name" => capture_name.as_str(),
                "node.passive" => "true",
                "node.dont-reconnect" => "true",
            },
        )
        .map_err(|e| err("capture stream", e))?;

        let capture_listener = capture
            .add_local_listener_with_user_data(CaptureCtx {
                params: params.clone(),
                ring: ring.clone(),
                input: Vec::with_capacity(4096),
                levels: levels.clone(),
                meter_slot,
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
                let samples = (bytes.len() / 4) / 2 * 2;
                decode_f32_chunk(&bytes[..samples * 4], &mut ctx.input);
                let (gain, muted) = ctx.params.snapshot();
                if muted {
                    ctx.input.iter_mut().for_each(|s| *s = 0.0);
                } else if gain != 1.0 {
                    ctx.input.iter_mut().for_each(|s| *s *= gain);
                }
                // Metered post-gain/mute, here rather than via a separate
                // MeterHandle tap: this is the one place that already holds
                // the exact samples the Stream lane's fader is controlling,
                // so the VU meter reacts to it independently of the
                // channel's own (Personal) meter.
                let mut peaks = [0.0f32; 2];
                for (i, sample) in ctx.input.iter().enumerate() {
                    let v = sample.abs();
                    peaks[i & 1] = peaks[i & 1].max(v);
                }
                ctx.levels.raise(ctx.meter_slot, 0, peaks[0]);
                ctx.levels.raise(ctx.meter_slot, 1, peaks[1]);
                let _ = ctx.ring.push(&ctx.input);
            })
            .register()
            .map_err(|e| err("capture listener", e))?;

        let capture_format = f32_format()?;
        let mut capture_params = [Pod::from_bytes(&capture_format)
            .ok_or_else(|| SinkError::Config("stream send capture format pod invalid".into()))?];
        capture
            .connect(
                spa::utils::Direction::Input,
                None,
                pw::stream::StreamFlags::MAP_BUFFERS | pw::stream::StreamFlags::RT_PROCESS,
                &mut capture_params,
            )
            .map_err(|e| err("capture connect", e))?;

        let playback_name = format!("{STREAM_SEND_PLAYBACK_PREFIX}{channel_name}");
        let playback = pw::stream::StreamRc::new(
            core.clone(),
            &playback_name,
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
            .add_local_listener_with_user_data(PlaybackCtx { ring: ring.clone() })
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
                let max_frames = max_bytes / 8;
                let frames = if requested > 0 {
                    requested.min(max_frames)
                } else {
                    max_frames.min(1024)
                };
                if frames == 0 {
                    return;
                }
                if let Some(bytes) = data.data() {
                    let mut chunk = [0.0f32; 2048];
                    let total = (frames * 2).min(chunk.len());
                    ctx.ring.pop(&mut chunk[..total]);
                    for (i, s) in chunk[..total].iter().enumerate() {
                        let off = i * 4;
                        bytes[off..off + 4].copy_from_slice(&s.to_ne_bytes());
                    }
                }
                let chunk_info = data.chunk_mut();
                *chunk_info.offset_mut() = 0;
                *chunk_info.stride_mut() = 8;
                *chunk_info.size_mut() = (frames * 8) as u32;
            })
            .register()
            .map_err(|e| err("playback listener", e))?;

        let playback_format = f32_format()?;
        let mut playback_params = [Pod::from_bytes(&playback_format)
            .ok_or_else(|| SinkError::Config("stream send playback format pod invalid".into()))?];
        playback
            .connect(
                spa::utils::Direction::Output,
                None,
                pw::stream::StreamFlags::MAP_BUFFERS | pw::stream::StreamFlags::RT_PROCESS,
                &mut playback_params,
            )
            .map_err(|e| err("playback connect", e))?;

        Ok(Self {
            capture,
            _capture_listener: capture_listener,
            playback,
            _playback_listener: playback_listener,
            params,
            levels,
            meter_key,
        })
    }

    /// Node id of the capture stream - the loop links every assigned app's
    /// own output ports here. Only valid once the server has created the
    /// stream's node (callers filter u32::MAX, like `mic_playback_node`).
    pub fn capture_node_id(&self) -> u32 {
        self.capture.node_id()
    }

    /// Node id of the playback stream - the loop links its output ports
    /// into the Streamer Mode mix. Same u32::MAX caveat as above.
    pub fn playback_node_id(&self) -> u32 {
        self.playback.node_id()
    }

    /// Same cubic percent->linear mapping as every other fader in Mixweave
    /// (`pods::percent_to_linear` - what a channel's own PipeWire
    /// channelVolumes actually receives). Applying gain linearly here
    /// instead made a Stream fader sound roughly 6-7x louder than its own
    /// Personal counterpart at the same percentage - e.g. 5% here matched
    /// what ~35% looks like on the cubic curve, since low percentages fall
    /// off far more slowly under a straight division than under a cube.
    pub fn set_gain_percent(&self, percent: u8) {
        self.params
            .set_gain(crate::audio::pw_native::pods::percent_to_linear(percent));
    }

    pub fn set_muted(&self, muted: bool) {
        self.params.set_muted(muted);
    }
}
