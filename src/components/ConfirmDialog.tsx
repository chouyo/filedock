import { Modal } from './Modal';
import { useI18n } from '../i18n/useI18n';

interface ConfirmDialogProps {
  open: boolean;
  title: string;
  message: string;
  confirmText?: string;
  cancelText?: string;
  danger?: boolean;
  onConfirm: () => void;
  onCancel: () => void;
}

export function ConfirmDialog({
  open,
  title,
  message,
  confirmText,
  cancelText,
  danger,
  onConfirm,
  onCancel,
}: ConfirmDialogProps) {
  const { t } = useI18n();

  return (
    <Modal open={open} onClose={onCancel} title={title} width="380px">
      <div className="flex flex-col gap-4">
        <p className="text-sm text-ink">{message}</p>
        <div className="flex items-center justify-end gap-2">
          <button
            onClick={onCancel}
            className="px-3 py-1.5 text-sm rounded-md border border-divider hover:bg-hover-bg text-ink"
          >
            {cancelText ?? t('common.cancel')}
          </button>
          <button
            onClick={onConfirm}
            className={
              danger
                ? 'px-3 py-1.5 text-sm rounded-md bg-danger text-white hover:opacity-90'
                : 'px-3 py-1.5 text-sm rounded-md bg-accent text-white hover:opacity-90'
            }
          >
            {confirmText ?? t('common.confirm')}
          </button>
        </div>
      </div>
    </Modal>
  );
}
