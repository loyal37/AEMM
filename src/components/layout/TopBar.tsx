import { Bell, ChevronDown } from "lucide-react";

export function TopBar() {
  return (
    <header className="top-bar">
      <div className="top-bar__context">
        <span className="eyebrow">当前配置方案</span>
        <button className="profile-selector" type="button" disabled>
          默认配置
          <ChevronDown size={15} />
        </button>
      </div>
      <button className="icon-button" type="button" aria-label="通知" disabled>
        <Bell size={18} />
      </button>
    </header>
  );
}
