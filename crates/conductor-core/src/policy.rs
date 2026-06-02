use std::collections::HashMap;

use crate::connectors::{ConnectorAuthStatus, ConnectorRegistry};
use crate::skills::SkillPackage;

/// The 9 policy checks for tool exposure.
pub struct PolicyEngine;

/// Result of a policy check for a single capability.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PolicyResult {
    pub capability: String,
    pub allowed_tools: Vec<String>,
    pub risk_level: String,
    pub requires_confirmation: bool,
    /// None if allowed, Some(reason) if denied.
    pub reason: Option<String>,
}

impl PolicyEngine {
    /// Filter tools through all 9 policy checks.
    ///
    /// Policies 1-3 are already handled by the caller:
    ///   1. Skill enabled (match_enabled_skills filters this)
    ///   2. Skill activation matched (match_enabled_skills filters this)
    ///   3. Skill requested capability (collect_capabilities gathers these)
    ///
    /// This function checks policies 4-9:
    ///   4. ConnectorRegistry has capability
    ///   5. Connector enabled
    ///   6. Connector auth valid (Authenticated)
    ///   7. User authorized capability
    ///   8. Risk policy allows (low/medium ok, high needs confirmation flag)
    ///   9. Confirmation policy (if requires_confirmation, user must have authorized)
    ///
    /// Returns only capabilities that pass **all** checks.
    pub async fn filter_tools(
        matched_skills: &[SkillPackage],
        _connector_registry: &ConnectorRegistry,
        capability_grants: &[(String, bool)],
    ) -> anyhow::Result<Vec<PolicyResult>> {
        // Collect unique capabilities from matched skills (policy 3).
        let capabilities = crate::skills::collect_capabilities(matched_skills);

        // Build a lookup for user authorization (policy 7).
        let auth_map: HashMap<&str, bool> = capability_grants
            .iter()
            .map(|(cap, auth)| (cap.as_str(), *auth))
            .collect();

        let mut results = Vec::new();

        for cap in capabilities {
            // Policy 4: ConnectorRegistry has capability.
            let resolved = ConnectorRegistry::resolve_capability(&cap).await?;
            let (connector, tools, risk_level) = match resolved {
                Some(r) => r,
                None => {
                    results.push(PolicyResult {
                        capability: cap,
                        allowed_tools: vec![],
                        risk_level: "unknown".to_string(),
                        requires_confirmation: false,
                        reason: Some("capability not found in connector registry".to_string()),
                    });
                    continue;
                }
            };

            // Policy 5: Connector enabled.
            if !connector.enabled {
                results.push(PolicyResult {
                    capability: cap,
                    allowed_tools: vec![],
                    risk_level: risk_level.clone(),
                    requires_confirmation: false,
                    reason: Some("connector is disabled".to_string()),
                });
                continue;
            }

            // Policy 6: Connector auth valid.
            //
            // Soft exposure rule for document/connectivity workflows:
            // if the connector is installed/enabled but not yet authenticated,
            // keep the tool definitions available so the UI/agent can surface
            // the capability together with the connector auth state. Actual
            // execution will still fail fast until the connector is logged in.
            let auth_ready = connector.auth_status == ConnectorAuthStatus::Authenticated;

            // Policy 7: User authorized capability.
            let user_authorized = auth_map.get(cap.as_str()).copied().unwrap_or(false);
            if !user_authorized {
                results.push(PolicyResult {
                    capability: cap,
                    allowed_tools: vec![],
                    risk_level: risk_level.clone(),
                    requires_confirmation: false,
                    reason: Some("user not authorized for this capability".to_string()),
                });
                continue;
            }

            // Look up the ConnectorCapability to get requires_confirmation.
            let connector_cap = connector.capabilities.iter().find(|c| c.capability == cap);
            let requires_confirmation = connector_cap
                .map(|c| c.requires_confirmation)
                .unwrap_or(false);

            // Policy 8: Risk policy allows.
            // low/medium are always allowed; high requires the connector to
            // declare requires_confirmation = true.
            if risk_level == "high" && !requires_confirmation {
                results.push(PolicyResult {
                    capability: cap,
                    allowed_tools: vec![],
                    risk_level: risk_level.clone(),
                    requires_confirmation: true,
                    reason: Some("high risk capability requires confirmation flag".to_string()),
                });
                continue;
            }

            // Policy 9: Confirmation policy.
            // If the capability requires confirmation, the user must have
            // authorized it (already checked in policy 7, so this only fires
            // when the connector demands confirmation but the grant is missing).
            if requires_confirmation && !user_authorized {
                results.push(PolicyResult {
                    capability: cap,
                    allowed_tools: vec![],
                    risk_level: risk_level.clone(),
                    requires_confirmation: true,
                    reason: Some("capability requires user confirmation".to_string()),
                });
                continue;
            }

            // All 9 checks passed.
            results.push(PolicyResult {
                capability: cap,
                allowed_tools: tools,
                risk_level,
                requires_confirmation,
                reason: if auth_ready {
                    None
                } else {
                    Some(format!(
                        "connector auth status is '{}'",
                        connector.auth_status.as_str()
                    ))
                },
            });
        }

        Ok(results)
    }

