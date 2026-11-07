use std::sync::mpsc::Sender;

use crate::models::ShortcutSettings;

#[derive(Debug, Clone, Copy)]
pub enum KeyboardAction {
    PasteSlot(usize),
    ToggleWindow,
    NormalPaste,
}

#[cfg(windows)]
mod imp {
    use std::{
        collections::HashSet,
        mem::{size_of, zeroed},
        ptr::null_mut,
        sync::{
            atomic::{AtomicBool, AtomicU32, Ordering},
            Arc, Mutex, OnceLock, RwLock,
        },
        thread,
    };

    use windows_sys::Win32::System::Threading::GetCurrentThreadId;
    use windows_sys::Win32::{
        Foundation::{LPARAM, LRESULT, WPARAM},
        UI::{
            Input::KeyboardAndMouse::{
                GetAsyncKeyState, SendInput, INPUT, INPUT_0, INPUT_KEYBOARD, KEYBDINPUT,
                KEYEVENTF_KEYUP, VK_CONTROL, VK_LCONTROL, VK_LMENU, VK_LSHIFT, VK_LWIN, VK_MENU,
                VK_RCONTROL, VK_RMENU, VK_RSHIFT, VK_RWIN, VK_SHIFT,
            },
            WindowsAndMessaging::{
                CallNextHookEx, DispatchMessageW, GetMessageW, PostThreadMessageW,
                SetWindowsHookExW, TranslateMessage, UnhookWindowsHookEx, KBDLLHOOKSTRUCT,
                LLKHF_INJECTED, MSG, WH_KEYBOARD_LL, WM_KEYDOWN, WM_KEYUP, WM_QUIT, WM_SYSKEYDOWN,
                WM_SYSKEYUP,
            },
        },
    };

    use super::{KeyboardAction, Sender, ShortcutSettings};

    const VK_V: u32 = 0x56;

    #[derive(Clone, Debug, Default)]
    struct Binding {
        keys: Vec<u32>,
        terminal: u32,
        ctrl_v_sequence: bool,
    }

    #[derive(Default)]
    struct Bindings {
        open: Option<Binding>,
        slots: Vec<Option<Binding>>,
    }

    #[derive(Default)]
    struct InputState {
        pressed: HashSet<u32>,
        swallowed: HashSet<u32>,
        ctrl_v_armed: bool,
    }

    struct HookGlobals {
        sender: Sender<KeyboardAction>,
        bindings: RwLock<Bindings>,
        input: Mutex<InputState>,
    }

    static GLOBALS: OnceLock<HookGlobals> = OnceLock::new();

    pub struct HookController {
        active: Arc<AtomicBool>,
        thread_id: Arc<AtomicU32>,
    }

    impl HookController {
        pub fn is_active(&self) -> bool {
            self.active.load(Ordering::Acquire)
        }
        pub fn stop(&self) {
            let id = self.thread_id.load(Ordering::Acquire);
            if id != 0 {
                unsafe {
                    PostThreadMessageW(id, WM_QUIT, 0, 0);
                }
            }
        }
    }

    impl Drop for HookController {
        fn drop(&mut self) {
            self.stop();
        }
    }

