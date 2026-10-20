import { useCallback, useEffect, useMemo, useRef, useState } from 'react';
import { invoke } from '@tauri-apps/api/core';
import { listen } from '@tauri-apps/api/event';
import { getCurrentWebviewWindow } from '@tauri-apps/api/webviewWindow';
import { Plus } from 'lucide-react';
import { useI18n } from './i18n/useI18n';
import { useToast } from './components/Toast';
import { SplashScreen } from './components/splash/SplashScreen';
import { EmptyGuide } from './components/EmptyGuide';
import { TitleBar } from './components/TitleBar';
import { CategoryToolbar } from './components/CategoryToolbar';
import type { ToolbarItem } from './components/CategoryToolbar';
import { ContextMenu } from './components/ContextMenu';
import { FileTable } from './components/FileTable';
import { CategoryList } from './components/CategoryList';
import { CategoryEditDialog } from './components/CategoryEditDialog';
import { SettingsDialog } from './components/SettingsDialog';
import { CloseDialog } from './components/CloseDialog';
import { ConfirmDialog } from './components/ConfirmDialog';
import { CategoryDropOverlay } from './components/CategoryDropOverlay';
import { DEFAULT_COLUMNS } from './lib/columns';
import { buildDraft } from './lib/dropDraft';
import { useCategoryDrop } from './lib/useCategoryDrop';
import type {
  Config,
  Settings,
  Category,
  CategoryDraft,
  FileEntry,
  ColumnKey,
  Theme,
} from './types';

