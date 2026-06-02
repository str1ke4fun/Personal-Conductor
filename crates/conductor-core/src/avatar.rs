use crate::db;
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use sqlx::Row;

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum AvatarId {
    Original,
    DocumentSecretary,
    Programmer,
}

impl AvatarId {
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Original => "original",
            Self::DocumentSecretary => "document_secretary",
            Self::Programmer => "programmer",
        }
    }

    pub fn from_str(value: &str) -> anyhow::Result<Self> {
        match value {
            "original" => Ok(Self::Original),
            "document_secretary" => Ok(Self::DocumentSecretary),
            "programmer" => Ok(Self::Programmer),
            other => Err(anyhow::anyhow!("unknown avatar_id: {}", other)),
        }
    }

    fn from_db_str(value: &str) -> anyhow::Result<Self> {
        match value {
            // Legacy theme values from the old cute/cool/professional/cartoon/minimal model.
            "cute" | "cool" | "professional" | "cartoon" | "minimal" => Ok(Self::Original),
            other => Self::from_str(other),
        }
    }
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum ActivityVariant {
    Idle,
    Thinking,
    Reading,
    Writing,
    ToolCalling,
    AgentLeading,
    WaitingUser,
    Error,
    Done,
}

impl ActivityVariant {
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Idle => "idle",
            Self::Thinking => "thinking",
            Self::Reading => "reading",
            Self::Writing => "writing",
            Self::ToolCalling => "tool_calling",
            Self::AgentLeading => "agent_leading",
            Self::WaitingUser => "waiting_user",
            Self::Error => "error",
            Self::Done => "done",
        }
    }

    pub fn from_str(value: &str) -> anyhow::Result<Self> {
        match value {
            "idle" => Ok(Self::Idle),
            "thinking" => Ok(Self::Thinking),
            "reading" => Ok(Self::Reading),
            "writing" => Ok(Self::Writing),
            "tool_calling" => Ok(Self::ToolCalling),
            "agent_leading" => Ok(Self::AgentLeading),
            "waiting_user" => Ok(Self::WaitingUser),
            "error" => Ok(Self::Error),
            "done" => Ok(Self::Done),
            other => Err(anyhow::anyhow!("unknown activity_variant: {}", other)),
        }
    }
}

impl Default for ActivityVariant {
    fn default() -> Self {
        Self::Idle
    }
}

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct AvatarState {
    pub id: String,
    pub avatar_id: AvatarId,
    pub character: String,
    pub color_scheme: String,
    pub size: u32,
    pub position_x: i32,
    pub position_y: i32,
    pub animation_enabled: bool,
    pub auto_hide: bool,
    pub last_active_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
    #[serde(default)]
    pub activity_variant: ActivityVariant,
    #[serde(default)]
    pub activity_priority: u8,
    #[serde(default)]
    pub activity_expires_at: Option<DateTime<Utc>>,
    #[serde(default)]
    pub locked_main_avatar: bool,
    #[serde(default)]
    pub locked_activity_variant: bool,
}

pub async fn get_current_avatar() -> anyhow::Result<AvatarState> {
    let pool = db::pool().await?;
    ensure_avatar_schema(&pool).await?;

    let rows = sqlx::query(
        r#"
        SELECT id, avatar_id, character, color_scheme, size, position_x, position_y,
               animation_enabled, auto_hide, last_active_at, updated_at,
               activity_variant, activity_priority, activity_expires_at,
               locked_main_avatar, locked_activity_variant
        FROM avatar_state
        ORDER BY updated_at DESC
        LIMIT 1
        "#,
    )
    .fetch_all(&pool)
    .await?;

    if let Some(row) = rows.into_iter().next() {
        Ok(avatar_state_from_row(row)?)
    } else {
        Ok(get_default_avatar())
    }
}

