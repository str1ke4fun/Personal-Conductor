use anyhow::Context;
use std::path::PathBuf;

#[cfg(windows)]
use windows::Win32::{
    Foundation::{CloseHandle, HWND, RECT},
    System::{
        ProcessStatus::GetModuleFileNameExW,
        Threading::{OpenProcess, PROCESS_QUERY_INFORMATION, PROCESS_VM_READ},
    },
    UI::{
        WindowsAndMessaging::GetWindowLongW,
        WindowsAndMessaging::{
            GetForegroundWindow, GetWindowRect, GetWindowTextLengthW, GetWindowTextW,
            GetWindowThreadProcessId, IsWindowVisible, WS_EX_LAYERED,
        },
    },
};

#[cfg(windows)]
use windows::Win32::Graphics::Gdi::{MonitorFromWindow, MONITOR_DEFAULTTONEAREST};

#[derive(Clone, Debug)]
pub struct WindowPosition {
    pub x: i32,
    pub y: i32,
    pub width: i32,
    pub height: i32,
    pub monitor_index: i32,
}

#[cfg(windows)]
pub fn get_window_rect(hwnd: isize) -> anyhow::Result<WindowPosition> {
    let hwnd = HWND(hwnd as *mut _);
    let mut rect = RECT::default();
    unsafe {
        let hmon = MonitorFromWindow(hwnd, MONITOR_DEFAULTTONEAREST);
        let monitor_info = get_monitor_index(hmon);
        if GetWindowRect(hwnd, &mut rect).is_ok() {
            Ok(WindowPosition {
                x: rect.left,
                y: rect.top,
                width: rect.right - rect.left,
                height: rect.bottom - rect.top,
                monitor_index: monitor_info,
            })
        } else {
            anyhow::bail!("GetWindowRect failed")
        }
    }
}

#[cfg(windows)]
fn get_monitor_index(_hmon: windows::Win32::Graphics::Gdi::HMONITOR) -> i32 {
    0
}

#[cfg(not(windows))]
pub fn get_window_rect(_hwnd: isize) -> anyhow::Result<WindowPosition> {
    anyhow::bail!("get_window_rect is only implemented on Windows")
}

pub fn is_on_primary_monitor(pos: &WindowPosition) -> bool {
    pos.monitor_index == 0
}

#[cfg(windows)]
pub fn is_window_visible(hwnd: isize) -> bool {
    let hwnd = HWND(hwnd as *mut _);
    unsafe { IsWindowVisible(hwnd).as_bool() }
}

#[cfg(not(windows))]
pub fn is_window_visible(_hwnd: isize) -> bool {
    false
}

#[cfg(windows)]
pub fn is_transparent_window(hwnd: isize) -> bool {
    let hwnd = HWND(hwnd as *mut _);
    unsafe {
        let ex_style = GetWindowLongW(hwnd, windows::Win32::UI::WindowsAndMessaging::GWL_EXSTYLE);
        (ex_style & (WS_EX_LAYERED.0 as i32)) != 0
    }
}

#[cfg(not(windows))]
pub fn is_transparent_window(_hwnd: isize) -> bool {
    false
}

#[cfg(windows)]
pub fn is_maximized(hwnd: isize) -> bool {
    let hwnd = HWND(hwnd as *mut _);
    unsafe {
        let state = windows::Win32::UI::WindowsAndMessaging::GetWindowLongW(
            hwnd,
            windows::Win32::UI::WindowsAndMessaging::GWL_STYLE,
        );
        (state & windows::Win32::UI::WindowsAndMessaging::WS_MAXIMIZE.0 as i32) != 0
    }
}

#[cfg(not(windows))]
pub fn is_maximized(_hwnd: isize) -> bool {
    false
}

#[cfg(windows)]
pub fn is_minimized(hwnd: isize) -> bool {
    let hwnd = HWND(hwnd as *mut _);
    unsafe {
        let state = windows::Win32::UI::WindowsAndMessaging::GetWindowLongW(
            hwnd,
            windows::Win32::UI::WindowsAndMessaging::GWL_STYLE,
        );
        (state & windows::Win32::UI::WindowsAndMessaging::WS_MINIMIZE.0 as i32) != 0
    }
}

#[cfg(not(windows))]
pub fn is_minimized(_hwnd: isize) -> bool {
    false
}

#[derive(Clone, Debug, serde::Serialize, serde::Deserialize)]
pub struct WindowInfo {
    pub hwnd: isize,
    pub title: String,
    pub process_name: String,
    pub process_path: Option<PathBuf>,
}

