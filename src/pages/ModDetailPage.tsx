import {
  AlertTriangle,
  ArrowLeft,
  CalendarDays,
  ExternalLink,
  FileQuestion,
  Files,
  FolderOpen,
  HardDrive,
  Power,
  Heart,
  LoaderCircle,
  Save,
  ShieldCheck,
  Star,
  Tag,
  Trash2,
  UserRound,
} from "lucide-react";
import { type FormEvent, useEffect, useState } from "react";
import { Link, useNavigate, useParams } from "react-router";
import { EmptyState } from "../components/ui/EmptyState";
import { useAppBootstrap } from "../features/bootstrap/useAppBootstrap";
import { ConflictReportPanel } from "../features/conflicts/ConflictReportPanel";
import { useConflictReport } from "../features/conflicts/useConflictReport";
import { useGameStatus } from "../features/game/useGameManager";
import {
  formatFileSize,
  formatTimestamp,
  lifecycleLabel,
} from "../features/mods/modQuery";
import { ModPreviewImage } from "../features/mods/ModPreviewImage";
import {
  useModDetails,
  useOpenModDirectory,
  useSetModFavorite,
  useSetModsEnabled,
  useUninstallMods,
  useUpdateLocalModMetadata,
} from "../features/mods/useModManager";
import { VirtualModFileList } from "../features/mods/VirtualModFileList";
import { commandErrorMessage } from "../lib/tauri";
import type { LocalModMetadata, ModDetails } from "../types/app";

