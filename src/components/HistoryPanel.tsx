import type { RefObject } from "react";
import type { ClipboardEntry, ClipboardKind, EntryAction } from "../types";
import { entryTitle, formatRelativeTime, kindLabel, sameId } from "../lib/format";
import { ClipboardPreview } from "./ClipboardPreview";
import { Icon, type IconName } from "./Icon";

export type KindFilter = "all" | ClipboardKind;

const FILTERS: { value: KindFilter; label: string; icon?: IconName }[] = [
  { value: "all", label: "All" },
  { value: "text", label: "Text", icon: "text" },
  { value: "html", label: "Rich", icon: "code" },
  { value: "image", label: "Images", icon: "image" },
  { value: "files", label: "Files", icon: "file" },
];

interface HistoryPanelProps {
  entries: ClipboardEntry[];
  selectedId: ClipboardEntry["id"] | null;
  query: string;
  kindFilter: KindFilter;
  pinnedOnly: boolean;
  loading: boolean;
  loadingMore: boolean;
  hasMore: boolean;
  busyIds: Set<string>;
  searchRef: RefObject<HTMLInputElement | null>;
  listRef: RefObject<HTMLDivElement | null>;
  onQueryChange: (value: string) => void;
  onKindFilterChange: (value: KindFilter) => void;
  onPinnedOnlyChange: (value: boolean) => void;
  onSelect: (entry: ClipboardEntry) => void;
  onAction: (action: EntryAction, entry: ClipboardEntry) => void;
  onClear: () => void;
  onLoadMore: () => void;
  onRetry: () => void;
  loadError?: string;
}

function HistorySkeleton() {
  return (
    <div className="history-skeleton" aria-label="Loading clipboard history">
      {[0, 1, 2, 3, 4].map((row) => (
        <div className="history-skeleton__row" key={row}>
          <span className="skeleton-square" />
          <span><i className="skeleton-line"/><i className="skeleton-line skeleton-line--short"/></span>
        </div>
      ))}
    </div>
  );
}

