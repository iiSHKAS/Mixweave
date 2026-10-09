// Mixweave OSD - GNOME Shell extension.
//
// Wayland gives ordinary application windows no protocol to demand
// always-on-top placement (that's the compositor's call, by design), so a
// second Mixweave-drawn window can never appear over a fullscreen game the
// way it can on X11. A Shell extension runs *inside* the compositor process
// itself, so it can draw chrome above everything - including fullscreen
// windows, via `trackFullscreen` - without needing any such protocol at all.
//
// Mixweave's Rust backend calls the `ShowStyled` method below over the
// session D-Bus (dev.sonux.Osd) every time a keyboard shortcut mutes or
// changes a channel/bus/mic's volume; see linux_shortcuts.rs's
// show_gnome_osd command. `Show` and `ShowThemed` are older signatures that
// older Mixweave builds still call. This file is installed and kept enabled
// by Mixweave itself (ensure_installed in gnome_osd_extension.rs) - there is
// nothing to set up by hand.
//
// Three looks are drawn (chosen in Mixweave's Settings): "segments",
// "fader" and "waves". Each builder returns { root, update, start, stop };
// if a builder throws, the plain "classic" popup below is used instead so a
// bug in one look can never leave the shortcut without any popup.

import St from 'gi://St';
import Clutter from 'gi://Clutter';
import GLib from 'gi://GLib';
import Gio from 'gi://Gio';
import Pango from 'gi://Pango';
import {Extension} from 'resource:///org/gnome/shell/extensions/extension.js';
import * as Main from 'resource:///org/gnome/shell/ui/main.js';

const DBUS_INTERFACE = `
<node>
  <interface name="dev.sonux.Osd">
    <method name="Show">
      <arg type="s" direction="in" name="label"/>
      <arg type="i" direction="in" name="volume_percent"/>
      <arg type="i" direction="in" name="max"/>
      <arg type="b" direction="in" name="muted"/>
    </method>
    <method name="ShowThemed">
      <arg type="s" direction="in" name="label"/>
      <arg type="i" direction="in" name="volume_percent"/>
      <arg type="i" direction="in" name="max"/>
      <arg type="b" direction="in" name="muted"/>
      <arg type="s" direction="in" name="theme"/>
    </method>
    <method name="ShowStyled">
      <arg type="s" direction="in" name="label"/>
      <arg type="i" direction="in" name="volume_percent"/>
      <arg type="i" direction="in" name="max"/>
      <arg type="b" direction="in" name="muted"/>
      <arg type="s" direction="in" name="theme"/>
      <arg type="s" direction="in" name="style"/>
    </method>
    <method name="ShowPositioned">
      <arg type="s" direction="in" name="label"/>
      <arg type="i" direction="in" name="volume_percent"/>
      <arg type="i" direction="in" name="max"/>
      <arg type="b" direction="in" name="muted"/>
      <arg type="s" direction="in" name="theme"/>
      <arg type="s" direction="in" name="style"/>
      <arg type="s" direction="in" name="position"/>
    </method>
  </interface>
</node>`;

const DISPLAY_MS = 1800;
const SLIDE_DISTANCE = 260;
/* Matches Mixweave's OsdPosition on the Rust/TS side. Kept as the original
   fixed placement so extensions/callers that predate the position setting
   see no change. */
const DEFAULT_POSITION = 'middle-right';
const POSITIONS = [
    'top-right', 'middle-right', 'bottom-right',
    'top-left', 'middle-left', 'bottom-left',
];

// Segments look.
const SEGMENTS = 24;
const SEGMENT_WIDTH = 6;
const SEGMENT_HEIGHT = 22;

// Vertical fader look.
const FADER_TRACK_WIDTH = 12;
const FADER_TRACK_HEIGHT = 98;

// Waves look.
const BARS = 28;
const BAR_WIDTH = 4;
const BAR_HEIGHT = 25;

// Classic (fallback) look.
const TRACK_WIDTH = 220;
const TRACK_HEIGHT = 7;

function icon_for(muted) {
    return muted ? 'audio-volume-muted-symbolic' : 'audio-volume-high-symbolic';
}

function ellipsized(label) {
    label.clutter_text.ellipsize = Pango.EllipsizeMode.END;
    return label;
}