export function ModDetailPage() {
  const { modId } = useParams();
  const navigate = useNavigate();
  const bootstrap = useAppBootstrap();
  const details = useModDetails(modId);
  const conflicts = useConflictReport();
  const gameStatus = useGameStatus();
  const favorite = useSetModFavorite();
  const deployment = useSetModsEnabled();
  const uninstall = useUninstallMods();
  const openDirectory = useOpenModDirectory();
  const desktopReady = bootstrap.data?.runtimeMode === "desktop";
  const deploymentAvailable = desktopReady && gameStatus.data?.loader?.valid === true;

  if (details.isPending) {
    return (
      <div className="page-stack">
        <Link className="back-link" to="/mods">
          <ArrowLeft size={17} /> 返回模组列表
        </Link>
        <section className="panel panel--fill">
          <div className="loading-state">
            <LoaderCircle className="spin" size={27} />
            <span>正在读取模组详情…</span>
          </div>
        </section>
      </div>
    );
  }

  if (!details.data || details.isError || !modId) {
    return (
      <div className="page-stack">
        <Link className="back-link" to="/mods">
          <ArrowLeft size={17} /> 返回模组列表
        </Link>
        <section className="panel panel--fill">
          <EmptyState
            icon={FileQuestion}
            title="无法读取模组详情"
            description={commandErrorMessage(details.error)}
          />
        </section>
      </div>
    );
  }

  const data = details.data;
  const item = data.item;
  return (
    <div className="page-stack mod-detail-page">
      <Link className="back-link" to="/mods">
        <ArrowLeft size={17} /> 返回模组列表
      </Link>

      <section className="mod-detail-hero">
        <ModPreviewImage
          modId={item.id}
          name={item.name}
          hasPreview={Boolean(item.previewPath)}
          variant="detail"
        />
        <div className="mod-detail-hero__content">
          <div className="mod-detail-hero__topline">
            <span className="eyebrow">{item.category ?? "未分类"}</span>
            <span className={`mod-state mod-state--${item.lifecycleState}`}>
              {item.lifecycleState === "broken" ? <AlertTriangle size={12} /> : null}
              {lifecycleLabel(item.lifecycleState)}
            </span>
          </div>
          <h1>{item.name}</h1>
          <p>{item.description ?? "该模组没有提供说明。"}</p>
          <div className="mod-detail-hero__meta">
            <span><UserRound size={14} /> {item.author ?? "未知作者"}</span>
            <span><Tag size={14} /> v{item.version ?? "未知"}</span>
            <span><HardDrive size={14} /> {formatFileSize(item.sizeBytes)}</span>
          </div>
          <div className="mod-detail-hero__actions">
            <button
              className={item.enabled ? "button button--secondary" : "button button--primary"}
              type="button"
              disabled={
                item.lifecycleState !== "installed" ||
                !deploymentAvailable ||
                deployment.isPending
              }
              title={!deploymentAvailable ? "请先配置有效的 EFMI 加载器" : undefined}
              onClick={() =>
                deployment.mutate({ modIds: [item.id], enabled: !item.enabled })
              }
            >
              <Power size={16} />
              {deployment.isPending
                ? "正在同步…"
                : item.enabled
                  ? "禁用模组"
                  : "启用模组"}
            </button>
            <button
              className="button button--secondary"
              type="button"
              disabled={favorite.isPending}
              onClick={() =>
                favorite.mutate({ modIds: [item.id], favorite: !item.favorite })
              }
            >
              <Star size={16} fill={item.favorite ? "currentColor" : "none"} />
              {item.favorite ? "已收藏" : "收藏"}
            </button>
            <button
              className="button button--secondary"
              type="button"
              disabled={!desktopReady || openDirectory.isPending}
              onClick={() => openDirectory.mutate(item.id)}
            >
              <FolderOpen size={16} /> 打开所在目录
            </button>
            <button
              className="button button--danger"
              type="button"
              disabled={item.enabled || uninstall.isPending || deployment.isPending}
              title={item.enabled ? "请先禁用模组再卸载" : undefined}
              onClick={() => {
                if (!window.confirm(`确定卸载“${item.name}”吗？此操作会删除 AEMM 仓库中的模组本体，并从所有 Profile 中移除对应记录。`)) return;
                uninstall.mutate([item.id], { onSuccess: () => navigate("/mods") });
              }}
            >
              <Trash2 size={16} /> {uninstall.isPending ? "正在卸载…" : "卸载模组"}
            </button>
          </div>
          {favorite.isError || openDirectory.isError || deployment.isError || uninstall.isError ? (
            <p className="inline-error">
              {commandErrorMessage(
                favorite.error ?? openDirectory.error ?? deployment.error ?? uninstall.error,
              )}
            </p>
          ) : null}
          {deployment.isSuccess && deployment.data.guidance ? (
            <p className="inline-success">{deployment.data.guidance}</p>
          ) : null}
        </div>
      </section>

      {item.lifecycleState === "broken" ? (
        <div className="detail-warning">
          <AlertTriangle size={18} />
          <div>
            <strong>该模组需要检查</strong>
            <span>仓库文件可能缺失、包含不安全条目或在最近一次扫描时无法读取。</span>
          </div>
        </div>
      ) : null}

      <section className="mod-detail-facts" aria-label="模组信息">
        <DetailFact icon={CalendarDays} label="安装时间" value={formatTimestamp(item.installedAt)} />
        <DetailFact icon={Files} label="文件数量" value={`${item.fileCount} 个`} />
        <DetailFact icon={ShieldCheck} label="元数据来源" value={data.metadataSource === "modJson" ? "作者 mod.json" : "AEMM 推断"} />
        <DetailFact icon={Heart} label="兼容游戏版本" value={data.gameVersion ?? "作者未声明"} />
      </section>

      <section className="mod-detail-columns">
        <article className="panel detail-metadata-panel">
          <div className="panel__header">
            <div>
              <span className="eyebrow">作者数据</span>
              <h2>原始元数据</h2>
            </div>
            <ShieldCheck size={20} />
          </div>
          <dl className="detail-definition-list">
            <div><dt>原始名称</dt><dd>{data.authorName}</dd></div>
            <div><dt>原始分类</dt><dd>{data.authorCategory ?? "未声明"}</dd></div>
            <div><dt>逻辑 ID</dt><dd><code>{item.logicalId}</code></dd></div>
            <div><dt>仓库路径</dt><dd><code>{item.repositoryPath}</code></dd></div>
            <div>
              <dt>作者网站</dt>
              <dd className="untrusted-url">
                <ExternalLink size={13} /> {data.website ?? "未提供"}
              </dd>
            </div>
          </dl>
          <p className="detail-author-description">
            {data.authorDescription ?? "作者没有提供原始描述。"}
          </p>
        </article>

        <LocalMetadataEditor details={data} />
      </section>

      {conflicts.data ? (
        <ConflictReportPanel
          report={conflicts.data}
          modId={item.id}
          title="当前模组的冲突"
        />
      ) : conflicts.isPending ? (
        <section className="panel detail-conflict-panel">
          <LoaderCircle className="spin" size={20} />
          <span>正在分析已启用模组的部署与 EFMI INI…</span>
        </section>
      ) : (
        <section className="panel detail-conflict-panel">
          <AlertTriangle size={20} />
          <span>{commandErrorMessage(conflicts.error)}</span>
        </section>
      )}

      <section className="panel detail-files-panel">
        <div className="panel__header">
          <div>
            <span className="eyebrow">文件清单</span>
            <h2>{data.files.length} 个仓库文件</h2>
          </div>
          <span className="file-inventory-note">Hash 只读展示</span>
        </div>
        <VirtualModFileList files={data.files} />
      </section>
    </div>
  );
}

