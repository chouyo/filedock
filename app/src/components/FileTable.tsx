import { useEffect, useMemo, useRef, useState } from 'react';
import { RefreshCw, SlidersHorizontal } from 'lucide-react';
import { useI18n } from '../i18n/useI18n';
import { cn } from '../lib/utils';
import { ALL_COLUMNS } from '../lib/columns';
import { FileRow } from './FileRow';
import { Tooltip } from './Tooltip';
import { SearchInput } from './SearchInput';
import type { FileEntry, ColumnKey, Language } from '../types';

interface FileTableProps {
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
        cmp = String(va ?? '').localeCompare(String(vb ?? ''));
      }
      return sortDir === 'asc' ? cmp : -cmp;
    });
    return sorted;
  }, [files, sortCol, sortDir]);

  const filteredFiles = useMemo(() => {
    if (!searchText) return sortedFiles;
    const q = searchText.toLowerCase();
    return sortedFiles.filter((f) => f.name.toLowerCase().includes(q));
  }, [sortedFiles, searchText]);

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

      <div className="flex-1 overflow-auto relative">
        <table className="w-full" onContextMenu={(e) => e.preventDefault()}>
          <thead className="sticky top-0 bg-surface-secondary z-10">
            <tr className="border-b border-divider">
              {columns.map((key) => {
                const def = ALL_COLUMNS.find((c) => c.key === key);
                if (!def) return null;
                return (
                  <th
                    key={key}
                    onClick={() => def.sortable && handleSort(key)}
                    className={cn(
                      'px-3 py-2 text-xs font-medium text-ink-secondary text-left whitespace-nowrap',
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
              <th className="relative w-8 px-2 py-2">
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
            {filteredFiles.map((file) => (
              <FileRow
                key={file.path}
                file={file}
                columns={columns}
                lang={lang}
                onContextMenu={onContextMenu}
              />
            ))}
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
