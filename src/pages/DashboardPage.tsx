import { AlertTriangle, Boxes, CheckCircle2, FolderCog, Plus, ShieldCheck } from "lucide-react";
import { Link } from "react-router";
import { PageHeader } from "../components/ui/PageHeader";
import { useAppBootstrap } from "../features/bootstrap/useAppBootstrap";
import { useConflictReport } from "../features/conflicts/useConflictReport";
import { formatTimestamp } from "../features/mods/modQuery";
import { useInstalledMods } from "../features/mods/useModManager";
import { useProfiles } from "../features/profiles/useProfiles";

export function DashboardPage() {
  const bootstrap = useAppBootstrap();
  const mods = useInstalledMods();
  const profiles = useProfiles();
  const conflicts = useConflictReport();
  const desktopReady = bootstrap.data?.runtimeMode === "desktop";
  const databaseReady = bootstrap.data?.databaseReady === true;
  const modsPath = bootstrap.data?.settings.game.loaderRoot
    ? bootstrap.data.settings.storage.repositoryPath
    : null;
  const installedMods = mods.data ?? [];
  const activeProfile = profiles.data?.find((profile) => profile.isActive);
  const recentMods = [...installedMods]
    .sort((left, right) => right.installedAt - left.installedAt)
    .slice(0, 4);
  const statistics = [
    {
      label: "已发现模组",
      value: mods.isPending ? "…" : String(installedMods.length),
      hint: installedMods.length
        ? `${installedMods.filter((item) => item.favorite).length} 个收藏`
        : "EFMI Mods 中暂无记录",
      icon: Boxes,
      tone: "violet",
    },
    {
      label: "已启用",
      value: mods.isPending
        ? "—"
        : String(installedMods.filter((item) => item.enabled).length),
      hint: modsPath ? "来自文件夹名称的实际状态" : "配置 Mods 目录后可启停",
      icon: CheckCircle2,
      tone: "green",
    },
    {
      label: "检测到冲突",
      value: conflicts.isPending
        ? "—"
        : conflicts.isError
          ? "!"
          : String(conflicts.data?.conflicts.length ?? 0),
      hint: conflicts.isError
        ? "冲突分析暂不可用"
        : `${conflicts.data?.affectedMods ?? 0} 个启用模组受影响`,
      icon: AlertTriangle,
      tone: "amber",
    },
  ];

  return (
    <div className="page-stack">
      <PageHeader
        eyebrow="概览"
        title="管理 EFMI 模组"
        description="直接扫描和管理 EFMI Mods；AEMM 不检测、启动或接管游戏。"
        actions={
          <>
            <Link className="button button--secondary" to="/mods">
              <Plus size={17} /> 导入模组
            </Link>
            <Link className="button button--primary" to="/settings">
              <FolderCog size={17} /> 配置 Mods 目录
            </Link>
          </>
        }
      />

      <section className="hero-card">
        <div className="hero-card__glow" />
        <div className="hero-card__content">
          <div className="hero-card__icon"><FolderCog size={31} strokeWidth={1.5} /></div>
          <div>
            <span className="eyebrow">模组来源</span>
            <h2>{modsPath ? "已连接 EFMI Mods" : "等待选择 EFMI Mods"}</h2>
            <p className="path-value" title={modsPath ?? undefined}>
              {modsPath ?? "在设置中选择 EFMI 根目录或它的 Mods 子目录。"}
            </p>
          </div>
        </div>
        <Link className="button button--secondary" to="/mods">查看模组</Link>
      </section>

      <section className="stats-grid" aria-label="模组统计">
        {statistics.map(({ label, value, hint, icon: Icon, tone }) => (
          <article className="stat-card" key={label}>
            <div className={`stat-card__icon stat-card__icon--${tone}`}><Icon size={20} /></div>
            <div><span>{label}</span><strong>{value}</strong><small>{hint}</small></div>
          </article>
        ))}
      </section>

      <section className="dashboard-grid">
        <article className="panel">
          <div className="panel__header">
            <div><span className="eyebrow">最近活动</span><h2>最近发现</h2></div>
            <Link to="/mods">查看全部</Link>
          </div>
          {recentMods.length ? (
            <div className="recent-mod-list">
              {recentMods.map((item) => (
                <Link to={`/mods/${item.id}`} key={item.id}>
                  <span className={`recent-mod-list__marker${item.lifecycleState === "broken" ? " is-warning" : ""}`} />
                  <span><strong>{item.name}</strong><small>{item.author ?? "未知作者"} · {formatTimestamp(item.installedAt)}</small></span>
                  <span>{item.category ?? "未分类"}</span>
                </Link>
              ))}
            </div>
          ) : (
            <div className="compact-empty"><Boxes size={23} /><div><strong>还没有模组记录</strong><span>扫描 EFMI Mods 后会显示在这里。</span></div></div>
          )}
        </article>

        <article className="panel runtime-panel">
          <div className="panel__header">
            <div><span className="eyebrow">运行状态</span><h2>AEMM 本地服务</h2></div>
            <ShieldCheck size={21} />
          </div>
          <dl className="status-list">
            <div><dt>EFMI Mods</dt><dd className={modsPath ? "status-ok" : "status-muted"}>{modsPath ? "已连接" : "待配置"}</dd></div>
            <div><dt>SQLite 数据库</dt><dd className={databaseReady ? "status-ok" : "status-muted"}>{databaseReady ? "就绪" : "等待桌面启动"}</dd></div>
            <div><dt>当前 Profile</dt><dd className={activeProfile ? "status-ok" : "status-muted"}>{activeProfile?.name ?? "正在读取…"}</dd></div>
            <div><dt>运行模式</dt><dd>{desktopReady ? "桌面" : "浏览器预览"}</dd></div>
            <div><dt>应用版本</dt><dd>{bootstrap.data?.appVersion ?? "正在读取…"}</dd></div>
          </dl>
          {bootstrap.isError ? <p className="inline-error">无法读取后端状态，请检查本地日志。</p> : null}
        </article>
      </section>
    </div>
  );
}
