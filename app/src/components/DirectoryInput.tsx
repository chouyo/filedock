import { useState } from 'react';
import { useI18n } from '../i18n/useI18n';
import { cn } from '../lib/utils';
import { useSafeHover } from '../lib/useSafeHover';

interface DirectoryInputProps {
  value: string;
  onChange: (path: string) => void;
  onPick: () => void;
}

export function DirectoryInput({ value, onChange, onPick }: DirectoryInputProps) {
  const { t } = useI18n();
  const [error, setError] = useState<string | null>(null);
  const pickHover = useSafeHover();

  const validate = (path: string) => {
    if (!path) {
      setError(null);
      return;
    }
    const isAbsolute =
      path.startsWith('/') ||
      /^[A-Za-z]:[\\/]/.test(path) ||
      path.startsWith('\\\\');
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
          autoComplete="off"
          autoCorrect="off"
          autoCapitalize="off"
          spellCheck={false}
          className={cn(
            'flex-1 px-3 py-1.5 rounded-md border text-sm',
            'bg-surface-input border-divider text-ink',
            'focus:outline-none focus:border-accent',
            error && 'border-danger',
          )}
        />
        <button
          onClick={() => {
            pickHover.clearHover();
            onPick();
          }}
          {...pickHover.hoverProps}
          className={cn(
            'px-3 py-1.5 text-sm rounded-md border border-divider whitespace-nowrap text-ink transition',
            pickHover.isHovered && 'bg-hover-bg',
          )}
        >
          {t('category.dir.pick')}
        </button>
      </div>
      {error && <span className="text-xs text-danger">{error}</span>}
    </div>
  );
}
