use serde::{Deserialize, Serialize};

/// True if `sink_name` is one of our managed virtual channels. Channels
/// are user-defined (see persistence::channels) but always carry the
/// `sink_` prefix; Sink's own service nodes are excluded.
pub fn is_virtual_sink(sink_name: &str) -> bool {
    sink_name.starts_with("sink_")
        && !crate::persistence::channels::is_reserved_sink_name(sink_name)
}

/// Infrastructure and desktop event streams are not useful application
/// routing targets. Keep this deliberately narrow: regular browsers, games,
/// voice clients and media players must remain visible even when idle.
pub fn should_hide_app(get: impl Fn(&str) -> Option<String>) -> bool {
    let hidden_name = |value: &str| {
        let value = value.trim().to_ascii_lowercase();
        value == "libcanberra"
            || value.contains("sink spatial prototype")
            || value.starts_with("sink-internal-")
    };
    if get("media.role").is_some_and(|role| {
        role.eq_ignore_ascii_case("event") || role.eq_ignore_ascii_case("notification")
    }) {
        return true;
    }
    [
        "application.name",
        "application.process.binary",
        "media.name",
        "node.name",
    ]
    .into_iter()
    .filter_map(get)
    .any(|value| hidden_name(&value))
}

/// History only retains the resolved identity, so use the same narrow name
/// rules when loading entries recorded by older builds.
pub fn is_hidden_app_identity(display_name: &str, match_value: &str) -> bool {
    let values = [display_name, match_value];
    values.iter().any(|value| {
        let value = value.trim().to_ascii_lowercase();
        value == "libcanberra"
            || value.contains("sink spatial prototype")
            || value.starts_with("sink-internal-")
    })
}

/// Property values that are useless as names - media frameworks announcing
/// themselves, or placeholder stream titles.
const GENERIC_NAMES: [&str; 13] = [
    "WEBRTC VoiceEngine",
    "audio-src",
    "Playback Stream",
    "playStream",
    "audio stream",
    "Audio Stream",
    "audio player",
    "media player",
    "output",
    "Playback",
    "ALSA Playback",
    "Audio output",
    "Audio Source",
];

/// Runtime/wrapper names that hide the real app - e.g. Spotify is a
/// Chromium shell, so application.name says "Chromium" while the process
/// binary says "spotify". A wrapper beats a generic, but a real name
/// (usually the binary) beats both.
const WRAPPER_NAMES: [&str; 14] = [
    "Chromium",
    "Google Chrome",
    "Chrome",
    "Electron",
    "WINE",
    "wine64-preloader",
    "java",
    "python",
    "python3",
    "node",
    "mono",
    "dotnet",
    "QtWebEngine",
    "CEF",
];

fn name_quality(value: &str) -> u8 {
    if GENERIC_NAMES.iter().any(|g| g.eq_ignore_ascii_case(value)) {
        0
    } else if WRAPPER_NAMES.iter().any(|w| w.eq_ignore_ascii_case(value)) {
        1
    } else {
        2
    }
}

/// Prettify a value for display: lone all-lowercase binary names get a
/// capital ("spotify" → "Spotify"). Identity matching always uses the raw
/// value, so this never affects routing rules.
fn prettify(value: &str) -> String {
    if !value.contains(' ') && value.chars().all(|c| c.is_ascii_lowercase() || c == '-') {
        let mut chars = value.chars();
        match chars.next() {
            Some(first) => first.to_ascii_uppercase().to_string() + chars.as_str(),
            None => value.to_string(),
        }
    } else {
        value.to_string()
    }
}

/// Resolve a stream's identity: returns (display name, match property,
/// raw match value). The best-quality candidate along the chain wins:
/// real app names beat runtime wrappers beat generic stream titles.
pub fn resolve_identity(get: impl Fn(&str) -> Option<String>) -> (String, String, String) {
    const CHAIN: [&str; 4] = [
        "application.name",
        "application.process.binary",
        "media.name",
        "node.name",
    ];
    let mut best: Option<(u8, String, String)> = None;
    for key in CHAIN {
        if let Some(value) = get(key) {
            // Empty/whitespace property values are noise, not identities.
            if value.trim().is_empty() {
                continue;
            }
            let quality = name_quality(&value);
            // Keep the highest-quality identity discovered so far.
            if best.as_ref().is_none_or(|(q, _, _)| quality > *q) {
                let stop = quality == 2;
                best = Some((quality, key.to_string(), value));
                if stop {
                    break;
                }
            }
        }
    }
    match best {
        Some((_, key, value)) => (prettify(&value), key, value),
        None => (
            "Unknown".to_string(),
            "application.name".to_string(),
            "Unknown".to_string(),
        ),
    }
}