pub async fn set_avatar(avatar_id: AvatarId) -> anyhow::Result<AvatarState> {
    let pool = db::pool().await?;
    ensure_avatar_schema(&pool).await?;
    let now = Utc::now();

    let current = get_current_avatar().await?;

    // Lock check: if main avatar is locked, skip auto-switch
    if current.locked_main_avatar {
        return Ok(current);
    }

    let avatar = AvatarState {
        id: uuid::Uuid::new_v4().to_string(),
        avatar_id,
        character: current.character,
        color_scheme: current.color_scheme,
        size: current.size,
        position_x: current.position_x,
        position_y: current.position_y,
        animation_enabled: current.animation_enabled,
        auto_hide: current.auto_hide,
        last_active_at: now,
        updated_at: now,
        activity_variant: current.activity_variant,
        activity_priority: current.activity_priority,
        activity_expires_at: current.activity_expires_at,
        locked_main_avatar: current.locked_main_avatar,
        locked_activity_variant: current.locked_activity_variant,
    };

    sqlx::query(
        r#"
        INSERT INTO avatar_state (
            id, theme, avatar_id, character, color_scheme, size, position_x, position_y,
            animation_enabled, auto_hide, last_active_at, updated_at,
            locked_main_avatar, locked_activity_variant
        )
        VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14)
        "#,
    )
    .bind(&avatar.id)
    .bind(avatar.avatar_id.as_str())
    .bind(avatar.avatar_id.as_str())
    .bind(&avatar.character)
    .bind(&avatar.color_scheme)
    .bind(avatar.size as i64)
    .bind(avatar.position_x as i64)
    .bind(avatar.position_y as i64)
    .bind(avatar.animation_enabled)
    .bind(avatar.auto_hide)
    .bind(avatar.last_active_at.to_rfc3339())
    .bind(avatar.updated_at.to_rfc3339())
    .bind(avatar.locked_main_avatar)
    .bind(avatar.locked_activity_variant)
    .execute(&pool)
    .await?;

    // Record avatar switch event
    let _ = record_avatar_event(
        &avatar.avatar_id,
        &avatar.activity_variant,
        "user_switch",
        None,
        None,
    )
    .await;

    Ok(avatar)
}

pub async fn update_position(x: i32, y: i32) -> anyhow::Result<AvatarState> {
    let pool = db::pool().await?;
    ensure_avatar_schema(&pool).await?;
    let now = Utc::now();

    let current = get_current_avatar().await?;

    let avatar = AvatarState {
        id: uuid::Uuid::new_v4().to_string(),
        avatar_id: current.avatar_id,
        character: current.character,
        color_scheme: current.color_scheme,
        size: current.size,
        position_x: x,
        position_y: y,
        animation_enabled: current.animation_enabled,
        auto_hide: current.auto_hide,
        last_active_at: now,
        updated_at: now,
        activity_variant: current.activity_variant,
        activity_priority: current.activity_priority,
        activity_expires_at: current.activity_expires_at,
        locked_main_avatar: current.locked_main_avatar,
        locked_activity_variant: current.locked_activity_variant,
    };

    sqlx::query(
        r#"
        INSERT INTO avatar_state (
            id, theme, avatar_id, character, color_scheme, size, position_x, position_y,
            animation_enabled, auto_hide, last_active_at, updated_at,
            locked_main_avatar, locked_activity_variant
        )
        VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14)
        "#,
    )
    .bind(&avatar.id)
    .bind(avatar.avatar_id.as_str())
    .bind(avatar.avatar_id.as_str())
    .bind(&avatar.character)
    .bind(&avatar.color_scheme)
    .bind(avatar.size as i64)
    .bind(avatar.position_x as i64)
    .bind(avatar.position_y as i64)
    .bind(avatar.animation_enabled)
    .bind(avatar.auto_hide)
    .bind(avatar.last_active_at.to_rfc3339())
    .bind(avatar.updated_at.to_rfc3339())
    .bind(avatar.locked_main_avatar)
    .bind(avatar.locked_activity_variant)
    .execute(&pool)
    .await?;

    Ok(avatar)
}

