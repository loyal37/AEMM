import { Copy, Layers3, Plus } from "lucide-react";
import { EmptyState } from "../components/ui/EmptyState";
import { PageHeader } from "../components/ui/PageHeader";

export function ProfilesPage() {
  return (
    <div className="page-stack">
      <PageHeader
        eyebrow="配置方案"
        title="Profiles"
        description="为角色替换、截图和测试维护彼此独立的模组组合与加载顺序。"
        actions={
          <button className="button button--primary" type="button" disabled>
            <Plus size={17} />
            新建配置
          </button>
        }
      />
      <section className="panel panel--fill">
        <EmptyState
          icon={Layers3}
          title="默认配置已预留"
          description="Profile 的创建、复制、切换和事务化部署同步将在 Phase 8 开放。"
          action={
            <button className="button button--secondary" type="button" disabled>
              <Copy size={16} />
              复制默认配置
            </button>
          }
        />
      </section>
    </div>
  );
}
