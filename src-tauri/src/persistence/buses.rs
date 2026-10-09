use std::fs;
use std::path::PathBuf;

use serde::{Deserialize, Serialize};

use crate::error::SinkError;

/// Node-name prefix for user-created mixes (the seeded default keeps the
/// historical "sink_stream" name so existing OBS setups keep working).
pub const BUS_PREFIX: &str = "sink_bus_";
pub const DEFAULT_BUS_NODE: &str = "sink_stream";
/// The always-on "Streamer Mode" mix: what OBS/recorders capture as the
/// stream's audio, fed by every channel's independent Stream send (see
/// `streamer_gain` and `crate::audio::types::VirtualSink::stream_send_*`) -
/// entirely separate from what the master mix scales for the user's own
/// ("Personal") listening volume.
pub const STREAMER_MODE_BUS_NODE: &str = "sink_streamer_mode";
pub const MAX_BUSES: usize = 4;

/// True if `name` is a mix bus node (not a channel, not a service node).
pub fn is_bus_name(name: &str) -> bool {
    name == DEFAULT_BUS_NODE
        || name == STREAMER_MODE_BUS_NODE
        || name.strip_prefix(BUS_PREFIX).is_some_and(|suffix| {
            !suffix.is_empty()
                && suffix.chars().all(|character| {
                    character.is_ascii_lowercase() || character.is_ascii_digit() || character == '_'
                })
        })
}

/// True if `name` is the always-on master mix: it carries every channel,
/// can't be deleted, and its membership is managed automatically.
pub fn is_master(name: &str) -> bool {
    name == DEFAULT_BUS_NODE
}

/// True if `name` is the always-on Streamer Mode mix (see
/// `STREAMER_MODE_BUS_NODE`): same protections as the master mix (can't be
/// deleted, always carries every channel), but a fully independent gain
/// stage from it.
pub fn is_streamer_mode(name: &str) -> bool {
    name == STREAMER_MODE_BUS_NODE
}

/// True for either of the two always-on, undeletable mixes.
pub fn is_protected_bus(name: &str) -> bool {
    is_master(name) || is_streamer_mode(name)
}

/// The master mix's volume/mute, read as a true listening-volume control:
/// every channel's own live level is multiplied by this fraction, and
/// every channel is silenced while this is muted - not just what a
/// recorder captures. Defaults to unity/unmuted if the master definition
/// is ever missing.
pub fn master_gain(buses: &Buses) -> (f32, bool) {
    match buses.get(DEFAULT_BUS_NODE) {
        Some(def) => (def.volume_percent as f32 / 100.0, def.muted),
        None => (1.0, false),
    }
}

/// The Streamer Mode mix's volume/mute, read the same way as
/// [`master_gain`] but applied to every channel's independent Stream send
/// instead of its Personal level - muting this silences the stream output
/// only, never what the user hears themselves (and vice versa).
pub fn streamer_gain(buses: &Buses) -> (f32, bool) {
    match buses.get(STREAMER_MODE_BUS_NODE) {
        Some(def) => (def.volume_percent as f32 / 100.0, def.muted),
        None => (1.0, false),
    }
}

/// The volume actually sent to a channel's live PipeWire sink at `raw`
/// (0-150%), scaled by a gain fraction from [`master_gain`]/[`streamer_gain`].
pub fn scaled_volume(raw: u8, fraction: f32) -> u8 {
    ((raw as f32) * fraction).round().clamp(0.0, 150.0) as u8
}

/// One user-defined mix: a capturable virtual source carrying the chosen
/// channels. The label is what recorders (OBS etc.) display.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct BusDef {
    /// Stable node name, e.g. "sink_stream" or "sink_bus_voice_only".
    pub name: String,
    /// Display label - also the device description recorders see.
    pub label: String,
    /// Exclude mode (false): channels carried by this mix.
    /// Exclude mode (true): channels kept OUT of this mix.
    pub channels: Vec<String>,
    /// True = the mix carries every channel except `channels`, so new
    /// channels join automatically ("everything but music"). False = the
    /// mix carries exactly `channels` (manual selection).
    #[serde(default)]
    pub exclude: bool,
    /// Playback level (0-150%): what recorders hear for a regular mix, or
    /// the true listening-volume fraction for the master mix (see
    /// `master_gain`). Persisted so a mix keeps its level across UI
    /// remounts, profile switches, and restarts.
    #[serde(default = "default_volume")]
    pub volume_percent: u8,
    /// Muted: recorders hear silence for a regular mix, or every channel is
    /// silenced for the master mix (see `master_gain`). Persisted like the
    /// volume.
    #[serde(default)]
    pub muted: bool,
}

fn default_volume() -> u8 {
    100
}

impl BusDef {
    /// The channels this mix actually carries, given the full channel set.
    pub fn effective_members(&self, all_channels: &[String]) -> Vec<String> {
        if self.exclude {
            all_channels
                .iter()
                .filter(|c| !self.channels.contains(c))
                .cloned()
                .collect()
        } else {
            self.channels.clone()
        }
    }
}

