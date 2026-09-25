import { Fragment, useCallback, useEffect, useLayoutEffect, useRef, useState } from 'react';
import { Search, X } from 'lucide-react';
import { readText, writeText } from '@tauri-apps/plugin-clipboard-manager';
import { useI18n } from '../i18n/useI18n';
import { cn } from '../lib/utils';

interface SearchInputProps {
  value: string;
  onChange: (value: string) => void;
  onClear: () => void;
  placeholder: string;
  containerClassName?: string;
  inputRef?: React.RefObject<HTMLInputElement>;
}

type MenuAction = 'cut' | 'copy' | 'paste' | 'delete' | 'selectAll';

export function SearchInput({
  value,
  onChange,
  onClear,
  placeholder,
  containerClassName,
  inputRef: externalInputRef,
}: SearchInputProps) {
  const { t } = useI18n();
  const internalRef = useRef<HTMLInputElement>(null);
  const inputRef = externalInputRef ?? internalRef;
  const menuRef = useRef<HTMLDivElement>(null);
  const [menu, setMenu] = useState<{ x: number; y: number; hasSelection: boolean } | null>(null);
  const [pos, setPos] = useState<{ left: number; top: number } | null>(null);
  const pendingSelection = useRef<{ start: number; end: number } | null>(null);
  const onChangeRef = useRef(onChange);
  onChangeRef.current = onChange;

  // Restore cursor selection after controlled value updates triggered by menu actions
  useLayoutEffect(() => {
    if (!pendingSelection.current || !inputRef.current) return;
    const { start, end } = pendingSelection.current;
    inputRef.current.focus();
    inputRef.current.setSelectionRange(start, end);
    pendingSelection.current = null;
  });

  // Keep the menu within the viewport
  useLayoutEffect(() => {
    if (!menu || !menuRef.current) return;
    const rect = menuRef.current.getBoundingClientRect();
    setPos({
      left: Math.min(menu.x, window.innerWidth - rect.width - 8),
      top: Math.min(menu.y, window.innerHeight - rect.height - 8),
    });
  }, [menu]);

  // Close the menu on outside interaction / escape / scroll
  useEffect(() => {
    if (!menu) return;
    const onKey = (e: KeyboardEvent) => e.key === 'Escape' && setMenu(null);
    const onDown = (e: MouseEvent) => {
      if (menuRef.current && !menuRef.current.contains(e.target as Node)) setMenu(null);
    };
    const onScroll = () => setMenu(null);
    window.addEventListener('keydown', onKey);
    window.addEventListener('mousedown', onDown);
    window.addEventListener('scroll', onScroll, true);
    return () => {
      window.removeEventListener('keydown', onKey);
      window.removeEventListener('mousedown', onDown);
      window.removeEventListener('scroll', onScroll, true);
    };
  }, [menu]);

  const handleContextMenu = useCallback(
    (e: React.MouseEvent<HTMLInputElement>) => {
      e.preventDefault();
      e.stopPropagation();
      const input = inputRef.current;
      const start = input?.selectionStart ?? 0;
      const end = input?.selectionEnd ?? 0;
      setMenu({ x: e.clientX, y: e.clientY, hasSelection: start !== end });
    },
    [inputRef],
  );

  const applyValue = useCallback(
    (newVal: string, selStart: number, selEnd: number) => {
      pendingSelection.current = { start: selStart, end: selEnd };
      onChangeRef.current(newVal);
    },
    [],
  );

  const runAction = useCallback(
    async (action: MenuAction) => {
      const input = inputRef.current;
      if (!input) {
        setMenu(null);
        return;
      }
      const { selectionStart, selectionEnd, value: val } = input;
      const start = selectionStart ?? 0;
      const end = selectionEnd ?? val.length;
      const selected = val.slice(start, end);
      const hasSelection = start !== end;

      switch (action) {
        case 'cut':
          if (hasSelection) {
            try {
              await writeText(selected);
            } catch {
              /* ignore clipboard errors */
            }
            applyValue(val.slice(0, start) + val.slice(end), start, start);
          }
          break;
        case 'copy':
          if (hasSelection) {
            try {
              await writeText(selected);
            } catch {
              /* ignore clipboard errors */
            }
          }
          break;
        case 'paste': {
          let clip = '';
          try {
            clip = await readText();
          } catch {
            /* ignore clipboard errors */
          }
          if (clip) {
            applyValue(
              val.slice(0, start) + clip + val.slice(end),
              start + clip.length,
              start + clip.length,
            );
          }
          break;
        }
        case 'delete':
          if (hasSelection) {
            applyValue(val.slice(0, start) + val.slice(end), start, start);
          }
          break;
        case 'selectAll':
          input.focus();
          input.setSelectionRange(0, val.length);
          break;
      }
      setMenu(null);
    },
    [applyValue, inputRef],
  );

  const items: { action: MenuAction; label: string; shortcut: string; disabled: boolean }[] = [
    { action: 'cut', label: t('inputMenu.cut'), shortcut: 'Ctrl+X', disabled: !menu?.hasSelection },
    { action: 'copy', label: t('inputMenu.copy'), shortcut: 'Ctrl+C', disabled: !menu?.hasSelection },
    { action: 'paste', label: t('inputMenu.paste'), shortcut: 'Ctrl+V', disabled: false },
    { action: 'delete', label: t('inputMenu.delete'), shortcut: 'Del', disabled: !menu?.hasSelection },
    { action: 'selectAll', label: t('inputMenu.selectAll'), shortcut: 'Ctrl+A', disabled: false },
  ];

  return (
    <div className={cn('relative', containerClassName)}>
      <Search
        size={14}
        className="absolute left-2.5 top-1/2 -translate-y-1/2 text-ink-secondary pointer-events-none"
      />
      <input
        ref={inputRef}
        type="text"
        value={value}
        onChange={(e) => onChange(e.target.value)}
        onContextMenu={handleContextMenu}
        placeholder={placeholder}
        autoComplete="off"
        autoCorrect="off"
        autoCapitalize="off"
        spellCheck={false}
        className="w-full pl-8 pr-8 py-1.5 text-sm rounded-md bg-surface-input border border-divider text-ink focus:outline-none focus:border-accent"
      />
      {value && (
        <button
          onClick={onClear}
          className="absolute right-2 top-1/2 -translate-y-1/2 text-ink-secondary hover:text-ink transition"
          aria-label={t('common.clear')}
        >
          <X size={14} />
        </button>
      )}
      {menu && (
        <div
          ref={menuRef}
          className="fixed z-50 bg-surface-elevated border border-divider rounded-lg shadow-lg py-1 min-w-[180px] text-sm"
          style={pos ?? { left: menu.x, top: menu.y }}
          onClick={(e) => e.stopPropagation()}
          onMouseDown={(e) => e.stopPropagation()}
        >
          {items.map((item, i) => (
            <Fragment key={item.action}>
              {i === 4 && <div className="my-1 border-t border-divider" />}
              <button
                onClick={() => !item.disabled && runAction(item.action)}
                onMouseDown={(e) => e.stopPropagation()}
                className={cn(
                  'flex items-center justify-between gap-4 w-full text-left px-3 py-1.5 transition text-ink',
                  item.disabled ? 'opacity-40 cursor-default' : 'hover:bg-hover-bg cursor-pointer',
                )}
              >
                <span>{item.label}</span>
                <span className="text-xs text-ink-secondary">{item.shortcut}</span>
              </button>
            </Fragment>
          ))}
        </div>
      )}
    </div>
  );
}