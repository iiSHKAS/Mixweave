use std::collections::HashSet;
use std::fs;
use std::path::PathBuf;

use serde::{Deserialize, Serialize};

use crate::audio::types::VirtualSink;
use crate::error::SinkError;
use crate::persistence::assignments::Assignments;

pub const MAX_MIC_CHANNELS: usize = 4;

/// A named snapshot of the mixer: channel volumes/mutes, the app→channel
/// assignment set, and per-channel output choices. Stored as JSON in
/// `$XDG_CONFIG_HOME/mixweave/profiles/<name>.json`.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Profile {
    pub name: String,
    /// The installation's original fallback profile. At least one profile is
    /// promoted to this role on startup and it cannot be deleted.
    #[serde(default)]
    pub protected: bool,
    pub channels: Vec<VirtualSink>,
    /// None only for profiles written before microphone settings became
    /// profile-scoped; those inherit the current global mic on first load.
    #[serde(default)]
    pub mic: Option<crate::audio::types::MicConfig>,
    /// Additional processed microphones. The legacy `mic` remains primary.
    #[serde(default)]
    pub secondary_mics: Vec<crate::audio::types::MicConfig>,
    pub assignments: Assignments,
    /// Default output assignments keep older profile files loadable.
    #[serde(default)]
    pub outputs: crate::persistence::outputs::ChannelOutputs,
    /// Per-channel parametric EQ; default keeps older profile files loadable.
    #[serde(default)]
    pub eq: crate::persistence::eq::ChannelEq,
    /// Output device (node.name) whose appearance auto-loads this
    /// profile - automatic hardware profile switching.
    #[serde(default)]
    pub trigger_device: Option<String>,
    /// User-defined mixes (record buses) with their member channels.
    #[serde(default)]
    pub buses: crate::persistence::buses::Buses,
}

/// Listing entry: name plus trigger metadata for the UI/auto-switcher.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProfileInfo {
    pub name: String,
    pub trigger_device: Option<String>,
    pub protected: bool,
}

fn profiles_dir() -> Result<PathBuf, SinkError> {
    let dir = dirs::config_dir()
        .ok_or_else(|| SinkError::Config("cannot resolve the user config directory".into()))?;
    Ok(dir.join("mixweave").join("profiles"))
}

/// Profile names become file names: restrict to a safe charset so a name
/// can never traverse out of the profiles directory.
pub fn sanitize_name(name: &str) -> Result<String, SinkError> {
    let trimmed = name.trim();
    if trimmed.is_empty() || trimmed.len() > 64 {
        return Err(SinkError::Config(
            "profile name must be 1-64 characters".into(),
        ));
    }
    if !trimmed
        .chars()
        .all(|c| c.is_alphanumeric() || c == ' ' || c == '-' || c == '_')
    {
        return Err(SinkError::Config(
            "profile name may only contain letters, digits, spaces, '-' and '_'".into(),
        ));
    }
    Ok(trimmed.to_string())
}

pub fn validate_mic_channels(profile: &Profile) -> Result<(), SinkError> {
    if profile
        .mic
        .as_ref()
        .is_some_and(|mic| mic.node_name != "sink_mic")
    {
        return Err(SinkError::Config(
            "primary microphone must use the sink_mic node".into(),
        ));
    }
    // Legacy profiles may omit `mic`, but still inherit one primary channel
    // when applied, so always reserve one slot for it.
    if profile.secondary_mics.len() + 1 > MAX_MIC_CHANNELS {
        return Err(SinkError::Config(format!(
            "at most {MAX_MIC_CHANNELS} microphone channels are supported"
        )));
    }
    let mut names = std::collections::HashSet::new();
    for mic in &profile.secondary_mics {
        let suffix = mic.node_name.strip_prefix("source_mic_");
        if suffix.is_none_or(|suffix| {
            suffix.is_empty()
                || mic.node_name.len() > 64
                || !suffix
                    .chars()
                    .all(|character| character.is_ascii_alphanumeric() || character == '_')
        }) || !names.insert(mic.node_name.as_str())
        {
            return Err(SinkError::Config(format!(
                "invalid or duplicate secondary microphone node: {}",
                mic.node_name
            )));
        }
    }
    Ok(())
}

