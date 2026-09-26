import { useCallback, useEffect, useRef, useState } from 'react';
import { invoke } from '@tauri-apps/api/core';
import { Minus, Square, X, Copy, Settings } from 'lucide-react';
import { useI18n } from '../i18n/useI18n';
import { cn } from '../lib/utils';
import { useSafeHover } from '../lib/useSafeHover';
import titlebarIcon from '../../src-tauri/icons/32x32.png';

interface TitleBarProps {
  onOpenSettings: () => void;
}

export function TitleBar({ onOpenSettings }: TitleBarProps) {
  const { t } = useI18n();
  const [isMaximized, setIsMaximized] = useState(false);
  const winRef = useRef<import('@tauri-apps/api/window').Window | null>(null);

  useEffect(() => {
    let unlistenFn: (() => void) | undefined;
    const setup = async () => {
      const { getCurrentWindow } = await import('@tauri-apps/api/window');
      const win = getCurrentWindow();
      winRef.current = win;
      const unlisten = await win.onResized(async () => {
        setIsMaximized(await win.isMaximized());
      });
      unlistenFn = unlisten;
      setIsMaximized(await win.isMaximized());
    };
    setup();
    return () => {
      unlistenFn?.();
    };
  }, []);

  const handleDragStart = useCallback(async (e: React.MouseEvent) => {
    const target = e.target as HTMLElement;
    if (target.closest('button')) return;
    const win = winRef.current;
    if (win) {
      await win.startDragging();
    }
  }, []);

  const handleDoubleClick = useCallback((e: React.MouseEvent) => {
    const target = e.target as HTMLElement;
    if (target.closest('button')) return;
    invoke('toggle_maximize_main_window');
  }, []);

  const handleMinimize = useCallback(() => invoke('minimize_main_window'), []);
  const handleToggleMaximize = useCallback(() => invoke('toggle_maximize_main_window'), []);
  const handleClose = useCallback(() => invoke('close_main_window'), []);

  const settingsHover = useSafeHover();
  const minimizeHover = useSafeHover();
  const maximizeHover = useSafeHover();
  const closeHover = useSafeHover();

  return (
    <div
      onMouseDown={handleDragStart}
      onDoubleClick={handleDoubleClick}
      className="flex items-center h-9 shrink-0 bg-surface-secondary border-b border-divider select-none cursor-default"
    >
      <img src={titlebarIcon} alt="" className="w-4 h-4 ml-3 pointer-events-none" />
      <span className="text-xs font-medium text-ink-secondary ml-2">
        {t('app.name')}
      </span>

      <div className="flex-1" />

      <div className="flex items-center h-full">
        <button
          onMouseDown={(e) => e.stopPropagation()}
          onClick={onOpenSettings}
          {...settingsHover.hoverProps}
          title={t('toolbar.settings')}
          className={cn(
            'flex items-center justify-center w-11 h-full',
            'text-ink-secondary transition',
            settingsHover.isHovered && 'bg-hover-bg',
          )}
          aria-label={t('toolbar.settings')}
        >
          <Settings size={15} />
        </button>
        {/* Window controls stay out of the Tab order, like native caption buttons. */}
        <button
          onMouseDown={(e) => e.stopPropagation()}
          onClick={() => {
            minimizeHover.clearHover();
            handleMinimize();
          }}
          {...minimizeHover.hoverProps}
          className={cn(
            'flex items-center justify-center w-11 h-full',
            'text-ink-secondary transition',
            minimizeHover.isHovered && 'bg-hover-bg',
          )}
          aria-label="Minimize"
          tabIndex={-1}
        >
          <Minus size={15} />
        </button>
        <button
          onMouseDown={(e) => e.stopPropagation()}
          onClick={handleToggleMaximize}
          {...maximizeHover.hoverProps}
          className={cn(
            'flex items-center justify-center w-11 h-full',
            'text-ink-secondary transition',
            maximizeHover.isHovered && 'bg-hover-bg',
          )}
          aria-label="Maximize"
          tabIndex={-1}
        >
          {isMaximized ? <Copy size={13} /> : <Square size={13} />}
        </button>
        <button
          onMouseDown={(e) => e.stopPropagation()}
          onClick={() => {
            closeHover.clearHover();
            handleClose();
          }}
          {...closeHover.hoverProps}
          className={cn(
            'flex items-center justify-center w-11 h-full',
            'text-ink-secondary transition',
            closeHover.isHovered && 'bg-danger text-white',
          )}
          aria-label="Close"
          tabIndex={-1}
        >
          <X size={16} />
        </button>
      </div>
    </div>
  );
}