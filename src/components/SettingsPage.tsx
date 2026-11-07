import { useEffect, useMemo, useRef, useState, type KeyboardEvent } from "react";
import type { AppSettings, AppStatus, ThemePreference } from "../types";
import { DEFAULT_SLOT_SHORTCUTS } from "../lib/format";
import { Icon } from "./Icon";

const DEFAULT_SLOT_LABELS = ["1", "2", "3", "4", "5", "6", "7", "8", "9", "0"];

interface ShortcutFieldProps {
  value: string;
  label: string;
  compact?: boolean;
  onChange: (value: string) => void;
}

const MODIFIER_KEYS = new Set(["Control", "Shift", "Alt", "Meta"]);

function canonicalKey(key: string): string {
  if (key === " ") return "Space";
  if (key === "Escape") return "Esc";
  if (key === "ArrowUp") return "Up";
  if (key === "ArrowDown") return "Down";
  if (key === "ArrowLeft") return "Left";
  if (key === "ArrowRight") return "Right";
  if (key.length === 1) return key.toUpperCase();
  return key.replace(/^Key/, "").replace(/^Digit/, "");
}

function modifiers(event: KeyboardEvent): string[] {
  return [event.ctrlKey && "Ctrl", event.altKey && "Alt", event.shiftKey && "Shift", event.metaKey && "Win"].filter(Boolean) as string[];
}

function ShortcutField({ value, label, compact, onChange }: ShortcutFieldProps) {
  const [recording, setRecording] = useState(false);
  const [captured, setCaptured] = useState<string[]>([]);
  const [preview, setPreview] = useState(value);
  const original = useRef(value);

  useEffect(() => { if (!recording) setPreview(value); }, [recording, value]);

  const finish = (commit: boolean) => {
    if (commit && preview && !/^(Ctrl|Alt|Shift|Win)(\+(Ctrl|Alt|Shift|Win))*$/.test(preview)) onChange(preview);
    if (!commit) setPreview(original.current);
    setCaptured([]);
    setRecording(false);
  };

  const begin = () => {
    original.current = value;
    setCaptured([]);
    setPreview("");
    setRecording(true);
  };

  const onKeyDown = (event: KeyboardEvent<HTMLButtonElement>) => {
    if (!recording) return;
    event.preventDefault();
    event.stopPropagation();
    if (event.key === "Escape") { finish(false); return; }
    if (event.key === "Backspace" || event.key === "Delete") {
      onChange("");
      setPreview("");
      setCaptured([]);
      setRecording(false);
      return;
    }
    let nextCaptured = captured;
    if (!MODIFIER_KEYS.has(event.key) && !event.repeat) {
      const key = canonicalKey(event.key);
      nextCaptured = [...captured, key].slice(-3);
      setCaptured(nextCaptured);
    }
    setPreview([...modifiers(event), ...nextCaptured].join("+"));
  };

  const onKeyUp = (event: KeyboardEvent<HTMLButtonElement>) => {
    if (!recording) return;
    event.preventDefault();
    if (MODIFIER_KEYS.has(event.key) && captured.length > 0) finish(true);
  };

  return (
    <div className={`shortcut-field ${compact ? "shortcut-field--compact" : ""}`}>
      {!compact && <span className="shortcut-field__label">{label}</span>}
      <button
        type="button"
        className={`shortcut-recorder ${recording ? "is-recording" : ""}`}
        aria-label={`${label}: ${value || "not set"}. Click to record a new shortcut.`}
        onClick={() => recording ? finish(captured.length > 0) : begin()}
        onBlur={() => recording && finish(captured.length > 0)}
        onKeyDown={onKeyDown}
        onKeyUp={onKeyUp}
      >
        {recording ? <span className="recording-dot"/> : <Icon name="keyboard" size={14}/>} 
        <kbd>{recording ? preview || "Press keys…" : value || "Not set"}</kbd>
      </button>
      {recording && <span className="shortcut-field__hint">Hold modifiers, then press keys. Esc cancels.</span>}
    </div>
  );
}

interface ToggleProps {
  checked: boolean;
  onChange: (checked: boolean) => void;
  label: string;
  disabled?: boolean;
}

