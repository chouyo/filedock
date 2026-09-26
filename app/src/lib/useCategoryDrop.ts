import { useEffect, useRef, useState } from 'react';
import type { RefObject } from 'react';
import { invoke } from '@tauri-apps/api/core';
import { getCurrentWebview } from '@tauri-apps/api/webview';
import { summarizeDrop } from './dropDraft';
import type { DropSummary } from './dropDraft';
import { endInternalDrag, isInternalDragActive } from './internalDrag';
import type { DropMode, PathInfo } from '../types';

/** `dir` is the single zone shown when only directories are dragged. */
export type DropZone = 'dir' | DropMode;

export interface CategoryDropState {
  /** An external drag carrying usable paths is over the window. */
  dragging: boolean;
  summary: DropSummary | null;
  /** Pointer is over the drop container. */
  over: boolean;
  /** Zone under the pointer; null when outside the container. */
  zone: DropZone | null;
}

// wry reports drag positions in physical pixels on Windows (WebView2) but in
// logical points on macOS/Linux, despite the PhysicalPosition type.
const DRAG_POSITION_IS_PHYSICAL = navigator.userAgent.includes('Windows');

const IDLE: CategoryDropState = { dragging: false, summary: null, over: false, zone: null };

/** Zones the overlay renders for a summary, top to bottom. */
export function zonesFor(summary: DropSummary): DropZone[] {
  if (summary.files.length === 0) return ['dir'];
  if (summary.extensions.length === 0) return ['file'];
  return ['file', 'type'];
}

interface Options {
  /** Element that accepts drops; zones inside it carry `data-drop-zone`. */
  containerRef: RefObject<HTMLElement>;
  /** Ignore drags entirely, e.g. while a modal is open. */
  disabled: boolean;
  onDrop: (infos: PathInfo[], mode: DropMode) => void;
  /** Dropped on the container, but none of the paths exist. */
  onUnrecognized: () => void;
}

function hitTest(
  container: HTMLElement,
  summary: DropSummary,
  x: number,
  y: number,
): { over: boolean; zone: DropZone | null } {
  const rect = container.getBoundingClientRect();
  const over = x >= rect.left && x <= rect.right && y >= rect.top && y <= rect.bottom;
  if (!over) return { over, zone: null };

  const zones = zonesFor(summary);
  const els = Array.from(container.querySelectorAll<HTMLElement>('[data-drop-zone]'));
  if (els.length === 0) {
    // Overlay not rendered yet: assume zones split the container evenly.
    const index = Math.min(zones.length - 1, Math.floor(((y - rect.top) / rect.height) * zones.length));
    return { over, zone: zones[index] };
  }
  // Pick the zone whose vertical span is closest, so gaps between zones and
  // the footnote area still resolve to a zone.
  let best: HTMLElement | null = null;
  let bestDist = Infinity;
  for (const el of els) {
    const r = el.getBoundingClientRect();
    const dist = y < r.top ? r.top - y : y > r.bottom ? y - r.bottom : 0;
    if (dist < bestDist) {
      bestDist = dist;
      best = el;
    }
  }
  return { over, zone: (best?.dataset.dropZone as DropZone | undefined) ?? null };
}

/**
 * Listens for OS file drags over the window and resolves which sidebar drop
 * zone the pointer is on. Drags started from FileDock's own file table are
 * ignored.
 */
export function useCategoryDrop({
  containerRef,
  disabled,
  onDrop,
  onUnrecognized,
}: Options): CategoryDropState {
  const [state, setState] = useState<CategoryDropState>(IDLE);

  // Latest values for the long-lived listener.
  const optsRef = useRef({ disabled, onDrop, onUnrecognized });
  optsRef.current = { disabled, onDrop, onUnrecognized };

  useEffect(() => {
    // Each drag gets a session; results of stale async lookups are dropped.
    let session = 0;
    let ignored = false;
    let infosPromise: Promise<PathInfo[]> | null = null;
    let summary: DropSummary | null = null;
    let lastPoint: { x: number; y: number } | null = null;

    const reset = () => {
      session++;
      ignored = false;
      infosPromise = null;
      summary = null;
      lastPoint = null;
      setState(IDLE);
    };

    const update = () => {
      const container = containerRef.current;
      if (!summary || !container || !lastPoint) return;
      const { over, zone } = hitTest(container, summary, lastPoint.x, lastPoint.y);
      setState((prev) =>
        prev.dragging && prev.over === over && prev.zone === zone && prev.summary === summary
          ? prev
          : { dragging: true, summary, over, zone },
      );
    };

    const toCss = (p: { x: number; y: number }) => {
      const scale = DRAG_POSITION_IS_PHYSICAL ? window.devicePixelRatio : 1;
      return { x: p.x / scale, y: p.y / scale };
    };

    const unlisten = getCurrentWebview().onDragDropEvent(async (event) => {
      const payload = event.payload;
      switch (payload.type) {
        case 'enter': {
          reset();
          if (optsRef.current.disabled || isInternalDragActive()) {
            ignored = true;
            return;
          }
          const current = session;
          lastPoint = toCss(payload.position);
          infosPromise = invoke<PathInfo[]>('inspect_paths', { paths: payload.paths });
          let infos: PathInfo[];
          try {
            infos = await infosPromise;
          } catch {
            infos = [];
          }
          if (current !== session) return;
          const s = summarizeDrop(infos);
          if (s.dirs.length === 0 && s.files.length === 0) return;
          summary = s;
          update();
          return;
        }
        case 'over': {
          if (ignored) return;
          lastPoint = toCss(payload.position);
          update();
          return;
        }
        case 'drop': {
          if (ignored) {
            endInternalDrag();
            reset();
            return;
          }
          const pending = infosPromise;
          const point = toCss(payload.position);
          const container = containerRef.current;
          // Resolve the zone before reset() unmounts the overlay, so the hit
          // test uses the zones the user actually saw.
          const seenZone =
            summary && container ? hitTest(container, summary, point.x, point.y).zone : null;
          reset();
          if (!pending || !container || optsRef.current.disabled) return;
          let infos: PathInfo[];
          try {
            infos = await pending;
          } catch {
            infos = [];
          }
          const s = summarizeDrop(infos);
          if (s.dirs.length === 0 && s.files.length === 0) {
            const rect = container.getBoundingClientRect();
            const inside =
              point.x >= rect.left && point.x <= rect.right &&
              point.y >= rect.top && point.y <= rect.bottom;
            if (inside) optsRef.current.onUnrecognized();
            return;
          }
          const zone = seenZone ?? hitTest(container, s, point.x, point.y).zone;
          if (!zone) return;
          optsRef.current.onDrop(infos, zone === 'type' ? 'type' : 'file');
          return;
        }
        case 'leave': {
          if (ignored) endInternalDrag();
          reset();
          return;
        }
      }
    });

    return () => {
      session++;
      unlisten.then((fn) => fn());
    };
  }, [containerRef]);

  // A modal opening mid-drag hides the overlay; the drop is ignored too.
  return disabled ? IDLE : state;
}
