pub mod adapters;
pub mod affection;
pub mod agent_backends;
pub mod agent_messages;
pub mod agent_runs;
pub mod agent_team_members;
pub mod agent_teams;
pub mod app_state;
pub mod avatar;
pub mod chat;
pub mod chat_parser;
pub mod codex;
pub mod command_runs;
pub mod config;
pub mod connectors;
pub mod db;
pub mod embedding;
pub mod events;
pub mod expression;
pub mod feishu;
pub mod filewatch;
pub mod goal_hints;
pub mod goal_orchestrator;
pub mod goal_tasks;
pub mod goals;
pub mod heartbeat;
pub mod initiative;
pub mod inject;
pub mod leases;
pub mod llm;
pub mod llm_profiles;
pub mod lock;
pub mod mcp;
pub mod memory;
pub mod model_resolver;
pub mod music;
pub mod pacer;
pub mod paths;
pub mod permissions;
pub mod persona;
pub mod policy;
pub mod projection;
pub mod proposals;
pub mod recovery;
pub mod routing;
pub mod runtime_api;
pub mod scene;
pub mod shell;
pub mod skills;
pub mod smart_monitor;
pub mod subagent;
pub mod summarizer;
pub mod tasklist;
pub mod tasks;
pub mod todo;
pub mod tool_calls;
pub mod tools;
pub mod transcript;
pub mod user_presence;
pub mod workspaces;

pub fn hello() -> &'static str {
    "conductor-core"
}

#[cfg(any(test, feature = "test-utils"))]
pub mod test_support {
    use std::sync::{Mutex, MutexGuard, OnceLock};
    use tempfile::TempDir;

    static ENV_LOCK: OnceLock<Mutex<()>> = OnceLock::new();

    pub struct TestRoot {
        _guard: MutexGuard<'static, ()>,
        temp: TempDir,
    }

    impl TestRoot {
        pub fn new() -> Self {
            let mutex = ENV_LOCK.get_or_init(|| Mutex::new(()));
            let guard = mutex
                .lock()
                .unwrap_or_else(|poisoned| poisoned.into_inner());
            let temp = tempfile::tempdir().expect("create temp conductor root");
            std::env::set_var("CONDUCTOR_ROOT", temp.path());
            Self {
                _guard: guard,
                temp,
            }
        }

        pub fn path(&self) -> &std::path::Path {
            self.temp.path()
        }
    }

    impl Drop for TestRoot {
        fn drop(&mut self) {
            std::env::remove_var("CONDUCTOR_ROOT");
        }
    }
}
