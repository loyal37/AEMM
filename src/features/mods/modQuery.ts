import type { ModLifecycleState, ModListItem } from "../../types/app";

export type ModViewMode = "grid" | "list";
export type ModSort = "name" | "installed" | "updated" | "size";

export interface ModFilters {
  query: string;
  category: string;
  lifecycle: "all" | ModLifecycleState;
  deployment: "all" | "enabled" | "disabled";
  favoritesOnly: boolean;
  sort: ModSort;
}

export const defaultModFilters: ModFilters = {
  query: "",
  category: "all",
  lifecycle: "all",
  deployment: "all",
  favoritesOnly: false,
  sort: "updated",
};

const nameCollator = new Intl.Collator("zh-CN", {
  numeric: true,
  sensitivity: "base",
});

export function modCategories(mods: ModListItem[]): string[] {
  return Array.from(
    new Set(
      mods
        .map((item) => item.category?.trim())
        .filter((category): category is string => Boolean(category)),
    ),
  ).sort(nameCollator.compare);
}

export function applyModQuery(mods: ModListItem[], filters: ModFilters): ModListItem[] {
  const query = normalize(filters.query);
  return mods
    .filter((item) => {
      if (filters.category !== "all" && item.category !== filters.category) return false;
      if (filters.lifecycle !== "all" && item.lifecycleState !== filters.lifecycle) return false;
      if (filters.deployment === "enabled" && !item.enabled) return false;
      if (filters.deployment === "disabled" && item.enabled) return false;
      if (filters.favoritesOnly && !item.favorite) return false;
      if (!query) return true;
      return [item.name, item.author, item.category, item.logicalId, item.description]
        .filter((value): value is string => Boolean(value))
        .some((value) => normalize(value).includes(query));
    })
    .sort((left, right) => compareMods(left, right, filters.sort));
}

function compareMods(left: ModListItem, right: ModListItem, sort: ModSort): number {
  switch (sort) {
    case "name":
      return nameCollator.compare(left.name, right.name);
    case "installed":
      return right.installedAt - left.installedAt || nameCollator.compare(left.name, right.name);
    case "updated":
      return right.updatedAt - left.updatedAt || nameCollator.compare(left.name, right.name);
    case "size":
      return right.sizeBytes - left.sizeBytes || nameCollator.compare(left.name, right.name);
  }
}

function normalize(value: string): string {
  return value.normalize("NFKC").trim().toLocaleLowerCase("zh-CN");
}

export function formatFileSize(bytes: number): string {
  if (!Number.isFinite(bytes) || bytes < 0) return "未知";
  if (bytes < 1024) return `${bytes} B`;
  const units = ["KB", "MB", "GB", "TB"];
  let value = bytes / 1024;
  let unit = units[0] ?? "KB";
  for (let index = 1; index < units.length && value >= 1024; index += 1) {
    value /= 1024;
    unit = units[index] ?? unit;
  }
  return `${value >= 100 ? value.toFixed(0) : value.toFixed(1)} ${unit}`;
}

export function formatTimestamp(seconds: number): string {
  if (!Number.isFinite(seconds) || seconds <= 0) return "未知";
  return new Intl.DateTimeFormat("zh-CN", {
    year: "numeric",
    month: "short",
    day: "numeric",
  }).format(new Date(seconds * 1000));
}

export function lifecycleLabel(state: ModLifecycleState): string {
  const labels: Record<ModLifecycleState, string> = {
    installing: "安装中",
    installed: "已安装",
    broken: "需要检查",
    removing: "移除中",
  };
  return labels[state];
}
