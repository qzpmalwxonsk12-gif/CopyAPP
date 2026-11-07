mod clipboard;
mod db;
mod keyboard;
mod models;
mod pdf;
mod platform;

use std::{
    sync::{
        atomic::{AtomicBool, Ordering},
        mpsc, Arc, Mutex, RwLock,
    },
    thread,
    time::{Duration, Instant, SystemTime, UNIX_EPOCH},
};

use db::Database;
use keyboard::{HookController, KeyboardAction};
use models::{AppStatus, ClipboardChangedEvent, EntryDetail, EntrySummary, Settings, StoredEntry};
use tauri::{
    image::Image,
    menu::{Menu, MenuItem},
    tray::{MouseButton, MouseButtonState, TrayIconBuilder, TrayIconEvent},
    AppHandle, Emitter, Manager, State, WebviewWindow,
};

#[derive(Clone)]
struct AppState {
    db: Arc<Database>,
    settings: Arc<RwLock<Settings>>,
    shutdown: Arc<AtomicBool>,
    quit_requested: Arc<AtomicBool>,
    hook: Arc<Mutex<Option<HookController>>>,
    monitor_shutdown: Arc<Mutex<Option<clipboard::MonitorShutdown>>>,
    watcher_active: Arc<AtomicBool>,
    suppression: Arc<Mutex<Option<(String, Instant)>>>,
    last_foreground: Arc<Mutex<Option<isize>>>,
    current_source: Arc<Mutex<Option<String>>>,
    last_error: Arc<Mutex<Option<String>>>,
}

impl AppState {
    fn settings(&self) -> Result<Settings, String> {
        self.settings
            .read()
            .map(|v| v.clone())
            .map_err(|_| "settings lock was poisoned".into())
    }

    fn hook_active(&self) -> bool {
        self.hook
            .lock()
            .ok()
            .and_then(|v| v.as_ref().map(HookController::is_active))
            .unwrap_or(false)
    }
}

#[tauri::command]
fn get_recent_slots(state: State<'_, AppState>) -> Result<Vec<EntrySummary>, String> {
    let slot_count = state.settings()?.slot_shortcuts.len() as u32;
    state.db.recent(slot_count)
}

#[tauri::command]
fn get_history(
    query: Option<String>,
    limit: Option<u32>,
    offset: Option<u32>,
    state: State<'_, AppState>,
) -> Result<Vec<EntrySummary>, String> {
    state
        .db
        .history(query.as_deref(), limit.unwrap_or(100), offset.unwrap_or(0))
}

#[tauri::command]
fn get_entry(id: i64, state: State<'_, AppState>) -> Result<EntryDetail, String> {
    state
        .db
        .get_detail(id)?
        .ok_or_else(|| format!("clipboard entry {id} was not found"))
}

#[tauri::command]
fn copy_entry(id: i64, app: AppHandle, state: State<'_, AppState>) -> Result<bool, String> {
    let entry = state
        .db
        .stored(id)?
        .ok_or_else(|| format!("clipboard entry {id} was not found"))?;
    write_stored_entry(&app, &state, &entry)?;
    Ok(true)
}

#[tauri::command]
fn paste_entry(id: i64, app: AppHandle, state: State<'_, AppState>) -> Result<bool, String> {
    let entry = state
        .db
        .stored(id)?
        .ok_or_else(|| format!("clipboard entry {id} was not found"))?;
    write_stored_entry(&app, &state, &entry)?;
    if let Some(window) = app.get_webview_window("main") {
        let _ = window.hide();
    }
    thread::sleep(Duration::from_millis(45));
    if let Ok(target) = state.last_foreground.lock() {
        if let Some(handle) = *target {
            let _ = platform::activate_window(handle);
        }
    }
    thread::sleep(Duration::from_millis(35));
    keyboard::send_paste()?;
    Ok(true)
}

#[tauri::command]
fn toggle_pin(id: i64, app: AppHandle, state: State<'_, AppState>) -> Result<EntrySummary, String> {
    let entry = state
        .db
        .toggle_pin(id)?
        .ok_or_else(|| format!("clipboard entry {id} was not found"))?;
    emit_change(&app, "pinned", Some(entry.clone()));
    Ok(entry)
}

#[tauri::command]
fn delete_entry(id: i64, app: AppHandle, state: State<'_, AppState>) -> Result<bool, String> {
    let deleted = state.db.delete(id)?;
    if deleted {
        refresh_text_history(&app, &state);
        emit_change(&app, "deleted", None);
    }
    Ok(deleted)
}

