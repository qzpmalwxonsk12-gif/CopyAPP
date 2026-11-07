import { convertFileSrc, invoke } from "@tauri-apps/api/core";
import { listen, type UnlistenFn } from "@tauri-apps/api/event";
import type {
  AppSettings,
  AppSettingsWire,
  AppStatus,
  AppStatusWire,
  ClipboardEntry,
  ClipboardEntryWire,
  HistoryPageWire,
} from "../types";
import { errorMessage, normalizeEntry, normalizeSettings, normalizeStatus, settingsToWire } from "./format";

export const isTauri =
  typeof window !== "undefined" &&
  ("__TAURI_INTERNALS__" in window || "__TAURI__" in window);

async function command<T>(name: string, args?: Record<string, unknown>): Promise<T> {
  if (!isTauri) {
    throw new Error("The desktop service is unavailable. Open the app through Tauri.");
  }
  return invoke<T>(name, args);
}

function unpackEntries(payload: ClipboardEntryWire[] | HistoryPageWire): {
  entries: ClipboardEntry[];
  total?: number;
  hasMore?: boolean;
} {
  if (Array.isArray(payload)) return { entries: payload.map(normalizeEntry) };
  const rows = payload.items ?? payload.entries ?? [];
  return { entries: rows.map(normalizeEntry), total: payload.total, hasMore: payload.hasMore };
}

export const clipboardApi = {
  async getRecentSlots(): Promise<ClipboardEntry[]> {
    const result = await command<ClipboardEntryWire[]>("get_recent_slots");
    return result.map(normalizeEntry);
  },

  async getHistory(query: string, limit: number, offset: number) {
    const result = await command<ClipboardEntryWire[] | HistoryPageWire>("get_history", {
      query: query || null,
      limit,
      offset,
    });
    return unpackEntries(result);
  },

  async getEntry(id: ClipboardEntry["id"]): Promise<ClipboardEntry> {
    return normalizeEntry(await command<ClipboardEntryWire>("get_entry", { id }));
  },

  pasteEntry(id: ClipboardEntry["id"]): Promise<void> {
    return command("paste_entry", { id });
  },

  copyEntry(id: ClipboardEntry["id"]): Promise<void> {
    return command("copy_entry", { id });
  },

  togglePin(id: ClipboardEntry["id"]): Promise<boolean | ClipboardEntryWire | void> {
    return command("toggle_pin", { id });
  },

  deleteEntry(id: ClipboardEntry["id"]): Promise<void> {
    return command("delete_entry", { id });
  },

  clearHistory(): Promise<void> {
    return command("clear_history");
  },

  async getSettings(): Promise<AppSettings> {
    return normalizeSettings(await command<AppSettingsWire>("get_settings"));
  },

  async updateSettings(settings: AppSettings): Promise<AppSettings> {
    const result = await command<AppSettingsWire | void>("update_settings", {
      settings: settingsToWire(settings),
    });
    return result ? normalizeSettings(result) : settings;
  },

  setRecordingPaused(paused: boolean): Promise<void> {
    return command("set_recording_paused", { paused });
  },

  toggleWindow(): Promise<void> {
    return command("toggle_window");
  },

  openTextHistory(): Promise<string> {
    return command("open_text_history");
  },

  clearTextHistory(): Promise<void> {
    return command("clear_text_history");
  },

  convertTextHistoryToPdf(): Promise<string> {
    return command("convert_text_history_to_pdf");
  },

  async getStatus(): Promise<AppStatus> {
    return normalizeStatus(await command<AppStatusWire>("get_status"));
  },
};

export function localImageUrl(entry: ClipboardEntry): string {
  if (entry.imageSource) return entry.imageSource;
  if (!entry.imagePath) return "";
  try {
    return isTauri ? convertFileSrc(entry.imagePath) : entry.imagePath;
  } catch {
    return "";
  }
}

export async function listenForBackendChanges(
  onChange: () => void,
  onError?: (message: string) => void,
): Promise<UnlistenFn> {
  if (!isTauri) return () => undefined;
  const unlisteners = await Promise.all([
    listen("clipboard://changed", onChange),
    listen("settings://changed", onChange),
    listen("recording://changed", onChange),
    listen<unknown>("clipboard://error", (event) => onError?.(errorMessage(event.payload))),
  ]);
  return () => unlisteners.forEach((unlisten) => unlisten());
}