export default class MixweaveOsdExtension extends Extension {
    enable() {
        // Deliberately verbose: `journalctl --user -b | grep sonux-osd`
        // must be able to answer "did enable() even run, and how far did
        // it get" on its own, since a caught exception here doesn't always
        // surface anywhere else obviously.
        console.log('[sonux-osd] enable() starting');
        try {
            this._widget = null;
            this._ui = null;
            this._style = null;
            this._hideId = null;
            // True only while fully on screen (past the enter animation,
            // not yet starting the exit one) - lets Show() tell "still
            // showing, just update the numbers" apart from "was hidden,
            // slide in".
            this._showing = false;

            this._dbusImpl = Gio.DBusExportedObject.wrapJSObject(DBUS_INTERFACE, this);
            this._dbusImpl.export(Gio.DBus.session, '/dev/sonux/Osd');
            console.log('[sonux-osd] D-Bus object exported at /dev/sonux/Osd');

            this._ownerId = Gio.bus_own_name(
                Gio.BusType.SESSION,
                'dev.sonux.Osd',
                Gio.BusNameOwnerFlags.NONE,
                () => console.log('[sonux-osd] bus_acquired'),
                () => console.log('[sonux-osd] name_acquired: dev.sonux.Osd is now owned'),
                () => console.error('[sonux-osd] name_lost: could not own dev.sonux.Osd (already taken?)'),
            );
            console.log(`[sonux-osd] enable() finished, bus_own_name id=${this._ownerId}`);
        } catch (error) {
            console.error(`[sonux-osd] enable() threw: ${error}\n${error.stack}`);
        }
    }

    disable() {
        console.log('[sonux-osd] disable() called');
        if (this._hideId) {
            GLib.source_remove(this._hideId);
            this._hideId = null;
        }
        if (this._dbusImpl) {
            this._dbusImpl.unexport();
            this._dbusImpl = null;
        }
        if (this._ownerId) {
            Gio.bus_unown_name(this._ownerId);
            this._ownerId = null;
        }
        this._destroyWidget();
        this._showing = false;
    }

    _destroyWidget() {
        if (this._ui) {
            try {
                this._ui.stop();
            } catch (error) {
                console.error(`[sonux-osd] stop() threw: ${error}`);
            }
        }
        if (this._widget) {
            try {
                Main.layoutManager.removeChrome(this._widget);
            } catch (_error) {
                // Not yet added as chrome: nothing to remove.
            }
            this._widget.destroy();
        }
        this._widget = null;
        this._ui = null;
        this._style = null;
    }

    // ------------------------------------------------------------------
    // Looks
    // ------------------------------------------------------------------

    _buildSegments() {
        const root = new St.BoxLayout({
            style_class: 'sonux-osd sonux-osd-segments dark',
            vertical: true,
            reactive: false,
            can_focus: false,
            opacity: 0,
        });

        const head = new St.BoxLayout({style_class: 'sonux-osd-seg-head', vertical: false});
        const icon = new St.Icon({
            style_class: 'sonux-osd-seg-icon',
            icon_name: icon_for(false),
            y_align: Clutter.ActorAlign.CENTER,
        });
        const label = ellipsized(new St.Label({
            style_class: 'sonux-osd-label',
            y_align: Clutter.ActorAlign.CENTER,
            x_expand: true,
        }));
        const value = new St.Label({
            style_class: 'sonux-osd-value sonux-osd-value-big',
            y_align: Clutter.ActorAlign.CENTER,
        });
        head.add_child(icon);
        head.add_child(label);
        head.add_child(value);
        root.add_child(head);

        const row = new St.BoxLayout({style_class: 'sonux-osd-segrow', vertical: false});
        const segments = [];
        for (let i = 0; i < SEGMENTS; i++) {
            const segment = new St.Widget({
                style_class: `sonux-osd-seg b${Math.floor((i / SEGMENTS) * 6)}`,
                width: SEGMENT_WIDTH,
                height: SEGMENT_HEIGHT,
            });
            segment.set_pivot_point(0.5, 1);
            segment.scale_y = 0.55;
            row.add_child(segment);
            segments.push(segment);
        }
        root.add_child(row);

        return {
            root,
            start() {},
            stop() {
                segments.forEach(segment => segment.remove_all_transitions());
            },
            update(text, percent, ratio, muted, theme, max) {
                label.set_text(text);
                value.set_text(`${percent}%`);
                icon.icon_name = icon_for(muted);
                root.set_style_class_name(
                    `sonux-osd sonux-osd-segments ${theme}${muted ? ' muted' : ''}`);
                const lit = Math.round(ratio * SEGMENTS);
                segments.forEach((segment, i) => {
                    const on = !muted && i < lit;
                    const hot = max > 100 && i / (SEGMENTS - 1) > 100 / max;
                    segment.set_style_class_name(
                        `sonux-osd-seg b${Math.floor((i / SEGMENTS) * 6)}${hot ? ' hot' : ''}${on ? ' on' : ''}`);
                    segment.remove_all_transitions();
                    segment.ease({
                        scale_y: on ? 1 : 0.55,
                        duration: 260,
                        delay: Math.abs(i - lit) * 8,
                        mode: Clutter.AnimationMode.EASE_OUT_BACK,
                    });
                });
            },
        };
    }

