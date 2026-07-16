import { invoke, isTauri } from "@tauri-apps/api/core";
import { open } from "@tauri-apps/plugin-dialog";
import type {
  AppBootstrap,
  AppSettings,
  DetectedGameInstallation,
  GameLaunchMode,
  GameLaunchResult,
  GameStatus,
  LocalModMetadata,
  ModDetails,
  ModImportPlan,
  ModInstallResult,
  ModListItem,
  ModMutationResult,
  ModPreview,
  ModScanResult,
} from "../types/app";
import {
  getPreviewDetails,
  getPreviewImage,
  getPreviewMods,
  setPreviewFavorites,
  updatePreviewMetadata,
} from "./previewMods";

const previewSettings: AppSettings = {
  schemaVersion: 1,
  language: "zh-CN",
  theme: "dark",
  game: {
    adapterId: "endfield.local",
    edition: null,
    installationPath: null,
    loaderRoot: null,
    launchMode: "efmiLoader",
  },
  storage: {
    repositoryPath: "仅桌面模式可用",
    stagingPath: "仅桌面模式可用",
  },
  logLevel: "info",
};

function browserPreviewBootstrap(): AppBootstrap {
  return {
    appName: "Endfield Mod Manager",
    appVersion: "0.1.0-preview",
    runtimeMode: "browserPreview",
    databaseReady: false,
    configPath: "仅桌面模式可用",
    databasePath: "仅桌面模式可用",
    logDirectory: "仅桌面模式可用",
    settings: previewSettings,
  };
}

export async function getAppBootstrap(): Promise<AppBootstrap> {
  if (!isTauri()) {
    return browserPreviewBootstrap();
  }

  return invoke<AppBootstrap>("get_app_bootstrap");
}

export async function updateSettings(settings: AppSettings): Promise<AppSettings> {
  return invoke<AppSettings>("update_settings", { settings });
}

const previewGameStatus: GameStatus = {
  configured: false,
  installation: null,
  loader: null,
  launchMode: "efmiLoader",
  canLaunch: false,
  launchBlockReason: "仅桌面模式可以管理游戏路径。",
};

function requireDesktop(): void {
  if (!isTauri()) {
    throw new Error("该操作仅在 AEMM 桌面应用中可用。");
  }
}

export async function getGameStatus(): Promise<GameStatus> {
  if (!isTauri()) {
    return previewGameStatus;
  }
  return invoke<GameStatus>("get_game_status");
}

export async function detectGameInstallations(): Promise<DetectedGameInstallation[]> {
  requireDesktop();
  return invoke<DetectedGameInstallation[]>("detect_game_installations");
}

export async function setGameInstallation(path: string): Promise<GameStatus> {
  requireDesktop();
  return invoke<GameStatus>("set_game_installation", { path });
}

export async function setEfmiLoaderRoot(path: string | null): Promise<GameStatus> {
  requireDesktop();
  return invoke<GameStatus>("set_efmi_loader_root", { path });
}

export async function setGameLaunchMode(launchMode: GameLaunchMode): Promise<GameStatus> {
  requireDesktop();
  return invoke<GameStatus>("set_game_launch_mode", { launchMode });
}

export async function openGameDirectory(): Promise<void> {
  requireDesktop();
  return invoke<void>("open_game_directory");
}

export async function launchGame(): Promise<GameLaunchResult> {
  requireDesktop();
  return invoke<GameLaunchResult>("launch_game");
}

export function commandErrorMessage(error: unknown): string {
  if (error instanceof Error) {
    return error.message;
  }
  if (typeof error === "object" && error !== null && "message" in error) {
    const message = (error as { message?: unknown }).message;
    if (typeof message === "string") {
      return message;
    }
  }
  return "操作失败，请检查本地日志。";
}

export async function scanModRepository(): Promise<ModScanResult> {
  requireDesktop();
  return invoke<ModScanResult>("scan_mod_repository");
}

export async function selectModArchive(): Promise<string | null> {
  requireDesktop();
  const selected = await open({
    multiple: false,
    directory: false,
    title: "选择模组压缩包",
    filters: [{ name: "模组压缩包", extensions: ["zip", "7z", "rar"] }],
  });
  return typeof selected === "string" ? selected : null;
}

export async function selectModDirectory(): Promise<string | null> {
  requireDesktop();
  const selected = await open({
    multiple: false,
    directory: true,
    title: "选择模组文件夹",
  });
  return typeof selected === "string" ? selected : null;
}

export async function prepareModImport(sourcePath: string): Promise<ModImportPlan> {
  requireDesktop();
  return invoke<ModImportPlan>("prepare_mod_import", {
    request: { sourcePath },
  });
}

export async function commitModImport(operationId: string): Promise<ModInstallResult> {
  requireDesktop();
  return invoke<ModInstallResult>("commit_mod_import", {
    request: { operationId },
  });
}

export async function cancelModImport(operationId: string): Promise<void> {
  requireDesktop();
  return invoke<void>("cancel_mod_import", {
    request: { operationId },
  });
}

export async function listInstalledMods(): Promise<ModListItem[]> {
  if (!isTauri()) return getPreviewMods();
  return invoke<ModListItem[]>("list_installed_mods");
}

export async function getModDetails(modId: string): Promise<ModDetails> {
  if (!isTauri()) {
    const details = getPreviewDetails(modId);
    if (!details) throw new Error("模组不存在。");
    return details;
  }
  return invoke<ModDetails>("get_mod_details", { modId });
}

export async function setModFavorite(
  modIds: string[],
  favorite: boolean,
): Promise<ModMutationResult> {
  if (!isTauri()) {
    setPreviewFavorites(modIds, favorite);
    return { updated: modIds.length };
  }
  return invoke<ModMutationResult>("set_mod_favorite", {
    request: { modIds, favorite },
  });
}

export async function getModPreview(modId: string): Promise<ModPreview | null> {
  if (!isTauri()) return getPreviewImage(modId);
  return invoke<ModPreview | null>("get_mod_preview", { modId });
}

export async function openModDirectory(modId: string): Promise<void> {
  requireDesktop();
  return invoke<void>("open_mod_directory", { modId });
}

export async function updateLocalModMetadata(
  modId: string,
  metadata: LocalModMetadata,
): Promise<ModListItem> {
  if (!isTauri()) {
    const item = updatePreviewMetadata(modId, metadata);
    if (!item) throw new Error("模组不存在。");
    return item;
  }
  return invoke<ModListItem>("update_local_mod_metadata", {
    request: { modId, metadata },
  });
}
