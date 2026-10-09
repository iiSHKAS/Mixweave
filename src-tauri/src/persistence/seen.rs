use std::fs;
use std::path::PathBuf;

use serde::{Deserialize, Serialize};

use crate::error::SinkError;

/// How long an app the user never touched stays in the history before it is
/// forgotten. Entries carrying user intent (an assignment, an alias, or the
/// ignore flag) are exempt and kept indefinitely.
pub const MAX_SEEN_AGE_SECS: u64 = 7 * 24 * 60 * 60;

/// One app identity Sink has ever observed playing audio.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SeenEntry {
    pub match_prop: String,
    pub match_value: String,
    /// Display name at last sighting (resolver output, pre-alias).
    pub display_name: String,
    pub icon_name: Option<String>,
    /// Stable desktop application id when one could be resolved. Optional for
    /// compatibility with existing history and apps without desktop entries.
    #[serde(default)]
    pub desktop_id: Option<String>,
    /// Unix seconds of the last sighting.
    pub last_seen: u64,
    /// Ignored apps are hidden from the app list and never auto-routed.
    #[serde(default)]
    pub ignored: bool,
}

/// Registry of every app identity ever seen, stored as JSON at
/// `$XDG_CONFIG_HOME/mixweave/seen_apps.json`. Powers the inactive-apps list
/// and the ignore feature.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct SeenApps {
    pub apps: Vec<SeenEntry>,
}

impl SeenApps {
    pub fn config_path() -> Result<PathBuf, SinkError> {
        let dir = dirs::config_dir()
            .ok_or_else(|| SinkError::Config("cannot resolve the user config directory".into()))?;
        Ok(dir.join("mixweave").join("seen_apps.json"))
    }

    pub fn load() -> Self {
        let Ok(path) = Self::config_path() else {
            return Self::default();
        };
        match fs::read_to_string(&path) {
            Ok(raw) => {
                let mut seen: Self = serde_json::from_str(&raw).unwrap_or_else(|e| {
                    eprintln!("mixweave: ignoring malformed {}: {e}", path.display());
                    Self::default()
                });
                // Scrub nameless and internal/helper entries recorded before
                // those identities were filtered at the audio boundary.
                let before = seen.apps.len();
                seen.apps.retain(|a| {
                    !a.display_name.trim().is_empty()
                        && !a.match_value.trim().is_empty()
                        && !crate::audio::types::is_hidden_app_identity(
                            &a.display_name,
                            &a.match_value,
                        )
                });
                if seen.apps.len() != before {
                    if let Err(e) = seen.save() {
                        eprintln!("mixweave: could not persist cleaned app history: {e}");
                    }
                }
                seen
            }
            Err(_) => Self::default(),
        }
    }

    pub fn save(&self) -> Result<(), SinkError> {
        let path = Self::config_path()?;
        if let Some(parent) = path.parent() {
            crate::persistence::ensure_private_dir(parent)?;
        }
        let json = serde_json::to_string_pretty(self)
            .map_err(|e| SinkError::Config(format!("serialize seen apps: {e}")))?;
        super::write_atomic(&path, &json)?;
        Ok(())
    }

    fn entry_mut(&mut self, match_prop: &str, match_value: &str) -> Option<&mut SeenEntry> {
        self.apps
            .iter_mut()
            .find(|a| a.match_prop == match_prop && a.match_value == match_value)
    }

    pub fn get(&self, match_prop: &str, match_value: &str) -> Option<&SeenEntry> {
        self.apps
            .iter()
            .find(|a| a.match_prop == match_prop && a.match_value == match_value)
    }

    /// Record a sighting. Returns true when the registry changed in a way
    /// worth persisting (new identity, or display/icon changed) - pure
    /// last_seen bumps return false so the poll doesn't hit the disk.
    pub fn upsert(
        &mut self,
        match_prop: &str,
        match_value: &str,
        display_name: &str,
        icon_name: Option<&str>,
        desktop_id: Option<&str>,
        now: u64,
    ) -> bool {
        if let Some(entry) = self.entry_mut(match_prop, match_value) {
            entry.last_seen = now;
            // Resolution can temporarily lose process or desktop metadata.
            // Never erase a canonical ID on a miss; a newly resolved ID may
            // still replace it when desktop integration changes legitimately.
            let next_desktop_id = desktop_id.or(entry.desktop_id.as_deref());
            let changed = entry.display_name != display_name
                || entry.icon_name.as_deref() != icon_name
                || entry.desktop_id.as_deref() != next_desktop_id;
            if changed {
                entry.display_name = display_name.to_string();
                entry.icon_name = icon_name.map(str::to_string);
                entry.desktop_id = next_desktop_id.map(str::to_string);
            }
            changed
        } else {
            self.apps.push(SeenEntry {
                match_prop: match_prop.to_string(),
                match_value: match_value.to_string(),
                display_name: display_name.to_string(),
                icon_name: icon_name.map(str::to_string),
                desktop_id: desktop_id.map(str::to_string),
                last_seen: now,
                ignored: false,
            });
            true
        }
    }

    pub fn is_ignored(&self, match_prop: &str, match_value: &str) -> bool {
        self.get(match_prop, match_value).is_some_and(|e| e.ignored)
    }

    pub fn set_ignored(&mut self, match_prop: &str, match_value: &str, ignored: bool) -> bool {
        match self.entry_mut(match_prop, match_value) {
            Some(entry) => {
                entry.ignored = ignored;
                true
            }
            None => false,
        }
    }

    pub fn forget(&mut self, match_prop: &str, match_value: &str) {
        self.apps
            .retain(|a| !(a.match_prop == match_prop && a.match_value == match_value));
    }

    /// Drop history entries last seen over `max_age_secs` ago that the user
    /// never acted on. `has_intent` reports whether an identity carries an
    /// assignment or an alias; those and ignored entries survive forever, so
    /// a game played once a month keeps its channel. Returns true when
    /// anything was removed (i.e. the caller should persist).
    pub fn prune<F>(&mut self, now: u64, max_age_secs: u64, has_intent: F) -> bool
    where
        F: Fn(&str, &str) -> bool,
    {
        let before = self.apps.len();
        self.apps.retain(|a| {
            a.ignored
                || now.saturating_sub(a.last_seen) <= max_age_secs
                || has_intent(&a.match_prop, &a.match_value)
        });
        self.apps.len() != before
    }
}