export function App() {
  const { t, lang, setLang } = useI18n();
  const { showToast } = useToast();

  const [showSplash, setShowSplash] = useState(true);
  const [config, setConfig] = useState<Config | null>(null);
  const [settings, setSettings] = useState<Settings | null>(null);
  // Bumped to make the file table refetch its visible rows.
  const [reloadToken, setReloadToken] = useState(0);
  // A manual refresh (full rescan) is running.
  const [inFlight, setInFlight] = useState(false);
  const [categorySearch, setCategorySearch] = useState('');
  const [windowVisible, setWindowVisible] = useState(true);

  const [showSettings, setShowSettings] = useState(false);
  const [showCloseDialog, setShowCloseDialog] = useState(false);
  const [showQuitConfirm, setShowQuitConfirm] = useState(false);
  const [showCategoryEdit, setShowCategoryEdit] = useState(false);
  const [editingCategory, setEditingCategory] = useState<Category | null>(null);
  const [categoryDraft, setCategoryDraft] = useState<CategoryDraft | null>(null);
  const [deleteTarget, setDeleteTarget] = useState<string | null>(null);
  const [contextMenu, setContextMenu] = useState<{ x: number; y: number; file: FileEntry } | null>(null);

  const searchTimer = useRef<ReturnType<typeof setTimeout> | undefined>(undefined);
  const lastCategoryClickRef = useRef<{ id: string; time: number }>({ id: '', time: 0 });
  const fileSearchRef = useRef<HTMLInputElement>(null);
  const categoryDropRef = useRef<HTMLDivElement>(null);

  const categories = useMemo(() => {
    if (!config) return [];
    return [...config.categories].sort((a, b) => a.order - b.order);
  }, [config]);

  const activeCategoryId = config?.session.activeCategoryId ?? null;
  const searchText = config?.session.searchText ?? '';
  const columns: ColumnKey[] = (settings?.columns as ColumnKey[]) ?? DEFAULT_COLUMNS;

  const deleteCategoryName = useMemo(() => {
    if (!deleteTarget) return '';
    return categories.find((c) => c.id === deleteTarget)?.name ?? '';
  }, [deleteTarget, categories]);

  const watchActive =
    !!settings?.autoRefreshEnabled && !!activeCategoryId && windowVisible;

  const noopCheckUpdate = useCallback(async () => {}, []);

  const applyTheme = useCallback((theme: Theme) => {
    const prefersDark = window.matchMedia('(prefers-color-scheme: dark)').matches;
    const isDark = theme === 'dark' || (theme === 'system' && prefersDark);
    document.documentElement.classList.toggle('dark', isDark);
    // Match the native window/webview background to the theme so the area a
    // live resize exposes before the webview catches up (visible on macOS)
    // blends in instead of flashing the default white.
    const bg = getComputedStyle(document.documentElement).getPropertyValue('--bg-primary').trim();
    if (bg) getCurrentWebviewWindow().setBackgroundColor(bg).catch(() => {});
  }, []);

  useEffect(() => {
    if (settings) applyTheme(settings.theme);
  }, [settings, applyTheme]);

  useEffect(() => {
    if (settings?.theme !== 'system') return;
    const mq = window.matchMedia('(prefers-color-scheme: dark)');
    const handler = () => applyTheme('system');
    mq.addEventListener('change', handler);
    return () => mq.removeEventListener('change', handler);
  }, [settings?.theme, applyTheme]);

  // ---- Load config and settings on mount ----
  useEffect(() => {
    const load = async () => {
      const [cfg, set] = await Promise.all([
        invoke<Config>('load_config'),
        invoke<Settings>('load_settings'),
      ]);

      if (cfg.session.activeCategoryId) {
        const exists = cfg.categories.some((c) => c.id === cfg.session.activeCategoryId);
        if (!exists) {
          cfg.session.activeCategoryId = cfg.categories.length > 0 ? cfg.categories[0].id : null;
          await invoke('save_session', { session: cfg.session });
        }
      } else if (cfg.categories.length > 0) {
        cfg.session.activeCategoryId = cfg.categories[0].id;
        await invoke('save_session', { session: cfg.session });
      }

      setConfig(cfg);
      setSettings(set);
      setLang(set.language);

      await new Promise((resolve) => setTimeout(resolve, 3000));
      setShowSplash(false);
    };
    load();
  }, [setLang]);

  // ---- Listen for Rust events ----
  useEffect(() => {
    const unlistenClose = listen('close-requested', () => setShowCloseDialog(true));
    const unlistenSettings = listen('open-settings', () => {
      setShowSettings(true);
    });
    const unlistenQuit = listen('request-quit', () => setShowQuitConfirm(true));
    const unlistenVisibility = listen<boolean>('window-visibility', (e) => {
      setWindowVisible(e.payload === true);
    });

    return () => {
      unlistenClose.then((fn) => fn());
      unlistenSettings.then((fn) => fn());
      unlistenQuit.then((fn) => fn());
      unlistenVisibility.then((fn) => fn());
    };
  }, []);

  // ---- Disable default context menu except on file table rows ----
  useEffect(() => {
    const handler = (e: MouseEvent) => {
      const target = e.target as HTMLElement;
      if (!target.closest('tbody tr')) {
        e.preventDefault();
      }
    };
    window.addEventListener('contextmenu', handler);
    return () => window.removeEventListener('contextmenu', handler);
  }, []);

  // ---- File list reloads ----
  // The table fetches the rows it shows from the index; a reload only makes
  // it fetch them again, so bursts of reloads are cheap.
  const refreshFiles = useCallback(() => setReloadToken((n) => n + 1), []);

  // ---- Auto refresh on file-change events ----
  useEffect(() => {
    const unlisten = listen<string>('files-changed', (e) => {
      if (windowVisible && activeCategoryId && e.payload === activeCategoryId) {
        refreshFiles();
      }
    });
    return () => {
      unlisten.then((fn) => fn());
    };
  }, [activeCategoryId, windowVisible, refreshFiles]);

  // ---- Actively refresh once when the window returns to visible ----
  const prevVisibleRef = useRef(true);
  useEffect(() => {
    if (prevVisibleRef.current === false && windowVisible === true) {
      if (activeCategoryId && settings?.autoRefreshEnabled) {
        refreshFiles();
      }
    }
    prevVisibleRef.current = windowVisible;
  }, [windowVisible, activeCategoryId, settings?.autoRefreshEnabled, refreshFiles]);

  // ---- F12 devtools shortcut (debug only) ----
  useEffect(() => {
    if (!import.meta.env.DEV) return;
    const handler = (e: KeyboardEvent) => {
      if (e.key === 'F12') {
        e.preventDefault();
        invoke('open_devtools');
      }
    };
    window.addEventListener('keydown', handler);
    return () => window.removeEventListener('keydown', handler);
  }, []);

  // ---- Ctrl+F focuses the file search box (overrides default find) ----
  useEffect(() => {
    const handler = (e: KeyboardEvent) => {
      if ((e.ctrlKey || e.metaKey) && (e.key === 'f' || e.key === 'F')) {
        e.preventDefault();
        const input = fileSearchRef.current;
        if (input) {
          input.focus();
          input.select();
        }
      }
    };
    window.addEventListener('keydown', handler);
    return () => window.removeEventListener('keydown', handler);
  }, []);

  // ---- Drop files/folders onto the sidebar to create a category ----
  const openNewCategory = (draft: CategoryDraft | null = null) => {
    setEditingCategory(null);
    setCategoryDraft(draft);
    setShowCategoryEdit(true);
  };

  const anyModalOpen =
    showCategoryEdit || showSettings || showCloseDialog || showQuitConfirm || !!deleteTarget;

  const categoryDrop = useCategoryDrop({
    containerRef: categoryDropRef,
    disabled: anyModalOpen,
    onDrop: (infos, mode) => {
      const draft = buildDraft(infos, mode);
      if (draft) openNewCategory(draft);
      else showToast(t('toast.dropUnrecognized'), 'error');
    },
    onUnrecognized: () => showToast(t('toast.dropUnrecognized'), 'error'),
  });

  // ---- Category operations ----
  const selectCategory = (id: string) => {
    const now = Date.now();
    if (lastCategoryClickRef.current.id === id && now - lastCategoryClickRef.current.time < 300) {
      return;
    }
    lastCategoryClickRef.current = { id, time: now };

    // Clicking the open category reloads it.
    if (id === activeCategoryId) {
      refreshFiles();
      return;
    }
    setConfig((prev) =>
      prev ? { ...prev, session: { ...prev.session, activeCategoryId: id } } : prev,
    );
    if (config) {
      invoke('save_session', { session: { ...config.session, activeCategoryId: id } });
    }
  };

  const handleCategorySaved = async () => {
    const cats = await invoke<Category[]>('list_categories');
    const wasEmpty = config ? config.categories.length === 0 : false;
    setConfig((prev) => (prev ? { ...prev, categories: cats } : prev));
    if (wasEmpty && cats.length > 0) {
      const first = [...cats].sort((a, b) => a.order - b.order)[0];
      selectCategory(first.id);
      return;
    }
    // If the active category's monitoring rules were edited, refresh the
    // file list immediately (silently, so the manual refresh button stays
    // usable). The backend watcher follows the new rules on its own.
    if (editingCategory && editingCategory.id === activeCategoryId) {
      refreshFiles();
    }
  };

  const handleDeleteCategory = async (id: string) => {
    try {
      const cats = await invoke<Category[]>('delete_category', { id });
      const wasActive = config?.session.activeCategoryId === id;
      const newActiveId = wasActive
        ? cats[0]?.id ?? null
        : config?.session.activeCategoryId ?? null;
      setConfig((prev) =>
        prev
          ? { ...prev, categories: cats, session: { ...prev.session, activeCategoryId: newActiveId } }
          : prev,
      );
      if (wasActive && config) {
        await invoke('save_session', {
          session: { ...config.session, activeCategoryId: newActiveId },
        });
      }
      showToast(t('toast.categoryDeleted'), 'success');
    } catch {
      showToast(t('toast.categoryDeleteFailed'), 'error');
    }
  };

  const handleReorder = async (ids: string[]) => {
    const cats = await invoke<Category[]>('reorder_categories', { ids });
    setConfig((prev) => (prev ? { ...prev, categories: cats } : prev));
  };

  // ---- Settings operations ----
  const handleSaveSettings = async (newSettings: Settings) => {
    try {
      const saved = await invoke<Settings>('save_settings', { settings: newSettings });
      setSettings(saved);
      if (saved.language !== lang) {
        setLang(saved.language);
      }
    } catch {
      showToast(t('toast.saveFailed'), 'error');
    }
  };

  const handleToggleColumn = (key: string) => {
    if (!settings) return;
    const cols = settings.columns.includes(key)
      ? settings.columns.filter((c) => c !== key)
      : [...settings.columns, key];
    if (cols.length === 0) return;
    handleSaveSettings({ ...settings, columns: cols });
  };

  const handleSearchChange = (text: string) => {
    setConfig((prev) =>
      prev ? { ...prev, session: { ...prev.session, searchText: text } } : prev,
    );
    if (searchTimer.current) clearTimeout(searchTimer.current);
    searchTimer.current = setTimeout(() => {
      if (config) {
        invoke('save_session', { session: { ...config.session, searchText: text } });
      }
    }, 500);
  };

  const handleClearSearch = () => {
    setCategorySearch('');
    handleSearchChange('');
  };

  // Checks the disk fully before reloading, unlike the automatic reloads,
  // which trust the index.
  const handleManualRefresh = useCallback(() => {
    if (!activeCategoryId || inFlight) return;
    setInFlight(true);
    invoke('rescan_category', { categoryId: activeCategoryId })
      .catch(() => {})
      .finally(() => {
        setInFlight(false);
        refreshFiles();
      });
  }, [activeCategoryId, inFlight, refreshFiles]);

  // ---- Close / Quit handlers ----
  const handleCloseConfirm = async (action: 'close' | 'minimize', remember: boolean) => {
    setShowCloseDialog(false);
    await invoke('save_window_state');
    if (remember && settings) {
      await handleSaveSettings({ ...settings, closeAction: action });
    }
    if (action === 'close') {
      await invoke('quit_app');
    } else {
      await invoke('hide_main_window');
    }
  };

  const handleQuitConfirm = async () => {
    setShowQuitConfirm(false);
    await invoke('save_window_state');
    await invoke('quit_app');
  };

  const handleDeleteConfirm = async () => {
    if (deleteTarget) {
      await handleDeleteCategory(deleteTarget);
    }
    setDeleteTarget(null);
  };

  const handleFileContextMenu = useCallback(
    (x: number, y: number, file: FileEntry) => setContextMenu({ x, y, file }),
    [],
  );

  // ---- Toolbar items ----
  const toolbarItems: ToolbarItem[] = [
    {
      id: 'newCategory',
      icon: <Plus size={16} />,
      tooltip: t('toolbar.newCategory'),
      onClick: () => openNewCategory(),
    },
  ];

  // ---- Render ----
  if (showSplash || !config || !settings) {
    return <SplashScreen />;
  }

  return (
    <div className="flex flex-col h-screen bg-surface text-ink">
      <TitleBar onOpenSettings={() => setShowSettings(true)} />

      <div className="flex flex-1 min-h-0">
        {/* Sidebar */}
        <div className="w-60 shrink-0 flex flex-col bg-surface-secondary border-r border-divider">
          <CategoryToolbar items={toolbarItems} />
          <div ref={categoryDropRef} className="relative flex-1 flex flex-col min-h-0">
            <CategoryList
              categories={categories}
              activeCategoryId={activeCategoryId}
              searchText={categorySearch}
              onSearchChange={setCategorySearch}
              onClearSearch={handleClearSearch}
              onSelect={selectCategory}
              onReorder={handleReorder}
              onEdit={(cat) => {
                setEditingCategory(cat);
                setCategoryDraft(null);
                setShowCategoryEdit(true);
              }}
              onDelete={(id) => setDeleteTarget(id)}
            />
            <CategoryDropOverlay state={categoryDrop} />
          </div>
        </div>

        {/* Main content */}
        <div className="flex-1 flex flex-col min-h-0">
          {activeCategoryId ? (
            <FileTable
              categoryId={activeCategoryId}
              reloadToken={reloadToken}
              columns={columns}
              lang={lang}
              searchText={searchText}
              inFlight={inFlight}
              watchActive={watchActive}
              onSearchChange={handleSearchChange}
              onClearSearch={handleClearSearch}
              onRefresh={handleManualRefresh}
              onReload={refreshFiles}
              onToggleColumn={handleToggleColumn}
              onContextMenu={handleFileContextMenu}
              searchInputRef={fileSearchRef}
            />
          ) : (
            <EmptyGuide onCreate={() => openNewCategory()} />
          )}
        </div>
      </div>

      {/* Dialogs */}
      <CategoryEditDialog
        open={showCategoryEdit}
        category={editingCategory}
        draft={categoryDraft}
        onClose={() => setShowCategoryEdit(false)}
        onSaved={handleCategorySaved}
      />
      <SettingsDialog
        open={showSettings}
        settings={settings}
        initialPage="appearance"
        onClose={() => setShowSettings(false)}
        onSave={handleSaveSettings}
        onCheckUpdate={noopCheckUpdate}
      />
      <CloseDialog
        open={showCloseDialog}
        onClose={() => setShowCloseDialog(false)}
        onConfirm={handleCloseConfirm}
      />
      <ConfirmDialog
        open={showQuitConfirm}
        title={t('quit.title')}
        message={t('quit.message')}
        confirmText={t('quit.confirm')}
        onConfirm={handleQuitConfirm}
        onCancel={() => setShowQuitConfirm(false)}
      />
      <ConfirmDialog
        open={!!deleteTarget}
        title={t('confirm.delete.title')}
        message={t('confirm.delete.messageWith', { name: deleteCategoryName })}
        confirmText={t('confirm.delete.confirm')}
        danger
        onConfirm={handleDeleteConfirm}
        onCancel={() => setDeleteTarget(null)}
      />

      {/* Floating menus */}
      {contextMenu && (
        <ContextMenu
          x={contextMenu.x}
          y={contextMenu.y}
          file={contextMenu.file}
          onClose={() => setContextMenu(null)}
        />
      )}
    </div>
  );
}