    _buildFader() {
        const root = new St.BoxLayout({
            style_class: 'sonux-osd sonux-osd-fader dark',
            vertical: true,
            reactive: false,
            can_focus: false,
            opacity: 0,
        });

        const disc = new St.BoxLayout({
            style_class: 'sonux-osd-disc',
            x_align: Clutter.ActorAlign.CENTER,
        });
        const icon = new St.Icon({
            style_class: 'sonux-osd-disc-icon',
            icon_name: icon_for(false),
            x_align: Clutter.ActorAlign.CENTER,
            y_align: Clutter.ActorAlign.CENTER,
            x_expand: true,
            y_expand: true,
        });
        disc.add_child(icon);
        root.add_child(disc);

        // The fill is always the *whole* rail in size (so no layout manager
        // has any offset to compute) and is scaled from its bottom edge:
        // the animation is a transform, not a re-layout.
        const track = new St.BoxLayout({
            style_class: 'sonux-osd-vtrack',
            x_align: Clutter.ActorAlign.CENTER,
            width: FADER_TRACK_WIDTH,
            height: FADER_TRACK_HEIGHT,
        });
        const fill = new St.Widget({
            style_class: 'sonux-osd-vfill',
            width: FADER_TRACK_WIDTH,
            height: FADER_TRACK_HEIGHT,
        });
        fill.set_pivot_point(0.5, 1);
        fill.scale_y = 0.001;
        track.add_child(fill);
        root.add_child(track);

        const value = new St.Label({
            style_class: 'sonux-osd-value sonux-osd-value-fader',
            x_align: Clutter.ActorAlign.CENTER,
        });
        const name = ellipsized(new St.Label({
            style_class: 'sonux-osd-name',
            x_align: Clutter.ActorAlign.CENTER,
        }));
        root.add_child(value);
        root.add_child(name);

        return {
            root,
            start() {},
            stop() {
                fill.remove_all_transitions();
            },
            update(text, percent, ratio, muted, theme) {
                name.set_text(text);
                value.set_text(`${percent}%`);
                icon.icon_name = icon_for(muted);
                root.set_style_class_name(
                    `sonux-osd sonux-osd-fader ${theme}${muted ? ' muted' : ''}`);
                fill.remove_all_transitions();
                fill.ease({
                    scale_y: Math.max(0.001, ratio),
                    duration: 420,
                    mode: Clutter.AnimationMode.EASE_OUT_EXPO,
                });
            },
        };
    }

