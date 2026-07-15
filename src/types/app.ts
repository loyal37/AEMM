export type ThemePreference = "dark" | "system";

export interface GameSettings {
  adapterId: string;
  edition: string | null;
  installationPath: string | null;
  loaderRoot: string | null;
  launchMode: "game" | "efmiLoader" | "externalLauncher";
}

export interface StorageSettings {
  repositoryPath: string;
  stagingPath: string;
}

export interface AppSettings {
  schemaVersion: number;
  language: string;
  theme: ThemePreference;
  game: GameSettings;
  storage: StorageSettings;
  logLevel: "error" | "warn" | "info" | "debug" | "trace";
}

export interface AppBootstrap {
  appName: string;
  appVersion: string;
  runtimeMode: "desktop" | "browserPreview";
  databaseReady: boolean;
  configPath: string;
  databasePath: string;
  logDirectory: string;
  settings: AppSettings;
}

export interface CommandError {
  code: string;
  message: string;
  details?: Record<string, unknown> | null;
}
