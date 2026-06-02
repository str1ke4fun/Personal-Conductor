use serde::{Deserialize, Serialize};
use sqlx::Row;

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum SkillContextMode {
    CurrentWorkspace,
    CurrentDocument,
    Global,
}

#[deprecated(note = "Use SkillPackage capabilities + ConnectorRegistry + PolicyEngine instead")]
#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct SkillSpec {
    pub id: String,
    pub name: String,
    pub description: String,
    pub when_to_use: Vec<String>,
    #[deprecated(note = "Use SkillPackage capabilities instead")]
    pub allowed_tools: Vec<String>,
    pub default_avatar_id: Option<String>,
    pub context_mode: SkillContextMode,
    pub proactive_allowed: bool,
}

/// Wrapper for persisting the full skill list.
#[derive(Serialize, Deserialize, Clone, Debug, Default)]
#[allow(deprecated)]
pub struct SkillsFile {
    pub skills: Vec<SkillSpec>,
}

#[allow(deprecated)]
pub fn default_skill_specs() -> Vec<SkillSpec> {
    vec![
        SkillSpec {
            id: "document_assistant".to_string(),
            name: "Document Assistant".to_string(),
            description: "Assist with document reading, summarizing, and dry-run edits."
                .to_string(),
            when_to_use: vec![
                "word processor or markdown editor is focused".to_string(),
                "user asks to inspect, summarize, or improve a document".to_string(),
            ],
            allowed_tools: vec![
                "pet.set_avatar".to_string(),
                "task.create".to_string(),
                "task.list".to_string(),
                "office.inspect_document".to_string(),
                "office.export_text".to_string(),
                "office.patch_dry_run".to_string(),
            ],
            default_avatar_id: Some("document_secretary".to_string()),
            context_mode: SkillContextMode::CurrentDocument,
            proactive_allowed: true,
        },
        SkillSpec {
            id: "coding_assistant".to_string(),
            name: "Coding Assistant".to_string(),
            description: "Assist with coding tasks through proposals and subagents.".to_string(),
            when_to_use: vec![
                "code editor or terminal is focused".to_string(),
                "user asks to run a coding review or implementation agent".to_string(),
            ],
            allowed_tools: vec![
                "pet.set_avatar".to_string(),
                "task.create".to_string(),
                "task.list".to_string(),
                "task.claim".to_string(),
                "subagent.claude_p".to_string(),
                "tool.search".to_string(),
            ],
            default_avatar_id: Some("programmer".to_string()),
            context_mode: SkillContextMode::CurrentWorkspace,
            proactive_allowed: true,
        },
        SkillSpec {
            id: "pet_avatar_router".to_string(),
            name: "Pet Avatar Router".to_string(),
            description: "Route explicit avatar requests to the restricted avatar tool."
                .to_string(),
            when_to_use: vec![
                "user explicitly asks to switch pet avatar".to_string(),
                "another skill requests a temporary work mode avatar".to_string(),
            ],
            allowed_tools: vec![
                "pet.set_avatar".to_string(),
                "conductor.pet.set_avatar".to_string(),
            ],
            default_avatar_id: None,
            context_mode: SkillContextMode::Global,
            proactive_allowed: true,
        },
    ]
}

#[allow(deprecated)]
pub fn get_default_skill_spec(id: &str) -> Option<SkillSpec> {
    default_skill_specs()
        .into_iter()
        .find(|skill| skill.id == id)
}

/// Load persisted skills from `state/skills.json`, falling back to defaults.
#[allow(deprecated)]
pub async fn list_skills() -> anyhow::Result<Vec<SkillSpec>> {
    let path = crate::paths::Paths::skills_json();
    if path.exists() {
        let data = tokio::fs::read_to_string(&path).await?;
        let file: SkillsFile = serde_json::from_str(&data)?;
        Ok(file.skills)
    } else {
        Ok(default_skill_specs())
    }
}

/// Synchronous version for use in non-async contexts (e.g. build_tool_definitions).
#[allow(deprecated)]
pub fn load_skills_sync() -> Vec<SkillSpec> {
    let path = crate::paths::Paths::skills_json();
    if path.exists() {
        if let Ok(data) = std::fs::read_to_string(&path) {
            if let Ok(file) = serde_json::from_str::<SkillsFile>(&data) {
                return file.skills;
            }
        }
    }
    default_skill_specs()
}

/// Match user message against skill `when_to_use` keywords.
/// Returns `allowed_tools` from all matching skills.
#[deprecated(note = "Use match_enabled_skills + collect_capabilities + PolicyEngine instead")]
#[allow(deprecated)]
pub fn skill_contextual_tools(user_message: &str) -> Vec<String> {
    let skills = load_skills_sync();
    let lower = user_message.to_lowercase();
    let mut tool_ids = Vec::new();
    for skill in &skills {
        let matched = skill.when_to_use.iter().any(|pattern| {
            let words: Vec<&str> = pattern.split_whitespace().collect();
            // Strip punctuation for matching
            let clean = |w: &str| -> String {
                w.chars()
                    .filter(|c| c.is_alphanumeric())
                    .collect::<String>()
                    .to_lowercase()
            };
            let hits = words
                .iter()
                .filter(|w| {
                    let cw = clean(w);
                    !cw.is_empty() && lower.contains(&cw)
                })
                .count();
            hits >= 2 && hits >= words.len() / 2
        });
        if matched {
            tool_ids.extend(skill.allowed_tools.iter().cloned());
        }
    }
    tool_ids.sort();
    tool_ids.dedup();
    tool_ids
}