function Toggle({ checked, onChange, label, disabled }: ToggleProps) {
  return (
    <button
      type="button"
      role="switch"
      aria-label={label}
      aria-checked={checked}
      disabled={disabled}
      className={`toggle ${checked ? "is-on" : ""}`}
      onClick={() => onChange(!checked)}
    ><span/></button>
  );
}

interface SettingsPageProps {
  settings: AppSettings | null;
  status: AppStatus | null;
  loading: boolean;
  saving: boolean;
  onSave: (settings: AppSettings) => Promise<void>;
  onPausedChange: (paused: boolean) => Promise<void>;
  onClearHistory: () => void;
  onOpenTextHistory: () => Promise<void>;
  onClearTextHistory: () => Promise<void>;
  onConvertTextHistoryToPdf: () => Promise<void>;
}

export function SettingsPage({ settings, status, loading, saving, onSave, onPausedChange, onClearHistory, onOpenTextHistory, onClearTextHistory, onConvertTextHistoryToPdf }: SettingsPageProps) {
  const [draft, setDraft] = useState<AppSettings | null>(settings);
  const [excludedInput, setExcludedInput] = useState("");
  const [pauseBusy, setPauseBusy] = useState(false);

  useEffect(() => setDraft(settings), [settings]);

  const duplicateShortcuts = useMemo(() => {
    if (!draft) return new Set<string>();
    const values = [draft.openShortcut, ...draft.slotShortcuts].map((value) => value.trim().toLowerCase()).filter(Boolean);
    return new Set(values.filter((value, index) => values.indexOf(value) !== index));
  }, [draft]);

  const normalPasteConflict = useMemo(() => {
    if (!draft) return false;
    return [draft.openShortcut, ...draft.slotShortcuts].some((value) => value.trim().toLowerCase() === "ctrl+v");
  }, [draft]);

  const dirty = Boolean(draft && settings && JSON.stringify({
    launchAtStartup: draft.launchAtStartup,
    openShortcut: draft.openShortcut,
    slotShortcuts: draft.slotShortcuts,
    excludedApplications: draft.excludedApplications,
    theme: draft.theme,
  }) !== JSON.stringify({
    launchAtStartup: settings.launchAtStartup,
    openShortcut: settings.openShortcut,
    slotShortcuts: settings.slotShortcuts,
    excludedApplications: settings.excludedApplications,
    theme: settings.theme,
  }));

  if (loading || !draft) {
    return (
      <main className="settings-page">
        <div className="settings-heading"><div><p className="eyebrow">Preferences</p><h1>Settings</h1></div></div>
        <div className="settings-loading"><span className="spinner"/><span>Loading settings…</span></div>
      </main>
    );
  }

  const update = <K extends keyof AppSettings>(key: K, value: AppSettings[K]) => {
    setDraft((current) => current ? { ...current, [key]: value } : current);
  };

  const addExcludedApplication = () => {
    const value = excludedInput.trim().replace(/^"|"$/g, "");
    if (!value || draft.excludedApplications.some((app) => app.toLowerCase() === value.toLowerCase())) return;
    update("excludedApplications", [...draft.excludedApplications, value]);
    setExcludedInput("");
  };

  const setPaused = async (paused: boolean) => {
    setPauseBusy(true);
    try { await onPausedChange(paused); } finally { setPauseBusy(false); }
  };

  return (
    <main className="settings-page">
      <div className="settings-heading">
        <div><p className="eyebrow">Preferences</p><h1>Settings</h1><p>Control shortcuts, privacy, and how CopyAPP runs.</p></div>
        <div className="settings-save-actions">
          {dirty && <span className="unsaved-indicator">Unsaved changes</span>}
          <button className="secondary-button" type="button" disabled={!dirty || saving} onClick={() => setDraft(settings)}>Discard</button>
          <button className="primary-button" type="button" disabled={!dirty || saving || duplicateShortcuts.size > 0 || normalPasteConflict || !draft.openShortcut.trim() || draft.slotShortcuts.some((item) => !item.trim())} onClick={() => void onSave(draft)}>
            {saving ? <span className="spinner spinner--light"/> : <Icon name="check" size={16}/>} {saving ? "Saving…" : "Save changes"}
          </button>
        </div>
      </div>

      <div className="settings-layout">
        <div className="settings-main">
          <section className="settings-card">
            <div className="settings-card__heading"><span className="settings-icon"><Icon name="keyboard"/></span><div><h2>Keyboard shortcuts</h2><p>Click a shortcut, then press the new key combination.</p></div></div>
            <div className="setting-row">
              <div><strong>Open or close CopyAPP</strong><p>Show the clipboard window from anywhere.</p></div>
              <div className="setting-control">
                <ShortcutField label="Open or close CopyAPP" value={draft.openShortcut} onChange={(value) => update("openShortcut", value)}/>
                {draft.openShortcut.trim().toLowerCase() === "ctrl+v" && <small className="shortcut-error">Reserved for normal paste</small>}
              </div>
            </div>
            <div className="slot-shortcut-heading">
              <div><strong>Quick-paste slots</strong><p>Ten slots are always available. Add another only by assigning it a shortcut.</p></div>
              <div className="slot-shortcut-actions">
                <button type="button" className="text-button" disabled={draft.slotShortcuts.length >= 50} onClick={() => update("slotShortcuts", [...draft.slotShortcuts, ""])}>Add shortcut</button>
                <button type="button" className="text-button" onClick={() => update("slotShortcuts", [...DEFAULT_SLOT_SHORTCUTS])}>Reset to 10 defaults</button>
              </div>
            </div>
            <div className="shortcut-grid">
              {draft.slotShortcuts.map((shortcut, index) => {
                const label = DEFAULT_SLOT_LABELS[index] ?? String(index + 1);
                const reserved = shortcut.trim().toLowerCase() === "ctrl+v";
                const duplicate = duplicateShortcuts.has(shortcut.trim().toLowerCase());
                return (
                  <div className={`slot-shortcut ${duplicate || reserved ? "has-error" : ""}`} key={index}>
                    <span className="slot-shortcut__number">{label}</span>
                    <div><span>Slot {index + 1}</span><ShortcutField compact label={`Paste slot ${index + 1}`} value={shortcut} onChange={(value) => { const next = [...draft.slotShortcuts]; next[index] = value; update("slotShortcuts", next); }}/>{(duplicate || reserved) && <small>{reserved ? "Reserved for normal paste" : "Already in use"}</small>}</div>
                    {index >= 10 && <button type="button" className="remove-slot-shortcut" aria-label={`Remove slot ${index + 1}`} onClick={() => update("slotShortcuts", draft.slotShortcuts.filter((_, slotIndex) => slotIndex !== index))}><Icon name="close" size={13}/></button>}
                  </div>
                );
              })}
            </div>
            <div className="settings-note"><Icon name="shield" size={16}/><span><strong>Normal Ctrl+V is preserved.</strong> Release Ctrl to paste normally, or keep it held and press a configured slot key for quick paste.</span></div>
          </section>

          <section className="settings-card">
            <div className="settings-card__heading"><span className="settings-icon"><Icon name="file"/></span><div><h2>Plain-text history</h2><p>An always-current, readable copy of your complete clipboard history.</p></div></div>
            <div className="setting-row setting-row--text-history">
              <div><strong>CopyAPP History.txt</strong><p>Oldest to newest, with a blank line between copies. No dates, applications, or other metadata.</p></div>
              <div className="text-history-actions">
                <button className="secondary-button" type="button" onClick={() => void onOpenTextHistory()}><Icon name="file" size={14}/>Open text file</button>
                <button className="secondary-button text-history-clear-button" type="button" onClick={() => void onClearTextHistory()}><Icon name="trash" size={14}/>Clear text file</button>
                <button className="secondary-button" type="button" onClick={() => void onConvertTextHistoryToPdf()}><Icon name="file" size={14}/>Convert to PDF</button>
              </div>
            </div>
            {status?.textHistoryPath && <code className="text-history-path" title={status.textHistoryPath}>{status.textHistoryPath}</code>}
          </section>

          <section className="settings-card">
            <div className="settings-card__heading"><span className="settings-icon"><Icon name="shield"/></span><div><h2>Privacy & recording</h2><p>Clipboard contents stay on this Windows device.</p></div></div>
            <div className="setting-row">
              <div><strong>Record clipboard history</strong><p>Pause to use the clipboard without adding new entries.</p></div>
              <div className="toggle-with-status"><span className={status?.recordingPaused ? "status-text status-text--paused" : "status-text"}>{status?.recordingPaused ? "Paused" : "Recording"}</span><Toggle checked={!status?.recordingPaused} disabled={pauseBusy} onChange={(enabled) => void setPaused(!enabled)} label="Record clipboard history"/></div>
            </div>
            <div className="excluded-apps">
              <div><strong>Excluded applications</strong><p>CopyAPP ignores clipboard changes made while these executables are active.</p></div>
              <div className="excluded-input-row">
                <input value={excludedInput} onChange={(event) => setExcludedInput(event.target.value)} onKeyDown={(event) => { if (event.key === "Enter") { event.preventDefault(); addExcludedApplication(); } }} placeholder="Application.exe" aria-label="Application to exclude" spellCheck={false}/>
                <button className="secondary-button" type="button" disabled={!excludedInput.trim()} onClick={addExcludedApplication}>Add</button>
              </div>
              <div className="app-chips">
                {draft.excludedApplications.map((app) => (
                  <span className="app-chip" key={app}><span className="app-chip__icon">{app.charAt(0).toUpperCase()}</span><span>{app}</span><button type="button" onClick={() => update("excludedApplications", draft.excludedApplications.filter((item) => item !== app))} aria-label={`Remove ${app}`}><Icon name="close" size={13}/></button></span>
                ))}
                {!draft.excludedApplications.length && <span className="empty-inline">No applications excluded</span>}
              </div>
            </div>
          </section>

          <section className="settings-card">
            <div className="settings-card__heading"><span className="settings-icon"><Icon name="settings"/></span><div><h2>Windows & appearance</h2><p>Choose how the app starts and looks.</p></div></div>
            <div className="setting-row">
              <div><strong>Launch at Windows startup</strong><p>Start minimized in the system tray after you sign in.</p></div>
              <Toggle checked={draft.launchAtStartup} onChange={(value) => update("launchAtStartup", value)} label="Launch at Windows startup"/>
            </div>
            <div className="setting-row setting-row--theme">
              <div><strong>Theme</strong><p>Follow Windows or choose a fixed appearance.</p></div>
              <div className="theme-picker" role="radiogroup" aria-label="Theme">
                {(["system", "light", "dark"] as ThemePreference[]).map((theme) => (
                  <button type="button" role="radio" aria-checked={draft.theme === theme} className={draft.theme === theme ? "is-active" : ""} key={theme} onClick={() => update("theme", theme)}>
                    <Icon name={theme === "dark" ? "moon" : theme === "light" ? "sun" : "settings"} size={14}/>{theme.charAt(0).toUpperCase() + theme.slice(1)}
                  </button>
                ))}
              </div>
            </div>
          </section>

          <section className="settings-card danger-zone">
            <div><strong>Clear clipboard history</strong><p>Pinned items are preserved. This can’t be undone.</p></div>
            <button className="danger-button" type="button" onClick={onClearHistory}><Icon name="trash" size={15}/>Clear history</button>
          </section>
        </div>

        <aside className="settings-sidebar">
          <div className="privacy-card"><Icon name="shield" size={24}/><h3>Local by design</h3><p>Your clipboard database and image files never leave this computer. CopyAPP has no account, analytics, or cloud sync.</p></div>
          <div className="health-card">
            <h3>Service status</h3>
            <span><i className={status?.databaseReady ? "health-dot" : "health-dot is-error"}/>Local database<b>{status?.databaseReady ? "Ready" : "Error"}</b></span>
            <span><i className={status?.clipboardWatcherActive ? "health-dot" : "health-dot is-error"}/>Clipboard watcher<b>{status?.clipboardWatcherActive ? "Active" : "Stopped"}</b></span>
            <span><i className={status?.hookActive ? "health-dot" : "health-dot is-error"}/>Keyboard hook<b>{status?.hookActive ? "Active" : "Stopped"}</b></span>
            {status?.version && <small>CopyAPP {status.version}</small>}
          </div>
        </aside>
      </div>
    </main>
  );
}
