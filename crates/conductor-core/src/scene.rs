use crate::paths::Paths;
use anyhow::Context;
use chrono::{DateTime, Datelike, Local, Timelike, Utc};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use tokio::{
    fs,
    io::{AsyncWriteExt, BufWriter},
};

/// Generate scene context tags from the current local time.
///
/// Returns tags for:
/// - Time of day: "morning" (5-11), "afternoon" (12-16), "evening" (17-20), "night" (21-4)
/// - Day type: "weekday" (Mon-Fri) or "weekend" (Sat-Sun)
///
/// Foreground app and task context tags can be layered on top later.
pub fn generate_scene_tags() -> Vec<String> {
    generate_scene_tags_at(Local::now())
}

/// Inner implementation that accepts a fixed datetime for deterministic testing.
fn generate_scene_tags_at(now: DateTime<Local>) -> Vec<String> {
    let mut tags = Vec::with_capacity(2);

    let hour = now.hour();
    let time_tag = match hour {
        5..=11 => "morning",
        12..=16 => "afternoon",
        17..=20 => "evening",
        _ => "night", // 21-23, 0-4
    };
    tags.push(time_tag.to_string());

    let weekday = now.weekday();
    let day_tag = if weekday == chrono::Weekday::Sat || weekday == chrono::Weekday::Sun {
        "weekend"
    } else {
        "weekday"
    };
    tags.push(day_tag.to_string());

    tags
}

/// Semantic scene tags derived from foreground app, task status, idle time, and clock.
///
/// Multiple tags can be active simultaneously (e.g. `CodingFocus` + `LateNight`).
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub enum SceneTag {
    /// IDE or code editor is in the foreground.
    CodingFocus,
    /// Document editor (Word, PPT, Markdown, etc.) is in the foreground.
    DocumentWork,
    /// Task management or whiteboard tool is in the foreground.
    Planning,
    /// Task status contains error/test/fail/debug keywords.
    Debugging,
    /// Short idle: 1-5 minutes without input.
    IdleShort,
    /// Long idle: 30+ minutes without input.
    IdleLong,
    /// Active during 23:00-06:00 window.
    LateNight,
}

impl std::fmt::Display for SceneTag {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            SceneTag::CodingFocus => write!(f, "coding_focus"),
            SceneTag::DocumentWork => write!(f, "document_work"),
            SceneTag::Planning => write!(f, "planning"),
            SceneTag::Debugging => write!(f, "debugging"),
            SceneTag::IdleShort => write!(f, "idle_short"),
            SceneTag::IdleLong => write!(f, "idle_long"),
            SceneTag::LateNight => write!(f, "late_night"),
        }
    }
}

/// Inputs used to derive semantic scene tags.
pub struct SceneInput {
    /// Name of the foreground application (case-insensitive matching).
    pub foreground_app: Option<String>,
    /// Current workspace or project name.
    pub workspace: Option<String>,
    /// Free-text task status (e.g. "running tests", "build error").
    pub task_status: Option<String>,
    /// Seconds since last user input.
    pub idle_seconds: u64,
    /// Current hour of day (0-23).
    pub current_hour: u32,
}

/// Derive all applicable [`SceneTag`]s from the given [`SceneInput`].
///
/// Multiple tags can be active at the same time.
pub fn derive_scene_tags(input: SceneInput) -> Vec<SceneTag> {
    let mut tags = Vec::new();

    // -- foreground app matching (case-insensitive) --------------------------------
    if let Some(ref app) = input.foreground_app {
        let app_lower = app.to_lowercase();

        if contains_any(&app_lower, &["code", "idea", "vim", "emacs", "vscode"]) {
            tags.push(SceneTag::CodingFocus);
        }
        if contains_any(
            &app_lower,
            &["word", "ppt", "markdown", "notion", "obsidian"],
        ) {
            tags.push(SceneTag::DocumentWork);
        }
        if contains_any(
            &app_lower,
            &["trello", "notion", "jira", "asana", "whiteboard"],
        ) {
            tags.push(SceneTag::Planning);
        }
    }

    // -- task status matching ------------------------------------------------------
    if let Some(ref status) = input.task_status {
        let status_lower = status.to_lowercase();
        if contains_any(&status_lower, &["error", "test", "fail", "debug"]) {
            tags.push(SceneTag::Debugging);
        }
    }

    // -- idle time -----------------------------------------------------------------
    if (60..300).contains(&input.idle_seconds) {
        tags.push(SceneTag::IdleShort);
    }
    if input.idle_seconds >= 1800 {
        tags.push(SceneTag::IdleLong);
    }

    // -- late night ----------------------------------------------------------------
    if input.current_hour >= 23 || input.current_hour < 6 {
        tags.push(SceneTag::LateNight);
    }

    tags
}

