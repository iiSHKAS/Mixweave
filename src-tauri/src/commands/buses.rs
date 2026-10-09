use tauri::State;

use crate::audio::backend::AudioBackend;
use crate::commands::routing::MAX_VOLUME;
use crate::persistence::buses::BusDef;
use crate::state::AppState;

pub(crate) fn set_bus_level(
    backend: &dyn AudioBackend,
    def: &BusDef,
) -> Result<(), crate::error::SinkError> {
    if crate::persistence::buses::is_master(&def.name) {
        // The master mix is a true listening-volume control: its level/mute
        // are applied to every channel's own live sink (see
        // `push_master_gain_to_channels`), so its own node must stay a fixed
        // passthrough rather than double-attenuating what it captures.
        backend.set_sink_volume(&def.name, 100)?;
        backend.set_sink_mute(&def.name, false)?;
        return Ok(());
    }
    if crate::persistence::buses::is_streamer_mode(&def.name) {
        // Same passthrough reasoning as the master mix above, but for the
        // independent Stream path (see `push_streamer_gain_to_channels`).
        backend.set_sink_volume(&def.name, 100)?;
        backend.set_sink_mute(&def.name, false)?;
        return Ok(());
    }
    // Existing nodes may carry values from another profile. Defaults are
    // values too: omitting 100% or false would leave an old level/mute live.
    backend.set_sink_volume(&def.name, def.volume_percent)?;
    backend.set_sink_mute(&def.name, def.muted)?;
    Ok(())
}

/// Push a newly (re)computed master gain fraction/mute to every channel's
/// live sink, so a master volume/mute change is immediately heard as a
/// rescale of each channel's own level rather than a change to a separate
/// recording-only node.
fn push_master_gain_to_channels(
    backend: &dyn AudioBackend,
    channels: &[crate::audio::types::VirtualSink],
    fraction: f32,
    master_muted: bool,
) -> Result<(), crate::error::SinkError> {
    for channel in channels {
        crate::commands::routing::push_channel_controls(
            backend,
            &channel.name,
            channel.volume_percent,
            channel.muted,
            fraction,
            master_muted,
        )?;
    }
    Ok(())
}

/// Push a newly (re)computed Streamer Mode gain fraction/mute to every
/// channel's live Stream-send sink - the exact same idea as
/// `push_master_gain_to_channels`, entirely independent of it.
fn push_streamer_gain_to_channels(
    backend: &dyn AudioBackend,
    channels: &[crate::audio::types::VirtualSink],
    fraction: f32,
    streamer_muted: bool,
) -> Result<(), crate::error::SinkError> {
    for channel in channels {
        crate::commands::routing::push_channel_stream_controls(
            backend,
            &channel.name,
            channel.stream_send_volume_percent,
            channel.stream_send_muted,
            fraction,
            streamer_muted,
        )?;
    }
    Ok(())
}

fn configure_existing_bus(
    backend: &dyn AudioBackend,
    def: &BusDef,
    all_channels: &[String],
) -> Result<(), crate::error::SinkError> {
    backend.set_bus_members(&def.name, &def.effective_members(all_channels))?;
    set_bus_level(backend, def)
}

fn restore_bus(
    backend: &dyn AudioBackend,
    def: &BusDef,
    label: &str,
    all_channels: &[String],
) -> Result<(), crate::error::SinkError> {
    backend.set_bus_label(&def.name, label)?;
    configure_existing_bus(backend, def, all_channels)
}

fn rename_failure(
    error: impl std::fmt::Display,
    rollback: Result<(), crate::error::SinkError>,
) -> String {
    match rollback {
        Ok(()) => error.to_string(),
        Err(rollback_error) => {
            format!("{error}; restoring the previous mix also failed: {rollback_error}")
        }
    }
}

fn mutation_failure(
    error: impl std::fmt::Display,
    rollbacks: &[(&str, Result<(), crate::error::SinkError>)],
) -> String {
    let mut message = error.to_string();
    for (action, rollback) in rollbacks {
        if let Err(rollback_error) = rollback {
            message.push_str(&format!("; {action} also failed: {rollback_error}"));
        }
    }
    message
}

