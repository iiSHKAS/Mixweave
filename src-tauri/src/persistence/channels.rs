use std::fs;
use std::path::PathBuf;

use serde::{Deserialize, Serialize};

use crate::error::SinkError;

/// Sink node names reserved by Sink itself (not user channels).
pub const RESERVED_SINK_NAMES: [&str; 3] = [
    "sink_mic",
    "sink_stream",
    crate::persistence::buses::STREAMER_MODE_BUS_NODE,
];
/// Upper bound on user channels (level-meter slots are budgeted for this).
pub const MAX_CHANNELS: usize = 10;

pub fn is_reserved_sink_name(name: &str) -> bool {
    RESERVED_SINK_NAMES.contains(&name)
}

/// One user-defined mixer channel.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ChannelDef {
    /// PipeWire sink node name, e.g. "sink_game". Stable once created.
    pub name: String,
    /// Display label, e.g. "Game". Renameable.
    pub label: String,
    /// Material Symbol name for the strip icon (None = legacy default).
    #[serde(default)]
    pub icon: Option<String>,
    /// Whether the channel feeds the Stream Mix source (default: yes).
    #[serde(default = "default_true")]
    pub stream_mix: bool,
}

fn default_true() -> bool {
    true
}

/// The user's channel set, stored as JSON at
/// `$XDG_CONFIG_HOME/mixweave/channels.json`. Defaults to the classic four.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Channels {
    pub channels: Vec<ChannelDef>,
}

