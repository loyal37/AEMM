import { useVirtualizer } from "@tanstack/react-virtual";
import { AlertTriangle, Check, Files, Heart, Star } from "lucide-react";
import { useEffect, useMemo, useRef, useState } from "react";
import { Link } from "react-router";
import type { ModListItem } from "../../types/app";
import {
  formatFileSize,
  formatTimestamp,
  lifecycleLabel,
  type ModViewMode,
} from "./modQuery";
import { ModPreviewImage } from "./ModPreviewImage";

interface VirtualModBrowserProps {
  items: ModListItem[];
  viewMode: ModViewMode;
  selectedIds: Set<string>;
  favoritePending: boolean;
  onToggleSelected: (modId: string) => void;
  onFavorite: (item: ModListItem) => void;
}

export function VirtualModBrowser({
  items,
  viewMode,
  selectedIds,
  favoritePending,
  onToggleSelected,
  onFavorite,
}: VirtualModBrowserProps) {
  const scrollRef = useRef<HTMLDivElement>(null);
  const [width, setWidth] = useState(900);

  useEffect(() => {
    const element = scrollRef.current;
    if (!element) return;
    const observer = new ResizeObserver(([entry]) => {
      if (entry) setWidth(entry.contentRect.width);
    });
    observer.observe(element);
    return () => observer.disconnect();
  }, []);

  const columns = viewMode === "grid" ? (width >= 980 ? 3 : width >= 650 ? 2 : 1) : 1;
  const rows = Math.ceil(items.length / columns);
  const virtualizer = useVirtualizer({
    count: rows,
    getScrollElement: () => scrollRef.current,
    estimateSize: () => (viewMode === "grid" ? 286 : 78),
    overscan: 5,
  });

  useEffect(() => {
    virtualizer.measure();
  }, [columns, viewMode, virtualizer]);

  const virtualRows = virtualizer.getVirtualItems();
  const rowItems = useMemo(
    () =>
      virtualRows.map((virtualRow) => ({
        virtualRow,
        items: items.slice(virtualRow.index * columns, virtualRow.index * columns + columns),
      })),
    [columns, items, virtualRows],
  );

  return (
    <section className={`mod-browser mod-browser--${viewMode}`} aria-label="模组结果">
      {viewMode === "list" ? (
        <div className="mod-list-header" aria-hidden="true">
          <span />
          <span>模组</span>
          <span>分类</span>
          <span>大小</span>
          <span>更新时间</span>
          <span>状态</span>
          <span />
        </div>
      ) : null}
      <div className="mod-virtual-scroll" ref={scrollRef}>
        <div
          className="mod-virtual-canvas"
          style={{ height: `${virtualizer.getTotalSize()}px` }}
        >
          {rowItems.map(({ virtualRow, items: row }) => (
            <div
              className="mod-virtual-row"
              data-index={virtualRow.index}
              key={virtualRow.key}
              ref={virtualizer.measureElement}
              style={{
                gridTemplateColumns: `repeat(${columns}, minmax(0, 1fr))`,
                transform: `translateY(${virtualRow.start}px)`,
              }}
            >
              {row.map((item) =>
                viewMode === "grid" ? (
                  <ModCard
                    item={item}
                    key={item.id}
                    selected={selectedIds.has(item.id)}
                    favoritePending={favoritePending}
                    onToggleSelected={onToggleSelected}
                    onFavorite={onFavorite}
                  />
                ) : (
                  <ModListRow
                    item={item}
                    key={item.id}
                    selected={selectedIds.has(item.id)}
                    favoritePending={favoritePending}
                    onToggleSelected={onToggleSelected}
                    onFavorite={onFavorite}
                  />
                ),
              )}
            </div>
          ))}
        </div>
      </div>
    </section>
  );
}

interface ModEntryProps {
  item: ModListItem;
  selected: boolean;
  favoritePending: boolean;
  onToggleSelected: (modId: string) => void;
  onFavorite: (item: ModListItem) => void;
}