pub async fn set_activity_variant(variant: ActivityVariant) -> anyhow::Result<AvatarState> {
    let pool = db::pool().await?;
    ensure_avatar_schema(&pool).await?;

    let current = get_current_avatar().await?;

    // Lock check: if activity variant is locked, skip auto-switch
    if current.locked_activity_variant {
        return Ok(current);
    }

    // Dedup: if variant is unchanged, return current state without DB write or event
    if current.activity_variant == variant {
        return Ok(current);
    }

    let now = Utc::now();

    let avatar = AvatarState {
        id: uuid::Uuid::new_v4().to_string(),
        avatar_id: current.avatar_id,
        character: current.character,
        color_scheme: current.color_scheme,
        size: current.size,
        position_x: current.position_x,
        position_y: current.position_y,
        animation_enabled: current.animation_enabled,
        auto_hide: current.auto_hide,
        last_active_at: now,
        updated_at: now,
        activity_variant: variant,
        activity_priority: current.activity_priority,
        activity_expires_at: current.activity_expires_at,
        locked_main_avatar: current.locked_main_avatar,
        locked_activity_variant: current.locked_activity_variant,
    };

    sqlx::query(
        r#"
        INSERT INTO avatar_state (
            id, theme, avatar_id, character, color_scheme, size, position_x, position_y,
            animation_enabled, auto_hide, last_active_at, updated_at,
            activity_variant, activity_priority, activity_expires_at,
            locked_main_avatar, locked_activity_variant
        )
        VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14, ?15, ?16, ?17)
        "#,
    )
    .bind(&avatar.id)
    .bind(avatar.avatar_id.as_str())
    .bind(avatar.avatar_id.as_str())
    .bind(&avatar.character)
    .bind(&avatar.color_scheme)
    .bind(avatar.size as i64)
    .bind(avatar.position_x as i64)
    .bind(avatar.position_y as i64)
    .bind(avatar.animation_enabled)
    .bind(avatar.auto_hide)
    .bind(avatar.last_active_at.to_rfc3339())
    .bind(avatar.updated_at.to_rfc3339())
    .bind(avatar.activity_variant.as_str())
    .bind(avatar.activity_priority as i64)
    .bind(avatar.activity_expires_at.map(|dt| dt.to_rfc3339()))
    .bind(avatar.locked_main_avatar)
    .bind(avatar.locked_activity_variant)
    .execute(&pool)
    .await?;

    Ok(avatar)
}

pub fn get_default_avatar() -> AvatarState {
    AvatarState {
        id: "default".to_string(),
        avatar_id: AvatarId::Original,
        character: "pet".to_string(),
        color_scheme: "blue".to_string(),
        size: 64,
        position_x: 100,
        position_y: 100,
        animation_enabled: true,
        auto_hide: false,
        last_active_at: Utc::now(),
        updated_at: Utc::now(),
        activity_variant: ActivityVariant::Idle,
        activity_priority: 0,
        activity_expires_at: None,
        locked_main_avatar: false,
        locked_activity_variant: false,
    }
}

/// Manual main avatar selection — bypasses lock (user has ultimate authority).
pub async fn set_main_avatar_manual(avatar_id: AvatarId) -> anyhow::Result<AvatarState> {
    let pool = db::pool().await?;
    ensure_avatar_schema(&pool).await?;
    let now = Utc::now();

    let current = get_current_avatar().await?;

    let avatar = AvatarState {
        id: uuid::Uuid::new_v4().to_string(),
        avatar_id,
        character: current.character,
        color_scheme: current.color_scheme,
        size: current.size,
        position_x: current.position_x,
        position_y: current.position_y,
        animation_enabled: current.animation_enabled,
        auto_hide: current.auto_hide,
        last_active_at: now,
        updated_at: now,
        activity_variant: current.activity_variant,
        activity_priority: current.activity_priority,
        activity_expires_at: current.activity_expires_at,
        locked_main_avatar: current.locked_main_avatar,
        locked_activity_variant: current.locked_activity_variant,
    };

    sqlx::query(
        r#"
        INSERT INTO avatar_state (
            id, theme, avatar_id, character, color_scheme, size, position_x, position_y,
            animation_enabled, auto_hide, last_active_at, updated_at,
            locked_main_avatar, locked_activity_variant
        )
        VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14)
        "#,
    )
    .bind(&avatar.id)
    .bind(avatar.avatar_id.as_str())
    .bind(avatar.avatar_id.as_str())
    .bind(&avatar.character)
    .bind(&avatar.color_scheme)
    .bind(avatar.size as i64)
    .bind(avatar.position_x as i64)
    .bind(avatar.position_y as i64)
    .bind(avatar.animation_enabled)
    .bind(avatar.auto_hide)
    .bind(avatar.last_active_at.to_rfc3339())
    .bind(avatar.updated_at.to_rfc3339())
    .bind(avatar.locked_main_avatar)
    .bind(avatar.locked_activity_variant)
    .execute(&pool)
    .await?;

    let _ = record_avatar_event(
        &avatar.avatar_id,
        &avatar.activity_variant,
        "manual_switch",
        None,
        None,
    )
    .await;

    Ok(avatar)
}

