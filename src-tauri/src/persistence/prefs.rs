use std::fs;
use std::path::PathBuf;

use serde::{Deserialize, Serialize};

use crate::error::SinkError;

/// How Sink's devices are labeled in other apps' device lists.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum DeviceLabelStyle {
    /// "Game"
    #[default]
    Plain,
    /// "Game (Mixweave)"
    Suffix,
    /// "Mixweave · Game"
    Prefix,
}

/// Visual refresh policy for the mixer VU meters. This never changes audio
/// processing or routing; it only controls frontend rendering work.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum MeterMode {
    Monitor,
    #[serde(rename = "fps_144")]
    Fps144,
    #[serde(rename = "fps_120")]
    Fps120,
    #[serde(rename = "fps_100")]
    Fps100,
    #[default]
    #[serde(
        rename = "fps_60",
        alias = "high",
        alias = "balanced",
        alias = "low_power"
    )]
    Fps60,
    Off,
}

/// App preferences, stored at `$XDG_CONFIG_HOME/mixweave/prefs.json`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Prefs {
    /// AppImage checks, downloads and installs; never automatically restarts.
    #[serde(default = "default_true")]
    pub auto_update_enabled: bool,
    #[serde(default)]
    pub device_label_style: DeviceLabelStyle,
    /// Live meter animation rate. Every enabled rate sleeps at silence.
    #[serde(default)]
    pub meter_mode: MeterMode,
    /// First-run tutorial completed (false = show it on launch).
    #[serde(default)]
    pub onboarded: bool,
    /// ChatMix-style balance: the two channel sink names being balanced
    /// (None = auto: Game/Chat when present, else the first two channels).
    #[serde(default)]
    pub balance_a: Option<String>,
    #[serde(default)]
    pub balance_b: Option<String>,
    /// Show the balance slider in the title bar.
    #[serde(default = "default_true")]
    pub show_balance: bool,
    /// When autostarting on login, boot straight to the tray instead of
    /// showing the window (only meaningful with autostart enabled).
    #[serde(default)]
    pub start_minimized: bool,
    /// Advanced opt-in: allow profiles to publish secondary processed mics.
    #[serde(default)]
    pub multiple_mics: bool,
    /// Whether the Streamer Mode mix's live node exists at all. While false
    /// it is never created (see `commands::devices::init_virtual_devices`),
    /// so it cannot appear as a selectable device anywhere until the user
    /// turns it on - every channel's independent Stream send keeps working
    /// underneath either way (see `commands::buses::set_streamer_mode_enabled`).
    #[serde(default)]
    pub streamer_mode_enabled: bool,
}

fn default_true() -> bool {
    true
}

impl Default for Prefs {
    fn default() -> Self {
        Self {
            auto_update_enabled: true,
            device_label_style: DeviceLabelStyle::default(),
            meter_mode: MeterMode::default(),
            onboarded: false,
            balance_a: None,
            balance_b: None,
            show_balance: true,
            start_minimized: false,
            multiple_mics: false,
            streamer_mode_enabled: false,
        }
    }
}

impl Prefs {
    pub fn config_path() -> Result<PathBuf, SinkError> {
        let dir = dirs::config_dir()
            .ok_or_else(|| SinkError::Config("cannot resolve the user config directory".into()))?;
        Ok(dir.join("mixweave").join("prefs.json"))
    }

    pub fn load() -> Self {
        let Ok(path) = Self::config_path() else {
            return Self::default();
        };
        fs::read_to_string(&path)
            .map(|raw| Self::parse(&raw))
            .unwrap_or_default()
    }

    /// Parse stored prefs; malformed input degrades to defaults rather
    /// than blocking launch.
    fn parse(raw: &str) -> Self {
        serde_json::from_str(raw).unwrap_or_else(|e| {
            eprintln!("mixweave: ignoring malformed prefs: {e}");
            Self::default()
        })
    }

    pub fn save(&self) -> Result<(), SinkError> {
        let path = Self::config_path()?;
        if let Some(parent) = path.parent() {
            crate::persistence::ensure_private_dir(parent)?;
        }
        let json = serde_json::to_string_pretty(self)
            .map_err(|e| SinkError::Config(format!("serialize prefs: {e}")))?;
        super::write_atomic(&path, &json)?;
        Ok(())
    }

    /// Decorate a device label per the chosen style (applied at node
    /// creation; stored labels stay raw).
    pub fn decorate(&self, label: &str) -> String {
        match self.device_label_style {
            DeviceLabelStyle::Plain => label.to_string(),
            DeviceLabelStyle::Suffix => format!("{label} (Mixweave)"),
            DeviceLabelStyle::Prefix => format!("Mixweave · {label}"),
        }
    }
}
