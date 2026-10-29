//! API writes respect desktop amplification choices and Streamer Mode availability.
use super::ApiError;
use crate::persistence::buses::{DEFAULT_BUS_NODE, STREAMER_MODE_BUS_NODE};
use crate::{audio::types::MicConfig, mixer::state::MixerState};
use axum::http::StatusCode;
use serde_json::Value;

fn limit(mixer: &MixerState, id: &str, value: u64, hard_max: u64) -> Result<(), ApiError> {
    // The Master strip owns both its Personal and Stream sliders.
    let id = if id == STREAMER_MODE_BUS_NODE {
        DEFAULT_BUS_NODE
    } else {
        id
    };
    let allowed = mixer
        .prefs
        .volume_range_overrides
        .as_ref()
        .and_then(|ranges| ranges.get(id))
        .copied()
        .unwrap_or(false);
    let maximum = if allowed { hard_max } else { 100 };
    if value > maximum {
        return Err(ApiError::new(StatusCode::CONFLICT, "volume_limit_exceeded",
            format!("Volume for {id} cannot exceed {maximum}%. Enable 'Allow above 100%' for this strip in Mixweave to allow amplification (hardware maximum: {hard_max}%).")));
    }
    Ok(())
}

/// Saturate valid channel/bus requests at the desktop's selected ceiling.
/// Keep malformed values and hardware-limit violations for normal validation.
pub(super) fn clamp_volume(mixer: &MixerState, method: &str, params: &mut Value) {
    let key = match method {
        "set_channel_volume" | "set_channel_stream_volume" => "sink_name",
        "set_bus_volume" => "name",
        _ => return,
    };
    let Some(id) = params.get(key).and_then(Value::as_str) else {
        return;
    };
    let Some(volume) = params.get("volume").and_then(Value::as_u64) else {
        return;
    };
    if volume <= 150 && limit(mixer, id, volume, 150).is_err() {
        params["volume"] = Value::from(100);
    }
}

