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

export type ModLifecycleState = "installing" | "installed" | "broken" | "removing";

export interface LocalModMetadata {
  displayNameOverride: string | null;
  categoryOverride: string | null;
  descriptionOverride: string | null;
  favorite: boolean;
  notes: string | null;
  tags: string[];
}

export interface ModListItem {
  id: string;
  logicalId: string;
  repositoryPath: string;
  name: string;
  author: string | null;
  version: string | null;
  description: string | null;
  category: string | null;
  previewPath: string | null;
  favorite: boolean;
  sizeBytes: number;
  fileCount: number;
  installedAt: number;
  updatedAt: number;
  lifecycleState: ModLifecycleState;
}

export interface ModScanResult {
  discovered: number;
  added: number;
  updated: number;
  unchanged: number;
  broken: number;
  missing: number;
  hashedFiles: number;
  reusedHashes: number;
  skippedEntries: number;
  durationMs: number;
  issues: string[];
}

export type MetadataSourceKind = "modJson" | "inferred";

export interface ModFileDetails {
  sourcePath: string;
  sizeBytes: number;
  contentHash: string | null;
  fileRole: string;
  modifiedAtMs: number;
}

export interface ModDetails {
  item: ModListItem;
  authorName: string;
  authorDescription: string | null;
  authorCategory: string | null;
  gameVersion: string | null;
  website: string | null;
  metadataSource: MetadataSourceKind;
  localMetadata: LocalModMetadata;
  files: ModFileDetails[];
}

export interface ModMutationResult {
  updated: number;
}

export interface ModPreview {
  dataUrl: string;
}

export type ModImportSourceKind = "zip" | "sevenZip" | "rar" | "directory";

export interface ModImportPlan {
  operationId: string;
  sourceKind: ModImportSourceKind;
  sourceName: string;
  logicalId: string;
  name: string;
  author: string | null;
  version: string | null;
  description: string | null;
  category: string | null;
  fileCount: number;
  sizeBytes: number;
  contentFingerprint: string;
  destinationRelativePath: string;
  warnings: string[];
  blockingIssues: string[];
  canInstall: boolean;
}

export type ModInstallProgressStage =
  | "inspecting"
  | "extracting"
  | "analyzing"
  | "ready"
  | "committing"
  | "synchronizing"
  | "rollingBack"
  | "completed";

export interface ModInstallProgress {
  operationId: string;
  stage: ModInstallProgressStage;
  message: string;
  processedItems: number;
  totalItems: number | null;
  processedBytes: number;
  totalBytes: number | null;
}

export interface ModInstallResult {
  operationId: string;
  modId: string;
  name: string;
  repositoryPath: string;
}
