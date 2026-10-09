use std::fs;
use std::path::PathBuf;

use serde::{Deserialize, Serialize};

use crate::error::SinkError;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ApplicationRule {
    pub executable: String,
    #[serde(default)]
    pub path: Option<String>,
    pub profile: String,
    #[serde(default = "default_true")]
    pub enabled: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ProfileAutomationConfig {
    #[serde(default)]
    pub enabled: bool,
    #[serde(default)]
    pub return_profile: Option<String>,
    #[serde(default = "default_true")]
    pub notifications: bool,
    #[serde(default)]
    pub rules: Vec<ApplicationRule>,
}

impl Default for ProfileAutomationConfig {
    fn default() -> Self {
        Self {
            enabled: false,
            return_profile: None,
            notifications: true,
            rules: Vec::new(),
        }
    }
}

fn default_true() -> bool {
    true
}

fn path() -> Result<PathBuf, SinkError> {
    let dir = dirs::config_dir()
        .ok_or_else(|| SinkError::Config("cannot resolve the user config directory".into()))?
        .join("mixweave");
    Ok(dir.join("profile_automation.json"))
}

pub fn load() -> ProfileAutomationConfig {
    path()
        .ok()
        .and_then(|path| fs::read_to_string(path).ok())
        .and_then(|raw| serde_json::from_str(&raw).ok())
        .unwrap_or_default()
}

pub fn save(config: &ProfileAutomationConfig) -> Result<(), SinkError> {
    validate(config)?;
    let path = path()?;
    if let Some(parent) = path.parent() {
        super::ensure_private_dir(parent)?;
    }
    let json = serde_json::to_string_pretty(config)
        .map_err(|error| SinkError::Config(format!("serialize profile automation: {error}")))?;
    super::write_atomic(&path, json).map_err(Into::into)
}

pub fn validate(config: &ProfileAutomationConfig) -> Result<(), SinkError> {
    let profile_names = super::profiles::list()?
        .into_iter()
        .map(|profile| profile.name)
        .collect::<Vec<_>>();
    validate_with_profiles(config, &profile_names)
}

pub(crate) fn validate_with_profiles(
    config: &ProfileAutomationConfig,
    profile_names: &[String],
) -> Result<(), SinkError> {
    if let Some(name) = &config.return_profile {
        if !profile_names.contains(name) {
            return Err(SinkError::Config(format!("no such return profile: {name}")));
        }
    }
    let mut executables = std::collections::HashSet::new();
    for rule in &config.rules {
        let executable = rule.executable.trim();
        if executable.is_empty()
            || executable.len() > 255
            || executable
                .chars()
                .any(|character| matches!(character, '/' | '\\' | '\0'))
        {
            return Err(SinkError::Config(
                "application executable must be a 1-255 character file name".into(),
            ));
        }
        if !profile_names.contains(&rule.profile) {
            return Err(SinkError::Config(format!(
                "no such profile for {executable}: {}",
                rule.profile
            )));
        }
        if !executables.insert(executable.to_ascii_lowercase()) {
            return Err(SinkError::Config(format!(
                "{executable} already has a profile rule"
            )));
        }
    }
    Ok(())
}
