import { useCallback, useEffect, useMemo, useRef, useState } from "react";
import { EntryInspector } from "./components/EntryInspector";
import { HistoryPanel, type KindFilter } from "./components/HistoryPanel";
import { Icon } from "./components/Icon";
import { RecentSlots } from "./components/RecentSlots";
import { SettingsPage } from "./components/SettingsPage";
import { errorMessage, sameId } from "./lib/format";
import { clipboardApi, isTauri, listenForBackendChanges } from "./lib/tauri";
import type { AppSettings, AppStatus, ClipboardEntry, EntryAction, ToastMessage, ToastTone } from "./types";

const PAGE_SIZE = 50;

function useDebouncedValue<T>(value: T, delay: number): T {
  const [debounced, setDebounced] = useState(value);
  useEffect(() => {
    const timeout = window.setTimeout(() => setDebounced(value), delay);
    return () => window.clearTimeout(timeout);
  }, [delay, value]);
  return debounced;
}

function ToastStack({ toasts, dismiss }: { toasts: ToastMessage[]; dismiss: (id: number) => void }) {
  return (
    <div className="toast-stack" role="region" aria-label="Notifications">
      {toasts.map((toast) => (
        <div className={`toast toast--${toast.tone}`} key={toast.id} role={toast.tone === "error" ? "alert" : "status"}>
          <span className="toast__icon"><Icon name={toast.tone === "error" ? "warning" : toast.tone === "success" ? "check" : "clipboard"} size={16}/></span>
          <span><strong>{toast.title}</strong>{toast.detail && <small>{toast.detail}</small>}</span>
          <button type="button" onClick={() => dismiss(toast.id)} aria-label="Dismiss notification"><Icon name="close" size={14}/></button>
        </div>
      ))}
    </div>
  );
}

function ConfirmClearDialog({ open, busy, onCancel, onConfirm }: { open: boolean; busy: boolean; onCancel: () => void; onConfirm: () => void }) {
  const confirmRef = useRef<HTMLButtonElement>(null);
  useEffect(() => {
    if (open) window.setTimeout(() => confirmRef.current?.focus(), 50);
  }, [open]);
  if (!open) return null;
  return (
    <div className="modal-backdrop" role="presentation" onMouseDown={(event) => { if (event.target === event.currentTarget && !busy) onCancel(); }}>
      <div className="modal" role="alertdialog" aria-modal="true" aria-labelledby="clear-title" aria-describedby="clear-description" onKeyDown={(event) => { if (event.key === "Escape" && !busy) onCancel(); }}>
        <span className="modal__icon"><Icon name="trash" size={21}/></span>
        <div><h2 id="clear-title">Clear clipboard history?</h2><p id="clear-description">Every unpinned item will be permanently deleted. Your pinned items will remain available.</p></div>
        <div className="modal__actions">
          <button className="secondary-button" type="button" disabled={busy} onClick={onCancel}>Cancel</button>
          <button ref={confirmRef} className="danger-button" type="button" disabled={busy} onClick={onConfirm}>{busy ? <span className="spinner"/> : <Icon name="trash" size={15}/>} {busy ? "Clearing…" : "Clear history"}</button>
        </div>
      </div>
    </div>
  );
}

