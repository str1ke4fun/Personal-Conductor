pub mod lark;

use serde::{Deserialize, Serialize};
use sqlx::Row;

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum ConnectorImplementation {
    NativeRust,
    LocalCli,
    McpServer,
    HttpApi,
}

impl ConnectorImplementation {
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::NativeRust => "native_rust",
            Self::LocalCli => "local_cli",
            Self::McpServer => "mcp_server",
            Self::HttpApi => "http_api",
        }
    }

    pub fn from_str(s: &str) -> anyhow::Result<Self> {
        match s {
            "native_rust" => Ok(Self::NativeRust),
            "local_cli" => Ok(Self::LocalCli),
            "mcp_server" => Ok(Self::McpServer),
            "http_api" => Ok(Self::HttpApi),
            other => anyhow::bail!("unknown connector implementation: {other}"),
        }
    }
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum ConnectorAuthStatus {
    NotConfigured,
    Authenticated,
    Expired,
    Failed,
}

impl ConnectorAuthStatus {
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::NotConfigured => "not_configured",
            Self::Authenticated => "authenticated",
            Self::Expired => "expired",
            Self::Failed => "failed",
        }
    }

    pub fn from_str(s: &str) -> anyhow::Result<Self> {
        match s {
            "not_configured" => Ok(Self::NotConfigured),
            "authenticated" => Ok(Self::Authenticated),
            "expired" => Ok(Self::Expired),
            "failed" => Ok(Self::Failed),
            other => anyhow::bail!("unknown connector auth status: {other}"),
        }
    }
}

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct ConnectorCapability {
    pub capability: String,
    pub tools: Vec<String>,
    pub risk_level: String,
    pub requires_confirmation: bool,
}

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct ConnectorSpec {
    pub id: String,
    pub name: String,
    pub description: String,
    pub implementation_type: ConnectorImplementation,
    pub capabilities: Vec<ConnectorCapability>,
    pub auth_status: ConnectorAuthStatus,
    pub enabled: bool,
    pub config_json: Option<String>,
}

/// Registry that maps capabilities to tools with risk levels.
pub struct ConnectorRegistry;

impl ConnectorRegistry {
    /// Register a connector: upsert into `connectors` and `capability_grants`.
    pub async fn register(connector: ConnectorSpec) -> anyhow::Result<()> {
        let pool = crate::db::pool().await?;
        let now = chrono::Utc::now().to_rfc3339();

        sqlx::query(
            r#"INSERT INTO connectors
              (id, name, description, implementation_type, auth_status, enabled, config_json, created_at, updated_at)
              VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?)
              ON CONFLICT(id) DO UPDATE SET
                name = excluded.name,
                description = excluded.description,
                implementation_type = excluded.implementation_type,
                auth_status = excluded.auth_status,
                enabled = excluded.enabled,
                config_json = excluded.config_json,
                updated_at = excluded.updated_at"#,
        )
        .bind(&connector.id)
        .bind(&connector.name)
        .bind(&connector.description)
        .bind(connector.implementation_type.as_str())
        .bind(connector.auth_status.as_str())
        .bind(connector.enabled)
        .bind(&connector.config_json)
        .bind(&now)
        .bind(&now)
        .execute(&pool)
        .await?;

        // Remove old capability grants for this connector before re-inserting.
        sqlx::query("DELETE FROM capability_grants WHERE connector_id = ?")
            .bind(&connector.id)
            .execute(&pool)
            .await?;

        for cap in &connector.capabilities {
            let tools_json = serde_json::to_string(&cap.tools)?;
            sqlx::query(
                r#"INSERT INTO capability_grants
                  (connector_id, capability, tools_json, risk_level, requires_confirmation)
                  VALUES (?, ?, ?, ?, ?)"#,
            )
            .bind(&connector.id)
            .bind(&cap.capability)
            .bind(&tools_json)
            .bind(&cap.risk_level)
            .bind(cap.requires_confirmation)
            .execute(&pool)
            .await?;
        }

