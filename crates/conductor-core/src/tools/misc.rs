use crate::proposals::RiskLevel;

use super::registry::{
    list_tools, ToolExecutionResult, ToolPermission, ToolProviderKind, ToolRegistry, ToolSpec,
};
use super::{current_workspace_root, shared_runtime};

fn truncate_utf8_by_bytes(content: &str, max_bytes: usize) -> String {
    if content.len() <= max_bytes {
        return content.to_string();
    }

    let mut end = max_bytes;
    while end > 0 && !content.is_char_boundary(end) {
        end -= 1;
    }
    content[..end].to_string()
}

fn execute_demo_echo(
    _spec: &ToolSpec,
    input: &serde_json::Value,
) -> Result<ToolExecutionResult, anyhow::Error> {
    let message = input.get("message").and_then(|v| v.as_str()).unwrap_or("");
    Ok(ToolExecutionResult {
        success: true,
        output: serde_json::json!({ "echo": message }),
        error: None,
        duration_ms: 0,
    })
}

fn execute_tool_search(
    _spec: &ToolSpec,
    input: &serde_json::Value,
) -> Result<ToolExecutionResult, anyhow::Error> {
    let raw_query = input
        .get("query")
        .or_else(|| input.get("q"))
        .and_then(|v| v.as_str())
        .unwrap_or("")
        .trim();
    let query = normalize_search_text(raw_query);
    let query_terms = search_terms(&query);
    let limit = input
        .get("limit")
        .and_then(|v| v.as_u64())
        .unwrap_or(10)
        .clamp(1, 50) as usize;

    let mut matches = list_tools()
        .into_iter()
        .filter_map(|tool| {
            let score = tool_search_score(&tool, &query, &query_terms);
            if score == 0 {
                return None;
            }
            Some((score, tool))
        })
        .map(|(score, tool)| {
            serde_json::json!({
                "id": tool.id,
                "name": tool.name,
                "description": tool.description,
                "provider": tool.provider.as_str(),
                "risk_level": tool.risk_level.as_str(),
                "supports_dry_run": tool.supports_dry_run,
                "workspace_required": tool.workspace_required,
                "score": score,
            })
        })
        .collect::<Vec<_>>();
    matches.sort_by(|a, b| {
        let score_cmp = b
            .get("score")
            .and_then(|v| v.as_i64())
            .unwrap_or(0)
            .cmp(&a.get("score").and_then(|v| v.as_i64()).unwrap_or(0));
        if score_cmp != std::cmp::Ordering::Equal {
            return score_cmp;
        }
        a.get("id")
            .and_then(|v| v.as_str())
            .unwrap_or("")
            .cmp(b.get("id").and_then(|v| v.as_str()).unwrap_or(""))
    });
    matches.truncate(limit);

    Ok(ToolExecutionResult {
        success: true,
        output: serde_json::json!({ "tools": matches }),
        error: None,
        duration_ms: 0,
    })
}

fn normalize_search_text(value: &str) -> String {
    value
        .to_lowercase()
        .chars()
        .map(|ch| {
            if ch.is_ascii_alphanumeric() || ('\u{4e00}'..='\u{9fff}').contains(&ch) {
                ch
            } else {
                ' '
            }
        })
        .collect::<String>()
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ")
}

fn search_terms(normalized_query: &str) -> Vec<&str> {
    normalized_query
        .split_whitespace()
        .filter(|term| term.chars().count() >= 2)
        .collect()
}

fn tool_search_aliases(tool_id: &str) -> &'static str {
    match tool_id {
        "file.glob" => {
            "file glob list files list directory list dir browse workspace ls tree find paths"
        }
        "file.grep" => "file grep search files search content find text ripgrep rg",
        "file.read" => "file read read file open file cat inspect file view file content",
        "file.write" => "file write write file create file save file output report",
        "file.append" => "file append append file chunked write long report add content",
        "file.edit" => "file edit edit file patch file modify file replace text",
        "file.stat" => "file stat file info file metadata exists size modified",
        "agent.start" => {
            "agent start start agent background agent subagent claude run delegate async status"
        }
        "agent.read_output" => {
            "agent read output read agent output agent logs agent status running result"
        }
        "agent.stop" => "agent stop stop agent cancel agent terminate",
        "subagent.claude_p" => {
            "subagent claude claude p claude_p delegate child agent background agent"
        }
        "tool.search" => "tool search find tools discover tools catalog capabilities",
        "workspace.current" => "workspace current workspace root cwd working directory",
        _ => "",
    }
}

