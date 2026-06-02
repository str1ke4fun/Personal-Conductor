use crate::{whitelist, window_title};
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::{
    path::PathBuf,
    sync::{
        atomic::{AtomicBool, Ordering},
        Arc,
    },
    thread,
    time::Duration,
};
use tokio::sync::mpsc::UnboundedSender;

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct FocusEvent {
    pub ts: DateTime<Utc>,
    pub hwnd: isize,
    pub title: String,
    pub process_name: String,
    pub process_path: Option<PathBuf>,
    pub monitor_index: i32,
    pub is_maximized: bool,
    pub is_minimized: bool,
}

pub struct FocusWatcherHandle {
    stop: Arc<AtomicBool>,
}

impl FocusWatcherHandle {
    pub fn stop(&self) {
        self.stop.store(true, Ordering::SeqCst);
    }
}

pub fn spawn_focus_watcher(tx: UnboundedSender<FocusEvent>) -> anyhow::Result<FocusWatcherHandle> {
    let stop = Arc::new(AtomicBool::new(false));
    let stop_thread = stop.clone();
    thread::Builder::new()
        .name("conductor-focus-watcher".into())
        .spawn(move || {
            let mut last_hwnd = 0isize;
            while !stop_thread.load(Ordering::SeqCst) {
                if let Ok(info) = window_title::foreground_window_info() {
                    if info.hwnd != last_hwnd
                        && whitelist::is_interesting(&info.process_name, &info.title)
                        && window_title::is_window_visible(info.hwnd)
                        && !window_title::is_transparent_window(info.hwnd)
                    {
                        last_hwnd = info.hwnd;
                        thread::sleep(Duration::from_secs(2));
                        if let Ok(stable) = window_title::foreground_window_info() {
                            if stable.hwnd == last_hwnd
                                && window_title::is_window_visible(stable.hwnd)
                                && !window_title::is_transparent_window(stable.hwnd)
                            {
                                let monitor_index = window_title::get_window_rect(stable.hwnd)
                                    .map(|pos| pos.monitor_index)
                                    .unwrap_or(-1);
                                let is_maximized = window_title::is_maximized(stable.hwnd);
                                let is_minimized = window_title::is_minimized(stable.hwnd);
                                let sanitized_title = window_title::sanitize_title(&stable.title);

                                let _ = tx.send(FocusEvent {
                                    ts: Utc::now(),
                                    hwnd: stable.hwnd,
                                    title: sanitized_title,
                                    process_name: stable.process_name,
                                    process_path: stable.process_path,
                                    monitor_index,
                                    is_maximized,
                                    is_minimized,
                                });
                            }
                        }
                    }
                }
                thread::sleep(Duration::from_millis(500));
            }
        })?;
    Ok(FocusWatcherHandle { stop })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_focus_event_serialization() {
        let event = FocusEvent {
            ts: chrono::Utc::now(),
            hwnd: 12345,
            title: "Test Window".to_string(),
            process_name: "test.exe".to_string(),
            process_path: Some(PathBuf::from("C:\\test\\test.exe")),
            monitor_index: 0,
            is_maximized: false,
            is_minimized: false,
        };

        let json = serde_json::to_string(&event).expect("Failed to serialize");
        assert!(json.contains("\"hwnd\":12345"));
        assert!(json.contains("\"monitor_index\":0"));
        assert!(json.contains("\"is_maximized\":false"));
        assert!(json.contains("\"is_minimized\":false"));
    }

    #[test]
    fn test_focus_event_clone() {
        let event = FocusEvent {
            ts: chrono::Utc::now(),
            hwnd: 12345,
            title: "Test Window".to_string(),
            process_name: "test.exe".to_string(),
            process_path: None,
            monitor_index: 1,
            is_maximized: true,
            is_minimized: false,
        };

        let cloned = event.clone();
        assert_eq!(cloned.hwnd, event.hwnd);
        assert_eq!(cloned.monitor_index, event.monitor_index);
        assert_eq!(cloned.is_maximized, event.is_maximized);
    }

    #[test]
    fn test_focus_event_debug() {
        let event = FocusEvent {
            ts: chrono::Utc::now(),
            hwnd: 12345,
            title: "Test Window".to_string(),
            process_name: "test.exe".to_string(),
            process_path: None,
            monitor_index: 0,
            is_maximized: false,
            is_minimized: true,
        };

        let debug_str = format!("{:?}", event);
        assert!(debug_str.contains("hwnd"));
        assert!(debug_str.contains("monitor_index"));
        assert!(debug_str.contains("is_maximized"));
        assert!(debug_str.contains("is_minimized"));
    }

    #[test]
    fn test_focus_watcher_handle_stop() {
        let handle = FocusWatcherHandle {
            stop: Arc::new(AtomicBool::new(false)),
        };
        handle.stop();
        assert!(handle.stop.load(Ordering::SeqCst));
    }
}
