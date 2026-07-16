import { useVirtualizer } from "@tanstack/react-virtual";
import { File } from "lucide-react";
import { useRef } from "react";
import type { ModFileDetails } from "../../types/app";
import { formatFileSize } from "./modQuery";

export function VirtualModFileList({ files }: { files: ModFileDetails[] }) {
  const scrollRef = useRef<HTMLDivElement>(null);
  const virtualizer = useVirtualizer({
    count: files.length,
    getScrollElement: () => scrollRef.current,
    estimateSize: () => 49,
    overscan: 8,
  });

  return (
    <div className="mod-files">
      <div className="mod-files__header" aria-hidden="true">
        <span>相对路径</span>
        <span>类型</span>
        <span>大小</span>
        <span>Hash</span>
      </div>
      <div className="mod-files__scroll" ref={scrollRef}>
        <div
          className="mod-files__canvas"
          style={{ height: `${virtualizer.getTotalSize()}px` }}
        >
          {virtualizer.getVirtualItems().map((virtualRow) => {
            const file = files[virtualRow.index];
            if (!file) return null;
            return (
              <div
                className="mod-file-row"
                data-index={virtualRow.index}
                key={virtualRow.key}
                ref={virtualizer.measureElement}
                style={{ transform: `translateY(${virtualRow.start}px)` }}
              >
                <span className="mod-file-row__path" title={file.sourcePath}>
                  <File size={14} /> {file.sourcePath}
                </span>
                <span>{file.fileRole}</span>
                <span>{formatFileSize(file.sizeBytes)}</span>
                <code title={file.contentHash ?? "未计算"}>
                  {file.contentHash?.slice(0, 12) ?? "—"}
                </code>
              </div>
            );
          })}
        </div>
      </div>
    </div>
  );
}