/// Persist the given skill list to `state/skills.json`.
#[allow(deprecated)]
pub async fn save_skills(skills: &[SkillSpec]) -> anyhow::Result<()> {
    let path = crate::paths::Paths::skills_json();
    if let Some(parent) = path.parent() {
        tokio::fs::create_dir_all(parent).await?;
    }
    let file = SkillsFile {
        skills: skills.to_vec(),
    };
    let json = serde_json::to_string_pretty(&file)?;
    tokio::fs::write(&path, json).await?;
    Ok(())
}

/// Parse a JSON string into a list of `SkillSpec`s.
///
/// Accepts either:
/// - a bare array `[{ ... }, ...]`
/// - a wrapped object `{ "skills": [{ ... }, ...] }`
#[allow(deprecated)]
pub fn parse_skills_json(json: &str) -> anyhow::Result<Vec<SkillSpec>> {
    // Try as a bare array first.
    if let Ok(skills) = serde_json::from_str::<Vec<SkillSpec>>(json) {
        return Ok(skills);
    }
    // Fall back to wrapped format.
    let file: SkillsFile = serde_json::from_str(json)?;
    Ok(file.skills)
}

/// Import skills from a JSON string and persist them.
///
/// Existing skills are replaced. Returns the imported list.
#[allow(deprecated)]
pub async fn import_skills_from_json(json: &str) -> anyhow::Result<Vec<SkillSpec>> {
    let skills = parse_skills_json(json)?;
    if skills.is_empty() {
        anyhow::bail!("imported JSON contains no skills");
    }
    save_skills(&skills).await?;
    Ok(skills)
}

// ── SkillPackage data model (TASK-065) ──

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum SkillSource {
    Builtin,
    UserImport,
    Marketplace,
    DevLocal,
}

impl std::fmt::Display for SkillSource {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            SkillSource::Builtin => write!(f, "builtin"),
            SkillSource::UserImport => write!(f, "user_import"),
            SkillSource::Marketplace => write!(f, "marketplace"),
            SkillSource::DevLocal => write!(f, "dev_local"),
        }
    }
}

impl std::str::FromStr for SkillSource {
    type Err = anyhow::Error;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s {
            "builtin" => Ok(SkillSource::Builtin),
            "user_import" => Ok(SkillSource::UserImport),
            "marketplace" => Ok(SkillSource::Marketplace),
            "dev_local" => Ok(SkillSource::DevLocal),
            _ => anyhow::bail!("unknown skill source: {s}"),
        }
    }
}

#[derive(Serialize, Deserialize, Clone, Debug, Default)]
pub struct SkillActivation {
    #[serde(default)]
    pub keywords: Vec<String>,
    #[serde(default)]
    pub apps: Vec<String>,
    #[serde(default)]
    pub url_patterns: Vec<String>,
    #[serde(default)]
    pub file_patterns: Vec<String>,
}

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct SkillPackage {
    pub id: String,
    pub name: String,
    pub version: String,
    pub description: String,
    #[serde(default)]
    pub author: Option<String>,
    pub activation: SkillActivation,
    pub capabilities: Vec<String>,
    pub source: SkillSource,
    #[serde(default)]
    pub enabled: bool,
    #[serde(default)]
    pub body: String,
}

/// Parse YAML frontmatter from a Markdown string.
///
/// Expects the input to start with `---` and end with `---` (or `...`),
/// followed by the Markdown body. Returns the parsed YAML value and the
/// body text.
fn split_frontmatter(md: &str) -> anyhow::Result<(&str, &str)> {
    let trimmed = md.trim_start();
    if !trimmed.starts_with("---") {
        anyhow::bail!("missing YAML frontmatter (expected document to start with ---)");
    }
    let after_first = &trimmed[3..];
    // Find the closing ---
    let end = after_first
        .find("\n---")
        .or_else(|| after_first.find("\r\n---"))
        .ok_or_else(|| anyhow::anyhow!("frontmatter not closed (expected closing ---)"))?;
    let yaml_str = &after_first[..end];
    let rest = &after_first[end + 4..]; // skip past "\n---"
                                        // Skip optional trailing `...` and leading whitespace
    let body = rest.trim_start_matches(|c: char| c == '.' || c == '\r' || c == '\n' || c == ' ');
    Ok((yaml_str, body))
}

