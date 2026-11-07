export type ClipboardKind = "text" | "html" | "image" | "files";

export type EntryId = string | number;

/**
 * Wire shape returned by the Rust backend. A handful of optional aliases are
 * accepted so the UI can continue to read databases created by early builds.
 */
export interface ClipboardEntryWire {
  id: EntryId;
  kind?: ClipboardKind | string;
  contentType?: ClipboardKind | string;
  entryType?: ClipboardKind | string;
  text?: string | null;
  plainText?: string | null;
  html?: string | null;
  content?: string | null;
  preview?: string | null;
  thumbnail?: string | null;
  thumbnailDataUrl?: string | null;
  imageData?: string | null;
  imageDataUrl?: string | null;
  imagePath?: string | null;
  filePaths?: string[] | null;
  files?: string[] | null;
  createdAt?: string | number;
  timestamp?: string | number;
  copiedAt?: string | number;
  isPinned?: boolean;
  pinned?: boolean;
  sourceApp?: string | null;
  sourceApplication?: string | null;
  sourceProcess?: string | null;
  sizeBytes?: number | null;
  byteSize?: number | null;
  width?: number | null;
  height?: number | null;
  copyCount?: number | null;
}

export interface ClipboardEntry {
  id: EntryId;
  kind: ClipboardKind;
  text: string;
  html: string;
  imageSource: string;
  imagePath: string;
  filePaths: string[];
  createdAt: string | number;
  pinned: boolean;
  sourceApp: string;
  sizeBytes?: number;
  width?: number;
  height?: number;
  copyCount?: number;
  raw: ClipboardEntryWire;
}

export type ThemePreference = "system" | "light" | "dark";

export interface AppSettingsWire {
  launchAtStartup?: boolean;
  startupEnabled?: boolean;
  openShortcut?: string;
  windowShortcut?: string;
  slotShortcuts?: string[];
  pasteShortcuts?: string[];
  excludedApplications?: string[];
  excludedApps?: string[];
  theme?: ThemePreference;
  [key: string]: unknown;
}

export interface AppSettings {
  launchAtStartup: boolean;
  openShortcut: string;
  slotShortcuts: string[];
  excludedApplications: string[];
  theme: ThemePreference;
  raw: AppSettingsWire;
}

export interface AppStatusWire {
  recordingPaused?: boolean;
  paused?: boolean;
  databaseReady?: boolean;
  dbReady?: boolean;
  hookActive?: boolean;
  keyboardHookActive?: boolean;
  clipboardWatcherActive?: boolean;
  watcherActive?: boolean;
  entryCount?: number;
  totalEntries?: number;
  version?: string;
  lastError?: string | null;
  textHistoryPath?: string;
  [key: string]: unknown;
}

export interface AppStatus {
  recordingPaused: boolean;
  databaseReady: boolean;
  hookActive: boolean;
  clipboardWatcherActive: boolean;
  entryCount?: number;
  version?: string;
  lastError?: string;
  textHistoryPath?: string;
}

export type EntryAction = "paste" | "copy" | "pin" | "delete";

export type ToastTone = "success" | "error" | "info";

export interface ToastMessage {
  id: number;
  tone: ToastTone;
  title: string;
  detail?: string;
}

export interface HistoryPageWire {
  items?: ClipboardEntryWire[];
  entries?: ClipboardEntryWire[];
  total?: number;
  hasMore?: boolean;
}