    pub fn start(
        shortcuts: &ShortcutSettings,
        sender: Sender<KeyboardAction>,
    ) -> Result<HookController, String> {
        let globals = HookGlobals {
            sender,
            bindings: RwLock::new(compile(shortcuts)?),
            input: Mutex::new(InputState::default()),
        };
        GLOBALS
            .set(globals)
            .map_err(|_| "keyboard hook already initialized".to_string())?;

        let active = Arc::new(AtomicBool::new(false));
        let thread_id = Arc::new(AtomicU32::new(0));
        let active_thread = Arc::clone(&active);
        let id_thread = Arc::clone(&thread_id);
        let (ready_sender, ready_receiver) = std::sync::mpsc::sync_channel(1);
        thread::Builder::new()
            .name("copyapp-keyboard-hook".into())
            .spawn(move || unsafe {
                id_thread.store(GetCurrentThreadId(), Ordering::Release);
                let hook = SetWindowsHookExW(WH_KEYBOARD_LL, Some(keyboard_proc), null_mut(), 0);
                if hook.is_null() {
                    let _ = ready_sender.send(Err(format!(
                        "install Windows keyboard hook: {}",
                        std::io::Error::last_os_error()
                    )));
                    return;
                }
                active_thread.store(true, Ordering::Release);
                let _ = ready_sender.send(Ok(()));
                let mut message: MSG = zeroed();
                while GetMessageW(&mut message, null_mut(), 0, 0) > 0 {
                    TranslateMessage(&message);
                    DispatchMessageW(&message);
                }
                active_thread.store(false, Ordering::Release);
                UnhookWindowsHookEx(hook);
            })
            .map_err(|e| format!("start keyboard hook: {e}"))?;
        ready_receiver
            .recv_timeout(std::time::Duration::from_secs(3))
            .map_err(|_| "keyboard hook did not report startup readiness".to_string())??;
        Ok(HookController { active, thread_id })
    }

    pub fn update(shortcuts: &ShortcutSettings) -> Result<(), String> {
        let compiled = compile(shortcuts)?;
        let globals = GLOBALS
            .get()
            .ok_or_else(|| "keyboard hook is not initialized".to_string())?;
        *globals
            .bindings
            .write()
            .map_err(|_| "shortcut lock was poisoned".to_string())? = compiled;
        *globals
            .input
            .lock()
            .map_err(|_| "keyboard input state lock was poisoned".to_string())? =
            InputState::default();
        Ok(())
    }

    unsafe extern "system" fn keyboard_proc(code: i32, wparam: WPARAM, lparam: LPARAM) -> LRESULT {
        if code < 0 {
            return CallNextHookEx(null_mut(), code, wparam, lparam);
        }
        let event = &*(lparam as *const KBDLLHOOKSTRUCT);
        if event.flags & LLKHF_INJECTED != 0 {
            return CallNextHookEx(null_mut(), code, wparam, lparam);
        }
        let message = wparam as u32;
        let down = message == WM_KEYDOWN || message == WM_SYSKEYDOWN;
        let up = message == WM_KEYUP || message == WM_SYSKEYUP;
        if !down && !up {
            return CallNextHookEx(null_mut(), code, wparam, lparam);
        }
        let key = normalize_key(event.vkCode);
        let Some(globals) = GLOBALS.get() else {
            return CallNextHookEx(null_mut(), code, wparam, lparam);
        };
        let Ok(bindings) = globals.bindings.read() else {
            return CallNextHookEx(null_mut(), code, wparam, lparam);
        };
        let Ok(mut state) = globals.input.lock() else {
            return CallNextHookEx(null_mut(), code, wparam, lparam);
        };

        if up {
            state.pressed.remove(&key);
            if state.swallowed.remove(&key) {
                return 1;
            }
            if key == VK_CONTROL as u32 && state.ctrl_v_armed {
                state.ctrl_v_armed = false;
                let _ = globals.sender.send(KeyboardAction::NormalPaste);
            }
            return CallNextHookEx(null_mut(), code, wparam, lparam);
        }

        let repeated = !state.pressed.insert(key);
        if repeated {
            return if state.swallowed.contains(&key) {
                1
            } else {
                CallNextHookEx(null_mut(), code, wparam, lparam)
            };
        }

        // A low-level hook can miss a modifier key-up while Windows is switching
        // desktops or the process is starting. Reconcile held modifiers from the
        // physical keyboard before matching a terminal key so stale Ctrl/Shift
        // state can never turn an ordinary Space press into a global shortcut.
        if !is_modifier(key) {
            sync_modifier_state_with(&mut state.pressed, physical_modifier_down);
        }

        if state.ctrl_v_armed {
            if let Some(binding) = bindings.open.as_ref() {
                if binding.ctrl_v_sequence
                    && binding.terminal == key
                    && sequence_matches(binding, &state.pressed)
                {
                    state.ctrl_v_armed = false;
                    state.swallowed.insert(key);
                    let _ = globals.sender.send(KeyboardAction::ToggleWindow);
                    return 1;
                }
            }
            if let Some((slot, _)) = bindings.slots.iter().enumerate().find(|(_, b)| {
                b.as_ref().is_some_and(|b| {
                    b.ctrl_v_sequence && b.terminal == key && sequence_matches(b, &state.pressed)
                })
            }) {
                state.ctrl_v_armed = false;
                state.swallowed.insert(key);
                let _ = globals.sender.send(KeyboardAction::PasteSlot(slot));
                return 1;
            }
            if !is_modifier(key) && key != VK_V {
                state.ctrl_v_armed = false;
                let _ = globals.sender.send(KeyboardAction::NormalPaste);
            }
        }

        if let Some(binding) = bindings.open.as_ref() {
            if !binding.ctrl_v_sequence
                && binding.terminal == key
                && binding_matches(binding, &state.pressed)
            {
                state.swallowed.insert(key);
                let _ = globals.sender.send(KeyboardAction::ToggleWindow);
                return 1;
            }
        }
        if let Some((slot, _)) = bindings.slots.iter().enumerate().find(|(_, b)| {
            b.as_ref().is_some_and(|b| {
                !b.ctrl_v_sequence && b.terminal == key && binding_matches(b, &state.pressed)
            })
        }) {
            state.swallowed.insert(key);
            let _ = globals.sender.send(KeyboardAction::PasteSlot(slot));
            return 1;
        }

        let has_ctrl_v_binding = bindings.slots.iter().flatten().any(|b| b.ctrl_v_sequence)
            || bindings.open.as_ref().is_some_and(|b| b.ctrl_v_sequence);
        if key == VK_V && state.pressed.contains(&(VK_CONTROL as u32)) && has_ctrl_v_binding {
            state.ctrl_v_armed = true;
            state.swallowed.insert(key);
            return 1;
        }
        CallNextHookEx(null_mut(), code, wparam, lparam)
    }