fn tool_search_score(tool: &ToolSpec, query: &str, query_terms: &[&str]) -> i64 {
    if query.is_empty() {
        return 1;
    }

    let haystack = normalize_search_text(&format!(
        "{} {} {} {} {} {}",
        tool.id,
        tool.name,
        tool.description,
        tool.provider.as_str(),
        tool.risk_level.as_str(),
        tool_search_aliases(&tool.id)
    ));
    if haystack.is_empty() {
        return 0;
    }

    let compact_query = query.replace(' ', "");
    let compact_haystack = haystack.replace(' ', "");
    let mut score = 0;
    if haystack.contains(query) {
        score += 100;
    }
    if !compact_query.is_empty() && compact_haystack.contains(&compact_query) {
        score += 80;
    }

    let mut matched_terms = 0;
    for term in query_terms {
        if haystack.split_whitespace().any(|part| part == *term) || haystack.contains(term) {
            matched_terms += 1;
        }
    }
    if matched_terms == 0 {
        return score;
    }

    score += matched_terms as i64 * 10;
    if matched_terms == query_terms.len() {
        score += 40;
    }
    score
}

fn execute_todo_write(
    _spec: &ToolSpec,
    input: &serde_json::Value,
) -> Result<ToolExecutionResult, anyhow::Error> {
    let items = input["items"]
        .as_array()
        .ok_or_else(|| anyhow::anyhow!("missing required field: items (array)"))?;

    let chatsession_id = input["chatsession_id"].as_str().unwrap_or("default");

    let rt = shared_runtime();
    rt.block_on(async {
        // Clear existing items for this session
        crate::todo::clear_session(chatsession_id).await?;

        // Create new items
        let mut created = Vec::new();
        for item in items {
            let todo_item = crate::todo::create(chatsession_id, item.clone()).await?;
            created.push(todo_item.content);
        }

        Ok(ToolExecutionResult {
            success: true,
            output: serde_json::json!({
                "count": created.len(),
                "items": created,
            }),
            error: None,
            duration_ms: 0,
        })
    })
}

fn execute_events_recent(
    _spec: &ToolSpec,
    input: &serde_json::Value,
) -> Result<ToolExecutionResult, anyhow::Error> {
    let limit = input.get("limit").and_then(|v| v.as_u64()).unwrap_or(10) as usize;

    let runtime = shared_runtime();
    let events = runtime.block_on(crate::events::recent(limit))?;
    let event_list: Vec<serde_json::Value> = events
        .into_iter()
        .map(|e| {
            serde_json::json!({
                "ts": e.ts.to_rfc3339(),
                "source": e.source,
                "kind": e.kind,
                "payload": e.payload,
            })
        })
        .collect();

    Ok(ToolExecutionResult {
        success: true,
        output: serde_json::json!({ "events": event_list }),
        error: None,
        duration_ms: 0,
    })
}

fn execute_set_pet_avatar(
    _spec: &ToolSpec,
    input: &serde_json::Value,
) -> Result<ToolExecutionResult, anyhow::Error> {
    let runtime = shared_runtime();
    let config = runtime.block_on(crate::config::load())?;
    if config.pet.avatar_locked {
        return Ok(ToolExecutionResult {
            success: false,
            output: serde_json::json!({}),
            error: Some("形象已被用户设置锁定".to_string()),
            duration_ms: 0,
        });
    }

    let avatar_id_str = input
        .get("avatar_id")
        .and_then(|v| v.as_str())
        .ok_or_else(|| anyhow::anyhow!("missing avatar_id"))?;
    let mode = input
        .get("mode")
        .and_then(|v| v.as_str())
        .unwrap_or("temporary");
    let ttl_minutes = input.get("ttl_minutes").and_then(|v| v.as_u64());
    let reason = input.get("reason").and_then(|v| v.as_str());

    let avatar_id = crate::avatar::AvatarId::from_str(avatar_id_str)?;

    let avatar = runtime.block_on(crate::avatar::set_avatar(avatar_id))?;

    Ok(ToolExecutionResult {
        success: true,
        output: serde_json::json!({
            "id": avatar.id,
            "avatar_id": avatar.avatar_id.as_str(),
            "mode": mode,
            "ttl_minutes": ttl_minutes,
            "reason": reason,
        }),
        error: None,
        duration_ms: 0,
    })
}

