import { useMemo, useRef, useState } from 'react';
import {
  DndContext,
  closestCenter,
  PointerSensor,
  useSensor,
  useSensors,
  type DragEndEvent,
} from '@dnd-kit/core';
import {
  SortableContext,
  verticalListSortingStrategy,
  useSortable,
  arrayMove,
} from '@dnd-kit/sortable';
import { CSS } from '@dnd-kit/utilities';
import { GripVertical, Pencil, Trash2 } from 'lucide-react';
import { useI18n } from '../i18n/useI18n';
import { cn } from '../lib/utils';
import { Tooltip } from './Tooltip';
import { SearchInput } from './SearchInput';
import type { Category } from '../types';

interface CategoryListProps {
  categories: Category[];
  activeCategoryId: string | null;
  searchText: string;
  onSearchChange: (text: string) => void;
  onClearSearch: () => void;
  onSelect: (id: string) => void;
  onReorder: (ids: string[]) => void;
  onEdit: (category: Category) => void;
  onDelete: (id: string) => void;
}

export function CategoryList({
  categories,
  activeCategoryId,
  searchText,
  onSearchChange,
  onClearSearch,
  onSelect,
  onReorder,
  onEdit,
  onDelete,
}: CategoryListProps) {
  const { t } = useI18n();
  const sensors = useSensors(
    useSensor(PointerSensor, { activationConstraint: { distance: 5 } }),
  );

  const filtered = useMemo(() => {
    if (!searchText) return categories;
    const q = searchText.toLowerCase();
    return categories.filter((c) => c.name.toLowerCase().includes(q));
  }, [categories, searchText]);

  // Roving tabindex: the list is a single Tab stop. Arrow keys move focus
  // between categories; Enter/Space selects the focused one.
  const [focusedId, setFocusedId] = useState<string | null>(null);
  const itemRefs = useRef(new Map<string, HTMLButtonElement>());
  const tabStopId =
    [focusedId, activeCategoryId].find((id) => id && filtered.some((c) => c.id === id)) ??
    filtered[0]?.id ??
    null;

  const handleItemKeyDown = (e: React.KeyboardEvent, index: number) => {
    let next: number | null = null;
    if (e.key === 'ArrowDown') next = Math.min(index + 1, filtered.length - 1);
    else if (e.key === 'ArrowUp') next = Math.max(index - 1, 0);
    else if (e.key === 'Home') next = 0;
    else if (e.key === 'End') next = filtered.length - 1;
    if (next === null) return;
    e.preventDefault();
    itemRefs.current.get(filtered[next].id)?.focus();
  };

  const handleDragEnd = (event: DragEndEvent) => {
    const { active, over } = event;
    if (!over || active.id === over.id) return;
    const oldIndex = categories.findIndex((c) => c.id === active.id);
    const newIndex = categories.findIndex((c) => c.id === over.id);
    if (oldIndex === -1 || newIndex === -1) return;
    const newOrder = arrayMove(categories, oldIndex, newIndex);
    onReorder(newOrder.map((c) => c.id));
  };

  return (
    <div className="flex flex-col flex-1 min-h-0">
      <div className="px-2 py-2 border-b border-divider shrink-0">
        <SearchInput
          value={searchText}
          onChange={onSearchChange}
          onClear={onClearSearch}
          placeholder={t('toolbar.searchCategories')}
        />
      </div>
      <div className="flex-1 overflow-y-auto">
        <DndContext
          sensors={sensors}
          collisionDetection={closestCenter}
          onDragEnd={handleDragEnd}
        >
          <SortableContext
            items={filtered.map((c) => c.id)}
            strategy={verticalListSortingStrategy}
          >
            {filtered.map((c, index) => (
              <SortableCategory
                key={c.id}
                category={c}
                isActive={c.id === activeCategoryId}
                isTabStop={c.id === tabStopId}
                itemRef={(el) => {
                  if (el) itemRefs.current.set(c.id, el);
                  else itemRefs.current.delete(c.id);
                }}
                onItemFocus={() => setFocusedId(c.id)}
                onItemKeyDown={(e) => handleItemKeyDown(e, index)}
                onSelect={() => onSelect(c.id)}
                onEdit={() => onEdit(c)}
                onDelete={() => onDelete(c.id)}
              />
            ))}
          </SortableContext>
        </DndContext>
      </div>
    </div>
  );
}

interface SortableCategoryProps {
  category: Category;
  isActive: boolean;
  isTabStop: boolean;
  itemRef: (el: HTMLButtonElement | null) => void;
  onItemFocus: () => void;
  onItemKeyDown: (e: React.KeyboardEvent) => void;
  onSelect: () => void;
  onEdit: () => void;
  onDelete: () => void;
}

function SortableCategory({
  category,
  isActive,
  isTabStop,
  itemRef,
  onItemFocus,
  onItemKeyDown,
  onSelect,
  onEdit,
  onDelete,
}: SortableCategoryProps) {
  const { t } = useI18n();
  const { attributes, listeners, setNodeRef, transform, transition, isDragging } =
    useSortable({ id: category.id });
  const tabIndex = isTabStop ? 0 : -1;

  const style = {
    transform: CSS.Transform.toString(transform),
    transition,
    zIndex: isDragging ? 10 : undefined,
    opacity: isDragging ? 0.8 : 1,
  };

  return (
    <div
      ref={setNodeRef}
      style={style}
      className={cn(
        'group flex items-center gap-2 px-3 py-2 cursor-pointer border-l-2 transition',
        'has-[[data-category-item]:focus-visible]:ring-1 has-[[data-category-item]:focus-visible]:ring-inset has-[[data-category-item]:focus-visible]:ring-accent',
        isActive
          ? 'bg-selected-bg border-accent'
          : 'border-transparent hover:bg-hover-bg',
      )}
    >
      {/* Pointer-only drag handle; no keyboard sensor is registered. */}
      <button
        {...attributes}
        {...listeners}
        tabIndex={-1}
        aria-hidden="true"
        className="cursor-grab text-ink-secondary opacity-0 group-hover:opacity-100"
      >
        <GripVertical size={14} />
      </button>
      <Tooltip content={category.name} className="flex-1 min-w-0">
        <button
          ref={itemRef}
          data-category-item
          tabIndex={tabIndex}
          aria-current={isActive || undefined}
          onClick={onSelect}
          onFocus={onItemFocus}
          onKeyDown={onItemKeyDown}
          className="block w-full min-w-0 text-left text-sm text-ink truncate cursor-pointer focus:outline-none"
        >
          {category.name}
        </button>
      </Tooltip>
      <div className="flex items-center gap-0.5 opacity-0 group-hover:opacity-100 group-focus-within:opacity-100">
        <button
          onClick={onEdit}
          tabIndex={tabIndex}
          aria-label={t('common.edit')}
          className="p-1 rounded hover:bg-hover-bg text-ink-secondary hover:text-ink"
        >
          <Pencil size={13} />
        </button>
        <button
          onClick={onDelete}
          tabIndex={tabIndex}
          aria-label={t('common.delete')}
          className="p-1 rounded hover:bg-hover-bg text-ink-secondary hover:text-danger"
        >
          <Trash2 size={13} />
        </button>
      </div>
    </div>
  );
}