    fn binding_matches(binding: &Binding, pressed: &HashSet<u32>) -> bool {
        binding.keys.iter().all(|key| pressed.contains(key)) && modifiers_match(binding, pressed)
    }

    fn sequence_matches(binding: &Binding, pressed: &HashSet<u32>) -> bool {
        binding
            .keys
            .iter()
            .all(|key| *key == VK_V || pressed.contains(key))
            && modifiers_match(binding, pressed)
    }

    fn modifiers_match(binding: &Binding, pressed: &HashSet<u32>) -> bool {
        [
            VK_CONTROL as u32,
            VK_SHIFT as u32,
            VK_MENU as u32,
            VK_LWIN as u32,
        ]
        .iter()
        .all(|modifier| binding.keys.contains(modifier) == pressed.contains(modifier))
    }

    fn sync_modifier_state_with(pressed: &mut HashSet<u32>, is_down: impl Fn(u32) -> bool) {
        for modifier in [
            VK_CONTROL as u32,
            VK_SHIFT as u32,
            VK_MENU as u32,
            VK_LWIN as u32,
        ] {
            if is_down(modifier) {
                pressed.insert(modifier);
            } else {
                pressed.remove(&modifier);
            }
        }
    }

    fn physical_modifier_down(modifier: u32) -> bool {
        unsafe {
            match modifier {
                key if key == VK_LWIN as u32 => {
                    GetAsyncKeyState(VK_LWIN as i32) < 0 || GetAsyncKeyState(VK_RWIN as i32) < 0
                }
                key => GetAsyncKeyState(key as i32) < 0,
            }
        }
    }

    fn compile(shortcuts: &ShortcutSettings) -> Result<Bindings, String> {
        if !(10..=50).contains(&shortcuts.slots.len()) {
            return Err("between ten and fifty slot shortcuts are required".into());
        }
        let open = parse_binding(&shortcuts.open_close)?;
        let mut slots = Vec::with_capacity(shortcuts.slots.len());
        for value in &shortcuts.slots {
            slots.push(parse_binding(value)?);
        }
        let mut canonical = HashSet::new();
        for binding in slots.iter().chain(std::iter::once(&open)).flatten() {
            let mut keys = binding.keys.clone();
            keys.sort_unstable();
            if !canonical.insert(keys) {
                return Err("two actions cannot use the same shortcut".into());
            }
        }
        Ok(Bindings { open, slots })
    }

