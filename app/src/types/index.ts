export type Language = 'en' | 'zh-CN';
export type SortField = 'name' | 'createdAt' | 'updatedAt';
export type SortDirection = 'asc' | 'desc';
export type CloseAction = 'ask' | 'close' | 'minimize';
export type Theme = 'system' | 'light' | 'dark';
export type ColumnKey =
  | 'name'
  | 'extension'
  | 'size'
  | 'createdAt'
  | 'modifiedAt'
  | 'accessedAt'
  | 'parentDir'
  | 'path'
  | 'isReadonly';

export interface Config {
  version: number;
  categories: Category[];
  session: Session;
}

export interface Category {
  id: string;
  name: string;
  order: number;
  targets: TargetRule[];
  createdAt: number;
  updatedAt: number;
}

export interface TargetRule {
  id: string;
  dir: string;
  matchType: 'glob' | 'regex';
  pattern: string;
  recursive: boolean;
}

export interface Session {
  activeCategoryId: string | null;
  searchText: string;
}

export interface Settings {
  version: number;
  language: Language;
  theme: Theme;
  closeAction: CloseAction;
  columns: string[];
  windowState: WindowState | null;
  autoRefreshEnabled: boolean;
  watchDebounceMs: number;
}

export interface WindowState {
  monitorName: string | null;
  monitorPosition: { x: number; y: number };
  monitorSize: { width: number; height: number };
  windowX: number;
  windowY: number;
  windowWidth: number;
  windowHeight: number;
  maximized: boolean;
}

export interface FileEntry {
  path: string;
  name: string;
  extension: string;
  parentDir: string;
  size: number;
  createdAt: number;
  modifiedAt: number;
  accessedAt: number;
  isReadonly: boolean;
}

export interface UpdateInfo {
  version: string;
  notes: string;
  date: string;
}

export interface SortState {
  field: SortField;
  direction: SortDirection;
}

export interface PathInfo {
  path: string;
  kind: 'dir' | 'file' | 'missing';
  name: string;
  /** Lowercase, without the leading dot; empty when there is none. */
  extension: string;
  parentDir: string;
}

/** How dropped files become rules: match the exact files, or their types. */
export type DropMode = 'file' | 'type';

export interface CategoryDraft {
  name: string;
  targets: TargetRule[];
}