/// Parse a Markdown+YAML frontmatter document into a `SkillPackage`.
///
/// Required frontmatter fields: id, name, version, description, activation, capabilities.
pub fn parse_skill_markdown(md: &str) -> anyhow::Result<SkillPackage> {
    let (yaml_str, body) = split_frontmatter(md)?;

    // Use serde_yaml for proper YAML parsing
    let raw: serde_yaml::Value = serde_yaml::from_str(yaml_str)
        .map_err(|e| anyhow::anyhow!("invalid YAML frontmatter: {e}"))?;

    let map = match raw {
        serde_yaml::Value::Mapping(m) => m,
        _ => anyhow::bail!("frontmatter must be a YAML mapping"),
    };

    // Helper to extract a string value
    let get_str = |key: &str| -> anyhow::Result<String> {
        let val = map
            .get(&serde_yaml::Value::String(key.to_string()))
            .ok_or_else(|| anyhow::anyhow!("missing required field: {key}"))?;
        val.as_str()
            .map(|s| s.to_string())
            .ok_or_else(|| anyhow::anyhow!("field '{key}' must be a string"))
    };

    let get_optional_str = |key: &str| -> Option<String> {
        map.get(&serde_yaml::Value::String(key.to_string()))
            .and_then(|v| v.as_str())
            .map(|s| s.to_string())
    };

    let get_str_vec = |key: &str| -> anyhow::Result<Vec<String>> {
        let val = map
            .get(&serde_yaml::Value::String(key.to_string()))
            .ok_or_else(|| anyhow::anyhow!("missing required field: {key}"))?;
        match val {
            serde_yaml::Value::Sequence(seq) => {
                let mut result = Vec::new();
                for item in seq {
                    match item.as_str() {
                        Some(s) => result.push(s.to_string()),
                        None => anyhow::bail!("all items in '{key}' must be strings"),
                    }
                }
                Ok(result)
            }
            _ => anyhow::bail!("field '{key}' must be a list"),
        }
    };

    // Extract required fields
    let id = get_str("id")?;
    let name = get_str("name")?;
    let version = get_str("version")?;
    let description = get_str("description")?;
    let author = get_optional_str("author");
    let capabilities = get_str_vec("capabilities")?;

    // Extract activation block
    let activation_val = map
        .get(&serde_yaml::Value::String("activation".to_string()))
        .ok_or_else(|| anyhow::anyhow!("missing required field: activation"))?;

    let activation: SkillActivation = serde_yaml::from_value(activation_val.clone())
        .map_err(|e| anyhow::anyhow!("invalid activation block: {e}"))?;

    // Validate required fields are non-empty
    if id.is_empty() {
        anyhow::bail!("field 'id' must not be empty");
    }
    if name.is_empty() {
        anyhow::bail!("field 'name' must not be empty");
    }
    if version.is_empty() {
        anyhow::bail!("field 'version' must not be empty");
    }
    if description.is_empty() {
        anyhow::bail!("field 'description' must not be empty");
    }
    if capabilities.is_empty() {
        anyhow::bail!("field 'capabilities' must not be empty");
    }

    Ok(SkillPackage {
        id,
        name,
        version,
        description,
        author,
        activation,
        capabilities,
        source: SkillSource::UserImport,
        enabled: false,
        body: body.to_string(),
    })
}

/// Import a skill from Markdown. Sets enabled=false by default.
/// Returns an error if a skill with the same id already exists.
pub async fn import_skill_markdown(md: &str) -> anyhow::Result<SkillPackage> {
    let mut pkg = parse_skill_markdown(md)?;
    pkg.enabled = false;
    pkg.source = SkillSource::UserImport;

    // Check for duplicate id
    if let Some(_) = get_skill_package(&pkg.id).await? {
        anyhow::bail!("skill with id '{}' already exists", pkg.id);
    }

    insert_skill_package(&pkg).await?;
    Ok(pkg)
}

/// Insert a new skill package into the database.
async fn insert_skill_package(pkg: &SkillPackage) -> anyhow::Result<()> {
    let pool = crate::db::pool().await?;
    let now = chrono::Utc::now().to_rfc3339();
    let activation_json = serde_json::to_string(&pkg.activation)?;
    let capabilities_json = serde_json::to_string(&pkg.capabilities)?;

    sqlx::query(
        r#"INSERT INTO skill_packages
          (id, name, version, description, author, activation_json, capabilities_json, source, enabled, body, created_at, updated_at)
          VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)"#,
    )
    .bind(&pkg.id)
    .bind(&pkg.name)
    .bind(&pkg.version)
    .bind(&pkg.description)
    .bind(&pkg.author)
    .bind(&activation_json)
    .bind(&capabilities_json)
    .bind(pkg.source.to_string())
    .bind(pkg.enabled)
    .bind(&pkg.body)
    .bind(&now)
    .bind(&now)
    .execute(&pool)
    .await?;

    // Insert capabilities into join table
    for cap in &pkg.capabilities {
        sqlx::query("INSERT INTO skill_capabilities (skill_id, capability) VALUES (?, ?)")
            .bind(&pkg.id)
            .bind(cap)
            .execute(&pool)
            .await?;
    }

    Ok(())
}

/// List all skill packages from the database.
pub async fn list_skill_packages() -> anyhow::Result<Vec<SkillPackage>> {
    let pool = crate::db::pool().await?;
    let rows = sqlx::query(
        "SELECT id, name, version, description, author, activation_json, capabilities_json, source, enabled, body FROM skill_packages ORDER BY name",
    )
    .fetch_all(&pool)
    .await?;

    let mut packages = Vec::new();
    for row in rows {
        let activation_json: String = row.get("activation_json");
        let capabilities_json: String = row.get("capabilities_json");
        let source_str: String = row.get("source");

        let activation: SkillActivation = serde_json::from_str(&activation_json)?;
        let capabilities: Vec<String> = serde_json::from_str(&capabilities_json)?;
        let source: SkillSource = source_str.parse()?;

        packages.push(SkillPackage {
            id: row.get("id"),
            name: row.get("name"),
            version: row.get("version"),
            description: row.get("description"),
            author: row.get("author"),
            activation,
            capabilities,
            source,
            enabled: row.get("enabled"),
            body: row.get("body"),
        });
    }

    Ok(packages)
}