        Ok(())
    }

    /// Look up a connector by id.
    pub async fn get(id: &str) -> anyhow::Result<Option<ConnectorSpec>> {
        let pool = crate::db::pool().await?;

        let row = sqlx::query(
            "SELECT id, name, description, implementation_type, auth_status, enabled, config_json FROM connectors WHERE id = ?",
        )
        .bind(id)
        .fetch_optional(&pool)
        .await?;

        match row {
            None => Ok(None),
            Some(row) => {
                let connector_id: String = row.get("id");
                let caps = Self::fetch_capabilities(&pool, &connector_id).await?;
                Ok(Some(ConnectorSpec {
                    id: connector_id,
                    name: row.get("name"),
                    description: row.get("description"),
                    implementation_type: ConnectorImplementation::from_str(
                        &row.get::<String, _>("implementation_type"),
                    )?,
                    capabilities: caps,
                    auth_status: ConnectorAuthStatus::from_str(
                        &row.get::<String, _>("auth_status"),
                    )?,
                    enabled: row.get::<bool, _>("enabled"),
                    config_json: row.get("config_json"),
                }))
            }
        }
    }

    /// List all registered connectors.
    pub async fn list() -> anyhow::Result<Vec<ConnectorSpec>> {
        let pool = crate::db::pool().await?;

        let rows = sqlx::query(
            "SELECT id, name, description, implementation_type, auth_status, enabled, config_json FROM connectors ORDER BY name",
        )
        .fetch_all(&pool)
        .await?;

        let mut result = Vec::with_capacity(rows.len());
        for row in rows {
            let connector_id: String = row.get("id");
            let caps = Self::fetch_capabilities(&pool, &connector_id).await?;
            result.push(ConnectorSpec {
                id: connector_id,
                name: row.get("name"),
                description: row.get("description"),
                implementation_type: ConnectorImplementation::from_str(
                    &row.get::<String, _>("implementation_type"),
                )?,
                capabilities: caps,
                auth_status: ConnectorAuthStatus::from_str(&row.get::<String, _>("auth_status"))?,
                enabled: row.get::<bool, _>("enabled"),
                config_json: row.get("config_json"),
            });
        }

        Ok(result)
    }

    /// Find a connector that provides the given capability.
    /// Returns (connector, tools, risk_level).
    pub async fn resolve_capability(
        capability: &str,
    ) -> anyhow::Result<Option<(ConnectorSpec, Vec<String>, String)>> {
        let pool = crate::db::pool().await?;

        let row = sqlx::query(
            r#"SELECT connector_id, tools_json, risk_level
              FROM capability_grants
              WHERE capability = ?
              LIMIT 1"#,
        )
        .bind(capability)
        .fetch_optional(&pool)
        .await?;

        match row {
            None => Ok(None),
            Some(row) => {
                let connector_id: String = row.get("connector_id");
                let tools_json: String = row.get("tools_json");
                let risk_level: String = row.get("risk_level");
                let tools: Vec<String> = serde_json::from_str(&tools_json)?;

                match Self::get(&connector_id).await? {
                    Some(connector) => Ok(Some((connector, tools, risk_level))),
                    None => Ok(None),
                }
            }
        }
    }

    /// Batch resolve multiple capabilities.
    /// Returns (capability, connector, tools, risk_level) tuples for each found capability.
    pub async fn resolve_capabilities(
        capabilities: &[String],
    ) -> anyhow::Result<Vec<(String, ConnectorSpec, Vec<String>, String)>> {
        let mut results = Vec::new();
        for cap in capabilities {
            if let Some((connector, tools, risk)) = Self::resolve_capability(cap).await? {
                results.push((cap.clone(), connector, tools, risk));
            }
        }
        Ok(results)
    }

    /// Delete a connector and its capability grants.
    pub async fn delete(id: &str) -> anyhow::Result<bool> {
        let pool = crate::db::pool().await?;

        sqlx::query("DELETE FROM capability_grants WHERE connector_id = ?")
            .bind(id)
            .execute(&pool)
            .await?;

        let result = sqlx::query("DELETE FROM connectors WHERE id = ?")
            .bind(id)
            .execute(&pool)
            .await?;

        Ok(result.rows_affected() > 0)
    }

    // ── internal helpers ──

    async fn fetch_capabilities(
        pool: &sqlx::SqlitePool,
        connector_id: &str,
    ) -> anyhow::Result<Vec<ConnectorCapability>> {
        let rows = sqlx::query(
            "SELECT capability, tools_json, risk_level, requires_confirmation FROM capability_grants WHERE connector_id = ?",
        )
        .bind(connector_id)
        .fetch_all(pool)
        .await?;

        let mut caps = Vec::with_capacity(rows.len());
        for row in rows {
            let tools_json: String = row.get("tools_json");
            caps.push(ConnectorCapability {
                capability: row.get("capability"),
                tools: serde_json::from_str(&tools_json)?,
                risk_level: row.get("risk_level"),
                requires_confirmation: row.get::<bool, _>("requires_confirmation"),
            });
        }
        Ok(caps)
    }
}

