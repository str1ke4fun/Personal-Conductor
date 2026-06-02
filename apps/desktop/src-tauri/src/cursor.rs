use serde::Serialize;
use tauri::{AppHandle, Emitter};
use tokio::time::{self, Duration};

#[derive(Clone, Copy, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct CursorPosition {
    x: i32,
    y: i32,
}

pub fn spawn_cursor_watcher(app: AppHandle) {
    tauri::async_runtime::spawn(async move {
        let mut last: Option<CursorPosition> = None;
        let mut interval = time::interval(Duration::from_millis(33));

        loop {
            interval.tick().await;
            let Some(position) = current_cursor_position() else {
                continue;
            };
            if last.is_some_and(|prev| prev.x == position.x && prev.y == position.y) {
                continue;
            }
            last = Some(position);
            let _ = app.emit("cursor_position", position);
        }
    });
}

#[cfg(windows)]
fn current_cursor_position() -> Option<CursorPosition> {
    use windows::Win32::Foundation::POINT;
    use windows::Win32::UI::WindowsAndMessaging::GetCursorPos;

    let mut point = POINT::default();
    unsafe { GetCursorPos(&mut point).ok()? };
    Some(CursorPosition {
        x: point.x,
        y: point.y,
    })
}

#[cfg(not(windows))]
fn current_cursor_position() -> Option<CursorPosition> {
    None
}
