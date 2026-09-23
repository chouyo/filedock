import { Inbox } from 'lucide-react';
import { useI18n } from '../i18n/useI18n';

export function EmptyGuide({ onCreate }: { onCreate: () => void }) {
  const { t } = useI18n();
  return (
    <div className="flex-1 flex flex-col items-center justify-center gap-4 text-center px-6">
      <Inbox size={56} className="text-ink-secondary opacity-30" />
      <div className="text-lg font-medium text-ink">{t('empty.title')}</div>
      <div className="text-sm text-ink-secondary">{t('empty.subtitle')}</div>
      <button
        onClick={onCreate}
        className="mt-2 px-4 py-2 rounded-lg bg-accent text-white hover:opacity-90 transition text-sm font-medium"
      >
        + {t('empty.action')}
      </button>
    </div>
  );
}
