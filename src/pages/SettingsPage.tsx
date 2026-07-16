import { open } from "@tauri-apps/plugin-dialog";
import {
  CheckCircle2,
  Database,
  ExternalLink,
  FileClock,
  FolderCog,
  Gamepad2,
  Languages,
  LoaderCircle,
  Palette,
  RefreshCw,
  Search,
  TriangleAlert,
} from "lucide-react";
import type { ChangeEvent } from "react";
import { PageHeader } from "../components/ui/PageHeader";
import { useAppBootstrap } from "../features/bootstrap/useAppBootstrap";
import {
  useDetectGameInstallations,
  useGameStatus,
  useOpenGameDirectory,
  useSetEfmiLoaderRoot,
  useSetGameInstallation,
  useSetGameLaunchMode,
} from "../features/game/useGameManager";
import { commandErrorMessage } from "../lib/tauri";
import type { DetectedGameInstallation, GameEdition, GameLaunchMode } from "../types/app";

function displayPath(value: string | null | undefined) {
  return value && value.length > 0 ? value : "尚未设置";
}

function editionLabel(edition: GameEdition | null | undefined) {
  if (edition === "china") return "国服";
  if (edition === "international") return "国际服";
  return "版本区域待确认";
}

function discoverySourceLabel(source: DetectedGameInstallation["source"]) {
  const labels: Record<DetectedGameInstallation["source"], string> = {
    configuredPath: "已保存路径",
    launcherRegistry: "鹰角启动器注册表",
    knownInstallRoot: "常见安装位置",
    manualSelection: "手动选择",
  };
  return labels[source];
}

