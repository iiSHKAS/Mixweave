use std::collections::HashMap;
use std::fs;
use std::path::PathBuf;

use serde::{Deserialize, Serialize};

use crate::audio::types::EqConfig;
use crate::error::SinkError;

/// Per-channel parametric EQ configs, stored as JSON at
/// `$XDG_CONFIG_HOME/mixweave/eq.json`. A missing entry means "never touched" -
/// the default (disabled, flat) config.
#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq)]
pub struct ChannelEq {
    /// `serde(default)` keeps pre-EQ profile files loading cleanly.
    #[serde(default)]
    pub configs: HashMap<String, EqConfig>,
}

impl ChannelEq {
    pub fn config_path() -> Result<PathBuf, SinkError> {
        let dir = dirs::config_dir()
            .ok_or_else(|| SinkError::Config("cannot resolve the user config directory".into()))?;
        Ok(dir.join("mixweave").join("eq.json"))
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
            .map_err(|e| SinkError::Config(format!("serialize eq: {e}")))?;
        super::write_atomic(&path, &json)?;
        Ok(())
    }

    /// A channel's EQ, defaulting to disabled/flat when never configured.
    pub fn get(&self, sink_name: &str) -> EqConfig {
        self.configs.get(sink_name).cloned().unwrap_or_default()
    }

    pub fn set(&mut self, sink_name: &str, config: EqConfig) {
        self.configs.insert(sink_name.to_string(), config);
    }

    /// Drop all state for a removed channel.
    pub fn remove(&mut self, sink_name: &str) {
        self.configs.remove(sink_name);
    }
}
