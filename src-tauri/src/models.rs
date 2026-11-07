use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ShortcutSettings {
    pub open_close: String,
    pub slots: Vec<String>,
}

impl Default for ShortcutSettings {
    fn default() -> Self {
        Self {
            open_close: "Ctrl+Shift+Space".into(),
            slots: (1..=9)
                .map(|n| format!("Ctrl+V+{n}"))
                .chain(std::iter::once("Ctrl+V+0".into()))
                .collect(),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default, rename_all = "camelCase")]
pub struct Settings {
    pub recording_paused: bool,
    pub launch_at_startup: bool,
    pub minimize_to_tray: bool,
    pub close_to_tray: bool,
    pub deduplicate_consecutive: bool,
    pub excluded_applications: Vec<String>,
    pub open_shortcut: String,
    pub slot_shortcuts: Vec<String>,
    pub theme: String,
}

impl Default for Settings {
    fn default() -> Self {
        Self {
            recording_paused: false,
            launch_at_startup: false,
            minimize_to_tray: true,
            close_to_tray: true,
            deduplicate_consecutive: true,
            excluded_applications: Vec::new(),
            open_shortcut: ShortcutSettings::default().open_close,
            slot_shortcuts: ShortcutSettings::default().slots,
            theme: "system".into(),
        }
    }
}

impl Settings {
    pub fn shortcuts(&self) -> ShortcutSettings {
        ShortcutSettings {
            open_close: self.open_shortcut.clone(),
            slots: self.slot_shortcuts.clone(),
        }
    }
}

#[derive(Debug, Clone)]
pub struct CapturedClipboard {
    pub kind: String,
    pub text: Option<String>,
    pub html: Option<String>,
    pub image_png: Option<Vec<u8>>,
    pub thumbnail_png: Option<Vec<u8>>,
    pub file_paths: Vec<String>,
    pub hash: String,
    pub byte_size: i64,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct EntrySummary {
    pub id: i64,
    pub kind: String,
    pub preview: String,
    pub image_data_url: Option<String>,
    pub file_paths: Vec<String>,
    pub source_application: Option<String>,
    pub created_at: i64,
    pub pinned: bool,
    pub byte_size: i64,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct EntryDetail {
    #[serde(flatten)]
    pub summary: EntrySummary,
    pub text: Option<String>,
    pub html: Option<String>,
}

#[derive(Debug, Clone)]
pub struct StoredEntry {
    pub text: Option<String>,
    pub html: Option<String>,
    pub image_png: Option<Vec<u8>>,
    pub file_paths: Vec<String>,
    pub hash: String,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AppStatus {
    pub recording_paused: bool,
    pub hook_active: bool,
    pub watcher_active: bool,
    pub database_ready: bool,
    pub entry_count: i64,
    pub database_path: String,
    pub text_history_path: String,
    pub current_source_application: Option<String>,
    pub last_error: Option<String>,
    pub platform: &'static str,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ClipboardChangedEvent {
    pub reason: &'static str,
    pub entry: Option<EntrySummary>,
}