fn persist_bus_edit(
    mixer: &crate::mixer::state::MixerState,
    old_defs: &crate::persistence::buses::Buses,
    defs: &crate::persistence::buses::Buses,
    rollback_action: &str,
    rollback_live: impl Fn() -> Result<(), crate::error::SinkError>,
) -> Result<(), String> {
    if let Err(error) = defs.save() {
        return Err(mutation_failure(
            error,
            &[
                ("restoring the previous mix definitions", old_defs.save()),
                (rollback_action, rollback_live()),
            ],
        ));
    }
    if let Err(error) = crate::commands::profiles::save_active_with_buses(mixer, defs) {
        return Err(mutation_failure(
            error,
            &[
                (
                    "restoring the previous active profile",
                    crate::commands::profiles::save_active_with_buses(mixer, old_defs),
                ),
                ("restoring the previous mix definitions", old_defs.save()),
                (rollback_action, rollback_live()),
            ],
        ));
    }
    Ok(())
}

fn restore_removed_bus(
    backend: &dyn AudioBackend,
    def: &BusDef,
    label: &str,
    all_channels: &[String],
) -> Result<(), crate::error::SinkError> {
    // PipeWire removes globals asynchronously. A failed delete transaction
    // can reach rollback while the old global is still disappearing, so use
    // the bounded destroy/recreate path rather than a one-shot create.
    backend.set_bus_label(&def.name, label)?;
    if let Err(error) = configure_existing_bus(backend, def, all_channels) {
        return match backend.destroy_bus(&def.name) {
            Ok(()) => Err(error),
            Err(cleanup_error) => Err(crate::error::SinkError::Config(format!(
                "{error}; removing the incomplete restored mix also failed: {cleanup_error}"
            ))),
        };
    }
    Ok(())
}

fn validate_member_channels(channels: &[String], all_channels: &[String]) -> Result<(), String> {
    let mut seen = std::collections::HashSet::new();
    for channel in channels {
        if !all_channels.contains(channel) {
            return Err(format!("unknown channel in mix membership: {channel}"));
        }
        if !seen.insert(channel) {
            return Err(format!("duplicate channel in mix membership: {channel}"));
        }
    }
    Ok(())
}

/// The user's mixes (buses) with their member channels.
#[tauri::command]
pub fn list_buses(state: State<'_, AppState>) -> Result<Vec<BusDef>, String> {
    let mixer = state.lock_mixer()?;
    Ok(mixer.buses.buses.clone())
}

/// Create a new mix. Recorders see it under `label`. New mixes carry
/// every channel (auto-include) until the user unchecks some.
#[tauri::command]
pub fn add_bus(
    state: State<'_, AppState>,
    label: String,
    expected_profile: Option<String>,
) -> Result<(), String> {
    let _profile_operation = state.lock_expected_profile_operation(expected_profile.as_deref())?;
    let (def, old_defs, defs, prefs, all) = {
        let mixer = state.lock_mixer()?;
        let old_defs = mixer.buses.clone();
        let mut defs = old_defs.clone();
        let def = defs.add(&label).map_err(|e| e.to_string())?;
        (
            def,
            old_defs,
            defs,
            mixer.prefs.clone(),
            channel_names(&mixer),
        )
    };
    if let Err(e) = state
        .backend
        .create_bus(&def.name, &prefs.decorate(&def.label))
    {
        return Err(e.to_string());
    }
    if let Err(error) = configure_existing_bus(state.backend.as_ref(), &def, &all) {
        return Err(mutation_failure(
            error,
            &[(
                "removing the incomplete mix",
                state.backend.destroy_bus(&def.name),
            )],
        ));
    }
    if let Err(error) = defs.save() {
        return Err(mutation_failure(
            error,
            &[
                ("restoring the previous mix definitions", old_defs.save()),
                (
                    "removing the unpersisted mix",
                    state.backend.destroy_bus(&def.name),
                ),
            ],
        ));
    }
    {
        let mixer = state.lock_mixer()?;
        if let Err(error) = crate::commands::profiles::save_active_with_buses(&mixer, &defs) {
            return Err(mutation_failure(
                error,
                &[
                    (
                        "restoring the previous active profile",
                        crate::commands::profiles::save_active_with_buses(&mixer, &old_defs),
                    ),
                    ("restoring the previous mix definitions", old_defs.save()),
                    (
                        "removing the unpersisted mix",
                        state.backend.destroy_bus(&def.name),
                    ),
                ],
            ));
        }
    }
    let mut mixer = state.lock_mixer()?;
    mixer.buses = defs;
    Ok(())
}