fn execute_web_fetch(
    _spec: &ToolSpec,
    input: &serde_json::Value,
) -> Result<ToolExecutionResult, anyhow::Error> {
    let url = input["url"]
        .as_str()
        .ok_or_else(|| anyhow::anyhow!("missing required field: url"))?;

    let max_bytes = input["max_bytes"].as_u64().unwrap_or(102_400); // 100KB default

    let runtime = shared_runtime();
    let resp = runtime.block_on(async {
        reqwest::get(url)
            .await
            .map_err(|e| anyhow::anyhow!("fetch failed: {e}"))
    })?;

    let status = resp.status().as_u16();
    let body = runtime.block_on(async {
        resp.text()
            .await
            .map_err(|e| anyhow::anyhow!("read body failed: {e}"))
    })?;

    let truncated = if body.len() > max_bytes as usize {
        format!(
            "{}...(truncated, {} bytes total)",
            truncate_utf8_by_bytes(&body, max_bytes as usize),
            body.len()
        )
    } else {
        body
    };

    Ok(ToolExecutionResult {
        success: status >= 200 && status < 400,
        output: serde_json::json!({
            "status": status,
            "body": truncated,
        }),
        error: if status >= 400 {
            Some(format!("HTTP {status}"))
        } else {
            None
        },
        duration_ms: 0,
    })
}

fn execute_config_get(
    _spec: &ToolSpec,
    input: &serde_json::Value,
) -> Result<ToolExecutionResult, anyhow::Error> {
    let path = input["path"]
        .as_str()
        .ok_or_else(|| anyhow::anyhow!("missing required field: path"))?;

    let runtime = shared_runtime();
    let config = runtime.block_on(crate::config::load())?;

    // Navigate the config by dotted path
    let value = match path {
        "llm.provider" => serde_json::json!(config.llm.provider),
        "llm.model" => serde_json::json!(config.llm.model),
        "llm.base_url" => serde_json::json!(config.llm.base_url),
        "llm.temperature" => serde_json::json!(config.llm.temperature),
        "llm.api_key_set" => serde_json::json!(config.llm.api_key.is_some()),
        "persona.name" => serde_json::json!(config.persona.name),
        "persona.style" => serde_json::json!(config.persona.style),
        "pet.enabled" => serde_json::json!(config.pet.enabled),
        "pet.scale" => serde_json::json!(config.pet.scale),
        "pet.always_on_top" => serde_json::json!(config.pet.always_on_top),
        "focus_window_minutes" => serde_json::json!(config.focus_window_minutes),
        "chat_history_limit" => serde_json::json!(config.chat_history_limit),
        _ => serde_json::json!(null),
    };

    Ok(ToolExecutionResult {
        success: true,
        output: serde_json::json!({ "path": path, "value": value }),
        error: None,
        duration_ms: 0,
    })
}

fn execute_workspace_current(
    _spec: &ToolSpec,
    _input: &serde_json::Value,
) -> Result<ToolExecutionResult, anyhow::Error> {
    let root = current_workspace_root();
    let exists = root.exists();
    let writable = std::fs::metadata(&root)
        .map(|metadata| !metadata.permissions().readonly())
        .unwrap_or(false);

    Ok(ToolExecutionResult {
        success: true,
        output: serde_json::json!({
            "root": root.display().to_string(),
            "exists": exists,
            "writable": writable,
        }),
        error: None,
        duration_ms: 0,
    })
}

