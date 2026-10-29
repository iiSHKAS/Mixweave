# Mixweave Local API — Developer Guide

Build a desktop utility, script, hardware controller, or automation tool that controls a running Mixweave instance over local HTTP. Your client does not need Tauri, Rust, or direct access to PipeWire.

This document describes **API v1** and the **95 commands** in this repository. Discover the running server's catalog instead of assuming that every installed version supports the same commands.

## Contents

1. [Quick start](#1-quick-start)
2. [Starting Mixweave](#2-starting-mixweave)
3. [Discovery and authentication](#3-discovery-and-authentication)
4. [HTTP contract](#4-http-contract)
5. [Python client](#5-python-client)
6. [Core concepts](#6-core-concepts)
7. [Reading state](#7-reading-state)
8. [Audio-control recipes](#8-audio-control-recipes)
9. [Data types, limits and Streamer Mode](#9-data-types-limits-and-streamer-mode)
10. [Errors, concurrency and reconnection](#10-errors-concurrency-and-reconnection)
11. [Building your own control panel](#11-building-your-own-control-panel)
12. [Testing your integration](#12-testing-your-integration)
13. [Linux compatibility and troubleshooting](#13-linux-compatibility-and-troubleshooting)
14. [Complete command reference](#14-complete-command-reference)
15. [Maintaining this guide (for contributors)](#15-maintaining-this-guide-for-contributors)

## 1. Quick start

Start an API-enabled Mixweave build in your desktop session. For installation/build details, see [Starting Mixweave](#2-starting-mixweave). Python 3 is enough for the example below; Qt and additional Python packages are unnecessary.

Copy [client.py](../tools/api-panel/client.py) into a folder of your choice. Save the following beside it as `inspect_mixweave.py`, then run `python3 inspect_mixweave.py`:

```python
from client import Client

api = Client.discover()
health = api.request('/v1/health')
if health['api_version'] != 1:
    raise RuntimeError('Unsupported API version')

# Requests are sequential: the server executes one RPC at a time.
snapshot = api.snapshot()
defaults = api.rpc('get_default_devices')
prefs = api.rpc('get_prefs')

print('Profile:', snapshot['active_profile'] or '(none)')
print('Default input:', defaults['input'] or '(none)')
print('Default output:', defaults['output'] or '(none)')
print('Streamer Mode:', prefs['streamer_mode_enabled'])

for channel in snapshot['channels']:
    print(channel['name'], channel['label'])
    print('  Personal:', channel['volume_percent'], '%; muted:', channel['muted'])
    print('  Stream:', channel['stream_send_volume_percent'],
          '%; muted:', channel['stream_send_muted'])

for bus in snapshot['buses']:
    print('Mix:', bus['name'], bus['volume_percent'], '%; muted:', bus['muted'])

for mic in snapshot['mic_configs']:
    print('Mic:', mic['node_name'])
    print('  Personal:', mic['gain_percent'], '%; muted:', mic['muted'])
    print('  Stream:', mic['stream_send_gain_percent'],
          '%; muted:', mic['stream_send_muted'])
```

This example submits only read commands and handles empty channel/microphone lists. Stream values describe the saved controls; check `streamer_mode_enabled` before treating them as an active Stream path. The three reads are not a transaction: another client can change state between them.

The Python client returns the contents of `result` directly. Raw HTTP clients receive the full `{ "ok": true, "result": ... }` envelope. A missing discovery file usually means Mixweave is not running, lacks the API, or uses a different configuration directory; see [troubleshooting](#13-linux-compatibility-and-troubleshooting).

## 2. Starting Mixweave

Use a build that includes the local API. The server starts automatically with Mixweave and remains available while its window is hidden in the system tray. It is not a separate daemon: exiting Mixweave stops the server.

When building this repository, install its normal Linux build dependencies and JavaScript dependencies first (see [README](../README.md)), then run from the repository root:

```sh
npm ci
./scripts/build-standalone.sh
./target/standalone/mixweave
```

Quit an existing instance from its tray menu before launching a replacement. Closing the window only hides it.

For frontend development, use `npm run tauri -- dev`. A development executable may expect Vite at `http://localhost:1420`; that address is **not the local audio API**. The standalone build embeds its frontend and does not need Vite.

### Server configuration

Set these environment variables **before starting Mixweave**:

| Variable | Default | Meaning |
|---|---|---|
| `MIXWEAVE_API_PORT` | `0` | Let the OS choose a free port. Set a value such as `47832` for a fixed port. |
| `MIXWEAVE_API_DISABLED` | Unset | Set exactly `1` to disable the API. |

### Enable or disable the API in Settings

The API is **enabled by default**. Turn it off or on in **Settings → Local API**. The change takes effect immediately and is remembered across restarts (`local_api_enabled` in `get_prefs`):

- **Off:** the server stops, its port is closed and `local-api.json` is removed. Clients get "connection refused" or a missing discovery file.
- **On:** the server starts again with a **new token** (and a new port unless `MIXWEAVE_API_PORT` is set). Re-read the discovery file.

`MIXWEAVE_API_DISABLED=1` always wins: the API stays off for that process even when the setting is on. The setting cannot be changed through the API itself.

```sh
MIXWEAVE_API_PORT=47832 ./target/standalone/mixweave
```

The server binds exclusively to IPv4 loopback, `127.0.0.1`. There is no setting to expose it on the LAN. A startup error, such as a port conflict, is logged as `mixweave: local API unavailable: ...`; the desktop application can continue running without the API.

## 3. Discovery and authentication

### Discovery file

Read the file written by the **running Mixweave user's configuration environment**:

```text
$XDG_CONFIG_HOME/mixweave/local-api.json
```

If `XDG_CONFIG_HOME` is unset, the normal location is:

```text
~/.config/mixweave/local-api.json
```

Example shape (these values are placeholders, not usable credentials):

```json
{
  "url": "http://127.0.0.1:47832",
  "token": "<64-character hexadecimal token>",
  "pid": 12345,
  "api_version": 1
}
```

The file is atomically replaced with permissions `0600`. The token is regenerated at server startup. An automatically allocated port can change too. Do not store a permanent copy of either value.

The file can remain after Mixweave exits or a later startup fails. Its presence, or the presence of its PID, is not proof that the API is ready: authenticate against `/v1/health`.

Flatpak, containers, or an IDE sandbox may use a different `XDG_CONFIG_HOME` or network namespace. A client must read the file belonging to Mixweave and be able to reach its loopback network. Prefer running both applications in the same user session. Do not change file permissions to make discovery public.

### Authentication requirements

Send the following header on **every endpoint**, including health and catalog requests:

```http
Authorization: Bearer YOUR_TOKEN
```

Use the discovered URL exactly. The server checks that `Host` matches `127.0.0.1:PORT`; substituting `localhost`, a custom hostname, or IPv6 will not work.

The server rejects any request containing an `Origin` header, including `Origin: null`. It does not provide browser CORS access. For Electron or a web-based desktop panel, make requests from the trusted native/main-process side, not directly from a browser renderer. Never expose the token to arbitrary web content.

Clients should bypass HTTP proxies for this connection and should not follow redirects with the token. Treat the token as a credential: it authorizes audio changes, profile deletion, and the existing microphone/channel recording-test commands. There are no per-command token scopes.

## 4. HTTP contract

| HTTP method | Path | Purpose |
|---|---|---|
| `GET` | `/v1/health` | Confirm authentication and inspect API/application versions. |
| `GET` | `/v1/commands` | Discover commands, groups, parameters, and read/write classification. |
| `POST` | `/v1/rpc` | Execute one named command. |

This is a simple HTTP RPC protocol, **not JSON-RPC 2.0**. Do not add `jsonrpc`, `id`, or batch arrays.

### Health response

```json
{
  "ok": true,
  "result": {
    "api_version": 1,
    "application": "Mixweave",
    "version": "0.5.1"
  }
}
```

The application version above is illustrative. Check the actual response. A successful health request confirms the server, not that initialization has finished or every audio device/backend feature is available. Next call `get_backend_info` and `get_profile_snapshot`.

### Executing a command

```http
POST /v1/rpc HTTP/1.1
Host: 127.0.0.1:47832
Authorization: Bearer YOUR_TOKEN
Content-Type: application/json
```

```json
{
  "method": "set_channel_volume",
  "params": {
    "sink_name": "sink_game",
    "volume": 50,
    "expected_profile": "Default"
  }
}
```

Use `snake_case` names from the catalog, not Tauri's JavaScript `camelCase` argument names. `params` must be an object; omitting it is equivalent to `{}`. Required arguments cannot be omitted or `null`. Optional arguments may be omitted or `null`. Unknown top-level request fields and unknown command parameter names are rejected.

Send `Content-Type: application/json` (an optional charset suffix is accepted). Request bodies are limited to **1 MiB**.

### Success and failure envelopes

A command that returns data:

```json
{"ok": true, "result": {"native": true}}
```

A command with no return value:

```json
{"ok": true, "result": null}
```

An API error:

```json
{
  "ok": false,
  "error": {
    "code": "command_failed",
    "message": "profile changed before the pending edit could be applied"
  }
}
```

Some framework-level errors, such as body-size rejection or an unknown URL, can return a plain-text or empty body. Check HTTP status before assuming JSON.

### Command discovery

`GET /v1/commands` returns an array inside `result`. One entry looks like:

```json
{
  "method": "set_channel_volume",
  "group": "routing",
  "read_only": false,
  "params": [
    {"name": "sink_name", "type": "String", "required": true},
    {"name": "volume", "type": "u8", "required": true},
    {"name": "expected_profile", "type": "Option<String>", "required": false}
  ]
}
```

The catalog describes Rust parameter types. It is not a full JSON Schema, does not include return schemas, and does not specify every nested field or semantic constraint. `read_only` identifies commands that do not explicitly request mutations; some reads refresh backend state or enforce existing application-routing rules.

## 5. Python client

The repository provides a standard-library client: [tools/api-panel/client.py](../tools/api-panel/client.py). Copy it beside your own Python script as `client.py`. **PyQt6 is not required** unless you want to use the supplied GUI.

Create `inspect_mixweave.py` beside that file:

```python
from client import Client

api = Client.discover()
health = api.request('/v1/health')
if health['api_version'] != 1:
    raise RuntimeError('Unsupported Mixweave API version')

catalog = api.request('/v1/commands')
available = {entry['method'] for entry in catalog}
if 'get_profile_snapshot' not in available:
    raise RuntimeError('This Mixweave build cannot provide a mixer snapshot')

print('Connected to', health['application'], health['version'])
print('Audio backend:', api.rpc('get_backend_info'))
snapshot = api.snapshot()
print('Active profile:', snapshot['active_profile'])
for channel in snapshot['channels']:
    print(channel['name'], channel['label'],
          channel['volume_percent'], channel['muted'])
```

Run:

```sh
python3 inspect_mixweave.py
```

This example only reads state. The client unwraps `result` and raises `ApiError` on HTTP/API failures. It uses a 15-second default network timeout, bypasses proxies, validates the loopback URL, and refuses redirects. You can also instantiate `Client(url, token, timeout=30)` using a discovery file read by your own code. Do not print the token.

### Minimal curl example

For a temporary terminal session, with Python 3 and curl installed:

```sh
DISCOVERY="${XDG_CONFIG_HOME:-$HOME/.config}/mixweave/local-api.json"
API_URL=$(python3 -c 'import json,sys; print(json.load(open(sys.argv[1]))["url"])' "$DISCOVERY")
API_TOKEN=$(python3 -c 'import json,sys; print(json.load(open(sys.argv[1]))["token"])' "$DISCOVERY")

# Supply the authorization header on stdin instead of the process command line.
printf 'Authorization: Bearer %s\n' "$API_TOKEN" |
  curl --noproxy '*' --max-time 15 --silent --show-error --fail-with-body \
    --header @- "$API_URL/v1/health"

printf 'Authorization: Bearer %s\n' "$API_TOKEN" |
  curl --noproxy '*' --max-time 15 --silent --show-error --fail-with-body \
    --header @- --header 'Content-Type: application/json' \
    --data '{"method":"get_virtual_devices","params":{}}' \
    "$API_URL/v1/rpc"

unset API_TOKEN API_URL
```

Do not run credential-handling shell commands with tracing (`set -x`). An HTTP library in your own process is preferable for a persistent application.

## 6. Core concepts

| Term | Meaning in this API |
|---|---|
| Channel | A named virtual strip, such as `sink_game`; use its returned `name` as `sink_name` |
| Personal | A channel's normal playback level/mute: `volume_percent` and `muted` |
| Stream | An independent send to the Streamer Mode output; it has its own level and mute |
| Master | The protected bus named **`sink_stream`**; its name does not mean it is the separate Streamer Mode bus |
| Streamer Mode mix | The separate bus named **`sink_streamer_mode`** |
| Bus / mix | A collection of channels with its own volume, mute and membership |
| Hardware volume | Physical device volume; separate from channel and bus volume |
| Default device | The system's current default input/output; a channel may use a pinned output instead |
| Node name | The stable identifier returned as `name` or `node_name`; use it in requests |
| Display label | `label` or `description`, intended for people; do not use it as a node identifier |

Mute fields are booleans: `true` means muted. Muting does not require setting the volume to zero, and a zero volume does not imply `muted: true`. These percentages describe level controls, not live signal peaks or measured loudness. The HTTP API does not provide an audio-meter stream.

Volume popups (the on-screen overlay) can be disabled in **Settings → Popup Overlay** (`overlay_enabled` in `get_prefs`, enabled by default). When disabled, API volume and mute changes still apply but no popup is shown.

The channel field `stream_mix` is a legacy mix-membership flag. It is **not** the Stream mute state or the global Streamer Mode switch. Use `stream_send_muted` and `get_prefs().streamer_mode_enabled` for those.

## 7. Reading state

### Command lookup

All commands below use `POST /v1/rpc`. Commands without arguments take `"params": {}`.

| I want to… | Read command | Relevant result |
|---|---|---|
| Read all channels, both volume paths and both mute states | `get_virtual_devices` | Array of channels; see [channel fields](#channel-state) |
| Read the active profile name | `get_active_profile` | String or `null` |
| Read channels, microphones, mixes and profile together | `get_profile_snapshot` | Snapshot object |
| Read Master and other mix levels/mutes | `list_buses` | Array of mix objects |
| Read microphone levels/mutes | `get_mic_configs` | Array of full microphone configurations |
| Read the system default input/output | `get_default_devices` | `input` and `output` node names, each nullable |
| List available output/input devices | `get_output_devices` / `get_input_devices` | Arrays with `name`, `description`, `index` |
| Read physical device volume/mute | `get_hardware_devices` | Array with `kind`, `volume_percent`, `muted`, `is_default` |
| Read a channel's configured output | `get_channel_outputs` | Map from channel name to device name or `null` |
| Read a channel's effective output | `get_resolved_outputs` | Map of resolved outputs, where supported by the backend |
| Check Streamer Mode and amplification choices | `get_prefs` | `streamer_mode_enabled`, `volume_range_overrides` |
| List available profiles | `list_profiles` | Array of profile information |
| Check commands supported by the running version | `GET /v1/commands` (HTTP endpoint) | Command catalog |

For edits, use the [audio-control recipes](#8-audio-control-recipes). For every command's parameters and return type, use the [complete reference](#14-complete-command-reference).

### Response and field reference

The values below are illustrative. Discover actual names on the running server. Examples labeled “excerpt” omit fields for readability and must not be sent back as complete configuration objects.

#### Channel state

Request:

```json
{"method":"get_virtual_devices","params":{}}
```

Example HTTP response:

```json
{
  "ok": true,
  "result": [
    {
      "name": "sink_game",
      "label": "Game",
      "icon": "sports_esports",
      "volume_percent": 80,
      "muted": false,
      "stream_mix": true,
      "stream_send_volume_percent": 65,
      "stream_send_muted": true
    }
  ]
}
```

| Field | JSON type | Meaning |
|---|---|---|
| `name` | string | Channel identifier used as `sink_name` when writing |
| `label` | string | Display name |
| `icon` | string or null | Optional icon name |
| `volume_percent` | integer | Personal volume |
| `muted` | boolean | Personal mute |
| `stream_mix` | boolean | Legacy mix-membership flag |
| `stream_send_volume_percent` | integer | Independent Stream volume |
| `stream_send_muted` | boolean | Independent Stream mute |

The response means Personal is at 80% and unmuted; Stream is configured at 65% and muted. Whether the Stream path is currently enabled is a separate preference.

#### Active profile and default devices

```json
{"method":"get_active_profile","params":{}}
```

```json
{"ok":true,"result":"Default"}
```

The result can be `null` when there is no active profile. `get_profile_content` reads a saved profile; use `get_profile_snapshot` for the current mixer state.

```json
{"method":"get_default_devices","params":{}}
```

```json
{
  "ok": true,
  "result": {
    "output": "YOUR_OUTPUT_NODE_NAME",
    "input": "YOUR_INPUT_NODE_NAME"
  }
}
```

Either device field can be `null`. Resolve a friendly description by matching the returned name against `get_output_devices` or `get_input_devices`; if no match exists, display the node name and refresh the list. A device can disappear between queries.

#### Mix and microphone fields

A `list_buses` result item looks like this:

```json
{
  "name": "sink_stream",
  "label": "Master",
  "channels": ["sink_game", "sink_chat"],
  "exclude": false,
  "volume_percent": 90,
  "muted": false
}
```

`channels` contains channel identifiers. With `exclude: false`, these are included members; with `exclude: true`, they are excluded members. `volume_percent` and `muted` control this mix. Enumerate buses instead of assuming custom mixes or the Streamer Mode mix are present.

A `get_mic_configs` result item contains these level fields (excerpt):

```json
{
  "node_name": "sink_mic",
  "output_label": "Mixweave Mic",
  "input_device": null,
  "gain_percent": 80,
  "muted": false,
  "stream_send_gain_percent": 70,
  "stream_send_muted": true
}
```

`node_name` identifies the microphone. `input_device` stores its selected input, or `null` for the default. `gain_percent` and `muted` control the Personal microphone path; `stream_send_gain_percent` and `stream_send_muted` control its Stream send. `get_mic_config` returns the primary microphone; `get_mic_configs` returns all configured microphones.

For writes, fetch the full configuration, change only the intended fields and send the full object to `set_mic_config`. The excerpt above omits processing settings and is not a write template.

#### Snapshot coverage

`get_profile_snapshot` combines the following fields in one response:

| Field | Contents |
|---|---|
| `active_profile` | Current profile name or `null` |
| `channels` | Channel objects with Personal and Stream controls |
| `buses` | Mix objects with levels, mutes and membership |
| `mic_configs` | Full microphone configurations |
| `output_devices`, `input_devices` | Available device lists |
| `channel_outputs` | Configured channel output choices; `null` means follow default |
| `resolved_outputs` | Effective channel output choices reported by the backend |
| `channel_failover` | Channel failover settings |
| `eq_configs` | Channel equalizer configurations |
| `app_streams` | Running audio streams |
| `mic_clients` | Microphone clients |
| `seen_apps` | Known application identities |
| `profiles` | Available profile information |

System default device names, global preferences, physical hardware levels, backend information and automation status require separate queries. Use `get_default_devices`, `get_prefs`, `get_hardware_devices`, `get_backend_info` and `get_profile_automation_status`, respectively.

## 8. Audio-control recipes

The following Python snippets use `api = Client.discover()` from the previous section. Writes take effect on live audio and use Mixweave's existing persistence/autosave behavior. They are not dry runs. Choose devices and profiles intentionally rather than blindly selecting the first item in a list.

### Choose a channel and protect profile-scoped edits

```python
snapshot = api.snapshot()
profile = snapshot['active_profile']
channel = next(c for c in snapshot['channels'] if c['name'] == 'sink_game')
guard = {'expected_profile': profile}
```

`sink_game` is an example; enumerate `channels` in your real UI. `expected_profile` rejects an edit if another operation has switched profiles since your read. Omitting it or passing `null` disables that check. It is not a general revision number and does not detect other edits within the same profile.

### Set, raise, or lower a channel's volume

```python
api.rpc('set_channel_volume', sink_name=channel['name'], volume=50, **guard)

# Read again before a relative change. There is no atomic volume-up/down RPC.
snapshot = api.snapshot()
channel = next(c for c in snapshot['channels'] if c['name'] == 'sink_game')
ranges = api.rpc('get_prefs').get('volume_range_overrides') or {}
maximum = 150 if ranges.get(channel['name'], False) else 100
new_volume = max(0, min(maximum, channel['volume_percent'] + 5))  # use -5 to lower
api.rpc('set_channel_volume', sink_name=channel['name'], volume=new_volume,
        expected_profile=snapshot['active_profile'])
```

The read/modify/write pair is not atomic; another client can change the level between requests.

### Mute or unmute

```python
api.rpc('toggle_channel_mute', sink_name=channel['name'], muted=True, **guard)
api.rpc('toggle_channel_mute', sink_name=channel['name'], muted=False, **guard)
```

Despite the method name, `toggle_channel_mute` **sets an explicit boolean**. To implement a toggle button, read `muted` and send its inverse.

### Master volume and mix buses

```python
buses = api.rpc('list_buses')
master = next(bus for bus in buses if bus['name'] == 'sink_stream')
api.rpc('set_bus_volume', name=master['name'], volume=80, **guard)
api.rpc('set_bus_mute', name=master['name'], muted=False, **guard)
```

In this version, `sink_stream` is the protected **Master** bus; do not invent `sink_master`. `sink_streamer_mode` is the separate Streamer Mode mix. Use `list_buses` for names and labels, including custom mixes.

### Device selection: three different operations

| Desired change | Command |
|---|---|
| Set the system's default output/input | `set_default_output` / `set_default_input` |
| Pin a Mixweave playback channel to an output | `set_channel_output` |
| Select the physical input captured by a processed microphone | `set_mic_config` with `input_device` |

```python
outputs = api.rpc('get_output_devices')
inputs = api.rpc('get_input_devices')
print([(d['name'], d['description']) for d in outputs])
print([(d['name'], d['description']) for d in inputs])

# Replace these placeholders with names returned above.
output_name = 'YOUR_OUTPUT_NODE_NAME'
input_name = 'YOUR_INPUT_NODE_NAME'

api.rpc('set_default_output', name=output_name)
api.rpc('set_default_input', name=input_name)
api.rpc('set_channel_output', sink_name=channel['name'],
        output_name=output_name, **guard)

# Empty string means follow the system default, not an explicit device.
api.rpc('set_channel_output', sink_name=channel['name'], output_name='', **guard)
```

Use `name`, not the human-readable `description`. `get_channel_outputs` returns configured choices; `get_resolved_outputs` reports effective routing when supported by the backend. `set_channel_failover` controls fallback behavior if the preferred output disappears. A system-default change can settle asynchronously through WirePlumber: poll the reported default with a finite deadline instead of assuming the first immediate read has changed.

### Physical hardware volume and mute

```python
hardware = api.rpc('get_hardware_devices')
# kind must be exactly "output" or "input".
api.rpc('set_hardware_volume', kind='output', name=output_name, percent=70)
api.rpc('set_hardware_mute', kind='output', name=output_name, muted=False)
api.rpc('set_hardware_volume', kind='input', name=input_name, percent=80)
```

Hardware volume, Master volume, and individual channel volume are separate controls. Hardware commands currently depend on `pactl` and its audio-server connection.

### Microphone gain, mute, input, and processing

```python
snapshot = api.snapshot()
mic = dict(snapshot['mic_configs'][0])  # present a selection for multiple mics
mic.update(gain_percent=80, muted=False, input_device=input_name)
api.rpc('set_mic_config', config=mic,
        expected_profile=snapshot['active_profile'])
```

`set_mic_config` takes the **complete configuration**, not a partial patch. Preserve unrelated fields and `node_name`. `input_device: null` follows the system default input. The object also contains EQ, noise suppression, gate, compressor, limiter, echo cancellation, and independent stream-send settings. For secondary microphones, enable `set_multiple_mics` before adding them. Refresh before another edit to avoid overwriting another client's changes with an old object.

### Streamer Mode

```python
api.rpc('set_streamer_mode_enabled', enabled=True)
api.rpc('set_channel_stream_volume', sink_name=channel['name'], volume=75, **guard)
api.rpc('toggle_channel_stream_mute', sink_name=channel['name'], muted=False, **guard)
```

Personal playback and Stream sends are independent. Disabling Streamer Mode removes its live mix node while retaining its configuration. Microphone stream gain/mute are fields of `MicConfig`.

### Route application audio

```python
streams = api.rpc('get_app_streams')
if streams:
    stream = streams[0]  # select the intended application in your UI
    api.rpc('route_app_to_channel', stream_index=stream['index'],
            sink_name=channel['name'], **guard)
    api.rpc('set_app_volume', stream_index=stream['index'], volume=60)
```

Live stream indices are temporary. Re-enumerate them when applications restart or streams disappear. For identity-based assignment, use `get_seen_apps` and `set_app_assignment` / `set_app_group_assignment`. An `AppIdentity` contains `match_prop` and `match_value`; preserve values supplied by Mixweave. Group-routing commands accept arrays of stream indices and identities. Do not confuse channel node names with stream indices or application display names.

### Switch and manage profiles

```python
profiles = api.rpc('list_profiles')
print([p['name'] for p in profiles])
api.rpc('load_profile', name='YOUR_PROFILE_NAME')
snapshot = api.snapshot()  # discard prior selections and profile guards
```

Creation and copying do not automatically imply activation. Examples:

```python
api.rpc('create_blank_profile', name='My Controller Profile', mic_enabled=False)
api.rpc('copy_profile', source_name='My Controller Profile', name='My Copy')
api.rpc('rename_profile', name='My Copy', new_name='My Renamed Copy')
api.rpc('delete_profile', name='My Renamed Copy')
```

Protected-profile and last-profile safeguards are enforced by the existing commands. Loading a profile changes more than volume: channel definitions, routes, processing, microphones, and mixes can change. Changes may autosave into the active profile. Use `get_profile_content` to inspect a saved profile and `get_profile_snapshot` for live state.

### Equalizer and presets

Read `get_channel_eq_configs` and modify the selected channel's complete configuration. If a channel has no saved entry, one way to obtain a complete flat configuration is the existing text importer:

```python
config = api.rpc('import_eq_config', text='Preamp: 0 dB')
config['enabled'] = True
config['preamp_db'] = -3.0
api.rpc('set_channel_eq', sink_name=channel['name'], config=config, **guard)
```

That example deliberately replaces the channel's EQ with the imported configuration. To retain current processing, start from the current object instead. EQ presets use `list_eq_presets`, `save_user_eq_preset`, and `delete_user_eq_preset`. `export_channel_eq` returns text; it does not open a native save dialog. Spatial processing support depends on the backend and channel capabilities.

### Automation and test audio

Use `get_profile_automation` → edit the returned configuration → `save_profile_automation`. Existing profiles and executable names are validated. `get_profile_automation_status` exposes runtime status; `list_running_applications` helps choose application rules.

The catalog includes channel/microphone recording and playback tests. They can capture or play actual audio: invoke start/stop commands deliberately, keep track of the selected channel/microphone, and stop playback/recording when finished. The HTTP API controls these tests; it does not provide an audio-download or streaming endpoint.

## 9. Data types, limits and Streamer Mode

### Parameter type mapping

| Catalog type | JSON representation |
|---|---|
| `String` | String |
| `bool` | `true` or `false`, not a quoted string |
| `u8` | Integer 0–255 at deserialization; command limits can be lower |
| `u32` | Nonnegative integer, at most 4,294,967,295 |
| `Option<T>` | Value of type T, `null`, or omitted parameter |
| `Vec<T>` | Array of T values |
| Named configuration struct | Object matching that struct's serialized fields |
| Enum | Its serialized string value; see below |

| Control | Current command range |
|---|---|
| Channel volume / channel stream volume / bus volume | 0–100%; up to 150% only when the strip allows amplification |
| Application volume | 0–150% |
| Microphone gain / microphone stream gain | 0–100%; up to 200% only when the mic strip allows amplification |
| Physical input/output hardware volume | 0–100% |

Amplification is an explicit desktop option, persisted in `get_prefs().volume_range_overrides`, keyed by strip node name. Missing entries (or a null map during first migration) mean **disabled**. Personal and Stream lanes share the same choice; `sink_streamer_mode` uses the Master strip's `sink_stream` entry. The API cannot enable this option implicitly. Channel and bus volume requests within the hardware range (0–150%) are clamped to 100% when amplification is disabled. They succeed and show the volume overlay even when already at 100%. Requests above the hardware maximum, and microphone gain requests above the selected limit, still return HTTP 409 with `volume_limit_exceeded`.

### Volume limits: what the client should expect

| Request | Desktop setting | Outcome |
|---|---|---|
| Set channel to 80% | Amplification disabled | Success; channel becomes 80% |
| Set channel to 105% while it is already at 100% | Amplification disabled | Success; channel stays at 100%; overlay still appears on supported desktops |
| Set channel to 150% | Amplification disabled | Success; channel is capped at 100% |
| Set channel to 150% | Amplification enabled | Success; channel becomes 150% |
| Set channel to 151% | Either | HTTP 409, `volume_limit_exceeded` |
| Set microphone gain to 150% | Amplification disabled | HTTP 409, `volume_limit_exceeded` |
| Set Stream volume while Streamer Mode is disabled | Either | HTTP 409, `streamer_mode_disabled` |

Channel saturation also applies to channel Stream volume and bus volume. It does not apply to microphone gain configuration writes. Malformed values or integers outside the parameter type are validation errors, not saturation requests.

A successful volume write returns `result: null`, not the applied percentage. Read the affected channel or bus after the write and display the confirmed value. Do not infer the final value from the requested number or show an error merely because the volume remains at its allowed ceiling.

### Stream availability

Stream writes require `get_prefs().streamer_mode_enabled == true`. Otherwise the API returns HTTP 409 with `streamer_mode_disabled` and explains how to enable the mode. This covers channel Stream volume/mute, Streamer Mode bus writes, microphone Stream gain/mute changes, and Stream microphone monitoring. Read operations and Personal controls remain available. A full `set_mic_config` containing unchanged Stream fields is allowed for Personal/processing edits. Enable the mode deliberately with `set_streamer_mode_enabled` and `{ "enabled": true }` before Stream writes.

Several other commands clamp values to their supported range. Out-of-range JSON integers that cannot deserialize into the parameter type fail before command execution. Read the resulting state when exact values matter.

`DeviceLabelStyle`: `plain`, `suffix`, `prefix`.

`MeterMode`: `monitor`, `fps_144`, `fps_120`, `fps_100`, `fps_60`, `off`.

For complex objects, retain fields from the running server. The relevant source definitions are:

| Object | Source / discovery |
|---|---|
| `VirtualSink`, `AppStream`, `OutputDevice`, `MicConfig`, `EqConfig`, EQ bands and test statuses | [Audio types](../src-tauri/src/audio/types.rs); use channel, stream, microphone, and EQ queries |
| `AppIdentity`, seen applications | [Application commands](../src-tauri/src/commands/apps.rs); `get_seen_apps` |
| `BusDef` | [Bus definitions](../src-tauri/src/persistence/buses.rs); `list_buses` |
| `Prefs`, `DeviceLabelStyle`, `MeterMode` | [Preferences](../src-tauri/src/persistence/prefs.rs); `get_prefs` |
| `ProfileAutomationConfig` | [Automation definitions](../src-tauri/src/persistence/profile_automation.rs); `get_profile_automation` |
| Live profile snapshot and saved profile content | [Profile commands](../src-tauri/src/commands/profiles.rs) |
| Hardware device state | [Hardware commands](../src-tauri/src/commands/hardware.rs); `get_hardware_devices` |


For example, an output choice map uses node-name keys and nullable values:

```json
{"sink_game": null, "sink_chat": "YOUR_OUTPUT_NODE_NAME"}
```

`null` means following the system default. Do not send this map to `set_channel_output`: that command takes one `sink_name` and an `output_name` string, where `""` requests the default.

## 10. Errors, concurrency and reconnection

| HTTP status | API code, when provided | Client action |
|---|---|---|
| 400 | `invalid_params` | Fix JSON, argument names, missing fields, or types. |
| 401 | `unauthorized` | Re-read discovery, then check health. Do not loop indefinitely. |
| 403 | `browser_origin` / `invalid_host` | Remove browser-origin access; use the discovered URL from a native client. |
| 404 | `unknown_method` | Refresh the catalog; the command may not exist in this build. Unknown paths may return no API code. |
| 409 | `volume_limit_exceeded` | Request exceeds the hardware maximum or the microphone gain limit. Channel/bus requests within 0–150% clamp to the selected ceiling. |
| 409 | `streamer_mode_disabled` | Enable Streamer Mode before writing Stream controls. |
| 409 | `command_failed` | Read the message and fresh state. A device, profile, backend operation, or persistence step failed. |
| 413 | Framework-dependent | Reduce the body below 1 MiB. |
| 415 | `content_type` | Send `Content-Type: application/json`. |
| 429 | `busy` | Another RPC is running. This request did not execute; retry with a bounded delay. |
| 500 | `internal_error` | Report the failure and inspect current state before another write. |

Only one `/v1/rpc` command is executed at a time across all API clients. This includes read commands. Health and catalog requests are separate. Use a serial queue, not simultaneous requests for every control. There is no batch transaction and no idempotency-key feature.

The GUI and background automation can still change state. Existing command locks protect profile operations, but a sequence of separate HTTP calls does not lock out other clients. Use `expected_profile` where supported and refresh after profile switches.

An HTTP timeout, client disconnect, or cancellation does **not** roll back an already-started command. Do not blindly resend writes—especially creation/deletion or recording actions—after losing the response. Inspect state first. A failed command can also leave partial changes if an underlying rollback fails; the command's message may describe this.

A sensible reconnect flow:

1. Stop submitting new writes when the connection fails.
2. Re-read discovery; its token or URL may have changed.
3. Authenticate with health and validate `api_version`.
4. Refresh the catalog and live state.
5. Re-enable controls using current entity IDs and profile context.

Do not automatically replay an old queue of writes into a new application session.

## 11. Building your own control panel

A practical client lifecycle is:

```text
Discovery → authenticated health → command catalog → snapshot/backend/prefs
    → render controls → serialized RPC write → refresh affected state
```

- Keep network work off the GUI thread. Use a Qt worker or asynchronous client.
- Poll at a modest rate suited to your UI and avoid overlapping reads; v1 provides no WebSocket or SSE feed.
- Debounce slider movement or send on release. Do not queue hundreds of outdated slider values.
- Show pending/error state and display the server-confirmed value after writes.
- Rebuild selectors after channel/device/profile changes. Do not keep stale stream IDs.
- Distinguish hardware volume, Master level, channel level, microphone gain, and Stream-send level.
- Inspect `get_backend_info` (`native`) before offering processing features. Catalog presence alone does not guarantee backend/device support.
- Keep complete configuration objects when editing a small number of fields.
- Unless the user disabled the overlay in Settings (`overlay_enabled` is `false` in `get_prefs`), successful API volume/mute commands trigger the existing desktop OSD for channels, buses, microphone gain/mute, Stream sends, hardware devices, and application volume. This is independent of whether keyboard shortcuts are enabled and uses the saved overlay style, position, and theme. Microphone processing-only edits and failed writes do not trigger a volume popup.
- OSD display uses the existing X11 overlay or GNOME Shell extension on GNOME/Wayland. Other Wayland compositors are not given a new overlay implementation by the API.
- Internal `local-api-changed` and `local-api-osd` notifications refresh Mixweave's UI and forward OSD updates. They are not public HTTP subscription endpoints.

v1 deliberately does not expose arbitrary Tauri IPC, application reset/restart, updater installation, backup restoration/file dialogs, desktop shortcut management, or raw audio transport. Global shortcut behavior and desktop permissions are separate from local API control.

## 12. Testing your integration

### Supplied PyQt6 console

From the repository root, in your system terminal:

```sh
python3 -m venv tools/api-panel/.venv
./tools/api-panel/.venv/bin/pip install -r tools/api-panel/requirements.txt
./tools/api-panel/run.sh
```

The launcher also recognizes `.venv-host` when a host-specific environment has been installed. Python environments created inside a sandbox may not work with the host Python ABI; create/install the environment with the Python that will run it.

In the console:

1. Start Mixweave, then click **Discover**.
2. Click **Connect / Refresh**.
3. **Mixer & devices** controls live channel, bus, microphone, hardware, and profile state.
4. **All API commands** exposes the complete catalog and a JSON parameter editor. Enable **Enable mutating request** before sending a write.
5. **Diagnostics** runs read queries plus authentication/validation checks and can save a JSON report.

The diagnostics tab does not automatically exercise every mutating operation. Template values in the editor are placeholders; populate complex types from their matching read commands before sending them.

### Repository checks

```sh
cargo test --manifest-path src-tauri/Cargo.toml --lib local_api::
node scripts/api/test-osd-bridge.cjs
python3 -m unittest discover -s tools/api-panel -p test_client.py -v
QT_QPA_PLATFORM=offscreen tools/api-panel/.venv/bin/python tools/api-panel/test_ui.py
```

The isolated live suite requires a Linux host with PipeWire, WirePlumber, `pactl`, `pw-cat`, Python, D-Bus tools, Tauri dependencies, and an accessible Wayland session:

```sh
cargo build --manifest-path src-tauri/Cargo.toml --features tauri/custom-protocol
dbus-run-session -- scripts/api/run-isolated-test.sh
```

It creates private configuration directories and audio sockets, disables hardware monitoring in that test WirePlumber instance, and creates virtual test devices. It writes `tools/api-panel/isolated-test-report.json` and retains logs under `/tmp/mixweave-api-test.*`. It is not a test against your desktop's physical audio devices.

Record validation results with the tested commit, application version, date, backend, environment and exact commands. A past successful run is not evidence that the current checkout passes. The isolated suite validates its exercised operations, not every mutation, physical device or Linux distribution.

For a volume-control integration, verify at least:

- Read Personal and Stream levels/mutes independently.
- Raise a channel already at 100% with amplification disabled: the request succeeds, the confirmed volume stays at 100%, and the supported desktop overlay appears.
- Enable amplification in the desktop UI and confirm a channel can reach 150%.
- Confirm disabled Streamer Mode rejects Stream writes while reads and Personal writes remain usable.
- Switch profiles and refresh channel identifiers before further edits.
- Restart Mixweave and rediscover the endpoint/token before reconnecting.

## 13. Linux compatibility and troubleshooting

The HTTP control path does not synthesize key presses and does not depend on X11 global-key grabs or Wayland shortcut permission. It calls the same Rust audio commands used by Mixweave. The application and client still need a working user session, audio backend, and relevant runtime libraries.

Advanced processing depends on the native PipeWire backend. The `pactl` fallback does not implement every native feature. Hardware volume/mute uses `pactl`, including when the main backend is native. Distribution versions, missing `pipewire-pulse`, WirePlumber behavior, sandbox isolation, and disconnected devices can affect operations.

| Symptom | What to check |
|---|---|
| Mixweave window says it cannot connect to `localhost:1420` | You launched a development build without Vite. Use the standalone build or `npm run tauri -- dev`. |
| Discovery file missing | API-enabled Mixweave running? API disabled? Correct user and XDG configuration directory? Startup error in stderr? |
| Connection refused | Stale discovery file, application exited, startup failure, or different network namespace. Rediscover and check health. |
| HTTP 401 | Token changed after restart; reload the correct discovery file. |
| HTTP 403 | Exact numeric loopback URL/Host required; `Origin` is forbidden. |
| HTTP 429 | Serialize requests, including polling, and add a bounded retry delay. |
| HTTP 409 | Read the message. Re-enumerate devices/profiles and inspect backend support. |
| Default device appears unchanged immediately | Wait for session-manager propagation and poll with a deadline. |
| Hardware query fails | Check `pactl` availability and access to the user's PulseAudio-compatible server. |
| Qt window fails to open | Install the matching Qt platform dependencies. If XWayland is available, `QT_QPA_PLATFORM=xcb ./tools/api-panel/run.sh` is an alternative to Wayland rendering. |
| Volume response succeeds but the displayed value differs | Check clamping, concurrent edits, Master scaling, and the difference between configured and actual hardware levels. |

No claim is made that all Linux versions or all physical USB/Bluetooth devices behave identically. Start with the health/backend queries, then validate the operations your client needs.

## 14. Complete command reference

The tables below reflect [commands.json](../src-tauri/src/local_api/commands.json). The running server's `/v1/commands` is authoritative for its build.

**Notation:** `?` after a parameter name means it can be omitted or `null`. All other parameters are required. `Read`/`Write` reflects the catalog classification. Result types below come from the current command signatures: `()` is returned as JSON `null`, `Option<T>` can return `null`, vectors become arrays, and maps become objects. Domain-specific return types are serialized objects; consult the linked source and matching read responses.

### Apps

[Implementation](../src-tauri/src/commands/apps.rs)

| Method | Mode | Parameters | Result |
|---|---|---|---|
| `get_seen_apps` | Read | None | `Vec<SeenApp>` |
| `set_app_group_ignored` | Write | `identities: Vec<AppIdentity>`, `ignored: bool` | `()` |
| `set_app_ignored` | Write | `match_prop: String`, `match_value: String`, `ignored: bool` | `()` |
| `forget_app` | Write | `match_prop: String`, `match_value: String`, `expected_profile?: Option<String>` | `()` |
| `forget_app_group` | Write | `identities: Vec<AppIdentity>`, `expected_profile?: Option<String>` | `()` |
| `set_app_assignment` | Write | `match_prop: String`, `match_value: String`, `sink_name: String`, `expected_profile?: Option<String>` | `()` |
| `set_app_group_assignment` | Write | `identities: Vec<AppIdentity>`, `sink_name: String`, `expected_profile?: Option<String>` | `()` |

### Buses

[Implementation](../src-tauri/src/commands/buses.rs)

| Method | Mode | Parameters | Result |
|---|---|---|---|
| `list_buses` | Read | None | `Vec<BusDef>` |
| `add_bus` | Write | `label: String`, `expected_profile?: Option<String>` | `()` |
| `rename_bus` | Write | `name: String`, `label: String`, `expected_profile?: Option<String>` | `()` |
| `set_streamer_mode_enabled` | Write | `enabled: bool` | `()` |
| `remove_bus` | Write | `name: String`, `expected_profile?: Option<String>` | `()` |
| `set_bus_members` | Write | `name: String`, `channels: Vec<String>`, `expected_profile?: Option<String>` | `()` |
| `set_bus_exclude` | Write | `name: String`, `exclude: bool`, `expected_profile?: Option<String>` | `()` |
| `set_bus_volume` | Write | `name: String`, `volume: u8`, `expected_profile?: Option<String>` | `()` |
| `set_bus_mute` | Write | `name: String`, `muted: bool`, `expected_profile?: Option<String>` | `()` |

### Channels

[Implementation](../src-tauri/src/commands/channels.rs)

| Method | Mode | Parameters | Result |
|---|---|---|---|
| `add_channel` | Write | `label: String`, `icon?: Option<String>`, `spatial: bool`, `expected_profile?: Option<String>` | `()` |
| `reorder_channels` | Write | `order: Vec<String>`, `expected_profile?: Option<String>` | `()` |
| `set_channel_icon` | Write | `sink_name: String`, `icon: String`, `expected_profile?: Option<String>` | `()` |
| `rename_channel` | Write | `sink_name: String`, `label: String`, `expected_profile?: Option<String>` | `()` |
| `remove_channel` | Write | `sink_name: String`, `expected_profile?: Option<String>` | `()` |

### Channel Test

[Implementation](../src-tauri/src/commands/channel_test.rs)

| Method | Mode | Parameters | Result |
|---|---|---|---|
| `get_channel_test_status` | Read | `sink_name: String` | `ChannelTestStatus` |
| `start_channel_test_recording` | Write | `sink_name: String` | `ChannelTestStatus` |
| `stop_channel_test_recording` | Write | `sink_name: String` | `ChannelTestStatus` |
| `play_channel_test_loop` | Write | `sink_name: String` | `ChannelTestStatus` |
| `stop_channel_test_playback` | Write | `sink_name: String` | `ChannelTestStatus` |
| `play_channel_test_sample` | Write | `sink_name: String`, `sample: String` | `ChannelTestStatus` |

### Devices

[Implementation](../src-tauri/src/commands/devices.rs)

| Method | Mode | Parameters | Result |
|---|---|---|---|
| `get_virtual_devices` | Read | None | `Vec<VirtualSink>` |
| `get_app_streams` | Read | None | `Vec<AppStream>` |
| `get_output_devices` | Read | None | `Vec<OutputDevice>` |
| `get_channel_outputs` | Read | None | `std::collections::HashMap<String, Option<String>>` |
| `get_resolved_outputs` | Read | None | `std::collections::HashMap<String, Option<String>>` |
| `get_channel_failover` | Read | None | `std::collections::HashMap<String, bool>` |
| `set_channel_output` | Write | `sink_name: String`, `output_name: String`, `expected_profile?: Option<String>` | `()` |
| `set_channel_failover` | Write | `sink_name: String`, `enabled: bool`, `expected_profile?: Option<String>` | `()` |

### Eq

[Implementation](../src-tauri/src/commands/eq.rs)

| Method | Mode | Parameters | Result |
|---|---|---|---|
| `get_channel_eq_configs` | Read | None | `HashMap<String, EqConfig>` |
| `set_channel_eq` | Write | `sink_name: String`, `config: EqConfig`, `expected_profile?: Option<String>` | `()` |
| `test_spatial_channel` | Write | `sink_name: String`, `channel: String` | `()` |
| `list_eq_presets` | Read | `sink_name: String` | `Vec<EqPresetEntry>` |
| `save_user_eq_preset` | Write | `sink_name: String`, `name: String`, `config: EqConfig` | `()` |
| `delete_user_eq_preset` | Write | `sink_name: String`, `name: String` | `()` |
| `export_channel_eq` | Read | `sink_name: String` | `String` |
| `import_eq_config` | Read | `text: String` | `EqConfig` |

### Hardware

[Implementation](../src-tauri/src/commands/hardware.rs)

| Method | Mode | Parameters | Result |
|---|---|---|---|
| `get_hardware_devices` | Read | None | `Vec<HardwareDevice>` |
| `set_hardware_volume` | Write | `kind: String`, `name: String`, `percent: u8` | `()` |
| `set_hardware_mute` | Write | `kind: String`, `name: String`, `muted: bool` | `()` |

### Mic

[Implementation](../src-tauri/src/commands/mic.rs)

| Method | Mode | Parameters | Result |
|---|---|---|---|
| `get_mic_config` | Read | None | `MicConfig` |
| `get_mic_configs` | Read | None | `Vec<MicConfig>` |
| `set_mic_config` | Write | `config: MicConfig`, `expected_profile?: Option<String>` | `()` |
| `add_mic_channel` | Write | `label: String`, `input_device?: Option<String>`, `copy_from?: Option<String>`, `expected_profile?: Option<String>` | `MicConfig` |
| `remove_mic_channel` | Write | `node_name: String`, `expected_profile?: Option<String>` | `()` |
| `reorder_mic_channels` | Write | `order: Vec<String>`, `expected_profile?: Option<String>` | `()` |
| `get_input_devices` | Read | None | `Vec<OutputDevice>` |
| `get_mic_clients` | Read | None | `Vec<MicClient>` |
| `get_mic_test_status` | Read | `node_name?: Option<String>` | `MicTestStatus` |
| `start_mic_test_recording` | Write | `node_name?: Option<String>` | `MicTestStatus` |
| `stop_mic_test_recording` | Write | `node_name?: Option<String>` | `MicTestStatus` |
| `play_mic_test_loop` | Write | `node_name?: Option<String>` | `MicTestStatus` |
| `stop_mic_test_playback` | Write | `node_name?: Option<String>` | `MicTestStatus` |
| `list_mic_presets` | Read | None | `Vec<MicPreset>` |
| `save_mic_preset` | Write | `name: String`, `config: MicConfig` | `()` |
| `delete_mic_preset` | Write | `name: String` | `()` |

### Profiles

[Implementation](../src-tauri/src/commands/profiles.rs)

| Method | Mode | Parameters | Result |
|---|---|---|---|
| `list_profiles` | Read | None | `Vec<ProfileInfo>` |
| `get_profile_snapshot` | Read | None | `FrontendProfileSnapshot` |
| `get_profile_content` | Read | `name: String` | `ProfileContent` |
| `get_active_profile` | Read | None | `Option<String>` |
| `set_profile_trigger` | Write | `name: String`, `device: String` | `()` |
| `load_profile` | Write | `name: String` | `()` |
| `create_blank_profile` | Write | `name: String`, `mic_enabled: bool` | `()` |
| `copy_profile` | Write | `source_name: String`, `name: String` | `()` |
| `rename_profile` | Write | `name: String`, `new_name: String` | `()` |
| `delete_profile` | Write | `name: String` | `()` |

### Routing

[Implementation](../src-tauri/src/commands/routing.rs)

| Method | Mode | Parameters | Result |
|---|---|---|---|
| `route_app_to_channel` | Write | `stream_index: u32`, `sink_name: String`, `expected_profile?: Option<String>` | `()` |
| `route_app_group_to_channel` | Write | `stream_indices: Vec<u32>`, `identities: Vec<AppIdentity>`, `desktop_id?: Option<String>`, `sink_name: String`, `expected_profile?: Option<String>` | `()` |
| `set_channel_volume` | Write | `sink_name: String`, `volume: u8`, `expected_profile?: Option<String>` | `()` |
| `toggle_channel_mute` | Write | `sink_name: String`, `muted: bool`, `expected_profile?: Option<String>` | `()` |
| `set_channel_stream_volume` | Write | `sink_name: String`, `volume: u8`, `expected_profile?: Option<String>` | `()` |
| `toggle_channel_stream_mute` | Write | `sink_name: String`, `muted: bool`, `expected_profile?: Option<String>` | `()` |
| `set_monitor` | Write | `sink_name: String`, `enabled: bool` | `()` |
| `rename_app` | Write | `match_prop: String`, `match_value: String`, `alias: String` | `()` |
| `set_app_volume` | Write | `stream_index: u32`, `volume: u8` | `()` |

### Settings

[Implementation](../src-tauri/src/commands/settings.rs)

| Method | Mode | Parameters | Result |
|---|---|---|---|
| `get_backend_info` | Read | None | `BackendInfo` |
| `get_prefs` | Read | None | `Prefs` |
| `set_device_label_style` | Write | `style: DeviceLabelStyle` | `()` |
| `set_meter_mode` | Write | `mode: MeterMode` | `()` |
| `set_multiple_mics` | Write | `enabled: bool` | `()` |
| `set_balance_visible` | Write | `visible: bool` | `()` |
| `set_balance_channels` | Write | `a?: Option<String>`, `b?: Option<String>` | `()` |
| `get_default_devices` | Read | None | `DefaultDevices` |
| `set_default_output` | Write | `name: String` | `()` |
| `set_default_input` | Write | `name: String` | `()` |

### Profile Automation

[Implementation](../src-tauri/src/profile_automation.rs)

| Method | Mode | Parameters | Result |
|---|---|---|---|
| `get_profile_automation` | Read | None | `ProfileAutomationConfig` |
| `save_profile_automation` | Write | `config: ProfileAutomationConfig` | `()` |
| `get_profile_automation_status` | Read | None | `AutomationStatus` |
| `list_running_applications` | Read | None | `Vec<RunningApplication>` |

## 15. Maintaining this guide (for contributors)

The allowlisted dispatcher and command catalog are generated by [generate_registry.py](../scripts/api/generate_registry.py). When command signatures or API behavior change, regenerate the registry, review the allowlist, and update the reference tables and examples here. The transport implementation is [local_api/mod.rs](../src-tauri/src/local_api/mod.rs).

This is the canonical English guide. Keep the task lookup, examples, field descriptions and command tables consistent with the implementation. `docs/local-api.md` is a compatibility link for older references.

When updating this guide, check that all catalog commands appear exactly once in the reference, required/optional parameters match the catalog, local links resolve, and JSON/Python examples parse. Behavior changes also need matching examples and error guidance; regenerating the command catalog alone is insufficient.
