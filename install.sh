#!/usr/bin/env bash
set -euo pipefail

project_dir=$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")" && pwd)
cd "$project_dir"

missing=()
for command_name in node npm cargo rustc cc pkg-config install; do
  if ! command -v "$command_name" >/dev/null 2>&1; then
    missing+=("$command_name")
  fi
done

if ((${#missing[@]})); then
  printf 'Missing build commands: %s\n' "${missing[*]}" >&2
  printf 'Install the build dependencies listed in README.md, then run this script again.\n' >&2
  exit 1
fi

missing_libraries=()
for library in gtk+-3.0 webkit2gtk-4.1 libpipewire-0.3 libmysofa fftw3f ayatana-appindicator3-0.1; do
  if ! pkg-config --exists "$library"; then
    missing_libraries+=("$library")
  fi
done

if ((${#missing_libraries[@]})); then
  printf 'Missing development libraries: %s\n' "${missing_libraries[*]}" >&2
  printf 'Install the build dependencies listed in README.md, then run this script again.\n' >&2
  exit 1
fi

printf 'Building Mixweave...\n'
npm ci
npm run build
cargo build --release --locked --manifest-path src-tauri/Cargo.toml

bin_home=${XDG_BIN_HOME:-"$HOME/.local/bin"}
data_home=${XDG_DATA_HOME:-"$HOME/.local/share"}
app_id=dev.sonux.audio

install -Dm755 target/release/mixweave "$bin_home/mixweave"
for icon_size in 128x128:128x128.png 256x256:128x128@2x.png 512x512:icon.png; do
  install -Dm644 "src-tauri/icons/${icon_size#*:}" \
    "$data_home/icons/hicolor/${icon_size%%:*}/apps/$app_id.png"
done
install -Dm644 LICENSE "$data_home/doc/mixweave/GPL-3.0.txt"
install -Dm644 THIRD_PARTY_NOTICES.md "$data_home/doc/mixweave/THIRD_PARTY_NOTICES.md"
install -Dm644 src/styles/fonts/LICENSE.txt "$data_home/doc/mixweave/FIRA_CODE_LICENSE.txt"
install -Dm644 third_party/licenses/APACHE-2.0.txt \
  "$data_home/doc/mixweave/APACHE-2.0.txt"
install -Dm644 third_party/licenses/RNNOISE_BSD-3-Clause.txt \
  "$data_home/doc/mixweave/RNNOISE_BSD-3-Clause.txt"
install -Dm644 third_party/licenses/WEBRTC_SONORA_BSD-3-Clause.txt \
  "$data_home/doc/mixweave/WEBRTC_SONORA_BSD-3-Clause.txt"
install -Dm644 src-tauri/assets/test-audio/LICENSES.md \
  "$data_home/doc/mixweave/TEST_AUDIO_LICENSES.md"
install -Dm644 third_party/spatial/licenses/AALTO_HRTF_ATTRIBUTION.md \
  "$data_home/doc/mixweave/AALTO_HRTF_ATTRIBUTION.md"

desktop_file="$data_home/applications/$app_id.desktop"
install -d "$(dirname -- "$desktop_file")"
{
  printf '%s\n' '[Desktop Entry]'
  printf '%s\n' 'Type=Application'
  printf '%s\n' 'Name=Mixweave'
  printf '%s\n' 'Comment=PipeWire gaming audio router and mixer'
  printf 'Exec=%s\n' "$bin_home/mixweave"
  printf 'TryExec=%s\n' "$bin_home/mixweave"
  printf 'Icon=%s\n' "$app_id"
  printf '%s\n' 'Terminal=false'
  printf '%s\n' 'Categories=AudioVideo;Audio;Utility;'
  printf '%s\n' 'Keywords=audio;mixer;pipewire;gaming;microphone;'
} > "$desktop_file"
chmod 644 "$desktop_file"

if command -v update-desktop-database >/dev/null 2>&1; then
  update-desktop-database "$data_home/applications" >/dev/null 2>&1 || true
fi

# Remove the legacy executable left behind by the application rename.
if [[ -e "$bin_home/sonux" ]]; then
  rm -f "$bin_home/sonux"
fi

printf '\nMixweave installed successfully.\n'
printf 'Application: %s\n' "$bin_home/mixweave"
printf 'Launcher:    %s\n' "$desktop_file"
printf 'Licenses:    %s\n' "$data_home/doc/mixweave"
if [[ :$PATH: != *":$bin_home:"* ]]; then
  printf 'Note: add %s to PATH to launch Mixweave by typing "mixweave".\n' "$bin_home"
fi
