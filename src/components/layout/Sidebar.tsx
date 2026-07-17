import {
  Boxes,
  Gamepad2,
  LayoutDashboard,
  Settings,
  SlidersHorizontal,
} from "lucide-react";
import { NavLink } from "react-router";
import { useTranslation } from "react-i18next";
import { useGameStatus } from "../../features/game/useGameManager";

const navigation = [
  { to: "/", label: "navigation.dashboard", icon: LayoutDashboard, end: true },
  { to: "/mods", label: "navigation.mods", icon: Boxes },
  { to: "/profiles", label: "navigation.profiles", icon: SlidersHorizontal },
  { to: "/settings", label: "navigation.settings", icon: Settings },
];

export function Sidebar() {
  const gameStatus = useGameStatus();
  const { t } = useTranslation();
  const configured = gameStatus.data?.configured === true;

  return (
    <aside className="sidebar" aria-label={t("navigation.label")}>
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
            <span>{t(label)}</span>
          </NavLink>
        ))}
      </nav>

      <div className="sidebar-footer">
        <span className="eyebrow">{t("navigation.currentGame")}</span>
        <div className="game-chip">
          <span className={`status-dot ${configured ? "status-dot--ready" : "status-dot--idle"}`} />
          <div>
            <strong>{t("navigation.gameName")}</strong>
            <span>{configured ? t("navigation.configured") : t("navigation.waiting")}</span>
          </div>
        </div>
      </div>
    </aside>
  );
}
