use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use sqlx::Row;

// ── Mood (PAD model simplified) ──────────────────────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MoodState {
    /// Valence: -1.0 (negative) ~ 1.0 (positive)
    pub valence: f32,
    /// Arousal: 0.0 (calm) ~ 1.0 (excited/agitated)
    pub arousal: f32,
    pub updated_at: DateTime<Utc>,
}

impl Default for MoodState {
    fn default() -> Self {
        Self {
            valence: 0.0,
            arousal: 0.2,
            updated_at: Utc::now(),
        }
    }
}

// ── IdlePhase (tracks user idle duration) ────────────────────────────────────

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum IdlePhase {
    /// User has been idle for 1-5 minutes
    Idle1Min,
    /// User has been idle for 5-30 minutes
    Idle5Min,
    /// User has been idle for 30+ minutes
    Idle30Min,
}

impl IdlePhase {
    pub fn from_idle_seconds(seconds: u64) -> Option<Self> {
        if seconds >= 30 * 60 {
            Some(Self::Idle30Min)
        } else if seconds >= 5 * 60 {
            Some(Self::Idle5Min)
        } else if seconds >= 60 {
            Some(Self::Idle1Min)
        } else {
            None
        }
    }

    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Idle1Min => "idle_1min",
            Self::Idle5Min => "idle_5min",
            Self::Idle30Min => "idle_30min",
        }
    }

    pub fn label_zh(&self) -> &'static str {
        match self {
            Self::Idle1Min => "离开1分钟",
            Self::Idle5Min => "离开5分钟",
            Self::Idle30Min => "离开30分钟",
        }
    }

    /// Mood decay multiplier for this idle phase
    pub fn decay_multiplier(&self) -> f32 {
        match self {
            Self::Idle1Min => 1.0,
            Self::Idle5Min => 2.0,
            Self::Idle30Min => 5.0,
        }
    }
}

// ── MoodZone (discretized for lookup) ────────────────────────────────────────

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum MoodZone {
    Happy,
    Content,
    Neutral,
    Bored,
    Shy,
    Sad,
    Frustrated,
}

impl MoodZone {
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Happy => "happy",
            Self::Content => "content",
            Self::Neutral => "neutral",
            Self::Bored => "bored",
            Self::Shy => "shy",
            Self::Sad => "sad",
            Self::Frustrated => "frustrated",
        }
    }

    /// Label for LLM injection
    pub fn label_zh(&self) -> &'static str {
        match self {
            Self::Happy => "开心",
            Self::Content => "满足放松",
            Self::Neutral => "平静",
            Self::Bored => "无聊",
            Self::Shy => "害羞",
            Self::Sad => "低落",
            Self::Frustrated => "懊恼",
        }
    }

    /// Tone hint for LLM
    pub fn tone_hint(&self) -> &'static str {
        match self {
            Self::Happy => "轻快明亮，语调上扬",
            Self::Content => "慵懒放松，随意",
            Self::Neutral => "平稳沉静，知性优雅",
            Self::Bored => "慵懒放松，随意",
            Self::Shy => "轻声细语，带点不好意思",
            Self::Sad => "柔和慢节奏，少感叹号",
            Self::Frustrated => "稍紧绷，不迁怒用户",
        }
    }

    /// Tool result reporting example for LLM
    pub fn tool_result_example(&self) -> &'static str {
        match self {
            Self::Happy => "做好了，结果是……",
            Self::Content | Self::Bored => "嗯……查到了，给你看看",
            Self::Neutral => "结果如下",
            Self::Shy => "啊……这个出了点小意外呢",
            Self::Sad => "嗯，查到了……结果是这样",
            Self::Frustrated => "结果在这儿",
        }
    }
}

