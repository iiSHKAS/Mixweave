//! Quick control of the physical output/input devices from the Master strip:
//! their real hardware level and mute, read and written through `pactl` so the
//! value matches what the system mixer shows (not the node-level soft volume).
use crate::state::AppState;
use serde::{Deserialize, Serialize};
use std::process::Command;
use tauri::State;

#[derive(Serialize, Clone, Debug, PartialEq)]
pub struct HardwareDevice {
    pub name: String,
    pub description: String,
    /// "output" (sink) or "input" (source).
    pub kind: &'static str,
    pub volume_percent: u8,
    pub muted: bool,
    pub is_default: bool,
}

#[derive(Deserialize)]
struct PactlVolume {
    value_percent: String,
}

#[derive(Deserialize)]
struct PactlNode {
    name: String,
    #[serde(default)]
    mute: bool,
    #[serde(default)]
    volume: std::collections::HashMap<String, PactlVolume>,
}

fn pactl(args: &[&str]) -> Result<String, String> {
    let out = Command::new("pactl")
        .args(args)
        .output()
        .map_err(|e| format!("pactl: {e}"))?;
    if !out.status.success() {
        return Err(format!(
            "pactl {}: {}",
            args.join(" "),
            String::from_utf8_lossy(&out.stderr).trim()
        ));
    }
    Ok(String::from_utf8_lossy(&out.stdout).into_owned())
}

fn percent_of(node: &PactlNode) -> u8 {
    node.volume
        .values()
        .filter_map(|v| v.value_percent.trim().trim_end_matches('%').parse::<u16>().ok())
        .max()
        .unwrap_or(0)
        .min(255) as u8
}

fn list_kind(kind: &'static str) -> Result<Vec<PactlNode>, String> {
    let listing = if kind == "output" { "sinks" } else { "sources" };
    let json = pactl(&["--format=json", "list", listing])?;
    serde_json::from_str(&json).map_err(|e| format!("pactl list {listing}: {e}"))
}

fn kind_args(kind: &str) -> Result<(&'static str, &'static str), String> {
    match kind {
        "output" => Ok(("sink", "get-default-sink")),
        "input" => Ok(("source", "get-default-source")),
        _ => Err("unknown device kind".into()),
    }
}

/// Physical devices of one kind, in the same order the backend lists them.
fn devices_of(
    state: &AppState,
    kind: &'static str,
) -> Result<Vec<(String, String)>, String> {
    let devices = if kind == "output" {
        state.backend.list_output_devices()
    } else {
        state.backend.list_input_devices()
    }
    .map_err(|e| e.to_string())?;
    Ok(devices.into_iter().map(|d| (d.name, d.description)).collect())
}

#[tauri::command]
pub fn get_hardware_devices(state: State<'_, AppState>) -> Result<Vec<HardwareDevice>, String> {
    let mut result = Vec::new();
    for kind in ["output", "input"] {
        let (_, default_cmd) = kind_args(kind)?;
        let default = pactl(&[default_cmd]).map(|s| s.trim().to_string()).unwrap_or_default();
        let levels = list_kind(kind)?;
        for (name, description) in devices_of(&state, kind)? {
            let Some(node) = levels.iter().find(|n| n.name == name) else {
                continue;
            };
            result.push(HardwareDevice {
                is_default: name == default,
                volume_percent: percent_of(node),
                muted: node.mute,
                description,
                name,
                kind,
            });
        }
    }
    Ok(result)
}

fn ensure_physical(state: &AppState, kind: &'static str, name: &str) -> Result<(), String> {
    if devices_of(state, kind)?.iter().any(|(n, _)| n == name) {
        Ok(())
    } else {
        Err("Not a physical audio device".into())
    }
}

#[tauri::command]
pub fn set_hardware_volume(
    state: State<'_, AppState>,
    kind: String,
    name: String,
    percent: u8,
) -> Result<(), String> {
    let (noun, _) = kind_args(&kind)?;
    let kind: &'static str = if noun == "sink" { "output" } else { "input" };
    ensure_physical(&state, kind, &name)?;
    let level = format!("{}%", percent.min(100));
    pactl(&[&format!("set-{noun}-volume"), name.as_str(), level.as_str()]).map(|_| ())
}

#[tauri::command]
pub fn set_hardware_mute(
    state: State<'_, AppState>,
    kind: String,
    name: String,
    muted: bool,
) -> Result<(), String> {
    let (noun, _) = kind_args(&kind)?;
    let kind: &'static str = if noun == "sink" { "output" } else { "input" };
    ensure_physical(&state, kind, &name)?;
    pactl(&[
        format!("set-{noun}-mute").as_str(),
        name.as_str(),
        if muted { "1" } else { "0" },
    ])
    .map(|_| ())
}