/// The user's mixes, stored at `$XDG_CONFIG_HOME/mixweave/buses.json`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Buses {
    pub buses: Vec<BusDef>,
}

impl Default for Buses {
    fn default() -> Self {
        Self {
            buses: vec![
                BusDef {
                    name: DEFAULT_BUS_NODE.to_string(),
                    label: "Master Mix".to_string(),
                    channels: Vec::new(),
                    exclude: false,
                    volume_percent: 100,
                    muted: false,
                },
                BusDef {
                    name: STREAMER_MODE_BUS_NODE.to_string(),
                    label: "Streamer Mode".to_string(),
                    channels: Vec::new(),
                    exclude: false,
                    volume_percent: 100,
                    muted: false,
                },
            ],
        }
    }
}

fn slugify(label: &str) -> String {
    let slug: String = label
        .to_lowercase()
        .chars()
        .map(|c| if c.is_ascii_alphanumeric() { c } else { '_' })
        .collect::<String>()
        .split('_')
        .filter(|s| !s.is_empty())
        .collect::<Vec<_>>()
        .join("_");
    if slug.is_empty() {
        "mix".to_string()
    } else {
        slug
    }
}

impl Buses {
    pub fn config_path() -> Result<PathBuf, SinkError> {
        let dir = dirs::config_dir()
            .ok_or_else(|| SinkError::Config("cannot resolve the user config directory".into()))?;
        Ok(dir.join("mixweave").join("buses.json"))
    }

    /// Load from disk. On first run (no file), the default Stream Mix bus
    /// inherits membership from the legacy per-channel `stream_mix` flags.
    pub fn load(legacy_channels: &crate::persistence::channels::Channels) -> Self {
        let path = match Self::config_path() {
            Ok(p) => p,
            Err(_) => return Self::default(),
        };
        match fs::read_to_string(&path) {
            Ok(raw) => {
                let mut buses = serde_json::from_str::<Self>(&raw).ok().unwrap_or_default();
                let channels = legacy_channels
                    .channels
                    .iter()
                    .map(|channel| channel.name.clone())
                    .collect::<Vec<_>>();
                buses.sanitize(&channels);
                buses
            }
            Err(_) => {
                let mut buses = Self::default();
                buses.buses[0].channels = legacy_channels
                    .channels
                    .iter()
                    .filter(|c| c.stream_mix)
                    .map(|c| c.name.clone())
                    .collect();
                buses
            }
        }
    }

    pub fn save(&self) -> Result<(), SinkError> {
        let path = Self::config_path()?;
        if let Some(parent) = path.parent() {
            crate::persistence::ensure_private_dir(parent)?;
        }
        let json = serde_json::to_string_pretty(self)
            .map_err(|e| SinkError::Config(format!("serialize buses: {e}")))?;
        super::write_atomic(&path, &json)?;
        Ok(())
    }

    pub fn get(&self, name: &str) -> Option<&BusDef> {
        self.buses.iter().find(|b| b.name == name)
    }

    /// Drop hand-edited bus names and memberships that could otherwise make
    /// backend commands act on nodes outside Mixweave's owned graph.
    pub fn sanitize(&mut self, channel_names: &[String]) {
        let mut seen = std::collections::HashSet::new();
        let mut user_buses = 0usize;
        self.buses.retain_mut(|bus| {
            let user = !is_protected_bus(&bus.name);
            let keep = is_bus_name(&bus.name)
                && seen.insert(bus.name.clone())
                && (!user || user_buses < MAX_BUSES);
            if !keep {
                return false;
            }
            if user {
                user_buses += 1;
            }
            let mut members = std::collections::HashSet::new();
            bus.channels.retain(|channel| {
                channel_names.contains(channel) && members.insert(channel.clone())
            });
            bus.volume_percent = bus.volume_percent.min(150);
            true
        });
    }

    /// Ensure the master mix exists, sits first, and carries every channel.
    /// Called wherever the channel set changes (init, add, profile load).
    pub fn sync_master(&mut self, channels: &[String]) {
        let mut def = match self.buses.iter().position(|b| is_master(&b.name)) {
            Some(i) => self.buses.remove(i),
            None => BusDef {
                name: DEFAULT_BUS_NODE.to_string(),
                label: "Master Mix".to_string(),
                channels: Vec::new(),
                exclude: false,
                volume_percent: 100,
                muted: false,
            },
        };
        def.channels = channels.to_vec();
        def.exclude = false;
        self.buses.insert(0, def);
    }

