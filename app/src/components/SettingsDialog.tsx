import { useCallback, useEffect, useRef, useState } from 'react';
import { invoke } from '@tauri-apps/api/core';
import { FolderOpen } from 'lucide-react';
import { Modal } from './Modal';
import { useI18n } from '../i18n/useI18n';
import { cn } from '../lib/utils';
import { FEATURE_FLAGS } from '../lib/config';
import type { Settings, Language, CloseAction, Theme } from '../types';

interface SettingsDialogProps {
  open: boolean;
  settings: Settings;
  onClose: () => void;
  onSave: (settings: Settings) => void;
  initialPage?: 'appearance' | 'system' | 'monitor' | 'about';
  onCheckUpdate: () => void;
}

type SettingsPage = 'appearance' | 'system' | 'monitor' | 'about';

interface OptionItem<T extends string> {
  value: T;
  labelKey: string;
}

function SettingsCard({
  label,
  description,
  children,
}: {
  label: string;
  description?: string;
  children: React.ReactNode;
}) {
  return (
    <div className="flex flex-col gap-2 px-4 py-3 rounded-lg bg-surface-card mb-2">
      <div className="flex flex-col">
        <span className="text-sm font-medium text-ink">{label}</span>
        {description && <span className="text-xs text-ink-secondary mt-0.5">{description}</span>}
      </div>
      <div className="flex items-center gap-2">{children}</div>
    </div>
  );
}

function DropdownSelect({
  open,
  onToggle,
  value,
  options,
  onSelect,
  labels,
}: {
  open: boolean;
  onToggle: () => void;
  value: string;
  options: OptionItem<string>[];
  onSelect: (val: string) => void;
  labels: (key: string) => string;
}) {
  const current = options.find((o) => o.value === value);
  const triggerRef = useRef<HTMLButtonElement>(null);
  const listRef = useRef<HTMLDivElement>(null);
  const onToggleRef = useRef(onToggle);
  onToggleRef.current = onToggle;

  const optionButtons = () =>
    Array.from(listRef.current?.querySelectorAll<HTMLButtonElement>('[role="option"]') ?? []);

  // While open: focus the selected option, and let Escape close only the
  // dropdown (preventDefault tells the Modal not to close as well).
  useEffect(() => {
    if (!open) return;
    const buttons = optionButtons();
    (buttons.find((b) => b.getAttribute('aria-selected') === 'true') ?? buttons[0])?.focus();
    const onKey = (e: KeyboardEvent) => {
      if (e.key !== 'Escape') return;
      e.preventDefault();
      onToggleRef.current();
      triggerRef.current?.focus();
    };
    window.addEventListener('keydown', onKey, true);
    return () => window.removeEventListener('keydown', onKey, true);
  }, [open]);

  const handleListKeyDown = (e: React.KeyboardEvent) => {
    const buttons = optionButtons();
    const i = buttons.indexOf(document.activeElement as HTMLButtonElement);
    let next: number | null = null;
    if (e.key === 'ArrowDown') next = (i + 1) % buttons.length;
    else if (e.key === 'ArrowUp') next = (i - 1 + buttons.length) % buttons.length;
    else if (e.key === 'Home') next = 0;
    else if (e.key === 'End') next = buttons.length - 1;
    else if (e.key === 'Tab') {
      // Close and continue tabbing from the trigger, like a native select.
      onToggle();
      triggerRef.current?.focus();
      return;
    }
    if (next === null) return;
    e.preventDefault();
    buttons[next]?.focus();
  };

  return (
    <div className="relative">
      <button
        ref={triggerRef}
        onClick={(e) => {
          e.stopPropagation();
          onToggle();
        }}
        onKeyDown={(e) => {
          if (!open && (e.key === 'ArrowDown' || e.key === 'ArrowUp')) {
            e.preventDefault();
            onToggle();
          }
        }}
        aria-haspopup="listbox"
        aria-expanded={open}
        className="flex items-center gap-1.5 px-3 py-1.5 text-sm rounded-md border border-divider bg-surface-input text-ink hover:bg-hover-bg transition min-w-[120px] justify-between"
      >
        <span>{current ? labels(current.labelKey) : ''}</span>
        <span className="text-ink-secondary text-xs">▾</span>
      </button>
      {open && (
        <>
          <div className="fixed inset-0 z-40" onClick={(e) => { e.stopPropagation(); onToggle(); }} />
          <div
            ref={listRef}
            role="listbox"
            onKeyDown={handleListKeyDown}
            className="absolute right-0 top-full mt-1 z-50 bg-surface-elevated border border-divider rounded-lg shadow-lg py-1 min-w-[140px]"
          >
            {options.map((opt) => (
              <button
                key={opt.value}
                role="option"
                aria-selected={value === opt.value}
                onClick={(e) => {
                  e.stopPropagation();
                  onSelect(opt.value);
                  triggerRef.current?.focus();
                }}
                className={cn(
                  'w-full text-left px-3 py-1.5 text-sm hover:bg-hover-bg focus:bg-hover-bg focus:outline-none transition',
                  value === opt.value ? 'text-accent' : 'text-ink',
                )}
              >
                {labels(opt.labelKey)}
              </button>
            ))}
          </div>
        </>
      )}
    </div>
  );
}

