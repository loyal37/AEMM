import {
  AlertTriangle,
  Archive,
  CheckSquare2,
  Grid2X2,
  Heart,
  List,
  LoaderCircle,
  Power,
  PowerOff,
  RefreshCw,
  Search,
  Star,
  Upload,
  X,
} from "lucide-react";
import { useCallback, useDeferredValue, useEffect, useMemo, useState } from "react";
import { EmptyState } from "../components/ui/EmptyState";
import { PageHeader } from "../components/ui/PageHeader";
import { useAppBootstrap } from "../features/bootstrap/useAppBootstrap";
import { ConflictReportPanel } from "../features/conflicts/ConflictReportPanel";
import { useConflictReport } from "../features/conflicts/useConflictReport";
import { useGameStatus } from "../features/game/useGameManager";
import {
  applyModQuery,
  defaultModFilters,
  modCategories,
  type ModFilters,
  type ModViewMode,
} from "../features/mods/modQuery";
import {
  useInstalledMods,
  useScanMods,
  useSetModFavorite,
  useSetModsEnabled,
} from "../features/mods/useModManager";
import { VirtualModBrowser } from "../features/mods/VirtualModBrowser";
import { ImportModDialog } from "../features/mods/ImportModDialog";
import { commandErrorMessage } from "../lib/tauri";