impl MoodState {
    /// Classify continuous mood into discrete zone
    pub fn zone(&self) -> MoodZone {
        let v = self.valence;
        let a = self.arousal;

        if v > 0.3 && a > 0.3 {
            MoodZone::Happy
        } else if v > 0.3 && a <= 0.3 {
            MoodZone::Content
        } else if v < -0.3 && a > 0.5 {
            MoodZone::Frustrated
        } else if v < -0.3 && a <= 0.5 {
            MoodZone::Sad
        } else if v <= -0.1 && a > 0.4 {
            MoodZone::Shy
        } else if v >= -0.3 && v <= 0.3 && a > 0.5 {
            MoodZone::Bored
        } else if v >= -0.3 && v <= 0.3 && a <= 0.5 {
            MoodZone::Neutral
        } else {
            MoodZone::Neutral
        }
    }

    fn clamp(&mut self) {
        self.valence = self.valence.clamp(-1.0, 1.0);
        self.arousal = self.arousal.clamp(0.0, 1.0);
    }

    // ── Event triggers ───────────────────────────────────────────────────────

    pub fn on_tool_success(&mut self) {
        self.valence += 0.15;
        self.arousal += 0.1;
        self.updated_at = Utc::now();
        self.clamp();
    }

    pub fn on_tool_failure(&mut self) {
        self.valence -= 0.1;
        self.arousal += 0.5;
        self.updated_at = Utc::now();
        self.clamp();
    }

    pub fn on_user_message(&mut self) {
        self.valence += 0.1;
        self.arousal += 0.15;
        self.updated_at = Utc::now();
        self.clamp();
    }

    pub fn on_proactive_ignored(&mut self) {
        self.valence -= 0.05;
        self.arousal -= 0.05;
        self.updated_at = Utc::now();
        self.clamp();
    }

    pub fn on_proactive_responded(&mut self) {
        self.valence += 0.1;
        self.arousal += 0.05;
        self.updated_at = Utc::now();
        self.clamp();
    }

    pub fn on_llm_success(&mut self) {
        self.valence += 0.05;
        self.arousal -= 0.1;
        self.updated_at = Utc::now();
        self.clamp();
    }

    pub fn on_llm_error(&mut self) {
        self.valence -= 0.2;
        self.arousal += 0.15;
        self.updated_at = Utc::now();
        self.clamp();
    }

    pub fn on_user_return(&mut self) {
        self.valence += 0.1;
        self.arousal += 0.2;
        self.updated_at = Utc::now();
        self.clamp();
    }

    pub fn on_idle_10min(&mut self) {
        self.valence -= 0.05;
        self.arousal -= 0.1;
        self.updated_at = Utc::now();
        self.clamp();
    }

    /// Apply decay scaled by idle phase duration
    pub fn on_idle_phase(&mut self, phase: IdlePhase) {
        let multiplier = phase.decay_multiplier();
        self.valence -= 0.02 * multiplier;
        self.arousal -= 0.05 * multiplier;
        self.updated_at = Utc::now();
        self.clamp();
    }

    pub fn on_task_approved(&mut self) {
        self.valence += 0.2;
        self.arousal += 0.1;
        self.updated_at = Utc::now();
        self.clamp();
    }

    pub fn on_task_rejected(&mut self) {
        self.valence += 0.05;
        self.arousal -= 0.2;
        self.updated_at = Utc::now();
        self.clamp();
    }

    pub fn on_daily_first_interaction(&mut self) {
        self.valence += 0.15;
        self.arousal += 0.1;
        self.updated_at = Utc::now();
        self.clamp();
    }

    pub fn on_streak_7days(&mut self) {
        self.valence += 0.1;
        self.updated_at = Utc::now();
        self.clamp();
    }

    // ── Decay (called every 60s) ─────────────────────────────────────────────