/// Manual sub-avatar selection — bypasses lock (user has ultimate authority).
pub async fn set_sub_avatar_manual(variant: ActivityVariant) -> anyhow::Result<AvatarState> {
    let pool = db::pool().await?;
    ensure_avatar_schema(&pool).await?;

    let current = get_current_avatar().await?;

    // Dedup: if variant is unchanged, return current state without DB write or event
    if current.activity_variant == variant {
        return Ok(current);
    }

    let now = Utc::now();

    let avatar = AvatarState {
        id: uuid::Uuid::new_v4().to_string(),
        avatar_id: current.avatar_id,
        character: current.character,
        color_scheme: current.color_scheme,
        size: current.size,
        position_x: current.position_x,
        position_y: current.position_y,
        animation_enabled: current.animation_enabled,
        auto_hide: current.auto_hide,
        last_active_at: now,
        updated_at: now,
        activity_variant: variant,
        activity_priority: current.activity_priority,
        activity_expires_at: current.activity_expires_at,
        locked_main_avatar: current.locked_main_avatar,
        locked_activity_variant: current.locked_activity_variant,
    };

    sqlx::query(
        r#"
        INSERT INTO avatar_state (
            id, theme, avatar_id, character, color_scheme, size, position_x, position_y,
            animation_enabled, auto_hide, last_active_at, updated_at,
            activity_variant, activity_priority, activity_expires_at,
            locked_main_avatar, locked_activity_variant
        )
        VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14, ?15, ?16, ?17)
        "#,
    )
    .bind(&avatar.id)
    .bind(avatar.avatar_id.as_str())
    .bind(avatar.avatar_id.as_str())
    .bind(&avatar.character)
    .bind(&avatar.color_scheme)
    .bind(avatar.size as i64)
    .bind(avatar.position_x as i64)
    .bind(avatar.position_y as i64)
    .bind(avatar.animation_enabled)
    .bind(avatar.auto_hide)
    .bind(avatar.last_active_at.to_rfc3339())
    .bind(avatar.updated_at.to_rfc3339())
    .bind(avatar.activity_variant.as_str())
    .bind(avatar.activity_priority as i64)
    .bind(avatar.activity_expires_at.map(|dt| dt.to_rfc3339()))
    .bind(avatar.locked_main_avatar)
    .bind(avatar.locked_activity_variant)
    .execute(&pool)
    .await?;

    Ok(avatar)
}

