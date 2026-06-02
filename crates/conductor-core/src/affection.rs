use crate::db;
use chrono::{DateTime, Days, Local, NaiveDate, Utc};
use serde::{Deserialize, Serialize};
use sqlx::Row;

// ── Types ────────────────────────────────────────────────────────────────────

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum InteractionType {
    Chat,
    ToolSuccess,
    ToolFailure,
    TaskApproved,
    TaskRejected,
    ProactiveResponded,
    ProactiveIgnored,
    DailyReturn,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AffectionState {
    pub value: u32,
    pub stage: crate::expression::RelationshipStage,
    pub last_interaction_at: DateTime<Utc>,
    pub interaction_count: u32,
    pub consecutive_days: u32,
    pub last_daily_date: Option<NaiveDate>,
}

impl Default for AffectionState {
    fn default() -> Self {
        Self {
            value: 50,
            stage: crate::expression::RelationshipStage::Colleague,
            last_interaction_at: Utc::now(),
            interaction_count: 0,
            consecutive_days: 1,
            last_daily_date: None,
        }
    }
}

// ── Stage hysteresis ─────────────────────────────────────────────────────────

fn stage_from_value_with_hysteresis(
    new_value: u32,
    current_stage: crate::expression::RelationshipStage,
) -> crate::expression::RelationshipStage {
    use crate::expression::RelationshipStage::*;

    let (low, high): (u32, u32) = match current_stage {
        Stranger => (0, 19),
        Acquaintance => (20, 39),
        Colleague => (40, 59),
        Friend => (60, 79),
        CloseFriend => (80, 100),
    };

    // Upgrade: must exceed upper bound + 3
    if new_value > high + 3 {
        return crate::expression::RelationshipStage::from_value(new_value);
    }
    // Downgrade: must fall at or below lower bound - 3
    if new_value <= low.saturating_sub(3) {
        return crate::expression::RelationshipStage::from_value(new_value);
    }
    // Otherwise stay in current stage
    current_stage
}

// ── Core logic ───────────────────────────────────────────────────────────────

impl AffectionState {
    pub fn record(&mut self, interaction: InteractionType) {
        let delta: i32 = match interaction {
            InteractionType::Chat => 1,
            InteractionType::ToolSuccess => 1,
            InteractionType::ToolFailure => 0, // don't penalize for tool errors
            InteractionType::TaskApproved => 2,
            InteractionType::TaskRejected => -2,
            InteractionType::ProactiveResponded => 1,
            InteractionType::ProactiveIgnored => -1,
            InteractionType::DailyReturn => {
                // Consecutive day bonus (cap at 7)
                let bonus = self.consecutive_days.min(7) as i32;
                bonus
            }
        };

        let new_value = (self.value as i32 + delta).clamp(0, 100) as u32;
        self.value = new_value;
        self.stage = stage_from_value_with_hysteresis(new_value, self.stage);
        self.last_interaction_at = Utc::now();
        self.interaction_count += 1;

        // Daily return tracking
        let today = Local::now().date_naive();
        if self.last_daily_date != Some(today) {
            if let Some(last_date) = self.last_daily_date {
                if last_date + Days::new(1) == today {
                    self.consecutive_days += 1;
                } else {
                    self.consecutive_days = 1;
                }
            } else {
                self.consecutive_days = 1;
            }
            self.last_daily_date = Some(today);
        }
    }

    /// Apply daily decay for inactivity (3+ days no interaction)
    pub fn apply_daily_decay(&mut self) {
        let now = Utc::now();
        let days_since = (now - self.last_interaction_at).num_days();
        if days_since >= 3 {
            let decay_days = (days_since - 2) as i32; // first 2 days free
            let decay = decay_days.min(self.value as i32);
            self.value = self.value.saturating_sub(decay as u32);
            self.stage = stage_from_value_with_hysteresis(self.value, self.stage);
        }
    }

    /// Stage protection: don't drop below current stage minimum - 5
    pub fn stage_protect(&mut self) {
        use crate::expression::RelationshipStage::*;
        let min_allowed = match self.stage {
            Stranger => 0,
            Acquaintance => 15, // 20-5
            Colleague => 35,    // 40-5
            Friend => 55,       // 60-5
            CloseFriend => 75,  // 80-5
        };
        if self.value < min_allowed {
            self.value = min_allowed;
        }
    }
}

// ── SQLite persistence ───────────────────────────────────────────────────────

pub async fn load() -> anyhow::Result<AffectionState> {
    let pool = db::pool().await?;

    let rows = sqlx::query(
        r#"
        SELECT value, last_interaction_at, interaction_count, consecutive_days, last_daily_date
        FROM affection_state
        WHERE id = 1
        "#,
    )
    .fetch_all(&pool)
    .await?;

    if let Some(row) = rows.into_iter().next() {
        let value: i64 = row.try_get("value")?;
        let last_str: String = row.try_get("last_interaction_at")?;
        let last_at = DateTime::parse_from_rfc3339(&last_str)
            .map(|dt| dt.with_timezone(&Utc))
            .unwrap_or_else(|_| Utc::now());
        let count: i64 = row.try_get("interaction_count")?;
        let days: i64 = row.try_get("consecutive_days")?;
        let last_daily: Option<String> = row.try_get("last_daily_date").ok();
        let last_daily_date = last_daily
            .as_deref()
            .and_then(|s| NaiveDate::parse_from_str(s, "%Y-%m-%d").ok());

        let mut state = AffectionState {
            value: value as u32,
            stage: crate::expression::RelationshipStage::from_value(value as u32),
            last_interaction_at: last_at,
            interaction_count: count as u32,
            consecutive_days: days as u32,
            last_daily_date,
        };
        state.apply_daily_decay();
        Ok(state)
    } else {
        Ok(AffectionState::default())
    }
}

pub async fn save(state: &AffectionState) -> anyhow::Result<()> {
    let pool = db::pool().await?;

    sqlx::query(
        r#"
        INSERT INTO affection_state (id, value, last_interaction_at, interaction_count, consecutive_days, last_daily_date, updated_at)
        VALUES (1, ?1, ?2, ?3, ?4, ?5, ?6)
        ON CONFLICT(id) DO UPDATE SET
            value = excluded.value,
            last_interaction_at = excluded.last_interaction_at,
            interaction_count = excluded.interaction_count,
            consecutive_days = excluded.consecutive_days,
            last_daily_date = excluded.last_daily_date,
            updated_at = excluded.updated_at
        "#,
    )
    .bind(state.value as i64)
    .bind(state.last_interaction_at.to_rfc3339())
    .bind(state.interaction_count as i64)
    .bind(state.consecutive_days as i64)
    .bind(state.last_daily_date.map(|d| d.to_string()))
    .bind(Utc::now().to_rfc3339())
    .execute(&pool)
    .await?;

    Ok(())
}

pub async fn add(value: i32) -> anyhow::Result<AffectionState> {
    let mut state = load().await?;
    state.value = (state.value as i32 + value).clamp(0, 100) as u32;
    state.stage = crate::expression::RelationshipStage::from_value(state.value);
    save(&state).await?;
    Ok(state)
}

pub async fn record_interaction(interaction: InteractionType) -> anyhow::Result<AffectionState> {
    let mut state = load().await?;
    state.record(interaction);
    state.stage_protect();
    save(&state).await?;
    Ok(state)
}

pub async fn interact() -> anyhow::Result<AffectionState> {
    record_interaction(InteractionType::Chat).await
}

pub async fn decrease_over_time() -> anyhow::Result<AffectionState> {
    let mut state = load().await?;
    state.apply_daily_decay();
    save(&state).await?;
    Ok(state)
}

pub async fn on_music_playing() -> anyhow::Result<AffectionState> {
    let mut state = load().await?;
    // Small bonus for shared music experience
    state.value = (state.value + 1).min(100);
    state.stage = crate::expression::RelationshipStage::from_value(state.value);
    state.last_interaction_at = Utc::now();
    save(&state).await?;
    Ok(state)
}

pub async fn init_db() -> anyhow::Result<()> {
    let pool = db::pool().await?;

    // Migrate: add columns if missing (do this FIRST, before any inserts)
    let rows = sqlx::query("PRAGMA table_info(affection_state);")
        .fetch_all(&pool)
        .await?;
    let col_names: Vec<String> = rows
        .iter()
        .filter_map(|r| r.try_get::<String, _>("name").ok())
        .collect();

    if !col_names.contains(&"interaction_count".to_string()) {
        sqlx::query(
            "ALTER TABLE affection_state ADD COLUMN interaction_count INTEGER NOT NULL DEFAULT 0;",
        )
        .execute(&pool)
        .await?;
    }
    if !col_names.contains(&"consecutive_days".to_string()) {
        sqlx::query(
            "ALTER TABLE affection_state ADD COLUMN consecutive_days INTEGER NOT NULL DEFAULT 1;",
        )
        .execute(&pool)
        .await?;
    }
    if !col_names.contains(&"last_daily_date".to_string()) {
        sqlx::query("ALTER TABLE affection_state ADD COLUMN last_daily_date TEXT;")
            .execute(&pool)
            .await?;
    }
    if !col_names.contains(&"updated_at".to_string()) {
        sqlx::query(
            "ALTER TABLE affection_state ADD COLUMN updated_at TEXT DEFAULT CURRENT_TIMESTAMP;",
        )
        .execute(&pool)
        .await?;
    }

    // Ensure default row exists
    sqlx::query(
        r#"
        INSERT INTO affection_state (id, value, last_interaction_at, interaction_count, consecutive_days)
        VALUES (1, 50, ?1, 0, 1)
        ON CONFLICT(id) DO NOTHING
        "#,
    )
    .bind(Utc::now().to_rfc3339())
    .execute(&pool)
    .await?;

    Ok(())
}

// ── Tests ────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use crate::test_support::TestRoot;

    #[tokio::test]
    async fn test_load_default() {
        let _root = TestRoot::new();
        init_db().await.expect("init db");

        let state = load().await.expect("load");
        assert_eq!(state.value, 50);
    }

    #[tokio::test]
    async fn test_record_chat() {
        let _root = TestRoot::new();
        init_db().await.expect("init db");

        let state = record_interaction(InteractionType::Chat)
            .await
            .expect("record");
        assert_eq!(state.value, 51);
        assert_eq!(state.interaction_count, 1);
    }

    #[tokio::test]
    async fn test_record_tool_success() {
        let _root = TestRoot::new();
        init_db().await.expect("init db");

        let state = record_interaction(InteractionType::ToolSuccess)
            .await
            .expect("record");
        assert_eq!(state.value, 51);
    }

    #[tokio::test]
    async fn test_record_tool_failure_no_penalty() {
        let _root = TestRoot::new();
        init_db().await.expect("init db");

        let state = record_interaction(InteractionType::ToolFailure)
            .await
            .expect("record");
        assert_eq!(state.value, 50); // no change
    }

    #[tokio::test]
    async fn test_stage_hysteresis() {
        let _root = TestRoot::new();
        init_db().await.expect("init db");

        // Set value to 22 (Acquaintance range)
        let mut state = AffectionState::default();
        state.value = 22;
        state.stage = crate::expression::RelationshipStage::Acquaintance;
        save(&state).await.expect("save");

        // Value goes to 20 - should stay Acquaintance (not drop to Stranger)
        state.value = 20;
        state.stage = stage_from_value_with_hysteresis(20, state.stage);
        assert_eq!(
            state.stage,
            crate::expression::RelationshipStage::Acquaintance
        );

        // Value goes to 17 - should drop to Stranger
        state.stage = stage_from_value_with_hysteresis(17, state.stage);
        assert_eq!(state.stage, crate::expression::RelationshipStage::Stranger);
    }

    #[tokio::test]
    async fn test_stage_protect() {
        let _root = TestRoot::new();
        init_db().await.expect("init db");

        let mut state = AffectionState::default();
        state.value = 50;
        state.stage = crate::expression::RelationshipStage::Colleague;

        // Try to reduce below Colleague minimum - 5 = 35
        state.value = 30;
        state.stage_protect();
        assert_eq!(state.value, 35);
    }

    #[tokio::test]
    async fn test_persistence_round_trip() {
        let _root = TestRoot::new();
        init_db().await.expect("init db");

        let state = record_interaction(InteractionType::TaskApproved)
            .await
            .expect("record");
        assert_eq!(state.value, 52);

        let loaded = load().await.expect("load");
        assert_eq!(loaded.value, 52);
        assert_eq!(loaded.interaction_count, 1);
    }

    #[tokio::test]
    async fn test_add_positive() {
        let _root = TestRoot::new();
        init_db().await.expect("init db");

        let state = add(10).await.expect("add");
        assert_eq!(state.value, 60);
    }

    #[tokio::test]
    async fn test_add_negative() {
        let _root = TestRoot::new();
        init_db().await.expect("init db");

        let state = add(-20).await.expect("add");
        assert_eq!(state.value, 30);
    }

    #[test]
    fn test_relationship_stage_from_value() {
        use crate::expression::RelationshipStage;
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
}