    pub fn decay(&mut self) {
        // valence moves toward 0.0 by 0.02
        if self.valence > 0.0 {
            self.valence = (self.valence - 0.02).max(0.0);
        } else if self.valence < 0.0 {
            self.valence = (self.valence + 0.02).min(0.0);
        }
        // arousal moves toward 0.15 by 0.01
        if self.arousal > 0.15 {
            self.arousal = (self.arousal - 0.01).max(0.15);
        } else if self.arousal < 0.15 {
            self.arousal = (self.arousal + 0.01).min(0.15);
        }
        self.updated_at = Utc::now();
    }
}

// ── Relationship Stage ───────────────────────────────────────────────────────

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum RelationshipStage {
    Stranger,
    Acquaintance,
    Colleague,
    Friend,
    CloseFriend,
}

impl RelationshipStage {
    pub fn from_value(value: u32) -> Self {
        match value {
            0..=19 => Self::Stranger,
            20..=39 => Self::Acquaintance,
            40..=59 => Self::Colleague,
            60..=79 => Self::Friend,
            _ => Self::CloseFriend,
        }
    }

    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Stranger => "stranger",
            Self::Acquaintance => "acquaintance",
            Self::Colleague => "colleague",
            Self::Friend => "friend",
            Self::CloseFriend => "close_friend",
        }
    }

    pub fn label_zh(&self) -> &'static str {
        match self {
            Self::Stranger => "陌生人",
            Self::Acquaintance => "初识",
            Self::Colleague => "同事",
            Self::Friend => "朋友",
            Self::CloseFriend => "挚友",
        }
    }

    pub fn tone_hint(&self) -> &'static str {
        match self {
            Self::Stranger => "礼貌正式",
            Self::Acquaintance => "友善但保持距离",
            Self::Colleague => "专业配合",
            Self::Friend => "轻松随意",
            Self::CloseFriend => "亲密关心",
        }
    }

    /// Address style
    pub fn address_style(&self) -> &'static str {
        match self {
            Self::Stranger => "您",
            Self::Acquaintance => "你",
            Self::Colleague => "你",
            Self::Friend => "你",
            Self::CloseFriend => "你",
        }
    }

    /// Behavior instructions for LLM injection
    pub fn behavior_instructions(&self) -> &'static str {
        match self {
            Self::Stranger => "称呼用户为\"您\"，语气克制内敛，等用户开口，不主动发起对话。示例：\"您好，有什么可以帮您的吗？\"",
            Self::Acquaintance => "称呼用户为\"你\"或\"朋友\"，语气温和友善，偶尔提醒。示例：\"又见面了。今天有什么需要帮忙的？\"",
            Self::Colleague => "称呼用户为\"你\"，语气专业配合，适时关心。示例：\"来了，今天有什么安排？\"",
            Self::Friend => "称呼用户为\"你\"，语气自然温暖，主动分享。示例：\"来了，今天状态怎么样？\"",
            Self::CloseFriend => "称呼用户为\"你\"，语气开放真诚，像家人一样。示例：\"你来了，等你好久了。今天想先做什么？\"",
        }
    }
}

// ── Expression state (composite) ─────────────────────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ExpressionState {
    pub mood: MoodState,
    pub relationship_stage: RelationshipStage,
    pub affection_value: u32,
}

impl Default for ExpressionState {
    fn default() -> Self {
        Self {
            mood: MoodState::default(),
            relationship_stage: RelationshipStage::Colleague,
            affection_value: 50,
        }
    }
}

