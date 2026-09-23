import { useLayoutEffect, useRef, useState } from 'react';
import type { ReactNode } from 'react';

interface TooltipProps {
  content: string;
  children: ReactNode;
  placement?: 'top' | 'bottom';
  className?: string;
  delay?: number;
}

export function Tooltip({
  content,
  children,
  placement = 'bottom',
  className,
  delay = 300,
}: TooltipProps) {
  const [show, setShow] = useState(false);
  const [pos, setPos] = useState<{ left: number; top: number } | null>(null);
  const triggerRef = useRef<HTMLSpanElement>(null);
  const tipRef = useRef<HTMLSpanElement>(null);
  const timer = useRef<number | undefined>(undefined);

  const enter = () => {
    timer.current = window.setTimeout(() => setShow(true), delay);
  };
  const leave = () => {
    if (timer.current) clearTimeout(timer.current);
    setShow(false);
    setPos(null);
  };

  useLayoutEffect(() => {
    if (!show) return;
    const trigger = triggerRef.current;
    const tip = tipRef.current;
    if (!trigger || !tip) return;

    const r = trigger.getBoundingClientRect();
    const tw = tip.offsetWidth;
    const th = tip.offsetHeight;

    let left = r.left + r.width / 2 - tw / 2;
    left = Math.max(4, Math.min(left, window.innerWidth - tw - 4));

    let top = placement === 'top' ? r.top - th - 6 : r.bottom + 6;
    if (top < 4) top = r.bottom + 6;
    if (top + th > window.innerHeight - 4) top = r.top - th - 6;
    top = Math.max(4, top);

    setPos({ left, top });
  }, [show, placement, content]);

  return (
    <span
      ref={triggerRef}
      className={`relative inline-flex ${className ?? ''}`}
      onMouseEnter={enter}
      onMouseLeave={leave}
    >
      {children}
      {show && (
        <span
          ref={tipRef}
          style={{
            position: 'fixed',
            left: pos?.left ?? -9999,
            top: pos?.top ?? -9999,
            opacity: pos ? 1 : 0,
          }}
          className="px-2 py-1 rounded-md text-xs whitespace-nowrap bg-surface-tooltip text-ink-tooltip shadow-lg z-[9999] pointer-events-none"
        >
          {content}
        </span>
      )}
    </span>
  );
}