export function ModsPage() {
  const bootstrap = useAppBootstrap();
  const mods = useInstalledMods();
  const conflicts = useConflictReport();
  const gameStatus = useGameStatus();
  const scan = useScanMods();
  const favorite = useSetModFavorite();
  const deployment = useSetModsEnabled();
  const [viewMode, setViewMode] = useState<ModViewMode>("grid");
  const [filters, setFilters] = useState<ModFilters>(defaultModFilters);
  const [selectedIds, setSelectedIds] = useState<Set<string>>(() => new Set());
  const [importOpen, setImportOpen] = useState(false);
  const deferredQuery = useDeferredValue(filters.query);
  const allMods = mods.data ?? [];
  const categories = useMemo(() => modCategories(allMods), [allMods]);
  const conflictedModIds = useMemo(
    () =>
      new Set(
        (conflicts.data?.conflicts ?? []).flatMap((conflict) =>
          conflict.participants.map((participant) => participant.modId),
        ),
      ),
    [conflicts.data],
  );
  const visibleMods = useMemo(
    () =>
      applyModQuery(
        allMods,
        { ...filters, query: deferredQuery },
        conflictedModIds,
      ),
    [allMods, conflictedModIds, deferredQuery, filters],
  );
  const desktopReady = bootstrap.data?.runtimeMode === "desktop";
  const deploymentAvailable = desktopReady && gameStatus.data?.loader?.valid === true;
  const openImport = useCallback(() => setImportOpen(true), []);
  const closeImport = useCallback(() => setImportOpen(false), []);

  useEffect(() => {
    const currentIds = new Set(allMods.map((item) => item.id));
    setSelectedIds((previous) => {
      const next = new Set([...previous].filter((id) => currentIds.has(id)));
      return next.size === previous.size ? previous : next;
    });
  }, [allMods]);

  const updateFilter = <Key extends keyof ModFilters>(key: Key, value: ModFilters[Key]) => {
    setFilters((current) => ({ ...current, [key]: value }));
  };
  const toggleSelected = (modId: string) => {
    setSelectedIds((current) => {
      const next = new Set(current);
      if (next.has(modId)) next.delete(modId);
      else next.add(modId);
      return next;
    });
  };
  const allVisibleSelected =
    visibleMods.length > 0 && visibleMods.every((item) => selectedIds.has(item.id));
  const toggleAllVisible = () => {
    setSelectedIds((current) => {
      const next = new Set(current);
      if (allVisibleSelected) visibleMods.forEach((item) => next.delete(item.id));
      else visibleMods.forEach((item) => next.add(item.id));
      return next;
    });
  };

  return (
    <div className="page-stack page-stack--mods">
      <PageHeader
        eyebrow="模组仓库"
        title="模组"
        description="扫描、检索和整理仓库中的模组，或通过隔离暂存、确认计划与自动回滚安全导入新模组。"
        actions={
          <>
            <button
              className="button button--secondary"
              type="button"
              disabled={!desktopReady || scan.isPending}
              onClick={() => scan.mutate()}
            >
              <RefreshCw className={scan.isPending ? "spin" : undefined} size={17} />
              {scan.isPending ? "正在扫描…" : "扫描仓库"}
            </button>
            <button
              className="button button--primary"
              type="button"
              disabled={!desktopReady}
              onClick={openImport}
            >
              <Upload size={17} />
              导入模组
            </button>
          </>
        }
      />

      {scan.isSuccess ? (
        <p className="inline-success">
          扫描完成：发现 {scan.data.discovered} 个，新增 {scan.data.added} 个，更新{" "}
          {scan.data.updated} 个，复用 {scan.data.reusedHashes} 个文件 Hash。
        </p>
      ) : null}
      {scan.isError || mods.isError || favorite.isError || deployment.isError || conflicts.isError ? (
        <p className="inline-error">
          {commandErrorMessage(
            scan.error ??
              mods.error ??
              favorite.error ??
              deployment.error ??
              conflicts.error,
          )}
        </p>
      ) : null}

      {conflicts.data ? <ConflictReportPanel report={conflicts.data} /> : null}
      {deployment.isSuccess && deployment.data.updated > 0 ? (
        <div className="deployment-feedback" role="status">
          <strong>
            已{deployment.data.enabled ? "启用" : "禁用"} {deployment.data.updated} 个模组
          </strong>
          {deployment.data.guidance ? <span>{deployment.data.guidance}</span> : null}
          {deployment.data.warnings.map((warning) => (
            <span key={warning}>{warning}</span>
          ))}
        </div>
      ) : null}

      <section className="mod-controls" aria-label="模组筛选和排序">
        <label className="search-field mod-search">
          <Search size={17} />
          <span className="sr-only">搜索模组</span>
          <input
            type="search"
            placeholder="搜索名称、作者、分类或 ID"
            value={filters.query}
            onChange={(event) => updateFilter("query", event.target.value)}
          />
          {filters.query ? (
            <button
              className="search-field__clear"
              type="button"
              aria-label="清除搜索"
              onClick={() => updateFilter("query", "")}
            >
              <X size={14} />
            </button>
          ) : null}
        </label>
        <label className="compact-select">
          <span className="sr-only">按分类筛选</span>
          <select
            value={filters.category}
            onChange={(event) => updateFilter("category", event.target.value)}
          >
            <option value="all">全部分类</option>
            {categories.map((category) => (
              <option value={category} key={category}>
                {category}
              </option>
            ))}
          </select>
        </label>
        <label className="compact-select">
          <span className="sr-only">按状态筛选</span>
          <select
            value={filters.lifecycle}
            onChange={(event) =>
              updateFilter("lifecycle", event.target.value as ModFilters["lifecycle"])
            }
          >
            <option value="all">全部状态</option>
            <option value="installed">已安装</option>
            <option value="broken">需要检查</option>
            <option value="installing">安装中</option>
            <option value="removing">移除中</option>
          </select>
        </label>
        <label className="compact-select compact-select--sort">
          <span className="sr-only">排序方式</span>
          <select
            value={filters.sort}
            onChange={(event) => updateFilter("sort", event.target.value as ModFilters["sort"])}
          >
            <option value="updated">最近更新</option>
            <option value="installed">最近安装</option>
            <option value="name">名称排序</option>
            <option value="size">文件大小</option>
          </select>
        </label>
        <label className="compact-select">
          <span className="sr-only">按启用状态筛选</span>
          <select
            value={filters.deployment}
            onChange={(event) =>
              updateFilter(
                "deployment",
                event.target.value as ModFilters["deployment"],
              )
            }
          >
            <option value="all">全部启用状态</option>
            <option value="enabled">已启用</option>
            <option value="disabled">已禁用</option>
          </select>
        </label>
        <button
          className={`filter-toggle${filters.favoritesOnly ? " is-active" : ""}`}
          type="button"
          aria-pressed={filters.favoritesOnly}
          onClick={() => updateFilter("favoritesOnly", !filters.favoritesOnly)}
        >
          <Star size={16} fill={filters.favoritesOnly ? "currentColor" : "none"} />
          收藏
        </button>
        <button
          className={`filter-toggle filter-toggle--conflict${filters.conflictsOnly ? " is-active" : ""}`}
          type="button"
          aria-pressed={filters.conflictsOnly}
          onClick={() => updateFilter("conflictsOnly", !filters.conflictsOnly)}
        >
          <AlertTriangle size={16} />
          冲突 {conflictedModIds.size ? `(${conflictedModIds.size})` : ""}
        </button>
        <div className="segmented-control" aria-label="视图模式">
          <button
            type="button"
            aria-label="卡片视图"
            aria-pressed={viewMode === "grid"}
            onClick={() => setViewMode("grid")}
          >
            <Grid2X2 size={17} />
          </button>
          <button
            type="button"
            aria-label="列表视图"
            aria-pressed={viewMode === "list"}
            onClick={() => setViewMode("list")}
          >
            <List size={18} />
          </button>
        </div>
      </section>

      <div className="mod-results-meta">
        <span>
          显示 <strong>{visibleMods.length}</strong> / {allMods.length} 个模组
        </span>
        {visibleMods.length ? (
          <button type="button" onClick={toggleAllVisible}>
            <CheckSquare2 size={15} />
            {allVisibleSelected ? "取消选择当前结果" : "选择当前结果"}
          </button>
        ) : null}
      </div>

      {selectedIds.size > 0 ? (
        <div className="batch-bar" role="region" aria-label="批量操作">
          <span>已选择 {selectedIds.size} 个模组</span>
          <div>
            <button
              className="button button--primary"
              type="button"
              disabled={!deploymentAvailable || deployment.isPending}
              title={!deploymentAvailable ? "请先配置有效的 EFMI 加载器" : undefined}
              onClick={() =>
                deployment.mutate({ modIds: [...selectedIds], enabled: true })
              }
            >
              <Power size={15} /> 批量启用
            </button>
            <button
              className="button button--secondary"
              type="button"
              disabled={!deploymentAvailable || deployment.isPending}
              title={!deploymentAvailable ? "请先配置有效的 EFMI 加载器" : undefined}
              onClick={() =>
                deployment.mutate({ modIds: [...selectedIds], enabled: false })
              }
            >
              <PowerOff size={15} /> 批量禁用
            </button>
            <button
              className="button button--secondary"
              type="button"
              disabled={favorite.isPending || deployment.isPending}
              onClick={() =>
                favorite.mutate({ modIds: [...selectedIds], favorite: true })
              }
            >
              <Heart size={15} /> 批量收藏
            </button>
            <button
              className="button button--ghost"
              type="button"
              disabled={favorite.isPending || deployment.isPending}
              onClick={() =>
                favorite.mutate({ modIds: [...selectedIds], favorite: false })
              }
            >
              取消收藏
            </button>
            <button
              className="batch-bar__clear"
              type="button"
              aria-label="清除选择"
              onClick={() => setSelectedIds(new Set())}
            >
              <X size={17} />
            </button>
          </div>
        </div>
      ) : null}

      {mods.isPending ? (
        <section className="panel panel--fill">
          <div className="loading-state">
            <LoaderCircle className="spin" size={27} />
            <span>正在读取模组数据库…</span>
          </div>
        </section>
      ) : visibleMods.length ? (
        <VirtualModBrowser
          items={visibleMods}
          viewMode={viewMode}
          selectedIds={selectedIds}
          conflictedIds={conflictedModIds}
          favoritePending={favorite.isPending}
          deploymentPending={deployment.isPending}
          deploymentAvailable={deploymentAvailable}
          onToggleSelected={toggleSelected}
          onFavorite={(item) =>
            favorite.mutate({ modIds: [item.id], favorite: !item.favorite })
          }
          onSetEnabled={(item) =>
            deployment.mutate({ modIds: [item.id], enabled: !item.enabled })
          }
        />
      ) : (
        <section className="panel panel--fill">
          <EmptyState
            icon={Archive}
            title={allMods.length ? "没有符合条件的模组" : "模组仓库是空的"}
            description={
              allMods.length
                ? "调整搜索、分类或状态筛选后再试。"
                : "点击“导入模组”，或将一个 ZIP、7z、RAR 压缩包或文件夹直接拖入窗口。"
            }
            action={
              allMods.length ? (
                <button
                  className="button button--secondary"
                  type="button"
                  onClick={() => setFilters(defaultModFilters)}
                >
                  清除全部筛选
                </button>
              ) : undefined
            }
          />
        </section>
      )}
      <ImportModDialog
        desktopReady={desktopReady}
        isOpen={importOpen}
        onOpen={openImport}
        onClose={closeImport}
      />
    </div>
  );
}
