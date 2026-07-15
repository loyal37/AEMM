import { ArrowLeft, FileQuestion } from "lucide-react";
import { Link, useParams } from "react-router";
import { EmptyState } from "../components/ui/EmptyState";

export function ModDetailPage() {
  const { modId } = useParams();

  return (
    <div className="page-stack">
      <Link className="back-link" to="/mods">
        <ArrowLeft size={17} />
        返回模组列表
      </Link>
      <section className="panel panel--fill">
        <EmptyState
          icon={FileQuestion}
          title="模组详情尚未可用"
          description={`请求的模组 ID：${modId ?? "未知"}。详情数据将在 Phase 4 接入。`}
        />
      </section>
    </div>
  );
}
