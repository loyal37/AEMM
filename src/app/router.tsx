import { createHashRouter } from "react-router";
import { AppShell } from "../components/layout/AppShell";

function InitialRouteFallback() {
  return (
    <div className="initial-route-fallback" role="status" aria-live="polite">
      <span className="initial-route-fallback__mark" aria-hidden="true">
        A
      </span>
      <span>正在加载 AEMM…</span>
    </div>
  );
}

export const router = createHashRouter([
  {
    path: "/",
    Component: AppShell,
    HydrateFallback: InitialRouteFallback,
    children: [
      {
        index: true,
        lazy: async () => ({ Component: (await import("../pages/DashboardPage")).DashboardPage }),
      },
      {
        path: "mods",
        lazy: async () => ({ Component: (await import("../pages/ModsPage")).ModsPage }),
      },
      {
        path: "mods/:modId",
        lazy: async () => ({ Component: (await import("../pages/ModDetailPage")).ModDetailPage }),
      },
      {
        path: "profiles",
        lazy: async () => ({ Component: (await import("../pages/ProfilesPage")).ProfilesPage }),
      },
      {
        path: "settings",
        lazy: async () => ({ Component: (await import("../pages/SettingsPage")).SettingsPage }),
      },
      {
        path: "*",
        lazy: async () => ({ Component: (await import("../pages/NotFoundPage")).NotFoundPage }),
      },
    ],
  },
]);
