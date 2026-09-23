import dayjs from 'dayjs';
import type { Language } from '../types';

export function formatSize(bytes: number): string {
  if (!bytes) return '—';
  const units = ['B', 'KB', 'MB', 'GB', 'TB'];
  let size = bytes;
  let unitIndex = 0;
  while (size >= 1024 && unitIndex < units.length - 1) {
    size /= 1024;
    unitIndex++;
  }
  return `${size.toFixed(unitIndex === 0 ? 0 : 1)} ${units[unitIndex]}`;
}

export function formatTime(ms: number, lang: Language): string {
  if (!ms) return '—';
  const d = dayjs(ms);
  return lang === 'zh-CN'
    ? d.format('YYYY-MM-DD HH:mm')
    : d.format('MMM D, YYYY HH:mm');
}