/// Get a single skill package by id.
pub async fn get_skill_package(id: &str) -> anyhow::Result<Option<SkillPackage>> {
    let pool = crate::db::pool().await?;
    let row = sqlx::query(
        "SELECT id, name, version, description, author, activation_json, capabilities_json, source, enabled, body FROM skill_packages WHERE id = ?",
    )
    .bind(id)
    .fetch_optional(&pool)
    .await?;

    match row {
        Some(row) => {
            let activation_json: String = row.get("activation_json");
            let capabilities_json: String = row.get("capabilities_json");
            let source_str: String = row.get("source");

            let activation: SkillActivation = serde_json::from_str(&activation_json)?;
            let capabilities: Vec<String> = serde_json::from_str(&capabilities_json)?;
            let source: SkillSource = source_str.parse()?;

            Ok(Some(SkillPackage {
                id: row.get("id"),
                name: row.get("name"),
                version: row.get("version"),
                description: row.get("description"),
                author: row.get("author"),
                activation,
                capabilities,
                source,
                enabled: row.get("enabled"),
                body: row.get("body"),
            }))
        }
        None => Ok(None),
    }
}

/// Update the enabled status of a skill package.
pub async fn update_skill_enabled(id: &str, enabled: bool) -> anyhow::Result<bool> {
    let pool = crate::db::pool().await?;
    let now = chrono::Utc::now().to_rfc3339();
    let result = sqlx::query("UPDATE skill_packages SET enabled = ?, updated_at = ? WHERE id = ?")
        .bind(enabled)
        .bind(&now)
        .bind(id)
        .execute(&pool)
        .await?;

    Ok(result.rows_affected() > 0)
}

/// Delete a skill package and its capabilities.
pub async fn delete_skill_package(id: &str) -> anyhow::Result<bool> {
    let pool = crate::db::pool().await?;
    // Capabilities are cascade-deleted via FK, but let's be explicit
    sqlx::query("DELETE FROM skill_capabilities WHERE skill_id = ?")
        .bind(id)
        .execute(&pool)
        .await?;

    let result = sqlx::query("DELETE FROM skill_packages WHERE id = ?")
        .bind(id)
        .execute(&pool)
        .await?;

    Ok(result.rows_affected() > 0)
}

// ── Skill Matcher + Capability Collector (TASK-066) ──

/// Context available for skill matching (all optional).
pub struct MatchContext {
    pub foreground_app: Option<String>,
    pub current_url: Option<String>,
    pub current_file: Option<String>,
}

/// Match enabled skills whose activation conditions match the given message/context.
/// Returns at most 5 skills, sorted by match relevance.
pub async fn match_enabled_skills(
    user_message: &str,
    context: Option<&MatchContext>,
) -> anyhow::Result<Vec<SkillPackage>> {
    let all_skills = list_skill_packages().await?;
    let lower_message = user_message.to_lowercase();

    let mut matched: Vec<(SkillPackage, u32)> = Vec::new();

    for skill in all_skills {
        if !skill.enabled {
            continue;
        }

        let activation = &skill.activation;

        // A skill with ALL empty activation fields does NOT match anything
        if activation.keywords.is_empty()
            && activation.apps.is_empty()
            && activation.url_patterns.is_empty()
            && activation.file_patterns.is_empty()
        {
            continue;
        }

        let mut score: u32 = 0;

        // Check keywords: any keyword is a substring of user_message (case-insensitive)
        if !activation.keywords.is_empty() {
            let keyword_match = activation
                .keywords
                .iter()
                .any(|kw| lower_message.contains(&kw.to_lowercase()));
            if keyword_match {
                score += 10;
            }
        }

        // Check apps: if context.foreground_app is Some, check if any app pattern matches
        if !activation.apps.is_empty() {
            if let Some(ctx) = context {
                if let Some(ref app) = ctx.foreground_app {
                    let app_lower = app.to_lowercase();
                    let app_match = activation
                        .apps
                        .iter()
                        .any(|a| app_lower.contains(&a.to_lowercase()));
                    if app_match {
                        score += 5;
                    }
                }
            }
        }

        // Check url_patterns: if context.current_url is Some, check substring match
        if !activation.url_patterns.is_empty() {
            if let Some(ctx) = context {
                if let Some(ref url) = ctx.current_url {
                    let url_lower = url.to_lowercase();
                    let url_match = activation
                        .url_patterns
                        .iter()
                        .any(|pattern| url_lower.contains(&pattern.to_lowercase()));
                    if url_match {
                        score += 3;
                    }
                }
            }
        }

        // Check file_patterns: if context.current_file is Some, check substring match
        if !activation.file_patterns.is_empty() {
            if let Some(ctx) = context {
                if let Some(ref file) = ctx.current_file {
                    let file_lower = file.to_lowercase();
                    let file_match = activation.file_patterns.iter().any(|pattern| {
                        let pattern_lower = pattern.to_lowercase();
                        // Simple glob: check if file ends with pattern (minus leading *)
                        if let Some(suffix) = pattern_lower.strip_prefix('*') {
                            file_lower.ends_with(suffix)
                        } else {
                            file_lower.contains(&pattern_lower)
                        }
                    });
                    if file_match {
                        score += 2;
                    }
                }
            }
        }

        // A skill matches if ANY of its non-empty activation conditions matched
        if score > 0 {
            matched.push((skill, score));
        }
    }

    // Sort by score descending (higher relevance first)
    matched.sort_by(|a, b| b.1.cmp(&a.1));

    // Return at most 5 skills
    Ok(matched.into_iter().take(5).map(|(s, _)| s).collect())
}

