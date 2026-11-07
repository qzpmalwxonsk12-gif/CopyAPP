import type { ClipboardEntry, EntryAction } from "../types";
import { entryTitle, formatRelativeTime, kindLabel } from "../lib/format";
import { ClipboardPreview } from "./ClipboardPreview";
import { Icon } from "./Icon";

const DEFAULT_SLOT_LABELS = ["1", "2", "3", "4", "5", "6", "7", "8", "9", "0"];

interface RecentSlotsProps {
  entries: ClipboardEntry[];
  slotCount: number;
  loading: boolean;
  busyIds: Set<string>;
  onAction: (action: EntryAction, entry: ClipboardEntry) => void;
  onSelect: (entry: ClipboardEntry) => void;
}

export function RecentSlots({ entries, slotCount, loading, busyIds, onAction, onSelect }: RecentSlotsProps) {
  const slotLabels = Array.from({ length: Math.max(10, slotCount) }, (_, index) =>
    DEFAULT_SLOT_LABELS[index] ?? String(index + 1),
  );
  return (
    <section className="recent-section" aria-labelledby="recent-heading">
      <div className="section-heading">
        <div>
          <p className="eyebrow">Quick paste</p>
          <h2 id="recent-heading">Quick-paste slots</h2>
          <p className="slot-overflow-note">Newest first. When full, the oldest slot is replaced and remains in history.</p>
        </div>
        <p className="section-hint"><kbd>Ctrl</kbd><span>+</span><kbd>V</kbd><span>+</span><kbd>number</kbd></p>
      </div>

      <div className="slot-grid" role="list" aria-label={`${slotLabels.length} quick-paste clipboard slots`}>
        {slotLabels.map((label, index) => {
          const entry = entries[index];
          if (!entry) {
            return (
              <div className={`slot-card slot-card--empty ${loading ? "is-loading" : ""}`} key={label} role="listitem">
                <span className="slot-number">{label}</span>
                <div className="slot-empty-content">
                  {loading ? <span className="skeleton-line" /> : <><Icon name="clipboard" size={17}/><span>Empty slot</span></>}
                </div>
              </div>
            );
          }
          const busy = busyIds.has(String(entry.id));
          return (
            <article
              className={`slot-card slot-card--${entry.kind} ${busy ? "is-busy" : ""}`}
              key={String(entry.id)}
              role="listitem"
              tabIndex={0}
              onClick={() => onAction("paste", entry)}
              onDoubleClick={(event) => { event.stopPropagation(); onSelect(entry); }}
              onKeyDown={(event) => {
                if (event.key === "Enter" || event.key === " ") {
                  event.preventDefault();
                  onAction("paste", entry);
                }
              }}
              title={`Paste slot ${label}: ${entryTitle(entry)}`}
            >
              <span className="slot-number">{label}</span>
              <button
                aria-label={entry.pinned ? "Unpin item" : "Pin item"}
                className={`slot-pin ${entry.pinned ? "is-pinned" : ""}`}
                onClick={(event) => { event.stopPropagation(); onAction("pin", entry); }}
                type="button"
              >
                <Icon name="pin" size={14}/>
              </button>
              <ClipboardPreview entry={entry} mode="slot" />
              <div className="slot-card__footer">
                <span className="kind-dot" data-kind={entry.kind}/>
                <span>{kindLabel(entry.kind)}</span>
                <span className="separator-dot">·</span>
                <time>{formatRelativeTime(entry.createdAt)}</time>
              </div>
            </article>
          );
        })}
      </div>
    </section>
  );
}