function Toggle({ checked, onChange }: { checked: boolean; onChange: (v: boolean) => void }) {
  return (
    <button
      onClick={(e) => { e.stopPropagation(); onChange(!checked); }}
      className={cn(
        'relative w-10 h-5 rounded-full transition shrink-0',
        checked ? 'bg-accent' : 'bg-divider',
      )}
      role="switch"
      aria-checked={checked}
    >
      <span
        className={cn(
          'absolute top-0.5 left-0.5 w-4 h-4 rounded-full bg-white transition-transform',
          checked && 'translate-x-5',
        )}
      />
    </button>
  );
}

export function SettingsDialog({
  open,
  settings,
  onClose,
  onSave,
  initialPage = 'appearance',
  onCheckUpdate,
}: SettingsDialogProps) {
  const { t } = useI18n();
  const [page, setPage] = useState<SettingsPage>(initialPage);
  const [local, setLocal] = useState<Settings>(settings);
  const [configDir, setConfigDir] = useState('');
  const [dropdownOpen, setDropdownOpen] = useState<'language' | 'theme' | 'closeAction' | 'watchDebounce' | null>(null);
  const initializedRef = useRef(false);

  useEffect(() => {
    if (open && !initializedRef.current) {
      initializedRef.current = true;
      setPage(initialPage);
      invoke<string>('get_config_dir').then(setConfigDir);
    }
    if (!open) {
      setDropdownOpen(null);
    }
    setLocal(settings);
  }, [open, settings, initialPage]);

  const update = (patch: Partial<Settings>) => {
    const next = { ...local, ...patch };
    setLocal(next);
    onSave(next);
  };

  const handleOpenConfigDir = async () => {
    await invoke('open_config_directory');
  };

  const closeAllDropdowns = useCallback(() => setDropdownOpen(null), []);

  const navItems: { key: SettingsPage; label: string }[] = [
    { key: 'appearance', label: t('settings.appearance') },
    { key: 'system', label: t('settings.system') },
    { key: 'monitor', label: t('settings.monitor') },
    { key: 'about', label: t('settings.about') },
  ];

  const languageOptions: OptionItem<Language>[] = [
    { value: 'en', labelKey: 'settings.language.en' },
    { value: 'zh-CN', labelKey: 'settings.language.zh' },
  ];

  const themeOptions: OptionItem<Theme>[] = [
    { value: 'system', labelKey: 'settings.theme.system' },
    { value: 'light', labelKey: 'settings.theme.light' },
    { value: 'dark', labelKey: 'settings.theme.dark' },
  ];

  const closeOptions: OptionItem<CloseAction>[] = [
    { value: 'ask', labelKey: 'settings.closeBehavior.ask' },
    { value: 'close', labelKey: 'settings.closeBehavior.close' },
    { value: 'minimize', labelKey: 'settings.closeBehavior.minimize' },
  ];

  const debounceOptions: OptionItem<string>[] = [
    { value: '500', labelKey: 'settings.watch.debounce.500' },
    { value: '1000', labelKey: 'settings.watch.debounce.1000' },
    { value: '2000', labelKey: 'settings.watch.debounce.2000' },
    { value: '3000', labelKey: 'settings.watch.debounce.3000' },
  ];

  return (
    <Modal open={open} onClose={onClose} title={t('settings.title')} width="640px">
      <div className="flex gap-4 min-h-[280px]" onClick={closeAllDropdowns}>
        <div className="w-32 shrink-0 flex flex-col gap-1">
          {navItems.map((item) => (
            <button
              key={item.key}
              onClick={() => setPage(item.key)}
              className={cn(
                'text-left px-3 py-2 rounded-md text-sm transition',
                page === item.key
                  ? 'bg-accent-bg text-accent font-medium'
                  : 'text-ink hover:bg-hover-bg',
              )}
            >
              {item.label}
            </button>
          ))}
        </div>

        <div className="flex-1 min-w-0">
          {page === 'appearance' && (
            <div className="flex flex-col">
              <SettingsCard label={t('settings.language')}>
                <DropdownSelect
                  open={dropdownOpen === 'language'}
                  onToggle={() =>
                    setDropdownOpen((prev) => (prev === 'language' ? null : 'language'))
                  }
                  value={local.language}
                  options={languageOptions}
                  onSelect={(val) => { setDropdownOpen(null); update({ language: val as Language }); }}
                  labels={t}
                />
              </SettingsCard>
              <SettingsCard label={t('settings.theme')}>
                <DropdownSelect
                  open={dropdownOpen === 'theme'}
                  onToggle={() =>
                    setDropdownOpen((prev) => (prev === 'theme' ? null : 'theme'))
                  }
                  value={local.theme}
                  options={themeOptions}
                  onSelect={(val) => { setDropdownOpen(null); update({ theme: val as Theme }); }}
                  labels={t}
                />
              </SettingsCard>
            </div>
          )}

          {page === 'system' && (
            <div className="flex flex-col">
              <SettingsCard label={t('settings.closeBehavior')}>
                <DropdownSelect
                  open={dropdownOpen === 'closeAction'}
                  onToggle={() =>
                    setDropdownOpen((prev) => (prev === 'closeAction' ? null : 'closeAction'))
                  }
                  value={local.closeAction}
                  options={closeOptions}
                  onSelect={(val) => { setDropdownOpen(null); update({ closeAction: val as CloseAction }); }}
                  labels={t}
                />
              </SettingsCard>
            </div>
          )}

          {page === 'monitor' && (
            <div className="flex flex-col">
              <SettingsCard
                label={t('settings.watch.enable')}
                description={t('settings.watch.enableDesc')}
              >
                <Toggle
                  checked={local.autoRefreshEnabled}
                  onChange={(v) => update({ autoRefreshEnabled: v })}
                />
              </SettingsCard>
              <SettingsCard label={t('settings.watch.debounce')}>
                <DropdownSelect
                  open={dropdownOpen === 'watchDebounce'}
                  onToggle={() =>
                    setDropdownOpen((prev) => (prev === 'watchDebounce' ? null : 'watchDebounce'))
                  }
                  value={String(local.watchDebounceMs)}
                  options={debounceOptions}
                  onSelect={(val) => {
                    setDropdownOpen(null);
                    update({ watchDebounceMs: Number(val) });
                  }}
                  labels={t}
                />
              </SettingsCard>
            </div>
          )}

          {page === 'about' && (
            <div className="flex flex-col">
              <SettingsCard label={t('settings.version')}>
                <span className="text-sm text-ink-secondary">1.0.0</span>
              </SettingsCard>
              <SettingsCard label={t('settings.configDir')}>
                <div className="flex items-center gap-2">
                  <span className="text-xs text-ink-secondary max-w-[220px] truncate" title={configDir}>
                    {configDir}
                  </span>
                  <button
                    onClick={handleOpenConfigDir}
                    className="p-1.5 rounded hover:bg-hover-bg text-ink-secondary hover:text-ink transition"
                    title={t('settings.openConfigDir')}
                  >
                    <FolderOpen size={14} />
                  </button>
                </div>
              </SettingsCard>
              {FEATURE_FLAGS.updater.enabled && (
                <SettingsCard label={t('settings.checkUpdate')}>
                  <button
                    onClick={onCheckUpdate}
                    className="px-3 py-1.5 text-sm rounded-md bg-accent text-white hover:opacity-90"
                  >
                    {t('settings.checkUpdate')}
                  </button>
                </SettingsCard>
              )}
            </div>
          )}
        </div>
      </div>
    </Modal>
  );
}