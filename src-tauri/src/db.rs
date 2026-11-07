use std::{
    path::{Path, PathBuf},
    sync::Mutex,
};

use base64::{engine::general_purpose::STANDARD as BASE64, Engine};
use rusqlite::{params, Connection, OptionalExtension, Row};

use crate::models::{CapturedClipboard, EntryDetail, EntrySummary, Settings, StoredEntry};

pub struct Database {
    connection: Mutex<Connection>,
    path: PathBuf,
}

impl Database {
    pub fn open(path: &Path) -> Result<Self, String> {
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent).map_err(|e| format!("create data directory: {e}"))?;
        }
        let connection = Connection::open(path).map_err(|e| format!("open database: {e}"))?;
        connection
            .execute_batch(
                "PRAGMA journal_mode=WAL;
                 PRAGMA synchronous=NORMAL;
                 PRAGMA foreign_keys=ON;
                 PRAGMA busy_timeout=3000;
                 CREATE TABLE IF NOT EXISTS clipboard_entries (
                   id INTEGER PRIMARY KEY AUTOINCREMENT,
                   kind TEXT NOT NULL,
                   preview TEXT NOT NULL,
                   text_content TEXT,
                   html_content TEXT,
                   image_png BLOB,
                   thumbnail_png BLOB,
                   file_paths_json TEXT NOT NULL DEFAULT '[]',
                   content_hash TEXT NOT NULL,
                   source_application TEXT,
                   created_at INTEGER NOT NULL,
                   pinned INTEGER NOT NULL DEFAULT 0,
                   byte_size INTEGER NOT NULL DEFAULT 0
                 );
                 CREATE INDEX IF NOT EXISTS idx_entries_created
                   ON clipboard_entries(created_at DESC, id DESC);
                 CREATE INDEX IF NOT EXISTS idx_entries_hash
                   ON clipboard_entries(content_hash);
                 CREATE TABLE IF NOT EXISTS app_settings (
                   key TEXT PRIMARY KEY,
                   value TEXT NOT NULL
                 );",
            )
            .map_err(|e| format!("initialize database: {e}"))?;
        Ok(Self {
            connection: Mutex::new(connection),
            path: path.to_path_buf(),
        })
    }

    fn conn(&self) -> Result<std::sync::MutexGuard<'_, Connection>, String> {
        self.connection
            .lock()
            .map_err(|_| "database lock was poisoned".to_string())
    }

    pub fn path_display(&self) -> String {
        self.path.to_string_lossy().into_owned()
    }

    pub fn text_history_path(&self) -> PathBuf {
        self.path.with_file_name("CopyAPP History.txt")
    }

    pub fn text_history_path_display(&self) -> String {
        self.text_history_path().to_string_lossy().into_owned()
    }

    pub fn text_history_contents(&self) -> Result<String, String> {
        let conn = self.conn()?;
        let mut statement = conn
            .prepare(
                "SELECT kind,preview,text_content,file_paths_json
                 FROM clipboard_entries
                 WHERE id > COALESCE((
                   SELECT CAST(value AS INTEGER) FROM app_settings
                   WHERE key='text_history_cutoff_id'
                 ), 0)
                 ORDER BY created_at ASC,id ASC",
            )
            .map_err(|e| format!("prepare text history: {e}"))?;
        let rows = statement
            .query_map([], |row| {
                let kind: String = row.get(0)?;
                let preview: String = row.get(1)?;
                let text: Option<String> = row.get(2)?;
                let paths_json: String = row.get(3)?;
                let content = match kind.as_str() {
                    "files" => serde_json::from_str::<Vec<String>>(&paths_json)
                        .unwrap_or_default()
                        .join("\r\n"),
                    "image" => "[Image]".to_string(),
                    _ => text.unwrap_or(preview),
                };
                Ok(content)
            })
            .map_err(|e| format!("query text history: {e}"))?;
        let entries = rows
            .collect::<Result<Vec<_>, _>>()
            .map_err(|e| format!("read text history: {e}"))?;
        let mut output = entries.join("\r\n\r\n");
        if !output.is_empty() {
            output.push_str("\r\n");
        }
        Ok(output)
    }

    pub fn refresh_text_history_file(&self) -> Result<(), String> {
        let path = self.text_history_path();
        let contents = self.text_history_contents()?;
        std::fs::write(&path, contents)
            .map_err(|e| format!("write text history {}: {e}", path.display()))
    }

    pub fn clear_text_history(&self) -> Result<(), String> {
        self.conn()?
            .execute(
                "INSERT INTO app_settings(key,value)
                 VALUES('text_history_cutoff_id',(
                   SELECT CAST(COALESCE(MAX(id),0) AS TEXT) FROM clipboard_entries
                 ))
                 ON CONFLICT(key) DO UPDATE SET value=excluded.value",
                [],
            )
            .map(|_| ())
            .map_err(|e| format!("clear text history: {e}"))
    }

    pub fn load_settings(&self) -> Result<Settings, String> {
        let conn = self.conn()?;
        let raw: Option<String> = conn
            .query_row(
                "SELECT value FROM app_settings WHERE key='settings'",
                [],
                |r| r.get(0),
            )
            .optional()
            .map_err(|e| format!("read settings: {e}"))?;
        match raw {
            Some(json) => serde_json::from_str(&json).map_err(|e| format!("decode settings: {e}")),
            None => Ok(Settings::default()),
        }
    }

    pub fn save_settings(&self, settings: &Settings) -> Result<(), String> {
        let json = serde_json::to_string(settings).map_err(|e| format!("encode settings: {e}"))?;
        self.conn()?
            .execute(
                "INSERT INTO app_settings(key,value) VALUES('settings',?1)
                 ON CONFLICT(key) DO UPDATE SET value=excluded.value",
                [json],
            )
            .map_err(|e| format!("save settings: {e}"))?;
        Ok(())
    }

    pub fn insert(
        &self,
        item: &CapturedClipboard,
        source_application: Option<&str>,
        created_at: i64,
        deduplicate_consecutive: bool,
    ) -> Result<Option<EntrySummary>, String> {
        let conn = self.conn()?;
        if deduplicate_consecutive {
            let last_hash: Option<String> = conn
                .query_row(
                    "SELECT content_hash FROM clipboard_entries ORDER BY created_at DESC,id DESC LIMIT 1",
                    [],
                    |r| r.get(0),
                )
                .optional()
                .map_err(|e| format!("check duplicate: {e}"))?;
            if last_hash.as_deref() == Some(item.hash.as_str()) {
                return Ok(None);
            }
        }

        let preview = make_preview(item);
        let paths = serde_json::to_string(&item.file_paths).unwrap_or_else(|_| "[]".into());
        conn.execute(
            "INSERT INTO clipboard_entries
             (kind,preview,text_content,html_content,image_png,thumbnail_png,file_paths_json,
              content_hash,source_application,created_at,pinned,byte_size)
             VALUES(?1,?2,?3,?4,?5,?6,?7,?8,?9,?10,0,?11)",
            params![
                item.kind,
                preview,
                item.text,
                item.html,
                item.image_png,
                item.thumbnail_png,
                paths,
                item.hash,
                source_application,
                created_at,
                item.byte_size,
            ],
        )
        .map_err(|e| format!("store clipboard item: {e}"))?;
        let id = conn.last_insert_rowid();
        drop(conn);
        self.get_summary(id)
    }

    pub fn recent(&self, limit: u32) -> Result<Vec<EntrySummary>, String> {
        self.list_internal(None, limit.min(100), 0)
    }

    pub fn history(
        &self,
        query: Option<&str>,
        limit: u32,
        offset: u32,
    ) -> Result<Vec<EntrySummary>, String> {
        self.list_internal(query, limit.clamp(1, 250), offset)
    }

    fn list_internal(
        &self,
        query: Option<&str>,
        limit: u32,
        offset: u32,
    ) -> Result<Vec<EntrySummary>, String> {
        let conn = self.conn()?;
        let search = query.unwrap_or("").trim();
        let like = format!("%{search}%");
        let mut statement = conn
            .prepare(
                "SELECT id,kind,preview,thumbnail_png,file_paths_json,source_application,
                        created_at,pinned,byte_size
                 FROM clipboard_entries
                 WHERE (?1='' OR preview LIKE ?2 COLLATE NOCASE
                     OR COALESCE(text_content,'') LIKE ?2 COLLATE NOCASE
                     OR COALESCE(html_content,'') LIKE ?2 COLLATE NOCASE
                     OR file_paths_json LIKE ?2 COLLATE NOCASE
                     OR COALESCE(source_application,'') LIKE ?2 COLLATE NOCASE)
                 ORDER BY created_at DESC,id DESC LIMIT ?3 OFFSET ?4",
            )
            .map_err(|e| format!("prepare history query: {e}"))?;
        let rows = statement
            .query_map(params![search, like, limit, offset], summary_from_row)
            .map_err(|e| format!("query history: {e}"))?;
        rows.collect::<Result<Vec<_>, _>>()
            .map_err(|e| format!("read history: {e}"))
    }

    pub fn get_summary(&self, id: i64) -> Result<Option<EntrySummary>, String> {
        self.conn()?
            .query_row(
                "SELECT id,kind,preview,thumbnail_png,file_paths_json,source_application,
                        created_at,pinned,byte_size FROM clipboard_entries WHERE id=?1",
                [id],
                summary_from_row,
            )
            .optional()
            .map_err(|e| format!("read clipboard item: {e}"))
    }

    pub fn get_detail(&self, id: i64) -> Result<Option<EntryDetail>, String> {
        let conn = self.conn()?;
        conn.query_row(
            "SELECT id,kind,preview,thumbnail_png,file_paths_json,source_application,
                    created_at,pinned,byte_size,text_content,html_content,image_png
             FROM clipboard_entries WHERE id=?1",
            [id],
            |row| {
                let mut summary = summary_from_row(row)?;
                let image: Option<Vec<u8>> = row.get(11)?;
                summary.image_data_url = image.map(data_url);
                Ok(EntryDetail {
                    summary,
                    text: row.get(9)?,
                    html: row.get(10)?,
                })
            },
        )
        .optional()
        .map_err(|e| format!("read clipboard item details: {e}"))
    }

    pub fn stored(&self, id: i64) -> Result<Option<StoredEntry>, String> {
        self.conn()?
            .query_row(
                "SELECT text_content,html_content,image_png,file_paths_json,content_hash
                 FROM clipboard_entries WHERE id=?1",
                [id],
                |row| {
                    let raw_paths: String = row.get(3)?;
                    Ok(StoredEntry {
                        text: row.get(0)?,
                        html: row.get(1)?,
                        image_png: row.get(2)?,
                        file_paths: serde_json::from_str(&raw_paths).unwrap_or_default(),
                        hash: row.get(4)?,
                    })
                },
            )
            .optional()
            .map_err(|e| format!("load clipboard payload: {e}"))
    }

    pub fn slot(&self, index: usize) -> Result<Option<StoredEntry>, String> {
        self.conn()?
            .query_row(
                "SELECT text_content,html_content,image_png,file_paths_json,content_hash
                 FROM clipboard_entries ORDER BY created_at DESC,id DESC LIMIT 1 OFFSET ?1",
                [index as i64],
                |row| {
                    let raw_paths: String = row.get(3)?;
                    Ok(StoredEntry {
                        text: row.get(0)?,
                        html: row.get(1)?,
                        image_png: row.get(2)?,
                        file_paths: serde_json::from_str(&raw_paths).unwrap_or_default(),
                        hash: row.get(4)?,
                    })
                },
            )
            .optional()
            .map_err(|e| format!("load numbered slot: {e}"))
    }

    pub fn toggle_pin(&self, id: i64) -> Result<Option<EntrySummary>, String> {
        self.conn()?
            .execute("UPDATE clipboard_entries SET pinned=CASE pinned WHEN 0 THEN 1 ELSE 0 END WHERE id=?1", [id])
            .map_err(|e| format!("toggle pin: {e}"))?;
        self.get_summary(id)
    }

    pub fn delete(&self, id: i64) -> Result<bool, String> {
        self.conn()?
            .execute("DELETE FROM clipboard_entries WHERE id=?1", [id])
            .map(|n| n > 0)
            .map_err(|e| format!("delete clipboard item: {e}"))
    }

    pub fn clear(&self) -> Result<u64, String> {
        self.conn()?
            .execute("DELETE FROM clipboard_entries WHERE pinned=0", [])
            .map(|n| n as u64)
            .map_err(|e| format!("clear history: {e}"))
    }

    pub fn count(&self) -> Result<i64, String> {
        self.conn()?
            .query_row("SELECT COUNT(*) FROM clipboard_entries", [], |r| r.get(0))
            .map_err(|e| format!("count history: {e}"))
    }
}

