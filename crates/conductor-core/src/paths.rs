use std::path::PathBuf;

pub fn root() -> PathBuf {
    std::env::var_os("CONDUCTOR_ROOT")
        .map(PathBuf::from)
        .unwrap_or_else(default_root)
}

pub fn state() -> PathBuf {
    root().join("state")
}

#[cfg(debug_assertions)]
fn default_root() -> PathBuf {
    workspace_root_from_manifest().unwrap_or_else(current_dir_or_temp)
}

#[cfg(debug_assertions)]
fn workspace_root_from_manifest() -> Option<PathBuf> {
    let crate_root = std::path::Path::new(env!("CARGO_MANIFEST_DIR"));
    crate_root
        .parent()
        .and_then(|parent| parent.parent())
        .map(|path| path.to_path_buf())
}

#[cfg(all(not(debug_assertions), windows))]
fn default_root() -> PathBuf {
    std::env::var_os("LOCALAPPDATA")
        .map(PathBuf::from)
        .filter(|path| !path.as_os_str().is_empty())
        .unwrap_or_else(current_dir_or_temp)
        .join("PersonalConductor")
}

#[cfg(all(not(debug_assertions), not(windows)))]
fn default_root() -> PathBuf {
    if let Some(data_home) = std::env::var_os("XDG_DATA_HOME")
        .map(PathBuf::from)
        .filter(|path| !path.as_os_str().is_empty())
    {
        return data_home.join("personal-conductor");
    }

    if let Some(home) = std::env::var_os("HOME")
        .map(PathBuf::from)
        .filter(|path| !path.as_os_str().is_empty())
    {
        return home.join(".local").join("share").join("personal-conductor");
    }

    current_dir_or_temp().join("personal-conductor")
}

fn current_dir_or_temp() -> PathBuf {
    std::env::current_dir().unwrap_or_else(|_| std::env::temp_dir())
}

pub struct Paths;

impl Paths {
    pub fn tasks_json() -> PathBuf {
        state().join("tasks.json")
    }

    pub fn tasks_md() -> PathBuf {
        state().join("tasks.md")
    }

    pub fn conductor_sqlite() -> PathBuf {
        state().join("conductor.sqlite")
    }

    pub fn config_json() -> PathBuf {
        state().join("config.json")
    }

    pub fn events() -> PathBuf {
        state().join("events.ndjson")
    }

    pub fn summaries_dir() -> PathBuf {
        state().join("summaries")
    }

    pub fn on_stop_log() -> PathBuf {
        state().join("on-stop.log")
    }

    pub fn inject_log() -> PathBuf {
        state().join("inject.log")
    }

    pub fn proposals_json() -> PathBuf {
        state().join("proposals.json")
    }

    pub fn initiative_state_json() -> PathBuf {
        state().join("initiative_state.json")
    }

    pub fn persona_state_json() -> PathBuf {
        state().join("persona_state.json")
    }

    pub fn scene_state_json() -> PathBuf {
        state().join("scene_state.json")
    }

    pub fn skills_json() -> PathBuf {
        state().join("skills.json")
    }

    pub fn proposed_prompts_dir() -> PathBuf {
        state().join("proposed_prompts")
    }

    pub fn subagent_runs_dir() -> PathBuf {
        state().join("subagent-runs")
    }

    pub fn agent_runs_dir() -> PathBuf {
        state().join("agent-runs")
    }

    pub fn task_signal() -> PathBuf {
        state().join(".task_signal")
    }

    /// Signal file written by the Runtime API to request desktop execution of a goal task.
    pub fn task_execution_signal(task_id: &str) -> PathBuf {
        state().join("exec-signals").join(format!("{task_id}.exec"))
    }

    pub fn runtime_api_state_json() -> PathBuf {
        state().join("runtime-api.json")
    }

    pub fn runtime_token_txt() -> PathBuf {
        state().join("runtime_token.txt")
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::test_support::TestRoot;

    #[test]
    fn paths_point_under_state() {
        let root = TestRoot::new();
        assert_eq!(super::root(), root.path());
        assert_eq!(state(), root.path().join("state"));
        assert!(Paths::tasks_json().ends_with("state/tasks.json"));
        assert!(Paths::tasks_md().ends_with("state/tasks.md"));
        assert!(Paths::conductor_sqlite().ends_with("state/conductor.sqlite"));
        assert!(Paths::config_json().ends_with("state/config.json"));
        assert!(Paths::events().ends_with("state/events.ndjson"));
        assert!(Paths::summaries_dir().ends_with("state/summaries"));
        assert!(Paths::on_stop_log().ends_with("state/on-stop.log"));
        assert!(Paths::inject_log().ends_with("state/inject.log"));
        assert!(Paths::proposals_json().ends_with("state/proposals.json"));
        assert!(Paths::initiative_state_json().ends_with("state/initiative_state.json"));
        assert!(Paths::persona_state_json().ends_with("state/persona_state.json"));
        assert!(Paths::scene_state_json().ends_with("state/scene_state.json"));
        assert!(Paths::proposed_prompts_dir().ends_with("state/proposed_prompts"));
        assert!(Paths::subagent_runs_dir().ends_with("state/subagent-runs"));
        assert!(Paths::agent_runs_dir().ends_with("state/agent-runs"));
        assert!(Paths::task_signal().ends_with("state/.task_signal"));
        assert!(Paths::runtime_api_state_json().ends_with("state/runtime-api.json"));
        assert!(Paths::runtime_token_txt().ends_with("state/runtime_token.txt"));
    }
}