#[tauri::command]
fn clear_history(app: AppHandle, state: State<'_, AppState>) -> Result<u64, String> {
    let deleted = state.db.clear()?;
    refresh_text_history(&app, &state);
    emit_change(&app, "cleared", None);
    Ok(deleted)
}

#[tauri::command]
fn get_settings(state: State<'_, AppState>) -> Result<Settings, String> {
    state.settings()
}

#[tauri::command]
fn update_settings(
    mut settings: Settings,
    app: AppHandle,
    state: State<'_, AppState>,
) -> Result<Settings, String> {
    validate_settings(&settings)?;
    let previous = state.settings()?;
    // Recording has its own deliberate privacy control; a visual settings save must not unpause it.
    settings.recording_paused = previous.recording_paused;
    if settings.launch_at_startup != previous.launch_at_startup {
        platform::set_launch_at_startup(settings.launch_at_startup)?;
    }
    keyboard::update(&settings.shortcuts())?;
    state.db.save_settings(&settings)?;
    *state
        .settings
        .write()
        .map_err(|_| "settings lock was poisoned".to_string())? = settings.clone();
    let _ = app.emit("settings://changed", &settings);
    Ok(settings)
}

#[tauri::command]
fn set_recording_paused(
    paused: bool,
    app: AppHandle,
    state: State<'_, AppState>,
) -> Result<bool, String> {
    let mut settings = state.settings()?;
    settings.recording_paused = paused;
    state.db.save_settings(&settings)?;
    *state
        .settings
        .write()
        .map_err(|_| "settings lock was poisoned".to_string())? = settings;
    let _ = app.emit("recording://changed", paused);
    Ok(paused)
}

#[tauri::command]
fn toggle_window(app: AppHandle, state: State<'_, AppState>) -> Result<bool, String> {
    toggle_window_internal(&app, &state)
}

#[tauri::command]
fn open_text_history(state: State<'_, AppState>) -> Result<String, String> {
    state.db.refresh_text_history_file()?;
    let path = state.db.text_history_path();
    platform::open_file(&path)?;
    Ok(path.to_string_lossy().into_owned())
}

#[tauri::command]
fn clear_text_history(app: AppHandle, state: State<'_, AppState>) -> Result<bool, String> {
    state.db.clear_text_history()?;
    state.db.refresh_text_history_file()?;
    emit_change(&app, "text-history-cleared", None);
    Ok(true)
}

#[tauri::command]
fn convert_text_history_to_pdf(state: State<'_, AppState>) -> Result<String, String> {
    state.db.refresh_text_history_file()?;
    let text_path = state.db.text_history_path();
    let text = std::fs::read_to_string(&text_path)
        .map_err(|e| format!("read text history {}: {e}", text_path.display()))?;
    let pdf_path = text_path.with_extension("pdf");
    pdf::write_history_pdf(&pdf_path, &text)?;
    platform::open_file(&pdf_path)?;
    Ok(pdf_path.to_string_lossy().into_owned())
}

#[tauri::command]
fn get_status(state: State<'_, AppState>) -> Result<AppStatus, String> {
    let settings = state.settings()?;
    let current_source_application = state.current_source.lock().ok().and_then(|v| v.clone());
    Ok(AppStatus {
        recording_paused: settings.recording_paused,
        hook_active: state.hook_active(),
        watcher_active: state.watcher_active.load(Ordering::Acquire),
        database_ready: true,
        entry_count: state.db.count()?,
        database_path: state.db.path_display(),
        text_history_path: state.db.text_history_path_display(),
        current_source_application,
        last_error: state.last_error.lock().ok().and_then(|v| v.clone()),
        platform: if cfg!(windows) {
            "windows"
        } else {
            std::env::consts::OS
        },
    })
}

fn validate_settings(settings: &Settings) -> Result<(), String> {
    if !(10..=50).contains(&settings.slot_shortcuts.len()) {
        return Err("slotShortcuts must contain between ten and fifty shortcuts".into());
    }
    if !matches!(settings.theme.as_str(), "system" | "light" | "dark") {
        return Err("theme must be system, light, or dark".into());
    }
    let mut seen = std::collections::HashSet::new();
    for shortcut in settings
        .slot_shortcuts
        .iter()
        .chain(std::iter::once(&settings.open_shortcut))
    {
        let normalized = shortcut.trim().to_ascii_lowercase();
        if !normalized.is_empty() && !seen.insert(normalized) {
            return Err(format!("duplicate shortcut: {shortcut}"));
        }
    }
    Ok(())
}

