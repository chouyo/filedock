import { useDeferredValue, useEffect, useLayoutEffect, useMemo, useRef, useState } from 'react';
import { RefreshCw, SlidersHorizontal } from 'lucide-react';
import { useI18n } from '../i18n/useI18n';
import { cn } from '../lib/utils';
import { ALL_COLUMNS, FLEX_COLUMN_MIN_WIDTH, getColumnDef, type ColumnDef } from '../lib/columns';
import { useVirtualRows } from '../lib/useVirtualRows';
import { FileRow, FILE_ROW_HEIGHT } from './FileRow';
import { Tooltip } from './Tooltip';
import { SearchInput } from './SearchInput';
import type { FileEntry, ColumnKey, Language } from '../types';

/** Width in px of the trailing column that holds the column settings button. */
const SETTINGS_COLUMN_WIDTH = 40;

// Same ordering as String.prototype.localeCompare, without building a
// collator on every comparison.
const collator = new Intl.Collator();

interface FileTableProps {
  /** Scroll resets to the top when this changes (the active category). */
  categoryId: string;
  files: FileEntry[];
  columns: ColumnKey[];
  lang: Language;
  searchText: string;
  inFlight: boolean;
  watchActive: boolean;
  onSearchChange: (text: string) => void;
  onClearSearch: () => void;
  onRefresh: () => void;
  onToggleColumn: (key: string) => void;
  onContextMenu: (x: number, y: number, file: FileEntry) => void;
  searchInputRef?: React.RefObject<HTMLInputElement>;
}