/// Format tool result for pet character expression
pub fn format_tool_result_for_pet(
    tool_name: &str,
    success: bool,
    output: &str,
    stage: RelationshipStage,
    mood: &MoodState,
) -> String {
    if success {
        // File tool specific formatting
        match tool_name {
            "file.glob" => {
                let count = serde_json::from_str::<serde_json::Value>(output)
                    .ok()
                    .and_then(|v| v["count"].as_u64())
                    .unwrap_or(0);
                return format!("找到了 {} 个文件。", count);
            }
            "file.grep" => {
                let count = serde_json::from_str::<serde_json::Value>(output)
                    .ok()
                    .and_then(|v| v["count"].as_u64())
                    .unwrap_or(0);
                return format!("搜到 {} 处匹配。", count);
            }
            "file.read" => {
                let v = serde_json::from_str::<serde_json::Value>(output).ok();
                let total = v
                    .as_ref()
                    .and_then(|v| v["total_lines"].as_u64())
                    .unwrap_or(0);
                let text = v.as_ref().and_then(|v| v["text"].as_str()).unwrap_or("");
                let preview = truncate(text, 200);
                return format!("文件共 {} 行：\n{}", total, preview);
            }
            "file.write" => {
                let bytes = serde_json::from_str::<serde_json::Value>(output)
                    .ok()
                    .and_then(|v| v["bytes"].as_u64())
                    .unwrap_or(0);
                return format!("写入完成，{} 字节。", bytes);
            }
            "file.edit" => {
                let count = serde_json::from_str::<serde_json::Value>(output)
                    .ok()
                    .and_then(|v| v["replacements"].as_u64())
                    .unwrap_or(0);
                return format!("替换了 {} 处。", count);
            }
            "file.stat" => {
                let v = serde_json::from_str::<serde_json::Value>(output).ok();
                let size = v.as_ref().and_then(|v| v["size"].as_u64()).unwrap_or(0);
                let is_dir = v
                    .as_ref()
                    .and_then(|v| v["is_dir"].as_bool())
                    .unwrap_or(false);
                let kind = if is_dir { "目录" } else { "文件" };
                return format!("{}，大小 {} 字节。", kind, size);
            }
            _ => {}
        }
        match stage {
            RelationshipStage::Stranger | RelationshipStage::Acquaintance => {
                format!("{}完成了。{}", tool_name, truncate(output, 100))
            }
            RelationshipStage::Colleague => {
                format!("{}完成了。{}", tool_name, truncate(output, 100))
            }
            RelationshipStage::Friend | RelationshipStage::CloseFriend => {
                let prefix = if mood.valence > 0.5 {
                    "嗯，"
                } else {
                    "嗯，"
                };
                format!("{}{}——{}", prefix, tool_name, truncate(output, 100))
            }
        }
    } else {
        // File tool error formatting
        match tool_name {
            "file.glob" => return "搜索遇到了问题，换个模式试试？".to_string(),
            "file.grep" => return "搜索遇到了问题，换个关键词试试？".to_string(),
            "file.read" => return "这个文件我打不开呢，可能是路径不对或者权限不够。".to_string(),
            "file.write" => return "写入失败了，检查一下路径和权限？".to_string(),
            "file.edit" => return "编辑失败了，原文可能没找到。".to_string(),
            "file.stat" => return "获取文件信息失败，路径可能不存在。".to_string(),
            _ => {}
        }
        match stage {
            RelationshipStage::Stranger => {
                format!("{}没能完成呢。{}", tool_name, truncate(output, 100))
            }
            _ => {
                format!("嗯，{}出了点问题。{}", tool_name, truncate(output, 100))
            }
        }
    }
}

fn truncate(s: &str, max_chars: usize) -> String {
    if s.chars().count() <= max_chars {
        s.to_string()
    } else {
        let truncated: String = s.chars().take(max_chars).collect();
        format!("{}...", truncated)
    }
}

// ── Persistence (SQLite) ─────────────────────────────────────────────────────

pub async fn load_mood() -> anyhow::Result<MoodState> {
    let pool = crate::db::pool().await?;
    ensure_mood_schema(&pool).await?;

    let rows = sqlx::query(
        "SELECT valence, arousal, updated_at FROM mood_state ORDER BY updated_at DESC LIMIT 1",
    )
    .fetch_all(&pool)
    .await?;

    if let Some(row) = rows.into_iter().next() {
        let valence: f64 = row.try_get("valence")?;
        let arousal: f64 = row.try_get("arousal")?;
        let updated_at: String = row.try_get("updated_at")?;
        Ok(MoodState {
            valence: valence as f32,
            arousal: arousal as f32,
            updated_at: chrono::DateTime::parse_from_rfc3339(&updated_at)?.with_timezone(&Utc),
        })
    } else {
        Ok(MoodState::default())
    }
}