fn write_stored_entry(
    app: &AppHandle,
    state: &AppState,
    entry: &StoredEntry,
) -> Result<(), String> {
    if let Ok(mut suppression) = state.suppression.lock() {
        *suppression = Some((entry.hash.clone(), Instant::now()));
    }
    if let Err(error) = clipboard::write(entry) {
        if let Ok(mut suppression) = state.suppression.lock() {
            *suppression = None;
        }
        report_error(app, state, error.clone());
        return Err(error);
    }
    Ok(())
}

fn refresh_text_history(app: &AppHandle, state: &AppState) {
    if let Err(error) = state.db.refresh_text_history_file() {
        report_error(app, state, error);
    }
}

fn toggle_window_internal(app: &AppHandle, state: &AppState) -> Result<bool, String> {
    let window = app
        .get_webview_window("main")
        .ok_or_else(|| "main window is unavailable".to_string())?;
    let visible = window
        .is_visible()
        .map_err(|e| format!("read window state: {e}"))?;
    if visible {
        window.hide().map_err(|e| format!("hide window: {e}"))?;
        Ok(false)
    } else {
        remember_foreground(state);
        show_window(&window)?;
        Ok(true)
    }
}

fn show_window(window: &WebviewWindow) -> Result<(), String> {
    window
        .unminimize()
        .map_err(|e| format!("restore window: {e}"))?;
    window.show().map_err(|e| format!("show window: {e}"))?;
    window.set_focus().map_err(|e| format!("focus window: {e}"))
}

fn remember_foreground(state: &AppState) {
    if let Some((handle, path)) = platform::foreground_application() {
        if let Ok(mut target) = state.last_foreground.lock() {
            *target = Some(handle);
        }
        if let Ok(mut source) = state.current_source.lock() {
            *source = Some(platform::source_name(&path));
        }
    }
}

fn emit_change(app: &AppHandle, reason: &'static str, entry: Option<EntrySummary>) {
    let _ = app.emit(
        "clipboard://changed",
        ClipboardChangedEvent { reason, entry },
    );
}

fn report_error(app: &AppHandle, state: &AppState, error: String) {
    if let Ok(mut last) = state.last_error.lock() {
        *last = Some(error.clone());
    }
    let _ = app.emit("clipboard://error", error);
}

fn clear_error(app: &AppHandle, state: &AppState) {
    let recovered = state
        .last_error
        .lock()
        .ok()
        .is_some_and(|mut last| last.take().is_some());
    if recovered {
        emit_change(app, "recovered", None);
    }
}

fn start_clipboard_watcher(app: AppHandle, state: AppState) -> Result<(), String> {
    let (ready_sender, ready_receiver) = mpsc::sync_channel(1);
    let startup_app = app.clone();
    let startup_state = state.clone();
    thread::Builder::new()
        .name("copyapp-clipboard-monitor".into())
        .spawn(move || {
            startup_state.watcher_active.store(true, Ordering::Release);
            let callback_app = startup_app.clone();
            let callback_state = startup_state.clone();
            let shutdown = Arc::clone(&startup_state.shutdown);
            let result = clipboard::run_change_monitor(shutdown, ready_sender, move || {
                let settings = match callback_state.settings() {
                    Ok(settings) => settings,
                    Err(error) => {
                        report_error(&callback_app, &callback_state, error);
                        return;
                    }
                };
                if settings.recording_paused {
                    return;
                }

                let source = platform::foreground_application();
                if let Some((_, ref path)) = source {
                    if platform::is_excluded(path, &settings.excluded_applications) {
                        return;
                    }
                }
                let source_name = source.as_ref().map(|(_, path)| platform::source_name(path));
                if let Ok(mut current) = callback_state.current_source.lock() {
                    *current = source_name.clone();
                }

                let captured = match clipboard::capture() {
                    Ok(Some(item)) => item,
                    Ok(None) => return,
                    Err(error) => {
                        report_error(&callback_app, &callback_state, error);
                        return;
                    }
                };
                clear_error(&callback_app, &callback_state);
                let suppressed = callback_state
                    .suppression
                    .lock()
                    .ok()
                    .is_some_and(|mut guard| match guard.as_ref() {
                        Some((hash, when))
                            if hash == &captured.hash
                                && when.elapsed() < Duration::from_secs(4) =>
                        {
                            *guard = None;
                            true
                        }
                        Some((_, when)) if when.elapsed() >= Duration::from_secs(4) => {
                            *guard = None;
                            false
                        }
                        _ => false,
                    });
                if suppressed {
                    return;
                }
                match callback_state.db.insert(
                    &captured,
                    source_name.as_deref(),
                    unix_millis(),
                    settings.deduplicate_consecutive,
                ) {
                    Ok(Some(entry)) => {
                        refresh_text_history(&callback_app, &callback_state);
                        emit_change(&callback_app, "captured", Some(entry));
                    }
                    Ok(None) => {}
                    Err(error) => report_error(&callback_app, &callback_state, error),
                }
            });
            startup_state.watcher_active.store(false, Ordering::Release);
            if let Err(error) = result {
                report_error(&startup_app, &startup_state, error);
            }
        })
        .map_err(|e| format!("start clipboard monitor: {e}"))?;
    match ready_receiver
        .recv_timeout(Duration::from_secs(3))
        .map_err(|_| "clipboard watcher did not report startup readiness".to_string())?
    {
        clipboard::WatcherReady::Native(shutdown) => {
            *state
                .monitor_shutdown
                .lock()
                .map_err(|_| "clipboard monitor lock was poisoned".to_string())? = Some(shutdown);
        }
        clipboard::WatcherReady::Fallback(reason) => report_error(&app, &state, reason),
    }
    Ok(())
}

