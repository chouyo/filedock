// Tracks drags started from FileDock's own file table (via tauri-plugin-drag)
// so the window's drag-drop listener can ignore them: dropping our own rows
// onto the sidebar must not create a category.

let active = false;

export function beginInternalDrag() {
  active = true;
}

export function endInternalDrag() {
  active = false;
}

export function isInternalDragActive(): boolean {
  return active;
}
