import { useEffect, useRef, useState } from "react";
import { createPortal } from "react-dom";
import { listen } from "@tauri-apps/api/event";
import { useShortcutOsd, type ShortcutOsdPayload } from "../store/shortcutOsd";
import { syncTheme } from "../store/theme";
import { readOsdStyle, type OsdStyle } from "../store/osdStyle";
import { readOsdPosition, type OsdPosition } from "../store/osdPosition";
import { Ms } from "./Icons";

const DISPLAY_MS = 1800;
const EXIT_MS = 220;

const SEGMENTS = 24;
const BARS = 28;
/** Fixed per-bar timing so the waves look the same on every showing. */
const BAR_TIMING = Array.from({ length: BARS }, (_, i) => ({
  duration: 520 + ((i * 137) % 700),
  delay: -((i * 211) % 900),
}));

/** Warm ramp across the meter; red past 100 % when the control can go above it. */
function segmentColor(index: number, max: number): string {
  const t = index / (SEGMENTS - 1);
  const over = max > 100 ? 100 / max : 1;
  if (t > over) return "#EF4444";
  const k = t / over;
  return `hsl(${24 - k * 12} 100% ${58 + (1 - k) * 8}%)`;
}

/** Sonar-style on-screen popup for keyboard-shortcut-driven mute/volume
 * changes: slides in from the right edge, centered vertically, then fades
 * back out on its own. Purely a notification - it has no controls.
 *
 * Only ever mounted in the dedicated, always-on-top "osd" window overlay.rs
 * opens on X11 (see main.tsx's `?osd=1` branch) - GNOME/Wayland gets the
 * same popup from the Shell extension in gnome_osd_extension.rs instead,
 * and there is deliberately no in-window fallback for anything else.
 * useGlobalShortcuts emits a "shortcut-osd" event on every mute/volume
 * change, and this is the only place that turns that into the popup. */
export function ShortcutOsd() {
  const { token, label, volumePercent, max, muted } = useShortcutOsd();
  const [style, setStyle] = useState<OsdStyle>(readOsdStyle);
  const [position, setPosition] = useState<OsdPosition>(readOsdPosition);
  const [mounted, setMounted] = useState(false);
  const [visible, setVisible] = useState(false);
  const hideTimer = useRef<number>();
  const unmountTimer = useRef<number>();
  // Tracks "fully on screen" outside React state so the effect below can
  // read it synchronously the instant a new trigger comes in, rather than
  // racing a state update that hasn't committed yet.
  const showing = useRef(false);

  useEffect(() => {
    const unlisten = listen<ShortcutOsdPayload>("shortcut-osd", (event) => {
      useShortcutOsd.getState().show(event.payload);
    });
    return () => {
      void unlisten.then((fn) => fn());
    };
  }, []);

  useEffect(() => {
    if (token === 0) return;
    // The main window may have changed theme or popup style since this window was opened.
    syncTheme();
    setStyle(readOsdStyle());
    setPosition(readOsdPosition());
    window.clearTimeout(hideTimer.current);
    window.clearTimeout(unmountTimer.current);

    let raf1 = 0;
    let raf2 = 0;
    if (!showing.current) {
      // Mount off-screen first, then flip to the shown position on a later
      // frame. A single requestAnimationFrame callback can still land
      // before the browser has actually painted the off-screen state, so
      // the transition has nothing to animate from and the popup just
      // appears; nesting two rAFs guarantees a full paint happens first.
      setMounted(true);
      setVisible(false);
      raf1 = requestAnimationFrame(() => {
        raf2 = requestAnimationFrame(() => {
          showing.current = true;
          setVisible(true);
        });
      });
    }
    // else: already fully on screen from an earlier trigger (e.g. holding
    // a volume key) - the label/bar below already re-render from the
    // store's fresh values on their own, so just restart the auto-hide
    // countdown below instead of replaying the slide-in, which would yank
    // it back off-screen and in again on every repeat.

    hideTimer.current = window.setTimeout(() => {
      showing.current = false;
      setVisible(false);
      unmountTimer.current = window.setTimeout(() => setMounted(false), EXIT_MS);
    }, DISPLAY_MS);
    return () => {
      cancelAnimationFrame(raf1);
      cancelAnimationFrame(raf2);
      window.clearTimeout(hideTimer.current);
      window.clearTimeout(unmountTimer.current);
    };
  }, [token]);

  if (!mounted) return null;

  const ratio = Math.max(0, Math.min(1, volumePercent / max));
  const icon = <Ms name={muted ? "volume_off" : "volume_up"} />;
  const lit = Math.round(ratio * SEGMENTS);

  let body: JSX.Element;
  if (style === "segments") {
    body = (
      <>
        <div className="osd-head">
          <span className="osd-lab">{icon}{label}</span>
          <span className="osd-num">{volumePercent}%</span>
        </div>
        <div className="osd-segs">
          {Array.from({ length: SEGMENTS }, (_, i) => (
            <i
              key={i}
              className={i < lit && !muted ? "on" : undefined}
              style={{ "--seg": segmentColor(i, max), "--d": `${Math.abs(i - lit) * 8}ms` } as React.CSSProperties}
            />
          ))}
        </div>
      </>
    );
  } else if (style === "fader") {
    body = (
      <>
        <span className="osd-disc">{icon}</span>
        <div className="osd-rail">
          <div className="osd-rail-fill" style={{ height: `${ratio * 100}%` }} />
          <div className="osd-knob" style={{ bottom: `${ratio * 100}%` }} />
        </div>
        <span className="osd-num">{volumePercent}%</span>
        <span className="osd-name">{label}</span>
      </>
    );
  } else {
    const litBars = Math.round(ratio * BARS);
    body = (
      <>
        <span className="osd-tile">{icon}</span>
        <div className="osd-mid">
          <div className="osd-name">{label}</div>
          <div className="osd-bars">
            {BAR_TIMING.map((timing, i) => (
              <i
                key={i}
                className={i < litBars ? "on" : undefined}
                style={{ "--dur": `${timing.duration}ms`, "--del": `${timing.delay}ms` } as React.CSSProperties}
              />
            ))}
          </div>
        </div>
        <span className="osd-num">{volumePercent}%</span>
      </>
    );
  }

  return createPortal(
    <div
      className={`shortcut-osd style-${style} pos-${position}` + (visible ? " visible" : "") + (muted ? " muted" : "")}
      role="status"
      aria-live="polite"
    >
      {body}
    </div>,
    document.body,
  );
}
