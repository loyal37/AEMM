import { ImageOff } from "lucide-react";
import { useModPreview } from "./useModManager";

interface ModPreviewImageProps {
  modId: string;
  name: string;
  hasPreview: boolean;
  variant: "card" | "list" | "detail";
}

export function ModPreviewImage({
  modId,
  name,
  hasPreview,
  variant,
}: ModPreviewImageProps) {
  const preview = useModPreview(modId, hasPreview);
  return (
    <div className={`mod-preview mod-preview--${variant}`}>
      {preview.data?.dataUrl ? (
        <img src={preview.data.dataUrl} alt={`${name} 预览`} loading="lazy" />
      ) : (
        <div className="mod-preview__placeholder" aria-label={`${name} 暂无预览`}>
          <ImageOff size={variant === "detail" ? 34 : 20} strokeWidth={1.5} />
          {variant !== "list" ? <span>{name.slice(0, 2).toUpperCase()}</span> : null}
        </div>
      )}
    </div>
  );
}
