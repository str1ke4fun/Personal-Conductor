use serde::{Deserialize, Serialize};
use tauri::{Manager, PhysicalPosition, PhysicalSize};

const DEFAULT_WIDTH: u32 = 320;
const DEFAULT_HEIGHT: u32 = 420;

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct DesktopState {
    #[serde(default)]
    pub pet: PetWindowState,
}

impl Default for DesktopState {
    fn default() -> Self {
        Self {
            pet: PetWindowState::default(),
        }
    }
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PetWindowState {
    pub x: Option<i32>,
    pub y: Option<i32>,
    pub width: u32,
    pub height: u32,
    pub scale: f64,
    pub locked: bool,
}

impl Default for PetWindowState {
    fn default() -> Self {
        Self {
            x: None,
            y: None,
            width: DEFAULT_WIDTH,
            height: DEFAULT_HEIGHT,
            scale: 1.0,
            locked: false,
        }
    }
}

fn desktop_state_path() -> std::path::PathBuf {
    conductor_core::paths::state().join("desktop.json")
}

async fn read_desktop_state() -> DesktopState {
    let path = desktop_state_path();
    match tokio::fs::read_to_string(path).await {
        Ok(contents) => serde_json::from_str(&contents).unwrap_or_default(),
        Err(_) => DesktopState::default(),
    }
}

async fn write_desktop_state(state: &DesktopState) -> Result<(), String> {
    let path = desktop_state_path();
    if let Some(parent) = path.parent() {
        tokio::fs::create_dir_all(parent)
            .await
            .map_err(|err| err.to_string())?;
    }
    let contents = serde_json::to_string_pretty(state).map_err(|err| err.to_string())?;
    tokio::fs::write(path, contents)
        .await
        .map_err(|err| err.to_string())
}

#[tauri::command]
pub async fn load_pet_window_state() -> Result<PetWindowState, String> {
    Ok(read_desktop_state().await.pet)
}

#[tauri::command]
pub async fn save_pet_window_state(pet: PetWindowState) -> Result<(), String> {
    let mut state = read_desktop_state().await;
    state.pet = pet;
    write_desktop_state(&state).await
}

pub fn apply_pet_window_state(app: &tauri::AppHandle) {
    let app = app.clone();
    tauri::async_runtime::spawn(async move {
        let state = read_desktop_state().await.pet;
        let Some(pet) = app.get_webview_window("pet") else {
            return;
        };
        let width = state.width.clamp(240, 800);
        let height = state.height.clamp(315, 1040);
        let _ = pet.set_size(PhysicalSize::new(width, height));

        let mut x = state.x;
        let mut y = state.y;
        if let Ok(Some(monitor)) = pet.current_monitor() {
            let pos = monitor.position();
            let size = monitor.size();
            let min_x = pos.x;
            let min_y = pos.y;
            let max_x = pos.x + size.width as i32 - width as i32;
            let max_y = pos.y + size.height as i32 - height as i32;
            x = Some(x.unwrap_or(max_x - 24).clamp(min_x, max_x.max(min_x)));
            y = Some(y.unwrap_or(max_y - 24).clamp(min_y, max_y.max(min_y)));
        }

        if let (Some(x), Some(y)) = (x, y) {
            let _ = pet.set_position(PhysicalPosition::new(x, y));
        }
        let _ = pet.show();
        let _ = pet.set_focus();
    });
}