fn start_keyboard_worker(app: AppHandle, state: AppState) -> Result<(), String> {
    let (sender, receiver) = mpsc::channel();
    let controller = keyboard::start(&state.settings()?.shortcuts(), sender)?;
    *state
        .hook
        .lock()
        .map_err(|_| "keyboard hook lock was poisoned".to_string())? = Some(controller);
    thread::Builder::new()
        .name("copyapp-shortcut-actions".into())
        .spawn(move || {
            while !state.shutdown.load(Ordering::Acquire) {
                let action = match receiver.recv_timeout(Duration::from_millis(500)) {
                    Ok(action) => action,
                    Err(mpsc::RecvTimeoutError::Timeout) => continue,
                    Err(mpsc::RecvTimeoutError::Disconnected) => break,
                };
                match action {
                    KeyboardAction::NormalPaste => {
                        let _ = keyboard::send_paste();
                    }
                    KeyboardAction::ToggleWindow => {
                        let _ = toggle_window_internal(&app, &state);
                    }
                    KeyboardAction::PasteSlot(slot) => match state.db.slot(slot) {
                        Ok(Some(entry)) => {
                            if write_stored_entry(&app, &state, &entry).is_ok() {
                                thread::sleep(Duration::from_millis(18));
                                let _ = keyboard::send_paste();
                            }
                        }
                        Ok(None) => {}
                        Err(error) => report_error(&app, &state, error),
                    },
                }
            }
        })
        .map(|_| ())
        .map_err(|e| format!("start shortcut action worker: {e}"))
}

fn stop_background_services(state: &AppState) {
    state.shutdown.store(true, Ordering::Release);
    if let Ok(mut monitor) = state.monitor_shutdown.lock() {
        if let Some(monitor) = monitor.take() {
            monitor.stop();
        }
    }
    if let Ok(hook) = state.hook.lock() {
        if let Some(hook) = hook.as_ref() {
            hook.stop();
        }
    }
}

fn setup_tray(app: &AppHandle) -> Result<(), String> {
    let toggle = MenuItem::with_id(app, "toggle", "Open CopyAPP", true, None::<&str>)
        .map_err(|e| format!("create tray menu: {e}"))?;
    let pause = MenuItem::with_id(app, "pause", "Pause/resume recording", true, None::<&str>)
        .map_err(|e| format!("create tray menu: {e}"))?;
    let quit = MenuItem::with_id(app, "quit", "Quit", true, None::<&str>)
        .map_err(|e| format!("create tray menu: {e}"))?;
    let menu = Menu::with_items(app, &[&toggle, &pause, &quit])
        .map_err(|e| format!("create tray menu: {e}"))?;
    let icon = tray_icon_image();
    TrayIconBuilder::with_id("main-tray")
        .tooltip("CopyAPP clipboard history")
        .icon(icon)
        .menu(&menu)
        .show_menu_on_left_click(false)
        .on_menu_event(|app, event| match event.id().as_ref() {
            "toggle" => {
                let state = app.state::<AppState>();
                let _ = toggle_window_internal(app, &state);
            }
            "pause" => {
                let state = app.state::<AppState>();
                if let Ok(settings) = state.settings() {
                    let _ = set_recording_paused(!settings.recording_paused, app.clone(), state);
                }
            }
            "quit" => {
                let state = app.state::<AppState>();
                state.quit_requested.store(true, Ordering::Release);
                stop_background_services(&state);
                app.exit(0);
            }
            _ => {}
        })
        .on_tray_icon_event(|tray, event| {
            if let TrayIconEvent::Click {
                button: MouseButton::Left,
                button_state: MouseButtonState::Up,
                ..
            } = event
            {
                let app = tray.app_handle();
                let state = app.state::<AppState>();
                let _ = toggle_window_internal(app, &state);
            }
        })
        .build(app)
        .map(|_| ())
        .map_err(|e| format!("create system tray icon: {e}"))
}

