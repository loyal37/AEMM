import { listen } from "@tauri-apps/api/event";
import { isTauri } from "@tauri-apps/api/core";
import { getCurrentWindow } from "@tauri-apps/api/window";
import {
  AlertTriangle,
  Archive,
  CheckCircle2,
  FileArchive,
  FolderOpen,
  LoaderCircle,
  ShieldCheck,
  Upload,
  X,
} from "lucide-react";
import { useEffect, useMemo, useRef, useState } from "react";
import {
  commandErrorMessage,
  selectModArchive,
  selectModDirectory,
} from "../../lib/tauri";
import type {
  ModImportPlan,
  ModInstallProgress,
  ModInstallResult,
} from "../../types/app";
import { formatFileSize } from "./modQuery";
import {
  useCancelModImport,
  useCommitModImport,
  usePrepareModImport,
} from "./useModManager";

interface ImportModDialogProps {
  desktopReady: boolean;
  isOpen: boolean;
  onOpen: () => void;
  onClose: () => void;
}

export function ImportModDialog({
  desktopReady,
  isOpen,
  onOpen,
  onClose,
}: ImportModDialogProps) {
  const prepare = usePrepareModImport();
  const commit = useCommitModImport();
  const cancel = useCancelModImport();
  const [plan, setPlan] = useState<ModImportPlan | null>(null);
  const [progress, setProgress] = useState<ModInstallProgress | null>(null);
  const [result, setResult] = useState<ModInstallResult | null>(null);
  const [localError, setLocalError] = useState<string | null>(null);
  const [dragActive, setDragActive] = useState(false);
  const beginPrepareRef = useRef<(sourcePath: string) => Promise<void>>(async () => {});
  const busy = prepare.isPending || commit.isPending || cancel.isPending;

  useEffect(() => {
    if (!desktopReady || !isTauri()) return;
    let disposed = false;
    let removeProgress: (() => void) | undefined;
    let removeDrag: (() => void) | undefined;
    void listen<ModInstallProgress>("mod-install-progress", (event) => {
      if (!disposed) setProgress(event.payload);
    }).then((unlisten) => {
      if (disposed) unlisten();
      else removeProgress = unlisten;
    });
    void getCurrentWindow()
      .onDragDropEvent((event) => {
        if (disposed) return;
        if (event.payload.type === "enter" || event.payload.type === "over") {
          setDragActive(true);
          return;
        }
        if (event.payload.type === "leave") {
          setDragActive(false);
          return;
        }
        if (event.payload.type === "drop") {
          setDragActive(false);
          const paths = event.payload.paths;
          onOpen();
          if (paths.length !== 1) {
            setLocalError("一次只能导入一个压缩包或文件夹。");
            return;
          }
          const [sourcePath] = paths;
          if (!sourcePath) {
            setLocalError("拖入内容没有可读取的本地路径。");
            return;
          }
          void beginPrepareRef.current(sourcePath);
        }
      })
      .then((unlisten) => {
        if (disposed) unlisten();
        else removeDrag = unlisten;
      });
    return () => {
      disposed = true;
      removeProgress?.();
      removeDrag?.();
    };
    // The listener intentionally keeps the latest render callback through React state.
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [desktopReady, onOpen]);

  async function beginPrepare(sourcePath: string) {
    if (busy) return;
    setLocalError(null);
    setResult(null);
    setProgress(null);
    if (plan) {
      try {
        await cancel.mutateAsync(plan.operationId);
      } catch (error) {
        setLocalError(commandErrorMessage(error));
        return;
      }
      setPlan(null);
    }
    try {
      const nextPlan = await prepare.mutateAsync(sourcePath);
      setPlan(nextPlan);
    } catch (error) {
      setLocalError(commandErrorMessage(error));
    }
  }
  beginPrepareRef.current = beginPrepare;

  async function pickArchive() {
    try {
      const path = await selectModArchive();
      if (path) await beginPrepare(path);
    } catch (error) {
      setLocalError(commandErrorMessage(error));
    }
  }

  async function pickDirectory() {
    try {
      const path = await selectModDirectory();
      if (path) await beginPrepare(path);
    } catch (error) {
      setLocalError(commandErrorMessage(error));
    }
  }

  async function commitPlan() {
    if (!plan?.canInstall || busy) return;
    setLocalError(null);
    try {
      const installed = await commit.mutateAsync(plan.operationId);
      setResult(installed);
    } catch (error) {
      setLocalError(commandErrorMessage(error));
      try {
        await cancel.mutateAsync(plan.operationId);
        setPlan(null);
      } catch (cleanupError) {
        if (commandErrorCode(cleanupError) === "NOT_AVAILABLE") {
          setPlan(null);
        } else {
          setLocalError(
            `${commandErrorMessage(error)} 暂存清理也未完成：${commandErrorMessage(cleanupError)}`,
          );
        }
      }
    }
  }

  async function closeDialog() {
    if (busy) return;
    if (plan && !result) {
      try {
        await cancel.mutateAsync(plan.operationId);
      } catch (error) {
        setLocalError(commandErrorMessage(error));
        return;
      }
    }
    setPlan(null);
    setProgress(null);
    setResult(null);
    setLocalError(null);
    onClose();
  }

  const percent = useMemo(() => progressPercent(progress), [progress]);
  const shownError =
    localError ??
    (prepare.isError || commit.isError || cancel.isError
      ? commandErrorMessage(prepare.error ?? commit.error ?? cancel.error)
      : null);

  return (
    <>
      {dragActive ? (
        <div className="mod-drop-overlay" aria-hidden="true">
          <div>
            <Upload size={34} />
            <strong>释放以安全导入模组</strong>
            <span>支持 ZIP、7z、RAR 与文件夹</span>
          </div>
        </div>
      ) : null}
      {isOpen ? (
        <div className="modal-backdrop" role="presentation">
          <section
            className="import-dialog"
            role="dialog"
            aria-modal="true"
            aria-labelledby="import-dialog-title"
          >
            <header className="import-dialog__header">
              <div>
                <span className="eyebrow">安全安装工作流</span>
                <h2 id="import-dialog-title">导入模组</h2>
              </div>
              <button
                className="icon-button"
                type="button"
                aria-label="关闭导入窗口"
                disabled={busy}
                onClick={() => void closeDialog()}
              >
                <X size={18} />
              </button>
            </header>

            {result ? (
              <div className="import-success">
                <CheckCircle2 size={42} />
                <h3>{result.name} 已安装</h3>
                <p>模组文件已安全提交到仓库，数据库与列表也已同步。</p>
                <code>{result.repositoryPath}</code>
                <button className="button button--primary" type="button" onClick={() => void closeDialog()}>
                  完成
                </button>
              </div>
            ) : (
              <>
                <div className="import-source-actions">
                  <button type="button" disabled={busy} onClick={() => void pickArchive()}>
                    <FileArchive size={22} />
                    <span>
                      <strong>选择压缩包</strong>
                      <small>ZIP / 7z / RAR</small>
                    </span>
                  </button>
                  <button type="button" disabled={busy} onClick={() => void pickDirectory()}>
                    <FolderOpen size={22} />
                    <span>
                      <strong>选择文件夹</strong>
                      <small>复制到隔离暂存区后分析</small>
                    </span>
                  </button>
                </div>

                {progress && (busy || progress.stage !== "ready") ? (
                  <div className="import-progress" aria-live="polite">
                    <div className="import-progress__headline">
                      <span>
                        {progress.stage === "rollingBack" ? (
                          <AlertTriangle size={17} />
                        ) : (
                          <LoaderCircle className={busy ? "spin" : undefined} size={17} />
                        )}
                        {progress.message}
                      </span>
                      <strong>{Math.round(percent)}%</strong>
                    </div>
                    <div className="progress-track">
                      <span style={{ width: `${percent}%` }} />
                    </div>
                    <small>
                      {progress.totalBytes
                        ? `${formatFileSize(progress.processedBytes)} / ${formatFileSize(progress.totalBytes)}`
                        : "正在建立安全安装计划…"}
                    </small>
                  </div>
                ) : null}

                {plan ? <PlanConfirmation plan={plan} /> : null}

                {!plan && !prepare.isPending ? (
                  <div className="import-drop-hint">
                    <Archive size={21} />
                    <span>也可以直接把一个压缩包或文件夹拖入窗口</span>
                  </div>
                ) : null}

                {shownError ? <p className="inline-error">{shownError}</p> : null}

                <footer className="import-dialog__footer">
                  <div className="security-note">
                    <ShieldCheck size={16} />
                    <span>不会覆盖现有文件；失败时自动回滚</span>
                  </div>
                  <div>
                    <button
                      className="button button--ghost"
                      type="button"
                      disabled={busy}
                      onClick={() => void closeDialog()}
                    >
                      {plan ? "取消安装" : "关闭"}
                    </button>
                    {plan ? (
                      <button
                        className="button button--primary"
                        type="button"
                        disabled={!plan.canInstall || busy}
                        onClick={() => void commitPlan()}
                      >
                        {commit.isPending ? <LoaderCircle className="spin" size={16} /> : <Upload size={16} />}
                        {commit.isPending ? "正在安装…" : "确认安装"}
                      </button>
                    ) : null}
                  </div>
                </footer>
              </>
            )}
          </section>
        </div>
      ) : null}
    </>
  );
}

function commandErrorCode(error: unknown): string | null {
  if (typeof error === "object" && error !== null && "code" in error) {
    const code = (error as { code?: unknown }).code;
    return typeof code === "string" ? code : null;
  }
  return null;
}

function PlanConfirmation({ plan }: { plan: ModImportPlan }) {
  return (
    <div className="install-plan">
      <div className="install-plan__title">
        <div>
          <span className="eyebrow">安装确认</span>
          <h3>{plan.name}</h3>
          <p>{plan.author ?? "未知作者"} · v{plan.version ?? "未知版本"}</p>
        </div>
        <span className={`plan-status${plan.canInstall ? " is-ready" : " is-blocked"}`}>
          {plan.canInstall ? <ShieldCheck size={14} /> : <AlertTriangle size={14} />}
          {plan.canInstall ? "可以安装" : "已阻止"}
        </span>
      </div>
      <dl className="install-plan__facts">
        <div><dt>模组 ID</dt><dd>{plan.logicalId}</dd></div>
        <div><dt>内容规模</dt><dd>{plan.fileCount} 个文件 · {formatFileSize(plan.sizeBytes)}</dd></div>
        <div><dt>目标目录</dt><dd>{plan.destinationRelativePath}</dd></div>
        <div><dt>来源</dt><dd>{sourceKindLabel(plan.sourceKind)} · {plan.sourceName}</dd></div>
      </dl>
      {plan.description ? <p className="install-plan__description">{plan.description}</p> : null}
      {plan.blockingIssues.length ? (
        <div className="plan-messages plan-messages--error">
          <strong>必须先处理</strong>
          {plan.blockingIssues.map((message) => <span key={message}>{message}</span>)}
        </div>
      ) : null}
      {plan.warnings.length ? (
        <div className="plan-messages">
          <strong>检查提示</strong>
          {plan.warnings.map((message) => <span key={message}>{message}</span>)}
        </div>
      ) : null}
    </div>
  );
}

function sourceKindLabel(kind: ModImportPlan["sourceKind"]): string {
  if (kind === "directory") return "文件夹";
  if (kind === "sevenZip") return "7z";
  return kind.toUpperCase();
}

function progressPercent(progress: ModInstallProgress | null): number {
  if (!progress) return 0;
  if (progress.stage === "completed" || progress.stage === "ready") return 100;
  if (progress.totalBytes && progress.totalBytes > 0) {
    return Math.max(4, Math.min(96, (progress.processedBytes / progress.totalBytes) * 100));
  }
  if (progress.totalItems && progress.totalItems > 0) {
    return Math.max(4, Math.min(96, (progress.processedItems / progress.totalItems) * 100));
  }
  return progress.stage === "inspecting" ? 8 : progress.stage === "analyzing" ? 78 : 18;
}
