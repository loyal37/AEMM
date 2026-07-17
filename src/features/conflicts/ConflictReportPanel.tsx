import {
  AlertTriangle,
  CircleAlert,
  FileWarning,
  GitMerge,
  ShieldCheck,
} from "lucide-react";
import { Link } from "react-router";
import type {
  ConflictKind,
  ConflictReport,
  ConflictSeverity,
  ModConflict,
} from "../../types/app";

const MAX_VISIBLE_CONFLICTS = 40;

interface ConflictReportPanelProps {
  report: ConflictReport;
  modId?: string;
  title?: string;
}

export function ConflictReportPanel({
  report,
  modId,
  title = "当前 Profile 冲突",
}: ConflictReportPanelProps) {
  const matching = modId
    ? report.conflicts.filter((conflict) =>
        conflict.participants.some((participant) => participant.modId === modId),
      )
    : report.conflicts;
  const visible = matching.slice(0, MAX_VISIBLE_CONFLICTS);

  return (
    <section className="panel conflict-report-panel" aria-label={title}>
      <div className="panel__header conflict-report-panel__header">
        <div>
          <span className="eyebrow">冲突分析</span>
          <h2>{title}</h2>
          <p>
            已分析 {report.enabledMods} 个启用模组、{report.analyzedIniFiles} 个 INI；
            {modId ? `与当前模组相关 ${matching.length} 组` : `共 ${matching.length} 组冲突`}。
          </p>
        </div>
        <div className={`conflict-total${matching.length ? " is-warning" : " is-clear"}`}>
          {matching.length ? <AlertTriangle size={18} /> : <ShieldCheck size={18} />}
          <strong>{matching.length}</strong>
        </div>
      </div>

      <div className="load-order-disclaimer">
        <GitMerge size={17} />
        <span>{report.loadOrderNote}</span>
        <strong>{report.loadOrderVerified ? "顺序已验证" : "胜出顺序未验证"}</strong>
      </div>

      {report.warnings.length ? (
        <details className="conflict-analysis-warnings">
          <summary>{report.warnings.length} 条分析警告</summary>
          <ul>
            {report.warnings.map((warning) => (
              <li key={warning}>{warning}</li>
            ))}
          </ul>
        </details>
      ) : null}

      {visible.length ? (
        <div className="conflict-list">
          {visible.map((conflict) => (
            <ConflictEntry conflict={conflict} key={conflict.id} />
          ))}
          {matching.length > visible.length ? (
            <p className="conflict-list__limit">
              当前显示前 {visible.length} 组；其余 {matching.length - visible.length} 组仍计入统计。
            </p>
          ) : null}
        </div>
      ) : (
        <div className="conflict-empty">
          <ShieldCheck size={21} />
          <div>
            <strong>{modId ? "当前模组未检测到冲突" : "当前启用组合未检测到冲突"}</strong>
            <span>该结论仅覆盖已部署文件路径、显式 namespace 与已识别的 EFMI Override Hash。</span>
          </div>
        </div>
      )}
    </section>
  );
}

function ConflictEntry({ conflict }: { conflict: ModConflict }) {
  const Icon = conflict.severity === "error" ? CircleAlert : FileWarning;
  return (
    <article className={`conflict-entry conflict-entry--${conflict.severity}`}>
      <div className="conflict-entry__heading">
        <Icon size={18} />
        <div>
          <span>{kindLabel(conflict.kind)}</span>
          <strong>{conflict.summary}</strong>
        </div>
        <span className={`conflict-severity conflict-severity--${conflict.severity}`}>
          {severityLabel(conflict.severity)}
        </span>
      </div>
      <code className="conflict-resource-key">{conflict.resourceKey}</code>
      <div className="conflict-participants">
        {conflict.participants.map((participant) => (
          <article key={participant.modId}>
            <div className="conflict-participant__identity">
              <Link to={`/mods/${participant.modId}`}>{participant.modName}</Link>
              <span>AEMM 顺序 #{participant.loadOrder + 1}</span>
            </div>
            <ul>
              {participant.evidence.map((evidence, index) => (
                <li key={`${evidence.sourcePath}-${evidence.section ?? "file"}-${index}`}>
                  <code>{evidence.sourcePath}</code>
                  {evidence.section ? <span>{evidence.section}</span> : null}
                  <small>{evidence.detail}</small>
                </li>
              ))}
            </ul>
          </article>
        ))}
      </div>
      <div className="conflict-winner-state">
        {conflict.winningModId
          ? "已根据加载器规则确定胜出模组"
          : "未推断胜出模组；调整 AEMM 顺序目前不会被描述为 EFMI 的确定优先级。"}
      </div>
    </article>
  );
}

function kindLabel(kind: ConflictKind): string {
  const labels: Record<ConflictKind, string> = {
    deploymentPath: "部署目标文件",
    efmiNamespace: "EFMI namespace",
    efmiTextureOverride: "TextureOverride Hash",
    efmiShaderOverride: "ShaderOverride Hash",
  };
  return labels[kind];
}

function severityLabel(severity: ConflictSeverity): string {
  const labels: Record<ConflictSeverity, string> = {
    information: "提示",
    warning: "警告",
    error: "错误",
  };
  return labels[severity];
}