    fn parse_binding(value: &str) -> Result<Option<Binding>, String> {
        let value = value.trim();
        if value.is_empty() {
            return Ok(None);
        }
        let mut keys = Vec::new();
        for token in value.split('+').map(str::trim).filter(|v| !v.is_empty()) {
            let key =
                parse_key(token).ok_or_else(|| format!("unsupported shortcut key: {token}"))?;
            if !keys.contains(&key) {
                keys.push(key);
            }
        }
        if keys.is_empty() {
            return Ok(None);
        }
        if keys.len() == 2 && keys.contains(&(VK_CONTROL as u32)) && keys.contains(&VK_V) {
            return Err("Ctrl+V is reserved for normal paste and cannot be assigned".into());
        }
        let ctrl_v_sequence =
            keys.contains(&(VK_CONTROL as u32)) && keys.contains(&VK_V) && keys.len() >= 3;
        let terminal = *keys
            .iter()
            .rev()
            .find(|key| !is_modifier(**key) && (!ctrl_v_sequence || **key != VK_V))
            .unwrap_or(keys.last().unwrap());
        Ok(Some(Binding {
            keys,
            terminal,
            ctrl_v_sequence,
        }))
    }

    fn parse_key(token: &str) -> Option<u32> {
        let upper = token.to_ascii_uppercase().replace([' ', '_', '-'], "");
        Some(match upper.as_str() {
            "CTRL" | "CONTROL" | "LCTRL" | "RCTRL" => VK_CONTROL as u32,
            "SHIFT" | "LSHIFT" | "RSHIFT" => VK_SHIFT as u32,
            "ALT" | "MENU" | "LALT" | "RALT" => VK_MENU as u32,
            "WIN" | "WINDOWS" | "META" | "SUPER" => VK_LWIN as u32,
            "SPACE" | "SPACEBAR" => 0x20,
            "TAB" => 0x09,
            "ENTER" | "RETURN" => 0x0D,
            "ESC" | "ESCAPE" => 0x1B,
            "BACKSPACE" => 0x08,
            "INSERT" | "INS" => 0x2D,
            "DELETE" | "DEL" => 0x2E,
            "HOME" => 0x24,
            "END" => 0x23,
            "PAGEUP" | "PGUP" => 0x21,
            "PAGEDOWN" | "PGDN" => 0x22,
            "LEFT" => 0x25,
            "UP" => 0x26,
            "RIGHT" => 0x27,
            "DOWN" => 0x28,
            value if value.len() == 1 => {
                let byte = value.as_bytes()[0];
                if byte.is_ascii_alphanumeric() {
                    byte as u32
                } else {
                    return None;
                }
            }
            value if value.starts_with("DIGIT") && value.len() == 6 => value.as_bytes()[5] as u32,
            value if value.starts_with('F') => {
                let number: u32 = value[1..].parse().ok()?;
                if !(1..=24).contains(&number) {
                    return None;
                }
                0x6F + number
            }
            _ => return None,
        })
    }

    fn normalize_key(key: u32) -> u32 {
        match key as u16 {
            VK_LCONTROL | VK_RCONTROL => VK_CONTROL as u32,
            VK_LSHIFT | VK_RSHIFT => VK_SHIFT as u32,
            VK_LMENU | VK_RMENU => VK_MENU as u32,
            VK_LWIN | VK_RWIN => VK_LWIN as u32,
            _ => key,
        }
    }

    fn is_modifier(key: u32) -> bool {
        matches!(key, x if x == VK_CONTROL as u32 || x == VK_SHIFT as u32 || x == VK_MENU as u32 || x == VK_LWIN as u32)
    }

