#[cfg(windows)]
mod imp {
    use std::{os::windows::ffi::OsStrExt, path::Path};

    use windows_sys::Win32::{
        Foundation::CloseHandle,
        System::Threading::{
            OpenProcess, QueryFullProcessImageNameW, PROCESS_QUERY_LIMITED_INFORMATION,
        },
        UI::Shell::ShellExecuteW,
        UI::WindowsAndMessaging::{
            GetForegroundWindow, GetWindowThreadProcessId, SetForegroundWindow, SW_SHOWNORMAL,
        },
    };
    use winreg::{enums::HKEY_CURRENT_USER, RegKey};

    pub fn foreground_application() -> Option<(isize, String)> {
        unsafe {
            let window = GetForegroundWindow();
            if window.is_null() {
                return None;
            }
            let mut pid = 0u32;
            GetWindowThreadProcessId(window, &mut pid);
            if pid == 0 {
                return None;
            }
            let process = OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION, 0, pid);
            if process.is_null() {
                return Some((window as isize, format!("pid-{pid}")));
            }
            let mut buffer = vec![0u16; 32_768];
            let mut length = buffer.len() as u32;
            let ok = QueryFullProcessImageNameW(process, 0, buffer.as_mut_ptr(), &mut length) != 0;
            CloseHandle(process);
            if !ok {
                return Some((window as isize, format!("pid-{pid}")));
            }
            let path = String::from_utf16_lossy(&buffer[..length as usize]);
            Some((window as isize, path))
        }
    }

    pub fn source_name(path: &str) -> String {
        Path::new(path)
            .file_name()
            .and_then(|v| v.to_str())
            .unwrap_or(path)
            .to_string()
    }

    pub fn activate_window(handle: isize) -> bool {
        handle != 0 && unsafe { SetForegroundWindow(handle as *mut _) != 0 }
    }

    pub fn open_file(path: &Path) -> Result<(), String> {
        let operation: Vec<u16> = "open\0".encode_utf16().collect();
        let file: Vec<u16> = path.as_os_str().encode_wide().chain(Some(0)).collect();
        let result = unsafe {
            ShellExecuteW(
                std::ptr::null_mut(),
                operation.as_ptr(),
                file.as_ptr(),
                std::ptr::null(),
                std::ptr::null(),
                SW_SHOWNORMAL,
            )
        };
        if result as isize <= 32 {
            Err(format!("Windows could not open {}", path.display()))
        } else {
            Ok(())
        }
    }

    pub fn set_launch_at_startup(enabled: bool) -> Result<(), String> {
        let key = RegKey::predef(HKEY_CURRENT_USER)
            .open_subkey_with_flags(
                "Software\\Microsoft\\Windows\\CurrentVersion\\Run",
                winreg::enums::KEY_WRITE,
            )
            .map_err(|e| format!("open Windows startup registry key: {e}"))?;
        if enabled {
            let executable =
                std::env::current_exe().map_err(|e| format!("find application executable: {e}"))?;
            key.set_value("CopyAPP", &format!("\"{}\"", executable.display()))
                .map_err(|e| format!("enable launch at startup: {e}"))
        } else {
            match key.delete_value("CopyAPP") {
                Ok(()) => Ok(()),
                Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(()),
                Err(e) => Err(format!("disable launch at startup: {e}")),
            }
        }
    }

    pub fn is_excluded(application_path: &str, exclusions: &[String]) -> bool {
        let path = application_path.to_lowercase();
        let name = source_name(application_path).to_lowercase();
        exclusions.iter().any(|rule| {
            let rule = rule.trim().to_lowercase();
            if rule.is_empty() {
                return false;
            }
            let trimmed = rule.trim_matches('*');
            if rule.starts_with('*') || rule.ends_with('*') {
                path.contains(trimmed) || name.contains(trimmed)
            } else {
                path == rule
                    || name == rule
                    || name.trim_end_matches(".exe") == rule.trim_end_matches(".exe")
            }
        })
    }
}

#[cfg(not(windows))]
mod imp {
    use std::path::Path;

    pub fn foreground_application() -> Option<(isize, String)> {
        None
    }
    pub fn source_name(path: &str) -> String {
        path.to_string()
    }
    pub fn activate_window(_handle: isize) -> bool {
        false
    }
    pub fn open_file(_path: &Path) -> Result<(), String> {
        Err("opening the text history file is available on Windows only".into())
    }
    pub fn set_launch_at_startup(enabled: bool) -> Result<(), String> {
        if enabled {
            Err("launch at startup is available on Windows only".into())
        } else {
            Ok(())
        }
    }
    pub fn is_excluded(application_path: &str, exclusions: &[String]) -> bool {
        exclusions
            .iter()
            .any(|v| v.eq_ignore_ascii_case(application_path))
    }
}

pub use imp::*;