    /// Authorize concrete tool ids discovered at runtime.
    ///
    /// Tools backed by connectors are exposed when their connector is enabled.
    /// If the connector is not authenticated yet, downstream execution is
    /// expected to surface that state to the user.
    pub async fn authorize_tool_ids(tool_ids: &[String]) -> anyhow::Result<Vec<String>> {
        let requested: Vec<String> = tool_ids
            .iter()
            .map(|tool_id| tool_id.trim())
            .filter(|tool_id| !tool_id.is_empty())
            .map(str::to_string)
            .collect();
        if requested.is_empty() {
            return Ok(vec![]);
        }

        let mut allowed = Vec::new();
        let connectors = ConnectorRegistry::list().await?;

        for tool_id in requested {
            let mut connector_match = None;
            for connector in &connectors {
                if let Some(capability) = connector.capabilities.iter().find(|capability| {
                    capability
                        .tools
                        .iter()
                        .any(|candidate| candidate == &tool_id)
                }) {
                    connector_match = Some((connector, capability));
                    break;
                }
            }

            match connector_match {
                Some((connector, capability)) => {
                    if !connector.enabled {
                        continue;
                    }
                    if capability.risk_level == "high" && !capability.requires_confirmation {
                        continue;
                    }
                    allowed.push(tool_id);
                }
                None => allowed.push(tool_id),
            }
        }

        allowed.sort();
        allowed.dedup();
        Ok(allowed)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::connectors::{
        ConnectorAuthStatus, ConnectorCapability, ConnectorImplementation, ConnectorSpec,
    };
    use crate::skills::{SkillActivation, SkillPackage, SkillSource};
    use crate::test_support::TestRoot;

    // ── helpers ──

    fn make_skill(id: &str, caps: Vec<&str>) -> SkillPackage {
        SkillPackage {
            id: id.to_string(),
            name: format!("Skill {id}"),
            version: "1.0.0".to_string(),
            description: format!("test skill {id}"),
            author: None,
            activation: SkillActivation {
                keywords: vec!["test".to_string()],
                apps: vec![],
                url_patterns: vec![],
                file_patterns: vec![],
            },
            capabilities: caps.into_iter().map(String::from).collect(),
            source: SkillSource::Builtin,
            enabled: true,
            body: String::new(),
        }
    }

    fn make_connector(
        id: &str,
        enabled: bool,
        auth: ConnectorAuthStatus,
        caps: Vec<ConnectorCapability>,
    ) -> ConnectorSpec {
        ConnectorSpec {
            id: id.to_string(),
            name: format!("Connector {id}"),
            description: format!("test connector {id}"),
            implementation_type: ConnectorImplementation::NativeRust,
            capabilities: caps,
            auth_status: auth,
            enabled,
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

    // ── test: all 9 policies pass → allowed ──

    #[tokio::test]
    async fn all_policies_pass_capability_allowed() {
        let _root = TestRoot::new();

        let connector = make_connector(
            "ok-conn",
            true,
            ConnectorAuthStatus::Authenticated,
            vec![make_cap("file.read", &["fs.read_file"], "low", false)],
        );
        ConnectorRegistry::register(connector).await.unwrap();

        let skills = vec![make_skill("s1", vec!["file.read"])];
        let grants = vec![("file.read".to_string(), true)];

        let results = PolicyEngine::filter_tools(&skills, &ConnectorRegistry, &grants)
            .await
            .unwrap();

        assert_eq!(results.len(), 1);
        let r = &results[0];
        assert_eq!(r.capability, "file.read");
        assert_eq!(r.allowed_tools, vec!["fs.read_file"]);
        assert_eq!(r.risk_level, "low");
        assert!(!r.requires_confirmation);
        assert!(r.reason.is_none());
    }

    // ── test: connector not found → denied (policy 4) ──

    #[tokio::test]
    async fn connector_not_found_denied() {
        let _root = TestRoot::new();

        let skills = vec![make_skill("s1", vec!["nonexistent.cap"])];
        let grants = vec![("nonexistent.cap".to_string(), true)];

        let results = PolicyEngine::filter_tools(&skills, &ConnectorRegistry, &grants)
            .await
            .unwrap();

        assert_eq!(results.len(), 1);
        assert!(results[0].reason.is_some());
        assert!(results[0].reason.as_ref().unwrap().contains("not found"));
        assert!(results[0].allowed_tools.is_empty());
    }

    // ── test: connector disabled → denied (policy 5) ──

    #[tokio::test]
    async fn connector_disabled_denied() {
        let _root = TestRoot::new();

        let connector = make_connector(
            "disabled-conn",
            false, // disabled
            ConnectorAuthStatus::Authenticated,
            vec![make_cap("web.search", &["web.search_api"], "medium", false)],
        );
        ConnectorRegistry::register(connector).await.unwrap();

        let skills = vec![make_skill("s1", vec!["web.search"])];
        let grants = vec![("web.search".to_string(), true)];

        let results = PolicyEngine::filter_tools(&skills, &ConnectorRegistry, &grants)
            .await
            .unwrap();

        assert_eq!(results.len(), 1);
        assert!(results[0].reason.is_some());
        assert!(results[0].reason.as_ref().unwrap().contains("disabled"));
    }

    // ── test: connector auth expired → soft exposed with auth hint ──

    #[tokio::test]
    async fn connector_auth_expired_soft_exposed() {
        let _root = TestRoot::new();

        let connector = make_connector(
            "expired-conn",
            true,
            ConnectorAuthStatus::Expired,
            vec![make_cap("email.send", &["smtp.send"], "high", true)],
        );
        ConnectorRegistry::register(connector).await.unwrap();

        let skills = vec![make_skill("s1", vec!["email.send"])];
        let grants = vec![("email.send".to_string(), true)];

        let results = PolicyEngine::filter_tools(&skills, &ConnectorRegistry, &grants)
            .await
            .unwrap();

        assert_eq!(results.len(), 1);
        assert_eq!(results[0].allowed_tools, vec!["smtp.send"]);
        assert!(results[0].reason.is_some());
        assert!(results[0].reason.as_ref().unwrap().contains("expired"));
    }

    // ── test: user not authorized → denied (policy 7) ──

    #[tokio::test]
    async fn user_not_authorized_denied() {
        let _root = TestRoot::new();

        let connector = make_connector(
            "auth-conn",
            true,
            ConnectorAuthStatus::Authenticated,
            vec![make_cap("calendar.read", &["cal.list"], "low", false)],
        );
        ConnectorRegistry::register(connector).await.unwrap();

        let skills = vec![make_skill("s1", vec!["calendar.read"])];
        let grants = vec![("calendar.read".to_string(), false)]; // not authorized

        let results = PolicyEngine::filter_tools(&skills, &ConnectorRegistry, &grants)
            .await
            .unwrap();

        assert_eq!(results.len(), 1);
        assert!(results[0].reason.is_some());
        assert!(results[0]
            .reason
            .as_ref()
            .unwrap()
            .contains("not authorized"));
    }

    // ── test: high risk without confirmation → denied (policy 8) ──

    #[tokio::test]
    async fn high_risk_without_confirmation_denied() {
        let _root = TestRoot::new();

        let connector = make_connector(
            "high-no-conf",
            true,
            ConnectorAuthStatus::Authenticated,
            vec![make_cap("admin.delete", &["admin.wipe"], "high", false)],
        );
        ConnectorRegistry::register(connector).await.unwrap();

        let skills = vec![make_skill("s1", vec!["admin.delete"])];
        let grants = vec![("admin.delete".to_string(), true)];

        let results = PolicyEngine::filter_tools(&skills, &ConnectorRegistry, &grants)
            .await
            .unwrap();

        assert_eq!(results.len(), 1);
        assert!(results[0].reason.is_some());
        assert!(results[0].reason.as_ref().unwrap().contains("confirmation"));
        assert!(results[0].requires_confirmation);
    }

    // ── test: high risk with confirmation → allowed (policies 8+9) ──

    #[tokio::test]
    async fn high_risk_with_confirmation_allowed() {
        let _root = TestRoot::new();

        let connector = make_connector(
            "high-conf",
            true,
            ConnectorAuthStatus::Authenticated,
            vec![make_cap("email.send", &["smtp.send"], "high", true)],
        );
        ConnectorRegistry::register(connector).await.unwrap();

        let skills = vec![make_skill("s1", vec!["email.send"])];
        let grants = vec![("email.send".to_string(), true)]; // authorized

        let results = PolicyEngine::filter_tools(&skills, &ConnectorRegistry, &grants)
            .await
            .unwrap();

        assert_eq!(results.len(), 1);
        let r = &results[0];
        assert!(r.reason.is_none(), "should be allowed, got: {:?}", r.reason);
        assert_eq!(r.allowed_tools, vec!["smtp.send"]);
        assert_eq!(r.risk_level, "high");
        assert!(r.requires_confirmation);
    }

    // ── test: multiple capabilities, mixed results ──

    #[tokio::test]
    async fn multiple_capabilities_mixed_results() {
        let _root = TestRoot::new();

        // Connector A: file.read — low risk, authenticated, enabled
        let c_a = make_connector(
            "mixed-a",
            true,
            ConnectorAuthStatus::Authenticated,
            vec![make_cap("file.read", &["fs.read"], "low", false)],
        );
        // Connector B: web.scrape — medium risk, but disabled
        let c_b = make_connector(
            "mixed-b",
            false, // disabled
            ConnectorAuthStatus::Authenticated,
            vec![make_cap("web.scrape", &["scraper.run"], "medium", false)],
        );
        // Connector C: email.send — high risk, requires confirmation, enabled
        let c_c = make_connector(
            "mixed-c",
            true,
            ConnectorAuthStatus::Authenticated,
            vec![make_cap("email.send", &["smtp.send"], "high", true)],
        );
        ConnectorRegistry::register(c_a).await.unwrap();
        ConnectorRegistry::register(c_b).await.unwrap();
        ConnectorRegistry::register(c_c).await.unwrap();

        let skills = vec![
            make_skill("s1", vec!["file.read", "web.scrape"]),
            make_skill("s2", vec!["email.send"]),
        ];
        let grants = vec![
            ("file.read".to_string(), true),
            ("web.scrape".to_string(), true),
            ("email.send".to_string(), true),
        ];

        let results = PolicyEngine::filter_tools(&skills, &ConnectorRegistry, &grants)
            .await
            .unwrap();

        // file.read: allowed
        let file_read = results
            .iter()
            .find(|r| r.capability == "file.read")
            .unwrap();
        assert!(file_read.reason.is_none());
        assert_eq!(file_read.allowed_tools, vec!["fs.read"]);

        // web.scrape: denied (connector disabled)
        let web_scrape = results
            .iter()
            .find(|r| r.capability == "web.scrape")
            .unwrap();
        assert!(web_scrape.reason.is_some());
        assert!(web_scrape.reason.as_ref().unwrap().contains("disabled"));

        // email.send: allowed (high risk + confirmation + authorized)
        let email = results
            .iter()
            .find(|r| r.capability == "email.send")
            .unwrap();
        assert!(email.reason.is_none());
        assert_eq!(email.allowed_tools, vec!["smtp.send"]);
    }

    // ── test: legacy fallback (empty skills → empty result) ──

    #[tokio::test]
    async fn empty_matched_skills_empty_result() {
        let _root = TestRoot::new();

        let skills: Vec<SkillPackage> = vec![];
        let grants: Vec<(String, bool)> = vec![];

        let results = PolicyEngine::filter_tools(&skills, &ConnectorRegistry, &grants)
            .await
            .unwrap();

        assert!(results.is_empty());
    }

    // ── test: legacy fallback — no connectors registered ──

    #[tokio::test]
    async fn legacy_fallback_no_connectors_all_denied() {
        let _root = TestRoot::new();
        // No connectors registered at all.

        let skills = vec![make_skill("s1", vec!["unknown.cap"])];
        let grants = vec![("unknown.cap".to_string(), true)];

        let results = PolicyEngine::filter_tools(&skills, &ConnectorRegistry, &grants)
            .await
            .unwrap();

        assert_eq!(results.len(), 1);
        assert!(results[0].reason.is_some());
        assert!(results[0].allowed_tools.is_empty());
    }

    // ── test: connector auth failed → denied (policy 6 variant) ──

    #[tokio::test]
    async fn connector_auth_failed_denied() {
        let _root = TestRoot::new();

        let connector = make_connector(
            "failed-auth",
            true,
            ConnectorAuthStatus::Failed,
            vec![make_cap("git.push", &["git.push_remote"], "medium", false)],
        );
        ConnectorRegistry::register(connector).await.unwrap();

        let skills = vec![make_skill("s1", vec!["git.push"])];
        let grants = vec![("git.push".to_string(), true)];

        let results = PolicyEngine::filter_tools(&skills, &ConnectorRegistry, &grants)
            .await
            .unwrap();

        assert_eq!(results.len(), 1);
        assert!(results[0].reason.is_some());
        assert!(results[0].reason.as_ref().unwrap().contains("failed"));
    }

    // ── test: connector auth not configured → denied (policy 6 variant) ──

    #[tokio::test]
    async fn connector_auth_not_configured_denied() {
        let _root = TestRoot::new();

        let connector = make_connector(
            "no-auth",
            true,
            ConnectorAuthStatus::NotConfigured,
            vec![make_cap("cloud.deploy", &["deploy.run"], "high", true)],
        );
        ConnectorRegistry::register(connector).await.unwrap();

        let skills = vec![make_skill("s1", vec!["cloud.deploy"])];
        let grants = vec![("cloud.deploy".to_string(), true)];

        let results = PolicyEngine::filter_tools(&skills, &ConnectorRegistry, &grants)
            .await
            .unwrap();

        assert_eq!(results.len(), 1);
        assert!(results[0].reason.is_some());
        assert!(results[0]
            .reason
            .as_ref()
            .unwrap()
            .contains("not_configured"));
    }

    // ── test: duplicate capabilities across skills are deduplicated ──

    #[tokio::test]
    async fn duplicate_capabilities_deduplicated() {
        let _root = TestRoot::new();

        let connector = make_connector(
            "dedup-conn",
            true,
            ConnectorAuthStatus::Authenticated,
            vec![make_cap("shared.cap", &["tool.x"], "low", false)],
        );
        ConnectorRegistry::register(connector).await.unwrap();

        // Two skills requesting the same capability.
        let skills = vec![
            make_skill("s1", vec!["shared.cap"]),
            make_skill("s2", vec!["shared.cap"]),
        ];
        let grants = vec![("shared.cap".to_string(), true)];

        let results = PolicyEngine::filter_tools(&skills, &ConnectorRegistry, &grants)
            .await
            .unwrap();

        // Should appear exactly once.
        assert_eq!(results.len(), 1);
        assert_eq!(results[0].capability, "shared.cap");
        assert!(results[0].reason.is_none());
    }

    #[tokio::test]
    async fn authorize_tool_ids_keeps_local_builtin_tools() {
        let _root = TestRoot::new();

        let allowed =
            PolicyEngine::authorize_tool_ids(&["tool.search".to_string(), "demo.echo".to_string()])
                .await
                .unwrap();

        assert_eq!(allowed, vec!["demo.echo", "tool.search"]);
    }

    #[tokio::test]
    async fn authorize_tool_ids_filters_unavailable_connector_tools() {
        let _root = TestRoot::new();

        ConnectorRegistry::register(ConnectorSpec {
            id: "catalog-disabled".to_string(),
            name: "Catalog Disabled".to_string(),
            description: "disabled connector".to_string(),
            implementation_type: ConnectorImplementation::NativeRust,
            capabilities: vec![make_cap("lark.doc", &["lark.doc.search"], "low", false)],
            auth_status: ConnectorAuthStatus::Authenticated,
            enabled: false,
            config_json: None,
        })
        .await
        .unwrap();

        let allowed = PolicyEngine::authorize_tool_ids(&[
            "lark.doc.search".to_string(),
            "tool.search".to_string(),
        ])
        .await
        .unwrap();

        assert_eq!(allowed, vec!["tool.search"]);
    }
}