pub(super) fn register(registry: &mut ToolRegistry) {
    registry.register(
        ToolSpec {
            id: "demo.echo".to_string(), name: "Echo".to_string(),
            description: "返回输入的消息".to_string(), provider: ToolProviderKind::Internal,
            input_schema: serde_json::json!({"type":"object","properties":{"message":{"type":"string"}},"required":["message"]}),
            output_schema: serde_json::json!({"type":"object","properties":{"echo":{"type":"string"}}}),
            risk_level: RiskLevel::ReadOnly, permissions: vec![],
            supports_dry_run: true, workspace_required: false,
        },
        execute_demo_echo,
    );

    registry.register(
        ToolSpec {
            id: "todo.write".to_string(),
            name: "写入待办".to_string(),
            description: "更新当前会话的待办事项列表。传入 items 数组替换整个列表。".to_string(),
            provider: ToolProviderKind::Internal,
            input_schema: serde_json::json!({
                "type": "object",
                "properties": {
                    "items": {
                        "type": "array",
                        "items": {
                            "type": "object",
                            "properties": {
                                "content": { "type": "string" },
                                "priority": { "type": "string", "enum": ["high", "medium", "low"] },
                                "status": { "type": "string", "enum": ["pending", "done"] }
                            },
                            "required": ["content"]
                        },
                        "description": "待办事项列表（替换现有列表）"
                    }
                },
                "required": ["items"]
            }),
            output_schema: serde_json::json!({
                "type": "object",
                "properties": {
                    "count": { "type": "integer" },
                    "items": { "type": "array" }
                }
            }),
            risk_level: RiskLevel::ReadOnly,
            permissions: vec![],
            supports_dry_run: false,
            workspace_required: false,
        },
        execute_todo_write,
    );

    registry.register(
        ToolSpec {
            id: "tool.search".to_string(),
            name: "搜索工具".to_string(),
            description: "按关键词搜索已注册工具的精简信息".to_string(),
            provider: ToolProviderKind::Internal,
            input_schema: serde_json::json!({
                "type": "object",
                "properties": {
                    "query": { "type": "string" },
                    "q": { "type": "string" },
                    "limit": { "type": "integer", "minimum": 1, "maximum": 50 }
                }
            }),
            output_schema: serde_json::json!({
                "type": "object",
                "properties": {
                    "tools": {
                        "type": "array",
                        "items": {
                            "type": "object",
                            "properties": {
                                "id": { "type": "string" },
                                "name": { "type": "string" },
                                "description": { "type": "string" },
                                "provider": { "type": "string" },
                                "risk_level": { "type": "string" },
                                "supports_dry_run": { "type": "boolean" },
                                "workspace_required": { "type": "boolean" },
                                "score": { "type": "integer" }
                            }
                        }
                    }
                }
            }),
            risk_level: RiskLevel::ReadOnly,
            permissions: vec![],
            supports_dry_run: true,
            workspace_required: false,
        },
        execute_tool_search,
    );

    registry.register(
        ToolSpec {
            id: "events.recent".to_string(),
            name: "最近事件".to_string(),
            description: "获取最近的事件列表".to_string(),
            provider: ToolProviderKind::Internal,
            input_schema: serde_json::json!({
                "type": "object",
                "properties": {
                    "limit": { "type": "integer", "minimum": 1, "maximum": 100 }
                }
            }),
            output_schema: serde_json::json!({
                "type": "object",
                "properties": {
                    "events": {
                        "type": "array",
                        "items": {
                            "type": "object",
                            "properties": {
                                "ts": { "type": "string" },
                                "source": { "type": "string" },
                                "kind": { "type": "string" },
                                "payload": { "type": "object" }
                            }
                        }
                    }
                }
            }),
            risk_level: RiskLevel::ReadOnly,
            permissions: vec![ToolPermission::ReadWorkspace],
            supports_dry_run: true,
            workspace_required: false,
        },
        execute_events_recent,
    );

    registry.register(
        ToolSpec {
            id: "pet.set_avatar".to_string(),
            name: "设置桌宠形象".to_string(),
            description: "设置桌宠三枚举形象".to_string(),
            provider: ToolProviderKind::Internal,
            input_schema: serde_json::json!({
                "type": "object",
                "properties": {
                    "avatar_id": {
                        "type": "string",
                        "enum": ["original", "document_secretary", "programmer"]
                    },
                    "mode": {
                        "type": "string",
                        "enum": ["temporary", "persistent"]
                    },
                    "ttl_minutes": { "type": "integer", "minimum": 1, "maximum": 1440 },
                    "reason": { "type": "string" }
                },
                "required": ["avatar_id"]
            }),
            output_schema: serde_json::json!({
                "type": "object",
                "properties": {
                    "id": { "type": "string" },
                    "avatar_id": { "type": "string" },
                    "mode": { "type": "string" },
                    "ttl_minutes": { "type": ["integer", "null"] },
                    "reason": { "type": ["string", "null"] }
                }
            }),
            risk_level: RiskLevel::ReadOnly,
            permissions: vec![],
            supports_dry_run: true,
            workspace_required: false,
        },
        execute_set_pet_avatar,
    );

    registry.register(
        ToolSpec {
            id: "conductor.pet.set_avatar".to_string(),
            name: "设置桌宠形象".to_string(),
            description: "MCP 兼容入口，转发到 pet.set_avatar".to_string(),
            provider: ToolProviderKind::Mcp,
            input_schema: serde_json::json!({
                "type": "object",
                "properties": {
                    "avatar_id": {
                        "type": "string",
                        "enum": ["original", "document_secretary", "programmer"]
                    },
                    "mode": {
                        "type": "string",
                        "enum": ["temporary", "persistent"]
                    },
                    "ttl_minutes": { "type": "integer", "minimum": 1, "maximum": 1440 },
                    "reason": { "type": "string" }
                },
                "required": ["avatar_id"]
            }),
            output_schema: serde_json::json!({
                "type": "object",
                "properties": {
                    "id": { "type": "string" },
                    "avatar_id": { "type": "string" },
                    "mode": { "type": "string" },
                    "ttl_minutes": { "type": ["integer", "null"] },
                    "reason": { "type": ["string", "null"] }
                }
            }),
            risk_level: RiskLevel::ReadOnly,
            permissions: vec![],
            supports_dry_run: true,
            workspace_required: false,
        },
        execute_set_pet_avatar,
    );

    registry.register(
        ToolSpec {
            id: "web.fetch".to_string(),
            name: "获取网页".to_string(),
            description: "获取指定 URL 的网页内容，返回状态码和正文（默认截断至 100KB）。"
                .to_string(),
            provider: ToolProviderKind::Internal,
            input_schema: serde_json::json!({
                "type": "object",
                "properties": {
                    "url": { "type": "string", "description": "要获取的 URL" },
                    "max_bytes": { "type": "integer", "description": "最大返回字节数 (默认 102400)" }
                },
                "required": ["url"]
            }),
            output_schema: serde_json::json!({
                "type": "object",
                "properties": {
                    "status": { "type": "integer" },
                    "body": { "type": "string" }
                }
            }),
            risk_level: RiskLevel::ReadOnly,
            permissions: vec![ToolPermission::Network],
            supports_dry_run: false,
            workspace_required: false,
        },
        execute_web_fetch,
    );

    registry.register(
        ToolSpec {
            id: "config.get".to_string(),
            name: "读取配置".to_string(),
            description: "读取当前配置项的值，用点号路径指定（如 llm.model、persona.name）。"
                .to_string(),
            provider: ToolProviderKind::Internal,
            input_schema: serde_json::json!({
                "type": "object",
                "properties": {
                    "path": { "type": "string", "description": "配置路径，如 llm.model、pet.enabled" }
                },
                "required": ["path"]
            }),
            output_schema: serde_json::json!({
                "type": "object",
                "properties": {
                    "path": { "type": "string" },
                    "value": {}
                }
            }),
            risk_level: RiskLevel::ReadOnly,
            permissions: vec![ToolPermission::ReadWorkspace],
            supports_dry_run: false,
            workspace_required: false,
        },
        execute_config_get,
    );

    registry.register(
        ToolSpec {
            id: "workspace.current".to_string(), name: "当前工作区".to_string(),
            description: "返回当前工作区根目录和基础状态，不执行 Shell 命令。".to_string(),
            provider: ToolProviderKind::Internal,
            input_schema: serde_json::json!({"type":"object","properties":{}}),
            output_schema: serde_json::json!({"type":"object","properties":{"root":{"type":"string"},"exists":{"type":"boolean"},"writable":{"type":"boolean"}}}),
            risk_level: RiskLevel::ReadOnly, permissions: vec![ToolPermission::ReadWorkspace],
            supports_dry_run: true, workspace_required: false,
        },
        execute_workspace_current,
    );
}
