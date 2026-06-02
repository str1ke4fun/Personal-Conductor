use anyhow::bail;
use serde::{Deserialize, Serialize};

/// Handoff contract defines what a member must produce before handing off to the next member.
#[derive(Serialize, Deserialize, Clone, Debug, Default)]
pub struct HandoffContract {
    /// Required output artifact paths (e.g., ["src/foo.rs", "tests/foo_test.rs"]).
    pub required_artifacts: Vec<String>,
    /// Validation command to run before accepting handoff (e.g., "cargo test -p foo").
    pub validation_command: Option<String>,
    /// Maximum time allowed for this member's work in seconds.
    pub timeout_secs: Option<u64>,
}

/// Extended member configuration for AgentTeam lifecycle.
///
/// These fields control per-member tool access, handoff behavior, and conflict resolution.
#[derive(Serialize, Deserialize, Clone, Debug, Default)]
pub struct AgentTeamMemberConfig {
    /// Allowed tool IDs for this member (empty = all tools allowed).
    pub allowed_tools: Vec<String>,
    /// Handoff contract: what this member must produce before the next member can start.
    pub handoff_contract: Option<HandoffContract>,
    /// Conflict lock policy for this member's write operations.
    pub conflict_lock_policy: ConflictLockPolicy,
}

/// Conflict lock policy for team members controlling write access.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum ConflictLockPolicy {
    /// No locking — member can write freely.
    None,
    /// Advisory lock — warns on overlap but does not block.
    Advisory,
    /// Exclusive lock — blocks write if another team holds overlapping scope.
    Exclusive,
}

impl Default for ConflictLockPolicy {
    fn default() -> Self {
        Self::Advisory
    }
}

impl ConflictLockPolicy {
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::None => "none",
            Self::Advisory => "advisory",
            Self::Exclusive => "exclusive",
        }
    }

    pub fn from_str(value: &str) -> anyhow::Result<Self> {
        match value {
            "none" => Ok(Self::None),
            "advisory" => Ok(Self::Advisory),
            "exclusive" => Ok(Self::Exclusive),
            other => bail!("unknown conflict lock policy: {other}"),
        }
    }
}

/// Serialize AgentTeamMemberConfig to JSON string for DB storage.
pub fn serialize_member_config(config: &AgentTeamMemberConfig) -> anyhow::Result<String> {
    Ok(serde_json::to_string(config)?)
}

/// Deserialize AgentTeamMemberConfig from JSON string.
pub fn deserialize_member_config(json: &str) -> anyhow::Result<AgentTeamMemberConfig> {
    Ok(serde_json::from_str(json)?)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn member_config_serialization_roundtrip() {
        let config = AgentTeamMemberConfig {
            allowed_tools: vec!["file.read".to_string(), "bash.execute".to_string()],
            handoff_contract: Some(HandoffContract {
                required_artifacts: vec!["src/foo.rs".to_string()],
                validation_command: Some("cargo test -p foo".to_string()),
                timeout_secs: Some(3600),
            }),
            conflict_lock_policy: ConflictLockPolicy::Exclusive,
        };
        let json = serialize_member_config(&config).expect("serialize");
        let restored = deserialize_member_config(&json).expect("deserialize");
        assert_eq!(restored.allowed_tools, config.allowed_tools);
        assert_eq!(
            restored
                .handoff_contract
                .as_ref()
                .unwrap()
                .required_artifacts,
            vec!["src/foo.rs".to_string()]
        );
        assert_eq!(restored.conflict_lock_policy, ConflictLockPolicy::Exclusive);
    }

    #[test]
    fn member_config_default_is_empty() {
        let config = AgentTeamMemberConfig::default();
        assert!(config.allowed_tools.is_empty());
        assert!(config.handoff_contract.is_none());
        assert_eq!(config.conflict_lock_policy, ConflictLockPolicy::Advisory);
    }

    #[test]
    fn conflict_lock_policy_roundtrip() {
        for policy in [
            ConflictLockPolicy::None,
            ConflictLockPolicy::Advisory,
            ConflictLockPolicy::Exclusive,
        ] {
            let s = policy.as_str();
            let restored = ConflictLockPolicy::from_str(s).expect("from_str");
            assert_eq!(restored, policy);
        }
    }
}
