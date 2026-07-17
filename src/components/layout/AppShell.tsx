import { Outlet } from "react-router";
import { useTranslation } from "react-i18next";
import {
  ExperienceController,
  GlobalActivityIndicator,
  OnboardingDialog,
  OnboardingProvider,
} from "../../features/experience/AppExperience";
import { Sidebar } from "./Sidebar";
import { TopBar } from "./TopBar";

export function AppShell() {
  const { t } = useTranslation();
  return (
    <OnboardingProvider>
      <ExperienceController />
      <GlobalActivityIndicator />
      <a className="skip-link" href="#main-content">
        {t("common.skipNavigation")}
      </a>
      <div className="app-shell">
        <Sidebar />
        <div className="workspace">
          <TopBar />
          <main className="page-container" id="main-content" tabIndex={-1}>
            <Outlet />
          </main>
        </div>
      </div>
      <OnboardingDialog />
    </OnboardingProvider>
  );
}