/// Toggle the main avatar lock. When locked, `set_avatar()` (auto) will be a no-op.
pub async fn toggle_lock_main_avatar(locked: bool) -> anyhow::Result<AvatarState> {
    let pool = db::pool().await?;
    ensure_avatar_schema(&pool).await?;
    let now = Utc::now();

    let current = get_current_avatar().await?;

    let avatar = AvatarState {
        id: uuid::Uuid::new_v4().to_string(),
        avatar_id: current.avatar_id,
        character: current.character,
        color_scheme: current.color_scheme,
        size: current.size,
        position_x: current.position_x,
        position_y: current.position_y,
        animation_enabled: current.animation_enabled,
        auto_hide: current.auto_hide,
        last_active_at: now,
        updated_at: now,
        activity_variant: current.activity_variant,
        activity_priority: current.activity_priority,
        activity_expires_at: current.activity_expires_at,
        locked_main_avatar: locked,
        locked_activity_variant: current.locked_activity_variant,
    };

    sqlx::query(
        r#"
        INSERT INTO avatar_state (
            id, theme, avatar_id, character, color_scheme, size, position_x, position_y,
            animation_enabled, auto_hide, last_active_at, updated_at,
            activity_variant, activity_priority, activity_expires_at,
            locked_main_avatar, locked_activity_variant
        )
        VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14, ?15, ?16, ?17)
        "#,
    )
    .bind(&avatar.id)
    .bind(avatar.avatar_id.as_str())
    .bind(avatar.avatar_id.as_str())
    .bind(&avatar.character)
    .bind(&avatar.color_scheme)
    .bind(avatar.size as i64)
    .bind(avatar.position_x as i64)
    .bind(avatar.position_y as i64)
    .bind(avatar.animation_enabled)
    .bind(avatar.auto_hide)
    .bind(avatar.last_active_at.to_rfc3339())
    .bind(avatar.updated_at.to_rfc3339())
    .bind(avatar.activity_variant.as_str())
    .bind(avatar.activity_priority as i64)
    .bind(avatar.activity_expires_at.map(|dt| dt.to_rfc3339()))
    .bind(avatar.locked_main_avatar)
    .bind(avatar.locked_activity_variant)
    .execute(&pool)
    .await?;

    Ok(avatar)
}

/// Toggle the activity variant lock. When locked, `set_activity_variant()` (auto) will be a no-op.
pub async fn toggle_lock_activity_variant(locked: bool) -> anyhow::Result<AvatarState> {
    let pool = db::pool().await?;
    ensure_avatar_schema(&pool).await?;
    let now = Utc::now();

    let current = get_current_avatar().await?;

    let avatar = AvatarState {
        id: uuid::Uuid::new_v4().to_string(),
        avatar_id: current.avatar_id,
        character: current.character,
        color_scheme: current.color_scheme,
        size: current.size,
        position_x: current.position_x,
        position_y: current.position_y,
        animation_enabled: current.animation_enabled,
        auto_hide: current.auto_hide,
        last_active_at: now,
        updated_at: now,
        activity_variant: current.activity_variant,
        activity_priority: current.activity_priority,
        activity_expires_at: current.activity_expires_at,
        locked_main_avatar: current.locked_main_avatar,
        locked_activity_variant: locked,
    };

    sqlx::query(
        r#"
        INSERT INTO avatar_state (
            id, theme, avatar_id, character, color_scheme, size, position_x, position_y,
            animation_enabled, auto_hide, last_active_at, updated_at,
            activity_variant, activity_priority, activity_expires_at,
            locked_main_avatar, locked_activity_variant
        )
        VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14, ?15, ?16, ?17)
        "#,
    )
    .bind(&avatar.id)
    .bind(avatar.avatar_id.as_str())
    .bind(avatar.avatar_id.as_str())
    .bind(&avatar.character)
    .bind(&avatar.color_scheme)
    .bind(avatar.size as i64)
    .bind(avatar.position_x as i64)
    .bind(avatar.position_y as i64)
    .bind(avatar.animation_enabled)
    .bind(avatar.auto_hide)
    .bind(avatar.last_active_at.to_rfc3339())
    .bind(avatar.updated_at.to_rfc3339())
    .bind(avatar.activity_variant.as_str())
    .bind(avatar.activity_priority as i64)
    .bind(avatar.activity_expires_at.map(|dt| dt.to_rfc3339()))
    .bind(avatar.locked_main_avatar)
    .bind(avatar.locked_activity_variant)
    .execute(&pool)
    .await?;

    Ok(avatar)
}

