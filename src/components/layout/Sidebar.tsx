import {
  Boxes,
  Gamepad2,
  LayoutDashboard,
  Settings,
  SlidersHorizontal,
} from "lucide-react";
import { NavLink } from "react-router";

const navigation = [
  { to: "/", label: "首页", icon: LayoutDashboard, end: true },
  { to: "/mods", label: "模组", icon: Boxes },
  { to: "/profiles", label: "配置方案", icon: SlidersHorizontal },
  { to: "/settings", label: "设置", icon: Settings },
];

export function Sidebar() {
  return (
    <aside className="sidebar" aria-label="主导航">
      <div className="brand">
        <div className="brand-mark" aria-hidden="true">
          <Gamepad2 size={22} strokeWidth={1.8} />
        </div>
        <div>
          <strong>AEMM</strong>
          <span>Endfield Mod Manager</span>
        </div>
      </div>

      <nav className="nav-list">
        {navigation.map(({ to, label, icon: Icon, end }) => (
          <NavLink
            key={to}
            to={to}
            end={end}
            className={({ isActive }) => `nav-item${isActive ? " is-active" : ""}`}
          >
            <Icon size={19} strokeWidth={1.8} />
            <span>{label}</span>
          </NavLink>
        ))}
      </nav>

      <div className="sidebar-footer">
        <span className="eyebrow">当前游戏</span>
        <div className="game-chip">
          <span className="status-dot status-dot--idle" />
          <div>
            <strong>明日方舟：终末地</strong>
            <span>等待设置路径</span>
          </div>
        </div>
      </div>
    </aside>
  );
}
