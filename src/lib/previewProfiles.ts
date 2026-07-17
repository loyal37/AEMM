import type { Profile, ProfileSwitchResult } from "../types/app";

const DEFAULT_PROFILE_ID = "00000000-0000-0000-0000-000000000001";
let nextProfile = 2;
let activeProfileId = DEFAULT_PROFILE_ID;
let profiles: Profile[] = [
  {
    id: DEFAULT_PROFILE_ID,
    name: "默认配置",
    isActive: true,
    createdAt: 1_752_000_000,
    updatedAt: 1_752_400_000,
    mods: Array.from({ length: 12 }, (_, index) => ({
      modId: `00000000-0000-4000-8000-${String(index * 4 + 2).padStart(12, "0")}`,
      modName: `预览模组 ${index + 1}`,
      enabled: true,
      loadOrder: index,
    })),
  },
  {
    id: "00000000-0000-0000-0000-000000000002",
    name: "截图配置",
    isActive: false,
    createdAt: 1_752_050_000,
    updatedAt: 1_752_350_000,
    mods: Array.from({ length: 5 }, (_, index) => ({
      modId: `00000000-0000-4000-8000-${String(index * 4 + 2).padStart(12, "0")}`,
      modName: `预览模组 ${index + 1}`,
      enabled: true,
      loadOrder: index,
    })),
  },
];

function snapshot(): Profile[] {
  return profiles.map((profile) => ({
    ...profile,
    isActive: profile.id === activeProfileId,
    mods: profile.mods.map((item) => ({ ...item })),
  }));
}

function requireProfile(profileId: string): Profile {
  const profile = profiles.find((item) => item.id === profileId);
  if (!profile) throw new Error("Profile 不存在。");
  return profile;
}

function ensureName(name: string, excluding?: string): string {
  const normalized = name.trim();
  if (!normalized) throw new Error("Profile 名称不能为空。");
  if (normalized.length > 64) throw new Error("Profile 名称不能超过 64 个字符。");
  if (
    profiles.some(
      (item) =>
        item.id !== excluding &&
        item.name.toLocaleLowerCase() === normalized.toLocaleLowerCase(),
    )
  ) {
    throw new Error(`Profile 名称“${normalized}”已存在。`);
  }
  return normalized;
}

function previewId(): string {
  nextProfile += 1;
  return `00000000-0000-4000-9000-${String(nextProfile).padStart(12, "0")}`;
}

export function getPreviewProfiles(): Profile[] {
  return snapshot();
}

export function isPreviewModEnabled(modId: string): boolean {
  const active = profiles.find((profile) => profile.id === activeProfileId);
  return active?.mods.some((item) => item.modId === modId && item.enabled) ?? false;
}

export function removePreviewModReferences(modIds: string[]): void {
  const removed = new Set(modIds);
  profiles = profiles.map((profile) => ({
    ...profile,
    mods: profile.mods
      .filter((item) => !removed.has(item.modId))
      .map((item, index) => ({ ...item, loadOrder: index })),
  }));
}

export function createPreviewProfile(name: string): Profile {
  const now = Math.floor(Date.now() / 1000);
  const profile: Profile = {
    id: previewId(),
    name: ensureName(name),
    isActive: false,
    createdAt: now,
    updatedAt: now,
    mods: [],
  };
  profiles = [...profiles, profile];
  return { ...profile, mods: [] };
}

export function renamePreviewProfile(profileId: string, name: string): Profile {
  const profile = requireProfile(profileId);
  profile.name = ensureName(name, profileId);
  profile.updatedAt = Math.floor(Date.now() / 1000);
  return {
    ...profile,
    isActive: profileId === activeProfileId,
    mods: profile.mods.map((item) => ({ ...item })),
  };
}

export function copyPreviewProfile(sourceProfileId: string, name: string): Profile {
  const source = requireProfile(sourceProfileId);
  const now = Math.floor(Date.now() / 1000);
  const profile: Profile = {
    id: previewId(),
    name: ensureName(name),
    isActive: false,
    createdAt: now,
    updatedAt: now,
    mods: source.mods.map((item) => ({ ...item })),
  };
  profiles = [...profiles, profile];
  return { ...profile, mods: profile.mods.map((item) => ({ ...item })) };
}

export function deletePreviewProfile(profileId: string): void {
  if (profileId === activeProfileId) throw new Error("不能删除当前正在使用的 Profile。");
  requireProfile(profileId);
  profiles = profiles.filter((item) => item.id !== profileId);
}

export function reorderPreviewProfile(profileId: string, modIds: string[]): Profile {
  const profile = requireProfile(profileId);
  const enabled = profile.mods.filter((item) => item.enabled);
  if (
    enabled.length !== modIds.length ||
    new Set(enabled.map((item) => item.modId)).size !== new Set(modIds).size ||
    modIds.some((modId) => !enabled.some((item) => item.modId === modId))
  ) {
    throw new Error("加载顺序必须包含该 Profile 的全部启用模组。");
  }
  const slots = enabled.map((item) => item.loadOrder).sort((left, right) => left - right);
  const positions = new Map(modIds.map((modId, index) => [modId, slots[index] ?? index]));
  profile.mods = profile.mods
    .map((item) =>
      item.enabled ? { ...item, loadOrder: positions.get(item.modId) ?? item.loadOrder } : item,
    )
    .sort((left, right) => left.loadOrder - right.loadOrder);
  profile.updatedAt = Math.floor(Date.now() / 1000);
  return {
    ...profile,
    isActive: profileId === activeProfileId,
    mods: profile.mods.map((item) => ({ ...item })),
  };
}

export function switchPreviewProfile(profileId: string): ProfileSwitchResult {
  const source = requireProfile(activeProfileId);
  const target = requireProfile(profileId);
  activeProfileId = profileId;
  return {
    profile: { ...target, isActive: true, mods: target.mods.map((item) => ({ ...item })) },
    disabledMods: source.mods.filter((item) => item.enabled).length,
    enabledMods: target.mods.filter((item) => item.enabled).length,
    guidance: "浏览器预览不会修改真实 EFMI 文件。",
    warnings: [],
  };
}
