<h1 align="center">Mixweave</h1>

<p align="center">
  <strong>Your audio. Your mix. Your stream.</strong>
</p>

Mixweave is an audio mixer for Linux, built on PipeWire. Route applications to dedicated channels, reduce microphone noise, and control what you hear separately from what your audience hears.

Mixweave is based on [Sonux](https://github.com/Haxinpro/Sonux) by [Haxinpro](https://github.com/Haxinpro). Thanks to the original project and its contributors for providing the foundation for Mixweave.

<p align="center">
  <a href="https://github.com/iiSHKAS/Mixweave/releases/latest"><strong>Download</strong></a>
  &nbsp;·&nbsp;
  <a href="#compatibility">Compatibility</a>
  &nbsp;·&nbsp;
  <a href="https://github.com/iiSHKAS/Mixweave/issues">Report an issue</a>
</p>

---

## Channel Controls

Control volume and mute for individual channels, and assign games, voice chat, music, and other applications to the channels you want. In **Normal Mode**, each channel can use its own audio output.

![Mixweave in Normal Mode, showing application channels and microphone controls](https://raw.githubusercontent.com/iiSHKAS/Mixweave/main/image/normal-mode.png)

## Streamer Mode

Manage two independent mixes: **Personal** for what you hear and **Stream** for what your audience hears. Each channel has separate volume and mute controls for both mixes.

Keep music in your headphones while muting it for your stream, or lower game audio for your audience without changing your own listening volume.

Select **Streamer Mode** as the audio source in your streaming software. The stream monitoring button on **Master** lets you preview the audience's mix.

![Mixweave in Streamer Mode, with separate Personal and Stream controls](https://raw.githubusercontent.com/iiSHKAS/Mixweave/main/image/streamer-mode.png)

## Application Routing

The **Apps** page lists applications that have produced audio and lets you assign each one to a channel. Assignments are remembered across restarts.

## Automatic Profiles

Save mixer configurations for games, calls, music, or streaming, and link them to application or game executables.

Mixweave switches profiles automatically when a linked application starts. If several linked applications are running, the most recently started one takes priority. When none remain running, Mixweave returns to **Default**.

## Equalizer & Presets

Shape channel audio with the equalizer, choose a built-in preset, or create and save your own.

## Microphone Controls

| Control | Function |
| :--- | :--- |
| **Noise suppression** | Reduce background noise with RNNoise. |
| **Mute** | Click the microphone mute button to toggle mute. |
| **Monitoring** | Hold the mute button to listen to your microphone; release it to stop. |

## Keyboard Shortcuts & Volume Overlays

Assign global shortcuts for volume up, volume down, and mute. Each volume adjustment changes the level by **5%**, and holding a volume shortcut repeats the adjustment. An optional setting extends the volume range to **150%**.

Choose from three overlay styles: **Segments**, **Vertical Fader**, and **Waves**. Overlays display the channel name and add a **Stream** label when adjusting stream audio. They can appear over fullscreen applications on supported desktops.

![Mixweave volume overlays in Segments, Vertical Fader, and Waves styles](https://raw.githubusercontent.com/iiSHKAS/Mixweave/main/image/volume-overlays.png)

## Desktop Integration

| Appearance | Desktop | Startup |
| :--- | :--- | :--- |
| Light, dark, and system themes | System tray support | Launch at login and start minimized |

## Languages

| English | Arabic | Russian | Simplified Chinese |
| :---: | :---: | :---: | :---: |
| **English** | **العربية** | **Русский** | **简体中文** |

Choose a language in **Settings** or follow your system language.

---

## Download

Get **`Mixweave.AppImage`** from the [latest release](https://github.com/iiSHKAS/Mixweave/releases/latest), make it executable, and launch it:

```bash
chmod +x Mixweave.AppImage
./Mixweave.AppImage
```

> [!NOTE]
> Automatic updates are available for **AppImage builds only** and can be managed in **Settings → Updates**.

## Compatibility

Mixweave requires **PipeWire** with **PulseAudio compatibility**, **WirePlumber 0.5 or newer**, and **`pactl`**.

**Global shortcuts & volume overlays**

| Desktop session | Support |
| :--- | :--- |
| **X11** | Supported |
| **GNOME on Wayland** | Supported |
| **KDE Plasma on Wayland** | Planned |
| **Other Wayland desktops** | Not currently supported |

GNOME overlays use a bundled Shell extension. You may need to log out and back in after its first installation or an update.

---

## Credits & License

Mixweave builds on **[Sonux](https://github.com/Haxinpro/Sonux)** by **[Haxinpro](https://github.com/Haxinpro)**, with foundations in **[Sink](https://github.com/NC1107/sink)** by **[NC1107](https://github.com/NC1107)**. Credit and thanks go to the original authors and contributors whose work made this project possible.

Mixweave is licensed under **GPL-3.0-only**. Bundled third-party components and assets retain their respective licenses.