pub async fn init_db() -> anyhow::Result<()> {
    let pool = db::pool().await?;

    sqlx::query(
        r#"
        CREATE TABLE IF NOT EXISTS avatar_state (
            id TEXT PRIMARY KEY,
            theme TEXT NOT NULL,
            avatar_id TEXT NOT NULL DEFAULT 'original',
            character TEXT NOT NULL,
            color_scheme TEXT NOT NULL,
            size INTEGER NOT NULL,
            position_x INTEGER NOT NULL,
            position_y INTEGER NOT NULL,
            animation_enabled BOOLEAN NOT NULL DEFAULT 1,
            auto_hide BOOLEAN NOT NULL DEFAULT 0,
            last_active_at TEXT NOT NULL,
            updated_at TEXT NOT NULL
        )
        "#,
    )
    .execute(&pool)
    .await?;

    ensure_avatar_schema(&pool).await?;

    sqlx::query("CREATE INDEX IF NOT EXISTS idx_avatar_updated_at ON avatar_state(updated_at);")
        .execute(&pool)
        .await?;

    // Avatar events table for tracking avatar switch history
    sqlx::query(
        r#"
        CREATE TABLE IF NOT EXISTS avatar_events (
            id INTEGER PRIMARY KEY AUTOINCREMENT,
            avatar_id TEXT NOT NULL,
            activity_variant TEXT NOT NULL DEFAULT 'idle',
            trigger_source TEXT NOT NULL,
            mood_zone TEXT,
            relationship_stage TEXT,
            created_at TEXT NOT NULL
        )
        "#,
    )
    .execute(&pool)
    .await?;

    sqlx::query(
        "CREATE INDEX IF NOT EXISTS idx_avatar_events_created_at ON avatar_events(created_at);",
    )
    .execute(&pool)
    .await?;

    Ok(())
}

/// Record an avatar switch event
pub async fn record_avatar_event(
    avatar_id: &AvatarId,
    activity_variant: &ActivityVariant,
    trigger_source: &str,
    mood_zone: Option<&str>,
    relationship_stage: Option<&str>,
) -> anyhow::Result<()> {
    let pool = db::pool().await?;

    sqlx::query(
        r#"
        INSERT INTO avatar_events (avatar_id, activity_variant, trigger_source, mood_zone, relationship_stage, created_at)
        VALUES (?1, ?2, ?3, ?4, ?5, ?6)
        "#,
    )
    .bind(avatar_id.as_str())
    .bind(activity_variant.as_str())
    .bind(trigger_source)
    .bind(mood_zone)
    .bind(relationship_stage)
    .bind(Utc::now().to_rfc3339())
    .execute(&pool)
    .await?;

    Ok(())
}

/// Get recent avatar events
pub async fn get_recent_avatar_events(limit: i64) -> anyhow::Result<Vec<AvatarEvent>> {
    let pool = db::pool().await?;

    let rows = sqlx::query(
        r#"
        SELECT id, avatar_id, activity_variant, trigger_source, mood_zone, relationship_stage, created_at
        FROM avatar_events
        ORDER BY created_at DESC
        LIMIT ?1
        "#,
    )
    .bind(limit)
    .fetch_all(&pool)
    .await?;

    let events = rows
        .into_iter()
        .map(|row| AvatarEvent {
            id: row.try_get::<i64, _>("id").unwrap_or(0),
            avatar_id: row.try_get::<String, _>("avatar_id").unwrap_or_default(),
            activity_variant: row
                .try_get::<String, _>("activity_variant")
                .unwrap_or_default(),
            trigger_source: row
                .try_get::<String, _>("trigger_source")
                .unwrap_or_default(),
            mood_zone: row
                .try_get::<Option<String>, _>("mood_zone")
                .unwrap_or(None),
            relationship_stage: row
                .try_get::<Option<String>, _>("relationship_stage")
                .unwrap_or(None),
            created_at: row.try_get::<String, _>("created_at").unwrap_or_default(),
        })
        .collect();

    Ok(events)
}

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct AvatarEvent {
    pub id: i64,
    pub avatar_id: String,
    pub activity_variant: String,
    pub trigger_source: String,
    pub mood_zone: Option<String>,
    pub relationship_stage: Option<String>,
    pub created_at: String,
}

