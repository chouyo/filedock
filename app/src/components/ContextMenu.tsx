import { useEffect, useLayoutEffect, useRef, useState } from 'react';
import { useI18n } from '../i18n/useI18n';
import { useToast } from './Toast';
import { writeText } from '@tauri-apps/plugin-clipboard-manager';
import { invoke } from '@tauri-apps/api/core';
import type { FileEntry } from '../types';

interface ContextMenuProps {
  x: number;
  y: number;
  file: FileEntry;
  onClose: () => void;
}

export function ContextMenu({ x, y, file, onClose }: ContextMenuProps) {
  const { t } = useI18n();
  const { showToast } = useToast();
  const ref = useRef<HTMLDivElement>(null);
  const [pos, setPos] = useState({ left: x, top: y });

  useLayoutEffect(() => {
    if (!ref.current) return;
    const rect = ref.current.getBoundingClientRect();
    setPos({
      left: Math.min(x, window.innerWidth - rect.width - 8),
      top: Math.min(y, window.innerHeight - rect.height - 8),
    });
  }, [x, y]);

  useEffect(() => {
    const onKey = (e: KeyboardEvent) => e.key === 'Escape' && onClose();
    const onClick = () => onClose();
    window.addEventListener('keydown', onKey);
    window.addEventListener('click', onClick);
    window.addEventListener('scroll', onClick, true);
    return () => {
      window.removeEventListener('keydown', onKey);
      window.removeEventListener('click', onClick);
      window.removeEventListener('scroll', onClick, true);
    };
  }, [onClose]);

  const copyFileName = async (e: React.MouseEvent) => {
    e.stopPropagation();
    try {
      await writeText(file.name);
      showToast(t('toast.copiedName'), 'success');
    } catch (err) {
      console.error('Copy filename failed:', err);
    }
    onClose();
  };

  const copyFullPath = async (e: React.MouseEvent) => {
    e.stopPropagation();
    try {
      await writeText(file.path);
      showToast(t('toast.copiedPath'), 'success');
    } catch (err) {
      console.error('Copy path failed:', err);
    }
    onClose();
  };

  const openInExplorer = async (e: React.MouseEvent) => {
    e.stopPropagation();
    try {
      await invoke('open_in_explorer', { path: file.path });
    } catch (err) {
      console.error('Open explorer failed:', err);
    }
    onClose();
  };

  const items = [
    { label: t('contextMenu.copyName'), action: copyFileName },
    { label: t('contextMenu.copyFullPath'), action: copyFullPath },
    { label: t('contextMenu.openFolder'), action: openInExplorer },
  ];

  return (
    <div
      ref={ref}
      className="fixed z-50 bg-surface-elevated border border-divider rounded-lg shadow-lg py-1 min-w-[180px] text-sm"
      style={pos}
      onClick={(e) => e.stopPropagation()}
    >
      {items.map((item, i) => (
        <button
          key={i}
          onClick={item.action}
          onMouseDown={(e) => e.stopPropagation()}
          className="w-full text-left px-3 py-1.5 hover:bg-hover-bg transition text-ink"
        >
          {item.label}
        </button>
      ))}
    </div>
  );
}
