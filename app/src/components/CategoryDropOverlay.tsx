import type { ReactNode } from 'react';
import { File, FileType, FolderPlus } from 'lucide-react';
import { useI18n } from '../i18n/useI18n';
import { cn } from '../lib/utils';
import { typePatternPreview } from '../lib/dropDraft';
import { zonesFor } from '../lib/useCategoryDrop';
import type { CategoryDropState, DropZone } from '../lib/useCategoryDrop';

/**
 * Drop target shown over the category list while files or folders are
 * dragged into the window. Zones carry `data-drop-zone` for hit testing in
 * `useCategoryDrop`; the overlay itself never takes pointer events (the OS
 * drag doesn't produce them anyway).
 */
export function CategoryDropOverlay({ state }: { state: CategoryDropState }) {
  const { t } = useI18n();
  const { summary } = state;
  if (!state.dragging || !summary) return null;

  if (!state.over) {
    return (
      <div className="absolute inset-0 z-30 p-2 bg-surface-secondary pointer-events-none animate-fade-in-fast">
        <div className="h-full flex flex-col items-center justify-center gap-2 rounded-lg border-2 border-dashed border-divider text-ink-secondary">
          <FolderPlus size={22} className="opacity-70" />
          <span className="text-xs text-center px-3">{t('drop.hint')}</span>
        </div>
      </div>
    );
  }

  const { dirs, files, extensions } = summary;
  const zones = zonesFor(summary);

  const zoneContent: Record<DropZone, { icon: ReactNode; title: string; detail: string[] }> = {
    dir: {
      icon: <FolderPlus size={22} />,
      title: t('drop.release'),
      detail: [t('drop.folderCount', { n: dirs.length })],
    },
    file: {
      icon: <File size={20} />,
      title: t('drop.byFile'),
      detail:
        files.length > 1
          ? [files[0]?.name ?? '', t('drop.moreFiles', { n: files.length })]
          : [files[0]?.name ?? ''],
    },
    type: {
      icon: <FileType size={20} />,
      title: t('drop.byType'),
      detail: [typePatternPreview(extensions)],
    },
  };

  return (
    <div className="absolute inset-0 z-30 p-2 flex flex-col gap-2 bg-surface-secondary pointer-events-none animate-fade-in-fast">
      {zones.map((zone) => {
        const active = state.zone === zone;
        const { icon, title, detail } = zoneContent[zone];
        return (
          <div
            key={zone}
            data-drop-zone={zone}
            className={cn(
              'flex-1 min-h-0 flex flex-col items-center justify-center gap-1.5 px-3 rounded-lg border-2 transition',
              active
                ? 'border-accent bg-accent/15 text-ink'
                : 'border-dashed border-divider text-ink-secondary opacity-60',
            )}
          >
            <span className={active ? 'text-accent' : undefined}>{icon}</span>
            <span className="text-sm font-medium">{title}</span>
            {detail.map((line, i) => (
              <span
                key={i}
                className="max-w-full truncate text-xs text-ink-secondary"
              >
                {line}
              </span>
            ))}
          </div>
        );
      })}
      {dirs.length > 0 && files.length > 0 && (
        <div className="shrink-0 px-1 text-xs text-center text-ink-secondary">
          {t('drop.alsoFolders', { n: dirs.length })}
        </div>
      )}
    </div>
  );
}