fn avatar_state_from_row(row: sqlx::sqlite::SqliteRow) -> anyhow::Result<AvatarState> {
    let activity_variant_str: String = row
        .try_get("activity_variant")
        .unwrap_or_else(|_| "idle".to_string());
    let activity_variant =
        ActivityVariant::from_str(&activity_variant_str).unwrap_or(ActivityVariant::Idle);
    let activity_priority: i64 = row.try_get("activity_priority").unwrap_or(0);
    let activity_expires_at: Option<String> = row.try_get("activity_expires_at").unwrap_or(None);
    let locked_main_avatar: bool = row.try_get("locked_main_avatar").unwrap_or(false);
    let locked_activity_variant: bool = row.try_get("locked_activity_variant").unwrap_or(false);
    Ok(AvatarState {
        id: row.try_get("id")?,
        avatar_id: AvatarId::from_db_str(row.try_get::<String, _>("avatar_id")?.as_str())?,
        character: row.try_get("character")?,
        color_scheme: row.try_get("color_scheme")?,
        size: row.try_get("size")?,
        position_x: row.try_get("position_x")?,
        position_y: row.try_get("position_y")?,
        animation_enabled: row.try_get("animation_enabled")?,
        auto_hide: row.try_get("auto_hide")?,
        last_active_at: DateTime::parse_from_rfc3339(
            row.try_get::<String, _>("last_active_at")?.as_str(),
        )?
        .with_timezone(&Utc),
        updated_at: DateTime::parse_from_rfc3339(row.try_get::<String, _>("updated_at")?.as_str())?
            .with_timezone(&Utc),
        activity_variant,
        activity_priority: activity_priority as u8,
        activity_expires_at: activity_expires_at
            .as_deref()
            .and_then(|s| DateTime::parse_from_rfc3339(s).ok())
            .map(|dt| dt.with_timezone(&Utc)),
        locked_main_avatar,
        locked_activity_variant,
    })
}

