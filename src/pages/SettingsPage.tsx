import { Database, FileClock, FolderCog, Gamepad2, Languages, Palette } from "lucide-react";
import { PageHeader } from "../components/ui/PageHeader";
import { useAppBootstrap } from "../features/bootstrap/useAppBootstrap";

function displayPath(value: string | null | undefined) {
  return value && value.length > 0 ? value : "尚未设置";
}

export function SettingsPage() {
  const bootstrap = useAppBootstrap();
  const settings = bootstrap.data?.settings;

  return (
    <div className="page-stack">
      <PageHeader
        eyebrow="应用设置"
        title="设置"
        description="机器相关路径保存在 config.json；模组、Profile 与加载顺序保存在 SQLite。"
      />

      <section className="settings-grid">
        <article className="setting-card setting-card--wide">
          <div className="setting-card__icon">
            <Gamepad2 size={20} />
          </div>
          <div className="setting-card__body">
            <span className="eyebrow">游戏</span>
            <h2>游戏与加载器路径</h2>
            <p className="path-value">{displayPath(settings?.game.installationPath)}</p>
            <p className="path-value path-value--muted">
              EFMI：{displayPath(settings?.game.loaderRoot)}
            </p>
          </div>
          <button className="button button--secondary" type="button" disabled>
            配置路径
          </button>
        </article>

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
    </div>
  );
}