fn summary_from_row(row: &Row<'_>) -> rusqlite::Result<EntrySummary> {
    let thumbnail: Option<Vec<u8>> = row.get(3)?;
    let raw_paths: String = row.get(4)?;
    Ok(EntrySummary {
        id: row.get(0)?,
        kind: row.get(1)?,
        preview: row.get(2)?,
        image_data_url: thumbnail.map(data_url),
        file_paths: serde_json::from_str(&raw_paths).unwrap_or_default(),
        source_application: row.get(5)?,
        created_at: row.get(6)?,
        pinned: row.get::<_, i64>(7)? != 0,
        byte_size: row.get(8)?,
    })
}

fn data_url(bytes: Vec<u8>) -> String {
    format!("data:image/png;base64,{}", BASE64.encode(bytes))
}

fn make_preview(item: &CapturedClipboard) -> String {
    let raw = if !item.file_paths.is_empty() {
        item.file_paths.join("\n")
    } else if let Some(text) = item.text.as_deref() {
        text.to_string()
    } else if let Some(html) = item.html.as_deref() {
        strip_html(html)
    } else if item.image_png.is_some() {
        "Image".into()
    } else {
        "Clipboard item".into()
    };
    truncate_chars(
        &raw.replace(['\r', '\n', '\t'], " ")
            .split_whitespace()
            .collect::<Vec<_>>()
            .join(" "),
        240,
    )
}