async fn ensure_avatar_schema(pool: &sqlx::SqlitePool) -> anyhow::Result<()> {
    let rows = sqlx::query("PRAGMA table_info(avatar_state);")
        .fetch_all(pool)
        .await?;
    let has_avatar_id = rows
        .iter()
        .filter_map(|row| row.try_get::<String, _>("name").ok())
        .any(|name| name == "avatar_id");

    if !has_avatar_id {
        sqlx::query(
            "ALTER TABLE avatar_state ADD COLUMN avatar_id TEXT NOT NULL DEFAULT 'original';",
        )
        .execute(pool)
        .await?;
    }

    sqlx::query(
        r#"
        UPDATE avatar_state
        SET avatar_id = CASE
            WHEN theme IN ('original', 'document_secretary', 'programmer') THEN theme
            ELSE 'original'
        END
        WHERE avatar_id IS NULL
           OR avatar_id = ''
           OR avatar_id NOT IN ('original', 'document_secretary', 'programmer')
        "#,
    )
    .execute(pool)
    .await?;

    let has_activity_variant = rows
        .iter()
        .filter_map(|row| row.try_get::<String, _>("name").ok())
        .any(|name| name == "activity_variant");

    if !has_activity_variant {
        sqlx::query(
            "ALTER TABLE avatar_state ADD COLUMN activity_variant TEXT NOT NULL DEFAULT 'idle';",
        )
        .execute(pool)
        .await?;
    }

    let has_activity_priority = rows
        .iter()
        .filter_map(|row| row.try_get::<String, _>("name").ok())
        .any(|name| name == "activity_priority");

    if !has_activity_priority {
        sqlx::query(
            "ALTER TABLE avatar_state ADD COLUMN activity_priority INTEGER NOT NULL DEFAULT 0;",
        )
        .execute(pool)
        .await?;
    }

    let has_activity_expires_at = rows
        .iter()
        .filter_map(|row| row.try_get::<String, _>("name").ok())
        .any(|name| name == "activity_expires_at");

    if !has_activity_expires_at {
        sqlx::query("ALTER TABLE avatar_state ADD COLUMN activity_expires_at TEXT;")
            .execute(pool)
            .await?;
    }

    let has_locked_main_avatar = rows
        .iter()
        .filter_map(|row| row.try_get::<String, _>("name").ok())
        .any(|name| name == "locked_main_avatar");

    if !has_locked_main_avatar {
        sqlx::query(
            "ALTER TABLE avatar_state ADD COLUMN locked_main_avatar BOOLEAN NOT NULL DEFAULT 0;",
        )
        .execute(pool)
        .await?;
    }

    let has_locked_activity_variant = rows
        .iter()
        .filter_map(|row| row.try_get::<String, _>("name").ok())
        .any(|name| name == "locked_activity_variant");

    if !has_locked_activity_variant {
        sqlx::query(
            "ALTER TABLE avatar_state ADD COLUMN locked_activity_variant BOOLEAN NOT NULL DEFAULT 0;",
        )
        .execute(pool)
        .await?;
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::test_support::TestRoot;

    #[tokio::test]
    async fn test_get_default_avatar() {
        let _root = TestRoot::new();

        let avatar = get_default_avatar();
        assert_eq!(avatar.avatar_id, AvatarId::Original);
        assert_eq!(avatar.character, "pet");
        assert_eq!(avatar.size, 64);
    }

    #[tokio::test]
    async fn test_get_and_set_avatar() {
        let _root = TestRoot::new();
        init_db().await.expect("init db");

        let avatar = set_avatar(AvatarId::Programmer).await.expect("set avatar");

        assert_eq!(avatar.avatar_id, AvatarId::Programmer);
        assert_eq!(avatar.character, "pet");
        assert_eq!(avatar.color_scheme, "blue");
        assert_eq!(avatar.size, 64);

        let current = get_current_avatar().await.expect("get current");
        assert_eq!(current.avatar_id, AvatarId::Programmer);
        assert_eq!(current.character, "pet");
    }

    #[tokio::test]
    async fn test_avatar_id_enum() {
        assert_eq!(AvatarId::Original.as_str(), "original");
        assert_eq!(AvatarId::Programmer.as_str(), "programmer");
        assert_eq!(AvatarId::from_str("original").unwrap(), AvatarId::Original);
        assert_eq!(
            AvatarId::from_str("document_secretary").unwrap(),
            AvatarId::DocumentSecretary
        );
        assert!(AvatarId::from_str("cute").is_err());
        assert_eq!(AvatarId::from_db_str("cute").unwrap(), AvatarId::Original);
    }

    #[tokio::test]
    async fn test_legacy_theme_rows_map_to_original() {
        let _root = TestRoot::new();
        init_db().await.expect("init db");
        let pool = db::pool().await.expect("pool");
        sqlx::query(
            r#"
            INSERT INTO avatar_state (
                id, theme, character, color_scheme, size, position_x, position_y,
                animation_enabled, auto_hide, last_active_at, updated_at
            )
            VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11)
            "#,
        )
        .bind("legacy")
        .bind("cool")
        .bind("pet")
        .bind("blue")
        .bind(64_i64)
        .bind(100_i64)
        .bind(100_i64)
        .bind(true)
        .bind(false)
        .bind(Utc::now().to_rfc3339())
        .bind(Utc::now().to_rfc3339())
        .execute(&pool)
        .await
        .expect("insert legacy avatar");

        let current = get_current_avatar().await.expect("get current");
        assert_eq!(current.avatar_id, AvatarId::Original);
    }

    #[tokio::test]
    async fn test_update_position() {
        let _root = TestRoot::new();
        init_db().await.expect("init db");

        let avatar = set_avatar(AvatarId::DocumentSecretary)
            .await
            .expect("set avatar");

        let updated = update_position(200, 300).await.expect("update position");
        assert_eq!(updated.position_x, 200);
        assert_eq!(updated.position_y, 300);
        assert_eq!(updated.avatar_id, avatar.avatar_id);
        assert_eq!(updated.character, avatar.character);
    }
}
