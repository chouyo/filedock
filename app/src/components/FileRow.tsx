import { memo, useCallback } from 'react';
import { startDrag } from '@crabnebula/tauri-plugin-drag';
import type { FileEntry, Language } from '../types';
import { renderCell, type ColumnDef } from '../lib/columns';
import { cn } from '../lib/utils';
import { beginInternalDrag, endInternalDrag } from '../lib/internalDrag';

const DRAG_ICON =
  'data:image/png;base64,iVBORw0KGgoAAAANSUhEUgAAAAEAAAABCAYAAAAfFcSJAAAADUlEQVR42mNk+M9QDwADhgGAWjR9awAAAABJRU5ErkJggg==';

/**
 * Exact row height in px. The file table is virtualized on this value, so
 * rows must not grow: cells are single-line and the divider is drawn with an
 * inset shadow rather than a border.
 */
export const FILE_ROW_HEIGHT = 32;

interface FileRowProps {
  file: FileEntry;
  columns: ColumnDef[];
  lang: Language;
  /** 1-based row index within the table, header included (for aria-rowindex). */
  rowIndex: number;
  onContextMenu: (x: number, y: number, file: FileEntry) => void;
}

export const FileRow = memo(function FileRow({
  file,
  columns,
  lang,
  rowIndex,
  onContextMenu,
}: FileRowProps) {
  const handleDragStart = useCallback(
    async (e: React.DragEvent) => {
      e.preventDefault();
      // Flag the drag as ours so the sidebar drop zone ignores it. The flag
      // is cleared when the drag finishes; the drop listener also clears it
      // when the drag leaves or drops, in case no event arrives.
      beginInternalDrag();
      try {
        await startDrag(
          {
            item: [file.path],
            icon: DRAG_ICON,
          },
          () => endInternalDrag(),
        );
      } catch (err) {
        endInternalDrag();
        console.error('Drag failed:', err);
      }
    },
    [file.path],
  );

  return (
    <tr
      draggable
      aria-rowindex={rowIndex}
      onDragStart={handleDragStart}
      onContextMenu={(e) => {
        e.preventDefault();
        e.stopPropagation();
        onContextMenu(e.clientX, e.clientY, file);
      }}
      style={{ height: FILE_ROW_HEIGHT }}
      className="hover:bg-hover-bg transition cursor-grab text-sm"
    >
      {columns.map((def) => (
        <td
          key={def.key}
          className={cn(
            'px-3 py-0 whitespace-nowrap overflow-hidden text-ellipsis shadow-[inset_0_-1px_0_var(--border-color)]',
            def.align === 'right' && 'text-right',
            def.align === 'center' && 'text-center',
          )}
        >
          {renderCell(file, def, lang)}
        </td>
      ))}
      <td className="shadow-[inset_0_-1px_0_var(--border-color)]" />
    </tr>
  );
});
