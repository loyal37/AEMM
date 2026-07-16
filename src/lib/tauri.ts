import { invoke, isTauri } from "@tauri-apps/api/core";
import type {
  AppBootstrap,
  AppSettings,
  DetectedGameInstallation,
  GameLaunchMode,
  GameLaunchResult,
  GameStatus,
} from "../types/app";

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