/// Rename a mix. The node is recreated so recorders immediately see the
/// new name (the node name stays stable, so OBS configs keep working -
/// capture re-attaches automatically).
#[tauri::command]
pub fn rename_bus(
    state: State<'_, AppState>,
    name: String,
    label: String,
    expected_profile: Option<String>,
) -> Result<(), String> {
    let _profile_operation = state.lock_expected_profile_operation(expected_profile.as_deref())?;
    let (old_def, def, old_defs, defs, prefs, all) = {
        let mixer = state.lock_mixer()?;
        let old_def = mixer
            .buses
            .get(&name)
            .cloned()
            .ok_or_else(|| "unknown mix".to_string())?;
        let old_defs = mixer.buses.clone();
        let mut defs = old_defs.clone();
        defs.rename(&name, &label).map_err(|e| e.to_string())?;
        let def = defs
            .get(&name)
            .cloned()
            .ok_or_else(|| "unknown mix".to_string())?;
        (
            old_def,
            def,
            old_defs,
            defs,
            mixer.prefs.clone(),
            channel_names(&mixer),
        )
    };

    let old_label = prefs.decorate(&old_def.label);
    let new_label = prefs.decorate(&def.label);

    let rename_result = state
        .backend
        .set_bus_label(&name, &new_label)
        .and_then(|()| configure_existing_bus(state.backend.as_ref(), &def, &all));
    if let Err(error) = rename_result {
        return Err(rename_failure(
            error,
            restore_bus(state.backend.as_ref(), &old_def, &old_label, &all),
        ));
    }

    {
        let mixer = state.lock_mixer()?;
        persist_bus_edit(
            &mixer,
            &old_defs,
            &defs,
            "restoring the previous live mix",
            || restore_bus(state.backend.as_ref(), &old_def, &old_label, &all),
        )?;
    }
    let mut mixer = state.lock_mixer()?;
    mixer.buses = defs;
    Ok(())
}

/// Turn the Streamer Mode mix's live node on or off. Its definition (level,
/// membership) always exists and keeps updating in the background - this
/// only gates whether the node itself is present at all, which is what
/// keeps it out of every device picker (system, OBS, ...) until the user
/// asks for it (see `commands::devices::init_virtual_devices`'s matching
/// startup check). Every channel's own independent Stream send keeps
/// running underneath regardless; there is simply nothing listening on the
/// other end while this is off. Persisted, so the state survives a restart.
#[tauri::command]
pub fn set_streamer_mode_enabled(state: State<'_, AppState>, enabled: bool) -> Result<(), String> {
    let (def, all, prefs) = {
        let mixer = state.lock_mixer()?;
        let def = mixer
            .buses
            .get(crate::persistence::buses::STREAMER_MODE_BUS_NODE)
            .cloned()
            .ok_or_else(|| "Streamer Mode mix definition is missing".to_string())?;
        (def, channel_names(&mixer), mixer.prefs.clone())
    };
    if enabled {
        state
            .backend
            .create_bus(&def.name, &prefs.decorate(&def.label))
            .map_err(|e| e.to_string())?;
        if let Err(error) = configure_existing_bus(state.backend.as_ref(), &def, &all) {
            return Err(mutation_failure(
                error,
                &[(
                    "removing the incomplete mix",
                    state.backend.destroy_bus(&def.name),
                )],
            ));
        }
    } else {
        state
            .backend
            .destroy_bus(&def.name)
            .map_err(|e| e.to_string())?;
    }
    let mut mixer = state.lock_mixer()?;
    mixer.prefs.streamer_mode_enabled = enabled;
    mixer.prefs.save().map_err(|e| e.to_string())
}

/// Delete a mix.
#[tauri::command]
pub fn remove_bus(
    state: State<'_, AppState>,
    name: String,
    expected_profile: Option<String>,
) -> Result<(), String> {
    let _profile_operation = state.lock_expected_profile_operation(expected_profile.as_deref())?;
    state.ensure_known_bus(&name)?;
    if crate::persistence::buses::is_master(&name) {
        return Err("the master mix can't be deleted".to_string());
    }
    let (old_def, old_label, old_defs, defs, all) = {
        let mixer = state.lock_mixer()?;
        let old_def = mixer
            .buses
            .get(&name)
            .cloned()
            .ok_or_else(|| "unknown mix".to_string())?;
        let old_label = mixer.prefs.decorate(&old_def.label);
        let old_defs = mixer.buses.clone();
        let mut defs = old_defs.clone();
        defs.remove(&name).map_err(|e| e.to_string())?;
        (old_def, old_label, old_defs, defs, channel_names(&mixer))
    };
    state
        .backend
        .destroy_bus(&name)
        .map_err(|e| e.to_string())?;
    if let Err(error) = defs.save() {
        return Err(mutation_failure(
            error,
            &[
                ("restoring the previous mix definitions", old_defs.save()),
                (
                    "restoring the deleted mix",
                    restore_removed_bus(state.backend.as_ref(), &old_def, &old_label, &all),
                ),
            ],
        ));
    }
    {
        let mixer = state.lock_mixer()?;
        if let Err(error) = crate::commands::profiles::save_active_with_buses(&mixer, &defs) {
            return Err(mutation_failure(
                error,
                &[
                    (
                        "restoring the previous active profile",
                        crate::commands::profiles::save_active_with_buses(&mixer, &old_defs),
                    ),
                    ("restoring the previous mix definitions", old_defs.save()),
                    (
                        "restoring the deleted mix",
                        restore_removed_bus(state.backend.as_ref(), &old_def, &old_label, &all),
                    ),
                ],
            ));
        }
    }
    let mut mixer = state.lock_mixer()?;
    mixer.buses = defs;
    Ok(())
}

