import { isTauri } from "@tauri-apps/api/core";
import { getCurrentWindow } from "@tauri-apps/api/window";
import {
  ImagePlus,
  LoaderCircle,
  RotateCcw,
  Trash2,
  Upload,
} from "lucide-react";
import { useEffect, useRef, useState } from "react";
import {
  commandErrorMessage,
  selectModPreviewImage,
} from "../../lib/tauri";
import { ModPreviewImage } from "./ModPreviewImage";
import {
  useClearModPreview,
  useSetModPreview,
} from "./useModManager";

interface ModPreviewEditorProps {
  modId: string;
  name: string;
  authorPreviewPath: string | null;
  hasCustomPreview: boolean;
  desktopReady: boolean;
}

export function ModPreviewEditor({
  modId,
  name,
  authorPreviewPath,
  hasCustomPreview,
  desktopReady,
}: ModPreviewEditorProps) {
  const setPreview = useSetModPreview(modId);
  const clearPreview = useClearModPreview(modId);
  const targetRef = useRef<HTMLDivElement>(null);
  const candidatePathsRef = useRef<string[]>([]);
  const applyPathRef = useRef<(path: string) => void>(() => {});
  const [dragActive, setDragActive] = useState(false);
  const [localError, setLocalError] = useState<string | null>(null);
  const [successMessage, setSuccessMessage] = useState<string | null>(null);
  const busy = setPreview.isPending || clearPreview.isPending;
  const hasPreview = Boolean(authorPreviewPath) || hasCustomPreview;

  function applyPath(path: string) {
    if (busy) return;
    setLocalError(null);
    setSuccessMessage(null);
    setPreview.mutate(path, {
      onSuccess: () => {
        setSuccessMessage("本地预览图已更新，不会修改模组原文件。");
      },
    });
  }
  applyPathRef.current = applyPath;

  useEffect(() => {
    if (!desktopReady || !isTauri()) return;
    let disposed = false;
    let removeListener: (() => void) | undefined;
    let scaleFactor = window.devicePixelRatio || 1;
    const appWindow = getCurrentWindow();

    void appWindow.scaleFactor().then((factor) => {
      if (!disposed && Number.isFinite(factor) && factor > 0) {
        scaleFactor = factor;
      }
    });
    void appWindow
      .onDragDropEvent((event) => {
        if (disposed) return;
        const payload = event.payload;
        if (payload.type === "leave") {
          candidatePathsRef.current = [];
          setDragActive(false);
          return;
        }
        if (payload.type === "enter") {
          candidatePathsRef.current = payload.paths;
          setDragActive(
            payload.paths.length === 1 &&
              isPointInsideTarget(payload.position, scaleFactor, targetRef.current),
          );
          return;
        }
        if (payload.type === "over") {
          setDragActive(
            candidatePathsRef.current.length === 1 &&
              isPointInsideTarget(payload.position, scaleFactor, targetRef.current),
          );
          return;
        }

        const inside = isPointInsideTarget(
          payload.position,
          scaleFactor,
          targetRef.current,
        );
        candidatePathsRef.current = [];
        setDragActive(false);
        if (!inside) return;
        if (payload.paths.length !== 1) {
          setLocalError("一次只能拖入一张图片。");
          return;
        }
        const [path] = payload.paths;
        if (!path) {
          setLocalError("拖入内容没有可读取的本地路径。");
          return;
        }
        applyPathRef.current(path);
      })
      .then((unlisten) => {
        if (disposed) unlisten();
        else removeListener = unlisten;
      });

    return () => {
      disposed = true;
      removeListener?.();
    };
  }, [desktopReady, modId]);

  async function pickImage() {
    if (busy || !desktopReady) return;
    try {
      const selected = await selectModPreviewImage();
      if (selected) applyPath(selected);
    } catch (error) {
      setLocalError(commandErrorMessage(error));
    }
  }

  function removeCustomPreview() {
    if (busy) return;
    setLocalError(null);
    setSuccessMessage(null);
    clearPreview.mutate(undefined, {
      onSuccess: () => {
        setSuccessMessage(
          authorPreviewPath ? "已恢复使用作者预览图。" : "本地预览图已移除。",
        );
      },
    });
  }

  const shownError =
    localError ??
    (setPreview.isError || clearPreview.isError
      ? commandErrorMessage(setPreview.error ?? clearPreview.error)
      : null);

  return (
    <div
      ref={targetRef}
      className={`mod-preview-editor${dragActive ? " is-drag-active" : ""}${
        hasPreview ? "" : " is-empty"
      }`}
    >
      <ModPreviewImage
        modId={modId}
        name={name}
        hasPreview={hasPreview}
        variant="detail"
      />
      <button
        className="mod-preview-editor__drop-target"
        type="button"
        disabled={!desktopReady || busy}
        aria-label={hasPreview ? `更换 ${name} 的预览图` : `为 ${name} 添加预览图`}
        aria-busy={busy}
        onClick={() => void pickImage()}
      >
        <span>
          {dragActive ? (
            <Upload size={30} />
          ) : busy ? (
            <LoaderCircle className="spin" size={28} />
          ) : (
            <ImagePlus size={28} />
          )}
          <strong>
            {dragActive
              ? "松开即可设置预览图"
              : busy
                ? "正在安全保存…"
                : hasPreview
                  ? "拖入或点击更换图片"
                  : "拖入或点击添加图片"}
          </strong>
          <small>PNG / JPG / WebP / GIF · 最大 16 MiB</small>
        </span>
      </button>
      {hasCustomPreview ? (
        <button
          className="mod-preview-editor__remove"
          type="button"
          disabled={busy}
          title={authorPreviewPath ? "恢复作者预览图" : "移除本地预览图"}
          aria-label={authorPreviewPath ? "恢复作者预览图" : "移除本地预览图"}
          onClick={removeCustomPreview}
        >
          {authorPreviewPath ? <RotateCcw size={17} /> : <Trash2 size={17} />}
        </button>
      ) : null}
      {shownError ? (
        <span className="mod-preview-editor__message is-error" role="alert">
          {shownError}
        </span>
      ) : successMessage ? (
        <span className="mod-preview-editor__message" role="status">
          {successMessage}
        </span>
      ) : null}
    </div>
  );
}

function isPointInsideTarget(
  position: { x: number; y: number },
  scaleFactor: number,
  target: HTMLDivElement | null,
): boolean {
  if (!target) return false;
  const bounds = target.getBoundingClientRect();
  const x = position.x / scaleFactor;
  const y = position.y / scaleFactor;
  return x >= bounds.left && x <= bounds.right && y >= bounds.top && y <= bounds.bottom;
}
