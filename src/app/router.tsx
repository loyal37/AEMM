import { createHashRouter } from "react-router";
import { AppShell } from "../components/layout/AppShell";
import { DashboardPage } from "../pages/DashboardPage";
import { ModDetailPage } from "../pages/ModDetailPage";
import { ModsPage } from "../pages/ModsPage";
import { NotFoundPage } from "../pages/NotFoundPage";
import { ProfilesPage } from "../pages/ProfilesPage";
import { SettingsPage } from "../pages/SettingsPage";

export const router = createHashRouter([
  {
    path: "/",
    Component: AppShell,
    children: [
      { index: true, Component: DashboardPage },
      { path: "mods", Component: ModsPage },
      { path: "mods/:modId", Component: ModDetailPage },
      { path: "profiles", Component: ProfilesPage },
      { path: "settings", Component: SettingsPage },
      { path: "*", Component: NotFoundPage },
    ],
  },
]);
