use chrono::{DateTime, Utc};
use std::collections::HashMap;
use std::sync::{Mutex, OnceLock};

#[derive(Debug, Clone)]
pub struct ActiveChatRun {
    pub session_id: String,
    pub request_id: String,
    pub started_at: DateTime<Utc>,
    pub phase: Option<String>,
    pub tool_run_count: u32,
    pub active_tool_count: u32,
}

type SessionRuns = HashMap<String, ActiveChatRun>;
type ActiveRunMap = HashMap<String, SessionRuns>;

static ACTIVE_RUNS: OnceLock<Mutex<ActiveRunMap>> = OnceLock::new();

fn runs() -> &'static Mutex<ActiveRunMap> {
    ACTIVE_RUNS.get_or_init(|| Mutex::new(HashMap::new()))
}

pub fn register_active_run(session_id: &str, request_id: &str) {
    let run = ActiveChatRun {
        session_id: session_id.to_string(),
        request_id: request_id.to_string(),
        started_at: Utc::now(),
        phase: None,
        tool_run_count: 0,
        active_tool_count: 0,
    };
    if let Ok(mut map) = runs().lock() {
        map.entry(session_id.to_string())
            .or_default()
            .insert(request_id.to_string(), run);
    }
}

pub fn update_active_phase(session_id: &str, request_id: &str, phase: Option<String>) {
    if let Ok(mut map) = runs().lock() {
        if let Some(run) = map
            .get_mut(session_id)
            .and_then(|session_runs| session_runs.get_mut(request_id))
        {
            run.phase = phase;
        }
    }
}

pub fn update_active_tool_count(
    session_id: &str,
    request_id: &str,
    run_count: u32,
    active_count: u32,
) {
    if let Ok(mut map) = runs().lock() {
        if let Some(run) = map
            .get_mut(session_id)
            .and_then(|session_runs| session_runs.get_mut(request_id))
        {
            run.tool_run_count = run_count;
            run.active_tool_count = active_count;
        }
    }
}

pub fn remove_active_run(session_id: &str, request_id: &str) {
    if let Ok(mut map) = runs().lock() {
        if let Some(session_runs) = map.get_mut(session_id) {
            session_runs.remove(request_id);
            if session_runs.is_empty() {
                map.remove(session_id);
            }
        }
    }
}

pub fn get_active_run(session_id: &str, request_id: &str) -> Option<ActiveChatRun> {
    if let Ok(map) = runs().lock() {
        map.get(session_id)
            .and_then(|session_runs| session_runs.get(request_id))
            .cloned()
    } else {
        None
    }
}

pub fn list_active_runs() -> Vec<ActiveChatRun> {
    if let Ok(map) = runs().lock() {
        map.values()
            .flat_map(|session_runs| session_runs.values().cloned())
            .collect()
    } else {
        Vec::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_register_and_get() {
        register_active_run("t_reg_s1", "t_reg_r1");
        let run = get_active_run("t_reg_s1", "t_reg_r1").unwrap();
        assert_eq!(run.session_id, "t_reg_s1");
        assert_eq!(run.request_id, "t_reg_r1");
        assert!(run.phase.is_none());
        assert_eq!(run.tool_run_count, 0);
    }

    #[test]
    fn test_get_unknown_returns_none() {
        assert!(get_active_run("t_unknown_nonexistent", "req").is_none());
    }

    #[test]
    fn test_update_phase() {
        register_active_run("t_phase_s2", "t_phase_r2");
        update_active_phase("t_phase_s2", "t_phase_r2", Some("tool_calling".to_string()));
        let run = get_active_run("t_phase_s2", "t_phase_r2").unwrap();
        assert_eq!(run.phase.as_deref(), Some("tool_calling"));
    }

    #[test]
    fn test_update_tool_count() {
        register_active_run("t_tool_s3", "t_tool_r3");
        update_active_tool_count("t_tool_s3", "t_tool_r3", 5, 2);
        let run = get_active_run("t_tool_s3", "t_tool_r3").unwrap();
        assert_eq!(run.tool_run_count, 5);
        assert_eq!(run.active_tool_count, 2);
    }

    #[test]
    fn test_remove() {
        register_active_run("t_rm_s4", "t_rm_r4");
        assert!(get_active_run("t_rm_s4", "t_rm_r4").is_some());
        remove_active_run("t_rm_s4", "t_rm_r4");
        assert!(get_active_run("t_rm_s4", "t_rm_r4").is_none());
    }

    #[test]
    fn test_register_keeps_multiple_runs_per_session() {
        register_active_run("t_multi_s5", "t_multi_r1");
        register_active_run("t_multi_s5", "t_multi_r2");

        let runs = list_active_runs();
        assert!(runs.iter().any(|run| run.request_id == "t_multi_r1"));
        assert!(runs.iter().any(|run| run.request_id == "t_multi_r2"));
    }

    #[test]
    fn test_list_active_runs() {
        register_active_run("t_list_s6", "t_list_r1");
        register_active_run("t_list_s7", "t_list_r2");
        let runs = list_active_runs();
        assert!(
            runs.len() >= 2,
            "expected at least 2 runs, got {}",
            runs.len()
        );
        assert!(runs.iter().any(|r| r.session_id == "t_list_s6"));
        assert!(runs.iter().any(|r| r.session_id == "t_list_s7"));
    }

    #[test]
    fn test_update_phase_noop_on_missing() {
        update_active_phase("t_noop_missing", "req", Some("test".to_string()));
        assert!(get_active_run("t_noop_missing", "req").is_none());
    }
}
