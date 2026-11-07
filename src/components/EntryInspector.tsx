import type { ClipboardEntry, EntryAction } from "../types";
import { entryTitle, formatBytes, formatDateTime, kindLabel } from "../lib/format";
import { ClipboardPreview } from "./ClipboardPreview";
import { Icon } from "./Icon";

interface EntryInspectorProps {
  entry: ClipboardEntry | null;
  busy: boolean;
  onAction: (action: EntryAction, entry: ClipboardEntry) => void;
}

export function EntryInspector({ entry, busy, onAction }: EntryInspectorProps) {
  if (!entry) {
    return (
      <aside className="inspector inspector--empty" aria-label="Clipboard item details">
        <span className="inspector-empty__art"><Icon name="clipboard" size={28}/></span>
        <strong>Select an item</strong>
        <p>Preview its full contents, then copy, paste, or pin it.</p>
        <div className="keyboard-tip"><kbd>↑</kbd><kbd>↓</kbd><span>navigate</span><kbd>Enter</kbd><span>paste</span></div>
      </aside>
    );
  }

  const details = [
    entry.sourceApp && ["Source", entry.sourceApp],
    ["Captured", formatDateTime(entry.createdAt)],
    entry.sizeBytes != null && ["Size", formatBytes(entry.sizeBytes)],
    entry.kind === "image" && entry.width && entry.height && ["Dimensions", `${entry.width} × ${entry.height}`],
    entry.kind === "files" && ["Items", String(entry.filePaths.length)],
    entry.copyCount && entry.copyCount > 1 && ["Copies", String(entry.copyCount)],
  ].filter(Boolean) as string[][];

  return (
    <aside className={`inspector ${busy ? "is-busy" : ""}`} aria-label="Clipboard item details">
      <div className="inspector-header">
        <div className={`entry-type-icon entry-type-icon--${entry.kind}`}>
          <Icon name={entry.kind === "text" ? "text" : entry.kind === "html" ? "code" : entry.kind === "image" ? "image" : "file"} size={17}/>
        </div>
        <div>
          <span className="inspector-kind">{kindLabel(entry.kind)}</span>
          <h3 title={entryTitle(entry)}>{entryTitle(entry)}</h3>
        </div>
      </div>

      <div className={`inspector-preview inspector-preview--${entry.kind}`}>
        <ClipboardPreview entry={entry} mode="detail"/>
      </div>

      <div className="inspector-actions">
        <button className="primary-button" type="button" disabled={busy} onClick={() => onAction("paste", entry)}><Icon name="paste" size={16}/>Paste</button>
        <button className="secondary-button" type="button" disabled={busy} onClick={() => onAction("copy", entry)}><Icon name="copy" size={16}/>Copy</button>
        <button className={`icon-button ${entry.pinned ? "is-pinned" : ""}`} type="button" disabled={busy} onClick={() => onAction("pin", entry)} title={entry.pinned ? "Unpin" : "Pin"} aria-label={entry.pinned ? "Unpin item" : "Pin item"}><Icon name="pin" size={16}/></button>
        <button className="icon-button danger-hover" type="button" disabled={busy} onClick={() => onAction("delete", entry)} title="Delete" aria-label="Delete item"><Icon name="trash" size={16}/></button>
      </div>

      {!!details.length && (
        <dl className="entry-details">
          {details.map(([label, value]) => <div key={label}><dt>{label}</dt><dd title={value}>{value}</dd></div>)}
        </dl>
      )}

      {entry.kind === "files" && entry.filePaths.length > 0 && (
        <details className="path-details">
          <summary>Full paths <Icon name="chevronDown" size={14}/></summary>
          {entry.filePaths.map((path, index) => <code key={`${path}-${index}`}>{path}</code>)}
        </details>
      )}
    </aside>
  );
}