export default function App() {
  const [view, setView] = useState<"library" | "settings">("library");
  const [recent, setRecent] = useState<ClipboardEntry[]>([]);
  const [history, setHistory] = useState<ClipboardEntry[]>([]);
  const [selectedEntry, setSelectedEntry] = useState<ClipboardEntry | null>(null);
  const [query, setQuery] = useState("");
  const debouncedQuery = useDebouncedValue(query.trim(), 220);
  const [kindFilter, setKindFilter] = useState<KindFilter>("all");
  const [pinnedOnly, setPinnedOnly] = useState(false);
  const [hasMore, setHasMore] = useState(false);
  const [loadingHistory, setLoadingHistory] = useState(true);
  const [loadingMore, setLoadingMore] = useState(false);
  const [loadingRecent, setLoadingRecent] = useState(true);
  const [historyError, setHistoryError] = useState<string>();
  const [settings, setSettings] = useState<AppSettings | null>(null);
  const [status, setStatus] = useState<AppStatus | null>(null);
  const [settingsLoading, setSettingsLoading] = useState(true);
  const [settingsSaving, setSettingsSaving] = useState(false);
  const [pauseBusy, setPauseBusy] = useState(false);
  const [busyIds, setBusyIds] = useState<Set<string>>(new Set());
  const [toasts, setToasts] = useState<ToastMessage[]>([]);
  const [clearDialogOpen, setClearDialogOpen] = useState(false);
  const [clearing, setClearing] = useState(false);
  const searchRef = useRef<HTMLInputElement>(null);
  const listRef = useRef<HTMLDivElement>(null);
  const requestGeneration = useRef(0);
  const toastCounter = useRef(0);

  const notify = useCallback((tone: ToastTone, title: string, detail?: string) => {
    const id = ++toastCounter.current;
    setToasts((current) => [...current.slice(-3), { id, tone, title, detail }]);
    window.setTimeout(() => setToasts((current) => current.filter((toast) => toast.id !== id)), tone === "error" ? 6000 : 2800);
  }, []);

  const dismissToast = useCallback((id: number) => {
    setToasts((current) => current.filter((toast) => toast.id !== id));
  }, []);

  const loadRecent = useCallback(async () => {
    try {
      setRecent(await clipboardApi.getRecentSlots());
    } catch (error) {
      if (isTauri) notify("error", "Recent items unavailable", errorMessage(error));
    } finally {
      setLoadingRecent(false);
    }
  }, [notify]);

  const loadStatus = useCallback(async () => {
    try { setStatus(await clipboardApi.getStatus()); }
    catch (error) { if (isTauri) console.warn("Unable to read service status", error); }
  }, []);

  const loadSettings = useCallback(async () => {
    try { setSettings(await clipboardApi.getSettings()); }
    catch (error) { notify("error", "Settings unavailable", errorMessage(error)); }
    finally { setSettingsLoading(false); }
  }, [notify]);

  const loadFirstPage = useCallback(async (background = false) => {
    const generation = ++requestGeneration.current;
    if (!background) setLoadingHistory(true);
    setHistoryError(undefined);
    try {
      const result = await clipboardApi.getHistory(debouncedQuery, PAGE_SIZE, 0);
      if (generation !== requestGeneration.current) return;
      setHistory(result.entries);
      setHasMore(result.hasMore ?? (result.total != null ? result.entries.length < result.total : result.entries.length === PAGE_SIZE));
      setSelectedEntry((selected) => {
        if (!selected) return result.entries[0] ?? null;
        return result.entries.find((entry) => sameId(entry.id, selected.id)) ?? result.entries[0] ?? null;
      });
    } catch (error) {
      if (generation !== requestGeneration.current) return;
      const message = errorMessage(error);
      setHistoryError(message);
      if (background) notify("error", "Couldn’t refresh history", message);
    } finally {
      if (generation === requestGeneration.current) setLoadingHistory(false);
    }
  }, [debouncedQuery, notify]);

  const loadMoreHistory = useCallback(async () => {
    if (loadingMore || !hasMore) return;
    setLoadingMore(true);
    try {
      const result = await clipboardApi.getHistory(debouncedQuery, PAGE_SIZE, history.length);
      setHistory((current) => {
        const known = new Set(current.map((entry) => String(entry.id)));
        return [...current, ...result.entries.filter((entry) => !known.has(String(entry.id)))];
      });
      setHasMore(result.hasMore ?? (result.total != null ? history.length + result.entries.length < result.total : result.entries.length === PAGE_SIZE));
    } catch (error) {
      notify("error", "Older items couldn’t be loaded", errorMessage(error));
    } finally {
      setLoadingMore(false);
    }
  }, [debouncedQuery, hasMore, history.length, loadingMore, notify]);

  useEffect(() => {
    void loadRecent();
    void loadStatus();
    void loadSettings();
  }, [loadRecent, loadSettings, loadStatus]);

  useEffect(() => { void loadFirstPage(); }, [loadFirstPage]);

  useEffect(() => {
    let disposed = false;
    let unlisten: (() => void) | undefined;
    let refreshTimer: number | undefined;
    void listenForBackendChanges(
      () => {
        window.clearTimeout(refreshTimer);
        refreshTimer = window.setTimeout(() => {
          if (disposed) return;
          void Promise.all([loadRecent(), loadFirstPage(true), loadStatus(), loadSettings()]);
        }, 80);
      },
      (message) => {
        if (disposed) return;
        setStatus((current) => ({
          recordingPaused: current?.recordingPaused ?? false,
          databaseReady: current?.databaseReady ?? false,
          hookActive: current?.hookActive ?? false,
          clipboardWatcherActive: current?.clipboardWatcherActive ?? false,
          entryCount: current?.entryCount,
          version: current?.version,
          lastError: message,
        }));
        notify("error", "Clipboard service error", message);
      },
    ).then((cleanup) => { if (disposed) cleanup(); else unlisten = cleanup; });
    return () => { disposed = true; window.clearTimeout(refreshTimer); unlisten?.(); };
  }, [loadFirstPage, loadRecent, loadSettings, loadStatus, notify]);

  useEffect(() => {
    const theme = settings?.theme ?? "system";
    const media = window.matchMedia("(prefers-color-scheme: dark)");
    const apply = () => { document.documentElement.dataset.theme = theme === "system" ? (media.matches ? "dark" : "light") : theme; };
    apply();
    media.addEventListener("change", apply);
    return () => media.removeEventListener("change", apply);
  }, [settings?.theme]);

  const visibleEntries = useMemo(() => history.filter((entry) =>
    (kindFilter === "all" || entry.kind === kindFilter) && (!pinnedOnly || entry.pinned),
  ), [history, kindFilter, pinnedOnly]);

  useEffect(() => {
    if (!visibleEntries.length) {
      if (selectedEntry && history.some((entry) => sameId(entry.id, selectedEntry.id))) setSelectedEntry(null);
      return;
    }
    if (!selectedEntry || !visibleEntries.some((entry) => sameId(entry.id, selectedEntry.id))) setSelectedEntry(visibleEntries[0]);
  }, [history, selectedEntry, visibleEntries]);

  // History rows are intentionally lightweight. Fetch the complete payload only
  // for the active item so large HTML/image entries do not slow down scrolling.
  useEffect(() => {
    if (!selectedEntry) return;
    const selectedId = selectedEntry.id;
    let cancelled = false;
    void clipboardApi.getEntry(selectedId).then((detail) => {
      if (cancelled) return;
      setSelectedEntry((current) => current && sameId(current.id, selectedId) ? detail : current);
    }).catch((error) => {
      if (!cancelled) console.warn("Unable to load full clipboard item", error);
    });
    return () => { cancelled = true; };
  }, [selectedEntry?.id]);

  const markBusy = (id: ClipboardEntry["id"], busy: boolean) => {
    setBusyIds((current) => {
      const next = new Set(current);
      if (busy) next.add(String(id)); else next.delete(String(id));
      return next;
    });
  };

  const replaceEntry = (id: ClipboardEntry["id"], patch: Partial<ClipboardEntry>) => {
    const apply = (entry: ClipboardEntry) => sameId(entry.id, id) ? { ...entry, ...patch } : entry;
    setHistory((current) => current.map(apply));
    setRecent((current) => current.map(apply));
    setSelectedEntry((current) => current && sameId(current.id, id) ? apply(current) : current);
  };

  const handleEntryAction = useCallback(async (action: EntryAction, entry: ClipboardEntry) => {
    if (busyIds.has(String(entry.id))) return;
    markBusy(entry.id, true);
    try {
      if (action === "paste") {
        await clipboardApi.pasteEntry(entry.id);
        notify("success", "Pasted", "Sent to the previous application");
      } else if (action === "copy") {
        await clipboardApi.copyEntry(entry.id);
        notify("success", "Copied to clipboard");
      } else if (action === "pin") {
        const nextPinned = !entry.pinned;
        replaceEntry(entry.id, { pinned: nextPinned });
        try {
          await clipboardApi.togglePin(entry.id);
          notify("success", nextPinned ? "Item pinned" : "Item unpinned");
        } catch (error) {
          replaceEntry(entry.id, { pinned: entry.pinned });
          throw error;
        }
      } else {
        await clipboardApi.deleteEntry(entry.id);
        setHistory((current) => current.filter((item) => !sameId(item.id, entry.id)));
        setRecent((current) => current.filter((item) => !sameId(item.id, entry.id)));
        setSelectedEntry((current) => current && sameId(current.id, entry.id) ? null : current);
        notify("success", "Item deleted");
      }
    } catch (error) {
      notify("error", `${action.charAt(0).toUpperCase() + action.slice(1)} failed`, errorMessage(error));
    } finally {
      markBusy(entry.id, false);
    }
  // busyIds is intentionally read to prevent duplicate clicks while the command is active.
  }, [busyIds, notify]);

  const handlePausedChange = useCallback(async (paused: boolean) => {
    if (pauseBusy) return;
    setPauseBusy(true);
    setStatus((current) => current ? { ...current, recordingPaused: paused } : current);
    try {
      await clipboardApi.setRecordingPaused(paused);
      notify("success", paused ? "Clipboard recording paused" : "Clipboard recording resumed");
      await loadStatus();
    } catch (error) {
      setStatus((current) => current ? { ...current, recordingPaused: !paused } : current);
      notify("error", "Recording state couldn’t be changed", errorMessage(error));
    } finally {
      setPauseBusy(false);
    }
  }, [loadStatus, notify, pauseBusy]);

  const saveSettings = async (next: AppSettings) => {
    setSettingsSaving(true);
    try {
      const updated = await clipboardApi.updateSettings(next);
      setSettings(updated);
      notify("success", "Settings saved");
    } catch (error) {
      notify("error", "Settings couldn’t be saved", errorMessage(error));
    } finally { setSettingsSaving(false); }
  };

  const clearHistory = async () => {
    setClearing(true);
    try {
      await clipboardApi.clearHistory();
      setClearDialogOpen(false);
      notify("success", "History cleared", "Pinned items were kept");
      await Promise.all([loadRecent(), loadFirstPage(true), loadStatus()]);
    } catch (error) {
      notify("error", "History couldn’t be cleared", errorMessage(error));
    } finally { setClearing(false); }
  };

  const openTextHistory = async () => {
    try {
      await clipboardApi.openTextHistory();
      notify("success", "Text history opened", "The file stays synchronized with your saved history");
    } catch (error) {
      notify("error", "Text history couldnâ€™t be opened", errorMessage(error));
    }
  };

  const clearTextHistory = async () => {
    try {
      await clipboardApi.clearTextHistory();
      notify("success", "Text history cleared", "Your clipboard database was not changed");
    } catch (error) {
      notify("error", "Text history couldnâ€™t be cleared", errorMessage(error));
    }
  };

  const convertTextHistoryToPdf = async () => {
    try {
      await clipboardApi.convertTextHistoryToPdf();
      notify("success", "History PDF created", "The converted PDF has been opened");
    } catch (error) {
      notify("error", "History PDF couldnâ€™t be created", errorMessage(error));
    }
  };

  useEffect(() => {
    const onKeyDown = (event: globalThis.KeyboardEvent) => {
      const target = event.target as HTMLElement | null;
      const editing = target?.matches("input, textarea, [contenteditable=true]");
      if (event.ctrlKey && event.key.toLowerCase() === "f") {
        event.preventDefault(); setView("library"); window.setTimeout(() => searchRef.current?.focus(), 0); return;
      }
      if (event.ctrlKey && event.key === ",") { event.preventDefault(); setView("settings"); return; }
      if (event.key === "Escape") {
        if (clearDialogOpen) { setClearDialogOpen(false); return; }
        if (query) { setQuery(""); return; }
        if (view === "settings") { setView("library"); return; }
        if (isTauri) void clipboardApi.toggleWindow();
        return;
      }
      if (view !== "library" || editing || clearDialogOpen || !visibleEntries.length) return;
      const index = selectedEntry ? visibleEntries.findIndex((entry) => sameId(entry.id, selectedEntry.id)) : -1;
      if (event.key === "ArrowDown" || event.key === "ArrowUp") {
        event.preventDefault();
        const delta = event.key === "ArrowDown" ? 1 : -1;
        const nextIndex = index < 0 ? 0 : Math.min(visibleEntries.length - 1, Math.max(0, index + delta));
        const next = visibleEntries[nextIndex];
        setSelectedEntry(next);
        window.requestAnimationFrame(() => listRef.current?.querySelector<HTMLElement>(`[data-entry-id="${CSS.escape(String(next.id))}"]`)?.focus());
      } else if (event.key === "Enter" && selectedEntry) {
        event.preventDefault(); void handleEntryAction("paste", selectedEntry);
      } else if (event.ctrlKey && event.key.toLowerCase() === "c" && selectedEntry) {
        event.preventDefault(); void handleEntryAction("copy", selectedEntry);
      } else if (event.key === "Delete" && selectedEntry) {
        event.preventDefault(); void handleEntryAction("delete", selectedEntry);
      } else if (event.key === "/") {
        event.preventDefault(); searchRef.current?.focus();
      }
    };
    window.addEventListener("keydown", onKeyDown);
    return () => window.removeEventListener("keydown", onKeyDown);
  }, [clearDialogOpen, handleEntryAction, query, selectedEntry, view, visibleEntries]);

  const serviceHealthy = Boolean(
    status?.databaseReady && status?.clipboardWatcherActive && status?.hookActive && !status?.lastError,
  );

  return (
    <div className="app-shell">
      <header className="app-header" data-tauri-drag-region>
        <button className="brand" type="button" onClick={() => setView("library")} aria-label="Open clipboard library">
          <span className="brand-mark"><Icon name="clipboard" size={19}/></span>
          <span><strong>CopyAPP</strong><small>Clipboard, remembered</small></span>
        </button>
        <nav className="main-nav" aria-label="Main navigation">
          <button type="button" className={view === "library" ? "is-active" : ""} onClick={() => setView("library")}><Icon name="history" size={16}/>Library</button>
          <button type="button" className={view === "settings" ? "is-active" : ""} onClick={() => setView("settings")}><Icon name="settings" size={16}/>Settings</button>
        </nav>
        <div className="header-actions">
          <span className={`service-health ${serviceHealthy ? "is-healthy" : ""}`} title={serviceHealthy ? "All services active" : "A background service needs attention"}><i/>{serviceHealthy ? "Ready" : "Check status"}</span>
          <button
            className={`recording-button ${status?.recordingPaused ? "is-paused" : ""}`}
            type="button"
            disabled={pauseBusy || !status}
            onClick={() => void handlePausedChange(!status?.recordingPaused)}
            title={status?.recordingPaused ? "Resume clipboard recording" : "Pause clipboard recording"}
          >
            <Icon name={status?.recordingPaused ? "play" : "pause"} size={14}/>{status?.recordingPaused ? "Resume" : "Pause"}
          </button>
        </div>
      </header>

      {!isTauri && <div className="environment-banner"><Icon name="warning" size={15}/><span>Desktop services aren’t connected in this browser preview. Run the Tauri app to capture your clipboard.</span></div>}
      {status?.lastError && <div className="service-error"><Icon name="warning" size={15}/><span>{status.lastError}</span></div>}

      {view === "library" ? (
        <main className="library-page">
          <RecentSlots entries={recent} slotCount={settings?.slotShortcuts.length ?? 10} loading={loadingRecent} busyIds={busyIds} onAction={(action, entry) => void handleEntryAction(action, entry)} onSelect={setSelectedEntry}/>
          <div className="history-workspace">
            <HistoryPanel
              entries={visibleEntries}
              selectedId={selectedEntry?.id ?? null}
              query={query}
              kindFilter={kindFilter}
              pinnedOnly={pinnedOnly}
              loading={loadingHistory}
              loadingMore={loadingMore}
              hasMore={hasMore}
              busyIds={busyIds}
              searchRef={searchRef}
              listRef={listRef}
              onQueryChange={setQuery}
              onKindFilterChange={setKindFilter}
              onPinnedOnlyChange={setPinnedOnly}
              onSelect={setSelectedEntry}
              onAction={(action, entry) => void handleEntryAction(action, entry)}
              onClear={() => setClearDialogOpen(true)}
              onLoadMore={() => void loadMoreHistory()}
              onRetry={() => void loadFirstPage()}
              loadError={historyError}
            />
            <EntryInspector entry={selectedEntry} busy={selectedEntry ? busyIds.has(String(selectedEntry.id)) : false} onAction={(action, entry) => void handleEntryAction(action, entry)}/>
          </div>
        </main>
      ) : (
        <SettingsPage settings={settings} status={status} loading={settingsLoading} saving={settingsSaving} onSave={saveSettings} onPausedChange={handlePausedChange} onClearHistory={() => setClearDialogOpen(true)} onOpenTextHistory={openTextHistory} onClearTextHistory={clearTextHistory} onConvertTextHistoryToPdf={convertTextHistoryToPdf}/>
      )}

      <footer className="app-footer">
        <span><i className={status?.recordingPaused ? "footer-dot is-paused" : "footer-dot"}/>{status?.recordingPaused ? "Recording paused" : "Watching clipboard"}</span>
        <span className="footer-shortcuts"><kbd>↑</kbd><kbd>↓</kbd> navigate <kbd>Enter</kbd> paste <kbd>Ctrl C</kbd> copy <kbd>Esc</kbd> hide</span>
        <span>{status?.entryCount != null ? `${status.entryCount.toLocaleString()} saved` : "Stored locally"}</span>
      </footer>

      <ToastStack toasts={toasts} dismiss={dismissToast}/>
      <ConfirmClearDialog open={clearDialogOpen} busy={clearing} onCancel={() => setClearDialogOpen(false)} onConfirm={() => void clearHistory()}/>
    </div>
  );
}