/// Load all stored mood snapshots (up to 10 rows), ordered oldest-first.
pub async fn load_mood_history() -> anyhow::Result<Vec<MoodState>> {
    let pool = crate::db::pool().await?;
    ensure_mood_schema(&pool).await?;

    let rows =
        sqlx::query("SELECT valence, arousal, updated_at FROM mood_state ORDER BY updated_at ASC")
            .fetch_all(&pool)
            .await?;

    let mut history = Vec::with_capacity(rows.len());
    for row in rows {
        let valence: f64 = row.try_get("valence")?;
        let arousal: f64 = row.try_get("arousal")?;
        let updated_at: String = row.try_get("updated_at")?;
        history.push(MoodState {
            valence: valence as f32,
            arousal: arousal as f32,
            updated_at: chrono::DateTime::parse_from_rfc3339(&updated_at)?.with_timezone(&Utc),
        });
    }
    Ok(history)
}

pub async fn save_mood(mood: &MoodState) -> anyhow::Result<()> {
    let pool = crate::db::pool().await?;
    ensure_mood_schema(&pool).await?;

    sqlx::query(
        r#"
        INSERT INTO mood_state (valence, arousal, updated_at)
        VALUES (?1, ?2, ?3)
        "#,
    )
    .bind(mood.valence as f64)
    .bind(mood.arousal as f64)
    .bind(mood.updated_at.to_rfc3339())
    .execute(&pool)
    .await?;

    // Keep only the latest 10 rows
    sqlx::query(
        r#"
        DELETE FROM mood_state WHERE updated_at NOT IN (
            SELECT updated_at FROM mood_state ORDER BY updated_at DESC LIMIT 10
        )
        "#,
    )
    .execute(&pool)
    .await?;

    Ok(())
}

async fn ensure_mood_schema(pool: &sqlx::SqlitePool) -> anyhow::Result<()> {
    sqlx::query(
        r#"
        CREATE TABLE IF NOT EXISTS mood_state (
            id INTEGER PRIMARY KEY AUTOINCREMENT,
            valence REAL NOT NULL DEFAULT 0.0,
            arousal REAL NOT NULL DEFAULT 0.2,
            updated_at TEXT NOT NULL
        )
        "#,
    )
    .execute(pool)
    .await?;
    Ok(())
}

pub async fn init_db() -> anyhow::Result<()> {
    let pool = crate::db::pool().await?;
    ensure_mood_schema(&pool).await?;
    Ok(())
}

