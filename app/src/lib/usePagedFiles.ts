import { useEffect, useRef, useState } from 'react';
import { invoke } from '@tauri-apps/api/core';
import type { ColumnKey, FileEntry, FilePage } from '../types';

/** Rows per backend request. */
export const PAGE_SIZE = 200;

interface PagedFilesOptions {
  categoryId: string;
  search: string;
  sortCol: ColumnKey;
  sortDesc: boolean;
  /** Bumped to refetch the visible rows (file changes, manual refresh). */
  reloadToken: number;
  /** Visible row range, end exclusive (from `useVirtualRows`). */
  start: number;
  end: number;
  /** Asks for a reload; called when the backend answered from an older result. */
  onRetry: () => void;
}

interface Page {
  rows: FileEntry[];
  /** `reloadToken` it was fetched under; older pages are refetched when visible. */
  token: number;
}

interface ViewState {
  key: string;
  /** Rows the whole query matches; null until the first page arrives. */
  total: number | null;
  pages: Map<number, Page>;
}

/**
 * Fetches a category's files page by page for the rows on screen. Filtering,
 * sorting and counting happen in the backend; a new category, search or sort
 * order starts a new view, while a reload refetches the visible pages and
 * keeps showing the old rows until they arrive.
 */
export function usePagedFiles({
  categoryId,
  search,
  sortCol,
  sortDesc,
  reloadToken,
  start,
  end,
  onRetry,
}: PagedFilesOptions) {
  const key = JSON.stringify([categoryId, search, sortCol, sortDesc]);
  const [view, setView] = useState<ViewState>({ key, total: null, pages: new Map() });
  const keyRef = useRef(key);
  const inFlight = useRef(new Set<string>());
  const retryTimer = useRef<ReturnType<typeof setTimeout> | undefined>(undefined);
  const onRetryRef = useRef(onRetry);
  onRetryRef.current = onRetry;
  useEffect(() => () => clearTimeout(retryTimer.current), []);

  // A new view starts empty. Done during render so the old view's rows are
  // never shown under the new key.
  let current = view;
  if (view.key !== key) {
    current = { key, total: null, pages: new Map() };
    setView(current);
  }
  keyRef.current = key;

  const tokenRef = useRef(reloadToken);
  useEffect(() => {
    if (tokenRef.current === reloadToken) return;
    tokenRef.current = reloadToken;
    // Off-screen pages are dropped rather than refetched.
    const first = Math.floor(start / PAGE_SIZE);
    const last = Math.floor(Math.max(start, end - 1) / PAGE_SIZE);
    setView((v) => {
      const pages = new Map([...v.pages].filter(([p]) => p >= first && p <= last));
      return { ...v, pages };
    });
    // Only `reloadToken` should trigger this.
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [reloadToken]);

  useEffect(() => {
    const total = current.total;
    const first = Math.floor(start / PAGE_SIZE);
    const last =
      total === null ? 0 : Math.floor(Math.max(start, Math.min(end, total) - 1) / PAGE_SIZE);
    for (let p = first; p <= Math.max(first, last); p++) {
      if (total !== null && p * PAGE_SIZE >= total && p > 0) break;
      const have = current.pages.get(p);
      if (have && have.token === reloadToken) continue;
      const flightKey = `${key}|${reloadToken}|${p}`;
      if (inFlight.current.has(flightKey)) continue;
      inFlight.current.add(flightKey);
      invoke<FilePage>('query_files', {
        query: {
          categoryId,
          search,
          sortCol,
          sortDesc,
          offset: p * PAGE_SIZE,
          limit: PAGE_SIZE,
          reload: reloadToken,
        },
      })
        .then((page) => {
          if (keyRef.current !== key) return;
          if (page.retryAfterMs !== undefined && retryTimer.current === undefined) {
            retryTimer.current = setTimeout(() => {
              retryTimer.current = undefined;
              onRetryRef.current();
            }, page.retryAfterMs);
          }
          setView((v) => {
            if (v.key !== key) return v;
            const prev = v.pages.get(p);
            if (prev && prev.token > reloadToken) return v;
            const pages = new Map(v.pages);
            pages.set(p, { rows: page.rows, token: reloadToken });
            return { ...v, total: page.total, pages };
          });
        })
        .catch(() => {
          if (keyRef.current !== key) return;
          setView((v) => (v.key === key && v.total === null ? { ...v, total: 0 } : v));
        })
        .finally(() => inFlight.current.delete(flightKey));
    }
  }, [key, categoryId, search, sortCol, sortDesc, reloadToken, start, end, current]);

  const rowAt = (i: number): FileEntry | undefined =>
    current.pages.get(Math.floor(i / PAGE_SIZE))?.rows[i % PAGE_SIZE];

  return { total: current.total, rowAt, loading: current.total === null };
}