export function HistoryPanel({
  entries,
  selectedId,
  query,
  kindFilter,
  pinnedOnly,
  loading,
  loadingMore,
  hasMore,
  busyIds,
  searchRef,
  listRef,
  onQueryChange,
  onKindFilterChange,
  onPinnedOnlyChange,
  onSelect,
  onAction,
  onClear,
  onLoadMore,
  onRetry,
  loadError,
}: HistoryPanelProps) {
  return (
    <section className="history-panel" aria-labelledby="history-heading">
      <div className="history-toolbar">
        <div className="history-title">
          <div>
            <p className="eyebrow">Library</p>
            <h2 id="history-heading">Full history</h2>
          </div>
          <span className="count-pill">{entries.length}{hasMore ? "+" : ""}</span>
        </div>

        <label className="search-box">
          <Icon name="search" size={17}/>
          <input
            ref={searchRef}
            type="search"
            value={query}
            onChange={(event) => onQueryChange(event.target.value)}
            placeholder="Search clipboard"
            aria-label="Search clipboard history"
            autoComplete="off"
            spellCheck={false}
          />
          {query ? (
            <button type="button" className="search-clear" onClick={() => onQueryChange("")} aria-label="Clear search">
              <Icon name="close" size={14}/>
            </button>
          ) : <kbd>Ctrl F</kbd>}
        </label>

        <div className="history-filters" aria-label="Filter clipboard history">
          <div className="segmented-control">
            {FILTERS.map((filter) => (
              <button
                key={filter.value}
                type="button"
                className={kindFilter === filter.value ? "is-active" : ""}
                aria-pressed={kindFilter === filter.value}
                onClick={() => onKindFilterChange(filter.value)}
              >
                {filter.icon && <Icon name={filter.icon} size={14}/>}<span>{filter.label}</span>
              </button>
            ))}
          </div>
          <button
            type="button"
            className={`filter-pin ${pinnedOnly ? "is-active" : ""}`}
            aria-pressed={pinnedOnly}
            onClick={() => onPinnedOnlyChange(!pinnedOnly)}
          >
            <Icon name="pin" size={14}/><span>Pinned</span>
          </button>
          <button type="button" className="icon-button danger-hover" onClick={onClear} title="Clear unpinned history" aria-label="Clear history">
            <Icon name="trash" size={16}/>
          </button>
        </div>
      </div>

      <div className="history-list" ref={listRef} role="listbox" aria-label="Clipboard history">
        {loading ? <HistorySkeleton /> : loadError && !entries.length ? (
          <div className="panel-state panel-state--error">
            <span className="state-icon"><Icon name="warning"/></span>
            <strong>History couldn’t be loaded</strong>
            <p>{loadError}</p>
            <button className="secondary-button" type="button" onClick={onRetry}><Icon name="refresh" size={15}/>Try again</button>
          </div>
        ) : !entries.length ? (
          <div className="panel-state">
            <span className="state-icon"><Icon name={query || kindFilter !== "all" || pinnedOnly ? "search" : "clipboard"}/></span>
            <strong>{query || kindFilter !== "all" || pinnedOnly ? "No matching items" : "Your clipboard is ready"}</strong>
            <p>{query || kindFilter !== "all" || pinnedOnly ? "Try a different search or filter." : "Copy text, images, or files and they’ll appear here."}</p>
            {(query || kindFilter !== "all" || pinnedOnly) && (
              <button className="secondary-button" type="button" onClick={() => { onQueryChange(""); onKindFilterChange("all"); onPinnedOnlyChange(false); }}>Reset filters</button>
            )}
          </div>
        ) : (
          <>
            {entries.map((entry) => {
              const selected = selectedId != null && sameId(selectedId, entry.id);
              const busy = busyIds.has(String(entry.id));
              return (
                <article
                  key={String(entry.id)}
                  role="option"
                  aria-selected={selected}
                  tabIndex={selected ? 0 : -1}
                  data-entry-id={String(entry.id)}
                  className={`history-row ${selected ? "is-selected" : ""} ${busy ? "is-busy" : ""}`}
                  onClick={() => onSelect(entry)}
                  onDoubleClick={() => onAction("paste", entry)}
                  onKeyDown={(event) => {
                    if (event.key === "Enter") onAction("paste", entry);
                    if (event.key === " " && !event.ctrlKey) { event.preventDefault(); onSelect(entry); }
                  }}
                >
                  <div className={`entry-type-icon entry-type-icon--${entry.kind}`}>
                    <Icon name={entry.kind === "text" ? "text" : entry.kind === "html" ? "code" : entry.kind === "image" ? "image" : "file"} size={17}/>
                  </div>
                  <div className="history-row__content">
                    <div className="history-row__topline">
                      <span className="history-row__title">{entryTitle(entry)}</span>
                      {entry.pinned && <Icon name="pin" className="pinned-mark" size={13}/>} 
                    </div>
                    <div className="history-row__meta">
                      <span>{kindLabel(entry.kind)}</span>
                      {entry.sourceApp && <><span>·</span><span className="truncate">{entry.sourceApp}</span></>}
                      <span>·</span><time dateTime={new Date(entry.createdAt).toISOString()}>{formatRelativeTime(entry.createdAt)}</time>
                    </div>
                  </div>
                  {entry.kind === "image" && <div className="row-thumbnail"><ClipboardPreview entry={entry} mode="row"/></div>}
                  <div className="row-actions">
                    <button type="button" onClick={(event) => { event.stopPropagation(); onAction("copy", entry); }} title="Copy to clipboard" aria-label="Copy item"><Icon name="copy" size={15}/></button>
                    <button type="button" className="row-paste" onClick={(event) => { event.stopPropagation(); onAction("paste", entry); }} title="Paste into previous app" aria-label="Paste item"><Icon name="paste" size={15}/></button>
                    <button type="button" className={entry.pinned ? "is-pinned" : ""} onClick={(event) => { event.stopPropagation(); onAction("pin", entry); }} title={entry.pinned ? "Unpin" : "Pin"} aria-label={entry.pinned ? "Unpin item" : "Pin item"}><Icon name="pin" size={15}/></button>
                    <button type="button" className="danger-hover" onClick={(event) => { event.stopPropagation(); onAction("delete", entry); }} title="Delete" aria-label="Delete item"><Icon name="trash" size={15}/></button>
                  </div>
                </article>
              );
            })}
          </>
        )}
        {!loading && hasMore && (
          <button className="load-more" type="button" disabled={loadingMore} onClick={onLoadMore}>
            {loadingMore ? <span className="spinner"/> : <Icon name="chevronDown" size={16}/>} {loadingMore ? "Loading…" : "Load older items"}
          </button>
        )}
      </div>
    </section>
  );
}
