export type ThemePreference = "dark" | "system";
export type GameEdition = "china" | "international" | "unknown";
export type GameLaunchMode = "game" | "efmiLoader" | "externalLauncher";

export interface GameSettings {
  adapterId: string;
  edition: string | null;
  installationPath: string | null;
  loaderRoot: string | null;
  launchMode: GameLaunchMode;
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

export interface GameVersionInfo {
  value: string | null;
  source: string | null;
  note: string;
}

export interface GameInstallation {
  adapterId: string;
  edition: GameEdition;
  installationRoot: string;
  executable: string;
  loaderRoot: string | null;
  version: GameVersionInfo;
}

export interface GameValidation {
  valid: boolean;
  confidence: number;
  evidence: string[];
  issues: string[];
  installation: GameInstallation | null;
}

export interface DetectedGameInstallation {
  source: "configuredPath" | "launcherRegistry" | "knownInstallRoot" | "manualSelection";
  validation: GameValidation;
}

export interface EfmiValidation {
  valid: boolean;
  launchReady: boolean;
  root: string | null;
  executable: string | null;
  configuredGameExecutable: string | null;
  evidence: string[];
  issues: string[];
}

export interface GameStatus {
  configured: boolean;
  installation: GameValidation | null;
  loader: EfmiValidation | null;
  launchMode: GameLaunchMode;
  canLaunch: boolean;
  launchBlockReason: string | null;
}

export interface GameLaunchResult {
  processId: number;
  mode: GameLaunchMode;
}