/// A running application audio stream (a PulseAudio "sink input").
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AppStream {
    pub index: u32,
    /// Display name (possibly prettified - not for matching).
    pub app_name: String,
    /// PipeWire property the identity was read from (e.g. "application.name").
    pub match_prop: String,
    /// Raw property value; with `match_prop` this is the stream's stable
    /// identity for assignments, aliases and WirePlumber rules.
    pub match_value: String,
    /// User-chosen display name overriding `app_name` (set via rename).
    pub alias: Option<String>,
    pub icon_name: Option<String>,
    /// Resolved absolute icon file path (desktop-entry based), ready for
    /// the asset protocol. Filled in by the command layer.
    pub icon_path: Option<String>,
    /// Canonical desktop-file id. Raw match fields remain authoritative for
    /// routing; this id groups helper processes belonging to one application.
    #[serde(default)]
    pub desktop_id: Option<String>,
    /// Producing process id - unlocks /proc-based desktop-entry lookup
    /// (cgroup scope, flatpak info, exe path) for icon resolution.
    #[serde(default)]
    pub pid: Option<u32>,
    /// Name of the virtual sink the stream is routed to, if it is one of ours.
    pub assigned_sink: Option<String>,
    pub volume_percent: u8,
    pub muted: bool,
    /// True while the stream is actively producing audio (node running /
    /// not corked) - drives the activity indicator in the app list.
    pub active: bool,
}

/// An application currently capturing the processed virtual microphone.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MicClient {
    pub index: u32,
    /// Processed microphone node this application is capturing.
    pub mic_node: String,
    pub app_name: String,
    pub match_prop: String,
    pub match_value: String,
    pub icon_name: Option<String>,
    pub icon_path: Option<String>,
    #[serde(default)]
    pub pid: Option<u32>,
    pub active: bool,
}

fn default_true() -> bool {
    true
}

/// One of the user-defined virtual channels.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct VirtualSink {
    /// e.g. "sink_game"
    pub name: String,
    /// e.g. "Game"
    pub label: String,
    /// Material Symbol for the strip icon.
    #[serde(default)]
    pub icon: Option<String>,
    pub volume_percent: u8,
    pub muted: bool,
    /// Whether this channel feeds the Stream Mix source (what OBS records).
    #[serde(default = "default_true")]
    pub stream_mix: bool,
    /// Independent "Stream" send level (0-150%): what reaches the Streamer
    /// Mode output, entirely separate from `volume_percent`/`muted` (the
    /// "Personal" level the user hears). Not to be confused with the
    /// legacy `stream_mix` bus-membership flag above.
    #[serde(default = "default_hundred")]
    pub stream_send_volume_percent: u8,
    #[serde(default)]
    pub stream_send_muted: bool,
}

fn default_hundred() -> u8 {
    100
}

/// Live controls reported by PipeWire/PulseAudio for one managed channel.
/// This lets Sink follow changes made in desktop volume controls instead of
/// treating only its own faders as authoritative.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SinkControlState {
    pub name: String,
    pub volume_percent: u8,
    pub muted: bool,
}

/// A physical audio output device.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OutputDevice {
    pub index: u32,
    pub name: String,
    pub description: String,
}

fn default_mic_label() -> String {
    "Mixweave Mic".to_string()
}
fn default_gate_threshold() -> f32 {
    -40.0
}
fn default_comp_threshold() -> f32 {
    -18.0
}
fn default_comp_ratio() -> f32 {
    3.0
}
fn default_limiter_ceiling() -> f32 {
    -1.0
}
fn default_denoise_strength() -> u8 {
    80
}

