//! Translate successful API level/mute writes into the existing desktop OSD.
//! Notifications use confirmed state, never the unvalidated request value.
use crate::state::AppState;
use serde::Serialize;
use serde_json::Value;
use tauri::Manager;

#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub(super) struct Notice {
    id: Option<String>,
    label: String,
    volume_percent: u8,
    max: u8,
    muted: bool,
    stream: bool,
}
impl Notice {
    fn new(
        id: Option<String>,
        label: String,
        volume_percent: u8,
        max: u8,
        muted: bool,
        stream: bool,
    ) -> Self {
        Self {
            id,
            label,
            volume_percent,
            max,
            muted,
            stream,
        }
    }
}

#[derive(Debug, PartialEq)]
pub(super) enum Target {
    Channel(String, bool),
    Bus(String),
    Mic(String),
    Hardware(String, String),
    App(u32),
}
impl Target {
    pub(super) fn from_request(method: &str, params: &Value) -> Option<Self> {
        let string = |key: &str| params.get(key)?.as_str().map(str::to_owned);
        match method {
            "set_channel_volume" | "toggle_channel_mute" => {
                Some(Self::Channel(string("sink_name")?, false))
            }
            "set_channel_stream_volume" | "toggle_channel_stream_mute" => {
                Some(Self::Channel(string("sink_name")?, true))
            }
            "set_bus_volume" | "set_bus_mute" => Some(Self::Bus(string("name")?)),
            "set_mic_config" => Some(Self::Mic(
                params
                    .get("config")?
                    .get("node_name")
                    .and_then(Value::as_str)
                    .unwrap_or("sink_mic")
                    .into(),
            )),
            "set_hardware_volume" | "set_hardware_mute" => {
                Some(Self::Hardware(string("kind")?, string("name")?))
            }
            "set_app_volume" => Some(Self::App(
                u32::try_from(params.get("stream_index")?.as_u64()?).ok()?,
            )),
            _ => None,
        }
    }

    pub(super) fn read(&self, app: &tauri::AppHandle) -> Option<Vec<Notice>> {
        let state = app.state::<AppState>();
        match self {
            Self::Hardware(kind, name) => {
                let devices = crate::commands::hardware::get_hardware_devices(app.state()).ok()?;
                let device = devices
                    .into_iter()
                    .find(|d| &d.name == name && d.kind == kind)?;
                Some(vec![Notice::new(
                    None,
                    device.description,
                    device.volume_percent,
                    100,
                    device.muted,
                    false,
                )])
            }
            Self::App(index) => {
                let streams = state.backend.list_app_streams().ok()?;
                let stream = streams.into_iter().find(|s| s.index == *index)?;
                Some(vec![Notice::new(
                    None,
                    stream.alias.unwrap_or(stream.app_name),
                    stream.volume_percent,
                    150,
                    stream.muted,
                    false,
                )])
            }
            _ => {
                let mixer = state.lock_mixer().ok()?;
                match self {
                    Self::Channel(name, stream) => {
                        let channel = mixer.channels.iter().find(|c| &c.name == name)?;
                        let (volume, muted) = if *stream {
                            (
                                channel.stream_send_volume_percent,
                                channel.stream_send_muted,
                            )
                        } else {
                            (channel.volume_percent, channel.muted)
                        };
                        Some(vec![Notice::new(
                            Some(name.clone()),
                            channel.label.clone(),
                            volume,
                            150,
                            muted,
                            *stream,
                        )])
                    }
                    Self::Bus(name) => {
                        let bus = mixer.buses.get(name)?;
                        Some(vec![Notice::new(
                            Some(name.clone()),
                            bus.label.clone(),
                            bus.volume_percent,
                            150,
                            bus.muted,
                            false,
                        )])
                    }
                    Self::Mic(name) => {
                        let mic = std::iter::once(&mixer.mic)
                            .chain(mixer.secondary_mics.iter())
                            .find(|m| &m.node_name == name)?;
                        Some(vec![
                            Notice::new(
                                Some(name.clone()),
                                mic.output_label.clone(),
                                mic.gain_percent,
                                200,
                                mic.muted,
                                false,
                            ),
                            Notice::new(
                                Some(name.clone()),
                                mic.output_label.clone(),
                                mic.stream_send_gain_percent,
                                200,
                                mic.stream_send_muted,
                                true,
                            ),
                        ])
                    }
                    _ => unreachable!(),
                }
            }
        }
    }

