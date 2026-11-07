import type {
  AppSettings,
  AppSettingsWire,
  AppStatus,
  AppStatusWire,
  ClipboardEntry,
  ClipboardEntryWire,
  ClipboardKind,
} from "../types";

const SLOT_LABELS = ["1", "2", "3", "4", "5", "6", "7", "8", "9", "0"];

export const DEFAULT_SLOT_SHORTCUTS = SLOT_LABELS.map((label) => `Ctrl+V+${label}`);

export function normalizeKind(entry: ClipboardEntryWire): ClipboardKind {
  const raw = String(entry.kind ?? entry.contentType ?? entry.entryType ?? "").toLowerCase();
  if (raw.includes("image") || raw === "png" || raw === "bitmap") return "image";
  if (raw.includes("file") || raw === "paths") return "files";
  if (raw.includes("html") || raw.includes("rich")) return "html";
  if ((entry.filePaths?.length ?? entry.files?.length ?? 0) > 0) return "files";
  if (entry.imageData || entry.imageDataUrl || entry.imagePath || entry.thumbnailDataUrl) return "image";
  if (entry.html) return "html";
  return "text";
}

function asImageDataUrl(value: string): string {
  if (!value) return "";
  if (/^(data:|asset:|https?:|blob:)/i.test(value)) return value;
  const compact = value.replace(/\s+/g, "");
  if (/^[a-zA-Z0-9+/]+=*$/.test(compact) && compact.length > 64) {
    return `data:image/png;base64,${compact}`;
  }
  return value;
}

export function normalizeEntry(entry: ClipboardEntryWire): ClipboardEntry {
  const kind = normalizeKind(entry);
  const text = entry.text ?? entry.plainText ?? entry.preview ?? entry.content ?? "";
  const html = entry.html ?? (kind === "html" ? entry.content ?? "" : "");
  const imageSource = asImageDataUrl(
    entry.thumbnailDataUrl ?? entry.imageDataUrl ?? entry.imageData ?? entry.thumbnail ?? "",
  );
  return {
    id: entry.id,
    kind,
    text,
    html,
    imageSource,
    imagePath: entry.imagePath ?? "",
    filePaths: entry.filePaths ?? entry.files ?? [],
    createdAt: entry.createdAt ?? entry.copiedAt ?? entry.timestamp ?? Date.now(),
    pinned: Boolean(entry.isPinned ?? entry.pinned),
    sourceApp: entry.sourceApp ?? entry.sourceApplication ?? entry.sourceProcess ?? "",
    sizeBytes: entry.sizeBytes ?? entry.byteSize ?? undefined,
    width: entry.width ?? undefined,
    height: entry.height ?? undefined,
    copyCount: entry.copyCount ?? undefined,
    raw: entry,
  };
}

export function normalizeSettings(settings: AppSettingsWire | null | undefined): AppSettings {
  const raw = settings ?? {};
  const shortcuts = raw.slotShortcuts ?? raw.pasteShortcuts ?? DEFAULT_SLOT_SHORTCUTS;
  const slotShortcuts = Array.from({ length: Math.max(10, Math.min(50, shortcuts.length)) }, (_, index) =>
    shortcuts[index]?.trim() || DEFAULT_SLOT_SHORTCUTS[index] || "",
  );
  return {
    launchAtStartup: Boolean(raw.launchAtStartup ?? raw.startupEnabled),
    openShortcut: raw.openShortcut ?? raw.windowShortcut ?? "Ctrl+Shift+V",
    slotShortcuts,
    excludedApplications: raw.excludedApplications ?? raw.excludedApps ?? [],
    theme: raw.theme === "light" || raw.theme === "dark" ? raw.theme : "system",
    raw,
  };
}

export function settingsToWire(settings: AppSettings): AppSettingsWire {
  return {
    ...settings.raw,
    launchAtStartup: settings.launchAtStartup,
    openShortcut: settings.openShortcut,
    slotShortcuts: settings.slotShortcuts,
    excludedApplications: settings.excludedApplications,
    theme: settings.theme,
  };
}