/// Mic chain configuration (persisted; applied live).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct MicConfig {
    /// Stable PipeWire node name. Older configs deserialize as the primary mic.
    #[serde(default = "default_mic_node")]
    pub node_name: String,
    pub enabled: bool,
    /// node.name of the hardware mic to capture (None = system default).
    pub input_device: Option<String>,
    /// What other apps list the processed mic as (node description).
    #[serde(default = "default_mic_label")]
    pub output_label: String,
    /// 0-200; 100 = unity.
    pub gain_percent: u8,
    pub gate_enabled: bool,
    pub comp_enabled: bool,
    pub limiter_enabled: bool,
    pub muted: bool,
    /// Parametric EQ at the start of the microphone processing chain.
    #[serde(default)]
    pub eq_enabled: bool,
    #[serde(default)]
    pub eq_preamp_db: f32,
    #[serde(default = "default_eq_bands")]
    pub eq_bands: Vec<EqBand>,
    /// Gate opens above this level (dBFS).
    #[serde(default = "default_gate_threshold")]
    pub gate_threshold_db: f32,
    /// Compression starts above this level (dBFS).
    #[serde(default = "default_comp_threshold")]
    pub comp_threshold_db: f32,
    /// Compression ratio (N:1).
    #[serde(default = "default_comp_ratio")]
    pub comp_ratio: f32,
    /// Hard ceiling (dBFS).
    #[serde(default = "default_limiter_ceiling")]
    pub limiter_ceiling_db: f32,
    /// Independent "Stream" send level (0-200%) and mute: what reaches the
    /// Streamer Mode output, entirely separate from `gain_percent`/`muted`
    /// (the "Personal" level other apps capture from this mic).
    #[serde(default = "default_hundred")]
    pub stream_send_gain_percent: u8,
    #[serde(default)]
    pub stream_send_muted: bool,
    /// RNNoise noise suppression ahead of the EQ and dynamics. Off by default:
    /// it adds ~10 ms of delay and changes how the voice sounds.
    #[serde(default)]
    pub denoise_enabled: bool,
    /// How much of the cleaned signal is mixed in, 0-100.
    #[serde(default = "default_denoise_strength")]
    pub denoise_strength_percent: u8,
    /// Acoustic echo cancellation against the default output's playback.
    /// Off by default; only useful with speakers.
    #[serde(default)]
    pub echo_cancel_enabled: bool,
}

impl MicConfig {
    /// Clamp numeric fields to their documented, DSP-safe ranges and replace
    /// non-finite values, so a malformed or hostile IPC payload can't push
    /// the mic chain out of range.
    pub fn clamp_ranges(&mut self) {
        fn finite(v: f32, fallback: f32, lo: f32, hi: f32) -> f32 {
            if v.is_finite() {
                v.clamp(lo, hi)
            } else {
                fallback
            }
        }
        self.gain_percent = self.gain_percent.min(200);
        self.stream_send_gain_percent = self.stream_send_gain_percent.min(200);
        self.denoise_strength_percent = self.denoise_strength_percent.min(100);
        self.eq_preamp_db = finite(self.eq_preamp_db, 0.0, -24.0, 24.0);
        self.eq_bands.truncate(MAX_EQ_BANDS);
        for band in &mut self.eq_bands {
            band.clamp_ranges();
        }
        self.gate_threshold_db = finite(
            self.gate_threshold_db,
            default_gate_threshold(),
            -100.0,
            0.0,
        );
        self.comp_threshold_db = finite(
            self.comp_threshold_db,
            default_comp_threshold(),
            -100.0,
            0.0,
        );
        self.comp_ratio = finite(self.comp_ratio, default_comp_ratio(), 1.0, 20.0);
        self.limiter_ceiling_db = finite(
            self.limiter_ceiling_db,
            default_limiter_ceiling(),
            -60.0,
            0.0,
        );
    }
}

impl Default for MicConfig {
    fn default() -> Self {
        Self {
            node_name: default_mic_node(),
            enabled: false,
            input_device: None,
            output_label: default_mic_label(),
            gain_percent: 100,
            gate_enabled: true,
            comp_enabled: true,
            limiter_enabled: true,
            muted: false,
            eq_enabled: false,
            eq_preamp_db: 0.0,
            eq_bands: default_eq_bands(),
            gate_threshold_db: default_gate_threshold(),
            comp_threshold_db: default_comp_threshold(),
            comp_ratio: default_comp_ratio(),
            limiter_ceiling_db: default_limiter_ceiling(),
            stream_send_gain_percent: 100,
            stream_send_muted: false,
            denoise_enabled: false,
            denoise_strength_percent: default_denoise_strength(),
            echo_cancel_enabled: false,
        }
    }
}

fn default_mic_node() -> String {
    "sink_mic".to_string()
}

#[derive(Debug, Clone, Copy)]
pub enum MicTestAction {
    StartRecording,
    StopRecording,
    StartLoop,
    StopLoop,
    Status,
}

#[derive(Debug, Clone, Copy, Serialize)]
pub struct MicTestStatus {
    pub recording: bool,
    pub playing: bool,
    pub has_recording: bool,
}

#[derive(Debug)]
pub enum ChannelTestAction {
    StartRecording,
    StopRecording,
    StartPlayback,
    StopPlayback,
    LoadAndPlay { samples: Vec<i16>, channels: usize },
    Status,
}

#[derive(Debug, Clone, Copy, Serialize)]
pub struct ChannelTestStatus {
    pub recording: bool,
    pub playing: bool,
    pub has_recording: bool,
    /// Saved user-recording peak in dBFS, clamped to -96 for silence.
    pub recorded_peak_db: f32,
}