/// Returns `true` if `haystack` contains any of the given `needles`.
fn contains_any(haystack: &str, needles: &[&str]) -> bool {
    needles.iter().any(|n| haystack.contains(n))
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
pub enum SceneType {
    Default,
    Morning,
    Afternoon,
    Evening,
    Night,
    Music,
    Work,
    Relax,
    Custom(String),
}

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct Scene {
    pub id: String,
    pub name: String,
    pub scene_type: SceneType,
    pub background_color: String,
    pub background_image: Option<String>,
    pub ambient_sound: Option<String>,
    pub description: String,
    pub available_time: Option<(u32, u32)>,
    pub transitions: Vec<String>,
    pub created_at: DateTime<Utc>,
}

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct SceneState {
    pub current_scene: String,
    pub previous_scene: Option<String>,
    pub scene_history: Vec<(String, DateTime<Utc>)>,
    pub auto_switch_enabled: bool,
}

impl Default for SceneState {
    fn default() -> Self {
        Self {
            current_scene: "default".to_string(),
            previous_scene: None,
            scene_history: Vec::new(),
            auto_switch_enabled: true,
        }
    }
}

pub struct SceneManager {
    scenes: HashMap<String, Scene>,
    state: SceneState,
}

impl SceneManager {
    pub fn new() -> Self {
        Self::with_state(SceneState::default())
    }

    pub fn with_state(state: SceneState) -> Self {
        let mut scenes = HashMap::new();
        for scene in get_default_scenes() {
            scenes.insert(scene.id.clone(), scene);
        }

        let state = normalize_state(state, &scenes);

        Self { scenes, state }
    }

    pub fn get_scene(&self, id: &str) -> Option<&Scene> {
        self.scenes.get(id)
    }

    pub fn get_current_scene(&self) -> Option<&Scene> {
        self.scenes.get(&self.state.current_scene)
    }

    pub fn switch_scene(&mut self, scene_id: &str) -> bool {
        if self.scenes.contains_key(scene_id) {
            self.state.previous_scene = Some(self.state.current_scene.clone());
            self.state.current_scene = scene_id.to_string();
            self.state
                .scene_history
                .push((scene_id.to_string(), Utc::now()));

            if self.state.scene_history.len() > 10 {
                self.state.scene_history.remove(0);
            }

            true
        } else {
            false
        }
    }

    pub fn get_state(&self) -> &SceneState {
        &self.state
    }

    pub fn enable_auto_switch(&mut self, enabled: bool) {
        self.state.auto_switch_enabled = enabled;
    }

    pub fn auto_switch_based_on_time(&mut self) -> Option<&Scene> {
        if !self.state.auto_switch_enabled {
            return None;
        }

        let hour = Utc::now().hour();
        let target_scene = match hour {
            5..=8 => "morning",
            9..=11 => "afternoon",
            12..=17 => "afternoon",
            18..=20 => "evening",
            21..=23 | 0..=4 => "night",
            _ => "default",
        };

        if self.switch_scene(target_scene) {
            self.get_current_scene()
        } else {
            None
        }
    }

    pub fn list_scenes(&self) -> Vec<&Scene> {
        self.scenes.values().collect()
    }

    pub fn add_scene(&mut self, scene: Scene) -> bool {
        if self.scenes.contains_key(&scene.id) {
            false
        } else {
            self.scenes.insert(scene.id.clone(), scene);
            true
        }
    }

    pub fn remove_scene(&mut self, id: &str) -> bool {
        if id == "default" {
            false
        } else {
            self.scenes.remove(id).is_some()
        }
    }
}

pub async fn load_state() -> anyhow::Result<SceneState> {
    let path = Paths::scene_state_json();
    let state = match fs::read_to_string(&path).await {
        Ok(content) if content.trim().is_empty() => SceneState::default(),
        Ok(content) => {
            serde_json::from_str(&content).with_context(|| format!("parse {}", path.display()))?
        }
        Err(err) if err.kind() == std::io::ErrorKind::NotFound => SceneState::default(),
        Err(err) => return Err(err).with_context(|| format!("read {}", path.display())),
    };

    let manager = SceneManager::with_state(state);
    Ok(manager.state.clone())
}

pub async fn save_state(state: &SceneState) -> anyhow::Result<SceneState> {
    let manager = SceneManager::with_state(state.clone());
    let state = manager.state.clone();
    let path = Paths::scene_state_json();
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).await?;
    }
    let content = serde_json::to_string_pretty(&state)?;
    let mut writer = BufWriter::new(fs::File::create(&path).await?);
    writer.write_all(content.as_bytes()).await?;
    writer.write_all(b"\n").await?;
    writer.flush().await?;
    writer.get_ref().sync_all().await?;
    Ok(state)
}