export function FileTable({
  categoryId,
  files,
  columns,
  lang,
  searchText,
  inFlight,
  watchActive,
  onSearchChange,
  onClearSearch,
  onRefresh,
  onToggleColumn,
  onContextMenu,
  searchInputRef,
}: FileTableProps) {
  const { t } = useI18n();
  const [sortCol, setSortCol] = useState<ColumnKey>('createdAt');
  const [sortDir, setSortDir] = useState<'asc' | 'desc'>('asc');
  const [showColumnSettings, setShowColumnSettings] = useState(false);
  const columnSettingsRef = useRef<HTMLDivElement>(null);
  const scrollRef = useRef<HTMLDivElement>(null);

  // Filtering runs on the deferred value so typing stays responsive; the
  // input itself is bound to `searchText`.
  const deferredSearchText = useDeferredValue(searchText);

  const columnDefs = useMemo(
    () => columns.map(getColumnDef).filter((d): d is ColumnDef => !!d),
    [columns],
  );
  const tableMinWidth = useMemo(
    () =>
      columnDefs.reduce((sum, d) => sum + (d.width ?? FLEX_COLUMN_MIN_WIDTH), 0) +
      SETTINGS_COLUMN_WIDTH,
    [columnDefs],
  );

  const sortedFiles = useMemo(() => {
    if (!sortCol) return files;
    const sorted = [...files];
    sorted.sort((a, b) => {
      let cmp = 0;
      const va = (a as unknown as Record<string, unknown>)[sortCol];
      const vb = (b as unknown as Record<string, unknown>)[sortCol];
      if (typeof va === 'number' && typeof vb === 'number') {
        cmp = va - vb;
      } else {
        cmp = collator.compare(String(va ?? ''), String(vb ?? ''));
      }
      return sortDir === 'asc' ? cmp : -cmp;
    });
    return sorted;
  }, [files, sortCol, sortDir]);

  const filteredFiles = useMemo(() => {
    if (!deferredSearchText) return sortedFiles;
    const q = deferredSearchText.toLowerCase();
    return sortedFiles.filter((f) => f.name.toLowerCase().includes(q));
  }, [sortedFiles, deferredSearchText]);

  const { start, end, paddingTop, paddingBottom } = useVirtualRows({
    count: filteredFiles.length,
    rowHeight: FILE_ROW_HEIGHT,
    scrollRef,
  });

  // A different category, query or ordering starts from the top. Auto
  // refreshes only replace `files`, so they keep the scroll position.
  useLayoutEffect(() => {
    if (scrollRef.current) scrollRef.current.scrollTop = 0;
  }, [categoryId, deferredSearchText, sortCol, sortDir]);

  const handleSort = (key: ColumnKey) => {
    if (sortCol === key) {
      setSortDir((d) => (d === 'asc' ? 'desc' : 'asc'));
    } else {
      setSortCol(key);
      setSortDir('asc');
    }
  };

  useEffect(() => {
    if (!showColumnSettings) return;
    const handleClickOutside = (e: MouseEvent) => {
      if (
        columnSettingsRef.current &&
        !columnSettingsRef.current.contains(e.target as Node)
      ) {
        setShowColumnSettings(false);
      }
    };
    const handleKeyDown = (e: KeyboardEvent) => {
      if (e.key === 'Escape') setShowColumnSettings(false);
    };
    window.addEventListener('mousedown', handleClickOutside);
    window.addEventListener('keydown', handleKeyDown);
    return () => {
      window.removeEventListener('mousedown', handleClickOutside);
      window.removeEventListener('keydown', handleKeyDown);
    };
  }, [showColumnSettings]);

  return (
    <div className="flex flex-col flex-1 min-h-0">
      <div className="flex items-center gap-2 px-3 py-2 border-b border-divider shrink-0">
        <SearchInput
          value={searchText}
          onChange={onSearchChange}
          onClear={onClearSearch}
          placeholder={t('toolbar.searchFiles')}
          containerClassName="flex-1"
          inputRef={searchInputRef}
        />
        <button
          disabled={inFlight}
          aria-busy={inFlight}
          title={t('toolbar.refresh')}
          onClick={onRefresh}
          className="p-1.5 rounded hover:bg-hover-bg disabled:opacity-50 text-ink-secondary hover:text-ink transition"
        >
          <RefreshCw className={cn(inFlight && 'animate-spin')} size={16} />
        </button>
      </div>

      <div ref={scrollRef} className="flex-1 overflow-auto relative">
        <table
          className="w-full table-fixed"
          style={{ minWidth: tableMinWidth }}
          aria-rowcount={filteredFiles.length + 1}
          onContextMenu={(e) => e.preventDefault()}
        >
          <colgroup>
            {columnDefs.map((def) => (
              <col key={def.key} style={def.width ? { width: def.width } : undefined} />
            ))}
            <col style={{ width: SETTINGS_COLUMN_WIDTH }} />
          </colgroup>
          <thead className="sticky top-0 bg-surface-secondary z-10">
            <tr aria-rowindex={1} className="border-b border-divider">
              {columnDefs.map((def) => {
                const key = def.key;
                return (
                  <th
                    key={key}
                    onClick={() => def.sortable && handleSort(key)}
                    className={cn(
                      'px-3 py-2 text-xs font-medium text-ink-secondary text-left whitespace-nowrap overflow-hidden text-ellipsis',
                      def.align === 'right' && 'text-right',
                      def.align === 'center' && 'text-center',
                      def.sortable && 'cursor-pointer hover:text-ink',
                    )}
                  >
                    {t(def.labelKey)}
                    {sortCol === key && (
                      <span className="ml-1">{sortDir === 'asc' ? '▲' : '▼'}</span>
                    )}
                  </th>
                );
              })}
              <th className="relative px-2 py-2">
                <div ref={columnSettingsRef} className="relative">
                  <button
                    onClick={() => setShowColumnSettings((v) => !v)}
                    className="p-1 rounded hover:bg-hover-bg text-ink-secondary hover:text-ink"
                  >
                    <SlidersHorizontal size={14} />
                  </button>
                  {showColumnSettings && (
                    <div
                      className="absolute right-0 top-full mt-1 z-50 bg-surface-elevated border border-divider rounded-lg shadow-lg py-1 min-w-[160px]"
                      onClick={(e) => e.stopPropagation()}
                    >
                      {ALL_COLUMNS.map((col) => (
                        <label
                          key={col.key}
                          className="flex items-center gap-2 px-3 py-1.5 hover:bg-hover-bg cursor-pointer text-xs text-ink"
                        >
                          <input
                            type="checkbox"
                            checked={columns.includes(col.key)}
                            onChange={() => onToggleColumn(col.key)}
                          />
                          {t(col.labelKey)}
                        </label>
                      ))}
                    </div>
                  )}
                </div>
              </th>
            </tr>
          </thead>
          <tbody>
            {paddingTop > 0 && (
              <tr aria-hidden="true" style={{ height: paddingTop }}>
                <td colSpan={columnDefs.length + 1} className="p-0" />
              </tr>
            )}
            {filteredFiles.slice(start, end).map((file, i) => (
              <FileRow
                key={file.path}
                file={file}
                columns={columnDefs}
                lang={lang}
                rowIndex={start + i + 2}
                onContextMenu={onContextMenu}
              />
            ))}
            {paddingBottom > 0 && (
              <tr aria-hidden="true" style={{ height: paddingBottom }}>
                <td colSpan={columnDefs.length + 1} className="p-0" />
              </tr>
            )}
          </tbody>
        </table>
        {filteredFiles.length === 0 && (
          <div className="flex items-center justify-center py-12 text-ink-secondary text-sm">
            {t('files.noResults')}
          </div>
        )}

        {inFlight && (
          <div className="absolute inset-0 z-20 flex items-center justify-center bg-black/30 backdrop-blur-sm">
            <div className="flex flex-col items-center gap-3">
              <div className="w-10 h-10 border-4 border-accent border-t-transparent rounded-full animate-spin" />
              <div className="text-sm text-white/80">{t('loading.files')}</div>
            </div>
          </div>
        )}
      </div>

      <div className="flex items-center gap-2 px-3 py-1 border-t border-divider shrink-0 text-xs text-ink-secondary">
        <span>{t('files.count', { count: filteredFiles.length })}</span>
        <div className="flex-1" />
        {watchActive && (
          <Tooltip content={t('status.watching')} placement="top">
            <span className="flex items-center gap-1.5">
              <span className="w-1.5 h-1.5 rounded-full bg-green-500 animate-pulse" />
            </span>
          </Tooltip>
        )}
      </div>
    </div>
  );
}