    pub fn send_paste() -> Result<(), String> {
        unsafe {
            let ctrl_down = GetAsyncKeyState(VK_CONTROL as i32) < 0;
            let keys: Vec<(u16, bool)> = if ctrl_down {
                vec![(VK_V as u16, false), (VK_V as u16, true)]
            } else {
                vec![
                    (VK_CONTROL, false),
                    (VK_V as u16, false),
                    (VK_V as u16, true),
                    (VK_CONTROL, true),
                ]
            };
            let mut inputs = Vec::with_capacity(keys.len());
            for (key, up) in keys {
                inputs.push(INPUT {
                    r#type: INPUT_KEYBOARD,
                    Anonymous: INPUT_0 {
                        ki: KEYBDINPUT {
                            wVk: key,
                            wScan: 0,
                            dwFlags: if up { KEYEVENTF_KEYUP } else { 0 },
                            time: 0,
                            dwExtraInfo: 0,
                        },
                    },
                });
            }
            let sent = SendInput(
                inputs.len() as u32,
                inputs.as_ptr(),
                size_of::<INPUT>() as i32,
            );
            if sent != inputs.len() as u32 {
                Err("Windows did not accept the synthetic paste input".into())
            } else {
                Ok(())
            }
        }
    }

    #[cfg(test)]
    mod tests {
        use super::*;

        #[test]
        fn default_shortcuts_compile_as_sequences() {
            let bindings = compile(&ShortcutSettings::default()).unwrap();
            assert_eq!(bindings.slots.len(), 10);
            assert!(bindings
                .slots
                .iter()
                .flatten()
                .all(|binding| binding.ctrl_v_sequence));
            assert_eq!(bindings.slots[9].as_ref().unwrap().terminal, b'0' as u32);
        }

        #[test]
        fn normal_paste_is_reserved_even_with_aliases() {
            assert!(parse_binding("Control + v")
                .unwrap_err()
                .contains("reserved"));
            assert!(parse_binding("LCTRL+V").unwrap_err().contains("reserved"));
        }

        #[test]
        fn direct_custom_chord_is_supported() {
            let binding = parse_binding("Ctrl+Alt+7").unwrap().unwrap();
            assert!(!binding.ctrl_v_sequence);
            assert_eq!(binding.terminal, b'7' as u32);
        }

        #[test]
        fn explicitly_added_slot_shortcut_is_supported() {
            let mut shortcuts = ShortcutSettings::default();
            shortcuts.slots.push("Ctrl+Shift+F11".into());
            let bindings = compile(&shortcuts).unwrap();
            assert_eq!(bindings.slots.len(), 11);
            assert_eq!(bindings.slots[10].as_ref().unwrap().terminal, 0x7A);
        }

        #[test]
        fn stale_modifiers_cannot_trigger_space_shortcut() {
            let binding = parse_binding("Ctrl+Shift+Space").unwrap().unwrap();
            let mut pressed = HashSet::from([VK_CONTROL as u32, VK_SHIFT as u32, 0x20]);

            sync_modifier_state_with(&mut pressed, |_| false);

            assert!(!binding_matches(&binding, &pressed));
            assert!(pressed.contains(&0x20));
            assert!(!pressed.contains(&(VK_CONTROL as u32)));
            assert!(!pressed.contains(&(VK_SHIFT as u32)));
        }
    }
}

#[cfg(not(windows))]
mod imp {
    use super::{KeyboardAction, Sender, ShortcutSettings};
    pub struct HookController;
    impl HookController {
        pub fn is_active(&self) -> bool {
            false
        }
        pub fn stop(&self) {}
    }
    pub fn start(
        _s: &ShortcutSettings,
        _tx: Sender<KeyboardAction>,
    ) -> Result<HookController, String> {
        Ok(HookController)
    }
    pub fn update(_s: &ShortcutSettings) -> Result<(), String> {
        Ok(())
    }
    pub fn send_paste() -> Result<(), String> {
        Err("keyboard injection is available on Windows only".into())
    }
}

pub use imp::{send_paste, start, update, HookController};