function DetailFact({
  icon: Icon,
  label,
  value,
}: {
  icon: typeof CalendarDays;
  label: string;
  value: string;
}) {
  return (
    <article>
      <Icon size={17} />
      <div><span>{label}</span><strong>{value}</strong></div>
    </article>
  );
}

function LocalMetadataEditor({ details }: { details: ModDetails }) {
  const mutation = useUpdateLocalModMetadata(details.item.id);
  const [displayName, setDisplayName] = useState("");
  const [category, setCategory] = useState("");
  const [description, setDescription] = useState("");
  const [notes, setNotes] = useState("");
  const [tags, setTags] = useState("");

  useEffect(() => {
    setDisplayName(details.localMetadata.displayNameOverride ?? "");
    setCategory(details.localMetadata.categoryOverride ?? "");
    setDescription(details.localMetadata.descriptionOverride ?? "");
    setNotes(details.localMetadata.notes ?? "");
    setTags(details.localMetadata.tags.join(", "));
  }, [details]);

  const submit = (event: FormEvent<HTMLFormElement>) => {
    event.preventDefault();
    const metadata: LocalModMetadata = {
      displayNameOverride: optionalValue(displayName),
      categoryOverride: optionalValue(category),
      descriptionOverride: optionalValue(description),
      favorite: details.item.favorite,
      notes: optionalValue(notes),
      tags: tags
        .split(/[,，\n]/)
        .map((tag) => tag.trim())
        .filter(Boolean),
    };
    mutation.mutate(metadata);
  };

  return (
    <form className="panel local-metadata-form" onSubmit={submit}>
      <div className="panel__header">
        <div>
          <span className="eyebrow">AEMM 本地数据</span>
          <h2>显示覆盖与备注</h2>
        </div>
        <span className="local-only-badge">不修改 mod.json</span>
      </div>
      <div className="local-metadata-form__grid">
        <label>
          <span>显示名称</span>
          <input maxLength={512} value={displayName} onChange={(event) => setDisplayName(event.target.value)} placeholder={details.authorName} />
        </label>
        <label>
          <span>本地分类</span>
          <input maxLength={512} value={category} onChange={(event) => setCategory(event.target.value)} placeholder={details.authorCategory ?? "未分类"} />
        </label>
        <label className="field-span-full">
          <span>显示描述</span>
          <textarea maxLength={32768} value={description} onChange={(event) => setDescription(event.target.value)} rows={3} placeholder="留空时使用作者描述" />
        </label>
        <label className="field-span-full">
          <span>私人备注</span>
          <textarea maxLength={32768} value={notes} onChange={(event) => setNotes(event.target.value)} rows={2} placeholder="只保存在本机数据库中" />
        </label>
        <label className="field-span-full">
          <span>标签</span>
          <input maxLength={4160} value={tags} onChange={(event) => setTags(event.target.value)} placeholder="角色, 截图, 常用" />
        </label>
      </div>
      {mutation.isError ? <p className="inline-error">{commandErrorMessage(mutation.error)}</p> : null}
      {mutation.isSuccess ? <p className="inline-success">本地元数据已保存。</p> : null}
      <div className="local-metadata-form__actions">
        <button className="button button--primary" type="submit" disabled={mutation.isPending}>
          <Save size={16} /> {mutation.isPending ? "正在保存…" : "保存本地信息"}
        </button>
      </div>
    </form>
  );
}

function optionalValue(value: string): string | null {
  const normalized = value.trim();
  return normalized ? normalized : null;
}
