import {
  AlertTriangle,
  Boxes,
  CheckCircle2,
  Gamepad2,
  Play,
  Plus,
  ShieldCheck,
} from "lucide-react";
import { Link } from "react-router";
import { PageHeader } from "../components/ui/PageHeader";
import { useAppBootstrap } from "../features/bootstrap/useAppBootstrap";
import { useGameStatus, useLaunchGame } from "../features/game/useGameManager";
import { formatTimestamp } from "../features/mods/modQuery";
import { useInstalledMods } from "../features/mods/useModManager";
import { commandErrorMessage } from "../lib/tauri";

export function DashboardPage() {
  const bootstrap = useAppBootstrap();
  const gameStatus = useGameStatus();
  const mods = useInstalledMods();
  const launch = useLaunchGame();
  const desktopReady = bootstrap.data?.runtimeMode === "desktop";
  const databaseReady = bootstrap.data?.databaseReady === true;
  const installation = gameStatus.data?.installation?.installation;
  const gameTitle = gameStatus.data?.configured
    ? `游戏目录已验证 · ${gameStatus.data.installation?.confidence ?? 0}%`
    : "等待配置游戏目录";
  const gameDescription = installation
    ? `${installation.installationRoot} · 游戏版本 ${installation.version.value ?? "未知"}`
    : "可在设置中自动检测鹰角启动器安装位置，或手动选择游戏目录。";
  const installedMods = mods.data ?? [];
  const recentMods = [...installedMods]
    .sort((left, right) => right.installedAt - left.installedAt)
    .slice(0, 4);
  const statistics = [
    {
      label: "已安装模组",
      value: mods.isPending ? "…" : String(installedMods.length),
      hint: installedMods.length ? `${installedMods.filter((item) => item.favorite).length} 个收藏` : "仓库中暂无模组",
      icon: Boxes,
      tone: "violet",
    },
    {
      label: "已启用",
      value: mods.isPending
        ? "—"
        : String(installedMods.filter((item) => item.enabled).length),
      hint: gameStatus.data?.loader?.valid
        ? "EFMI 清单部署状态"
        : "配置 EFMI 后可安全启停",
      icon: CheckCircle2,
      tone: "green",
    },
    { label: "检测到冲突", value: "—", hint: "Phase 7 接入分析结果", icon: AlertTriangle, tone: "amber" },
  ];

  return (
    <div className="page-stack">
      <PageHeader
        eyebrow="概览"
        title="管理你的终末地模组"
        description="安全安装、组合和切换模组，同时保留每一份原始内容。"
        actions={
          <>
            <Link className="button button--secondary" to="/mods">
              <Plus size={17} />
              导入模组
            </Link>
            <button
              className="button button--primary"
              type="button"
              disabled={!gameStatus.data?.canLaunch || launch.isPending}
              onClick={() => launch.mutate()}
            >
              <Play size={17} fill="currentColor" />
              {launch.isPending ? "正在启动…" : "启动游戏"}
            </button>
          </>
        }
      />

      <section className="hero-card">
        <div className="hero-card__glow" />
        <div className="hero-card__content">
          <div className="hero-card__icon">
            <Gamepad2 size={31} strokeWidth={1.5} />
          </div>
          <div>
            <span className="eyebrow">游戏状态</span>
            <h2>{gameTitle}</h2>
            <p>{gameDescription}</p>
            {gameStatus.data?.launchBlockReason ? (
              <small className="hero-card__notice">{gameStatus.data.launchBlockReason}</small>
            ) : null}
          </div>
        </div>
        <Link className="button button--secondary" to="/settings">
          前往设置
        </Link>
      </section>

      {launch.isError ? (
        <p className="inline-error">{commandErrorMessage(launch.error)}</p>
      ) : null}
      {launch.isSuccess ? (
        <p className="inline-success">启动请求已提交，进程 ID：{launch.data.processId}</p>
      ) : null}

      <section className="stats-grid" aria-label="模组统计">
        {statistics.map(({ label, value, hint, icon: Icon, tone }) => (
          <article className="stat-card" key={label}>
            <div className={`stat-card__icon stat-card__icon--${tone}`}>
              <Icon size={20} strokeWidth={1.8} />
            </div>
            <div>
              <span>{label}</span>
              <strong>{value}</strong>
              <small>{hint}</small>
            </div>
          </article>
        ))}
      </section>

      <section className="dashboard-grid">
        <article className="panel">
          <div className="panel__header">
            <div>
              <span className="eyebrow">最近活动</span>
              <h2>最近安装</h2>
            </div>
            <Link to="/mods">查看全部</Link>
          </div>
          {recentMods.length ? (
            <div className="recent-mod-list">
              {recentMods.map((item) => (
                <Link to={`/mods/${item.id}`} key={item.id}>
                  <span className={`recent-mod-list__marker${item.lifecycleState === "broken" ? " is-warning" : ""}`} />
                  <span>
                    <strong>{item.name}</strong>
                    <small>{item.author ?? "未知作者"} · {formatTimestamp(item.installedAt)}</small>
                  </span>
                  <span>{item.category ?? "未分类"}</span>
                </Link>
              ))}
            </div>
          ) : (
            <div className="compact-empty">
              <Boxes size={23} />
              <div>
                <strong>还没有安装模组</strong>
                <span>扫描仓库后会显示在这里。</span>
              </div>
            </div>
          )}
        </article>

        <article className="panel runtime-panel">
          <div className="panel__header">
            <div>
              <span className="eyebrow">运行状态</span>
              <h2>基础服务</h2>
            </div>
            <ShieldCheck size={21} />
          </div>
          <dl className="status-list">
            <div>
              <dt>桌面运行时</dt>
              <dd className={desktopReady ? "status-ok" : "status-muted"}>
                {desktopReady ? "已连接" : "浏览器预览"}
              </dd>
            </div>
            <div>
              <dt>SQLite 数据库</dt>
              <dd className={databaseReady ? "status-ok" : "status-muted"}>
                {databaseReady ? "就绪" : "等待桌面启动"}
              </dd>
            </div>
            <div>
              <dt>应用版本</dt>
              <dd>{bootstrap.data?.appVersion ?? "正在读取…"}</dd>
            </div>
          </dl>
          {bootstrap.isError ? (
            <p className="inline-error">无法读取后端状态，请检查本地日志。</p>
          ) : null}
        </article>
      </section>
    </div>
  );
}
