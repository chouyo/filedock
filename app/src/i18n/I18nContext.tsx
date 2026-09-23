import { createContext, useCallback, useContext, useMemo, useState } from 'react';
import type { ReactNode } from 'react';
import type { Language } from '../types';
import en from '../locales/en.json';
import zhCN from '../locales/zh-CN.json';

const RESOURCES: Record<Language, Record<string, string>> = {
  en: en,
  'zh-CN': zhCN,
};

interface I18nContextValue {
  lang: Language;
  setLang: (lang: Language) => void;
  t: (key: string, params?: Record<string, string | number>) => string;
}

const I18nContext = createContext<I18nContextValue | null>(null);

function interpolate(template: string, params?: Record<string, string | number>): string {
  if (!params) return template;
  return template.replace(/\{(\w+)\}/g, (_, k: string) =>
    params[k] !== undefined ? String(params[k]) : `{${k}}`,
  );
}

export function I18nProvider({
  initialLang = 'en',
  children,
}: {
  initialLang?: Language;
  children: ReactNode;
}) {
  const [lang, setLang] = useState<Language>(initialLang);

  const t = useCallback(
    (key: string, params?: Record<string, string | number>) => {
      const dict = RESOURCES[lang] ?? RESOURCES['en'];
      const fallback = RESOURCES['en'];
      const raw = dict[key] ?? fallback[key] ?? key;
      return interpolate(raw, params);
    },
    [lang],
  );

  const value = useMemo(() => ({ lang, setLang, t }), [lang, t]);

  return <I18nContext.Provider value={value}>{children}</I18nContext.Provider>;
}

export function useI18n() {
  const ctx = useContext(I18nContext);
  if (!ctx) throw new Error('useI18n must be used within I18nProvider');
  return ctx;
}