pub async fn load_manager() -> anyhow::Result<SceneManager> {
    Ok(SceneManager::with_state(load_state().await?))
}

fn normalize_state(mut state: SceneState, scenes: &HashMap<String, Scene>) -> SceneState {
    if !scenes.contains_key(&state.current_scene) {
        state.current_scene = "default".to_string();
    }

    if state
        .previous_scene
        .as_ref()
        .is_some_and(|previous| !scenes.contains_key(previous))
    {
        state.previous_scene = None;
    }

    state
        .scene_history
        .retain(|(scene_id, _)| scenes.contains_key(scene_id));
    if state.scene_history.len() > 10 {
        let drain_count = state.scene_history.len() - 10;
        state.scene_history.drain(0..drain_count);
    }

    state
}

pub fn get_default_scenes() -> Vec<Scene> {
    vec![
        Scene {
            id: "default".to_string(),
            name: "默认场景".to_string(),
            scene_type: SceneType::Default,
            background_color: "#1a1a2e".to_string(),
            background_image: None,
            ambient_sound: None,
            description: "温馨的默认场景".to_string(),
            available_time: None,
            transitions: vec![
                "morning".to_string(),
                "afternoon".to_string(),
                "evening".to_string(),
                "night".to_string(),
            ],
            created_at: Utc::now(),
        },
        Scene {
            id: "morning".to_string(),
            name: "早晨".to_string(),
            scene_type: SceneType::Morning,
            background_color: "#ffd93d".to_string(),
            background_image: None,
            ambient_sound: Some("birds_chirping".to_string()),
            description: "阳光明媚的早晨".to_string(),
            available_time: Some((5, 9)),
            transitions: vec!["afternoon".to_string()],
            created_at: Utc::now(),
        },
        Scene {
            id: "afternoon".to_string(),
            name: "下午".to_string(),
            scene_type: SceneType::Afternoon,
            background_color: "#87ceeb".to_string(),
            background_image: None,
            ambient_sound: None,
            description: "晴朗的下午".to_string(),
            available_time: Some((9, 18)),
            transitions: vec!["evening".to_string()],
            created_at: Utc::now(),
        },
        Scene {
            id: "evening".to_string(),
            name: "傍晚".to_string(),
            scene_type: SceneType::Evening,
            background_color: "#ffa502".to_string(),
            background_image: None,
            ambient_sound: Some("crickets".to_string()),
            description: "温暖的傍晚".to_string(),
            available_time: Some((18, 21)),
            transitions: vec!["night".to_string()],
            created_at: Utc::now(),
        },
        Scene {
            id: "night".to_string(),
            name: "夜晚".to_string(),
            scene_type: SceneType::Night,
            background_color: "#0d0d1a".to_string(),
            background_image: None,
            ambient_sound: Some("rain".to_string()),
            description: "宁静的夜晚".to_string(),
            available_time: Some((21, 5)),
            transitions: vec!["morning".to_string()],
            created_at: Utc::now(),
        },
        Scene {
            id: "music".to_string(),
            name: "音乐时光".to_string(),
            scene_type: SceneType::Music,
            background_color: "#4a1942".to_string(),
            background_image: None,
            ambient_sound: None,
            description: "与音乐相伴".to_string(),
            available_time: None,
            transitions: vec!["default".to_string()],
            created_at: Utc::now(),
        },
        Scene {
            id: "work".to_string(),
            name: "工作模式".to_string(),
            scene_type: SceneType::Work,
            background_color: "#1e3a5f".to_string(),
            background_image: None,
            ambient_sound: None,
            description: "专注工作的氛围".to_string(),
            available_time: None,
            transitions: vec!["relax".to_string(), "default".to_string()],
            created_at: Utc::now(),
        },
        Scene {
            id: "relax".to_string(),
            name: "放松时刻".to_string(),
            scene_type: SceneType::Relax,
            background_color: "#2d5a3d".to_string(),
            background_image: None,
            ambient_sound: Some("forest".to_string()),
            description: "放松身心的场景".to_string(),
            available_time: None,
            transitions: vec!["default".to_string()],
            created_at: Utc::now(),
        },
    ]
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{paths::Paths, test_support::TestRoot};
    use chrono::TimeZone;

    #[test]
    fn test_scene_manager() {
        let mut manager = SceneManager::new();
        assert!(manager.get_current_scene().is_some());
        assert!(manager.switch_scene("morning"));
        assert_eq!(manager.get_state().current_scene, "morning");
    }

    #[test]
    fn test_default_scenes() {
        let scenes = get_default_scenes();
        assert!(scenes.len() > 0);
        assert!(scenes.iter().any(|s| s.id == "default"));
    }

    #[test]
    fn test_auto_switch_disabled() {
        let mut manager = SceneManager::new();
        manager.enable_auto_switch(false);
        assert!(manager.auto_switch_based_on_time().is_none());
    }

    #[test]
    fn test_scene_history() {
        let mut manager = SceneManager::new();
        manager.switch_scene("morning");
        manager.switch_scene("afternoon");
        assert_eq!(manager.get_state().scene_history.len(), 2);
    }

    #[tokio::test]
    async fn test_scene_state_persists() {
        let _root = TestRoot::new();
        let mut manager = load_manager().await.expect("load manager");

        assert!(manager.switch_scene("work"));
        save_state(manager.get_state()).await.expect("save state");

        let loaded = load_manager().await.expect("reload manager");
        assert_eq!(loaded.get_state().current_scene, "work");
        assert!(fs::try_exists(Paths::scene_state_json())
            .await
            .expect("exists"));
    }

    // ── generate_scene_tags tests ──────────────────────────────────────────

    #[test]
    fn test_generate_scene_tags_morning() {
        // 08:00 on a Wednesday
        let dt = chrono::NaiveDateTime::parse_from_str("2026-05-27 08:00:00", "%Y-%m-%d %H:%M:%S")
            .unwrap();
        let local = chrono::Local.from_local_datetime(&dt).unwrap();
        let tags = generate_scene_tags_at(local);
        assert!(tags.contains(&"morning".to_string()));
        assert!(tags.contains(&"weekday".to_string()));
    }

    #[test]
    fn test_generate_scene_tags_afternoon() {
        // 14:00 on a Monday
        let dt = chrono::NaiveDateTime::parse_from_str("2026-05-25 14:00:00", "%Y-%m-%d %H:%M:%S")
            .unwrap();
        let local = chrono::Local.from_local_datetime(&dt).unwrap();
        let tags = generate_scene_tags_at(local);
        assert!(tags.contains(&"afternoon".to_string()));
        assert!(tags.contains(&"weekday".to_string()));
    }

    #[test]
    fn test_generate_scene_tags_evening() {
        // 19:00 on a Friday
        let dt = chrono::NaiveDateTime::parse_from_str("2026-05-29 19:00:00", "%Y-%m-%d %H:%M:%S")
            .unwrap();
        let local = chrono::Local.from_local_datetime(&dt).unwrap();
        let tags = generate_scene_tags_at(local);
        assert!(tags.contains(&"evening".to_string()));
        assert!(tags.contains(&"weekday".to_string()));
    }

    #[test]
    fn test_generate_scene_tags_night() {
        // 23:00 on a Saturday
        let dt = chrono::NaiveDateTime::parse_from_str("2026-05-30 23:00:00", "%Y-%m-%d %H:%M:%S")
            .unwrap();
        let local = chrono::Local.from_local_datetime(&dt).unwrap();
        let tags = generate_scene_tags_at(local);
        assert!(tags.contains(&"night".to_string()));
        assert!(tags.contains(&"weekend".to_string()));
    }

    #[test]
    fn test_generate_scene_tags_early_morning_night() {
        // 02:00 on a Sunday — should be "night" (not morning)
        let dt = chrono::NaiveDateTime::parse_from_str("2026-05-31 02:00:00", "%Y-%m-%d %H:%M:%S")
            .unwrap();
        let local = chrono::Local.from_local_datetime(&dt).unwrap();
        let tags = generate_scene_tags_at(local);
        assert!(tags.contains(&"night".to_string()));
        assert!(tags.contains(&"weekend".to_string()));
    }

    #[test]
    fn test_generate_scene_tags_returns_exactly_two() {
        let tags = generate_scene_tags();
        assert_eq!(tags.len(), 2);
    }

    // ── derive_scene_tags tests ─────────────────────────────────────────────

    #[test]
    fn derive_tags_coding_focus() {
        let input = SceneInput {
            foreground_app: Some("Visual Studio Code".into()),
            workspace: None,
            task_status: None,
            idle_seconds: 0,
            current_hour: 10,
        };
        let tags = derive_scene_tags(input);
        assert!(tags.contains(&SceneTag::CodingFocus));
        assert_eq!(tags.len(), 1);
    }

    #[test]
    fn derive_tags_document_work() {
        let input = SceneInput {
            foreground_app: Some("Notion".into()),
            workspace: None,
            task_status: None,
            idle_seconds: 0,
            current_hour: 14,
        };
        let tags = derive_scene_tags(input);
        assert!(tags.contains(&SceneTag::DocumentWork));
        // "notion" also matches Planning
        assert!(tags.contains(&SceneTag::Planning));
    }

    #[test]
    fn derive_tags_planning() {
        let input = SceneInput {
            foreground_app: Some("Jira".into()),
            workspace: None,
            task_status: None,
            idle_seconds: 0,
            current_hour: 11,
        };
        let tags = derive_scene_tags(input);
        assert!(tags.contains(&SceneTag::Planning));
        assert_eq!(tags.len(), 1);
    }

    #[test]
    fn derive_tags_debugging() {
        let input = SceneInput {
            foreground_app: None,
            workspace: None,
            task_status: Some("build error in main.rs".into()),
            idle_seconds: 0,
            current_hour: 15,
        };
        let tags = derive_scene_tags(input);
        assert!(tags.contains(&SceneTag::Debugging));
        assert_eq!(tags.len(), 1);
    }

    #[test]
    fn derive_tags_idle_short() {
        let input = SceneInput {
            foreground_app: None,
            workspace: None,
            task_status: None,
            idle_seconds: 120, // 2 minutes
            current_hour: 10,
        };
        let tags = derive_scene_tags(input);
        assert!(tags.contains(&SceneTag::IdleShort));
        assert!(!tags.contains(&SceneTag::IdleLong));
    }

    #[test]
    fn derive_tags_idle_long() {
        let input = SceneInput {
            foreground_app: None,
            workspace: None,
            task_status: None,
            idle_seconds: 3600, // 1 hour
            current_hour: 10,
        };
        let tags = derive_scene_tags(input);
        assert!(tags.contains(&SceneTag::IdleLong));
        assert!(!tags.contains(&SceneTag::IdleShort));
    }

    #[test]
    fn derive_tags_late_night() {
        let input = SceneInput {
            foreground_app: None,
            workspace: None,
            task_status: None,
            idle_seconds: 0,
            current_hour: 2, // 2 AM
        };
        let tags = derive_scene_tags(input);
        assert!(tags.contains(&SceneTag::LateNight));
        assert_eq!(tags.len(), 1);

        let input_23 = SceneInput {
            foreground_app: None,
            workspace: None,
            task_status: None,
            idle_seconds: 0,
            current_hour: 23,
        };
        let tags_23 = derive_scene_tags(input_23);
        assert!(tags_23.contains(&SceneTag::LateNight));
    }

    #[test]
    fn derive_tags_combined() {
        let input = SceneInput {
            foreground_app: Some("VSCode".into()),
            workspace: Some("my-project".into()),
            task_status: Some("running debug tests".into()),
            idle_seconds: 0,
            current_hour: 1, // 1 AM — late night
        };
        let tags = derive_scene_tags(input);
        assert!(tags.contains(&SceneTag::CodingFocus));
        assert!(tags.contains(&SceneTag::Debugging));
        assert!(tags.contains(&SceneTag::LateNight));
    }

    #[test]
    fn derive_tags_empty_input() {
        let input = SceneInput {
            foreground_app: None,
            workspace: None,
            task_status: None,
            idle_seconds: 10, // not idle enough for any idle tag
            current_hour: 12,
        };
        let tags = derive_scene_tags(input);
        assert!(tags.is_empty());
    }

    #[test]
    fn scene_tag_display() {
        assert_eq!(SceneTag::CodingFocus.to_string(), "coding_focus");
        assert_eq!(SceneTag::DocumentWork.to_string(), "document_work");
        assert_eq!(SceneTag::Planning.to_string(), "planning");
        assert_eq!(SceneTag::Debugging.to_string(), "debugging");
        assert_eq!(SceneTag::IdleShort.to_string(), "idle_short");
        assert_eq!(SceneTag::IdleLong.to_string(), "idle_long");
        assert_eq!(SceneTag::LateNight.to_string(), "late_night");
    }
}