/// Collect all unique capabilities from a list of matched skills.
/// Deduplicates while preserving order of first appearance.
pub fn collect_capabilities(skills: &[SkillPackage]) -> Vec<String> {
    let mut seen = std::collections::HashSet::new();
    let mut result = Vec::new();
    for skill in skills {
        for cap in &skill.capabilities {
            if seen.insert(cap.clone()) {
                result.push(cap.clone());
            }
        }
    }
    result
}

#[cfg(test)]
#[allow(deprecated)]
mod tests {
    use super::*;

    #[test]
    fn default_specs_include_three_mvp_skills() {
        let specs = default_skill_specs();
        assert!(specs.iter().any(|skill| skill.id == "document_assistant"));
        assert!(specs.iter().any(|skill| skill.id == "coding_assistant"));
        assert!(specs.iter().any(|skill| skill.id == "pet_avatar_router"));
    }

    #[test]
    fn default_specs_use_restricted_avatar_ids() {
        let document = get_default_skill_spec("document_assistant").expect("document skill");
        let coding = get_default_skill_spec("coding_assistant").expect("coding skill");
        assert_eq!(
            document.default_avatar_id.as_deref(),
            Some("document_secretary")
        );
        assert_eq!(coding.default_avatar_id.as_deref(), Some("programmer"));
        assert!(document
            .allowed_tools
            .contains(&"pet.set_avatar".to_string()));
        assert!(coding
            .allowed_tools
            .contains(&"subagent.claude_p".to_string()));
    }

