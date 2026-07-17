import type {
  ConflictReport,
  LocalModMetadata,
  ModDetails,
  ModListItem,
  ModPreview,
} from "../types/app";
import { isPreviewModEnabled, removePreviewModReferences } from "./previewProfiles";

const names = [
  "佩丽卡 · 深海礼装",
  "狼卫战术外观",
  "终末地清晰材质包",
  "夜间摄影光照",
  "工业区低饱和滤镜",
  "武器模型重制",
  "界面字体优化",
  "探索服配色方案",
] as const;
const categories = ["角色", "材质", "光影", "武器", "界面"] as const;
const favoriteOverrides = new Map<string, boolean>();
const metadataOverrides = new Map<string, LocalModMetadata>();
const removedMods = new Set<string>();

function idFor(index: number): string {
  return `00000000-0000-4000-8000-${String(index + 1).padStart(12, "0")}`;
}

function baseMods(): ModListItem[] {
  return Array.from({ length: 48 }, (_, index): ModListItem => {
    const name = names[index % names.length] ?? "终末地模组";
    const category = categories[index % categories.length] ?? "其他";
    const id = idFor(index);
    return {
      id,
      logicalId: `preview.author.mod_${String(index + 1).padStart(2, "0")}`,
      repositoryPath: `preview-mod-${String(index + 1).padStart(2, "0")}`,
      name: `${name}${index >= names.length ? ` ${Math.floor(index / names.length) + 1}` : ""}`,
      author: ["Rhodes Lab", "Endfield Studio", "Aki", "Tundra"][index % 4] ?? "未知作者",
      version: `1.${index % 6}.${index % 3}`,
      description: "用于浏览器预览的模组数据。桌面模式会显示 SQLite 中的真实扫描结果。",
      category,
      previewPath: "preview.png",
      favorite: favoriteOverrides.get(id) ?? index % 5 === 0,
      enabled: isPreviewModEnabled(id),
      sizeBytes: 2_400_000 + index * 731_113,
      fileCount: 8 + (index % 27),
      installedAt: 1_752_000_000 - index * 86_400,
      updatedAt: 1_752_400_000 - (index % 11) * 43_200,
      lifecycleState: index % 17 === 0 ? "broken" : "installed",
    };
  }).filter((item) => !removedMods.has(item.id));
}

export function getPreviewMods(): ModListItem[] {
  return baseMods().map((item) => {
    const local = metadataOverrides.get(item.id);
    return local
      ? {
          ...item,
          name: local.displayNameOverride ?? item.name,
          description: local.descriptionOverride ?? item.description,
          category: local.categoryOverride ?? item.category,
          favorite: local.favorite,
        }
      : item;
  });
}

export function getPreviewConflictReport(): ConflictReport {
  const enabled = getPreviewMods().filter((item) => item.enabled);
  const first = enabled[0];
  const second = enabled[1];
  const participants = [first, second]
    .filter((item): item is ModListItem => Boolean(item))
    .map((item, index) => ({
      modId: item.id,
      modName: item.name,
      loadOrder: index,
      evidence: [
        {
          sourcePath: "mod.ini",
          section: `[TextureOverride_Component${index}]`,
          detail: "hash=48e5c5f7, match_index_count=120, handling=skip",
        },
      ],
    }));
  return {
    profileId: "00000000-0000-0000-0000-000000000001",
    generatedAt: 1_752_400_000,
    enabledMods: enabled.length,
    analyzedIniFiles: enabled.length,
    affectedMods: participants.length,
    conflicts:
      participants.length >= 2
        ? [
            {
              id: "conflict-preview-texture",
              analyzerId: "efmi.ini.v1",
              kind: "efmiTextureOverride",
              severity: "warning",
              resourceKey: "texture-hash:48e5c5f7",
              summary: "多个已启用模组可能匹配同一 TextureOverride 资源 Hash。",
              participants,
              winningModId: null,
            },
          ]
        : [],
    loadOrderVerified: false,
    loadOrderNote:
      "列表显示 AEMM Profile 中保存的顺序；当前 EFMI 实际胜出规则尚未可靠验证。",
    warnings: [],
  };
}

