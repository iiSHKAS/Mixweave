import { useCallback, useEffect, useRef, useState } from "react";
import { createPortal } from "react-dom";
import type { AppIdentity } from "../../types";
import { AppIcon } from "../AppList/AppIcon";

export interface DragAppPayload {
  key: string;
  name: string;
  iconPath: string | null;
  streamIndexes: number[];
  identities: AppIdentity[];
  desktopId: string | null;
  /** Source channel, excluded from drop-target highlighting. */
  originChannel: string;
}

interface DragState {
  payload: DragAppPayload;
  pointerId: number;
  startX: number;
  startY: number;
  x: number;
  y: number;
  offsetX: number;
  offsetY: number;
  width: number;
  height: number;
  active: boolean;
  target: string | null;
  frame: number;
  /** Pointer position the ghost was last drawn at. */
  drawnX: number;
  drawnY: number;
}

const ACTIVATE_THRESHOLD = 6;
const reducedMotion = () => window.matchMedia("(prefers-reduced-motion: reduce)").matches;
const ghostTransform = (x: number, y: number, tilt: number) =>
  `translate3d(${x}px, ${y}px, 0) rotate(${tilt}deg) scale(1.04)`;

/**
 * Shared pointer-driven routing drag with a floating preview. MixerBoard owns
 * the instance so every channel can participate as a drop target. Movement
 * below the pickup threshold triggers onTap instead.
 */