fn strip_html(html: &str) -> String {
    let mut output = String::with_capacity(html.len().min(512));
    let mut in_tag = false;
    for ch in html.chars() {
        match ch {
            '<' => in_tag = true,
            '>' => {
                in_tag = false;
                output.push(' ');
            }
            _ if !in_tag => output.push(ch),
            _ => {}
        }
        if output.len() > 1024 {
            break;
        }
    }
    output
}

fn truncate_chars(value: &str, max: usize) -> String {
    let mut chars = value.chars();
    let mut out: String = chars.by_ref().take(max).collect();
    if chars.next().is_some() {
        out.push('…');
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn database() -> Database {
        Database::open(Path::new(":memory:")).expect("in-memory database")
    }

    fn text(value: &str, hash: &str) -> CapturedClipboard {
        CapturedClipboard {
            kind: "text".into(),
            text: Some(value.into()),
            html: None,
            image_png: None,
            thumbnail_png: None,
            file_paths: Vec::new(),
            hash: hash.into(),
            byte_size: value.len() as i64,
        }
    }

    #[test]
    fn recent_slots_are_newest_first() {
        let db = database();
        let oldest = db
            .insert(&text("old", "a"), None, 100, true)
            .unwrap()
            .unwrap();
        let newest = db
            .insert(&text("new", "b"), None, 300, true)
            .unwrap()
            .unwrap();
        let middle = db
            .insert(&text("middle", "c"), None, 200, true)
            .unwrap()
            .unwrap();
        let ids: Vec<i64> = db.recent(10).unwrap().into_iter().map(|v| v.id).collect();
        assert_eq!(ids, vec![newest.id, middle.id, oldest.id]);
    }

    #[test]
    fn only_adjacent_identical_copies_are_deduplicated() {
        let db = database();
        assert!(db
            .insert(&text("same", "same"), None, 1, true)
            .unwrap()
            .is_some());
        assert!(db
            .insert(&text("same", "same"), None, 2, true)
            .unwrap()
            .is_none());
        assert!(db
            .insert(&text("other", "other"), None, 3, true)
            .unwrap()
            .is_some());
        assert!(db
            .insert(&text("same", "same"), None, 4, true)
            .unwrap()
            .is_some());
        assert_eq!(db.count().unwrap(), 3);
    }

    #[test]
    fn clear_preserves_pinned_entries() {
        let db = database();
        let pinned = db
            .insert(&text("keep", "keep"), None, 1, true)
            .unwrap()
            .unwrap();
        db.insert(&text("remove", "remove"), None, 2, true).unwrap();
        assert!(db.toggle_pin(pinned.id).unwrap().unwrap().pinned);
        assert_eq!(db.clear().unwrap(), 1);
        assert_eq!(db.count().unwrap(), 1);
        assert!(db.get_summary(pinned.id).unwrap().unwrap().pinned);
    }

    #[test]
    fn text_history_is_oldest_first_with_blank_lines_and_no_metadata() {
        let db = database();
        db.insert(&text("one", "one"), Some("Notepad.exe"), 100, true)
            .unwrap();
        db.insert(&text("two", "two"), Some("Browser.exe"), 200, true)
            .unwrap();

        let exported = db.text_history_contents().unwrap();

        assert_eq!(exported, "one\r\n\r\ntwo\r\n");
        assert!(!exported.contains("Notepad"));
        assert!(!exported.contains("Browser"));
    }

    #[test]
    fn clearing_text_history_keeps_database_and_starts_a_new_text_log() {
        let db = database();
        db.insert(&text("one", "one"), None, 100, true).unwrap();
        db.insert(&text("two", "two"), None, 200, true).unwrap();

        db.clear_text_history().unwrap();
        assert_eq!(db.count().unwrap(), 2);
        assert_eq!(db.text_history_contents().unwrap(), "");

        db.insert(&text("three", "three"), None, 300, true).unwrap();
        assert_eq!(db.text_history_contents().unwrap(), "three\r\n");
    }
}
