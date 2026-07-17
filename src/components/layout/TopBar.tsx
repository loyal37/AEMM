import { Bell, LoaderCircle } from "lucide-react";
import { useProfiles, useSwitchProfile } from "../../features/profiles/useProfiles";
import { commandErrorMessage } from "../../lib/tauri";
import { useTranslation } from "react-i18next";

export function TopBar() {
  const profiles = useProfiles();
  const switchProfile = useSwitchProfile();
  const { t } = useTranslation();
  const items = profiles.data ?? [];
  const active = items.find((profile) => profile.isActive);

  return (
    <header className="top-bar">
      <div className="top-bar__context">
        <span className="eyebrow">{t("topbar.activeProfile")}</span>
        <div className="profile-selector-wrap">
          <select
            className="profile-selector"
            aria-label={t("topbar.switchProfile")}
            value={active?.id ?? ""}
            disabled={profiles.isPending || switchProfile.isPending || items.length === 0}
            title={switchProfile.isError ? commandErrorMessage(switchProfile.error) : undefined}
            onChange={(event) => switchProfile.mutate(event.target.value)}
          >
            {items.length === 0 ? <option value="">{t("topbar.loading")}</option> : null}
            {items.map((profile) => (
              <option value={profile.id} key={profile.id}>
                {profile.name}
              </option>
            ))}
          </select>
          {switchProfile.isPending ? <LoaderCircle className="spin" size={14} /> : null}
        </div>
      </div>
      <button className="icon-button" type="button" aria-label={t("topbar.notifications")} disabled>
        <Bell size={18} />
      </button>
    </header>
  );
}