export function SettingsPage() {
  const bootstrap = useAppBootstrap();
  const gameStatus = useGameStatus();
  const detect = useDetectGameInstallations();
  const configureGame = useSetGameInstallation();
  const configureLoader = useSetEfmiLoaderRoot();
  const setLaunchMode = useSetGameLaunchMode();
  const openDirectory = useOpenGameDirectory();

  const desktopReady = bootstrap.data?.runtimeMode === "desktop";
  const settings = bootstrap.data?.settings;
  const installation = gameStatus.data?.installation?.installation;
  const loader = gameStatus.data?.loader;
  const busy =
    detect.isPending ||
    configureGame.isPending ||
    configureLoader.isPending ||
    setLaunchMode.isPending;
  const operationError =
    configureGame.error ??
    configureLoader.error ??
    setLaunchMode.error ??
    detect.error ??
    openDirectory.error ??
    gameStatus.error;

  async function chooseDirectory(title: string) {
    if (!desktopReady) return null;
    return open({ directory: true, multiple: false, title });
  }

  async function handleChooseGame() {
    try {
      const selected = await chooseDirectory("选择《明日方舟：终末地》游戏目录");
      if (typeof selected === "string") {
        await configureGame.mutateAsync(selected);
      }
    } catch {
      // The mutation or dialog state renders the actionable error below.
    }
  }

  async function handleChooseLoader() {
    try {
      const selected = await chooseDirectory("选择 EFMI 加载器目录");
      if (typeof selected === "string") {
        await configureLoader.mutateAsync(selected);
      }
    } catch {
      // The mutation or dialog state renders the actionable error below.
    }
  }

  async function handleDetect() {
    try {
      const candidates = await detect.mutateAsync();
      if (candidates.length === 1) {
        const path = candidates[0]?.validation.installation?.installationRoot;
        if (path) await configureGame.mutateAsync(path);
      }
    } catch {
      // The mutation state renders the actionable error below.
    }
  }

  function handleLaunchModeChange(event: ChangeEvent<HTMLSelectElement>) {
    setLaunchMode.mutate(event.target.value as GameLaunchMode);
  }

  return (
    <div className="page-stack">
      <PageHeader
        eyebrow="应用设置"
        title="设置"
        description="机器相关路径保存在 config.json；保存前由后端重新规范化并验证目录身份。"
      />

      <section className="game-settings-panel" aria-labelledby="game-settings-title">
        <div className="panel__header">
          <div>
            <span className="eyebrow">游戏管理</span>
            <h2 id="game-settings-title">终末地与 EFMI</h2>
          </div>
          <span
            className={`validation-badge ${gameStatus.data?.configured ? "is-valid" : "is-warning"}`}
          >
            {gameStatus.data?.configured ? <CheckCircle2 size={14} /> : <TriangleAlert size={14} />}
            {gameStatus.data?.configured ? "游戏目录有效" : "等待有效路径"}
          </span>
        </div>

        <div className="path-setting-row">
          <div className="setting-card__icon">
            <Gamepad2 size={20} />
          </div>
          <div className="path-setting-row__content">
            <strong>游戏安装目录</strong>
            <span className="path-value">{displayPath(installation?.installationRoot)}</span>
            <small>
              {installation
                ? `${editionLabel(installation.edition)} · 置信度 ${gameStatus.data?.installation?.confidence ?? 0}%`
                : "需要包含 Endfield.exe 与匹配的 Endfield_Data/app.info。"}
            </small>
          </div>
          <div className="path-setting-row__actions">
            <button
              className="button button--secondary"
              type="button"
              disabled={!desktopReady || busy}
              onClick={() => void handleDetect()}
            >
              <Search size={16} />
              自动检测
            </button>
            <button
              className="button button--secondary"
              type="button"
              disabled={!desktopReady || busy}
              onClick={() => void handleChooseGame()}
            >
              <FolderCog size={16} />
              选择目录
            </button>
            <button
              className="icon-button icon-button--active"
              type="button"
              title="打开游戏目录"
              aria-label="打开游戏目录"
              disabled={!gameStatus.data?.configured || openDirectory.isPending}
              onClick={() => openDirectory.mutate()}
            >
              <ExternalLink size={16} />
            </button>
          </div>
        </div>

        <div className="path-setting-row">
          <div className="setting-card__icon">
            <LoaderCircle size={20} />
          </div>
          <div className="path-setting-row__content">
            <strong>EFMI 加载器目录</strong>
            <span className="path-value">{displayPath(loader?.root)}</span>
            <small className={loader && !loader.launchReady ? "text-warning" : undefined}>
              {loader
                ? loader.launchReady
                  ? "加载器有效，且 d3dx.ini 的 launch 路径与当前游戏一致。"
                  : loader.issues[0]
                : "可选；需要 3DMigotoLoader.exe、d3d11.dll、d3dx.ini 与 Mods。"}
            </small>
          </div>
          <div className="path-setting-row__actions">
            {loader ? (
              <button
                className="button button--ghost"
                type="button"
                disabled={busy}
                onClick={() => configureLoader.mutate(null)}
              >
                清除
              </button>
            ) : null}
            <button
              className="button button--secondary"
              type="button"
              disabled={!desktopReady || !gameStatus.data?.configured || busy}
              onClick={() => void handleChooseLoader()}
            >
              <FolderCog size={16} />
              选择目录
            </button>
          </div>
        </div>

        <div className="launch-mode-row">
          <div>
            <strong>启动方式</strong>
            <small>{gameStatus.data?.launchBlockReason ?? "当前启动方式已通过校验。"}</small>
          </div>
          <select
            className="select-field"
            aria-label="游戏启动方式"
            value={gameStatus.data?.launchMode ?? settings?.game.launchMode ?? "efmiLoader"}
            disabled={!desktopReady || busy}
            onChange={handleLaunchModeChange}
          >
            <option value="game">直接启动 Endfield.exe</option>
            <option value="efmiLoader">通过 EFMI / 3DMigotoLoader</option>
            <option value="externalLauncher" disabled>
              外部启动器（待适配）
            </option>
          </select>
        </div>

        {detect.isSuccess && detect.data.length === 0 ? (
          <p className="inline-notice">未在已验证的常见位置找到游戏，请手动选择安装目录。</p>
        ) : null}
        {detect.data && detect.data.length > 1 ? (
          <div className="detection-results">
            <span className="eyebrow">检测结果</span>
            {detect.data.map((candidate) => {
              const item = candidate.validation.installation;
              if (!item) return null;
              return (
                <button
                  type="button"
                  className="detection-result"
                  key={item.installationRoot}
                  onClick={() => configureGame.mutate(item.installationRoot)}
                >
                  <span>
                    <strong>{discoverySourceLabel(candidate.source)}</strong>
                    <small>{item.installationRoot}</small>
                  </span>
                  <span>{candidate.validation.confidence}%</span>
                </button>
              );
            })}
          </div>
        ) : null}
        {operationError ? (
          <p className="inline-error">{commandErrorMessage(operationError)}</p>
        ) : null}
      </section>

      <section className="settings-grid">
        <article className="setting-card setting-card--wide">
          <div className="setting-card__icon">
            <FolderCog size={20} />
          </div>
          <div className="setting-card__body">
            <span className="eyebrow">存储</span>
            <h2>模组仓库</h2>
            <p className="path-value">{displayPath(settings?.storage.repositoryPath)}</p>
            <p className="path-value path-value--muted">
              临时目录：{displayPath(settings?.storage.stagingPath)}
            </p>
          </div>
          <button className="button button--secondary" type="button" disabled>
            更改位置
          </button>
        </article>

        <article className="setting-card">
          <div className="setting-card__icon">
            <Palette size={20} />
          </div>
          <div className="setting-card__body">
            <span className="eyebrow">外观</span>
            <h2>深色主题</h2>
            <p>为桌面大列表和长时间使用优化。</p>
          </div>
        </article>

        <article className="setting-card">
          <div className="setting-card__icon">
            <Languages size={20} />
          </div>
          <div className="setting-card__body">
            <span className="eyebrow">语言</span>
            <h2>{settings?.language ?? "zh-CN"}</h2>
            <p>本地化框架将在 Phase 9 接入。</p>
          </div>
        </article>

        <article className="setting-card">
          <div className="setting-card__icon">
            <Database size={20} />
          </div>
          <div className="setting-card__body">
            <span className="eyebrow">数据库</span>
            <h2>{bootstrap.data?.databaseReady ? "SQLite 已就绪" : "等待桌面运行时"}</h2>
            <p className="path-value">{displayPath(bootstrap.data?.databasePath)}</p>
          </div>
        </article>

        <article className="setting-card">
          <div className="setting-card__icon">
            <FileClock size={20} />
          </div>
          <div className="setting-card__body">
            <span className="eyebrow">日志</span>
            <h2>{settings?.logLevel ?? "info"}</h2>
            <p className="path-value">{displayPath(bootstrap.data?.logDirectory)}</p>
          </div>
        </article>
      </section>

      {busy ? (
        <div className="floating-progress" role="status">
          <RefreshCw size={15} className="spin" />
          正在验证本地路径…
        </div>
      ) : null}
    </div>
  );
}