export function setPreviewFavorites(modIds: string[], favorite: boolean): void {
  for (const modId of modIds) favoriteOverrides.set(modId, favorite);
}

export function removePreviewMods(modIds: string[]): number {
  const current = new Map(getPreviewMods().map((item) => [item.id, item]));
  for (const modId of modIds) {
    const item = current.get(modId);
    if (!item) throw new Error(`Preview mod ${modId} does not exist.`);
    if (item.enabled) throw new Error("Enabled preview mods must be disabled before uninstalling.");
  }
  for (const modId of modIds) {
    removedMods.add(modId);
    favoriteOverrides.delete(modId);
    metadataOverrides.delete(modId);
  }
  removePreviewModReferences(modIds);
  return new Set(modIds).size;
}

export function getPreviewDetails(modId: string): ModDetails | null {
  const item = getPreviewMods().find((candidate) => candidate.id === modId);
  if (!item) return null;
  const localMetadata = metadataOverrides.get(modId) ?? {
    displayNameOverride: null,
    categoryOverride: null,
    descriptionOverride: null,
    favorite: item.favorite,
    notes: null,
    tags: item.category ? [item.category] : [],
  };
  return {
    item,
    authorName: names[(Number(modId.slice(-4)) - 1) % names.length] ?? item.name,
    authorDescription: "作者提供的原始说明会保存在这里，本地编辑不会修改作者的 mod.json。",
    authorCategory: item.category,
    gameVersion: null,
    website: "https://example.invalid/mod",
    metadataSource: Number(modId.slice(-2)) % 3 === 0 ? "inferred" : "modJson",
    localMetadata,
    files: Array.from({ length: item.fileCount }, (_, index) => ({
      sourcePath: index === 0 ? "mod.json" : `Assets/part-${String(index).padStart(3, "0")}.buf`,
      sizeBytes: Math.max(128, Math.floor(item.sizeBytes / item.fileCount)),
      contentHash: `${modId.replaceAll("-", "")}${String(index).padStart(32, "0")}`.slice(0, 64),
      fileRole: index === 0 ? "metadata" : index % 4 === 0 ? "texture" : "content",
      modifiedAtMs: item.updatedAt * 1000,
    })),
  };
}

export function updatePreviewMetadata(
  modId: string,
  metadata: LocalModMetadata,
): ModListItem | null {
  metadataOverrides.set(modId, metadata);
  favoriteOverrides.set(modId, metadata.favorite);
  return getPreviewMods().find((candidate) => candidate.id === modId) ?? null;
}

export function getPreviewImage(modId: string): ModPreview {
  const item = getPreviewMods().find((candidate) => candidate.id === modId);
  const label = escapeXml(item?.name.slice(0, 10) ?? "AEMM");
  const hue = Number(modId.slice(-4)) * 37 % 360;
  const svg = `<svg xmlns="http://www.w3.org/2000/svg" width="720" height="420" viewBox="0 0 720 420"><defs><linearGradient id="g" x1="0" y1="0" x2="1" y2="1"><stop stop-color="hsl(${hue} 42% 24%)"/><stop offset="1" stop-color="#11141b"/></linearGradient></defs><rect width="720" height="420" fill="url(#g)"/><circle cx="580" cy="80" r="150" fill="hsl(${hue} 70% 65% / .16)"/><path d="M80 316h560" stroke="white" stroke-opacity=".12"/><text x="64" y="294" fill="white" font-family="Segoe UI, sans-serif" font-size="38" font-weight="600">${label}</text><text x="66" y="337" fill="white" fill-opacity=".52" font-family="Segoe UI, sans-serif" font-size="16">END_FIELD / MOD ARCHIVE</text></svg>`;
  return { dataUrl: `data:image/svg+xml;charset=utf-8,${encodeURIComponent(svg)}` };
}

function escapeXml(value: string): string {
  return value.replace(/[&<>"']/g, (character) => {
    const entities: Record<string, string> = {
      "&": "&amp;",
      "<": "&lt;",
      ">": "&gt;",
      '"': "&quot;",
      "'": "&apos;",
    };
    return entities[character] ?? "";
  });
}
