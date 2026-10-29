#!/bin/sh
# Run on the Linux host with PipeWire, WirePlumber, pactl, Python and a Wayland session.
# Uses a private audio graph and private config; never connects to desktop audio.
set -eu
repo=$(CDPATH= cd -- "$(dirname -- "$0")/../.." && pwd)
test_dir=$(mktemp -d /tmp/mixweave-api-test.XXXXXX)
chmod 700 "$test_dir"
export WAYLAND_DISPLAY="${WAYLAND_DISPLAY:-wayland-0}"
case "$WAYLAND_DISPLAY" in /*) ;; *) WAYLAND_DISPLAY="${XDG_RUNTIME_DIR:-/run/user/$(id -u)}/$WAYLAND_DISPLAY"; export WAYLAND_DISPLAY;; esac
export XDG_RUNTIME_DIR="$test_dir/runtime"
export XDG_CONFIG_HOME="$test_dir/config"
export XDG_DATA_HOME="$test_dir/data"
export XDG_CACHE_HOME="$test_dir/cache"
export XDG_STATE_HOME="$test_dir/state"
export PIPEWIRE_RUNTIME_DIR="$XDG_RUNTIME_DIR"
export PIPEWIRE_REMOTE=pipewire-0
export PULSE_SERVER="unix:$XDG_RUNTIME_DIR/pulse/native"
export GSETTINGS_BACKEND=memory
export XDG_CURRENT_DESKTOP=MixweaveTest
export MIXWEAVE_API_PORT=0
export MIXWEAVE_API_DISABLED=0
mkdir -p "$XDG_RUNTIME_DIR" "$XDG_CONFIG_HOME/wireplumber/main.lua.d" "$XDG_CONFIG_HOME/wireplumber/bluetooth.lua.d" "$XDG_CONFIG_HOME/wireplumber/wireplumber.conf.d"
chmod 700 "$XDG_RUNTIME_DIR"
cat > "$XDG_CONFIG_HOME/wireplumber/main.lua.d/89-api-test.lua" <<'LUA'
alsa_monitor.enable = function() end
v4l2_monitor.enable = function() end
libcamera_monitor.enable = function() end
LUA
cat > "$XDG_CONFIG_HOME/wireplumber/bluetooth.lua.d/89-api-test.lua" <<'LUA'
bluez_monitor.enable = function() end
if bluez_midi_monitor then bluez_midi_monitor.enable = function() end end
LUA
cat > "$XDG_CONFIG_HOME/wireplumber/wireplumber.conf.d/10-api-test.conf" <<'CONF'
wireplumber.profiles = { main = { monitor.alsa = disabled monitor.bluez = disabled monitor.bluez-midi = disabled monitor.libcamera = disabled } }
CONF
# Persisted desktop choice: only Media opts into amplification in this fixture.
mkdir -p "$XDG_CONFIG_HOME/mixweave"
cat > "$XDG_CONFIG_HOME/mixweave/prefs.json" <<'JSON'
{"volume_range_overrides":{"sink_media":true}}
JSON
pids=''
cleanup() {
  for child in $pids; do kill "$child" 2>/dev/null || true; done
  echo "Test logs retained at $test_dir"
}
trap cleanup EXIT INT TERM
pipewire > "$test_dir/pipewire.log" 2>&1 & pids="$! $pids"
pipewire-pulse > "$test_dir/pulse.log" 2>&1 & pids="$! $pids"
wireplumber > "$test_dir/wireplumber.log" 2>&1 & pids="$! $pids"
i=0
until pactl info >/dev/null 2>&1; do i=$((i+1)); [ "$i" -lt 50 ] || exit 1; sleep 0.2; done
pactl load-module module-null-sink sink_name=api_test_out sink_properties=device.description=API_Test_Output >/dev/null
pactl load-module module-null-sink sink_name=api_test_out2 sink_properties=device.description=API_Test_Output_2 >/dev/null
pactl load-module module-remap-source master=api_test_out.monitor source_name=api_test_in source_properties=device.description=API_Test_Input >/dev/null
"${MIXWEAVE_TEST_BINARY:-$repo/target/debug/mixweave}" --minimized > "$test_dir/mixweave.log" 2>&1 & pids="$! $pids"
python3 "$repo/scripts/api/live_test.py" --isolated "$test_dir" --report "$repo/tools/api-panel/isolated-test-report.json"
