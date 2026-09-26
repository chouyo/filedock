import { useCallback, useEffect, useState } from 'react';

/**
 * Tracks hover state via explicit React state instead of relying on the CSS
 * `:hover` pseudo-class.
 *
 * On Windows/WebView2, clicking a button that causes the native window to
 * blur (minimize, hide-to-tray, or a native dialog stealing focus) leaves the
 * pointer resting over the button without a real `mousemove`/`mouseout`
 * event ever firing. The browser then freezes the `:hover` hit-test result,
 * so the hover highlight visually "sticks" until the window is restored and
 * the mouse is actually moved again. Listening for `window` `blur` lets us
 * proactively clear the hover state the moment focus is lost, regardless of
 * what caused it.
 */
export function useSafeHover() {
  const [isHovered, setIsHovered] = useState(false);

  useEffect(() => {
    const clearHover = () => setIsHovered(false);
    window.addEventListener('blur', clearHover);
    return () => {
      window.removeEventListener('blur', clearHover);
    };
  }, []);

  const onMouseEnter = useCallback(() => setIsHovered(true), []);
  const onMouseLeave = useCallback(() => setIsHovered(false), []);

  return {
    isHovered,
    hoverProps: { onMouseEnter, onMouseLeave },
    clearHover: onMouseLeave,
  };
}