impl Default for Channels {
    fn default() -> Self {
        let def = |name: &str, label: &str, icon: &str| ChannelDef {
            name: name.to_string(),
            label: label.to_string(),
            icon: Some(icon.to_string()),
            stream_mix: true,
        };
        Self {
            channels: vec![
                def("sink_game", "Game", "sports_esports"),
                def("sink_chat", "Chat", "forum"),
                def("sink_media", "Media", "music_note"),
                def("sink_aux", "Aux", "cable"),
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
        "channel".to_string()
    } else {
        slug
    }
}

impl Channels {
    pub fn config_path() -> Result<PathBuf, SinkError> {
        let dir = dirs::config_dir()
            .ok_or_else(|| SinkError::Config("cannot resolve the user config directory".into()))?;
        Ok(dir.join("mixweave").join("channels.json"))
    }

    pub fn load() -> Self {
        let Ok(path) = Self::config_path() else {
            return Self::default();
        };
        // A missing file is first run (silent default); a present-but-broken
        // one is a torn or hand-edited write we log rather than honour.
        let raw = match fs::read_to_string(&path) {
            Ok(raw) => raw,
            Err(_) => return Self::default(),
        };
        match Self::parse(&raw) {
            Ok(channels) => channels,
            Err(error) => {
                eprintln!("mixweave: channels.json is invalid ({error}); using defaults");
                Self::default()
            }
        }
    }

    /// Parse and validate the complete channel set. Reject the whole file when
    /// any entry breaks the invariants `add()` guarantees, rather than silently
    /// adopting a partial layout that no longer matches the other live files.
    pub(crate) fn parse(raw: &str) -> Result<Self, SinkError> {
        let mut parsed: Self = serde_json::from_str(raw)
            .map_err(|error| SinkError::Config(format!("malformed channels: {error}")))?;
        parsed.normalize_and_validate()?;
        Ok(parsed)
    }

    pub(crate) fn normalize_and_validate(&mut self) -> Result<(), SinkError> {
        if self.channels.is_empty() || self.channels.len() > MAX_CHANNELS {
            return Err(SinkError::Config(format!(
                "channel set must contain 1-{MAX_CHANNELS} channels"
            )));
        }
        let mut seen = std::collections::HashSet::new();
        for channel in &mut self.channels {
            validate_channel_name(&channel.name)?;
            if !seen.insert(channel.name.clone()) {
                return Err(SinkError::Config(format!(
                    "duplicate channel name: {}",
                    channel.name
                )));
            }
            let label = channel.label.trim();
            if label.is_empty() || label.len() > 24 {
                return Err(SinkError::Config(format!(
                    "channel label must be 1-24 characters: {}",
                    channel.name
                )));
            }
            channel.label = label.to_string();
        }
        Ok(())
    }

    pub fn save(&self) -> Result<(), SinkError> {
        let mut validated = self.clone();
        validated.normalize_and_validate()?;
        let path = Self::config_path()?;
        if let Some(parent) = path.parent() {
            crate::persistence::ensure_private_dir(parent)?;
        }
        let json = serde_json::to_string_pretty(&validated)
            .map_err(|e| SinkError::Config(format!("serialize channels: {e}")))?;
        super::write_atomic(&path, &json)?;
        Ok(())
    }

    pub fn get(&self, name: &str) -> Option<&ChannelDef> {
        self.channels.iter().find(|c| c.name == name)
    }

    pub fn set_icon(&mut self, name: &str, icon: Option<String>) -> Result<(), SinkError> {
        let def = self
            .channels
            .iter_mut()
            .find(|c| c.name == name)
            .ok_or_else(|| SinkError::UnknownSink(name.to_string()))?;
        def.icon = icon;
        Ok(())
    }

    /// Add a channel for `label`, generating a unique reserved-safe sink
    /// name. Returns the new definition.
    pub fn add_with_spatial(
        &mut self,
        label: &str,
        icon: Option<String>,
        spatial: bool,
    ) -> Result<ChannelDef, SinkError> {
        let label = label.trim();
        if label.is_empty() || label.len() > 24 {
            return Err(SinkError::Config(
                "channel label must be 1-24 characters".into(),
            ));
        }
        if self.channels.len() >= MAX_CHANNELS {
            return Err(SinkError::Config(format!(
                "at most {MAX_CHANNELS} channels are supported"
            )));
        }
        let prefix = if spatial { "sink_spatial_" } else { "sink_" };
        let mut base = format!("{prefix}{}", slugify(label));
        if is_reserved_sink_name(&base)
            || (!spatial && matches!(base.as_str(), "sink_game" | "sink_media"))
        {
            base = format!("{prefix}channel_{}", slugify(label));
        }
        let mut name = base.clone();
        let mut counter = 2;
        while self.get(&name).is_some() || is_reserved_sink_name(&name) {
            name = format!("{base}_{counter}");
            counter += 1;
        }
        let def = ChannelDef {
            name,
            label: label.to_string(),
            icon,
            stream_mix: true,
        };
        self.channels.push(def.clone());
        Ok(def)
    }

    pub fn rename(&mut self, name: &str, label: &str) -> Result<(), SinkError> {
        let label = label.trim();
        if label.is_empty() || label.len() > 24 {
            return Err(SinkError::Config(
                "channel label must be 1-24 characters".into(),
            ));
        }
        let def = self
            .channels
            .iter_mut()
            .find(|c| c.name == name)
            .ok_or_else(|| SinkError::UnknownSink(name.to_string()))?;
        def.label = label.to_string();
        Ok(())
    }

    /// Reorder the channel set. `order` must contain exactly the current
    /// sink names (it's a permutation, not an edit).
    pub fn reorder(&mut self, order: &[String]) -> Result<(), SinkError> {
        if order.len() != self.channels.len() || !order.iter().all(|n| self.get(n).is_some()) {
            return Err(SinkError::Config(
                "reorder must list every existing channel exactly once".into(),
            ));
        }
        self.channels.sort_by_key(|c| {
            order
                .iter()
                .position(|n| n == &c.name)
                .unwrap_or(usize::MAX)
        });
        Ok(())
    }

    pub fn remove(&mut self, name: &str) -> Result<(), SinkError> {
        if self.channels.len() <= 1 {
            return Err(SinkError::Config("at least one channel is required".into()));
        }
        let before = self.channels.len();
        self.channels.retain(|c| c.name != name);
        if self.channels.len() == before {
            return Err(SinkError::UnknownSink(name.to_string()));
        }
        Ok(())
    }
}

pub(crate) fn validate_channel_name(name: &str) -> Result<(), SinkError> {
    let suffix = name.strip_prefix("sink_");
    if suffix.is_none_or(|suffix| {
        suffix.is_empty()
            || name.len() > 64
            || !suffix
                .chars()
                .all(|character| character.is_ascii_alphanumeric() || character == '_')
    }) || is_reserved_sink_name(name)
    {
        return Err(SinkError::Config(format!("invalid channel name: {name}")));
    }
    Ok(())
}