pub(super) fn validate(mixer: &MixerState, method: &str, params: &Value) -> Result<(), ApiError> {
    let name = params.get("name").and_then(Value::as_str).unwrap_or("");
    let sink = params
        .get("sink_name")
        .and_then(Value::as_str)
        .unwrap_or("");
    let mic: Option<MicConfig> = if method == "set_mic_config" {
        params
            .get("config")
            .and_then(|c| serde_json::from_value(c.clone()).ok())
    } else {
        None
    };
    let previous = mic.as_ref().and_then(|config| {
        std::iter::once(&mixer.mic)
            .chain(mixer.secondary_mics.iter())
            .find(|old| old.node_name == config.node_name)
    });
    let mic_stream_changed = mic.as_ref().zip(previous).is_some_and(|(new, old)| {
        new.stream_send_gain_percent != old.stream_send_gain_percent
            || new.stream_send_muted != old.stream_send_muted
    });
    let stream_target = name == STREAMER_MODE_BUS_NODE
        || sink == STREAMER_MODE_BUS_NODE
        || std::iter::once(&mixer.mic)
            .chain(mixer.secondary_mics.iter())
            .any(|m| sink == format!("{}_stream", m.node_name));
    if !mixer.prefs.streamer_mode_enabled
        && (matches!(
            method,
            "set_channel_stream_volume" | "toggle_channel_stream_mute"
        ) || mic_stream_changed
            || (stream_target && !method.starts_with("get_") && !method.starts_with("list_")))
    {
        return Err(ApiError::new(StatusCode::CONFLICT, "streamer_mode_disabled",
            "Streamer Mode is disabled. Enable Streamer Mode in Mixweave or call set_streamer_mode_enabled with enabled=true before controlling Stream paths."));
    }
    match method {
        "set_channel_volume" | "set_channel_stream_volume" => {
            if let Some(volume) = params.get("volume").and_then(Value::as_u64) {
                limit(mixer, sink, volume, 150)?;
            }
        }
        "set_bus_volume" => {
            if let Some(volume) = params.get("volume").and_then(Value::as_u64) {
                limit(mixer, name, volume, 150)?;
            }
        }
        "set_mic_config" => {
            if let Some(config) = mic {
                // A full configuration also carries unchanged stream fields. Allow
                // unrelated edits to legacy configurations without forcing a reset.
                if previous.is_none_or(|old| config.gain_percent != old.gain_percent) {
                    limit(mixer, &config.node_name, config.gain_percent as u64, 200)?;
                }
                if previous.is_none_or(|old| {
                    config.stream_send_gain_percent != old.stream_send_gain_percent
                }) {
                    limit(
                        mixer,
                        &config.node_name,
                        config.stream_send_gain_percent as u64,
                        200,
                    )?;
                }
            }
        }
        _ => {}
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;
    #[test]
    fn channel_and_bus_requests_saturate_at_selected_ceiling() {
        let mut mixer = MixerState::default();
        mixer.prefs.streamer_mode_enabled = true;
        for (method, key, id) in [
            ("set_channel_volume", "sink_name", "sink_game"),
            ("set_channel_stream_volume", "sink_name", "sink_game"),
            ("set_bus_volume", "name", DEFAULT_BUS_NODE),
            ("set_bus_volume", "name", STREAMER_MODE_BUS_NODE),
        ] {
            for allowed in [false, true] {
                mixer.prefs.volume_range_overrides = Some(
                    [
                        ("sink_game".into(), allowed),
                        (DEFAULT_BUS_NODE.into(), allowed),
                    ]
                    .into(),
                );
                for volume in [40, 100, 105, 150] {
                    let mut request = json!({key: id, "volume": volume});
                    clamp_volume(&mixer, method, &mut request);
                    assert_eq!(
                        request["volume"],
                        if allowed { volume } else { volume.min(100) }
                    );
                    assert!(validate(&mixer, method, &request).is_ok());
                    let previous = request.clone();
                    clamp_volume(&mixer, method, &mut request);
                    assert_eq!(request, previous);
                }
            }
        }
    }

    #[test]
    fn amplification_is_explicit_persisted_and_per_strip() {
        let mut mixer = MixerState::default();
        let request = json!({"sink_name":"sink_game","volume":150});
        assert_eq!(
            validate(&mixer, "set_channel_volume", &request)
                .unwrap_err()
                .code,
            "volume_limit_exceeded"
        );
        mixer.prefs.volume_range_overrides = Some([("sink_game".into(), true)].into());
        assert!(validate(&mixer, "set_channel_volume", &request).is_ok());
        assert!(validate(
            &mixer,
            "set_channel_volume",
            &json!({"sink_name":"sink_chat","volume":101})
        )
        .is_err());
        assert!(validate(
            &mixer,
            "set_channel_volume",
            &json!({"sink_name":"sink_game","volume":151})
        )
        .is_err());
        mixer
            .prefs
            .volume_range_overrides
            .as_mut()
            .unwrap()
            .insert("sink_game".into(), false);
        assert!(validate(&mixer, "set_channel_volume", &request).is_err());
        assert!(validate(
            &mixer,
            "set_channel_volume",
            &json!({"sink_name":"sink_game","volume":100})
        )
        .is_ok());
    }
    #[test]
    fn stream_controls_require_mode_but_personal_and_reads_work() {
        let mut mixer = MixerState::default();
        for (method, params) in [
            (
                "set_channel_stream_volume",
                json!({"sink_name":"sink_game","volume":40}),
            ),
            (
                "toggle_channel_stream_mute",
                json!({"sink_name":"sink_game","muted":true}),
            ),
            (
                "set_bus_volume",
                json!({"name":STREAMER_MODE_BUS_NODE,"volume":40}),
            ),
            (
                "set_bus_mute",
                json!({"name":STREAMER_MODE_BUS_NODE,"muted":true}),
            ),
            (
                "set_monitor",
                json!({"sink_name":"sink_mic_stream","enabled":true}),
            ),
        ] {
            assert_eq!(
                validate(&mixer, method, &params).unwrap_err().code,
                "streamer_mode_disabled"
            );
            mixer.prefs.streamer_mode_enabled = true;
            assert!(validate(&mixer, method, &params).is_ok());
            mixer.prefs.streamer_mode_enabled = false;
        }
        assert!(validate(
            &mixer,
            "set_streamer_mode_enabled",
            &json!({"enabled":true})
        )
        .is_ok());
        assert!(validate(&mixer, "get_bus", &json!({"name":STREAMER_MODE_BUS_NODE})).is_ok());
    }
    #[test]
    fn mic_personal_edits_do_not_require_streamer_mode() {
        let mut mixer = MixerState::default();
        let mut config = mixer.mic.clone();
        config.gain_percent = 80;
        assert!(validate(&mixer, "set_mic_config", &json!({"config":config})).is_ok());
        config.stream_send_muted = true;
        assert_eq!(
            validate(&mixer, "set_mic_config", &json!({"config":config}))
                .unwrap_err()
                .code,
            "streamer_mode_disabled"
        );
        mixer.prefs.streamer_mode_enabled = true;
        config.gain_percent = 150;
        assert_eq!(
            validate(&mixer, "set_mic_config", &json!({"config":config}))
                .unwrap_err()
                .code,
            "volume_limit_exceeded"
        );
        mixer.prefs.volume_range_overrides = Some([("sink_mic".into(), true)].into());
        config.gain_percent = 200;
        assert!(validate(&mixer, "set_mic_config", &json!({"config":config})).is_ok());
    }
    #[test]
    fn stream_master_uses_master_toggle() {
        let mut mixer = MixerState::default();
        mixer.prefs.streamer_mode_enabled = true;
        let request = json!({"name":STREAMER_MODE_BUS_NODE,"volume":150});
        assert!(validate(&mixer, "set_bus_volume", &request).is_err());
        mixer.prefs.volume_range_overrides = Some([(DEFAULT_BUS_NODE.into(), true)].into());
        assert!(validate(&mixer, "set_bus_volume", &request).is_ok());
        let saved = serde_json::to_string(&mixer.prefs).unwrap();
        let restored: crate::persistence::prefs::Prefs = serde_json::from_str(&saved).unwrap();
        assert_eq!(restored, mixer.prefs);
    }
}