/// Hard cap on parametric EQ bands per channel. Ten provides a familiar EQ
/// users already know, keeps preset validation simple, and bounds the RT
/// cost per channel.
pub const MAX_EQ_BANDS: usize = 10;

/// Parametric EQ band shapes (RBJ Audio EQ Cookbook designs).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum EqBandKind {
    Peaking,
    LowShelf,
    HighShelf,
    LowPass,
    HighPass,
}

fn default_band_q() -> f32 {
    1.0
}

/// One parametric EQ band.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct EqBand {
    pub kind: EqBandKind,
    pub freq_hz: f32,
    /// Ignored by LowPass/HighPass (their shape has no gain parameter).
    #[serde(default)]
    pub gain_db: f32,
    /// Peaking/LowPass/HighPass: filter Q. Shelves: RBJ shelf slope S -
    /// one field, two meanings, so presets stay a flat 4-field record.
    #[serde(default = "default_band_q")]
    pub q: f32,
}

impl EqBand {
    /// Clamp to DSP-safe ranges, replacing non-finite values, so a
    /// hostile IPC payload or preset file can't blow up the filter design.
    pub fn clamp_ranges(&mut self) {
        fn finite(v: f32, fallback: f32, lo: f32, hi: f32) -> f32 {
            if v.is_finite() {
                v.clamp(lo, hi)
            } else {
                fallback
            }
        }
        self.freq_hz = finite(self.freq_hz, 1000.0, 20.0, 20000.0);
        self.gain_db = finite(self.gain_db, 0.0, -24.0, 24.0);
        self.q = finite(self.q, default_band_q(), 0.1, 10.0);
    }
}

/// The gaming-audio starting layout: shelves at the extremes, three mids,
/// everything flat. Five bands; the UI can add up to MAX_EQ_BANDS.
pub fn default_eq_bands() -> Vec<EqBand> {
    [
        (EqBandKind::LowShelf, 100.0, 0.71),
        (EqBandKind::Peaking, 500.0, 1.0),
        (EqBandKind::Peaking, 1500.0, 1.0),
        (EqBandKind::Peaking, 5000.0, 1.0),
        (EqBandKind::HighShelf, 10000.0, 0.71),
    ]
    .into_iter()
    .map(|(kind, freq_hz, q)| EqBand {
        kind,
        freq_hz,
        gain_db: 0.0,
        q,
    })
    .collect()
}

/// Playback presentation applied after EQ/dynamics. `Speakers` is a direct
/// stereo path; `Headphones` adds a light low-frequency crossfeed so hard
/// panned material is less fatiguing. This is deliberately not called
/// spatial audio: HRTF surround is a separate processing mode.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum PlaybackMode {
    #[default]
    Speakers,
    Headphones,
}

fn default_playback_gate_threshold_db() -> f32 {
    -48.0
}

fn default_playback_comp_threshold_db() -> f32 {
    -18.0
}

fn default_playback_comp_ratio() -> f32 {
    3.0
}

fn default_playback_limiter_ceiling_db() -> f32 {
    -1.0
}

fn default_spatial_tuning() -> f32 {
    0.5
}

fn default_spatial_distance() -> f32 {
    0.5
}

/// Game and Media deliberately publish a stable 7.1 layout. Keeping the
/// device shape stable means games do not lose their selected output when
/// spatial audio is toggled; the insert below performs either HRTF
/// virtualization or a standards-style stereo downmix.
pub fn is_spatial_channel(name: &str) -> bool {
    matches!(name, "sink_game" | "sink_media") || name.starts_with("sink_spatial_")
}

