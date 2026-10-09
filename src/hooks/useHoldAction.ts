import { useRef, type PointerEvent } from "react";

const HOLD_MS = 500;

/** Tap vs. press-and-hold on one button: a tap fires `onTap` (via the normal
 *  click, so keyboard activation still works); holding for `HOLD_MS` fires
 *  `onHold` instead and swallows the click that follows the release. */
export function useHoldAction(onTap: () => void, onHold: () => void) {
  const timer = useRef<ReturnType<typeof setTimeout> | null>(null);
  const held = useRef(false);

  const cancel = () => {
    if (timer.current) clearTimeout(timer.current);
    timer.current = null;
  };

  return {
    onPointerDown: (e: PointerEvent) => {
      if (e.button !== 0) return;
      held.current = false;
      cancel();
      timer.current = setTimeout(() => {
        timer.current = null;
        held.current = true;
        onHold();
      }, HOLD_MS);
    },
    onPointerUp: cancel,
    onPointerLeave: cancel,
    onPointerCancel: cancel,
    onClick: () => {
      if (held.current) {
        held.current = false;
        return;
      }
      onTap();
    },
  };
}