    /// Ensure the Streamer Mode mix exists, sits right after master, and
    /// carries every channel - mirrors `sync_master` exactly, but for the
    /// independent "stream master" control (see `streamer_gain`). Called
    /// alongside `sync_master` everywhere the channel set changes.
    pub fn sync_streamer_mode(&mut self, channels: &[String]) {
        let mut def = match self.buses.iter().position(|b| is_streamer_mode(&b.name)) {
            Some(i) => self.buses.remove(i),
            None => BusDef {
                name: STREAMER_MODE_BUS_NODE.to_string(),
                label: "Streamer Mode".to_string(),
                channels: Vec::new(),
                exclude: false,
                volume_percent: 100,
                muted: false,
            },
        };
        def.channels = channels.to_vec();
        def.exclude = false;
        let insert_at = usize::from(self.buses.first().is_some_and(|b| is_master(&b.name)));
        self.buses.insert(insert_at, def);
    }

    /// Switch a mix between manual and auto-include mode, preserving its
    /// current effective membership (the stored list flips meaning).
    pub fn set_exclude(
        &mut self,
        name: &str,
        exclude: bool,
        all_channels: &[String],
    ) -> Result<(), SinkError> {
        if is_protected_bus(name) {
            return Err(SinkError::Config(
                "this mix always carries every channel".into(),
            ));
        }
        let def = self
            .buses
            .iter_mut()
            .find(|b| b.name == name)
            .ok_or_else(|| SinkError::UnknownSink(name.to_string()))?;
        if def.exclude == exclude {
            return Ok(());
        }
        // Preserve what the mix carries: exclude mode stores the
        // complement, manual mode stores the carried set itself.
        let effective = def.effective_members(all_channels);
        def.channels = if exclude {
            all_channels
                .iter()
                .filter(|c| !effective.contains(c))
                .cloned()
                .collect()
        } else {
            effective
        };
        def.exclude = exclude;
        Ok(())
    }

    pub fn add(&mut self, label: &str) -> Result<BusDef, SinkError> {
        let label = label.trim();
        if label.is_empty() || label.len() > 24 {
            return Err(SinkError::Config(
                "mix label must be 1-24 characters".into(),
            ));
        }
        // The master and Streamer Mode mixes don't count against the user's
        // mixes.
        if self
            .buses
            .iter()
            .filter(|b| !is_protected_bus(&b.name))
            .count()
            >= MAX_BUSES
        {
            return Err(SinkError::Config(format!(
                "at most {MAX_BUSES} mixes are supported"
            )));
        }
        let base = format!("{BUS_PREFIX}{}", slugify(label));
        let mut name = base.clone();
        let mut counter = 2;
        while self.get(&name).is_some() || is_protected_bus(&name) {
            name = format!("{base}_{counter}");
            counter += 1;
        }
        // New mixes start in auto-include mode carrying everything -
        // uncheck what you don't want ("everything but music") and future
        // channels keep joining automatically.
        let def = BusDef {
            name,
            label: label.to_string(),
            channels: Vec::new(),
            exclude: true,
            volume_percent: 100,
            muted: false,
        };
        self.buses.push(def.clone());
        Ok(def)
    }

    pub fn rename(&mut self, name: &str, label: &str) -> Result<(), SinkError> {
        let label = label.trim();
        if label.is_empty() || label.len() > 24 {
            return Err(SinkError::Config(
                "mix label must be 1-24 characters".into(),
            ));
        }
        let def = self
            .buses
            .iter_mut()
            .find(|b| b.name == name)
            .ok_or_else(|| SinkError::UnknownSink(name.to_string()))?;
        def.label = label.to_string();
        Ok(())
    }

    pub fn remove(&mut self, name: &str) -> Result<(), SinkError> {
        if is_protected_bus(name) {
            return Err(SinkError::Config("this mix can't be deleted".into()));
        }
        let before = self.buses.len();
        self.buses.retain(|b| b.name != name);
        if self.buses.len() == before {
            return Err(SinkError::UnknownSink(name.to_string()));
        }
        Ok(())
    }

    pub fn set_members(&mut self, name: &str, channels: Vec<String>) -> Result<(), SinkError> {
        if is_protected_bus(name) {
            return Err(SinkError::Config(
                "this mix always carries every channel".into(),
            ));
        }
        let def = self
            .buses
            .iter_mut()
            .find(|b| b.name == name)
            .ok_or_else(|| SinkError::UnknownSink(name.to_string()))?;
        def.channels = channels;
        Ok(())
    }

    pub fn set_volume(&mut self, name: &str, volume: u8) -> Result<(), SinkError> {
        let def = self
            .buses
            .iter_mut()
            .find(|b| b.name == name)
            .ok_or_else(|| SinkError::UnknownSink(name.to_string()))?;
        def.volume_percent = volume;
        Ok(())
    }

    pub fn set_muted(&mut self, name: &str, muted: bool) -> Result<(), SinkError> {
        let def = self
            .buses
            .iter_mut()
            .find(|b| b.name == name)
            .ok_or_else(|| SinkError::UnknownSink(name.to_string()))?;
        def.muted = muted;
        Ok(())
    }

    /// Drop a deleted channel from every bus's membership.
    pub fn remove_channel(&mut self, channel: &str) {
        for bus in &mut self.buses {
            bus.channels.retain(|c| c != channel);
        }
    }
}