export function normalizeStatus(status: AppStatusWire | null | undefined): AppStatus {
  const raw = status ?? {};
  return {
    recordingPaused: Boolean(raw.recordingPaused ?? raw.paused),
    databaseReady: raw.databaseReady ?? raw.dbReady ?? false,
    hookActive: raw.hookActive ?? raw.keyboardHookActive ?? false,
    clipboardWatcherActive: raw.clipboardWatcherActive ?? raw.watcherActive ?? false,
    entryCount: raw.entryCount ?? raw.totalEntries,
    version: raw.version,
    lastError: raw.lastError || undefined,
    textHistoryPath: raw.textHistoryPath,
  };
}

export function entryTitle(entry: ClipboardEntry): string {
  if (entry.kind === "files") {
    if (!entry.filePaths.length) return "File selection";
    const first = fileName(entry.filePaths[0]);
    return entry.filePaths.length > 1 ? `${first} +${entry.filePaths.length - 1}` : first;
  }
  if (entry.kind === "image") {
    return entry.width && entry.height ? `Image · ${entry.width} × ${entry.height}` : "Image";
  }
  const value = plainPreview(entry).replace(/\s+/g, " ").trim();
  return value || (entry.kind === "html" ? "Rich text" : "Empty text");
}

export function plainPreview(entry: ClipboardEntry): string {
  if (entry.text) return entry.text;
  if (!entry.html) return "";
  if (typeof DOMParser === "undefined") return entry.html.replace(/<[^>]+>/g, " ");
  return new DOMParser().parseFromString(entry.html, "text/html").body.textContent ?? "";
}

export function fileName(path: string): string {
  const pieces = path.replace(/\\/g, "/").split("/");
  return pieces.at(-1) || path;
}

export function fileExtension(path: string): string {
  const name = fileName(path);
  const dot = name.lastIndexOf(".");
  return dot > 0 ? name.slice(dot + 1).toUpperCase().slice(0, 5) : "FILE";
}

export function formatRelativeTime(value: string | number): string {
  const date = new Date(value);
  if (Number.isNaN(date.getTime())) return "Unknown time";
  const elapsed = Date.now() - date.getTime();
  const future = elapsed < 0;
  const seconds = Math.abs(elapsed) / 1000;
  if (seconds < 45) return future ? "In a moment" : "Just now";
  const formatter = new Intl.RelativeTimeFormat(undefined, { numeric: "auto" });
  if (seconds < 3600) return formatter.format((future ? 1 : -1) * Math.round(seconds / 60), "minute");
  if (seconds < 86400) return formatter.format((future ? 1 : -1) * Math.round(seconds / 3600), "hour");
  if (seconds < 604800) return formatter.format((future ? 1 : -1) * Math.round(seconds / 86400), "day");
  return new Intl.DateTimeFormat(undefined, {
    month: "short",
    day: "numeric",
    ...(date.getFullYear() !== new Date().getFullYear() ? { year: "numeric" } : {}),
  }).format(date);
}

export function formatDateTime(value: string | number): string {
  const date = new Date(value);
  if (Number.isNaN(date.getTime())) return "Unknown time";
  return new Intl.DateTimeFormat(undefined, { dateStyle: "medium", timeStyle: "short" }).format(date);
}

export function formatBytes(bytes?: number): string {
  if (bytes == null || !Number.isFinite(bytes)) return "";
  if (bytes < 1024) return `${bytes} B`;
  const units = ["KB", "MB", "GB"];
  let value = bytes / 1024;
  let unitIndex = 0;
  while (value >= 1024 && unitIndex < units.length - 1) {
    value /= 1024;
    unitIndex += 1;
  }
  return `${value >= 10 ? value.toFixed(0) : value.toFixed(1)} ${units[unitIndex]}`;
}

export function kindLabel(kind: ClipboardKind): string {
  return kind === "files" ? "Files" : kind.charAt(0).toUpperCase() + kind.slice(1);
}

export function sameId(a: unknown, b: unknown): boolean {
  return String(a) === String(b);
}

export function errorMessage(error: unknown): string {
  if (error instanceof Error) return error.message;
  if (typeof error === "string") return error;
  if (error && typeof error === "object" && "message" in error) return String(error.message);
  return "Something went wrong";
}
