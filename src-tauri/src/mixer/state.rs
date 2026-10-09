use std::collections::HashSet;

use crate::audio::types::{SinkControlState, VirtualSink};
use crate::persistence::aliases::Aliases;
use crate::persistence::assignments::Assignments;
use crate::persistence::channels::Channels;

/// In-memory mixer state: the source of truth for channel volume/mute as
/// set through the UI, plus the persistent app→channel assignments.
#[derive(Debug, Default)]
pub struct MixerState {
    pub channels: Vec<VirtualSink>,
    /// User-defined channel set (persisted to disk).
    pub channel_defs: Channels,
    /// True once `init_virtual_devices` has created the sinks.
    pub initialized: bool,
    /// Saved app→channel assignments (persisted to disk + WirePlumber conf).
    pub assignments: Assignments,
    /// User-chosen display names for discovered apps (persisted to disk).
    pub aliases: Aliases,
    /// Per-channel output device choices (persisted to disk).
    pub outputs: crate::persistence::outputs::ChannelOutputs,
    /// Per-channel parametric EQ configs (persisted to disk).
    pub eq: crate::persistence::eq::ChannelEq,
    /// Mic chain configuration (persisted to disk).
    pub mic: crate::audio::types::MicConfig,
    /// Optional profile-specific processed microphone channels.
    pub secondary_mics: Vec<crate::audio::types::MicConfig>,
    /// Every app identity ever observed (history + ignore list).
    pub seen: crate::persistence::seen::SeenApps,
    /// Unix seconds of the last `seen` write. The poll only saves on
    /// structural changes, so this drives a slow flush that bounds how stale
    /// on-disk `last_seen` timestamps can get if Sink dies without a clean
    /// quit - the age-based prune trusts them.
    pub seen_saved_at: u64,
    /// Profile changes autosave into this profile (live-bound, not a
    /// snapshot). None = unmanaged state.
    pub active_profile: Option<String>,
    /// Cached trigger device of `active_profile`, so autosave preserves it
    /// without re-reading the profile file on every mutation. Kept in step
    /// whenever the active profile or its trigger changes.
    pub active_trigger: Option<String>,
    /// Cached deletion-protection bit of `active_profile`. Autosave rewrites
    /// the complete profile, so this metadata must be preserved as well.
    pub active_protected: bool,
    /// User-defined mixes (record buses), persisted to disk.
    pub buses: crate::persistence::buses::Buses,
    /// App preferences (device naming etc.), persisted to disk.
    pub prefs: crate::persistence::prefs::Prefs,
    /// Stream indices already considered for auto-routing this session.
    /// Each stream is enforced once, on first sight, so a user moving a
    /// stream elsewhere (here or in pavucontrol) isn't fought every poll.
    pub auto_routed: HashSet<u32>,
}

impl MixerState {
    /// Populate the channel strips from the user's channel definitions,
    /// each at 100% volume, unmuted.
    pub fn init_defaults(&mut self) {
        self.channels = self
            .channel_defs
            .channels
            .iter()
            .map(|def| VirtualSink {
                name: def.name.clone(),
                label: def.label.clone(),
                icon: def.icon.clone(),
                volume_percent: 100,
                muted: false,
                stream_mix: def.stream_mix,
                stream_send_volume_percent: 100,
                stream_send_muted: false,
            })
            .collect();
        self.initialized = true;
    }

    pub fn channel_mut(&mut self, sink_name: &str) -> Option<&mut VirtualSink> {
        self.channels.iter_mut().find(|c| c.name == sink_name)
    }

    /// The Streamer Mode mix's current gain fraction and mute flag,
    /// entirely independent of `master_gain`. See
    /// `persistence::buses::streamer_gain`.
    pub fn streamer_gain(&self) -> (f32, bool) {
        crate::persistence::buses::streamer_gain(&self.buses)
    }

    /// The master mix's current gain fraction and mute flag. See
    /// `persistence::buses::master_gain`.
    pub fn master_gain(&self) -> (f32, bool) {
        crate::persistence::buses::master_gain(&self.buses)
    }

    /// Adopt live volume/mute changes made outside Sink. Returns whether the
    /// profile-relevant mixer state changed.
    ///
    /// Every channel's *live* PipeWire volume/mute is the channel's own
    /// stored value scaled by the master gain (Master is a true listening
    /// volume control - see `persistence::buses::master_gain`), so the
    /// "expected" live value is recomputed fresh from `channel.volume_percent`
    /// and the current master gain on every call, rather than tracked
    /// separately. A live value that matches this expectation is our own
    /// write settling in, not an external change, and is left alone; a
    /// mismatch is a genuine external change (e.g. via pavucontrol), and the
    /// master scaling is reversed so the channel's stored "own level" still
    /// means "before master". Mute can't be reverse-derived the same way
    /// while master is muted (every channel's live mute is forced true
    /// regardless of its own flag then), so mute adoption is skipped in
    /// that case rather than guessed at.
    pub fn sync_controls(&mut self, controls: &[SinkControlState]) -> bool {
        let (fraction, master_muted) = self.master_gain();
        let mut changed = false;
        for control in controls {
            let Some(channel) = self.channel_mut(&control.name) else {
                continue;
            };
            let expected_volume =
                crate::persistence::buses::scaled_volume(channel.volume_percent, fraction);
            let expected_muted = channel.muted || master_muted;
            if control.volume_percent == expected_volume && control.muted == expected_muted {
                continue;
            }
            let raw_volume = if fraction > 0.01 {
                ((control.volume_percent as f32) / fraction)
                    .round()
                    .clamp(0.0, 150.0) as u8
            } else {
                control.volume_percent
            };
            if channel.volume_percent != raw_volume {
                channel.volume_percent = raw_volume;
                changed = true;
            }
            if !master_muted && channel.muted != control.muted {
                channel.muted = control.muted;
                changed = true;
            }
        }
        changed
    }

    /// Forget history entries the user never acted on and hasn't seen in a
    /// week, so the "not running" list stays about apps they actually use.
    /// Returns true when the history changed and should be saved.
    pub fn prune_stale_apps(&mut self, now: u64) -> bool {
        // Disjoint field borrows: `prune` needs `seen` mutably while the
        // intent test reads the other two.
        let Self {
            seen,
            assignments,
            aliases,
            ..
        } = self;
        seen.prune(
            now,
            crate::persistence::seen::MAX_SEEN_AGE_SECS,
            |prop, value| {
                assignments.sink_for(prop, value).is_some() || aliases.get(prop, value).is_some()
            },
        )
    }

    pub fn reset(&mut self) {
        self.channels.clear();
        self.initialized = false;
    }
}
