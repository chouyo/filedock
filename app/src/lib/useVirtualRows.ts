import { useLayoutEffect, useState } from 'react';

interface VirtualRowsOptions {
  count: number;
  rowHeight: number;
  scrollRef: React.RefObject<HTMLElement>;
  overscan?: number;
}

/**
 * Windowing for a list of fixed-height rows inside a scroll container.
 * Returns the index range to render and the space to reserve above and below
 * it. State only changes when the first visible row or the viewport height
 * changes, so scrolling within a row does not re-render.
 */
export function useVirtualRows({ count, rowHeight, scrollRef, overscan = 10 }: VirtualRowsOptions) {
  const [firstRow, setFirstRow] = useState(0);
  const [viewportHeight, setViewportHeight] = useState(0);

  useLayoutEffect(() => {
    const el = scrollRef.current;
    if (!el) return;
    const update = () => {
      setFirstRow(Math.floor(el.scrollTop / rowHeight));
      setViewportHeight(el.clientHeight);
    };
    update();
    el.addEventListener('scroll', update, { passive: true });
    const observer = new ResizeObserver(update);
    observer.observe(el);
    return () => {
      el.removeEventListener('scroll', update);
      observer.disconnect();
    };
  }, [scrollRef, rowHeight]);

  const start = Math.min(count, Math.max(0, firstRow - overscan));
  const end = Math.min(count, firstRow + Math.ceil(viewportHeight / rowHeight) + overscan);
  return {
    start,
    end,
    paddingTop: start * rowHeight,
    paddingBottom: (count - end) * rowHeight,
  };
}
