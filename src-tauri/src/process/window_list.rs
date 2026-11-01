use crate::store::config::same_game_path;
use once_cell::sync::Lazy;
use parking_lot::Mutex;
use serde::{Deserialize, Serialize};
use std::collections::{HashMap, HashSet};
use std::path::Path;
use windows_sys::Win32::Foundation::{BOOL, CloseHandle, FALSE, HWND, LPARAM, MAX_PATH, TRUE};
use windows_sys::Win32::System::Threading::{
    IsWow64Process, OpenProcess, PROCESS_QUERY_LIMITED_INFORMATION, QueryFullProcessImageNameW,
};
use windows_sys::Win32::UI::WindowsAndMessaging::{
    EnumWindows, GetWindowLongW, GetWindowTextLengthW, GetWindowTextW, GetWindowThreadProcessId,
    GWL_EXSTYLE, IsWindowVisible, WS_EX_TOOLWINDOW,
};

static ICON_CACHE: Lazy<Mutex<HashMap<String, Option<String>>>> =
    Lazy::new(|| Mutex::new(HashMap::new()));

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProcessEntry {
    pub pid: u32,
    pub name: String,
    pub title: String,
    pub exe_path: String,
    pub is_64bit: bool,
    pub is_saved: bool,
    pub is_knife: bool,
    pub icon_data_url: Option<String>,
}

struct EnumContext {
    entries: Vec<(u32, String)>,
}

unsafe extern "system" fn enum_windows_proc(hwnd: HWND, lparam: LPARAM) -> BOOL {
    unsafe {
        if IsWindowVisible(hwnd) == FALSE {
            return TRUE;
        }
        let ex_style = GetWindowLongW(hwnd, GWL_EXSTYLE) as u32;
        if (ex_style & WS_EX_TOOLWINDOW) != 0 {
            return TRUE;
        }
        let len = GetWindowTextLengthW(hwnd);
        if len <= 0 {
            return TRUE;
        }
        let mut buf = vec![0u16; (len + 1) as usize];
        let copied = GetWindowTextW(hwnd, buf.as_mut_ptr(), len + 1);
        if copied <= 0 {
            return TRUE;
        }
        let title = String::from_utf16_lossy(&buf[..copied as usize])
            .trim()
            .to_string();
        if title.is_empty()
            || title == "Program Manager"
            || title == "Windows Input Experience"
            || title == "Settings"
            || title == "MSCTFIME UI"
            || title == "Default IME"
        {
            return TRUE;
        }

        let mut pid: u32 = 0;
        GetWindowThreadProcessId(hwnd, &mut pid);
        if pid != 0 && pid != std::process::id() {
            let ctx = &mut *(lparam as *mut EnumContext);
            ctx.entries.push((pid, title));
        }
        TRUE
    }
}

pub fn query_process_info(pid: u32) -> Option<(String, bool)> {
    unsafe {
        let handle = OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION, FALSE, pid);
        if handle.is_null() {
            return None;
        }
        let mut buf = vec![0u16; MAX_PATH as usize * 8];
        let mut size = buf.len() as u32;
        let ok = QueryFullProcessImageNameW(handle, 0, buf.as_mut_ptr(), &mut size);
        let path = if ok != FALSE && size > 0 {
            Some(String::from_utf16_lossy(&buf[..size as usize]))
        } else {
            None
        };

        let mut wow64: BOOL = FALSE;
        let wow_ok = IsWow64Process(handle, &mut wow64);
        // On 64-bit Windows, a 32-bit process runs under WOW64 (wow64 == TRUE)
        let is_64bit = if wow_ok != FALSE {
            wow64 == FALSE
        } else {
            true
        };

        CloseHandle(handle);
        path.map(|p| (p, is_64bit))
    }
}

pub fn get_icon_cached(exe_path: &str) -> Option<String> {
    if exe_path.is_empty() {
        return None;
    }
    {
        let cache = ICON_CACHE.lock();
        if let Some(cached) = cache.get(exe_path) {
            return cached.clone();
        }
    }
    let icon = std::panic::catch_unwind(|| windows_icons::get_icon_base64_by_path(exe_path))
        .ok()
        .filter(|b64| !b64.is_empty())
        .map(|b64| {
            if b64.starts_with("data:image") {
                b64
            } else {
                format!("data:image/png;base64,{}", b64)
            }
        });
    ICON_CACHE.lock().insert(exe_path.to_string(), icon.clone());
    icon
}

pub fn list_windowed_processes(saved_exes: &[String]) -> Vec<ProcessEntry> {
    let mut ctx = EnumContext {
        entries: Vec::new(),
    };
    unsafe {
        EnumWindows(
            Some(enum_windows_proc),
            (&mut ctx as *mut EnumContext) as LPARAM,
        );
    }

    let mut seen_pids = HashSet::new();
    let mut results = Vec::new();

    for (pid, title) in ctx.entries {
        if !seen_pids.insert(pid) {
            continue;
        }
        let Some((exe_path, is_64bit)) = query_process_info(pid) else {
            continue;
        };
        let exe_lower = exe_path.to_lowercase();
        if exe_lower.contains("\\windows\\system32\\")
            || exe_lower.contains("\\windows\\systemapps\\")
            || exe_lower.contains("\\windows\\explorer.exe")
            || exe_lower.ends_with("msedgewebview2.exe")
        {
            continue;
        }

        let name = Path::new(&exe_path)
            .file_name()
            .and_then(|s| s.to_str())
            .unwrap_or("Unknown.exe")
            .to_string();
        let name_lower = name.to_lowercase();
        let is_saved = saved_exes.iter().any(|saved| same_game_path(saved, &exe_path));
        let is_knife = name_lower == "knife.exe";
        let icon_data_url = get_icon_cached(&exe_path);

        results.push(ProcessEntry {
            pid,
            name,
            title,
            exe_path,
            is_64bit,
            is_saved,
            is_knife,
            icon_data_url,
        });
    }

    // Sort: Saved games & Knife first, then alphabetically by title
    results.sort_by(|a, b| {
        b.is_saved
            .cmp(&a.is_saved)
            .then_with(|| b.is_knife.cmp(&a.is_knife))
            .then_with(|| a.title.to_lowercase().cmp(&b.title.to_lowercase()))
    });

    results
}