/// Replace the channel set a mix carries. `channels` is what the user
/// sees checked; for auto-include mixes the complement (the unchecked
/// set) is what gets stored, so future channels keep flowing in.
#[tauri::command]
pub fn set_bus_members(
    state: State<'_, AppState>,
    name: String,
    channels: Vec<String>,
    expected_profile: Option<String>,
) -> Result<(), String> {
    let _profile_operation = state.lock_expected_profile_operation(expected_profile.as_deref())?;
    // Validate against the definition set first, so a rejected request
    // (master mix, unknown name) never reaches the backend - otherwise
    // backend membership and the persisted definition could diverge.
    let (old_defs, defs, old_members) = {
        let mixer = state.lock_mixer()?;
        if crate::persistence::buses::is_master(&name) {
            return Err("the master mix always carries every channel".to_string());
        }
        let Some(def) = mixer.buses.get(&name) else {
            return Err("unknown mix".to_string());
        };
        let all = channel_names(&mixer);
        validate_member_channels(&channels, &all)?;
        let old_members = def.effective_members(&all);
        let stored = if def.exclude {
            all.into_iter().filter(|c| !channels.contains(c)).collect()
        } else {
            channels.clone()
        };
        let old_defs = mixer.buses.clone();
        let mut defs = old_defs.clone();
        defs.set_members(&name, stored).map_err(|e| e.to_string())?;
        (old_defs, defs, old_members)
    };
    state
        .backend
        .set_bus_members(&name, &channels)
        .map_err(|e| e.to_string())?;
    {
        let mixer = state.lock_mixer()?;
        persist_bus_edit(
            &mixer,
            &old_defs,
            &defs,
            "restoring the previous live mix membership",
            || state.backend.set_bus_members(&name, &old_members),
        )?;
    }
    let mut mixer = state.lock_mixer()?;
    mixer.buses = defs;
    Ok(())
}

/// Switch a mix between manual selection and auto-include mode. The
/// carried set is preserved; only what happens to future channels changes.
#[tauri::command]
pub fn set_bus_exclude(
    state: State<'_, AppState>,
    name: String,
    exclude: bool,
    expected_profile: Option<String>,
) -> Result<(), String> {
    let _profile_operation = state.lock_expected_profile_operation(expected_profile.as_deref())?;
    let (old_defs, defs) = {
        let mixer = state.lock_mixer()?;
        let all = channel_names(&mixer);
        let old_defs = mixer.buses.clone();
        let mut defs = old_defs.clone();
        defs.set_exclude(&name, exclude, &all)
            .map_err(|e| e.to_string())?;
        (old_defs, defs)
    };
    {
        let mixer = state.lock_mixer()?;
        persist_bus_edit(
            &mixer,
            &old_defs,
            &defs,
            "restoring the live mix mode",
            || Ok(()),
        )?;
    }
    let mut mixer = state.lock_mixer()?;
    mixer.buses = defs;
    Ok(())
}

