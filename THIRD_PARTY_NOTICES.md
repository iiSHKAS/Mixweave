# Third-party notices

## Bundled test audio

The six application test clips under `src-tauri/assets/test-audio` are derived
from works released under Creative Commons Zero 1.0. Source and creator details
are recorded in `src-tauri/assets/test-audio/LICENSES.md`.

## Aalto near-field HRTF

`third_party/spatial/runtime/NF_LIB_HRTF_LFE.sofa` comes from the Aalto
University near-field HRTF database, Zenodo DOI 10.5281/zenodo.7316545.

Creators: Sebastian Prepelita, Javier Gomez Bolanos, Ville Pulkki, and Marton
Marschall. License: Creative Commons Attribution 4.0 International. Full
attribution is in `third_party/spatial/licenses/AALTO_HRTF_ATTRIBUTION.md`.

## Fira Code

The bundled Fira Code fonts are Copyright (c) 2014, The Fira Code Project
Authors and licensed under the SIL Open Font License 1.1. The full license is
in `src/styles/fonts/LICENSE.txt`.

## Material Symbols

The bundled Material Symbols icon font is distributed under Apache-2.0. The
Apache 2.0 text is in `third_party/licenses/APACHE-2.0.txt`. Only the ~100
glyphs the interface actually uses are bundled (a subset of the upstream
`material-symbols` package, rebuilt by `scripts/subset-material-symbols.py`)
rather than the full ~4000-icon font.

## RNNoise (microphone noise suppression)

Noise suppression uses the `nnnoiseless` crate, a pure-Rust port of Xiph's
RNNoise (Copyright (c) 2020 Joe Neeman; 2017 Mozilla; 2007-2017 Jean-Marc
Valin; 2005-2017 Xiph.Org Foundation; 2003-2004 Mark Borgerding), licensed under
the BSD 3-Clause license. The license text is packaged as
`licenses/RNNOISE_BSD-3-Clause.txt`.

## WebRTC audio processing (microphone echo cancellation)

Echo cancellation uses the `sonora` crates, a pure-Rust port of WebRTC's audio
processing (Copyright (c) 2011 The WebRTC Project Authors; 2016 Arun Raghavan
and contributors; 2026 dignifiedquire), licensed under the BSD 3-Clause
license. The license text is packaged as
`licenses/WEBRTC_SONORA_BSD-3-Clause.txt`.

## pipewire-rs system bindings

The patched `libspa-sys` and `pipewire-sys` crates under `src-tauri/vendor`
come from pipewire-rs v0.10.0 and are licensed under the MIT License. The
upstream license text is packaged as `licenses/PIPEWIRE_RS_MIT.txt`; the patch
rationale is recorded in `src-tauri/vendor/README.md` in the source tree.