/// A channel's parametric EQ (persisted per channel; applied live).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct EqConfig {
    #[serde(default)]
    pub enabled: bool,
    /// Headroom trim applied before the band cascade (dB). Boost-heavy
    /// curves need this negative to avoid clipping.
    #[serde(default)]
    pub preamp_db: f32,
    #[serde(default = "default_eq_bands")]
    pub bands: Vec<EqBand>,
    /// Broad three-band tone stage kept separate from the parametric curve.
    #[serde(default)]
    pub tone_bass_db: f32,
    #[serde(default)]
    pub tone_voice_db: f32,
    #[serde(default)]
    pub tone_treble_db: f32,
    /// Post-EQ output gain. Kept separate from EQ preamp: preamp protects a
    /// boost-heavy curve, while this is the user-facing volume boost.
    #[serde(default)]
    pub boost_db: f32,
    #[serde(default)]
    pub gate_enabled: bool,
    #[serde(default = "default_playback_gate_threshold_db")]
    pub gate_threshold_db: f32,
    #[serde(default)]
    pub comp_enabled: bool,
    #[serde(default = "default_playback_comp_threshold_db")]
    pub comp_threshold_db: f32,
    #[serde(default = "default_playback_comp_ratio")]
    pub comp_ratio: f32,
    #[serde(default)]
    pub limiter_enabled: bool,
    #[serde(default = "default_playback_limiter_ceiling_db")]
    pub limiter_ceiling_db: f32,
    #[serde(default)]
    pub playback_mode: PlaybackMode,
    /// Real 7.1-to-binaural HRTF virtualization. Only Game and Media publish
    /// 7.1 inputs; older/custom stereo channels safely ignore these fields.
    #[serde(default)]
    pub spatial_enabled: bool,
    /// 0 = crisp directional performance, 0.5 = neutral, 1 = diffuse immersion.
    #[serde(default = "default_spatial_tuning")]
    pub spatial_tuning: f32,
    /// Apparent distance: close/+2.5 dB at 0, neutral at 0.5, far/-2.5 dB at 1.
    #[serde(default = "default_spatial_distance")]
    pub spatial_distance: f32,
}

impl EqConfig {
    /// Sanitization for the whole config (see EqBand).
    pub fn clamp_ranges(&mut self) {
        if !self.preamp_db.is_finite() {
            self.preamp_db = 0.0;
        }
        self.preamp_db = self.preamp_db.clamp(-24.0, 24.0);
        for tone in [
            &mut self.tone_bass_db,
            &mut self.tone_voice_db,
            &mut self.tone_treble_db,
        ] {
            if !tone.is_finite() {
                *tone = 0.0;
            }
            *tone = tone.clamp(-12.0, 12.0);
        }
        if !self.boost_db.is_finite() {
            self.boost_db = 0.0;
        }
        self.boost_db = self.boost_db.clamp(-12.0, 12.0);
        if !self.gate_threshold_db.is_finite() {
            self.gate_threshold_db = default_playback_gate_threshold_db();
        }
        self.gate_threshold_db = self.gate_threshold_db.clamp(-80.0, -10.0);
        if !self.comp_threshold_db.is_finite() {
            self.comp_threshold_db = default_playback_comp_threshold_db();
        }
        self.comp_threshold_db = self.comp_threshold_db.clamp(-60.0, 0.0);
        if !self.comp_ratio.is_finite() {
            self.comp_ratio = default_playback_comp_ratio();
        }
        self.comp_ratio = self.comp_ratio.clamp(1.0, 10.0);
        if !self.limiter_ceiling_db.is_finite() {
            self.limiter_ceiling_db = default_playback_limiter_ceiling_db();
        }
        self.limiter_ceiling_db = self.limiter_ceiling_db.clamp(-12.0, 0.0);
        if !self.spatial_tuning.is_finite() {
            self.spatial_tuning = default_spatial_tuning();
        }
        self.spatial_tuning = self.spatial_tuning.clamp(0.0, 1.0);
        if !self.spatial_distance.is_finite() {
            self.spatial_distance = default_spatial_distance();
        }
        self.spatial_distance = self.spatial_distance.clamp(0.0, 1.0);
        self.bands.truncate(MAX_EQ_BANDS);
        for band in &mut self.bands {
            band.clamp_ranges();
        }
    }

    /// The insert is shared by EQ and all playback effects. Keep the raw
    /// channel-to-device links only while every stage is neutral.
    pub fn needs_chain(&self) -> bool {
        self.enabled
            || self.boost_db.abs() > f32::EPSILON
            || self.gate_enabled
            || self.comp_enabled
            || self.limiter_enabled
            || self.playback_mode == PlaybackMode::Headphones
    }
}

impl Default for EqConfig {
    fn default() -> Self {
        Self {
            enabled: false,
            preamp_db: 0.0,
            bands: default_eq_bands(),
            tone_bass_db: 0.0,
            tone_voice_db: 0.0,
            tone_treble_db: 0.0,
            boost_db: 0.0,
            gate_enabled: false,
            gate_threshold_db: default_playback_gate_threshold_db(),
            comp_enabled: false,
            comp_threshold_db: default_playback_comp_threshold_db(),
            comp_ratio: default_playback_comp_ratio(),
            limiter_enabled: false,
            limiter_ceiling_db: default_playback_limiter_ceiling_db(),
            playback_mode: PlaybackMode::Speakers,
            spatial_enabled: false,
            spatial_tuning: default_spatial_tuning(),
            spatial_distance: default_spatial_distance(),
        }
    }
}
