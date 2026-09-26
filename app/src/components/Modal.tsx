import { useEffect, useId, useRef } from 'react';
import type { ReactNode } from 'react';
import { useI18n } from '../i18n/useI18n';

interface ModalProps {
  open: boolean;
  onClose: () => void;
  title: string;
  children: ReactNode;
  width?: string;
  footer?: ReactNode;
}

const FOCUSABLE =
  'a[href], button:not([disabled]), input:not([disabled]), select:not([disabled]), textarea:not([disabled]), [tabindex]:not([tabindex="-1"])';

// Open modals, innermost last. Only the top one reacts to Tab and Escape, so a
// confirm dialog opened over another dialog does not close both.
const modalStack: string[] = [];

function focusableIn(root: HTMLElement): HTMLElement[] {
  return Array.from(root.querySelectorAll<HTMLElement>(FOCUSABLE)).filter(
    (el) => el.tabIndex >= 0 && el.getClientRects().length > 0,
  );
}

/**
 * Keyboard behavior:
 * - on open, focuses the first `[data-autofocus]` element, else the first
 *   focusable element in the body, else the close button;
 * - Tab / Shift+Tab cycle inside the dialog;
 * - Escape closes it, unless an inner widget already handled the key
 *   (called preventDefault);
 * - on close, focus returns to the element that had it before opening.
 */
export function Modal({ open, onClose, title, children, width = '480px', footer }: ModalProps) {
  const { t } = useI18n();
  const id = useId();
  const titleId = `${id}-title`;
  const dialogRef = useRef<HTMLDivElement>(null);
  const bodyRef = useRef<HTMLDivElement>(null);
  const closeRef = useRef<HTMLButtonElement>(null);
  const onCloseRef = useRef(onClose);
  onCloseRef.current = onClose;

  useEffect(() => {
    if (!open) return;
    const opener = document.activeElement as HTMLElement | null;
    modalStack.push(id);

    const dialog = dialogRef.current;
    if (dialog && !dialog.contains(document.activeElement)) {
      const target =
        dialog.querySelector<HTMLElement>('[data-autofocus]') ??
        (bodyRef.current && focusableIn(bodyRef.current)[0]) ??
        closeRef.current;
      target?.focus();
    }

    const onKey = (e: KeyboardEvent) => {
      if (modalStack[modalStack.length - 1] !== id || !dialog) return;
      if (e.key === 'Escape') {
        if (!e.defaultPrevented) onCloseRef.current();
        return;
      }
      if (e.key !== 'Tab') return;
      const items = focusableIn(dialog);
      if (items.length === 0) {
        e.preventDefault();
        return;
      }
      const first = items[0];
      const last = items[items.length - 1];
      const active = document.activeElement as HTMLElement | null;
      const inside = !!active && dialog.contains(active);
      if (e.shiftKey && (!inside || active === first)) {
        e.preventDefault();
        last.focus();
      } else if (!e.shiftKey && (!inside || active === last)) {
        e.preventDefault();
        first.focus();
      }
    };
    window.addEventListener('keydown', onKey);

    return () => {
      window.removeEventListener('keydown', onKey);
      const i = modalStack.lastIndexOf(id);
      if (i !== -1) modalStack.splice(i, 1);
      if (opener?.isConnected) opener.focus();
    };
  }, [open, id]);

  if (!open) return null;

  return (
    <div
      className="fixed inset-0 z-50 flex items-center justify-center bg-black/30 backdrop-blur-sm animate-fade-in"
      onClick={onClose}
    >
      <div
        ref={dialogRef}
        role="dialog"
        aria-modal="true"
        aria-labelledby={titleId}
        className="bg-surface-elevated border border-divider rounded-xl shadow-2xl flex flex-col max-h-[85vh] animate-slide-up"
        style={{ width }}
        onClick={(e) => e.stopPropagation()}
      >
        <div className="flex items-center justify-between px-5 py-3.5 border-b border-divider shrink-0">
          <h2 id={titleId} className="text-sm font-semibold text-ink">{title}</h2>
          <button
            ref={closeRef}
            onClick={onClose}
            aria-label={t('common.close')}
            className="p-1 rounded hover:bg-hover-bg text-ink-secondary hover:text-ink transition text-base leading-none"
          >
            ✕
          </button>
        </div>
        <div ref={bodyRef} className="overflow-y-auto flex-1 px-5 py-4">{children}</div>
        {footer && (
          <div className="flex items-center justify-end gap-2 px-5 py-3 border-t border-divider shrink-0">
            {footer}
          </div>
        )}
      </div>
    </div>
  );
}
