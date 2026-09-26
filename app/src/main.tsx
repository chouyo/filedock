import React from 'react';
import ReactDOM from 'react-dom/client';
import { I18nProvider } from './i18n/useI18n';
import { ToastProvider } from './components/Toast';
import { App } from './App';
import { installShortcutGuard } from './lib/shortcuts';

import '@fontsource/inter/400.css';
import '@fontsource/inter/500.css';
import '@fontsource/inter/600.css';
import '@fontsource/inter/700.css';
import '@fontsource/noto-sans-sc/400.css';
import '@fontsource/noto-sans-sc/500.css';
import '@fontsource/noto-sans-sc/700.css';
import './index.css';

installShortcutGuard();

ReactDOM.createRoot(document.getElementById('root')!).render(
  <React.StrictMode>
    <I18nProvider initialLang="en">
      <ToastProvider>
        <App />
      </ToastProvider>
    </I18nProvider>
  </React.StrictMode>,
);
