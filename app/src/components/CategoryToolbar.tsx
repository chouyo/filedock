import type { ReactNode } from 'react';
import { Tooltip } from './Tooltip';
import { cn } from '../lib/utils';

export interface ToolbarItem {
  id: string;
  icon: ReactNode;
  tooltip: string;
  onClick: (e: React.MouseEvent) => void;
  active?: boolean;
}

export function CategoryToolbar({ items }: { items: ToolbarItem[] }) {
  return (
    <div className="flex items-center gap-0.5 px-2 py-1.5 border-b border-divider shrink-0">
      {items.map((item) => (
        <Tooltip key={item.id} content={item.tooltip}>
          <button
            onClick={(e) => item.onClick(e)}
            aria-label={item.tooltip}
            className={cn(
              'p-1.5 rounded-md text-ink-secondary',
              'hover:bg-hover-bg hover:text-ink transition',
              item.active && 'bg-accent-bg text-accent',
            )}
          >
            {item.icon}
          </button>
        </Tooltip>
      ))}
    </div>
  );
}
