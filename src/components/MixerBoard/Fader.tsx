import { useCallback, useEffect, useRef, useState } from "react";
import { handleSliderKey } from "../../lib/sliderKeyboard";

interface FaderProps {
  value: number;
  max: number;
  ariaLabel: string;
  onChange: (value: number) => void;
  /** Value restored on double-click (defaults to 100, the conventional unity level). */
  defaultValue?: number;
}

// 21 evenly spaced ticks; every 5th (0, 5, 10, 15, 20) is a major tick,
// lining up with the five scale labels below.
const TICKS = Array.from({ length: 21 }, (_, i) => i);
/** Scale labels sit on the five major ticks, so they follow the slider's range. */
const SCALE_FRACTIONS = [1, 0.75, 0.5, 0.25, 0];

/** Percent per wheel "notch" - matches the step used for the keyboard
 * shortcuts' volume up/down actions elsewhere, so scrolling and hotkeys
 * feel the same. */
const WHEEL_STEP = 5;
/** Typical deltaY a physical mouse wheel reports per notch. A trackpad's
 * continuous stream of much smaller deltas accumulates toward this instead
 * of applying a full step per event, which would otherwise fly by. */
const WHEEL_UNIT = 100;

/** Vertical channel fader: recessed rail, tick scale, and a draggable handle. */
export function Fader({ value, max, ariaLabel, onChange, defaultValue = 100 }: Readonly<FaderProps>) {
  const workRef = useRef<HTMLDivElement>(null);
  const interactRef = useRef<HTMLDivElement>(null);
  const [dragging, setDragging] = useState(false);
  const wheelAccumulator = useRef(0);
  // Read fresh inside the native listener below without re-attaching it
  // (and losing wheelAccumulator's mid-gesture progress) on every value
  // change - it fires on essentially every wheel tick while scrolling.
  const latest = useRef({ value, max, onChange });
  latest.current = { value, max, onChange };

  const setFromEvent = useCallback(
    (clientY: number) => {
      const el = workRef.current;
      if (!el) return;
      const r = el.getBoundingClientRect();
      const pct = Math.max(0, Math.min(1, 1 - (clientY - r.top) / r.height));
      onChange(Math.round(pct * max));
    },
    [onChange, max],
  );

  useEffect(() => {
    const el = interactRef.current;
    if (!el) return;
    // A plain onWheel prop is registered passive (React's default for
    // wheel/touch listeners), so preventDefault there is silently ignored -
    // needed here since a fader can sit inside the channels group's own
    // horizontally-scrolling strip, which would otherwise also shift
    // underneath a scroll meant only to change this fader's value.
    const handleWheel = (event: WheelEvent) => {
      event.preventDefault();
      event.stopPropagation();
      wheelAccumulator.current += event.deltaY;
      const notches = Math.trunc(wheelAccumulator.current / WHEEL_UNIT);
      if (notches === 0) return;
      wheelAccumulator.current -= notches * WHEEL_UNIT;
      const { value, max, onChange } = latest.current;
      // Natural scroll-up-for-louder, matching physical mixer wheels.
      onChange(Math.max(0, Math.min(max, value - notches * WHEEL_STEP)));
    };
    // Belt and suspenders alongside preventDefault above: WebKitGTK (the
    // real app's actual webview - Chromium here in dev/tests behaves
    // differently) still let the channels group's own horizontal scroll
    // ride along with a wheel gesture meant only for this fader, despite
    // the event being prevented. Locking the group's own overflow while
    // hovering any of its faders sidesteps that engine difference entirely
    // rather than depending on preventDefault reaching it. A plain-object
    // ancestor lookup (not a prop from a parent) because most Faders (Master,
    // mic, custom mixes) aren't inside a scrolling group at all - closest()
    // simply finds nothing for those, a harmless no-op.
    const scrollArea = el.closest<HTMLElement>(".group-strips");
    const lock = () => scrollArea?.classList.add("wheel-lock");
    const unlock = () => scrollArea?.classList.remove("wheel-lock");
    el.addEventListener("wheel", handleWheel, { passive: false });
    el.addEventListener("mouseenter", lock);
    el.addEventListener("mouseleave", unlock);
    return () => {
      el.removeEventListener("wheel", handleWheel);
      el.removeEventListener("mouseenter", lock);
      el.removeEventListener("mouseleave", unlock);
      unlock(); // in case this unmounts while still hovered
    };
  }, []);

  const pct = (Math.max(0, Math.min(max, value)) / max) * 100;

  return (
    <div className="fader-stage">
      <div
        ref={interactRef}
        className={"fader-interact" + (dragging ? " dragging" : "")}
        role="slider"
        tabIndex={0}
        aria-label={ariaLabel}
        aria-orientation="vertical"
        aria-valuemin={0}
        aria-valuemax={max}
        aria-valuenow={value}
        aria-valuetext={`${value}%`}
        onKeyDown={(event) => handleSliderKey(event, {
          min: 0, max, step: 1, value, onChange,
        })}
        onDoubleClick={() => onChange(Math.max(0, Math.min(max, defaultValue)))}
        onPointerDown={(event) => {
          // Pointer capture, not a manual dragging flag + window listeners:
          // once captured, every later move/up for this pointer is routed
          // here directly by the browser even if the cursor briefly leaves
          // the track during a fast drag. The flag+window-listener version
          // could lose that hand-off mid-drag, which read as the value
          // snapping back to an older position before catching up.
          event.currentTarget.setPointerCapture(event.pointerId);
          setDragging(true);
          setFromEvent(event.clientY);
        }}
        onPointerMove={(event) => {
          if (event.buttons === 1) setFromEvent(event.clientY);
        }}
        onPointerUp={() => setDragging(false)}
        onPointerCancel={() => setDragging(false)}
      >
        <div className="fader-work" ref={workRef}>
          <div className="fader-ticks" aria-hidden="true">
            {TICKS.map((i) => (
              <span key={i} className={"fader-tick" + (i % 5 === 0 ? " major" : "")} />
            ))}
          </div>
          <div className="fader-rail-col">
            <div className="fader-rail">
              <div className="fader-fill" style={{ height: pct + "%" }} />
            </div>
            <div className="fader-handle" style={{ bottom: pct + "%" }}>
              <div className="fader-handle-groove" />
            </div>
          </div>
          <div className="fader-scale" aria-hidden="true">
            {SCALE_FRACTIONS.map((fraction) => (
              <span key={fraction}>{Math.round(max * fraction)}</span>
            ))}
          </div>
        </div>
      </div>
    </div>
  );
}