function ModCard({
  item,
  selected,
  favoritePending,
  onToggleSelected,
  onFavorite,
}: ModEntryProps) {
  return (
    <article className={`mod-card${selected ? " is-selected" : ""}`}>
      <div className="mod-card__media">
        <Link to={`/mods/${item.id}`} aria-label={`查看 ${item.name} 详情`}>
          <ModPreviewImage
            modId={item.id}
            name={item.name}
            hasPreview={Boolean(item.previewPath)}
            variant="card"
          />
        </Link>
        <button
          className={`selection-check${selected ? " is-selected" : ""}`}
          type="button"
          aria-label={selected ? `取消选择 ${item.name}` : `选择 ${item.name}`}
          aria-pressed={selected}
          onClick={() => onToggleSelected(item.id)}
        >
          {selected ? <Check size={13} /> : null}
        </button>
        <button
          className={`favorite-button${item.favorite ? " is-active" : ""}`}
          type="button"
          aria-label={item.favorite ? `取消收藏 ${item.name}` : `收藏 ${item.name}`}
          aria-pressed={item.favorite}
          disabled={favoritePending}
          onClick={() => onFavorite(item)}
        >
          <Star size={16} fill={item.favorite ? "currentColor" : "none"} />
        </button>
      </div>
      <div className="mod-card__body">
        <div className="mod-card__title-row">
          <div>
            <Link to={`/mods/${item.id}`}>{item.name}</Link>
            <span>{item.author ?? "未知作者"}</span>
          </div>
          <LifecycleBadge state={item.lifecycleState} />
        </div>
        <div className="mod-card__tags">
          <span>{item.category ?? "未分类"}</span>
          <span>v{item.version ?? "未知"}</span>
        </div>
        <div className="mod-card__footer">
          <span>
            <Files size={13} /> {item.fileCount} 个文件
          </span>
          <span>{formatFileSize(item.sizeBytes)}</span>
        </div>
      </div>
    </article>
  );
}

function ModListRow({
  item,
  selected,
  favoritePending,
  onToggleSelected,
  onFavorite,
}: ModEntryProps) {
  return (
    <article className={`mod-list-row${selected ? " is-selected" : ""}`}>
      <button
        className={`selection-check selection-check--inline${selected ? " is-selected" : ""}`}
        type="button"
        aria-label={selected ? `取消选择 ${item.name}` : `选择 ${item.name}`}
        aria-pressed={selected}
        onClick={() => onToggleSelected(item.id)}
      >
        {selected ? <Check size={13} /> : null}
      </button>
      <div className="mod-list-row__identity">
        <Link to={`/mods/${item.id}`}>
          <ModPreviewImage
            modId={item.id}
            name={item.name}
            hasPreview={Boolean(item.previewPath)}
            variant="list"
          />
        </Link>
        <div>
          <Link to={`/mods/${item.id}`}>{item.name}</Link>
          <span>{item.author ?? "未知作者"} · v{item.version ?? "未知"}</span>
        </div>
      </div>
      <span className="mod-list-row__muted">{item.category ?? "未分类"}</span>
      <span className="mod-list-row__muted">{formatFileSize(item.sizeBytes)}</span>
      <span className="mod-list-row__muted">{formatTimestamp(item.updatedAt)}</span>
      <LifecycleBadge state={item.lifecycleState} />
      <button
        className={`favorite-button favorite-button--inline${item.favorite ? " is-active" : ""}`}
        type="button"
        aria-label={item.favorite ? `取消收藏 ${item.name}` : `收藏 ${item.name}`}
        aria-pressed={item.favorite}
        disabled={favoritePending}
        onClick={() => onFavorite(item)}
      >
        <Heart size={16} fill={item.favorite ? "currentColor" : "none"} />
      </button>
    </article>
  );
}

function LifecycleBadge({ state }: { state: ModListItem["lifecycleState"] }) {
  return (
    <span className={`mod-state mod-state--${state}`}>
      {state === "broken" ? <AlertTriangle size={12} /> : null}
      {lifecycleLabel(state)}
    </span>
  );
}
