import {
  AlertTriangle,
  ChevronDown,
  CircleAlert,
  FileWarning,
  GitMerge,
  ShieldCheck,
} from "lucide-react";
import { Link } from "react-router";
import type {
  ConflictEvidence,
  ConflictKind,
  ConflictParticipant,
  ConflictReport,
  ConflictSeverity,
  ModConflict,
} from "../../types/app";

const MAX_VISIBLE_CONFLICTS = 40;

interface EvidenceSourceSummary {
  sourcePath: string;
  sections: string[];
  matches: number;
}

interface ConflictParticipantSummary extends ConflictParticipant {
  matchedConflicts: number;
  sources: EvidenceSourceSummary[];
}

interface ConflictDisplayGroup {
  id: string;
  kind: ConflictKind;
  severity: ConflictSeverity;
  conflicts: ModConflict[];
  resourceKeys: string[];
  participants: ConflictParticipantSummary[];
  winningModId: string | null;
}

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
  const grouped = groupConflictsForDisplay(matching);
  const visible = grouped.slice(0, MAX_VISIBLE_CONFLICTS);

  return (
    <section className="panel conflict-report-panel" aria-label={title}>
      <div className="panel__header conflict-report-panel__header">
        <div>
          <span className="eyebrow">冲突分析</span>
          <h2>{title}</h2>
          <p>
            已分析 {report.enabledMods} 个启用模组、{report.analyzedIniFiles} 个 INI；
            {modId ? `与当前模组相关 ${matching.length} 项冲突` : `共 ${matching.length} 项冲突`}
            {matching.length ? `，已合并为 ${grouped.length} 组摘要` : ""}。
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
          {visible.map((group) => (
            <ConflictEntry group={group} key={group.id} />
          ))}
          {grouped.length > visible.length ? (
            <p className="conflict-list__limit">
              当前显示前 {visible.length} 组摘要；其余 {grouped.length - visible.length} 组仍计入统计。
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

function ConflictEntry({ group }: { group: ConflictDisplayGroup }) {
  const Icon = group.severity === "error" ? CircleAlert : FileWarning;
  const firstConflict = group.conflicts[0];
  const summary =
    group.conflicts.length === 1 && firstConflict
      ? firstConflict.summary
      : `${group.participants.length} 个启用模组涉及 ${group.resourceKeys.length} 个 ${kindLabel(group.kind)} 冲突。`;
  return (
    <details className={`conflict-entry conflict-entry--${group.severity}`}>
      <summary className="conflict-entry__heading">
        <Icon size={18} />
        <div>
          <span>{kindLabel(group.kind)}</span>
          <strong>{summary}</strong>
        </div>
        <span className="conflict-entry__heading-actions">
          <span className={`conflict-severity conflict-severity--${group.severity}`}>
            {severityLabel(group.severity)}
          </span>
          <ChevronDown className="conflict-entry__chevron" size={18} aria-hidden="true" />
        </span>
      </summary>
      <div className="conflict-entry__body">
        {group.resourceKeys.length === 1 ? (
          <code className="conflict-resource-key">{group.resourceKeys[0]}</code>
        ) : (
          <details className="conflict-resource-group">
            <summary>
              查看 {group.resourceKeys.length} 个冲突{resourceUnitLabel(group.kind)}
            </summary>
            <div>
              {group.resourceKeys.map((resourceKey) => (
                <code key={resourceKey}>{resourceKey}</code>
              ))}
            </div>
          </details>
        )}
        <div className="conflict-participants">
          {group.participants.map((participant) => (
            <article key={participant.modId}>
              <div className="conflict-participant__identity">
                <Link to={`/mods/${participant.modId}`}>{participant.modName}</Link>
                <span>AEMM 顺序 #{participant.loadOrder + 1}</span>
              </div>
              <div className="conflict-participant__summary">
                <span>{participant.sources.length} 个相关文件</span>
                <span>{participant.matchedConflicts} 个冲突项</span>
              </div>
              <details className="conflict-evidence-details">
                <summary>查看文件证据</summary>
                <ul>
                  {participant.sources.map((source) => (
                    <li key={source.sourcePath.toLowerCase()}>
                      <code>{source.sourcePath}</code>
                      <span>
                        {source.sections.length
                          ? `${source.sections.length} 个相关区段`
                          : "文件级匹配"}
                        {source.matches > 1 ? ` · ${source.matches} 条匹配` : ""}
                      </span>
                      {source.sections.length ? (
                        <small title={source.sections.join("、")}>
                          {summarizeSections(source.sections)}
                        </small>
                      ) : null}
                    </li>
                  ))}
                </ul>
              </details>
            </article>
          ))}
        </div>
        <div className="conflict-winner-state">
          {group.winningModId
            ? "已根据加载器规则确定胜出模组"
            : "未推断胜出模组；调整 AEMM 顺序目前不会被描述为 EFMI 的确定优先级。"}
        </div>
      </div>
    </details>
  );
}

function groupConflictsForDisplay(conflicts: ModConflict[]): ConflictDisplayGroup[] {
  const groups = new Map<string, ModConflict[]>();
  for (const conflict of conflicts) {
    const participantKey = conflict.participants
      .map((participant) => participant.modId)
      .sort()
      .join(",");
    const key = [
      conflict.analyzerId,
      conflict.kind,
      conflict.severity,
      conflict.winningModId ?? "",
      participantKey,
    ].join("|");
    const group = groups.get(key);
    if (group) group.push(conflict);
    else groups.set(key, [conflict]);
  }

  return Array.from(groups.entries()).flatMap(([id, items]) => {
    const first = items[0];
    if (!first) return [];
    const participants = new Map<
      string,
      ConflictParticipant & { matchedConflicts: number; evidence: ConflictEvidence[] }
    >();
    for (const conflict of items) {
      for (const participant of conflict.participants) {
        const existing = participants.get(participant.modId);
        if (existing) {
          existing.matchedConflicts += 1;
          existing.evidence.push(...participant.evidence);
        } else {
          participants.set(participant.modId, {
            ...participant,
            matchedConflicts: 1,
            evidence: [...participant.evidence],
          });
        }
      }
    }

    return [{
      id,
      kind: first.kind,
      severity: first.severity,
      conflicts: items,
      resourceKeys: Array.from(new Set(items.map((item) => item.resourceKey))).sort(),
      participants: Array.from(participants.values())
        .map((participant) => ({
          ...participant,
          evidence: deduplicateEvidence(participant.evidence),
          sources: summarizeEvidenceSources(participant.evidence),
        }))
        .sort(
          (left, right) =>
            left.loadOrder - right.loadOrder || left.modName.localeCompare(right.modName),
      ),
      winningModId: first.winningModId,
    }];
  });
}

function deduplicateEvidence(evidence: ConflictEvidence[]): ConflictEvidence[] {
  const unique = new Map<string, ConflictEvidence>();
  for (const item of evidence) {
    const key = [
      item.sourcePath.toLowerCase(),
      item.section?.toLowerCase() ?? "",
      item.detail,
    ].join("|");
    if (!unique.has(key)) unique.set(key, item);
  }
  return Array.from(unique.values());
}

function summarizeEvidenceSources(evidence: ConflictEvidence[]): EvidenceSourceSummary[] {
  const sources = new Map<
    string,
    { sourcePath: string; sections: Set<string>; matches: number }
  >();
  for (const item of deduplicateEvidence(evidence)) {
    const key = item.sourcePath.toLowerCase();
    const source = sources.get(key) ?? {
      sourcePath: item.sourcePath,
      sections: new Set<string>(),
      matches: 0,
    };
    if (item.section) source.sections.add(item.section);
    source.matches += 1;
    sources.set(key, source);
  }
  return Array.from(sources.values())
    .map((source) => ({
      sourcePath: source.sourcePath,
      sections: Array.from(source.sections).sort(),
      matches: source.matches,
    }))
    .sort((left, right) => left.sourcePath.localeCompare(right.sourcePath));
}

function summarizeSections(sections: string[]): string {
  const visible = sections.slice(0, 2);
  return sections.length > visible.length
    ? `${visible.join("、")} 等 ${sections.length} 个区段`
    : visible.join("、");
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

function resourceUnitLabel(kind: ConflictKind): string {
  return kind === "efmiTextureOverride" || kind === "efmiShaderOverride" ? " Hash" : "资源";
}

function severityLabel(severity: ConflictSeverity): string {
  const labels: Record<ConflictSeverity, string> = {
    information: "提示",
    warning: "警告",
    error: "错误",
  };
  return labels[severity];
}
