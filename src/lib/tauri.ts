import { invoke, isTauri } from "@tauri-apps/api/core";
import type { AppBootstrap, AppSettings } from "../types/app";

const previewSettings: AppSettings = {
  schemaVersion: 1,
  language: "zh-CN",
  theme: "dark",
  game: {
    adapterId: "endfield.efmi",
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
