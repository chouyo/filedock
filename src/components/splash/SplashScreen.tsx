import { useEffect, useState } from 'react';
import { useI18n } from '../../i18n/useI18n';
import ParticleText from './ParticleText';

const ACCENT = '#4F8CFF';

function readInk(): string {
  return (
    getComputedStyle(document.documentElement)
      .getPropertyValue('--text-primary')
      .trim() || '#1a1d23'
  );
}

function useInkColor(): string {
  const [ink, setInk] = useState(readInk);

  useEffect(() => {
    const el = document.documentElement;
    const update = () => setInk(readInk());
    const observer = new MutationObserver(update);
    observer.observe(el, { attributes: true, attributeFilter: ['class'] });
    const mq = window.matchMedia('(prefers-color-scheme: dark)');
    mq.addEventListener('change', update);
    return () => {
      observer.disconnect();
      mq.removeEventListener('change', update);
    };
  }, []);

  return ink;
}

export function SplashScreen() {
  const { t } = useI18n();
  const ink = useInkColor();

  return (
    <div className="fixed inset-0 flex flex-col items-center justify-center bg-surface animate-fade-in gap-8">
      <div className="w-full px-6" style={{ height: '35vh', minHeight: 240 }}>
        <ParticleText
          text="File Dock"
          particleSize={2}
          density={4}
          color={ink}
          highlightColor={ACCENT}
          scatter={180}
          gatherDuration={2000}
          stagger={420}
          pointerRepel={40}
          repelRadius={120}
          idleDrift={0.7}
          trigger="mount"
          fontSize="clamp(3rem, 12vw, 8rem)"
          fontWeight={800}
          fontFamily="inherit"
          glow
        />
      </div>
      <span className="text-sm text-ink-secondary animate-pulse">{t('app.starting')}</span>
    </div>
  );
}