    pub(super) fn notifications(
        &self,
        before: Option<Vec<Notice>>,
        after: Option<Vec<Notice>>,
        succeeded: bool,
    ) -> Vec<Notice> {
        if !succeeded {
            return vec![];
        }
        let Some(after) = after else {
            return vec![];
        };
        if matches!(self, Self::Mic(_)) {
            // set_mic_config also edits EQ, input selection and noise removal.
            // Only actual gain/mute changes should display a volume popup.
            let Some(before) = before else {
                return vec![];
            };
            after
                .into_iter()
                .filter(|next| {
                    before.iter().any(|old| {
                        old.stream == next.stream
                            && (old.volume_percent != next.volume_percent
                                || old.muted != next.muted)
                    })
                })
                .collect()
        } else {
            after
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;
    fn mic(volume: u8, muted: bool, stream: bool) -> Notice {
        Notice::new(
            Some("sink_mic".into()),
            "Mic".into(),
            volume,
            200,
            muted,
            stream,
        )
    }
    #[test]
    fn unchanged_channel_volume_still_shows_overlay() {
        let target = Target::Channel("sink_game".into(), false);
        let current = vec![Notice::new(
            Some("sink_game".into()),
            "Game".into(),
            100,
            150,
            false,
            false,
        )];
        assert_eq!(
            target.notifications(Some(current.clone()), Some(current.clone()), true),
            current
        );
    }

    #[test]
    fn selects_only_audio_level_commands() {
        assert_eq!(
            Target::from_request(
                "set_channel_stream_volume",
                &json!({"sink_name":"sink_game","volume":45})
            ),
            Some(Target::Channel("sink_game".into(), true))
        );
        assert_eq!(
            Target::from_request("set_mic_config", &json!({"config":{}})),
            Some(Target::Mic("sink_mic".into()))
        );
        for method in [
            "load_profile",
            "get_virtual_devices",
            "set_channel_output",
            "set_channel_eq",
        ] {
            assert_eq!(Target::from_request(method, &json!({})), None);
        }
    }
    #[test]
    fn suppresses_failed_commands_and_unreadable_state() {
        let target = Target::Bus("sink_stream".into());
        assert!(target
            .notifications(None, Some(vec![mic(80, false, false)]), false)
            .is_empty());
        assert!(target.notifications(None, None, true).is_empty());
        assert_eq!(
            target.notifications(None, Some(vec![mic(150, false, false)]), true)[0].volume_percent,
            150
        );
    }
    #[test]
    fn mic_processing_edits_do_not_trigger_volume_popup() {
        let target = Target::Mic("sink_mic".into());
        let before = vec![mic(100, false, false), mic(100, false, true)];
        assert!(target
            .notifications(Some(before.clone()), Some(before.clone()), true)
            .is_empty());
        let notices = target.notifications(
            Some(before.clone()),
            Some(vec![mic(100, false, false), mic(65, true, true)]),
            true,
        );
        assert_eq!(notices, vec![mic(65, true, true)]);
        let notices = target.notifications(
            Some(before),
            Some(vec![mic(100, true, false), mic(100, false, true)]),
            true,
        );
        assert_eq!(notices, vec![mic(100, true, false)]);
    }
    #[test]
    fn event_uses_frontend_field_names() {
        let value = serde_json::to_value(mic(75, true, false)).unwrap();
        assert_eq!(value["volumePercent"], 75);
        assert_eq!(value["muted"], true);
        assert_eq!(value["max"], 200);
    }
}