/// Register builtin connectors and refresh their computed auth/enabled state.
pub async fn register_builtin_connectors() -> anyhow::Result<()> {
    ConnectorRegistry::register(lark::build_lark_connector()).await?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::test_support::TestRoot;

    fn make_connector(id: &str, caps: Vec<ConnectorCapability>) -> ConnectorSpec {
        ConnectorSpec {
            id: id.to_string(),
            name: format!("Connector {id}"),
            description: format!("Test connector {id}"),
            implementation_type: ConnectorImplementation::NativeRust,
            capabilities: caps,
            auth_status: ConnectorAuthStatus::NotConfigured,
            enabled: true,
            config_json: None,
        }
    }

    fn make_cap(
        capability: &str,
        tools: &[&str],
        risk: &str,
        confirm: bool,
    ) -> ConnectorCapability {
        ConnectorCapability {
            capability: capability.to_string(),
            tools: tools.iter().map(|t| t.to_string()).collect(),
            risk_level: risk.to_string(),
            requires_confirmation: confirm,
        }
    }

    #[tokio::test]
    async fn register_and_retrieve_connector() {
        let _root = TestRoot::new();
        let connector = make_connector(
            "test-1",
            vec![make_cap("file.read", &["fs.read_file"], "low", false)],
        );
        ConnectorRegistry::register(connector.clone())
            .await
            .unwrap();

        let got = ConnectorRegistry::get("test-1")
            .await
            .unwrap()
            .expect("found");
        assert_eq!(got.id, "test-1");
        assert_eq!(got.name, "Connector test-1");
        assert_eq!(got.implementation_type, ConnectorImplementation::NativeRust);
        assert_eq!(got.auth_status, ConnectorAuthStatus::NotConfigured);
        assert!(got.enabled);
        assert_eq!(got.capabilities.len(), 1);
        assert_eq!(got.capabilities[0].capability, "file.read");
    }

    #[tokio::test]
    async fn resolve_known_capability_returns_correct_connector_and_tools() {
        let _root = TestRoot::new();
        let connector = make_connector(
            "resolver-1",
            vec![make_cap(
                "web.search",
                &["web.search_api", "web.scrape"],
                "medium",
                true,
            )],
        );
        ConnectorRegistry::register(connector).await.unwrap();

        let (conn, tools, risk) = ConnectorRegistry::resolve_capability("web.search")
            .await
            .unwrap()
            .expect("resolved");
        assert_eq!(conn.id, "resolver-1");
        assert_eq!(tools, vec!["web.search_api", "web.scrape"]);
        assert_eq!(risk, "medium");
    }

    #[tokio::test]
    async fn resolve_unknown_capability_returns_none() {
        let _root = TestRoot::new();
        let result = ConnectorRegistry::resolve_capability("nonexistent.cap")
            .await
            .unwrap();
        assert!(result.is_none());
    }

    #[tokio::test]
    async fn multiple_connectors_with_different_capabilities() {
        let _root = TestRoot::new();

        let c1 = make_connector(
            "multi-1",
            vec![make_cap("email.send", &["smtp.send"], "high", true)],
        );
        let c2 = make_connector(
            "multi-2",
            vec![make_cap(
                "calendar.read",
                &["cal.list_events"],
                "low",
                false,
            )],
        );
        ConnectorRegistry::register(c1).await.unwrap();
        ConnectorRegistry::register(c2).await.unwrap();

        let list = ConnectorRegistry::list().await.unwrap();
        assert_eq!(list.len(), 2);

        let (conn_email, _, _) = ConnectorRegistry::resolve_capability("email.send")
            .await
            .unwrap()
            .expect("email");
        assert_eq!(conn_email.id, "multi-1");

        let (conn_cal, _, _) = ConnectorRegistry::resolve_capability("calendar.read")
            .await
            .unwrap()
            .expect("cal");
        assert_eq!(conn_cal.id, "multi-2");
    }

    #[tokio::test]
    async fn risk_level_mapping_is_preserved() {
        let _root = TestRoot::new();
        let connector = make_connector(
            "risk-1",
            vec![
                make_cap("safe.action", &["tool.a"], "low", false),
                make_cap("moderate.action", &["tool.b"], "medium", true),
                make_cap("dangerous.action", &["tool.c"], "high", true),
            ],
        );
        ConnectorRegistry::register(connector).await.unwrap();

        let (_, _, risk_low) = ConnectorRegistry::resolve_capability("safe.action")
            .await
            .unwrap()
            .expect("low");
        assert_eq!(risk_low, "low");

        let (_, _, risk_med) = ConnectorRegistry::resolve_capability("moderate.action")
            .await
            .unwrap()
            .expect("medium");
        assert_eq!(risk_med, "medium");

        let (_, _, risk_high) = ConnectorRegistry::resolve_capability("dangerous.action")
            .await
            .unwrap()
            .expect("high");
        assert_eq!(risk_high, "high");
    }

    #[tokio::test]
    async fn delete_connector_removes_it_and_caps() {
        let _root = TestRoot::new();
        let connector = make_connector(
            "del-1",
            vec![make_cap("temp.cap", &["temp.tool"], "low", false)],
        );
        ConnectorRegistry::register(connector).await.unwrap();
        assert!(ConnectorRegistry::get("del-1").await.unwrap().is_some());

        let deleted = ConnectorRegistry::delete("del-1").await.unwrap();
        assert!(deleted);
        assert!(ConnectorRegistry::get("del-1").await.unwrap().is_none());
        assert!(ConnectorRegistry::resolve_capability("temp.cap")
            .await
            .unwrap()
            .is_none());
    }

    #[tokio::test]
    async fn register_upsert_updates_existing_connector() {
        let _root = TestRoot::new();
        let c1 = make_connector(
            "upsert-1",
            vec![make_cap("old.cap", &["old.tool"], "low", false)],
        );
        ConnectorRegistry::register(c1).await.unwrap();

        // Update with different capabilities.
        let c2 = ConnectorSpec {
            id: "upsert-1".to_string(),
            name: "Updated Connector".to_string(),
            description: "Updated description".to_string(),
            implementation_type: ConnectorImplementation::McpServer,
            capabilities: vec![make_cap("new.cap", &["new.tool"], "high", true)],
            auth_status: ConnectorAuthStatus::Authenticated,
            enabled: false,
            config_json: Some(r#"{"key":"value"}"#.to_string()),
        };
        ConnectorRegistry::register(c2).await.unwrap();

        let got = ConnectorRegistry::get("upsert-1")
            .await
            .unwrap()
            .expect("found");
        assert_eq!(got.name, "Updated Connector");
        assert_eq!(got.implementation_type, ConnectorImplementation::McpServer);
        assert_eq!(got.auth_status, ConnectorAuthStatus::Authenticated);
        assert!(!got.enabled);
        assert_eq!(got.config_json.as_deref(), Some(r#"{"key":"value"}"#));
        assert_eq!(got.capabilities.len(), 1);
        assert_eq!(got.capabilities[0].capability, "new.cap");

        // Old capability should be gone.
        assert!(ConnectorRegistry::resolve_capability("old.cap")
            .await
            .unwrap()
            .is_none());
    }

    #[tokio::test]
    async fn batch_resolve_capabilities() {
        let _root = TestRoot::new();
        let connector = make_connector(
            "batch-1",
            vec![
                make_cap("cap.a", &["tool.a1"], "low", false),
                make_cap("cap.b", &["tool.b1", "tool.b2"], "medium", true),
            ],
        );
        ConnectorRegistry::register(connector).await.unwrap();

        let results = ConnectorRegistry::resolve_capabilities(&[
            "cap.a".to_string(),
            "cap.b".to_string(),
            "cap.missing".to_string(),
        ])
        .await
        .unwrap();

        assert_eq!(results.len(), 2);
        assert_eq!(results[0].0, "cap.a");
        assert_eq!(results[0].2, vec!["tool.a1"]);
        assert_eq!(results[1].0, "cap.b");
        assert_eq!(results[1].2, vec!["tool.b1", "tool.b2"]);
    }

    #[tokio::test]
    async fn register_builtin_connectors_registers_lark() {
        let _root = TestRoot::new();
        register_builtin_connectors()
            .await
            .expect("register builtin connectors");

        let connector = ConnectorRegistry::get("lark")
            .await
            .expect("query lark connector")
            .expect("lark connector registered");

        assert_eq!(connector.id, "lark");
        assert!(!connector.capabilities.is_empty());
        assert!(connector
            .capabilities
            .iter()
            .any(|capability| capability.capability == "lark.doc"));
    }
}
