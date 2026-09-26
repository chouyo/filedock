// Global keyboard shortcut whitelist.
//
// WebView2 / WKWebView ship with browser shortcuts (reload, print, find, view
// source, history navigation, ...) that make no sense in a desktop app. The
// release build disables most of them natively (see src-tauri/src/webview.rs);
// this module is the cross-platform backstop: every "shortcut-like" keystroke
// that is not listed below gets preventDefault() in the capture phase.
//
// It only blocks the browser default. It never stops propagation, so feature
// handlers (Esc to close modals, Ctrl+F to focus search, ...) keep working and
// still have to be implemented where they belong.
//
// To enable a new shortcut, add an entry to WHITELIST.

type Scope = 'global' | 'editable';

interface Shortcut {
  /** KeyboardEvent.key, compared case-insensitively. */
  key: string;
  /** Ctrl on Windows/Linux, Cmd on macOS. */
  mod?: boolean;
  shift?: boolean;
  alt?: boolean;
  /** 'editable' = only while focus is in a text input. Default 'global'. */
  scope?: Scope;
  /** Only allowed in `npm run tauri dev`. */
  devOnly?: boolean;
}

const EDIT_NAV_KEYS = ['ArrowLeft', 'ArrowRight', 'ArrowUp', 'ArrowDown', 'Home', 'End'];

const WHITELIST: Shortcut[] = [
  // App shortcuts
  { key: 'f', mod: true }, // focus file search (App.tsx)
  { key: 'Escape' }, // close modal / menu

  // Development
  { key: 'F5', devOnly: true }, // reload
  { key: 'F12', devOnly: true }, // devtools (App.tsx)

  // Text editing inside inputs
  ...['a', 'c', 'v', 'x', 'z', 'y', 'Insert'].map(
    (key): Shortcut => ({ key, mod: true, scope: 'editable' }),
  ),
  { key: 'z', mod: true, shift: true, scope: 'editable' }, // redo
  { key: 'Backspace', mod: true, scope: 'editable' }, // delete word
  { key: 'Delete', mod: true, scope: 'editable' },
  ...EDIT_NAV_KEYS.flatMap((key): Shortcut[] => [
    { key, mod: true, scope: 'editable' }, // move by word / to start-end
    { key, mod: true, shift: true, scope: 'editable' }, // select by word
  ]),
];

const isMac = navigator.platform.toUpperCase().includes('MAC');

const NON_TEXT_INPUT_TYPES = new Set([
  'button', 'checkbox', 'color', 'file', 'image', 'radio', 'range', 'reset', 'submit',
]);

function isEditable(target: EventTarget | null): boolean {
  if (!(target instanceof HTMLElement)) return false;
  if (target.isContentEditable || target instanceof HTMLTextAreaElement) return true;
  return target instanceof HTMLInputElement && !NON_TEXT_INPUT_TYPES.has(target.type);
}

/**
 * Plain typing, Enter, Tab, arrows, Backspace, etc. (optionally with Shift)
 * are never treated as shortcuts. Anything with Ctrl/Cmd/Alt, a function key
 * or a browser/media key is, and must be whitelisted.
 */
function isShortcutLike(e: KeyboardEvent): boolean {
  if (e.ctrlKey || e.metaKey || e.altKey) return true;
  if (/^F\d{1,2}$/.test(e.key)) return true;
  return e.key.startsWith('Browser');
}

function matches(s: Shortcut, e: KeyboardEvent, editable: boolean): boolean {
  if (s.devOnly && !import.meta.env.DEV) return false;
  if (s.scope === 'editable' && !editable) return false;
  const mod = isMac ? e.metaKey : e.ctrlKey;
  const stray = isMac ? e.ctrlKey : e.metaKey;
  return (
    e.key.toLowerCase() === s.key.toLowerCase() &&
    mod === !!s.mod &&
    !stray &&
    e.shiftKey === !!s.shift &&
    e.altKey === !!s.alt
  );
}

function onKeyDown(e: KeyboardEvent) {
  // IME composition, bare modifier presses and AltGr characters (Ctrl+Alt on
  // many European layouts) are text input, not shortcuts.
  if (e.isComposing || e.key === 'Process') return;
  if (['Control', 'Shift', 'Alt', 'Meta', 'AltGraph'].includes(e.key)) return;
  if (e.getModifierState('AltGraph')) return;

  if (!isShortcutLike(e)) return;
  const editable = isEditable(e.target);
  if (WHITELIST.some((s) => matches(s, e, editable))) return;
  e.preventDefault();
}

// Mouse back/forward side buttons trigger history navigation.
function onMouseNav(e: MouseEvent) {
  if (e.button === 3 || e.button === 4) e.preventDefault();
}

export function installShortcutGuard() {
  window.addEventListener('keydown', onKeyDown, true);
  window.addEventListener('mousedown', onMouseNav, true);
  window.addEventListener('mouseup', onMouseNav, true);
  window.addEventListener('auxclick', onMouseNav, true);
}