#[cfg(windows)]
pub fn foreground_window_info() -> anyhow::Result<WindowInfo> {
    let hwnd = unsafe { GetForegroundWindow() };
    window_info(hwnd)
}

#[cfg(not(windows))]
pub fn foreground_window_info() -> anyhow::Result<WindowInfo> {
    anyhow::bail!("foreground window info is only implemented on Windows")
}

#[cfg(windows)]
pub fn window_info(hwnd: HWND) -> anyhow::Result<WindowInfo> {
    let title = unsafe {
        let len = GetWindowTextLengthW(hwnd);
        let mut buf = vec![0u16; (len + 1).max(1) as usize];
        let copied = GetWindowTextW(hwnd, &mut buf);
        String::from_utf16_lossy(&buf[..copied as usize])
    };

    let mut pid = 0u32;
    unsafe {
        GetWindowThreadProcessId(hwnd, Some(&mut pid));
    }
    let process_path = process_path(pid).ok();
    let process_name = process_path
        .as_ref()
        .and_then(|path| path.file_name())
        .and_then(|name| name.to_str())
        .unwrap_or("")
        .to_string();

    Ok(WindowInfo {
        hwnd: hwnd.0 as isize,
        title,
        process_name,
        process_path,
    })
}

#[cfg(windows)]
fn process_path(pid: u32) -> anyhow::Result<PathBuf> {
    unsafe {
        let handle = OpenProcess(PROCESS_QUERY_INFORMATION | PROCESS_VM_READ, false, pid)
            .context("OpenProcess")?;
        let mut buf = vec![0u16; 32768];
        let len = GetModuleFileNameExW(Some(handle), None, &mut buf);
        let _ = CloseHandle(handle);
        if len == 0 {
            anyhow::bail!("GetModuleFileNameExW returned 0");
        }
        Ok(PathBuf::from(String::from_utf16_lossy(
            &buf[..len as usize],
        )))
    }
}

pub fn sanitize_title(title: &str) -> String {
    let mut result = String::with_capacity(title.len().min(200));
    let mut in_password_field = false;
    let chars: Vec<char> = title.chars().collect();
    let mut i = 0;

    while i < chars.len() && result.len() < 200 {
        let c = chars[i];
        if c == '*' && i > 0 && chars.get(i - 1) == Some(&'*') {
            i += 1;
            continue;
        }

        if c.is_control() {
            i += 1;
            continue;
        }

        if c == '*' && !in_password_field {
            let next_stars = chars[i..].iter().take(3).filter(|&&x| x == '*').count();
            if next_stars >= 3 {
                in_password_field = true;
                result.push_str("[密码]");
                i += next_stars;
                continue;
            }
        }

        if c.is_ascii_digit() && i > 0 && i < chars.len() - 3 {
            let chunk: String = chars[i..i + 4].iter().collect();
            if chunk.chars().filter(|x| x.is_ascii_digit()).count() >= 4
                && (i == 0 || !chars[i - 1].is_ascii_digit())
            {
                result.push_str("[号码]");
                i += 4;
                continue;
            }
        }

        result.push(c);
        i += 1;
    }

    result
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_sanitize_title_normal() {
        let title = "Visual Studio Code - main.rs";
        let result = sanitize_title(title);
        assert_eq!(result, "Visual Studio Code - main.rs");
    }

    #[test]
    fn test_sanitize_title_password() {
        let title = "Login - Please enter password: *****";
        let result = sanitize_title(title);
        assert!(result.contains("[密码]"));
        assert!(!result.contains("*"));
    }

    #[test]
    fn test_sanitize_title_phone_number() {
        let title = "Contact: 1234567890";
        let result = sanitize_title(title);
        assert!(result.contains("[号码]"));
    }

    #[test]
    fn test_sanitize_title_max_length() {
        let title = "a".repeat(300);
        let result = sanitize_title(&title);
        assert_eq!(result.len(), 200);
    }

    #[test]
    fn test_sanitize_title_preserves_normal_chars() {
        let title = "Chrome - https://example.com - Tab 1";
        let result = sanitize_title(title);
        assert_eq!(result, "Chrome - https://example.com - Tab 1");
    }

    #[test]
    fn test_window_position_creation() {
        let pos = WindowPosition {
            x: 0,
            y: 0,
            width: 1920,
            height: 1080,
            monitor_index: 0,
        };
        assert!(is_on_primary_monitor(&pos));
    }

    #[test]
    fn test_window_position_secondary_monitor() {
        let pos = WindowPosition {
            x: 1920,
            y: 0,
            width: 1920,
            height: 1080,
            monitor_index: 1,
        };
        assert!(!is_on_primary_monitor(&pos));
    }
}