/// Enforce the invariants guaranteed by the normal channel/mic commands and
/// normalize bounded controls before a profile reaches an audio backend.
/// Structural errors are rejected; stale per-channel settings are discarded.
pub(crate) fn normalize_and_validate(profile: &mut Profile) -> Result<(), SinkError> {
    let safe_name = sanitize_name(&profile.name)?;
    if safe_name != profile.name {
        return Err(SinkError::Config(
            "profile name must not contain surrounding whitespace".into(),
        ));
    }

    if profile.channels.is_empty()
        || profile.channels.len() > crate::persistence::channels::MAX_CHANNELS
    {
        return Err(SinkError::Config(format!(
            "profile must contain 1-{} channels",
            crate::persistence::channels::MAX_CHANNELS
        )));
    }
    let mut channel_names = HashSet::new();
    for channel in &mut profile.channels {
        crate::persistence::channels::validate_channel_name(&channel.name)?;
        if !channel_names.insert(channel.name.clone()) {
            return Err(SinkError::Config(format!(
                "invalid or duplicate profile channel: {}",
                channel.name
            )));
        }
        let trimmed_label = channel.label.trim();
        if trimmed_label.is_empty() || trimmed_label.len() > 24 {
            return Err(SinkError::Config(format!(
                "invalid label for profile channel: {}",
                channel.name
            )));
        }
        channel.label = trimmed_label.to_string();
        channel.volume_percent = channel.volume_percent.min(150);
    }

    validate_mic_channels(profile)?;
    if let Some(mic) = &mut profile.mic {
        mic.clamp_ranges();
    }
    for mic in &mut profile.secondary_mics {
        mic.clamp_ranges();
    }

    if profile
        .assignments
        .assignments
        .iter()
        .any(|assignment| !channel_names.contains(&assignment.sink_name))
    {
        return Err(SinkError::Config(
            "profile contains an assignment to a missing channel".into(),
        ));
    }
    profile
        .outputs
        .outputs
        .retain(|channel, _| channel_names.contains(channel));
    profile
        .outputs
        .no_failover
        .retain(|channel| channel_names.contains(channel));
    profile
        .eq
        .configs
        .retain(|channel, _| channel_names.contains(channel));
    for config in profile.eq.configs.values_mut() {
        config.clamp_ranges();
    }
    let channel_names = profile
        .channels
        .iter()
        .map(|channel| channel.name.clone())
        .collect::<Vec<_>>();
    profile.buses.sanitize(&channel_names);
    Ok(())
}

fn profile_path(name: &str) -> Result<PathBuf, SinkError> {
    Ok(profiles_dir()?.join(format!("{}.json", sanitize_name(name)?)))
}

/// File existence is intentionally separate from profile validity. Creation
/// and rename must never overwrite a malformed profile that the user may want
/// to repair or recover.
pub fn exists(name: &str) -> Result<bool, SinkError> {
    profile_path(name)?.try_exists().map_err(Into::into)
}

pub fn has_any_profile_files() -> Result<bool, SinkError> {
    let dir = profiles_dir()?;
    has_profile_files_in(&dir)
}

fn has_profile_files_in(dir: &std::path::Path) -> Result<bool, SinkError> {
    let entries = match fs::read_dir(dir) {
        Ok(entries) => entries,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(false),
        Err(error) => return Err(error.into()),
    };
    for entry in entries {
        let entry = entry?;
        if entry.file_type()?.is_file()
            && entry
                .path()
                .extension()
                .is_some_and(|extension| extension == "json")
        {
            return Ok(true);
        }
    }
    Ok(false)
}

