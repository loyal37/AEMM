import {
  DndContext,
  KeyboardSensor,
  PointerSensor,
  closestCenter,
  useSensor,
  useSensors,
  type DragEndEvent,
} from "@dnd-kit/core";
import {
  SortableContext,
  arrayMove,
  sortableKeyboardCoordinates,
  useSortable,
  verticalListSortingStrategy,
} from "@dnd-kit/sortable";
import { CSS } from "@dnd-kit/utilities";
import { ArrowDown, ArrowUp, Check, GripVertical, LoaderCircle, X } from "lucide-react";
import { useMemo, useRef, useState } from "react";
import { commandErrorMessage } from "../../lib/tauri";
import type { Profile, ProfileMod } from "../../types/app";
import { useDialogFocus } from "../experience/useDialogFocus";

interface LoadOrderDialogProps {
  profile: Profile;
  pending: boolean;
  error: unknown;
  onClose: () => void;
  onSave: (modIds: string[]) => Promise<void>;
}

export function LoadOrderDialog({
  profile,
  pending,
  error,
  onClose,
  onSave,
}: LoadOrderDialogProps) {
  const initial = useMemo(
    () =>
      profile.mods
        .filter((item) => item.enabled)
        .sort((left, right) => left.loadOrder - right.loadOrder),
    [profile],
  );
  const [items, setItems] = useState(initial);
  const closeButton = useRef<HTMLButtonElement>(null);
  const sensors = useSensors(
    useSensor(PointerSensor, { activationConstraint: { distance: 6 } }),
    useSensor(KeyboardSensor, { coordinateGetter: sortableKeyboardCoordinates }),
  );

  const dialog = useDialogFocus<HTMLElement>(true, () => {
    if (!pending) onClose();
  }, closeButton);

  const move = (index: number, delta: number) => {
    const target = index + delta;
    if (target < 0 || target >= items.length) return;
    setItems((current) => arrayMove(current, index, target));
  };

  const handleDragEnd = ({ active, over }: DragEndEvent) => {
    if (!over || active.id === over.id) return;
    setItems((current) => {
      const from = current.findIndex((item) => item.modId === active.id);
      const to = current.findIndex((item) => item.modId === over.id);
      return from >= 0 && to >= 0 ? arrayMove(current, from, to) : current;
    });
  };

  return (
    <div className="modal-backdrop load-order-backdrop" role="presentation">
      <section
        ref={dialog}
        className="load-order-dialog"
        role="dialog"
        aria-modal="true"
        aria-labelledby="load-order-title"
      >
        <header className="load-order-dialog__header">
          <div>
            <span className="eyebrow">AEMM Profile</span>
            <h2 id="load-order-title">调整“{profile.name}”加载顺序</h2>
            <p>拖动手柄，或使用每行的上移/下移按钮。此顺序不代表已验证的 EFMI 胜出优先级。</p>
          </div>
          <button
            ref={closeButton}
            className="icon-button"
            type="button"
            aria-label="关闭加载顺序编辑器"
            disabled={pending}
            onClick={onClose}
          >
            <X size={17} />
          </button>
        </header>
        <DndContext sensors={sensors} collisionDetection={closestCenter} onDragEnd={handleDragEnd}>
          <SortableContext
            items={items.map((item) => item.modId)}
            strategy={verticalListSortingStrategy}
          >
            <ol className="load-order-editor-list">
              {items.map((item, index) => (
                <SortableModRow
                  item={item}
                  index={index}
                  count={items.length}
                  pending={pending}
                  onMove={move}
                  key={item.modId}
                />
              ))}
            </ol>
          </SortableContext>
        </DndContext>
        {error ? <p className="inline-error">{commandErrorMessage(error)}</p> : null}
        <footer className="load-order-dialog__actions">
          <span>{items.length} 个启用模组</span>
          <div>
            <button className="button button--secondary" type="button" disabled={pending} onClick={onClose}>
              取消
            </button>
            <button
              className="button button--primary"
              type="button"
              disabled={pending}
              onClick={() => void onSave(items.map((item) => item.modId))}
            >
              {pending ? <LoaderCircle className="spin" size={15} /> : <Check size={15} />}
              保存顺序
            </button>
          </div>
        </footer>
      </section>
    </div>
  );
}

interface SortableModRowProps {
  item: ProfileMod;
  index: number;
  count: number;
  pending: boolean;
  onMove: (index: number, delta: number) => void;
}

function SortableModRow({ item, index, count, pending, onMove }: SortableModRowProps) {
  const { attributes, listeners, setNodeRef, transform, transition, isDragging } = useSortable({
    id: item.modId,
  });
  return (
    <li
      ref={setNodeRef}
      className={isDragging ? "is-dragging" : ""}
      style={{ transform: CSS.Transform.toString(transform), transition }}
    >
      <button
        className="load-order-drag-handle"
        type="button"
        aria-label={`拖动 ${item.modName}，当前位置 ${index + 1}`}
        disabled={pending}
        {...attributes}
        {...listeners}
      >
        <GripVertical size={15} />
      </button>
      <span>{index + 1}</span>
      <strong>{item.modName}</strong>
      <div>
        <button
          className="icon-button"
          type="button"
          aria-label={`上移 ${item.modName}`}
          disabled={pending || index === 0}
          onClick={() => onMove(index, -1)}
        >
          <ArrowUp size={14} />
        </button>
        <button
          className="icon-button"
          type="button"
          aria-label={`下移 ${item.modName}`}
          disabled={pending || index === count - 1}
          onClick={() => onMove(index, 1)}
        >
          <ArrowDown size={14} />
        </button>
      </div>
    </li>
  );
}
