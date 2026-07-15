import { CircleOff } from "lucide-react";
import { Link } from "react-router";
import { EmptyState } from "../components/ui/EmptyState";

export function NotFoundPage() {
  return (
    <section className="panel panel--fill">
      <EmptyState
        icon={CircleOff}
        title="没有找到这个页面"
        description="页面地址可能已经变更。"
        action={
          <Link className="button button--secondary" to="/">
            返回首页
          </Link>
        }
      />
    </section>
  );
}