/// Set a mix's playback level (0-150%) and persist it. For a regular mix
/// this is what recorders hear; for the master mix it's the true listening
/// volume, rescaling what every channel's own live sink outputs (see
/// `push_master_gain_to_channels`) instead of the mix's own node. Unlike
/// `set_channel_volume`, this accepts mix nodes at all, including the master
/// mix, whose reserved name `set_channel_volume` rejects.
#[tauri::command]
pub fn set_bus_volume(
    state: State<'_, AppState>,
    name: String,
    volume: u8,
    expected_profile: Option<String>,
) -> Result<(), String> {
    let _profile_operation = state.lock_expected_profile_operation(expected_profile.as_deref())?;
    state.ensure_known_bus(&name)?;
    let volume = volume.min(MAX_VOLUME);
    let is_master = crate::persistence::buses::is_master(&name);
    let is_streamer_mode = crate::persistence::buses::is_streamer_mode(&name);
    let (old_defs, defs, old_volume, channels, gain_muted) = {
        let mixer = state.lock_mixer()?;
        let old_volume = mixer
            .buses
            .get(&name)
            .map(|def| def.volume_percent)
            .ok_or_else(|| "unknown mix".to_string())?;
        let old_defs = mixer.buses.clone();
        let mut defs = old_defs.clone();
        defs.set_volume(&name, volume).map_err(|e| e.to_string())?;
        let (_, gain_muted) = if is_streamer_mode {
            mixer.streamer_gain()
        } else {
            mixer.master_gain()
        };
        (
            old_defs,
            defs,
            old_volume,
            mixer.channels.clone(),
            gain_muted,
        )
    };
    if is_master {
        push_master_gain_to_channels(
            state.backend.as_ref(),
            &channels,
            volume as f32 / 100.0,
            gain_muted,
        )
    } else if is_streamer_mode {
        push_streamer_gain_to_channels(
            state.backend.as_ref(),
            &channels,
            volume as f32 / 100.0,
            gain_muted,
        )
    } else {
        state.backend.set_sink_volume(&name, volume)
    }
    .map_err(|e| e.to_string())?;
    {
        let mixer = state.lock_mixer()?;
        persist_bus_edit(
            &mixer,
            &old_defs,
            &defs,
            "restoring the previous live mix volume",
            || {
                if is_master {
                    push_master_gain_to_channels(
                        state.backend.as_ref(),
                        &channels,
                        old_volume as f32 / 100.0,
                        gain_muted,
                    )
                } else if is_streamer_mode {
                    push_streamer_gain_to_channels(
                        state.backend.as_ref(),
                        &channels,
                        old_volume as f32 / 100.0,
                        gain_muted,
                    )
                } else {
                    state.backend.set_sink_volume(&name, old_volume)
                }
            },
        )?;
    }
    let mut mixer = state.lock_mixer()?;
    mixer.buses = defs;
    Ok(())
}

/// Mute or unmute a mix, persisted. For a regular mix this only silences
/// recorders; for the master mix it silences every channel's live output -
/// the user hears nothing, not just what's being recorded.
#[tauri::command]
pub fn set_bus_mute(
    state: State<'_, AppState>,
    name: String,
    muted: bool,
    expected_profile: Option<String>,
) -> Result<(), String> {
    let _profile_operation = state.lock_expected_profile_operation(expected_profile.as_deref())?;
    state.ensure_known_bus(&name)?;
    let is_master = crate::persistence::buses::is_master(&name);
    let is_streamer_mode = crate::persistence::buses::is_streamer_mode(&name);
    let (old_defs, defs, old_muted, channels, fraction) = {
        let mixer = state.lock_mixer()?;
        let old_muted = mixer
            .buses
            .get(&name)
            .map(|def| def.muted)
            .ok_or_else(|| "unknown mix".to_string())?;
        let old_defs = mixer.buses.clone();
        let mut defs = old_defs.clone();
        defs.set_muted(&name, muted).map_err(|e| e.to_string())?;
        let (fraction, _) = if is_streamer_mode {
            mixer.streamer_gain()
        } else {
            mixer.master_gain()
        };
        (old_defs, defs, old_muted, mixer.channels.clone(), fraction)
    };
    if is_master {
        push_master_gain_to_channels(state.backend.as_ref(), &channels, fraction, muted)
    } else if is_streamer_mode {
        push_streamer_gain_to_channels(state.backend.as_ref(), &channels, fraction, muted)
    } else {
        state.backend.set_sink_mute(&name, muted)
    }
    .map_err(|e| e.to_string())?;
    {
        let mixer = state.lock_mixer()?;
        persist_bus_edit(
            &mixer,
            &old_defs,
            &defs,
            "restoring the previous live mix mute",
            || {
                if is_master {
                    push_master_gain_to_channels(
                        state.backend.as_ref(),
                        &channels,
                        fraction,
                        old_muted,
                    )
                } else if is_streamer_mode {
                    push_streamer_gain_to_channels(
                        state.backend.as_ref(),
                        &channels,
                        fraction,
                        old_muted,
                    )
                } else {
                    state.backend.set_sink_mute(&name, old_muted)
                }
            },
        )?;
    }
    let mut mixer = state.lock_mixer()?;
    mixer.buses = defs;
    Ok(())
}

/// The current channel sink names (the "all channels" set for mixes).
pub(crate) fn channel_names(mixer: &crate::mixer::state::MixerState) -> Vec<String> {
    mixer.channels.iter().map(|c| c.name.clone()).collect()
}