export function useAppCardDrag(onDrop: (payload: DragAppPayload, destination: string) => void) {
  const dragRef = useRef<DragState | null>(null);
  const [ghost, setGhost] = useState<{ payload: DragAppPayload; x: number; y: number; width: number } | null>(null);
  const [dropTarget, setDropTarget] = useState<HTMLElement | null>(null);
  const tapRef = useRef<((payload: DragAppPayload) => void) | null>(null);
  // The ghost follows the pointer by writing its transform straight to the
  // element: pushing the position through React state re-rendered the whole
  // mixer board on every animation frame, which is what made drags stutter.
  const ghostElRef = useRef<HTMLDivElement | null>(null);
  const tiltRef = useRef(reducedMotion() ? 0 : 2);

  const locateWell = useCallback((x: number, y: number) => {
    const el = document.elementFromPoint(x, y);
    return el?.closest<HTMLElement>("[data-app-well]") ?? null;
  }, []);

  // Edge auto-scroll: only the channels segment scrolls horizontally in
  // this layout (Master/Mic stay pinned), so that's the element that
  // advances as the pointer nears the mixer viewport's left/right edge.
  const autoScroll = useCallback((x: number, y: number): boolean => {
    const scrollEl = document.querySelector<HTMLElement>(".mix-group-channels .group-strips");
    const viewport = document.querySelector<HTMLElement>(".mix-scroll");
    if (!scrollEl || !viewport) return false;
    const vRect = viewport.getBoundingClientRect();
    if (y < vRect.top || y > vRect.bottom) return false;
    const sRect = scrollEl.getBoundingClientRect();
    const before = scrollEl.scrollLeft;
    if (x - sRect.left < 48) scrollEl.scrollLeft -= 10;
    else if (sRect.right - x < 48) scrollEl.scrollLeft += 10;
    return scrollEl.scrollLeft !== before;
  }, []);

  const frameRef = useRef<() => void>(() => {});
  frameRef.current = () => {
    const d = dragRef.current;
    if (!d?.active) return;
    const moved = d.x !== d.drawnX || d.y !== d.drawnY;
    if (moved && ghostElRef.current) {
      d.drawnX = d.x;
      d.drawnY = d.y;
      ghostElRef.current.style.transform = ghostTransform(d.x - d.offsetX, d.y - d.offsetY, tiltRef.current);
    }
    const scrolled = autoScroll(d.x, d.y);
    // The hit test only needs repeating when the pointer or the strips moved.
    if (moved || scrolled) {
      const well = locateWell(d.x, d.y);
      // A well never lights up as its own drop target.
      const rawDestination = well?.dataset.appWell ?? null;
      const destination = rawDestination !== null && rawDestination !== d.payload.originChannel ? rawDestination : null;
      if (destination !== d.target) {
        d.target = destination;
        setDropTarget(destination !== null ? well : null);
      }
    }
    d.frame = requestAnimationFrame(() => frameRef.current());
  };

  const beginDrag = useCallback((
    event: React.PointerEvent<HTMLElement>,
    payload: DragAppPayload,
    onTap: (payload: DragAppPayload) => void,
  ) => {
    if (event.button !== 0 || dragRef.current) return;
    const rect = event.currentTarget.getBoundingClientRect();
    tapRef.current = onTap;
    dragRef.current = {
      payload,
      pointerId: event.pointerId,
      startX: event.clientX,
      startY: event.clientY,
      x: event.clientX,
      y: event.clientY,
      offsetX: event.clientX - rect.left,
      offsetY: event.clientY - rect.top,
      width: rect.width,
      height: rect.height,
      active: false,
      target: null,
      frame: 0,
      drawnX: event.clientX,
      drawnY: event.clientY,
    };
    event.currentTarget.setPointerCapture(event.pointerId);
  }, []);

  useEffect(() => {
    const move = (event: PointerEvent) => {
      const d = dragRef.current;
      if (!d || d.pointerId !== event.pointerId) return;
      d.x = event.clientX;
      d.y = event.clientY;
      if (!d.active && Math.hypot(event.clientX - d.startX, event.clientY - d.startY) > ACTIVATE_THRESHOLD) {
        d.active = true;
        tiltRef.current = reducedMotion() ? 0 : 2;
        document.body.classList.add("dragging-app");
        setGhost({ payload: d.payload, x: d.x - d.offsetX, y: d.y - d.offsetY, width: d.width });
        d.frame = requestAnimationFrame(() => frameRef.current());
      }
    };
    const finish = (event: PointerEvent) => {
      const d = dragRef.current;
      if (!d || d.pointerId !== event.pointerId) return;
      cancelAnimationFrame(d.frame);
      dragRef.current = null;
      document.body.classList.remove("dragging-app");
      setGhost(null);
      setDropTarget(null);
      if (d.active) {
        if (d.target !== null) onDrop(d.payload, d.target);
      } else {
        tapRef.current?.(d.payload);
      }
    };
    const cancel = (event: PointerEvent) => {
      const d = dragRef.current;
      if (!d || d.pointerId !== event.pointerId) return;
      cancelAnimationFrame(d.frame);
      dragRef.current = null;
      document.body.classList.remove("dragging-app");
      setGhost(null);
      setDropTarget(null);
    };
    // Blur has no pointerId, so cancel any active drag.
    const cancelOnBlur = () => {
      const d = dragRef.current;
      if (!d) return;
      cancelAnimationFrame(d.frame);
      dragRef.current = null;
      document.body.classList.remove("dragging-app");
      setGhost(null);
      setDropTarget(null);
    };
    window.addEventListener("pointermove", move);
    window.addEventListener("pointerup", finish);
    window.addEventListener("pointercancel", cancel);
    window.addEventListener("blur", cancelOnBlur);
    return () => {
      window.removeEventListener("pointermove", move);
      window.removeEventListener("pointerup", finish);
      window.removeEventListener("pointercancel", cancel);
      window.removeEventListener("blur", cancelOnBlur);
    };
  }, [onDrop]);

  const ghostNode = ghost && createPortal(
    <div
      ref={ghostElRef}
      className="strip-app-chip drag-ghost"
      style={{
        width: ghost.width,
        transform: ghostTransform(ghost.x, ghost.y, tiltRef.current),
      }}
      aria-hidden="true"
    >
      <span className="strip-app-icon"><AppIcon iconPath={ghost.payload.iconPath} /></span>
      <span className="strip-app-name">{ghost.payload.name}</span>
    </div>,
    document.body,
  );

  return { beginDrag, ghostNode, dropTarget, draggingPayload: ghost?.payload ?? null };
}
