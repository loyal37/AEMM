import { Archive, Grid2X2, List, Search, SlidersHorizontal, Upload } from "lucide-react";
import { useState } from "react";
import { EmptyState } from "../components/ui/EmptyState";
import { PageHeader } from "../components/ui/PageHeader";

type ViewMode = "grid" | "list";

export function ModsPage() {
  const [viewMode, setViewMode] = useState<ViewMode>("grid");

  return (
    <div className="page-stack">
      <PageHeader
        eyebrow="模组仓库"
        title="模组"
        description="浏览、筛选和管理已安装内容。扫描与安装能力将在后续阶段接入。"
        actions={
          <button className="button button--primary" type="button" disabled>
            <Upload size={17} />
            导入模组
          </button>
        }
      />

      <div className="toolbar">
        <label className="search-field">
          <Search size={17} />
          <span className="sr-only">搜索模组</span>
          <input type="search" placeholder="搜索名称、作者或标签" disabled />
        </label>
        <button className="button button--ghost" type="button" disabled>
          <SlidersHorizontal size={17} />
          筛选与排序
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
      </div>

      <section className="panel panel--fill">
        <EmptyState
          icon={Archive}
          title="模组仓库是空的"
          description="Phase 3 会扫描仓库，Phase 5 将提供 ZIP、7z、RAR 与文件夹的安全导入流程。"
          action={
            <button className="button button--secondary" type="button" disabled>
              选择模组文件
            </button>
          }
        />
      </section>
    </div>
  );
}