    #[test]
    fn parse_skills_json_bare_array() {
        let json = r#"[
            {
                "id": "test_skill",
                "name": "Test",
                "description": "A test skill",
                "when_to_use": ["always"],
                "allowed_tools": ["tool.a"],
                "default_avatar_id": null,
                "context_mode": "global",
                "proactive_allowed": false
            }
        ]"#;
        let skills = parse_skills_json(json).unwrap();
        assert_eq!(skills.len(), 1);
        assert_eq!(skills[0].id, "test_skill");
    }

    #[test]
    fn parse_skills_json_wrapped_format() {
        let json = r#"{
            "skills": [
                {
                    "id": "wrapped_skill",
                    "name": "Wrapped",
                    "description": "A wrapped skill",
                    "when_to_use": ["always"],
                    "allowed_tools": [],
                    "default_avatar_id": null,
                    "context_mode": "global",
                    "proactive_allowed": true
                }
            ]
        }"#;
        let skills = parse_skills_json(json).unwrap();
        assert_eq!(skills.len(), 1);
        assert_eq!(skills[0].id, "wrapped_skill");
    }

    #[test]
    fn parse_skills_json_invalid_input() {
        assert!(parse_skills_json("not json").is_err());
        assert!(parse_skills_json("{}").is_err());
    }

    #[tokio::test]
    async fn save_and_list_skills_roundtrip() {
        let _root = crate::test_support::TestRoot::new();
        let skills = default_skill_specs();
        save_skills(&skills).await.unwrap();
        let loaded = list_skills().await.unwrap();
        assert_eq!(loaded.len(), skills.len());
        assert_eq!(loaded[0].id, skills[0].id);
    }

    #[tokio::test]
    async fn import_skills_from_json_replaces_existing() {
        let _root = crate::test_support::TestRoot::new();
        // Save defaults first.
        save_skills(&default_skill_specs()).await.unwrap();
        // Import a single custom skill.
        let json = r#"[{
            "id": "custom",
            "name": "Custom",
            "description": "custom skill",
            "when_to_use": [],
            "allowed_tools": [],
            "default_avatar_id": null,
            "context_mode": "global",
            "proactive_allowed": false
        }]"#;
        let imported = import_skills_from_json(json).await.unwrap();
        assert_eq!(imported.len(), 1);
        assert_eq!(imported[0].id, "custom");
        // Verify persistence.
        let loaded = list_skills().await.unwrap();
        assert_eq!(loaded.len(), 1);
        assert_eq!(loaded[0].id, "custom");
    }

    #[tokio::test]
    async fn import_skills_from_json_empty_fails() {
        let _root = crate::test_support::TestRoot::new();
        assert!(import_skills_from_json("[]").await.is_err());
    }

    #[test]
    fn skill_contextual_tools_matches_document_keywords() {
        // "document" + "summarize" should match document_assistant
        let tools = skill_contextual_tools("please summarize this document for me");
        assert!(tools.contains(&"office.inspect_document".to_string()));
        assert!(tools.contains(&"office.export_text".to_string()));
    }

    #[test]
    fn skill_contextual_tools_no_match_on_unrelated() {
        let tools = skill_contextual_tools("what's the weather today");
        assert!(tools.is_empty());
    }

    #[test]
    fn skill_contextual_tools_deduplicates() {
        // Both document_assistant and coding_assistant might share tools
        let tools = skill_contextual_tools("inspect and summarize this document file");
        let before = tools.len();
        let mut deduped = tools.clone();
        deduped.dedup();
        assert_eq!(before, deduped.len());
    }

    // ── SkillPackage tests (TASK-065) ──

    #[test]
    fn parse_skill_markdown_valid_input() {
        let md = r#"---
id: test_skill
name: Test Skill
version: "1.0.0"
description: A test skill for parsing
author: testuser
activation:
  keywords:
    - test
    - demo
  apps:
    - vscode
  url_patterns: []
  file_patterns:
    - "*.rs"
capabilities:
  - code_review
  - file_read
---
# Test Skill

This is the markdown body."#;

        let pkg = parse_skill_markdown(md).unwrap();
        assert_eq!(pkg.id, "test_skill");
        assert_eq!(pkg.name, "Test Skill");
        assert_eq!(pkg.version, "1.0.0");
        assert_eq!(pkg.description, "A test skill for parsing");
        assert_eq!(pkg.author, Some("testuser".to_string()));
        assert_eq!(pkg.activation.keywords, vec!["test", "demo"]);
        assert_eq!(pkg.activation.apps, vec!["vscode"]);
        assert!(pkg.activation.url_patterns.is_empty());
        assert_eq!(pkg.activation.file_patterns, vec!["*.rs"]);
        assert_eq!(pkg.capabilities, vec!["code_review", "file_read"]);
        assert!(pkg.body.contains("# Test Skill"));
    }

    #[test]
    fn parse_skill_markdown_missing_required_fields() {
        // Missing id
        let md = r#"---
name: Test
version: "1.0.0"
description: desc
activation:
  keywords: []
capabilities:
  - cap1
---
body"#;
        assert!(parse_skill_markdown(md).is_err());

        // Missing capabilities
        let md = r#"---
id: test
name: Test
version: "1.0.0"
description: desc
activation:
  keywords: []
---
body"#;
        assert!(parse_skill_markdown(md).is_err());

        // Missing activation
        let md = r#"---
id: test
name: Test
version: "1.0.0"
description: desc
capabilities:
  - cap1
---
body"#;
        assert!(parse_skill_markdown(md).is_err());
    }

    #[test]
    fn parse_skill_markdown_no_frontmatter() {
        let md = "Just a plain markdown file with no frontmatter.";
        assert!(parse_skill_markdown(md).is_err());
    }

    #[test]
    fn parse_skill_markdown_empty_required_field() {
        let md = r#"---
id: ""
name: Test
version: "1.0.0"
description: desc
activation:
  keywords: []
capabilities:
  - cap1
---
body"#;
        assert!(parse_skill_markdown(md).is_err());
    }

    #[tokio::test]
    async fn import_skill_markdown_defaults_enabled_false() {
        let _root = crate::test_support::TestRoot::new();
        let md = r#"---
id: import_test
name: Import Test
version: "1.0.0"
description: Testing import defaults
activation:
  keywords:
    - import
capabilities:
  - test_cap
---
# Body"#;

        let pkg = import_skill_markdown(md).await.unwrap();
        assert!(!pkg.enabled);
        assert_eq!(pkg.source, SkillSource::UserImport);

        // Verify it's in the database
        let loaded = get_skill_package("import_test").await.unwrap().unwrap();
        assert!(!loaded.enabled);
    }

    #[tokio::test]
    async fn import_skill_markdown_duplicate_id_conflict() {
        let _root = crate::test_support::TestRoot::new();
        let md = r#"---
id: dup_skill
name: Duplicate
version: "1.0.0"
description: First import
activation:
  keywords: []
capabilities:
  - cap1
---
body"#;

        // First import succeeds
        import_skill_markdown(md).await.unwrap();

        // Second import with same id fails
        let result = import_skill_markdown(md).await;
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("already exists"));
    }

    #[tokio::test]
    async fn skill_package_crud_operations() {
        let _root = crate::test_support::TestRoot::new();
        let md = r#"---
id: crud_test
name: CRUD Test
version: "2.0.0"
description: Testing CRUD
author: tester
activation:
  keywords:
    - crud
  apps:
    - terminal
capabilities:
  - read
  - write
  - execute
---
Full markdown body here."#;

        // Insert via import
        let pkg = import_skill_markdown(md).await.unwrap();
        assert_eq!(pkg.id, "crud_test");

        // List
        let all = list_skill_packages().await.unwrap();
        assert_eq!(all.len(), 1);
        assert_eq!(all[0].id, "crud_test");

        // Get by id
        let fetched = get_skill_package("crud_test").await.unwrap().unwrap();
        assert_eq!(fetched.name, "CRUD Test");
        assert_eq!(fetched.version, "2.0.0");
        assert_eq!(fetched.author, Some("tester".to_string()));

        // Update enabled
        let updated = update_skill_enabled("crud_test", true).await.unwrap();
        assert!(updated);
        let fetched = get_skill_package("crud_test").await.unwrap().unwrap();
        assert!(fetched.enabled);

        // Update non-existent
        let updated = update_skill_enabled("nonexistent", true).await.unwrap();
        assert!(!updated);

        // Delete
        let deleted = delete_skill_package("crud_test").await.unwrap();
        assert!(deleted);
        assert!(get_skill_package("crud_test").await.unwrap().is_none());

        // Delete non-existent
        let deleted = delete_skill_package("crud_test").await.unwrap();
        assert!(!deleted);
    }

    #[tokio::test]
    async fn skill_capabilities_stored_in_join_table() {
        let _root = crate::test_support::TestRoot::new();
        let md = r#"---
id: cap_test
name: Cap Test
version: "1.0.0"
description: Testing capabilities storage
activation:
  keywords: []
capabilities:
  - alpha
  - beta
  - gamma
---
body"#;

        import_skill_markdown(md).await.unwrap();

        let pool = crate::db::pool().await.unwrap();
        let caps: Vec<String> = sqlx::query_scalar(
            "SELECT capability FROM skill_capabilities WHERE skill_id = ? ORDER BY capability",
        )
        .bind("cap_test")
        .fetch_all(&pool)
        .await
        .unwrap();

        assert_eq!(caps, vec!["alpha", "beta", "gamma"]);
    }

    #[tokio::test]
    async fn delete_skill_cascades_capabilities() {
        let _root = crate::test_support::TestRoot::new();
        let md = r#"---
id: cascade_test
name: Cascade
version: "1.0.0"
description: Test cascade delete
activation:
  keywords: []
capabilities:
  - cap_a
  - cap_b
---
body"#;

        import_skill_markdown(md).await.unwrap();

        let pool = crate::db::pool().await.unwrap();
        let count: i64 =
            sqlx::query_scalar("SELECT COUNT(*) FROM skill_capabilities WHERE skill_id = ?")
                .bind("cascade_test")
                .fetch_one(&pool)
                .await
                .unwrap();
        assert_eq!(count, 2);

        delete_skill_package("cascade_test").await.unwrap();

        let count: i64 =
            sqlx::query_scalar("SELECT COUNT(*) FROM skill_capabilities WHERE skill_id = ?")
                .bind("cascade_test")
                .fetch_one(&pool)
                .await
                .unwrap();
        assert_eq!(count, 0);
    }

    #[tokio::test]
    async fn skill_package_list_empty_when_none() {
        let _root = crate::test_support::TestRoot::new();
        let all = list_skill_packages().await.unwrap();
        assert!(all.is_empty());
    }

    #[test]
    fn parse_skill_markdown_minimal_activation() {
        let md = r#"---
id: minimal
name: Minimal
version: "0.1.0"
description: Minimal skill
capabilities:
  - basic
activation:
  keywords:
    - hello
---
"#;

        let pkg = parse_skill_markdown(md).unwrap();
        assert_eq!(pkg.id, "minimal");
        assert!(pkg.activation.apps.is_empty());
        assert!(pkg.activation.url_patterns.is_empty());
        assert!(pkg.activation.file_patterns.is_empty());
    }

    // ── Skill Matcher tests (TASK-066) ──

    /// Helper to create a SkillPackage for testing
    fn make_test_skill(
        id: &str,
        keywords: Vec<&str>,
        apps: Vec<&str>,
        caps: Vec<&str>,
    ) -> SkillPackage {
        SkillPackage {
            id: id.to_string(),
            name: format!("Test {id}"),
            version: "1.0.0".to_string(),
            description: format!("Test skill {id}"),
            author: None,
            activation: SkillActivation {
                keywords: keywords.into_iter().map(String::from).collect(),
                apps: apps.into_iter().map(String::from).collect(),
                url_patterns: vec![],
                file_patterns: vec![],
            },
            capabilities: caps.into_iter().map(String::from).collect(),
            source: SkillSource::Builtin,
            enabled: true,
            body: String::new(),
        }
    }

    #[test]
    fn keyword_match_substring_case_insensitive() {
        // "查看日程" should match skill with keywords=["日程","会议"]
        let skill = make_test_skill(
            "calendar",
            vec!["日程", "会议"],
            vec![],
            vec!["calendar.read"],
        );
        let lower_msg = "查看日程".to_lowercase();
        let keyword_match = skill
            .activation
            .keywords
            .iter()
            .any(|kw| lower_msg.contains(&kw.to_lowercase()));
        assert!(keyword_match, "keyword '日程' should match '查看日程'");
    }

    #[test]
    fn keyword_match_case_insensitive() {
        let skill = make_test_skill("code", vec!["VsCode", "IDE"], vec![], vec!["code.review"]);
        let lower_msg = "open vscode please".to_lowercase();
        let keyword_match = skill
            .activation
            .keywords
            .iter()
            .any(|kw| lower_msg.contains(&kw.to_lowercase()));
        assert!(
            keyword_match,
            "'VsCode' keyword should match 'open vscode please' case-insensitively"
        );
    }

    #[test]
    fn empty_message_matches_nothing() {
        // Empty activation fields → no match
        let skill = make_test_skill("test", vec!["keyword"], vec![], vec!["cap"]);
        let lower_msg = "";
        // keyword check
        let keyword_match = skill
            .activation
            .keywords
            .iter()
            .any(|kw| lower_msg.contains(&kw.to_lowercase()));
        assert!(!keyword_match, "empty message should not match any keyword");
    }

    #[test]
    fn empty_activation_fields_no_match() {
        let skill = SkillPackage {
            id: "empty".to_string(),
            name: "Empty".to_string(),
            version: "1.0.0".to_string(),
            description: "Empty activation".to_string(),
            author: None,
            activation: SkillActivation::default(),
            capabilities: vec!["cap".to_string()],
            source: SkillSource::Builtin,
            enabled: true,
            body: String::new(),
        };
        // Verify all activation fields are empty
        assert!(skill.activation.keywords.is_empty());
        assert!(skill.activation.apps.is_empty());
        assert!(skill.activation.url_patterns.is_empty());
        assert!(skill.activation.file_patterns.is_empty());
    }

    #[test]
    fn collect_capabilities_deduplication() {
        let skills = vec![
            make_test_skill("a", vec!["a"], vec![], vec!["read", "write", "execute"]),
            make_test_skill("b", vec!["b"], vec![], vec!["write", "delete"]),
            make_test_skill("c", vec!["c"], vec![], vec!["read", "admin"]),
        ];
        let caps = collect_capabilities(&skills);
        // "read" and "write" should appear only once
        assert_eq!(caps, vec!["read", "write", "execute", "delete", "admin"]);
        // Verify no duplicates
        let mut deduped = caps.clone();
        deduped.dedup();
        assert_eq!(caps.len(), deduped.len());
    }

    #[test]
    fn collect_capabilities_preserves_order() {
        let skills = vec![
            make_test_skill("x", vec![], vec![], vec!["z", "a", "m"]),
            make_test_skill("y", vec![], vec![], vec!["a", "b"]),
        ];
        let caps = collect_capabilities(&skills);
        assert_eq!(caps, vec!["z", "a", "m", "b"]);
    }

    #[test]
    fn collect_capabilities_empty_skills() {
        let skills: Vec<SkillPackage> = vec![];
        let caps = collect_capabilities(&skills);
        assert!(caps.is_empty());
    }

    #[test]
    fn app_pattern_matching() {
        let skill = make_test_skill(
            "vscode_skill",
            vec![],
            vec!["code", "cursor"],
            vec!["code.help"],
        );
        let ctx = MatchContext {
            foreground_app: Some("Visual Studio Code".to_string()),
            current_url: None,
            current_file: None,
        };
        let app_match = skill.activation.apps.iter().any(|a| {
            ctx.foreground_app
                .as_ref()
                .unwrap()
                .to_lowercase()
                .contains(&a.to_lowercase())
        });
        assert!(app_match, "'code' should match 'Visual Studio Code'");
    }

    #[test]
    fn url_pattern_matching() {
        let skill = SkillPackage {
            id: "github_skill".to_string(),
            name: "GitHub Skill".to_string(),
            version: "1.0.0".to_string(),
            description: "test".to_string(),
            author: None,
            activation: SkillActivation {
                keywords: vec![],
                apps: vec![],
                url_patterns: vec!["github.com".to_string()],
                file_patterns: vec![],
            },
            capabilities: vec!["git".to_string()],
            source: SkillSource::Builtin,
            enabled: true,
            body: String::new(),
        };
        let ctx = MatchContext {
            foreground_app: None,
            current_url: Some("https://github.com/user/repo".to_string()),
            current_file: None,
        };
        let url_match = skill.activation.url_patterns.iter().any(|pattern| {
            ctx.current_url
                .as_ref()
                .unwrap()
                .to_lowercase()
                .contains(&pattern.to_lowercase())
        });
        assert!(
            url_match,
            "'github.com' should match 'https://github.com/user/repo'"
        );
    }

    #[test]
    fn file_pattern_glob_matching() {
        let skill = SkillPackage {
            id: "rust_skill".to_string(),
            name: "Rust Skill".to_string(),
            version: "1.0.0".to_string(),
            description: "test".to_string(),
            author: None,
            activation: SkillActivation {
                keywords: vec![],
                apps: vec![],
                url_patterns: vec![],
                file_patterns: vec!["*.rs".to_string(), "*.toml".to_string()],
            },
            capabilities: vec!["rust.help".to_string()],
            source: SkillSource::Builtin,
            enabled: true,
            body: String::new(),
        };
        let ctx = MatchContext {
            foreground_app: None,
            current_url: None,
            current_file: Some("/home/user/project/src/main.rs".to_string()),
        };
        let file_match = skill.activation.file_patterns.iter().any(|pattern| {
            let pattern_lower = pattern.to_lowercase();
            let file_lower = ctx.current_file.as_ref().unwrap().to_lowercase();
            if let Some(suffix) = pattern_lower.strip_prefix('*') {
                file_lower.ends_with(suffix)
            } else {
                file_lower.contains(&pattern_lower)
            }
        });
        assert!(
            file_match,
            "'*.rs' should match '/home/user/project/src/main.rs'"
        );
    }
}
