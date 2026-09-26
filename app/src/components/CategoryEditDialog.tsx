import { useEffect, useRef, useState } from 'react';
import { invoke } from '@tauri-apps/api/core';
import { open as openDialog } from '@tauri-apps/plugin-dialog';
import { Plus, Trash2 } from 'lucide-react';
import { Modal } from './Modal';
import { DirectoryInput } from './DirectoryInput';
import { useI18n } from '../i18n/useI18n';
import { useToast } from './Toast';
import type { Category, CategoryDraft, TargetRule } from '../types';

interface CategoryEditDialogProps {
  open: boolean;
  category: Category | null;
  /** Prefill for a new category (ignored when editing). */
  draft?: CategoryDraft | null;
  onClose: () => void;
  onSaved: () => void;
}

function emptyTarget(): TargetRule {
  return { id: '', dir: '', matchType: 'glob', pattern: '', recursive: false };
}

export function CategoryEditDialog({
  open,
  category,
  draft,
  onClose,
  onSaved,
}: CategoryEditDialogProps) {
  const { t } = useI18n();
  const { showToast } = useToast();
  const [name, setName] = useState('');
  const [targets, setTargets] = useState<TargetRule[]>([emptyTarget()]);
  const nameInputRef = useRef<HTMLInputElement>(null);

  useEffect(() => {
    if (!open) return;
    const source = category ?? draft ?? null;
    setName(source?.name ?? '');
    setTargets(
      source && source.targets.length > 0 ? source.targets.map((t) => ({ ...t })) : [emptyTarget()],
    );
    // A prefilled name is a suggestion; select it so typing replaces it.
    if (!category && draft) {
      requestAnimationFrame(() => {
        nameInputRef.current?.focus();
        nameInputRef.current?.select();
      });
    }
  }, [open, category, draft]);

  const updateTarget = (index: number, patch: Partial<TargetRule>) => {
    setTargets((prev) => prev.map((t, i) => (i === index ? { ...t, ...patch } : t)));
  };

  const addTarget = () => setTargets((prev) => [...prev, emptyTarget()]);

  const removeTarget = (index: number) =>
    setTargets((prev) => prev.filter((_, i) => i !== index));

  const pickDirectory = async (index: number) => {
    const currentPath = targets[index]?.dir.trim() ?? '';
    const hasValidDirectory = currentPath
      ? await invoke<boolean>('is_valid_directory', { path: currentPath })
      : false;
    const selected = await openDialog({
      directory: true,
      multiple: false,
      title: t('category.dir.label'),
      ...(hasValidDirectory ? { defaultPath: currentPath } : {}),
    });
    if (typeof selected === 'string') {
      updateTarget(index, { dir: selected });
    }
  };

  const handleSave = async () => {
    if (!name.trim()) return;
    const validTargets = targets.filter((t) => t.dir.trim());
    const categoryToSave: Category = {
      id: category?.id ?? '',
      name: name.trim(),
      order: category?.order ?? 0,
      targets: validTargets,
      createdAt: category?.createdAt ?? 0,
      updatedAt: category?.updatedAt ?? 0,
    };
    try {
      await invoke('save_category', { category: categoryToSave });
      showToast(t('toast.categorySaved'), 'success');
      onSaved();
      onClose();
    } catch {
      showToast(t('toast.saveFailed'), 'error');
    }
  };

  return (
    <Modal
      open={open}
      onClose={onClose}
      title={category ? t('category.edit.title') : t('category.create.title')}
      width="560px"
      footer={
        <>
          <button
            onClick={onClose}
            className="px-4 py-1.5 text-sm rounded-md border border-divider hover:bg-hover-bg text-ink"
          >
            {t('category.cancel')}
          </button>
          <button
            onClick={handleSave}
            className="px-4 py-1.5 text-sm rounded-md bg-accent text-white hover:opacity-90"
          >
            {t('category.save')}
          </button>
        </>
      }
    >
      <div className="flex flex-col gap-4">
        <div className="flex flex-col gap-1.5">
          <label className="text-sm font-medium text-ink">{t('category.name')}</label>
          <input
            ref={nameInputRef}
            type="text"
            value={name}
            onChange={(e) => setName(e.target.value)}
            placeholder={t('category.name.placeholder')}
            autoComplete="off"
            autoCorrect="off"
            autoCapitalize="off"
            spellCheck={false}
            className="px-3 py-1.5 rounded-md border bg-surface-input border-divider text-sm text-ink focus:outline-none focus:border-accent"
          />
        </div>

        <div className="flex flex-col gap-2">
          <label className="text-sm font-medium text-ink">{t('category.directories')}</label>
          {targets.map((target, index) => (
            <div
              key={index}
              className="flex flex-col gap-2 p-3 rounded-lg border border-divider bg-surface-card"
            >
              <div className="flex items-center justify-between">
                <span className="text-xs text-ink-secondary">{t('category.directory')}</span>
                {targets.length > 1 && (
                  <button
                    onClick={() => removeTarget(index)}
                    className="p-1 rounded hover:bg-hover-bg text-ink-secondary hover:text-danger"
                  >
                    <Trash2 size={13} />
                  </button>
                )}
              </div>
              <DirectoryInput
                value={target.dir}
                onChange={(dir) => updateTarget(index, { dir })}
                onPick={() => pickDirectory(index)}
              />
              <div className="flex items-center gap-4">
                <label className="flex items-center gap-1.5 text-sm text-ink cursor-pointer">
                  <input
                    type="radio"
                    checked={target.matchType === 'glob'}
                    onChange={() => updateTarget(index, { matchType: 'glob' })}
                  />
                  {t('category.mode.glob')}
                </label>
                <label className="flex items-center gap-1.5 text-sm text-ink cursor-pointer">
                  <input
                    type="radio"
                    checked={target.matchType === 'regex'}
                    onChange={() => updateTarget(index, { matchType: 'regex' })}
                  />
                  {t('category.mode.regex')}
                </label>
              </div>
              <input
                type="text"
                value={target.pattern}
                onChange={(e) => updateTarget(index, { pattern: e.target.value })}
                placeholder={t('category.pattern.placeholder')}
                autoComplete="off"
                autoCorrect="off"
                autoCapitalize="off"
                spellCheck={false}
                className="px-3 py-1.5 rounded-md border bg-surface-input border-divider text-sm text-ink focus:outline-none focus:border-accent"
              />
              <label className="flex items-center gap-1.5 text-sm text-ink cursor-pointer">
                <input
                  type="checkbox"
                  checked={target.recursive}
                  onChange={(e) => updateTarget(index, { recursive: e.target.checked })}
                />
                {t('category.recursive')}
              </label>
            </div>
          ))}
          <button
            onClick={addTarget}
            className="flex items-center gap-1.5 px-3 py-1.5 text-sm rounded-md border border-dashed border-divider hover:bg-hover-bg text-ink-secondary hover:text-ink transition w-fit"
          >
            <Plus size={14} />
            {t('category.addDirectory')}
          </button>
        </div>
      </div>
    </Modal>
  );
}
