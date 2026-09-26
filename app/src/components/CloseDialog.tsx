import { useState } from 'react';
import { Modal } from './Modal';
import { useI18n } from '../i18n/useI18n';
import { useSafeHover } from '../lib/useSafeHover';
import { cn } from '../lib/utils';

interface CloseDialogProps {
  open: boolean;
  onClose: () => void;
  onConfirm: (action: 'close' | 'minimize', remember: boolean) => void;
}

export function CloseDialog({ open, onClose, onConfirm }: CloseDialogProps) {
  const { t } = useI18n();
  const [remember, setRemember] = useState(false);
  const cancelHover = useSafeHover();
  const minimizeHover = useSafeHover();
  const closeHover = useSafeHover();

  const handleAction = (action: 'close' | 'minimize') => {
    onConfirm(action, remember);
    setRemember(false);
  };

  return (
    <Modal open={open} onClose={onClose} title={t('close.title')} width="420px">
      <div className="flex flex-col gap-4">
        <p className="text-sm text-ink">{t('close.message')}</p>
        <label className="flex items-center gap-2 text-sm text-ink cursor-pointer">
          <input
            type="checkbox"
            checked={remember}
            onChange={(e) => setRemember(e.target.checked)}
          />
          {t('close.remember')}
        </label>
        <div className="flex items-center justify-end gap-2">
          <button
            onClick={onClose}
            {...cancelHover.hoverProps}
            className={cn(
              'px-3 py-1.5 text-sm rounded-md border border-divider text-ink transition',
              cancelHover.isHovered && 'bg-hover-bg',
            )}
          >
            {t('close.cancel')}
          </button>
          <button
            onClick={() => {
              minimizeHover.clearHover();
              handleAction('minimize');
            }}
            {...minimizeHover.hoverProps}
            className={cn(
              'px-3 py-1.5 text-sm rounded-md border border-divider text-ink transition',
              minimizeHover.isHovered && 'bg-hover-bg',
            )}
          >
            {t('close.minimizeToTray')}
          </button>
          <button
            onClick={() => {
              closeHover.clearHover();
              handleAction('close');
            }}
            {...closeHover.hoverProps}
            className={cn(
              'px-3 py-1.5 text-sm rounded-md bg-accent text-white transition',
              closeHover.isHovered && 'opacity-90',
            )}
          >
            {t('close.closeWindow')}
          </button>
        </div>
      </div>
    </Modal>
  );
}
