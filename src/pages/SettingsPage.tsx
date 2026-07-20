import { open } from "@tauri-apps/plugin-dialog";
import {
  CheckCircle2,
  Database,
  FileClock,
  FolderCog,
  HardDrive,
  Languages,
  LoaderCircle,
  Palette,
  TriangleAlert,
} from "lucide-react";
import { PageHeader } from "../components/ui/PageHeader";
import { useAppBootstrap, useUpdateAppSettings } from "../features/bootstrap/useAppBootstrap";
import { useOnboarding } from "../features/experience/AppExperience";
import { useSetEfmiModsDirectory } from "../features/efmi/useEfmiManager";
import { commandErrorMessage } from "../lib/tauri";
import type { AppSettings } from "../types/app";

function displayPath(value: string | null | undefined) {
  return value && value.length > 0 ? value : "尚未设置";
}

export function SettingsPage() {
  const bootstrap = useAppBootstrap();
  const configureEfmi = useSetEfmiModsDirectory();
  const updatePreferences = useUpdateAppSettings();
  const onboarding = useOnboarding();
  const desktopReady = bootstrap.data?.runtimeMode === "desktop";
  const settings = bootstrap.data?.settings;
  const modsConfigured = Boolean(settings?.game.loaderRoot);
  const busy = configureEfmi.isPending || updatePreferences.isPending;
  const operationError = configureEfmi.error ?? updatePreferences.error ?? bootstrap.error;

  function updatePreference(patch: Partial<AppSettings>) {
    if (settings) updatePreferences.mutate({ ...settings, ...patch });
  }

  async function chooseEfmiMods() {
    if (!desktopReady) return;
    try {
      const selected = await open({
        directory: true,
        multiple: false,
        title: "选择 EFMI 根目录或 EFMI/Mods 目录",
      });
      if (typeof selected === "string") await configureEfmi.mutateAsync(selected);
    } catch {
      // The mutation renders a user-facing error below.
    }
  }

  return (
    <div className="page-stack">
      <PageHeader
        eyebrow="应用设置"
        title="设置"
        description="AEMM 只管理 EFMI Mods；SQLite、日志和临时文件固定保存在软件目录的 data 文件夹。"
      />

      <section className="game-settings-panel" aria-labelledby="efmi-settings-title">
        <div className="panel__header">
          <div><span className="eyebrow">模组目录</span><h2 id="efmi-settings-title">EFMI Mods</h2></div>
          <span className={`validation-badge ${modsConfigured ? "is-valid" : "is-warning"}`}>
            {modsConfigured ? <CheckCircle2 size={14} /> : <TriangleAlert size={14} />}
            {modsConfigured ? "已配置" : "等待选择"}
          </span>
        </div>
        <div className="path-setting-row">
          <div className="setting-card__icon"><FolderCog size={20} /></div>
          <div className="path-setting-row__content">
            <strong>直接管理的 Mods 文件夹</strong>
            <p className="path-value" title={settings?.storage.repositoryPath}>
              {displayPath(modsConfigured ? settings?.storage.repositoryPath : null)}
            </p>
            <small>普通目录表示启用；名称以 DISABLED 开头表示禁用。启停只做同目录原子重命名。</small>
          </div>
          <div className="path-setting-row__actions">
            <button className="button button--primary" type="button" disabled={!desktopReady || busy} onClick={() => void chooseEfmiMods()}>
              {configureEfmi.isPending ? <LoaderCircle size={16} className="spin" /> : <FolderCog size={16} />}
              {modsConfigured ? "更改目录" : "选择目录"}
            </button>
          </div>
        </div>
        <p className="settings-safety-note">
          AEMM 不检测游戏、不修改游戏路径，也不提供游戏或加载器启动功能。
        </p>
      </section>

      {operationError ? <p className="inline-error">{commandErrorMessage(operationError)}</p> : null}
      {configureEfmi.isSuccess ? <p className="inline-success">EFMI Mods 已连接；前往模组页面执行扫描即可同步实际内容。</p> : null}

      <section className="settings-grid">
        <article className="setting-card">
          <div className="setting-card__icon"><HardDrive size={20} /></div>
          <div className="setting-card__body">
            <span className="eyebrow">便携数据</span><h2>配置文件</h2>
            <p className="path-value" title={bootstrap.data?.configPath}>{displayPath(bootstrap.data?.configPath)}</p>
          </div>
        </article>
        <article className="setting-card">
          <div className="setting-card__icon"><Database size={20} /></div>
          <div className="setting-card__body">
            <span className="eyebrow">便携数据</span><h2>{bootstrap.data?.databaseReady ? "SQLite 已就绪" : "等待桌面运行时"}</h2>
            <p className="path-value" title={bootstrap.data?.databasePath}>{displayPath(bootstrap.data?.databasePath)}</p>
          </div>
        </article>
        <article className="setting-card">
          <div className="setting-card__icon"><FileClock size={20} /></div>
          <div className="setting-card__body">
            <span className="eyebrow">便携数据</span><h2>日志目录</h2>
            <p className="path-value" title={bootstrap.data?.logDirectory}>{displayPath(bootstrap.data?.logDirectory)}</p>
            <select className="select-field preference-select" aria-label="日志级别" value={settings?.logLevel ?? "info"} disabled={!settings || busy} onChange={(event) => updatePreference({ logLevel: event.target.value as AppSettings["logLevel"] })}>
              <option value="error">error</option><option value="warn">warn</option><option value="info">info</option><option value="debug">debug</option><option value="trace">trace</option>
            </select>
          </div>
        </article>
        <article className="setting-card">
          <div className="setting-card__icon"><Palette size={20} /></div>
          <div className="setting-card__body">
            <span className="eyebrow">外观</span><h2>界面主题</h2>
            <select className="select-field preference-select" aria-label="界面主题" value={settings?.theme ?? "dark"} disabled={!settings || busy} onChange={(event) => updatePreference({ theme: event.target.value as AppSettings["theme"] })}>
              <option value="dark">深色</option><option value="system">跟随系统</option>
            </select>
          </div>
        </article>
        <article className="setting-card">
          <div className="setting-card__icon"><Languages size={20} /></div>
          <div className="setting-card__body">
            <span className="eyebrow">语言</span><h2>界面语言</h2>
            <select className="select-field preference-select" aria-label="界面语言" value={settings?.language ?? "zh-CN"} disabled={!settings || busy} onChange={(event) => updatePreference({ language: event.target.value })}>
              <option value="zh-CN">简体中文</option><option value="en-US">English (Preview)</option>
            </select>
          </div>
        </article>
        <article className="setting-card">
          <div className="setting-card__icon"><CheckCircle2 size={20} /></div>
          <div className="setting-card__body"><span className="eyebrow">帮助</span><h2>安全工作流</h2><p>重新查看导入、启停和 Profile 说明。</p></div>
          <button className="button button--secondary" type="button" disabled={!settings || busy} onClick={onboarding.open}>重新查看引导</button>
        </article>
      </section>
    </div>
  );
}
