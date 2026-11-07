import { useEffect, useMemo, useState } from "react";
import type { ClipboardEntry } from "../types";
import { fileExtension, fileName, plainPreview } from "../lib/format";
import { localImageUrl } from "../lib/tauri";
import { Icon } from "./Icon";

interface ClipboardPreviewProps {
  entry: ClipboardEntry;
  mode?: "slot" | "row" | "detail";
}

function safeHtmlDocument(html: string): string {
  if (typeof DOMParser === "undefined") return "";
  const document = new DOMParser().parseFromString(html, "text/html");
  document.querySelectorAll("script,style,link,meta,iframe,object,embed,form,input,button,video,audio,svg").forEach((node) => node.remove());
  document.querySelectorAll("*").forEach((node) => {
    for (const attribute of Array.from(node.attributes)) {
      if (!new Set(["colspan", "rowspan"]).has(attribute.name.toLowerCase())) {
        node.removeAttribute(attribute.name);
      }
    }
  });
  return `<!doctype html><html><head><meta http-equiv="Content-Security-Policy" content="default-src 'none'; style-src 'unsafe-inline'"><style>body{font:13px/1.55 system-ui;color:#20242b;margin:16px;overflow-wrap:anywhere}table{border-collapse:collapse;max-width:100%}td,th{border:1px solid #d8dde5;padding:5px 8px}blockquote{border-left:3px solid #cbd2dc;margin-left:0;padding-left:12px;color:#59616d}pre{white-space:pre-wrap}</style></head><body>${document.body.innerHTML}</body></html>`;
}

export function ClipboardPreview({ entry, mode = "row" }: ClipboardPreviewProps) {
  const [imageFailed, setImageFailed] = useState(false);
  const imageUrl = localImageUrl(entry);
  const text = plainPreview(entry);
  const safeHtml = useMemo(() => (entry.html ? safeHtmlDocument(entry.html) : ""), [entry.html]);

  useEffect(() => setImageFailed(false), [imageUrl]);

  if (entry.kind === "image") {
    if (imageUrl && !imageFailed) {
      return (
        <div className={`entry-image entry-image--${mode}`}>
          <img src={imageUrl} alt="Clipboard image preview" draggable={false} onError={() => setImageFailed(true)} />
        </div>
      );
    }
    return (
      <div className={`preview-fallback preview-fallback--${mode}`}>
        <Icon name="image" size={mode === "detail" ? 30 : 20} />
        <span>Image preview unavailable</span>
      </div>
    );
  }

  if (entry.kind === "files") {
    const limit = mode === "detail" ? entry.filePaths.length : mode === "slot" ? 2 : 3;
    return (
      <div className={`file-preview file-preview--${mode}`}>
        {entry.filePaths.slice(0, limit).map((path, index) => (
          <div className="file-preview__row" key={`${path}-${index}`} title={path}>
            <span className="file-badge">{fileExtension(path)}</span>
            <span className="file-preview__name">{fileName(path)}</span>
          </div>
        ))}
        {entry.filePaths.length > limit && (
          <span className="file-preview__more">+{entry.filePaths.length - limit} more</span>
        )}
        {!entry.filePaths.length && <span className="preview-muted">File paths unavailable</span>}
      </div>
    );
  }

  if (entry.kind === "html" && mode === "detail" && safeHtml) {
    return (
      <iframe
        className="html-preview"
        sandbox=""
        srcDoc={safeHtml}
        title="Rich text preview"
      />
    );
  }

  return (
    <div className={`text-preview text-preview--${mode}`}>
      {text || <span className="preview-muted">No text preview</span>}
    </div>
  );
}
