import { Bell, LoaderCircle } from "lucide-react";
import { useProfiles, useSwitchProfile } from "../../features/profiles/useProfiles";
import { commandErrorMessage } from "../../lib/tauri";

export function TopBar() {
  const profiles = useProfiles();
  const switchProfile = useSwitchProfile();
  const items = profiles.data ?? [];
  const active = items.find((profile) => profile.isActive);

  return (
    <header className="top-bar">
      <div className="top-bar__context">
        <span className="eyebrow">当前配置方案</span>
        <div className="profile-selector-wrap">
          <select
            className="profile-selector"
            aria-label="切换当前 Profile"
            value={active?.id ?? ""}
            disabled={profiles.isPending || switchProfile.isPending || items.length === 0}
            title={switchProfile.isError ? commandErrorMessage(switchProfile.error) : undefined}
            onChange={(event) => switchProfile.mutate(event.target.value)}
          >
            {items.length === 0 ? <option value="">正在读取…</option> : null}
            {items.map((profile) => (
              <option value={profile.id} key={profile.id}>
                {profile.name}
              </option>
            ))}
          </select>
          {switchProfile.isPending ? <LoaderCircle className="spin" size={14} /> : null}
        </div>
      </div>
      <button className="icon-button" type="button" aria-label="通知" disabled>
        <Bell size={18} />
      </button>
    </header>
  );
}
