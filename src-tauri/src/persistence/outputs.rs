use std::collections::{HashMap, HashSet};
use std::fs;
use std::path::PathBuf;

use serde::{Deserialize, Serialize};

use crate::error::SinkError;

/// Per-channel output device choices, stored as JSON at
/// `$XDG_CONFIG_HOME/mixweave/outputs.json`. `None` = follow the system default
/// output, with automatic failover.
#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq)]
pub struct ChannelOutputs {
    pub outputs: HashMap<String, Option<String>>,
    /// Channels with auto-failover turned off: they route only to their chosen
    /// device (or the exact system default) and stay silent when it's gone,
    /// rather than falling back to another sink - so e.g. a headset-pinned
    /// channel never surprises you by jumping to the speakers. Absence (the
    /// default) means failover is on. `serde(default)` keeps older configs,
    /// written before this field, loading cleanly.
    #[serde(default)]
    pub no_failover: HashSet<String>,
}

impl ChannelOutputs {
    pub fn config_path() -> Result<PathBuf, SinkError> {
        let dir = dirs::config_dir()
            .ok_or_else(|| SinkError::Config("cannot resolve the user config directory".into()))?;
        Ok(dir.join("mixweave").join("outputs.json"))
    }

    pub fn load() -> Self {
        let Ok(path) = Self::config_path() else {
            return Self::default();
        };
        match fs::read_to_string(&path) {
            Ok(raw) => serde_json::from_str(&raw).unwrap_or_else(|e| {
                eprintln!("mixweave: ignoring malformed {}: {e}", path.display());
                Self::default()
            }),
            Err(_) => Self::default(),
        }
    }

    pub fn save(&self) -> Result<(), SinkError> {
        let path = Self::config_path()?;
        if let Some(parent) = path.parent() {
            crate::persistence::ensure_private_dir(parent)?;
        }
        let json = serde_json::to_string_pretty(self)
            .map_err(|e| SinkError::Config(format!("serialize outputs: {e}")))?;
        super::write_atomic(&path, &json)?;
        Ok(())
    }

    pub fn set(&mut self, sink_name: &str, output: Option<String>) {
        self.outputs.insert(sink_name.to_string(), output);
    }

    pub fn get(&self, sink_name: &str) -> Option<&str> {
        self.outputs.get(sink_name)?.as_deref()
    }

    /// Whether this channel fails over to another device when its chosen
    /// device (or the default) is gone. On unless explicitly turned off.
    pub fn failover(&self, sink_name: &str) -> bool {
        !self.no_failover.contains(sink_name)
    }

    pub fn set_failover(&mut self, sink_name: &str, enabled: bool) {
        if enabled {
            self.no_failover.remove(sink_name);
        } else {
            self.no_failover.insert(sink_name.to_string());
        }
    }

    /// Drop all state for a removed channel.
    pub fn remove(&mut self, sink_name: &str) {
        self.outputs.remove(sink_name);
        self.no_failover.remove(sink_name);
    }
}