pub fn list() -> Result<Vec<ProfileInfo>, SinkError> {
    let dir = profiles_dir()?;
    let mut infos = Vec::new();
    let entries = match fs::read_dir(&dir) {
        Ok(e) => e,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(infos),
        Err(e) => return Err(e.into()),
    };
    for entry in entries.flatten() {
        let path = entry.path();
        if path.extension().is_some_and(|ext| ext == "json") {
            if let Some(stem) = path.file_stem().and_then(|s| s.to_str()) {
                if let Some(info) = fs::read_to_string(&path)
                    .ok()
                    .and_then(|raw| profile_info_from_json(stem, &raw))
                {
                    infos.push(info);
                }
            }
        }
    }
    infos.sort_by(|a, b| a.name.cmp(&b.name));
    Ok(infos)
}

fn profile_info_from_json(stem: &str, raw: &str) -> Option<ProfileInfo> {
    let safe_name = sanitize_name(stem).ok()?;
    let mut profile: Profile = serde_json::from_str(raw).ok()?;
    if safe_name != stem || profile.name != stem {
        return None;
    }
    migrate_legacy_mic_nodes(&mut profile);
    normalize_and_validate(&mut profile).ok()?;
    Some(ProfileInfo {
        name: profile.name,
        trigger_device: profile.trigger_device,
        protected: profile.protected,
    })
}

/// Set or clear the trigger device on an existing profile file.
pub fn set_trigger(name: &str, trigger_device: Option<String>) -> Result<(), SinkError> {
    if let Some(device) = trigger_device.as_deref() {
        let infos = list()?;
        ensure_trigger_available(name, device, &infos)?;
    }
    let mut profile = load(name)?;
    profile.trigger_device = trigger_device;
    save(&profile)
}

fn ensure_trigger_available(
    name: &str,
    device: &str,
    profiles: &[ProfileInfo],
) -> Result<(), SinkError> {
    if let Some(owner) = profiles
        .iter()
        .find(|profile| profile.name != name && profile.trigger_device.as_deref() == Some(device))
    {
        return Err(SinkError::Config(format!(
            "device is already assigned to profile \"{}\"",
            owner.name
        )));
    }
    Ok(())
}

pub fn save(profile: &Profile) -> Result<(), SinkError> {
    let mut profile = profile.clone();
    normalize_and_validate(&mut profile)?;
    let path = profile_path(&profile.name)?;
    if let Some(parent) = path.parent() {
        crate::persistence::ensure_private_dir(parent)?;
    }
    let json = serde_json::to_string_pretty(&profile)
        .map_err(|e| SinkError::Config(format!("serialize profile: {e}")))?;
    super::write_atomic(&path, &json)?;
    Ok(())
}

pub fn load(name: &str) -> Result<Profile, SinkError> {
    let path = profile_path(name)?;
    let raw = fs::read_to_string(&path).map_err(|e| {
        if e.kind() == std::io::ErrorKind::NotFound {
            SinkError::Config(format!("no such profile: {name}"))
        } else {
            e.into()
        }
    })?;
    let mut profile: Profile = serde_json::from_str(&raw)
        .map_err(|e| SinkError::Config(format!("malformed profile {name}: {e}")))?;
    if profile.name != name {
        return Err(SinkError::Config(format!(
            "profile name does not match its file: {name}"
        )));
    }
    migrate_legacy_mic_nodes(&mut profile);
    normalize_and_validate(&mut profile)?;
    Ok(profile)
}

pub(crate) fn migrate_legacy_mic_nodes(profile: &mut Profile) {
    // Early multiple-mic builds used the playback-sink namespace for
    // secondary virtual sources. Move them into a distinct source namespace
    // so a user output channel can never collide with a microphone.
    for mic in &mut profile.secondary_mics {
        if let Some(suffix) = mic.node_name.strip_prefix("sink_mic_") {
            mic.node_name = format!("source_mic_{suffix}");
        }
    }
}

pub fn delete(name: &str) -> Result<(), SinkError> {
    let path = profile_path(name)?;
    super::remove_file(&path).map_err(|e| {
        if e.kind() == std::io::ErrorKind::NotFound {
            SinkError::Config(format!("no such profile: {name}"))
        } else {
            e.into()
        }
    })
}