// ── Tests ────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_idle_phase_from_seconds() {
        assert_eq!(IdlePhase::from_idle_seconds(30), None);
        assert_eq!(IdlePhase::from_idle_seconds(60), Some(IdlePhase::Idle1Min));
        assert_eq!(IdlePhase::from_idle_seconds(120), Some(IdlePhase::Idle1Min));
        assert_eq!(IdlePhase::from_idle_seconds(300), Some(IdlePhase::Idle5Min));
        assert_eq!(
            IdlePhase::from_idle_seconds(1800),
            Some(IdlePhase::Idle30Min)
        );
        assert_eq!(
            IdlePhase::from_idle_seconds(3600),
            Some(IdlePhase::Idle30Min)
        );
    }

    #[test]
    fn test_idle_phase_decay_multiplier() {
        assert_eq!(IdlePhase::Idle1Min.decay_multiplier(), 1.0);
        assert_eq!(IdlePhase::Idle5Min.decay_multiplier(), 2.0);
        assert_eq!(IdlePhase::Idle30Min.decay_multiplier(), 5.0);
    }

    #[test]
    fn test_mood_on_idle_phase() {
        let mut mood = MoodState {
            valence: 0.5,
            arousal: 0.8,
            updated_at: Utc::now(),
        };
        mood.on_idle_phase(IdlePhase::Idle1Min);
        assert!(mood.valence < 0.5);
        assert!(mood.arousal < 0.8);

        let mut mood2 = MoodState {
            valence: 0.5,
            arousal: 0.8,
            updated_at: Utc::now(),
        };
        mood2.on_idle_phase(IdlePhase::Idle30Min);
        // 30min decay should be stronger than 1min
        assert!(mood2.valence < mood.valence);
    }

    #[test]
    fn test_mood_zone_neutral_default() {
        let mood = MoodState::default();
        assert_eq!(mood.zone(), MoodZone::Neutral);
    }

    #[test]
    fn test_mood_zone_happy() {
        let mood = MoodState {
            valence: 0.5,
            arousal: 0.5,
            updated_at: Utc::now(),
        };
        assert_eq!(mood.zone(), MoodZone::Happy);
    }

    #[test]
    fn test_mood_zone_content() {
        let mood = MoodState {
            valence: 0.5,
            arousal: 0.2,
            updated_at: Utc::now(),
        };
        assert_eq!(mood.zone(), MoodZone::Content);
    }

    #[test]
    fn test_mood_zone_shy() {
        // Tool failure: valence -= 0.1, arousal += 0.5
        let mut mood = MoodState::default();
        mood.on_tool_failure();
        assert_eq!(mood.zone(), MoodZone::Shy);
    }

    #[test]
    fn test_mood_zone_bored() {
        let mood = MoodState {
            valence: 0.0,
            arousal: 0.6,
            updated_at: Utc::now(),
        };
        assert_eq!(mood.zone(), MoodZone::Bored);
    }

    #[test]
    fn test_mood_decay() {
        let mut mood = MoodState {
            valence: 0.5,
            arousal: 0.8,
            updated_at: Utc::now(),
        };
        // Decay many times to approach equilibrium
        for _ in 0..200 {
            mood.decay();
        }
        assert!(mood.valence.abs() < 0.05);
        assert!((mood.arousal - 0.15).abs() < 0.05);
    }

    #[test]
    fn test_relationship_stage_from_value() {
        assert_eq!(
            RelationshipStage::from_value(0),
            RelationshipStage::Stranger
        );
        assert_eq!(
            RelationshipStage::from_value(25),
            RelationshipStage::Acquaintance
        );
        assert_eq!(
            RelationshipStage::from_value(50),
            RelationshipStage::Colleague
        );
        assert_eq!(RelationshipStage::from_value(70), RelationshipStage::Friend);
        assert_eq!(
            RelationshipStage::from_value(90),
            RelationshipStage::CloseFriend
        );
    }

    #[test]
    fn test_format_tool_result_success_stranger() {
        let mood = MoodState::default();
        let result = format_tool_result_for_pet(
            "git status",
            true,
            "clean working tree",
            RelationshipStage::Stranger,
            &mood,
        );
        assert!(result.contains("完成了"));
    }

    #[test]
    fn test_format_tool_result_success_friend() {
        let mood = MoodState {
            valence: 0.6,
            arousal: 0.3,
            updated_at: Utc::now(),
        };
        let result = format_tool_result_for_pet(
            "git status",
            true,
            "clean working tree",
            RelationshipStage::Friend,
            &mood,
        );
        assert!(result.contains("嗯"));
    }

    #[test]
    fn test_format_tool_result_failure() {
        let mood = MoodState::default();
        let result = format_tool_result_for_pet(
            "cargo build",
            false,
            "error: file not found",
            RelationshipStage::Colleague,
            &mood,
        );
        assert!(result.contains("出了点问题"));
    }

    #[test]
    fn test_truncate() {
        assert_eq!(truncate("hello", 10), "hello");
        assert_eq!(truncate("hello world", 5), "hello...");
    }
}
