use std::fs;
use std::path::PathBuf;

use serde::{Deserialize, Serialize};

use crate::error::SinkError;

/// A user-chosen display name for a discovered app, keyed by the same
/// stream identity used for routing assignments (e.g. apps like Spotify
/// that only expose a generic `media.name = "audio-src"`).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct AliasEntry {
    pub match_prop: String,
    pub match_value: String,
    pub alias: String,
}

/// All saved aliases, stored as JSON at `$XDG_CONFIG_HOME/mixweave/aliases.json`.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct Aliases {
    pub aliases: Vec<AliasEntry>,
}

impl Aliases {
    pub fn config_path() -> Result<PathBuf, SinkError> {
        let dir = dirs::config_dir()
            .ok_or_else(|| SinkError::Config("cannot resolve the user config directory".into()))?;
        Ok(dir.join("mixweave").join("aliases.json"))
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
            .map_err(|e| SinkError::Config(format!("serialize aliases: {e}")))?;
        super::write_atomic(&path, &json)?;
        Ok(())
    }

    /// Set the alias for an identity; an empty alias removes it.
    pub fn set(&mut self, match_prop: &str, match_value: &str, alias: &str) {
        let alias = alias.trim();
        if alias.is_empty() {
            self.aliases
                .retain(|a| !(a.match_prop == match_prop && a.match_value == match_value));
            return;
        }
        match self
            .aliases
            .iter_mut()
            .find(|a| a.match_prop == match_prop && a.match_value == match_value)
        {
            Some(existing) => existing.alias = alias.to_string(),
            None => self.aliases.push(AliasEntry {
                match_prop: match_prop.to_string(),
                match_value: match_value.to_string(),
                alias: alias.to_string(),
            }),
        }
    }

    pub fn get(&self, match_prop: &str, match_value: &str) -> Option<&str> {
        self.aliases
            .iter()
            .find(|a| a.match_prop == match_prop && a.match_value == match_value)
            .map(|a| a.alias.as_str())
    }
}
