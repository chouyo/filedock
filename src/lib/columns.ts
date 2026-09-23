import type { ColumnKey, FileEntry, Language } from '../types';
import { formatSize, formatTime } from './format';

export interface ColumnDef {
  key: ColumnKey;
  labelKey: string;
  sortable: boolean;
  align?: 'left' | 'right' | 'center';
  defaultVisible: boolean;
}

export const ALL_COLUMNS: ColumnDef[] = [
  { key: 'name', labelKey: 'table.column.name', sortable: true, align: 'left', defaultVisible: true },
  { key: 'extension', labelKey: 'table.column.extension', sortable: true, align: 'left', defaultVisible: false },
  { key: 'size', labelKey: 'table.column.size', sortable: true, align: 'right', defaultVisible: false },
  { key: 'createdAt', labelKey: 'table.column.createdAt', sortable: true, align: 'left', defaultVisible: true },
  { key: 'modifiedAt', labelKey: 'table.column.modifiedAt', sortable: true, align: 'left', defaultVisible: false },
  { key: 'accessedAt', labelKey: 'table.column.accessedAt', sortable: true, align: 'left', defaultVisible: false },
  { key: 'parentDir', labelKey: 'table.column.parentDir', sortable: true, align: 'left', defaultVisible: false },
  { key: 'path', labelKey: 'table.column.path', sortable: false, align: 'left', defaultVisible: false },
  { key: 'isReadonly', labelKey: 'table.column.isReadonly', sortable: true, align: 'center', defaultVisible: false },
];

export const DEFAULT_COLUMNS: ColumnKey[] = ALL_COLUMNS
  .filter((c) => c.defaultVisible)
  .map((c) => c.key);

export function getColumnDef(key: string): ColumnDef | undefined {
  return ALL_COLUMNS.find((c) => c.key === key);
}

export function renderCell(
  file: FileEntry,
  col: ColumnDef,
  lang: Language,
): string {
  switch (col.key) {
    case 'size':
      return formatSize(file.size);
    case 'createdAt':
    case 'modifiedAt':
    case 'accessedAt':
      return formatTime(file[col.key], lang);
    case 'isReadonly':
      return file.isReadonly ? '✓' : '—';
    case 'extension':
      return file.extension ? `.${file.extension}` : '—';
    default:
      return String((file as unknown as Record<string, unknown>)[col.key] ?? '');
  }
}