    _buildWaves() {
        const root = new St.BoxLayout({
            style_class: 'sonux-osd sonux-osd-waves dark',
            vertical: false,
            reactive: false,
            can_focus: false,
            opacity: 0,
        });

        const tile = new St.BoxLayout({
            style_class: 'sonux-osd-tile',
            y_align: Clutter.ActorAlign.CENTER,
        });
        const icon = new St.Icon({
            style_class: 'sonux-osd-tile-icon',
            icon_name: icon_for(false),
            x_align: Clutter.ActorAlign.CENTER,
            y_align: Clutter.ActorAlign.CENTER,
            x_expand: true,
            y_expand: true,
        });
        tile.add_child(icon);
        root.add_child(tile);

        const middle = new St.BoxLayout({
            style_class: 'sonux-osd-mid',
            vertical: true,
            x_expand: true,
            y_align: Clutter.ActorAlign.CENTER,
        });
        const name = ellipsized(new St.Label({style_class: 'sonux-osd-label'}));
        const barRow = new St.BoxLayout({style_class: 'sonux-osd-barrow', vertical: false});
        const bars = [];
        for (let i = 0; i < BARS; i++) {
            const bar = new St.Widget({
                style_class: 'sonux-osd-bar',
                width: BAR_WIDTH,
                height: BAR_HEIGHT,
            });
            bar.set_pivot_point(0.5, 0.5);
            bar.scale_y = 0.28;
            bar._on = false;
            bar._animating = false;
            barRow.add_child(bar);
            bars.push(bar);
        }
        middle.add_child(name);
        middle.add_child(barRow);
        root.add_child(middle);

        const value = new St.Label({
            style_class: 'sonux-osd-value',
            y_align: Clutter.ActorAlign.CENTER,
        });
        root.add_child(value);

        let waving = false;
        const bounce = bar => {
            if (!waving || !bar._on || bar._animating)
                return;
            bar._animating = true;
            bar.ease({
                scale_y: 0.3 + Math.random() * 0.7,
                duration: 450 + Math.floor(Math.random() * 650),
                mode: Clutter.AnimationMode.EASE_IN_OUT_SINE,
                onComplete: () => {
                    bar._animating = false;
                    bounce(bar);
                },
            });
        };

        return {
            root,
            // The bars only bounce while the popup is on screen.
            start() {
                waving = true;
                bars.forEach(bounce);
            },
            stop() {
                waving = false;
                bars.forEach(bar => {
                    bar.remove_all_transitions();
                    bar._animating = false;
                });
            },
            update(text, percent, ratio, muted, theme) {
                name.set_text(text);
                value.set_text(`${percent}%`);
                icon.icon_name = icon_for(muted);
                root.set_style_class_name(
                    `sonux-osd sonux-osd-waves ${theme}${muted ? ' muted' : ''}`);
                const lit = Math.round(ratio * BARS);
                bars.forEach((bar, i) => {
                    const on = !muted && i < lit;
                    bar._on = on;
                    bar.set_style_class_name(`sonux-osd-bar${i < lit ? ' lit' : ''}`);
                    if (on) {
                        bounce(bar);
                    } else {
                        bar.remove_all_transitions();
                        bar._animating = false;
                        bar.ease({
                            scale_y: muted && i < lit ? 0.4 : 0.28,
                            duration: 200,
                            mode: Clutter.AnimationMode.EASE_OUT_QUAD,
                        });
                    }
                });
            },
        };
    }

    // The plain popup (icon, name, percentage, level bar) used if one of the
    // looks above fails to build.
    _buildClassic() {
        const root = new St.BoxLayout({
            style_class: 'sonux-osd dark',
            vertical: true,
            reactive: false,
            can_focus: false,
            opacity: 0,
        });

        const head = new St.BoxLayout({style_class: 'sonux-osd-head', vertical: false});
        const tile = new St.BoxLayout({
            style_class: 'sonux-osd-tile',
            x_align: Clutter.ActorAlign.START,
            y_align: Clutter.ActorAlign.CENTER,
        });
        const icon = new St.Icon({
            style_class: 'sonux-osd-icon',
            icon_name: icon_for(false),
            x_align: Clutter.ActorAlign.CENTER,
            y_align: Clutter.ActorAlign.CENTER,
            x_expand: true,
            y_expand: true,
        });
        tile.add_child(icon);
        const label = ellipsized(new St.Label({
            style_class: 'sonux-osd-label',
            y_align: Clutter.ActorAlign.CENTER,
            x_expand: true,
        }));
        const value = new St.Label({
            style_class: 'sonux-osd-value',
            y_align: Clutter.ActorAlign.CENTER,
        });
        head.add_child(tile);
        head.add_child(label);
        head.add_child(value);
        root.add_child(head);

        // See the note in _buildFader: an actor that always fills its
        // parent, changed by a transform (here clip) rather than layout.
        const track = new St.Widget({
            style_class: 'sonux-osd-track',
            width: TRACK_WIDTH,
            height: TRACK_HEIGHT,
        });
        const fill = new St.Widget({
            style_class: 'sonux-osd-fill',
            width: TRACK_WIDTH,
            height: TRACK_HEIGHT,
        });
        fill.set_clip(0, 0, 0, TRACK_HEIGHT);
        track.add_child(fill);
        root.add_child(track);

        return {
            root,
            start() {},
            stop() {},
            update(text, percent, ratio, muted, theme) {
                label.set_text(text);
                value.set_text(`${percent}%`);
                icon.icon_name = icon_for(muted);
                root.set_style_class_name(`sonux-osd ${theme}${muted ? ' muted' : ''}`);
                fill.set_clip(0, 0, Math.round(TRACK_WIDTH * ratio), TRACK_HEIGHT);
            },
        };
    }

