use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::{
    sync::{
        atomic::{AtomicBool, Ordering},
        Arc,
    },
    time::Duration,
};
use tokio::sync::mpsc::UnboundedSender;

#[cfg(windows)]
use windows::Win32::UI::Input::KeyboardAndMouse::{GetLastInputInfo, LASTINPUTINFO};

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum IdleEvent {
    IdleStarted {
        ts: DateTime<Utc>,
        since_seconds: u64,
    },
    IdleEnded {
        ts: DateTime<Utc>,
        duration_seconds: u64,
    },
}

pub async fn current_idle_seconds() -> u64 {
    current_idle_seconds_sync().unwrap_or(0)
}

#[cfg(windows)]
fn current_idle_seconds_sync() -> anyhow::Result<u64> {
    let mut info = LASTINPUTINFO {
        cbSize: std::mem::size_of::<LASTINPUTINFO>() as u32,
        dwTime: 0,
    };
    let ok = unsafe { GetLastInputInfo(&mut info) };
    if !ok.as_bool() {
        anyhow::bail!("GetLastInputInfo failed");
    }
    let now_ms = unsafe { windows::Win32::System::SystemInformation::GetTickCount64() };
    Ok(now_ms.saturating_sub(info.dwTime as u64) / 1000)
}

#[cfg(not(windows))]
fn current_idle_seconds_sync() -> anyhow::Result<u64> {
    Ok(0)
}

pub struct IdleWatcherHandle {
    stop: Arc<AtomicBool>,
}

impl IdleWatcherHandle {
    pub fn stop(&self) {
        self.stop.store(true, Ordering::SeqCst);
    }
}

pub fn spawn_idle_watcher(
    threshold_seconds: u64,
    tx: UnboundedSender<IdleEvent>,
) -> IdleWatcherHandle {
    let stop = Arc::new(AtomicBool::new(false));
    let stop_task = stop.clone();
    tokio::spawn(async move {
        let mut is_idle = false;
        let mut idle_started_at: Option<DateTime<Utc>> = None;
        while !stop_task.load(Ordering::SeqCst) {
            let idle = current_idle_seconds().await;
            if !is_idle && idle >= threshold_seconds {
                is_idle = true;
                idle_started_at = Some(Utc::now());
                let _ = tx.send(IdleEvent::IdleStarted {
                    ts: Utc::now(),
                    since_seconds: idle,
                });
            } else if is_idle && idle < threshold_seconds {
                is_idle = false;
                let duration_seconds = idle_started_at
                    .map(|ts| Utc::now().signed_duration_since(ts).num_seconds().max(0) as u64)
                    .unwrap_or(idle);
                let _ = tx.send(IdleEvent::IdleEnded {
                    ts: Utc::now(),
                    duration_seconds,
                });
                idle_started_at = None;
            }
            tokio::time::sleep(Duration::from_secs(30)).await;
        }
    });
    IdleWatcherHandle { stop }
}
