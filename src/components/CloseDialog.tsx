import { useState } from 'react';
import { Modal } from './Modal';
import { useI18n } from '../i18n/useI18n';

interface CloseDialogProps {
  open: boolean;
  onClose: () => void;
  onConfirm: (action: 'close' | 'minimize', remember: boolean) => void;
}

export function CloseDialog({ open, onClose, onConfirm }: CloseDialogProps) {
  const { t } = useI18n();
  const [remember, setRemember] = useState(false);

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
            className="px-3 py-1.5 text-sm rounded-md border border-divider hover:bg-hover-bg text-ink"
          >
            {t('close.cancel')}
          </button>
          <button
            onClick={() => handleAction('minimize')}
            className="px-3 py-1.5 text-sm rounded-md border border-divider hover:bg-hover-bg text-ink"
          >
            {t('close.minimizeToTray')}
          </button>
          <button
            onClick={() => handleAction('close')}
            className="px-3 py-1.5 text-sm rounded-md bg-accent text-white hover:opacity-90"
          >
            {t('close.closeWindow')}
          </button>
        </div>
      </div>
    </Modal>
  );
}