    _ensureWidget(style) {
        if (this._widget && this._style === style)
            return;
        this._destroyWidget();

        let ui;
        let built = style;
        try {
            if (style === 'segments')
                ui = this._buildSegments();
            else if (style === 'fader')
                ui = this._buildFader();
            else
                ui = this._buildWaves();
        } catch (error) {
            console.error(`[sonux-osd] building the "${style}" look threw: ${error}\n${error.stack}`);
            ui = this._buildClassic();
            built = 'classic';
        }
        this._ui = ui;
        this._widget = ui.root;
        this._style = style;
        console.log(`[sonux-osd] look "${built}" ready`);

        Main.layoutManager.addChrome(this._widget, {
            affectsStruts: false,
            trackFullscreen: true,
        });
    }

    // `position` is one of POSITIONS ("<edge>-<side>"); anything else (an
    // older caller that never sends one) falls back to DEFAULT_POSITION,
    // the original fixed placement.
    _place(position) {
        const monitor = Main.layoutManager.primaryMonitor;
        if (!monitor) return;
        const [width, height] = this._widget.get_size();
        const [edge, side] = (POSITIONS.includes(position) ? position : DEFAULT_POSITION).split('-');
        const x = side === 'left'
            ? monitor.x + 32
            : monitor.x + monitor.width - width - 32;
        const y = edge === 'top' ? monitor.y + 32
            : edge === 'bottom' ? monitor.y + monitor.height - height - 32
            : monitor.y + Math.floor((monitor.height - height) / 2);
        this._widget.set_position(x, y);
        // Slide in from whichever screen edge the popup now sits against.
        this._slideDistance = side === 'left' ? -SLIDE_DISTANCE : SLIDE_DISTANCE;
    }

    // ------------------------------------------------------------------
    // D-Bus methods - see DBUS_INTERFACE above. `Show` is the original
    // signature and `ShowThemed` the one with the app theme; both are kept
    // for older Mixweave builds and simply use the default look.
    // ------------------------------------------------------------------

    Show(label, volumePercent, max, muted) {
        this.ShowPositioned(label, volumePercent, max, muted, 'dark', 'waves', DEFAULT_POSITION);
    }

    ShowThemed(label, volumePercent, max, muted, theme) {
        this.ShowPositioned(label, volumePercent, max, muted, theme, 'waves', DEFAULT_POSITION);
    }

    ShowStyled(label, volumePercent, max, muted, theme, style) {
        this.ShowPositioned(label, volumePercent, max, muted, theme, style, DEFAULT_POSITION);
    }

    // The only place that updates and (re)shows the popup, so every trigger
    // source (mute, volume up, volume down, any channel/bus/mic) goes
    // through one path.
    ShowPositioned(label, volumePercent, max, muted, theme, style, position) {
        try {
            const look = style === 'segments' || style === 'fader' ? style : 'waves';
            this._ensureWidget(look);

            const ratio = max > 0 ? Math.max(0, Math.min(1, volumePercent / max)) : 0;
            this._ui.update(label, volumePercent, ratio, muted, theme === 'light' ? 'light' : 'dark', max);

            if (this._hideId) {
                GLib.source_remove(this._hideId);
                this._hideId = null;
            }

            if (!this._showing) {
                this._showing = true;
                this._widget.show();
                this._place(position);
                this._widget.remove_all_transitions();
                this._widget.translation_x = this._slideDistance;
                this._widget.opacity = 0;
                this._widget.ease({
                    translation_x: 0,
                    opacity: 255,
                    duration: 260,
                    mode: Clutter.AnimationMode.EASE_OUT_EXPO,
                });
            }
            this._ui.start();
            // else: already fully on screen from an earlier trigger (e.g.
            // holding a volume key) - everything above already updated in
            // place, so just restart the auto-hide countdown below instead
            // of replaying the slide-in, which would yank it back off-screen
            // and in again on every repeat.

            this._hideId = GLib.timeout_add(GLib.PRIORITY_DEFAULT, DISPLAY_MS, () => {
                this._showing = false;
                this._ui?.stop();
                this._widget?.ease({
                    translation_x: this._slideDistance,
                    opacity: 0,
                    duration: 220,
                    mode: Clutter.AnimationMode.EASE_IN_QUAD,
                });
                this._hideId = null;
                return GLib.SOURCE_REMOVE;
            });
        } catch (error) {
            console.error(`[sonux-osd] ShowPositioned() threw: ${error}\n${error.stack}`);
            throw error;
        }
    }
}
