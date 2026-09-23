import { useState } from 'react';
import { useI18n } from '../i18n/useI18n';
import { cn } from '../lib/utils';

interface DirectoryInputProps {
  value: string;
  onChange: (path: string) => void;
  onPick: () => void;
}

export function DirectoryInput({ value, onChange, onPick }: DirectoryInputProps) {
  const { t } = useI18n();
  const [error, setError] = useState<string | null>(null);

  const validate = (path: string) => {
    if (!path) {
      setError(null);
      return;
    }
    const isAbsolute = /^[A-Za-z]:\\/.test(path) || path.startsWith('\\\\');
    if (!isAbsolute) {
      setError(t('category.dir.notAbsolute'));
      return;
    }
    setError(null);
  };

  return (
    <div className="flex flex-col gap-1">
      <div className="flex items-center gap-2">
        <input
          type="text"
          value={value}
          onChange={(e) => {
            onChange(e.target.value);
            validate(e.target.value);
          }}
          placeholder={t('category.dir.placeholder')}
          className={cn(
            'flex-1 px-3 py-1.5 rounded-md border text-sm',
            'bg-surface-input border-divider text-ink',
            'focus:outline-none focus:border-accent',
            error && 'border-danger',
          )}
        />
        <button
          onClick={onPick}
          className="px-3 py-1.5 text-sm rounded-md border border-divider hover:bg-hover-bg whitespace-nowrap text-ink"
        >
          {t('category.dir.pick')}
        </button>
      </div>
      {error && <span className="text-xs text-danger">{error}</span>}
    </div>
  );
}