fn tray_icon_image() -> Image<'static> {
    let size = 32usize;
    let mut rgba = vec![0u8; size * size * 4];
    for y in 0..size {
        for x in 0..size {
            let i = (y * size + x) * 4;
            let dx = x as isize - 16;
            let dy = y as isize - 16;
            let inside = dx * dx + dy * dy <= 14 * 14;
            rgba[i] = if inside { 105 } else { 0 };
            rgba[i + 1] = if inside { 91 } else { 0 };
            rgba[i + 2] = if inside { 235 } else { 0 };
            rgba[i + 3] = if inside { 255 } else { 0 };
        }
    }
    Image::new_owned(rgba, size as u32, size as u32)
}

fn unix_millis() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis()
        .min(i64::MAX as u128) as i64
}

pub fn run() {
    let builder = tauri::Builder::default()
        .invoke_handler(tauri::generate_handler![
            get_recent_slots, get_history, get_entry, paste_entry, copy_entry, toggle_pin,
            delete_entry, clear_history, get_settings, update_settings, set_recording_paused,
            toggle_window, open_text_history, clear_text_history, convert_text_history_to_pdf,
            get_status,
        ])
        .on_window_event(|window, event| {
            match event {
                tauri::WindowEvent::CloseRequested { api, .. } => {
                    let state = window.app_handle().state::<AppState>();
                    if state.settings().map(|s| s.close_to_tray).unwrap_or(true) {
                        api.prevent_close();
                        let _ = window.hide();
                    }
                }
                tauri::WindowEvent::Resized(_) => {
                    let state = window.app_handle().state::<AppState>();
                    let minimize = state.settings().map(|s| s.minimize_to_tray).unwrap_or(true);
                    if minimize && window.is_minimized().unwrap_or(false) { let _ = window.hide(); }
                }
                _ => {}
            }
        })
        .setup(|app| {
            let data_dir = app.path().app_local_data_dir().map_err(|e| format!("resolve app data directory: {e}"))?;
            let db = Arc::new(Database::open(&data_dir.join("clipboard-history.sqlite3"))?);
            let (settings, startup_error) = match db.load_settings() {
                Ok(settings) => (settings, None),
                Err(error) => {
                    let mut safe = Settings::default();
                    safe.recording_paused = true;
                    db.save_settings(&safe)?;
                    (safe, Some(format!("Settings could not be decoded; recording was paused for safety: {error}")))
                }
            };
            let state = AppState {
                db,
                settings: Arc::new(RwLock::new(settings)),
                shutdown: Arc::new(AtomicBool::new(false)),
                quit_requested: Arc::new(AtomicBool::new(false)),
                hook: Arc::new(Mutex::new(None)),
                monitor_shutdown: Arc::new(Mutex::new(None)),
                watcher_active: Arc::new(AtomicBool::new(false)),
                suppression: Arc::new(Mutex::new(None)),
                last_foreground: Arc::new(Mutex::new(None)),
                current_source: Arc::new(Mutex::new(None)),
                last_error: Arc::new(Mutex::new(startup_error)),
            };
            app.manage(state.clone());
            refresh_text_history(app.handle(), &state);
            setup_tray(app.handle())?;
            start_clipboard_watcher(app.handle().clone(), state.clone())?;
            start_keyboard_worker(app.handle().clone(), state)?;
            if std::env::args_os().any(|argument| argument == "--show") {
                let app_handle = app.handle().clone();
                thread::spawn(move || {
                    thread::sleep(Duration::from_millis(250));
                    if let Some(window) = app_handle.get_webview_window("main") {
                        let _ = show_window(&window);
                    }
                });
            }
            Ok(())
        });

    let app = builder
        .build(tauri::generate_context!())
        .expect("failed to build CopyAPP");
    app.run(|handle, event| {
        if let tauri::RunEvent::Exit = event {
            let state = handle.state::<AppState>();
            stop_background_services(&state);
        }
    });
}
