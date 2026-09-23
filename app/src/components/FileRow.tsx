import { useCallback } from 'react';
import { startDrag } from '@crabnebula/tauri-plugin-drag';
import type { FileEntry, ColumnKey, Language } from '../types';
import { getColumnDef, renderCell } from '../lib/columns';
import { cn } from '../lib/utils';

const DRAG_ICON =
  'data:image/png;base64,iVBORw0KGgoAAAANSUhEUgAAAAEAAAABCAYAAAAfFcSJAAAADUlEQVR42mNk+M9QDwADhgGAWjR9awAAAABJRU5ErkJggg==';

interface FileRowProps {
  file: FileEntry;
  columns: ColumnKey[];
  lang: Language;
  onContextMenu: (x: number, y: number, file: FileEntry) => void;
}

export function FileRow({ file, columns, lang, onContextMenu }: FileRowProps) {
  const handleDragStart = useCallback(
    async (e: React.DragEvent) => {
      e.preventDefault();
      try {
        await startDrag({
          item: [file.path],
          icon: DRAG_ICON,
        });
      } catch (err) {
        console.error('Drag failed:', err);
      }
    },
    [file.path],
  );

  return (
    <tr
      draggable
      onDragStart={handleDragStart}
      onContextMenu={(e) => {
        e.preventDefault();
        e.stopPropagation();
        onContextMenu(e.clientX, e.clientY, file);
      }}
      className="border-b border-divider hover:bg-hover-bg transition cursor-grab text-sm"
    >
      {columns.map((key) => {
        const def = getColumnDef(key);
        if (!def) return null;
        return (
          <td
            key={key}
            className={cn(
              'px-3 py-1.5 whitespace-nowrap overflow-hidden text-ellipsis max-w-[300px]',
              def.align === 'right' && 'text-right',
              def.align === 'center' && 'text-center',
            )}
          >
            {renderCell(file, def, lang)}
          </td>
        );
      })}
    </tr>
  );
}
